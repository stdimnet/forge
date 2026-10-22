//! The Git Log context menu: Cherry-Pick, Revert Commit, Undo Commit, Edit Commit Message,
//! Squash Commits, Drop Commits, Interactively Rebase from Here, Create Patch and Show
//! Repository at Revision. The plans and todo files come from [`crate::rebase`].
//!
//! Operations that stop (conflicts, `edit` steps) leave the repository in the rebase or
//! cherry-pick; the UI then hands over to Continue / Abort / Skip and the conflict resolver.

use serde::Serialize;

use super::{OngoingOperation, Repo};
use crate::cmd::{self, run_raw, Run};
use crate::rebase::{
    autosquash, is_protected, is_sha, log_action_states, plan_todo, render_todo, sequence_editor,
    CommitFacts, LogActionStates, RebaseAction, RebaseCommit, RebaseEntry, RebasePlan,
    SelectionFacts,
};
use forge_core::{ForgeError, Result};

/// The outcome of an action that rewrote the current branch.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteResult {
    /// git's report.
    pub output: String,
    /// HEAD before the rewrite (Undo resets the branch back to it).
    pub old_head: String,
    /// HEAD afterwards; `None` while the rebase waits (stopped at an `edit` step).
    pub new_head: Option<String>,
    /// The rebase stopped at an `edit` step and waits for Continue.
    pub stopped: bool,
}

/// Undo Commit: the message of the undone commit, for the Commit tool window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoneCommit {
    pub message: String,
}

fn other(msg: impl Into<String>) -> ForgeError {
    ForgeError::Other(msg.into())
}

fn check_shas(shas: &[String]) -> Result<()> {
    if shas.is_empty() {
        return Err(other("no commits are selected"));
    }
    match shas.iter().find(|s| !is_sha(s)) {
        Some(bad) => Err(other(format!("invalid commit: {bad:?}"))),
        None => Ok(()),
    }
}

impl Repo {
    /// Run git and keep its output also on failure.
    fn git_out(&self, args: &[&str]) -> Result<cmd::Output> {
        run_raw(self.exec.as_ref(), &self.root, args, Run::default())
    }

    fn head_sha(&self) -> Result<String> {
        Ok(self.git(&["rev-parse", "HEAD"])?.trim().to_string())
    }

    fn full_sha(&self, sha: &str) -> Result<String> {
        if !is_sha(sha) {
            return Err(other(format!("invalid commit: {sha:?}")));
        }
        let spec = format!("{sha}^{{commit}}");
        Ok(self
            .git(&["rev-parse", "--verify", &spec])?
            .trim()
            .to_string())
    }

    fn is_ancestor(&self, sha: &str, of: &str) -> bool {
        self.git_out(&["merge-base", "--is-ancestor", sha, of])
            .is_ok_and(|o| o.ok)
    }

    /// Store `text` as a blob (todo files and commit messages for the rebase).
    fn write_blob(&self, text: &str) -> Result<String> {
        Ok(self
            .git_stdin(&["hash-object", "-w", "--stdin"], text)?
            .trim()
            .to_string())
    }

    // ---- facts and availability ---------------------------------------------------------------

