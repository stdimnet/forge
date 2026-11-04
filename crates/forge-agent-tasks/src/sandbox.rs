//! The sandbox of a task: a detached git worktree of the project's repository in Forge's
//! cache folder. It starts from a snapshot of the working tree (HEAD plus uncommitted and
//! untracked files, taken with `git stash create` without touching the working tree), so
//! the agent sees what the user sees, and the user's files stay unchanged until they apply
//! the diff.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

/// Identity of the sandbox's own commits.
const IDENTITY: [&str; 6] = [
    "-c",
    "user.name=Forge",
    "-c",
    "user.email=forge@localhost",
    "-c",
    "commit.gpgsign=false",
];

fn git(cwd: &Path, args: &[&str]) -> Result<String, String> {
    git_with(cwd, args, None)
}

fn git_with(cwd: &Path, args: &[&str], stdin: Option<&str>) -> Result<String, String> {
    use std::io::Write;
    let program = forge_vcs::git_binary();
    let mut cmd = Command::new(&program);
    cmd.current_dir(cwd)
        .args(["-c", "core.quotepath=off", "-c", "color.ui=never"])
        .args(args)
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_EDITOR", "true");
    // In Flatpak git runs on the host.
    let mut cmd = forge_hostspawn::host_command(cmd);
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not run {program}: {e}"))?;
    if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(input.as_bytes())
            .map_err(|e| format!("git: {e}"))?;
    }
    let out = child.wait_with_output().map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let msg = if err.trim().is_empty() {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        } else {
            err.trim().to_string()
        };
        Err(format!("git {}: {msg}", args.first().unwrap_or(&"")))
    }
}

/// A task's sandbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sandbox {
    /// The worktree (mirrors the repository root).
    pub dir: PathBuf,
    /// The project root inside the worktree (differs when the project is a subfolder of
    /// its repository).
    pub project_dir: PathBuf,
    /// The commit the diff is taken against.
    pub base: String,
}

/// The repository root of `project` and the project's path inside it (`""` at the root).
pub fn repository_of(project: &Path) -> Result<(PathBuf, String), String> {
    let top = git(project, &["rev-parse", "--show-toplevel"]).map_err(|_| {
        "Agent tasks need a Git repository: the sandbox is a git worktree.".to_string()
    })?;
    let top = PathBuf::from(top.trim());
    let prefix = git(project, &["rev-parse", "--show-prefix"]).unwrap_or_default();
    Ok((top, prefix.trim().trim_end_matches('/').to_string()))
}

/// Create the sandbox of a task in `dir` (which must not exist yet).
pub fn create(project: &Path, dir: &Path) -> Result<Sandbox, String> {
    let (top, prefix) = repository_of(project)?;
    if dir.starts_with(&top) {
        return Err("the sandbox must be outside the project".into());
    }
    let head = git(&top, &["rev-parse", "--verify", "HEAD"]).map_err(|_| {
        "The repository has no commits yet; commit once to use agent tasks.".to_string()
    })?;
    // A commit of the working tree and index; empty when nothing is modified.
    let stash = git(&top, &["stash", "create"]).unwrap_or_default();
    let start = if stash.trim().is_empty() {
        head.trim().to_string()
    } else {
        stash.trim().to_string()
    };
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let dir_s = dir.to_string_lossy().into_owned();
    git(
        &top,
        &["worktree", "add", "--detach", "--quiet", &dir_s, &start],
    )?;
    let mut base = head.trim().to_string();
    // Untracked (not ignored) files are copied as well.
    let untracked =
        git(&top, &["ls-files", "--others", "--exclude-standard", "-z"]).unwrap_or_default();
    let mut copied = false;
    for rel in untracked.split('\0').filter(|s| !s.is_empty()) {
        let (from, to) = (top.join(rel), dir.join(rel));
        if !from.is_file() {
            continue;
        }
        if let Some(parent) = to.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::copy(&from, &to).is_ok() {
            copied = true;
        }
    }
    if copied || start != base {
        // The snapshot becomes the sandbox's base commit, so the review shows only the
        // agent's changes.
        git(dir, &["add", "-A"])?;
        let mut args: Vec<&str> = IDENTITY.to_vec();
        args.extend([
            "commit",
            "--no-verify",
            "--quiet",
            "--allow-empty",
            "-m",
            "Working tree snapshot for a Forge agent task",
        ]);
        git(dir, &args)?;
        base = git(dir, &["rev-parse", "HEAD"])?.trim().to_string();
    }
    let project_dir = if prefix.is_empty() {
        dir.to_path_buf()
    } else {
        dir.join(&prefix)
    };
    Ok(Sandbox {
        dir: dir.to_path_buf(),
        project_dir,
        base,
    })
}

