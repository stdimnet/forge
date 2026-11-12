//! The Git menu dialogs on a repository: Pull, Merge and Rebase with options, the operation in
//! progress (Continue / Abort / Skip), Reset HEAD, New Tag, editing remotes and the stash
//! details of the Unstash dialog.
//!
//! Operations that stop with conflicts return git's error; the unmerged files are then
//! listed by [`Repo::conflicts`] and the UI opens the conflict resolver.

use serde::Serialize;

use super::{check_rev, parse_rebase_status, RemoteResult, Repo};
use crate::cmd::{self, run_raw, Run};
use crate::log::{parse_name_status, ChangedFile};
use crate::ops::{
    merge_args, pull_args, pull_into_args, rebase_args, reset_args, split_remote_branch, tag_args,
    update_branch_args, MergeOptions, OperationAction, PullOptions, RebaseOptions, ResetMode,
    TagOptions,
};
use forge_core::{ForgeError, Result};

/// An operation that stopped and waits for the user (conflicts, `edit` steps).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OngoingOperation {
    Rebase,
    Merge,
    CherryPick,
    Revert,
}

/// What the repository is in the middle of, for the branch widget ("Rebasing main") and the
/// Continue / Abort / Skip actions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationState {
    pub operation: Option<OngoingOperation>,
    /// The branch being rebased (or the current branch).
    pub branch: Option<String>,
    /// What the branch is rebased onto, as a name when git knows one.
    pub onto: Option<String>,
}

/// Before and after of one file of a stash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StashFileContents {
    pub before: Option<String>,
    pub after: Option<String>,
}

/// A rebase in progress (see [`Repo::rebase_in_progress`]).
pub(super) struct RebaseProgress {
    /// The branch being rebased and the commit it is rebased onto, when git says so.
    pub branch_onto: Option<(String, String)>,
}

const PSEUDO_REFS: [(&str, OngoingOperation); 4] = [
    ("REBASE_HEAD", OngoingOperation::Rebase),
    ("MERGE_HEAD", OngoingOperation::Merge),
    ("CHERRY_PICK_HEAD", OngoingOperation::CherryPick),
    ("REVERT_HEAD", OngoingOperation::Revert),
];

fn invalid(e: String) -> ForgeError {
    ForgeError::Other(e)
}

fn stash_ref(index: usize) -> String {
    format!("stash@{{{index}}}")
}

impl Repo {
    fn git_args(&self, args: &[String]) -> Result<String> {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.git(&refs)
    }

    /// Pull… with the options of the Pull dialog.
    pub fn pull_with(&self, options: &PullOptions) -> Result<RemoteResult> {
        let args = pull_args(options).map_err(invalid)?;
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.git_remote(&refs)
    }

    /// "Pull into 'current' Using Merge / Rebase" on a remote branch (`origin/feature/x`):
    /// fetches that branch and merges it into, or rebases the current branch onto, it.
    pub fn pull_into_current(&self, remote_branch: &str, rebase: bool) -> Result<RemoteResult> {
        check_rev(remote_branch)?;
        let remotes = self.remotes()?;
        let (remote, branch) = split_remote_branch(remote_branch, &remotes).ok_or_else(|| {
            ForgeError::Other(format!("'{remote_branch}' is not a branch of a remote"))
        })?;
        let args = pull_into_args(remote, branch, rebase).map_err(invalid)?;
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.git_remote(&refs)
    }

    /// "Update" on a local branch that is not checked out: fetches its tracked branch and
    /// fast-forwards the local branch to it, without touching the working tree. Fails when
    /// the branch has no tracked branch, is checked out (here or in another worktree) or has
    /// commits of its own, which would need a merge or rebase. Reports `<old>..<new>` (short
    /// hashes) or "Already up to date.".
    pub fn update_branch(&self, name: &str) -> Result<RemoteResult> {
        check_rev(name)?;
        if !self.has_branch(name) {
            return Err(ForgeError::Other(format!("no local branch '{name}'")));
        }
        let current = self
            .git(&["symbolic-ref", "--short", "-q", "HEAD"])
            .ok()
            .map(|s| s.trim().to_string());
        if current.as_deref() == Some(name) {
            return Err(ForgeError::Other(format!(
                "'{name}' is checked out: use Update Project"
            )));
        }
        let remote = self.config_value(&format!("branch.{name}.remote"));
        let merge = self.config_value(&format!("branch.{name}.merge"));
        let (Some(remote), Some(merge)) = (remote, merge) else {
            return Err(ForgeError::Other(format!("'{name}' has no tracked branch")));
        };
        let args = update_branch_args(&remote, &merge, name).map_err(invalid)?;
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let tip = || self.resolve_commit(&format!("refs/heads/{name}"));
        let before = tip();
        self.git_remote(&refs).map_err(|e| {
            if e.to_string().contains("non-fast-forward") {
                ForgeError::Other(format!(
                    "'{name}' has commits that are not in its tracked branch. Check it out and update it there."
                ))
            } else {
                e
            }
        })?;
        // git's fetch report lists every ref it touched; the UI only needs the branch's move.
        let short = |sha: &Option<String>| {
            sha.as_deref()
                .unwrap_or_default()
                .chars()
                .take(8)
                .collect::<String>()
        };
        let after = tip();
        Ok(RemoteResult {
            output: if before == after {
                "Already up to date.".into()
            } else {
                format!("{}..{}", short(&before), short(&after))
            },
        })
    }

