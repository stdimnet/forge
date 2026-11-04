//! Sandbox operations as calls of the backend protocol ([`SandboxCall`]), answered where the
//! project's files are: by the desktop app for a local project and by the Forge agent
//! (`crates/forge-remote-agent`) on a remote project's host. The sandbox is created under
//! that machine's cache folder (`<cache>/forge/agent-tasks/<project key>/<task id>`), the
//! built-in agent's file tools run there, and the diff, Apply and Discard work on it there.
//!
//! Every call names a sandbox folder; it must be one of the open project's sandboxes, so a
//! peer cannot reach other folders through these calls.

use std::path::{Component, Path, PathBuf};

use forge_ai_editor::policy::IGNORE_FILES;
use forge_ai_editor::AiIgnore;
use serde::{Deserialize, Serialize};

use crate::sandbox::{self, ApplyResult, FileChange, Sandbox};
use crate::tools::{execute_file_tool, ToolCall};
use crate::workspace::Workspace;

/// Environment variable overriding the base folder of all sandboxes (tests).
pub const BASE_ENV: &str = "FORGE_AGENT_TASKS_DIR";

/// A sandbox as the calls name it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxRef {
    /// The worktree.
    pub dir: String,
    /// The commit the diff is taken against.
    pub base: String,
}

/// A created sandbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxInfo {
    pub dir: String,
    /// The project folder inside the worktree.
    pub project_dir: String,
    pub base: String,
}

/// How to apply a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum ApplyMode {
    /// Patch the working tree.
    Apply,
    /// Patch the working tree and commit the changed files.
    Commit { message: String },
    /// Commit in the sandbox and create a branch; the working tree is not touched.
    Branch { branch: String, message: String },
}

/// What applying did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyAnswer {
    #[serde(flatten)]
    pub result: ApplyResult,
    /// The commit or branch created.
    pub created: Option<String>,
}

/// Both sides of a changed file for the diff viewer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileVersions {
    pub before: Option<String>,
    pub after: Option<String>,
}

/// The result of a file tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolReply {
    pub ok: bool,
    pub output: String,
}

impl From<Result<String, String>> for ToolReply {
    fn from(r: Result<String, String>) -> Self {
        match r {
            Ok(output) => Self { ok: true, output },
            Err(output) => Self { ok: false, output },
        }
    }
}

impl From<ToolReply> for Result<String, String> {
    fn from(r: ToolReply) -> Self {
        if r.ok {
            Ok(r.output)
        } else {
            Err(r.output)
        }
    }
}

/// A sandbox operation. The reply type of each call is noted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SandboxCall {
    /// Create the sandbox of task `id` from a snapshot of the working tree -> [`SandboxInfo`]
    Create { id: String },
    /// The files the agent changed -> `Vec<FileChange>`
    Changes { sandbox: SandboxRef },
    /// Base and new text of a changed file -> [`FileVersions`]
    FileVersions { sandbox: SandboxRef, path: String },
    /// Apply the changes (all, or `paths`) -> [`ApplyAnswer`]
    Apply {
        sandbox: SandboxRef,
        mode: ApplyMode,
        paths: Vec<String>,
    },
    /// Remove the sandbox -> `()`
    Remove { dir: String },
    /// A file tool of the built-in agent in the sandbox's project folder, with the project's
    /// `.aiignore` and the task's scope -> [`ToolReply`]
    Tool {
        sandbox: SandboxRef,
        scope: Vec<String>,
        call: ToolCall,
    },
}

/// `<cache>/forge/agent-tasks` on this machine (or [`BASE_ENV`]).
pub fn default_base() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(BASE_ENV).filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    Some(dirs::cache_dir()?.join("forge").join("agent-tasks"))
}

/// The folder of `project`'s tasks and sandboxes below `base`.
pub fn project_dir(base: &Path, project: &Path) -> PathBuf {
    base.join(forge_core::project_state::project_key(
        &project.to_string_lossy(),
    ))
}

fn valid_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err(format!("invalid task id {id:?}"));
    }
    Ok(())
}

/// Create the sandbox of task `id` of `project` below `base`.
pub fn create(project: &Path, base: &Path, id: &str) -> Result<SandboxInfo, String> {
    valid_id(id)?;
    let dir = project_dir(base, project).join(id);
    if dir.exists() {
        return Err(format!("the sandbox {} exists already", dir.display()));
    }
    let sb = sandbox::create(project, &dir)?;
    Ok(SandboxInfo {
        dir: sb.dir.to_string_lossy().into_owned(),
        project_dir: sb.project_dir.to_string_lossy().into_owned(),
        base: sb.base,
    })
}