/// How a file changed in the sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

/// One changed file of the sandbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    /// Repository-relative path.
    pub path: String,
    pub kind: ChangeKind,
    pub additions: u32,
    pub deletions: u32,
    pub binary: bool,
}

/// Stage everything in the sandbox (its own index) so new files show up in the diff.
fn stage(sb: &Sandbox) -> Result<(), String> {
    git(&sb.dir, &["add", "-A"]).map(|_| ())
}

/// The files the agent changed, against the base commit.
pub fn changes(sb: &Sandbox) -> Result<Vec<FileChange>, String> {
    stage(sb)?;
    let status = git(
        &sb.dir,
        &[
            "diff",
            "--cached",
            "--no-renames",
            "--name-status",
            "-z",
            &sb.base,
        ],
    )?;
    let numstat = git(
        &sb.dir,
        &[
            "diff",
            "--cached",
            "--no-renames",
            "--numstat",
            "-z",
            &sb.base,
        ],
    )?;
    let mut stats = std::collections::HashMap::new();
    for rec in numstat.split('\0').filter(|s| !s.is_empty()) {
        let mut it = rec.splitn(3, '\t');
        let (a, d, p) = (it.next(), it.next(), it.next());
        if let (Some(a), Some(d), Some(p)) = (a, d, p) {
            stats.insert(p.to_string(), (a.parse().ok(), d.parse().ok()));
        }
    }
    let mut out = Vec::new();
    let mut parts = status.split('\0').filter(|s| !s.is_empty());
    while let (Some(code), Some(path)) = (parts.next(), parts.next()) {
        let kind = match code.chars().next() {
            Some('A') => ChangeKind::Added,
            Some('D') => ChangeKind::Deleted,
            _ => ChangeKind::Modified,
        };
        let (a, d): (Option<u32>, Option<u32>) = stats.get(path).copied().unwrap_or((None, None));
        out.push(FileChange {
            path: path.to_string(),
            kind,
            additions: a.unwrap_or(0),
            deletions: d.unwrap_or(0),
            binary: a.is_none() && d.is_none() && stats.contains_key(path),
        });
    }
    Ok(out)
}

/// The base and the new text of a changed file (`None`: the file does not exist on that
/// side, or is binary).
pub fn file_versions(sb: &Sandbox, path: &str) -> Result<(Option<String>, Option<String>), String> {
    let rel = crate::workspace::normalize(path)?;
    let before = git(&sb.dir, &["show", &format!("{}:{rel}", sb.base)]).ok();
    let after = std::fs::read(sb.dir.join(&rel))
        .ok()
        .and_then(|b| String::from_utf8(b).ok());
    Ok((before, after))
}

/// The binary patch of the agent's changes, optionally only for `paths`.
pub fn patch(sb: &Sandbox, paths: &[String]) -> Result<String, String> {
    stage(sb)?;
    let mut args = vec![
        "diff",
        "--cached",
        "--no-renames",
        "--binary",
        "--full-index",
        sb.base.as_str(),
    ];
    if !paths.is_empty() {
        args.push("--");
        args.extend(paths.iter().map(String::as_str));
    }
    git(&sb.dir, &args)
}

/// The result of applying a sandbox to the working tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub applied: bool,
    pub files: Vec<String>,
    /// Files whose changes do not apply because the working tree changed meanwhile.
    pub conflicts: Vec<String>,
    pub message: Option<String>,
}