    /// Merge… with the options of the Merge dialog. Returns git's report.
    pub fn merge_with(&self, options: &MergeOptions) -> Result<String> {
        let args = merge_args(options).map_err(invalid)?;
        Ok(self.git_args(&args)?.trim().to_string())
    }

    /// Rebase… with the options of the Rebase dialog.
    pub fn rebase_with(&self, options: &RebaseOptions) -> Result<String> {
        let mut args: Vec<String> = vec!["-c".into(), "core.editor=true".into()];
        args.extend(rebase_args(options).map_err(invalid)?);
        let out = run_raw(
            self.exec.as_ref(),
            &self.root,
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
            Run::default(),
        )?;
        if out.ok {
            Ok(format!("{}{}", out.stdout, out.stderr).trim().to_string())
        } else {
            Err(cmd::error_from(&["rebase"], &out))
        }
    }

    /// The operation the repository is in the middle of, if any.
    pub fn operation_state(&self) -> Result<OperationState> {
        let input: String = PSEUDO_REFS.iter().map(|(r, _)| format!("{r}\n")).collect();
        let out = self.git_stdin(&["cat-file", "--batch-check=%(objectname)"], &input)?;
        let present: Vec<bool> = out.lines().map(|l| !l.ends_with(" missing")).collect();
        let current = || {
            self.git(&["symbolic-ref", "--short", "-q", "HEAD"])
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        if present.first() == Some(&true) {
            if let Some(progress) = self.rebase_in_progress() {
                let (branch, onto) = match progress.branch_onto {
                    Some((branch, onto)) => (Some(branch), Some(self.ref_name(&onto))),
                    None => (current(), None),
                };
                return Ok(OperationState {
                    operation: Some(OngoingOperation::Rebase),
                    branch,
                    onto,
                });
            }
        }
        let operation = PSEUDO_REFS
            .iter()
            .zip(present.iter().chain(std::iter::repeat(&false)))
            .skip(1)
            .find(|(_, on)| **on)
            .map(|((_, op), _)| *op);
        Ok(OperationState {
            branch: operation.and_then(|_| current()),
            operation,
            onto: None,
        })
    }

    /// A rebase that really is in progress, from the long `git status`. `REBASE_HEAD` alone
    /// is not enough: git leaves it behind after `rebase --skip`, which would make later
    /// merge or unstash conflicts look like a rebase (and swap Yours and Theirs).
    pub(super) fn rebase_in_progress(&self) -> Option<RebaseProgress> {
        let status = self.git(&["status"]).ok()?;
        status
            .contains("rebase in progress")
            .then(|| RebaseProgress {
                branch_onto: parse_rebase_status(&status),
            })
    }

    /// A branch or tag name for a commit (`name-rev`), else the short hash.
    fn ref_name(&self, sha: &str) -> String {
        let name = self
            .git(&["name-rev", "--name-only", "--no-undefined", sha])
            .unwrap_or_default();
        let name = name.trim();
        let name = name
            .strip_prefix("remotes/")
            .or_else(|| name.strip_prefix("tags/"))
            .unwrap_or(name);
        let name = name.split(['~', '^']).next().unwrap_or_default();
        if name.is_empty() || name == "undefined" {
            sha.chars().take(7).collect()
        } else {
            name.to_string()
        }
    }

    /// Continue, abort or skip the operation in progress (Continue Rebase, Abort Merge, …).
    pub fn operation_action(&self, action: OperationAction) -> Result<String> {
        let state = self.operation_state()?;
        let cmd = match state.operation {
            Some(OngoingOperation::Rebase) => "rebase",
            Some(OngoingOperation::Merge) => "merge",
            Some(OngoingOperation::CherryPick) => "cherry-pick",
            Some(OngoingOperation::Revert) => "revert",
            None => return Err(ForgeError::Other("no operation is in progress".into())),
        };
        let flag = match action {
            OperationAction::Continue => "--continue",
            OperationAction::Abort => "--abort",
            OperationAction::Skip if cmd == "merge" => {
                return Err(ForgeError::Other("a merge cannot skip commits".into()))
            }
            OperationAction::Skip => "--skip",
        };
        let args = ["-c", "core.editor=true", cmd, flag];
        let out = run_raw(self.exec.as_ref(), &self.root, &args, Run::default())?;
        if cmd == "rebase" && self.operation_state()?.operation != Some(OngoingOperation::Rebase) {
            // A finished rebase may leave REBASE_HEAD behind (`--skip`), which would make
            // later conflicts look like rebase conflicts.
            self.git(&["update-ref", "-d", "REBASE_HEAD"])?;
        }
        if out.ok {
            Ok(format!("{}{}", out.stdout, out.stderr).trim().to_string())
        } else {
            Err(cmd::error_from(&[cmd], &out))
        }
    }

    /// Reset HEAD… (`git reset --soft|--mixed|--hard|--keep <rev>`).
    pub fn reset_head(&self, rev: &str, mode: ResetMode) -> Result<()> {
        let args = reset_args(rev, mode).map_err(invalid)?;
        self.git_args(&args).map(drop)
    }

    /// New Tag… with the options of the Tag dialog (force, commit, annotation message).
    pub fn tag(&self, options: &TagOptions) -> Result<()> {
        let args = tag_args(options).map_err(invalid)?;
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let message = options
            .message
            .as_deref()
            .map(str::trim)
            .unwrap_or_default();
        self.git_stdin(&refs, message).map(drop)
    }

    /// Whether `rev` names a commit (the Validate buttons of the Tag and Reset dialogs).
    pub fn resolve_commit(&self, rev: &str) -> Option<String> {
        check_rev(rev).ok()?;
        let spec = format!("{rev}^{{commit}}");
        self.git(&["rev-parse", "-q", "--verify", &spec])
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Edit Remote: rename it and/or change its URL.
    pub fn edit_remote(&self, old_name: &str, name: &str, url: &str) -> Result<()> {
        check_rev(old_name)?;
        check_rev(name)?;
        let url = url.trim();
        if url.is_empty() || url.starts_with('-') {
            return Err(ForgeError::Other(format!("invalid remote URL: {url:?}")));
        }
        if old_name != name {
            self.git(&["remote", "rename", old_name, name])?;
        }
        self.git(&["remote", "set-url", name, url]).map(drop)
    }

    // ---- stash details ------------------------------------------------------------------

    /// The files a stash changes (untracked files it saved count as added).
    pub fn stash_files(&self, index: usize) -> Result<Vec<ChangedFile>> {
        let sel = stash_ref(index);
        let parent = format!("{sel}^1");
        let tree = self.git(&[
            "diff-tree",
            "-r",
            "-z",
            "--name-status",
            "--no-renames",
            &parent,
            &sel,
        ])?;
        let mut files = parse_name_status(&tree);
        let untracked = format!("{sel}^3");
        if self.resolve_commit(&untracked).is_some() {
            let list = self.git(&["ls-tree", "-r", "-z", "--name-only", &untracked])?;
            files.extend(
                list.split('\0')
                    .filter(|p| !p.is_empty())
                    .map(|p| ChangedFile {
                        path: p.to_string(),
                        orig_path: None,
                        status: "A".into(),
                    }),
            );
        }
        Ok(files
            .into_iter()
            .filter(|f| self.in_workspace(&f.path))
            .map(|mut f| {
                f.path = self.repo_to_ws(&f.path);
                f
            })
            .collect())
    }

    /// One file of a stash before and after the stashed change.
    pub fn stash_file(&self, index: usize, path: &str) -> Result<StashFileContents> {
        let sel = stash_ref(index);
        let before = self.show(&format!("{sel}^1"), path)?;
        let mut after = self.show(&sel, path)?;
        if after.is_none() && self.resolve_commit(&format!("{sel}^3")).is_some() {
            after = self.show(&format!("{sel}^3"), path)?;
        }
        Ok(StashFileContents { before, after })
    }

    /// Clear: drop every stash.
    pub fn stash_clear(&self) -> Result<()> {
        self.git(&["stash", "clear"]).map(drop)
    }

    /// Unstash "As new branch": create a branch at the stash's base, check it out and pop
    /// the stash there.
    pub fn stash_branch(&self, index: usize, branch: &str) -> Result<String> {
        check_rev(branch)?;
        let sel = stash_ref(index);
        Ok(self
            .git(&["stash", "branch", branch, &sel])?
            .trim()
            .to_string())
    }
}