    /// What the Log context menu needs to know about the selected commits (log order).
    pub fn log_selection_facts(
        &self,
        shas: &[String],
        protected: &[String],
    ) -> Result<SelectionFacts> {
        check_shas(shas)?;
        let head = self.head_sha()?;
        let detached = self
            .git_out(&["symbolic-ref", "-q", "HEAD"])
            .map(|o| !o.ok)
            .unwrap_or(true);
        let mut args = vec!["log", "--no-walk=unsorted", "--format=%H%x1f%P"];
        args.extend(shas.iter().map(String::as_str));
        let parents: Vec<(String, usize)> = self
            .git(&args)?
            .lines()
            .filter_map(|l| l.split_once('\x1f'))
            .map(|(sha, p)| (sha.to_string(), p.split_whitespace().count()))
            .collect();
        let mut commits = Vec::new();
        for (sha, n_parents) in parents {
            let pushed: Vec<String> = self
                .git(&[
                    "for-each-ref",
                    "--format=%(refname:short)",
                    "--contains",
                    &sha,
                    "refs/remotes",
                ])?
                .lines()
                .map(str::trim)
                .filter(|r| !r.is_empty() && !r.ends_with("/HEAD"))
                .map(str::to_string)
                .collect();
            let protected_branch = pushed
                .iter()
                .find(|r| {
                    let branch = r.split_once('/').map_or(r.as_str(), |(_, b)| b);
                    is_protected(branch, protected)
                })
                .cloned();
            commits.push(CommitFacts {
                in_current_branch: self.is_ancestor(&sha, "HEAD"),
                head: sha == head,
                merge: n_parents > 1,
                root: n_parents == 0,
                pushed_to: protected_branch.clone().or_else(|| pushed.first().cloned()),
                protected_branch,
                sha,
            });
        }
        let merges_after = if commits.iter().all(|c| c.in_current_branch) {
            let oldest = self.oldest_of(shas)?;
            let range = format!("{oldest}..HEAD");
            self.git(&["rev-list", "--merges", "--count", &range])?
                .trim()
                .parse::<usize>()
                .unwrap_or(0)
                > 0
        } else {
            false
        };
        let operation = self.operation_state()?.operation.map(|op| {
            match op {
                OngoingOperation::Rebase => "rebase",
                OngoingOperation::Merge => "merge",
                OngoingOperation::CherryPick => "cherryPick",
                OngoingOperation::Revert => "revert",
            }
            .to_string()
        });
        Ok(SelectionFacts {
            commits,
            merges_after,
            operation,
            detached,
        })
    }

    /// Which Log actions the selection allows (disabled ones with PHPStorm's reasons).
    pub fn log_action_states(
        &self,
        shas: &[String],
        protected: &[String],
    ) -> Result<LogActionStates> {
        Ok(log_action_states(
            &self.log_selection_facts(shas, protected)?,
        ))
    }

    /// The oldest of commits on one line of history (their common ancestor).
    fn oldest_of(&self, shas: &[String]) -> Result<String> {
        if shas.len() == 1 {
            return self.full_sha(&shas[0]);
        }
        let mut args = vec!["merge-base", "--octopus"];
        args.extend(shas.iter().map(String::as_str));
        let base = self.git(&args)?.trim().to_string();
        let full: Vec<String> = shas
            .iter()
            .map(|s| self.full_sha(s))
            .collect::<Result<_>>()?;
        if full.contains(&base) {
            Ok(base)
        } else {
            Err(other("the selected commits are not on one line of history"))
        }
    }

    // ---- cherry-pick, revert, undo ------------------------------------------------------------

    /// Cherry-Pick (`shas` in log order, newest first; they are applied oldest first). With
    /// `suffix`, commits pushed to a protected branch get the "cherry picked from commit"
    /// line (`-x`). Commits whose changes are already present are skipped.
    pub fn cherry_pick(
        &self,
        shas: &[String],
        suffix: bool,
        protected: &[String],
    ) -> Result<String> {
        check_shas(shas)?;
        let add_x = suffix
            && self
                .log_selection_facts(shas, protected)?
                .commits
                .iter()
                .any(|c| c.protected_branch.is_some());
        let mut args = vec!["-c", "core.editor=true", "cherry-pick"];
        if add_x {
            args.push("-x");
        }
        args.extend(shas.iter().rev().map(String::as_str));
        let mut out = self.git_out(&args)?;
        let mut skipped = 0;
        // An empty pick stops without conflicts; skip it like PHPStorm and go on.
        while !out.ok
            && format!("{}{}", out.stdout, out.stderr).contains("is now empty")
            && self.operation_state()?.operation == Some(OngoingOperation::CherryPick)
            && self.unmerged(&[])?.is_empty()
        {
            skipped += 1;
            out = self.git_out(&["-c", "core.editor=true", "cherry-pick", "--skip"])?;
        }
        if !out.ok {
            return Err(cmd::error_from(&["cherry-pick"], &out));
        }
        let mut report = format!("{}{}", out.stdout, out.stderr).trim().to_string();
        if skipped > 0 {
            report = format!(
                "{report}\n{skipped} commit{} skipped: the changes are already in the branch",
                if skipped == 1 { " was" } else { "s were" }
            )
            .trim()
            .to_string();
        }
        Ok(report)
    }