/// Paths named in `git apply` errors (`error: patch failed: a.php:3`,
/// `error: b.php: does not exist in index`, `error: c.php: already exists in working directory`).
pub fn conflicting_paths(stderr: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in stderr.lines() {
        let Some(rest) = line.trim().strip_prefix("error: ") else {
            continue;
        };
        let path = if let Some(p) = rest.strip_prefix("patch failed: ") {
            p.rsplit_once(':').map_or(p, |(p, _)| p)
        } else if let Some((p, _)) = rest.split_once(": ") {
            p
        } else {
            continue;
        };
        if !path.is_empty() && !out.iter().any(|o| o == path) {
            out.push(path.to_string());
        }
    }
    out
}

fn files_of(patch: &str) -> Vec<String> {
    forge_vcs::patch::split_patch(patch)
        .into_iter()
        .map(|f| f.path)
        .collect()
}

/// Apply the agent's changes (or those of `paths`) to the user's working tree. All or
/// nothing: when a file does not apply cleanly nothing is changed and the conflicting
/// files are reported.
pub fn apply(project: &Path, sb: &Sandbox, paths: &[String]) -> Result<ApplyResult, String> {
    let (top, _) = repository_of(project)?;
    let patch = patch(sb, paths)?;
    if patch.trim().is_empty() {
        return Ok(ApplyResult {
            applied: false,
            files: Vec::new(),
            conflicts: Vec::new(),
            message: Some("There are no changes to apply.".into()),
        });
    }
    let files = files_of(&patch);
    if let Err(e) = git_with(
        &top,
        &["apply", "--check", "--whitespace=nowarn", "-"],
        Some(&patch),
    ) {
        return Ok(ApplyResult {
            applied: false,
            files,
            conflicts: conflicting_paths(&e),
            message: Some(
                "The changes do not apply to the working tree, which changed since the task started. Nothing was changed; apply them as a branch instead."
                    .into(),
            ),
        });
    }
    git_with(&top, &["apply", "--whitespace=nowarn", "-"], Some(&patch))?;
    Ok(ApplyResult {
        applied: true,
        files,
        conflicts: Vec::new(),
        message: None,
    })
}

/// Apply and commit only the applied files (`git commit --only`). Returns the commit hash.
pub fn apply_as_commit(
    project: &Path,
    sb: &Sandbox,
    paths: &[String],
    message: &str,
) -> Result<(ApplyResult, Option<String>), String> {
    let result = apply(project, sb, paths)?;
    if !result.applied {
        return Ok((result, None));
    }
    let (top, _) = repository_of(project)?;
    let mut add = vec!["add", "-A", "--"];
    add.extend(result.files.iter().map(String::as_str));
    git(&top, &add)?;
    let mut commit = vec!["commit", "--quiet", "-m", message, "--only", "--"];
    commit.extend(result.files.iter().map(String::as_str));
    git(&top, &commit)?;
    let hash = git(&top, &["rev-parse", "--short", "HEAD"])?
        .trim()
        .to_string();
    Ok((result, Some(hash)))
}

/// Commit the agent's changes in the sandbox and point a new branch at that commit; the
/// working tree is not touched. The branch shares the repository, so it shows up in the
/// Git tool window right away.
pub fn apply_as_branch(sb: &Sandbox, branch: &str, message: &str) -> Result<String, String> {
    let branch = branch.trim();
    if branch.is_empty() || branch.starts_with('-') {
        return Err(format!("invalid branch name: {branch:?}"));
    }
    git(&sb.dir, &["check-ref-format", "--branch", branch])
        .map_err(|_| format!("invalid branch name: {branch:?}"))?;
    stage(sb)?;
    let mut args: Vec<&str> = IDENTITY[4..].to_vec();
    args.extend(["commit", "--no-verify", "--quiet", "-m", message]);
    git(&sb.dir, &args)?;
    git(&sb.dir, &["branch", branch, "HEAD"])?;
    Ok(git(&sb.dir, &["rev-parse", "--short", "HEAD"])?
        .trim()
        .to_string())
}