/// `dir` when it is a sandbox of `project` (directly below its tasks folder).
fn sandbox_dir(project: &Path, base: &Path, dir: &str) -> Result<PathBuf, String> {
    let dir = PathBuf::from(dir);
    let parent = project_dir(base, project);
    let ok = dir.parent() == Some(parent.as_path())
        && matches!(dir.file_name().and_then(|n| n.to_str()), Some(n) if valid_id(n).is_ok())
        && !dir.components().any(|c| matches!(c, Component::ParentDir));
    if !ok {
        return Err(format!(
            "{} is not a sandbox of this project",
            dir.display()
        ));
    }
    Ok(dir)
}

/// The sandbox `r` of `project`, checked.
pub fn open(project: &Path, base: &Path, r: &SandboxRef) -> Result<Sandbox, String> {
    let dir = sandbox_dir(project, base, &r.dir)?;
    if !dir.is_dir() {
        return Err("The sandbox of this task is gone".into());
    }
    if r.base.is_empty() || !r.base.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("invalid base commit {:?}", r.base));
    }
    let prefix = sandbox::repository_of(project)
        .map(|(_, p)| p)
        .unwrap_or_default();
    Ok(Sandbox {
        project_dir: if prefix.is_empty() {
            dir.clone()
        } else {
            dir.join(prefix)
        },
        dir,
        base: r.base.clone(),
    })
}

/// The agent's view of a sandbox: its project folder, the project's `.aiignore` rules
/// (as in the sandbox) and the task's scope.
pub fn workspace(sb: &Sandbox, scope: Vec<String>) -> Workspace {
    let texts: Vec<String> = IGNORE_FILES
        .iter()
        .filter_map(|f| std::fs::read_to_string(sb.project_dir.join(f)).ok())
        .collect();
    let ignore = AiIgnore::merged(texts.iter().map(String::as_str));
    Workspace::new(&sb.project_dir, ignore, scope)
}

/// Apply the changes of `sb` to `project`'s working tree (or commit them on a branch).
pub fn apply(
    project: &Path,
    sb: &Sandbox,
    mode: ApplyMode,
    paths: &[String],
) -> Result<ApplyAnswer, String> {
    let (result, created) = match mode {
        ApplyMode::Apply => (sandbox::apply(project, sb, paths)?, None),
        ApplyMode::Commit { message } => sandbox::apply_as_commit(project, sb, paths, &message)?,
        ApplyMode::Branch { branch, message } => {
            let files = sandbox::changes(sb)?.into_iter().map(|f| f.path).collect();
            let hash = sandbox::apply_as_branch(sb, &branch, &message)?;
            (
                ApplyResult {
                    applied: true,
                    files,
                    conflicts: Vec::new(),
                    message: None,
                },
                Some(format!("{} ({hash})", branch.trim())),
            )
        }
    };
    Ok(ApplyAnswer { result, created })
}

/// Remove the sandbox `dir` of `project` (nothing to do when it is gone already).
pub fn remove(project: &Path, base: &Path, dir: &str) -> Result<(), String> {
    let dir = sandbox_dir(project, base, dir)?;
    sandbox::remove(project, &dir)
}

/// The typed answers of [`SandboxCall`]s.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Info(SandboxInfo),
    Changes(Vec<FileChange>),
    Versions(FileVersions),
    Applied(ApplyAnswer),
    Removed,
    Tool(ToolReply),
}