    /// Revert Commit: new commits that undo `shas` (log order, reverted newest first).
    pub fn revert_commits(&self, shas: &[String]) -> Result<String> {
        check_shas(shas)?;
        let mut args = vec!["-c", "core.editor=true", "revert", "--no-edit"];
        args.extend(shas.iter().map(String::as_str));
        let out = self.git_out(&args)?;
        if out.ok {
            Ok(format!("{}{}", out.stdout, out.stderr).trim().to_string())
        } else {
            Err(cmd::error_from(&["revert"], &out))
        }
    }

    /// Undo Commit: remove the last commit and keep its changes staged (`reset --soft`).
    pub fn undo_commit(&self, sha: &str) -> Result<UndoneCommit> {
        let sha = self.full_sha(sha)?;
        if sha != self.head_sha()? {
            return Err(other(
                "Undo Commit is available only for the last commit of the current branch",
            ));
        }
        let message = self
            .git(&["log", "-1", "--format=%B", &sha])?
            .trim_end()
            .to_string();
        let parent = format!("{sha}^");
        if self.resolve_commit(&parent).is_some() {
            self.git(&["reset", "-q", "--soft", &parent])?;
        } else {
            // The root commit: the branch becomes unborn, the index keeps the files.
            let head_ref = self
                .git(&["symbolic-ref", "-q", "HEAD"])?
                .trim()
                .to_string();
            self.git(&["update-ref", "-d", &head_ref])?;
        }
        Ok(UndoneCommit { message })
    }

    /// Undo of a rewrite: move the branch back where it was, if nothing happened since.
    pub fn undo_rewrite(&self, old_head: &str, new_head: &str) -> Result<()> {
        let old = self.full_sha(old_head)?;
        let new = self.full_sha(new_head)?;
        if self.head_sha()? != new {
            return Err(other("the branch has changed since; cannot undo"));
        }
        self.git(&["reset", "-q", "--keep", &old]).map(drop)
    }

    // ---- interactive rebase -------------------------------------------------------------------

    /// The commits from `sha` (inclusive) to HEAD, oldest first, and where the rebase starts.
    fn rebase_range(&self, sha: &str) -> Result<(Option<String>, Vec<RebaseCommit>)> {
        let sha = self.full_sha(sha)?;
        if !self.is_ancestor(&sha, "HEAD") {
            return Err(other("The selected commit is not in the current branch"));
        }
        let parent = format!("{sha}^");
        let base = self.resolve_commit(&parent);
        let range = match &base {
            Some(b) => format!("{b}..HEAD"),
            None => "HEAD".to_string(),
        };
        if self
            .git(&["rev-list", "--merges", "--count", &range])?
            .trim()
            != "0"
        {
            return Err(other(
                "The branch contains merge commits after the selected commit; a rebase would flatten them",
            ));
        }
        let out = self.git(&[
            "log",
            "--reverse",
            "--topo-order",
            "--format=%H%x1f%an%x1f%at%x1f%B%x1e",
            &range,
        ])?;
        let commits = out
            .split('\x1e')
            .filter_map(|rec| {
                let rec = rec.trim_start_matches('\n');
                let f: Vec<&str> = rec.splitn(4, '\x1f').collect();
                (f.len() == 4).then(|| {
                    let message = f[3].trim_end().to_string();
                    RebaseCommit {
                        sha: f[0].to_string(),
                        subject: message.lines().next().unwrap_or_default().to_string(),
                        message,
                        author: f[1].to_string(),
                        time: f[2].parse().unwrap_or(0),
                    }
                })
            })
            .collect();
        Ok((base, commits))
    }

    /// Interactively Rebase from Here…: the dialog's commits and initial actions.
    pub fn rebase_plan(&self, sha: &str) -> Result<RebasePlan> {
        let (base, commits) = self.rebase_range(sha)?;
        let branch = self
            .git(&["symbolic-ref", "--short", "-q", "HEAD"])
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Ok(RebasePlan {
            entries: autosquash(&commits),
            base,
            branch,
            commits,
        })
    }