/// Remove the sandbox: the worktree, its administrative files and the folder.
pub fn remove(project: &Path, dir: &Path) -> Result<(), String> {
    if let Ok((top, _)) = repository_of(project) {
        let d = dir.to_string_lossy().into_owned();
        let _ = git(&top, &["worktree", "remove", "--force", &d]);
        let _ = git(&top, &["worktree", "prune"]);
    }
    if dir.exists() {
        std::fs::remove_dir_all(dir)
            .map_err(|e| format!("cannot remove {}: {e}", dir.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        git(r, &["init", "--quiet", "-b", "main"]).unwrap();
        // A committer identity for this test repository (shared by its worktrees).
        let config = r.join(".git/config");
        let mut text = std::fs::read_to_string(&config).unwrap();
        text.push_str(
            "[user]\n\tname = Test\n\temail = test@example.com\n[commit]\n\tgpgsign = false\n",
        );
        std::fs::write(&config, text).unwrap();
        std::fs::create_dir_all(r.join("src")).unwrap();
        std::fs::write(r.join("src/a.txt"), "one\ntwo\n").unwrap();
        std::fs::write(r.join("src/b.txt"), "bee\n").unwrap();
        std::fs::write(r.join(".gitignore"), "*.log\n").unwrap();
        git(r, &["add", "-A"]).unwrap();
        let mut args: Vec<&str> = IDENTITY.to_vec();
        args.extend(["commit", "--quiet", "-m", "init"]);
        git(r, &args).unwrap();
        dir
    }

    fn read(p: &Path) -> String {
        std::fs::read_to_string(p).unwrap()
    }

    #[test]
    fn sandbox_starts_from_the_working_tree_and_leaves_it_alone() {
        let repo = repo();
        let r = repo.path();
        let cache = tempfile::tempdir().unwrap();
        // Uncommitted and untracked work is part of the snapshot.
        std::fs::write(r.join("src/a.txt"), "one\ntwo\nthree\n").unwrap();
        std::fs::write(r.join("notes.md"), "draft\n").unwrap();
        std::fs::write(r.join("debug.log"), "ignored\n").unwrap();
        let sb = create(r, &cache.path().join("t1")).unwrap();
        assert_eq!(read(&sb.dir.join("src/a.txt")), "one\ntwo\nthree\n");
        assert_eq!(read(&sb.dir.join("notes.md")), "draft\n");
        assert!(!sb.dir.join("debug.log").exists());
        assert!(changes(&sb).unwrap().is_empty());
        // The user's working tree and branch are untouched.
        assert_eq!(read(&r.join("src/a.txt")), "one\ntwo\nthree\n");
        assert_eq!(
            git(r, &["rev-parse", "--abbrev-ref", "HEAD"])
                .unwrap()
                .trim(),
            "main"
        );
        assert!(git(r, &["status", "--porcelain"])
            .unwrap()
            .contains("notes.md"));

        // The agent edits, adds and deletes.
        std::fs::write(sb.dir.join("src/a.txt"), "one\nTWO\nthree\n").unwrap();
        std::fs::write(sb.dir.join("src/c.txt"), "new\n").unwrap();
        std::fs::remove_file(sb.dir.join("src/b.txt")).unwrap();
        let ch = changes(&sb).unwrap();
        let summary: Vec<(String, ChangeKind, u32, u32)> = ch
            .iter()
            .map(|c| (c.path.clone(), c.kind, c.additions, c.deletions))
            .collect();
        assert_eq!(
            summary,
            [
                ("src/a.txt".into(), ChangeKind::Modified, 1, 1),
                ("src/b.txt".into(), ChangeKind::Deleted, 0, 1),
                ("src/c.txt".into(), ChangeKind::Added, 1, 0),
            ]
        );
        let (before, after) = file_versions(&sb, "src/a.txt").unwrap();
        assert_eq!(before.as_deref(), Some("one\ntwo\nthree\n"));
        assert_eq!(after.as_deref(), Some("one\nTWO\nthree\n"));
        assert_eq!(file_versions(&sb, "src/c.txt").unwrap().0, None);
        assert_eq!(read(&r.join("src/a.txt")), "one\ntwo\nthree\n");

        // Apply only a.txt, then the rest.
        let res = apply(r, &sb, &["src/a.txt".into()]).unwrap();
        assert!(res.applied);
        assert_eq!(res.files, ["src/a.txt"]);
        assert_eq!(read(&r.join("src/a.txt")), "one\nTWO\nthree\n");
        assert!(r.join("src/b.txt").exists());
        let res = apply(r, &sb, &["src/b.txt".into(), "src/c.txt".into()]).unwrap();
        assert!(res.applied, "{res:?}");
        assert!(!r.join("src/b.txt").exists());
        assert_eq!(read(&r.join("src/c.txt")), "new\n");

        remove(r, &sb.dir).unwrap();
        assert!(!sb.dir.exists());
        assert_eq!(
            git(r, &["worktree", "list", "--porcelain"])
                .unwrap()
                .matches("worktree ")
                .count(),
            1
        );
    }

    #[test]
    fn conflicts_change_nothing() {
        let repo = repo();
        let r = repo.path();
        let cache = tempfile::tempdir().unwrap();
        let sb = create(r, &cache.path().join("t2")).unwrap();
        std::fs::write(sb.dir.join("src/a.txt"), "one\nagent\n").unwrap();
        std::fs::write(sb.dir.join("src/b.txt"), "bee agent\n").unwrap();
        // The user edits a.txt meanwhile.
        std::fs::write(r.join("src/a.txt"), "one\nuser\n").unwrap();
        let res = apply(r, &sb, &[]).unwrap();
        assert!(!res.applied);
        assert_eq!(res.conflicts, ["src/a.txt"]);
        assert_eq!(read(&r.join("src/a.txt")), "one\nuser\n");
        assert_eq!(read(&r.join("src/b.txt")), "bee\n");

        // As a branch it still works, without touching the working tree.
        let hash = apply_as_branch(&sb, "agent/fix-a", "Fix a").unwrap();
        assert!(!hash.is_empty());
        let log = git(r, &["log", "--format=%s", "agent/fix-a"]).unwrap();
        assert_eq!(log.lines().collect::<Vec<_>>(), ["Fix a", "init"]);
        assert_eq!(read(&r.join("src/a.txt")), "one\nuser\n");
        assert!(apply_as_branch(&sb, "bad name..", "x").is_err());
        remove(r, &sb.dir).unwrap();
    }

    #[test]
    fn apply_as_commit_commits_only_the_agents_files() {
        let repo = repo();
        let r = repo.path();
        let cache = tempfile::tempdir().unwrap();
        let sb = create(r, &cache.path().join("t3")).unwrap();
        std::fs::write(sb.dir.join("src/b.txt"), "bee 2\n").unwrap();
        std::fs::write(sb.dir.join("src/new.txt"), "n\n").unwrap();
        // Unrelated staged work of the user stays out of the commit.
        std::fs::write(r.join("other.txt"), "mine\n").unwrap();
        git(r, &["add", "other.txt"]).unwrap();
        let (res, hash) = apply_as_commit(r, &sb, &[], "Agent change").unwrap();
        assert!(res.applied);
        assert!(hash.is_some());
        let files = git(r, &["show", "--name-only", "--format=", "HEAD"]).unwrap();
        assert_eq!(
            files.lines().collect::<Vec<_>>(),
            ["src/b.txt", "src/new.txt"]
        );
        assert!(git(r, &["status", "--porcelain"])
            .unwrap()
            .contains("A  other.txt"));
        remove(r, &sb.dir).unwrap();
    }

    #[test]
    fn parses_apply_errors_and_requires_git() {
        let err = "error: patch failed: src/a.txt:1\nerror: src/a.txt: patch does not apply\nerror: src/c.txt: already exists in working directory\n";
        assert_eq!(conflicting_paths(err), ["src/a.txt", "src/c.txt"]);
        let plain = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        // A folder outside any repository (tempdirs are not inside this checkout).
        if git(plain.path(), &["rev-parse", "--show-toplevel"]).is_err() {
            assert!(create(plain.path(), &cache.path().join("x"))
                .unwrap_err()
                .contains("Git repository"));
        }
    }
}