/// Answer a call for the project at `project`, with the sandboxes below `base`.
pub fn handle(project: &Path, base: &Path, call: SandboxCall) -> Result<Reply, String> {
    Ok(match call {
        SandboxCall::Create { id } => Reply::Info(create(project, base, &id)?),
        SandboxCall::Changes { sandbox } => {
            Reply::Changes(sandbox::changes(&open(project, base, &sandbox)?)?)
        }
        SandboxCall::FileVersions { sandbox, path } => {
            let (before, after) = sandbox::file_versions(&open(project, base, &sandbox)?, &path)?;
            Reply::Versions(FileVersions { before, after })
        }
        SandboxCall::Apply {
            sandbox,
            mode,
            paths,
        } => Reply::Applied(apply(
            project,
            &open(project, base, &sandbox)?,
            mode,
            &paths,
        )?),
        SandboxCall::Remove { dir } => {
            remove(project, base, &dir)?;
            Reply::Removed
        }
        SandboxCall::Tool {
            sandbox,
            scope,
            call,
        } => {
            let ws = workspace(&open(project, base, &sandbox)?, scope);
            let r = execute_file_tool(&ws, &call)
                .unwrap_or_else(|| Err(format!("{} is not a file tool", call.name())));
            Reply::Tool(r.into())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn calls_work_on_the_projects_sandboxes_only() {
        let repo = tempfile::tempdir().unwrap();
        let r = repo.path();
        git(r, &["init", "--quiet", "-b", "main"]);
        std::fs::write(r.join("a.txt"), "one\n").unwrap();
        std::fs::write(r.join("secret.env"), "KEY=1\n").unwrap();
        std::fs::write(r.join(".aiignore"), "secret.env\n").unwrap();
        git(r, &["add", "-A"]);
        git(r, &["commit", "--quiet", "-m", "init"]);
        let base = tempfile::tempdir().unwrap();
        let b = base.path();

        assert!(create(r, b, "../x").is_err());
        let info = match handle(r, b, SandboxCall::Create { id: "t1".into() }).unwrap() {
            Reply::Info(i) => i,
            other => panic!("{other:?}"),
        };
        assert!(info.dir.starts_with(&*b.to_string_lossy()));
        let sb = SandboxRef {
            dir: info.dir.clone(),
            base: info.base.clone(),
        };
        let tool = |call: ToolCall, scope: Vec<String>| match handle(
            r,
            b,
            SandboxCall::Tool {
                sandbox: sb.clone(),
                scope,
                call,
            },
        )
        .unwrap()
        {
            Reply::Tool(t) => t,
            other => panic!("{other:?}"),
        };
        let w = tool(
            ToolCall::WriteFile {
                path: "a.txt".into(),
                content: "two\n".into(),
            },
            Vec::new(),
        );
        assert!(w.ok, "{w:?}");
        // .aiignore of the project applies, and the scope.
        assert!(
            !tool(
                ToolCall::ReadFile {
                    path: "secret.env".into(),
                    start_line: None,
                    end_line: None
                },
                Vec::new()
            )
            .ok
        );
        assert!(
            !tool(
                ToolCall::WriteFile {
                    path: "b.txt".into(),
                    content: "x".into(),
                },
                vec!["src".into()]
            )
            .ok
        );
        assert!(!tool(ToolCall::RunTests, Vec::new()).ok);

        let changes = match handle(
            r,
            b,
            SandboxCall::Changes {
                sandbox: sb.clone(),
            },
        )
        .unwrap()
        {
            Reply::Changes(c) => c,
            other => panic!("{other:?}"),
        };
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "a.txt");

        // Folders that are not sandboxes of this project are refused.
        for dir in [
            r.to_string_lossy().into_owned(),
            b.to_string_lossy().into_owned(),
            format!("{}/../t1", info.dir),
            b.join("other-project/t1").to_string_lossy().into_owned(),
        ] {
            let e = handle(
                r,
                b,
                SandboxCall::Changes {
                    sandbox: SandboxRef {
                        dir: dir.clone(),
                        base: info.base.clone(),
                    },
                },
            )
            .unwrap_err();
            assert!(e.contains("not a sandbox"), "{dir}: {e}");
            assert!(handle(r, b, SandboxCall::Remove { dir }).is_err());
        }
        assert!(r.exists());

        let applied = match handle(
            r,
            b,
            SandboxCall::Apply {
                sandbox: sb.clone(),
                mode: ApplyMode::Apply,
                paths: Vec::new(),
            },
        )
        .unwrap()
        {
            Reply::Applied(a) => a,
            other => panic!("{other:?}"),
        };
        assert!(applied.result.applied);
        assert_eq!(std::fs::read_to_string(r.join("a.txt")).unwrap(), "two\n");
        handle(
            r,
            b,
            SandboxCall::Remove {
                dir: info.dir.clone(),
            },
        )
        .unwrap();
        assert!(!Path::new(&info.dir).exists());
        assert!(handle(r, b, SandboxCall::Changes { sandbox: sb })
            .unwrap_err()
            .contains("gone"));
    }
}