    /// Start Rebasing: rebase the commits from `sha` to HEAD with the dialog's plan. Every
    /// commit of the range must appear in `entries` exactly once.
    pub fn interactive_rebase(&self, sha: &str, entries: &[RebaseEntry]) -> Result<RewriteResult> {
        let (base, commits) = self.rebase_range(sha)?;
        let mut want: Vec<&str> = commits.iter().map(|c| c.sha.as_str()).collect();
        let mut got: Vec<&str> = entries.iter().map(|e| e.sha.as_str()).collect();
        want.sort_unstable();
        got.sort_unstable();
        if want != got {
            return Err(other(
                "the branch has changed since the rebase was planned; start the rebase again",
            ));
        }
        self.run_rebase(base.as_deref(), entries)
    }

    /// `git rebase -i` with a todo generated from `entries`; git never opens an editor.
    fn run_rebase(&self, base: Option<&str>, entries: &[RebaseEntry]) -> Result<RewriteResult> {
        if self.operation_state()?.operation.is_some() {
            return Err(other("Finish the operation in progress first"));
        }
        let (steps, messages) = plan_todo(entries).map_err(other)?;
        let blobs: Vec<String> = messages
            .iter()
            .map(|m| self.write_blob(m))
            .collect::<Result<_>>()?;
        let todo = self.write_blob(&render_todo(&steps, &blobs))?;
        let old_head = self.head_sha()?;
        let editor = format!("sequence.editor={}", sequence_editor(&todo));
        let mut args = vec![
            "-c",
            editor.as_str(),
            "-c",
            "core.editor=true",
            "-c",
            "rebase.updateRefs=false",
            "-c",
            "rebase.missingCommitsCheck=ignore",
            "-c",
            "rebase.autoSquash=false",
            "rebase",
            "-i",
            "--autostash",
        ];
        match base {
            Some(b) => args.push(b),
            None => args.push("--root"),
        }
        let out = self.git_out(&args)?;
        let output = format!("{}{}", out.stdout, out.stderr).trim().to_string();
        if !out.ok {
            return Err(cmd::error_from(&["rebase"], &out));
        }
        let stopped = self.operation_state()?.operation == Some(OngoingOperation::Rebase);
        Ok(RewriteResult {
            output,
            old_head,
            new_head: if stopped {
                None
            } else {
                Some(self.head_sha()?)
            },
            stopped,
        })
    }

    /// Every commit from `oldest` to HEAD picked, with `edit` applied to the selected ones.
    fn rewrite_selected(
        &self,
        shas: &[String],
        edit: impl Fn(&str, &mut Vec<RebaseEntry>),
    ) -> Result<RewriteResult> {
        check_shas(shas)?;
        let oldest = self.oldest_of(shas)?;
        let (base, commits) = self.rebase_range(&oldest)?;
        let mut entries: Vec<RebaseEntry> = commits
            .iter()
            .map(|c| RebaseEntry {
                sha: c.sha.clone(),
                action: RebaseAction::Pick,
                message: None,
            })
            .collect();
        for s in shas {
            let full = self.full_sha(s)?;
            if !entries.iter().any(|e| e.sha == full) {
                return Err(other("The selected commit is not in the current branch"));
            }
            edit(&full, &mut entries);
        }
        self.run_rebase(base.as_deref(), &entries)
    }

    /// Edit Commit Message…: amend HEAD, or reword an older commit through a rebase.
    pub fn reword(&self, sha: &str, message: &str) -> Result<RewriteResult> {
        if message.trim().is_empty() {
            return Err(other("the commit message is empty"));
        }
        let full = self.full_sha(sha)?;
        let old_head = self.head_sha()?;
        if full == old_head {
            self.git_stdin(
                &[
                    "commit",
                    "-q",
                    "--amend",
                    "--only",
                    "--allow-empty",
                    "--no-verify",
                    "--cleanup=whitespace",
                    "-F",
                    "-",
                ],
                message,
            )?;
            return Ok(RewriteResult {
                output: String::new(),
                old_head,
                new_head: Some(self.head_sha()?),
                stopped: false,
            });
        }
        let message = message.to_string();
        self.rewrite_selected(&[full], |sha, entries| {
            if let Some(e) = entries.iter_mut().find(|e| e.sha == sha) {
                e.action = RebaseAction::Reword;
                e.message = Some(message.clone());
            }
        })
    }

    /// Squash Commits…: the selected commits become one commit with `message`, at the place
    /// of the oldest of them.
    pub fn squash_commits(&self, shas: &[String], message: &str) -> Result<RewriteResult> {
        if shas.len() < 2 {
            return Err(other("select at least two commits to squash"));
        }
        if message.trim().is_empty() {
            return Err(other("the commit message is empty"));
        }
        let oldest = self.oldest_of(shas)?;
        let selected: Vec<String> = shas
            .iter()
            .map(|s| self.full_sha(s))
            .collect::<Result<_>>()?;
        let message = message.to_string();
        self.rewrite_selected(&[oldest.clone()], move |_, entries| {
            let (mut moved, rest): (Vec<RebaseEntry>, Vec<RebaseEntry>) = entries
                .drain(..)
                .partition(|e| selected.contains(&e.sha) && e.sha != oldest);
            let at = rest
                .iter()
                .position(|e| e.sha == oldest)
                .map_or(0, |p| p + 1);
            let last = moved.len().saturating_sub(1);
            for (i, e) in moved.iter_mut().enumerate() {
                e.action = RebaseAction::Squash;
                e.message = (i == last).then(|| message.clone());
            }
            *entries = rest;
            entries.splice(at..at, moved);
        })
    }

    /// Drop Commits.
    pub fn drop_commits(&self, shas: &[String]) -> Result<RewriteResult> {
        self.rewrite_selected(shas, |sha, entries| {
            if let Some(e) = entries.iter_mut().find(|e| e.sha == sha) {
                e.action = RebaseAction::Drop;
            }
        })
    }

    // ---- patches and trees ----------------------------------------------------------------------

    /// Create Patch…: the changes of the selected commits (from the parent of the oldest to
    /// the newest) as a unified diff with workspace paths.
    pub fn commits_patch(&self, shas: &[String]) -> Result<String> {
        check_shas(shas)?;
        let full: Vec<String> = shas
            .iter()
            .map(|s| self.full_sha(s))
            .collect::<Result<_>>()?;
        // Oldest: the one that is an ancestor of all others; newest: the one all are ancestors of.
        let oldest = full
            .iter()
            .find(|a| full.iter().all(|b| *a == b || self.is_ancestor(a, b)))
            .ok_or_else(|| other("the selected commits are not on one line of history"))?;
        let newest = full
            .iter()
            .find(|a| full.iter().all(|b| *a == b || self.is_ancestor(b, a)))
            .ok_or_else(|| other("the selected commits are not on one line of history"))?;
        let parent = format!("{oldest}^");
        let base = match self.resolve_commit(&parent) {
            Some(p) => p,
            // The empty tree, in the repository's hash format.
            None => self
                .git_stdin(&["hash-object", "-t", "tree", "--stdin"], "")?
                .trim()
                .to_string(),
        };
        let spec = if self.prefix.is_empty() {
            ".".to_string()
        } else {
            self.prefix.clone()
        };
        let out = self.git(&[
            "diff",
            "--no-ext-diff",
            "--no-color",
            "--binary",
            "-M",
            &base,
            newest,
            "--",
            &spec,
        ])?;
        Ok(self.strip_diff_prefix(out))
    }

    /// Show Repository at Revision: the files of the workspace in `rev`.
    pub fn tree_files(&self, rev: &str) -> Result<Vec<String>> {
        super::check_rev(rev)?;
        let spec = if self.prefix.is_empty() {
            ".".to_string()
        } else {
            self.prefix.clone()
        };
        Ok(self
            .git(&["ls-tree", "-r", "-z", "--name-only", rev, "--", &spec])?
            .split('\0')
            .filter(|p| !p.is_empty() && self.in_workspace(p))
            .map(|p| self.repo_to_ws(p))
            .collect())
    }
}
