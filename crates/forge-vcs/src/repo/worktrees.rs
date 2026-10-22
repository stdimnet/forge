//! Worktrees (Git tool window › Worktrees), the user's own commit messages (Commit Message
//! History) and the signing setup from git config (read-only).

use super::{check_rev, Repo};
use crate::message_history::parse_messages;
use crate::signing::GitSigningConfig;
use crate::worktree::{parse_worktrees, NewWorktree, Worktree};
use forge_core::{ForgeError, Result};

fn check_path(path: &str) -> Result<&str> {
    let path = path.trim();
    if path.is_empty() || path.starts_with('-') || path.contains(['\0', '\n']) {
        return Err(ForgeError::Other(format!(
            "invalid worktree path: {path:?}"
        )));
    }
    Ok(path)
}

impl Repo {
    /// The repository's worktrees, the main one first; `current` marks this project's.
    pub fn worktrees(&self) -> Result<Vec<Worktree>> {
        let mut list = parse_worktrees(&self.git(&["worktree", "list", "--porcelain"])?);
        let root = self.root.to_string_lossy();
        let same = |a: &str, b: &str| {
            a.trim_end_matches(['/', '\\']) == b.trim_end_matches(['/', '\\'])
                || (cfg!(windows) && a.eq_ignore_ascii_case(b))
        };
        for w in &mut list {
            w.current = same(&w.path, &root);
        }
        Ok(list)
    }

    /// "New Worktree…": create it with a new or existing branch (or detached).
    pub fn add_worktree(&self, new: &NewWorktree) -> Result<String> {
        let args = new.args().map_err(ForgeError::Other)?;
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = super::run_raw(self.exec.as_ref(), &self.root, &refs, super::Run::default())?;
        if !out.ok {
            return Err(super::cmd::error_from(&refs, &out));
        }
        Ok(format!("{}{}", out.stdout, out.stderr).trim().to_string())
    }

    /// Remove a worktree; `force` also discards its local changes.
    pub fn remove_worktree(&self, path: &str, force: bool) -> Result<()> {
        let path = check_path(path)?;
        let mut args = vec!["worktree", "remove"];
        if force {
            args.push("--force");
        }
        args.push(path);
        self.git(&args).map(drop)
    }

    /// Forget worktrees whose directories are gone. Returns git's report.
    pub fn prune_worktrees(&self) -> Result<String> {
        Ok(self.git(&["worktree", "prune", "-v"])?.trim().to_string())
    }

    pub fn lock_worktree(&self, path: &str, reason: Option<&str>) -> Result<()> {
        let path = check_path(path)?;
        let mut args = vec!["worktree", "lock"];
        let reason = reason.map(str::trim).filter(|r| !r.is_empty());
        if let Some(r) = reason {
            args.extend(["--reason", r]);
        }
        args.push(path);
        self.git(&args).map(drop)
    }

    pub fn unlock_worktree(&self, path: &str) -> Result<()> {
        let path = check_path(path)?;
        self.git(&["worktree", "unlock", path]).map(drop)
    }

    /// Recent messages of commits by the current user (the author git would record now:
    /// `git var GIT_AUTHOR_IDENT`, i.e. `user.email` or `GIT_AUTHOR_EMAIL`), newest first.
    pub fn own_commit_messages(&self, limit: usize) -> Result<Vec<String>> {
        if !self.has_head() {
            return Ok(Vec::new());
        }
        let ident = self.git(&["var", "GIT_AUTHOR_IDENT"]).unwrap_or_default();
        let Some(email) = ident
            .split_once('<')
            .and_then(|(_, rest)| rest.split_once('>'))
            .map(|(email, _)| email.trim().to_string())
            .filter(|e| !e.is_empty())
        else {
            return Ok(Vec::new());
        };
        check_rev(&email)?;
        let author = format!("--author=<{email}>");
        let count = format!("--max-count={}", limit.max(1));
        let out = self.git(&[
            "log",
            "--no-merges",
            "--fixed-strings",
            &author,
            &count,
            "--format=%B%x1e",
            "HEAD",
            "--",
        ])?;
        Ok(parse_messages(&out))
    }

    /// The signing setup in git config (`user.signingkey`, `gpg.format`, `commit.gpgsign`).
    /// Only read, never changed.
    pub fn signing_config(&self) -> GitSigningConfig {
        GitSigningConfig {
            signing_key: self.config_value("user.signingkey"),
            format: self.config_value("gpg.format"),
            gpg_sign: self.config_value("commit.gpgsign").is_some_and(|v| {
                matches!(v.to_ascii_lowercase().as_str(), "true" | "yes" | "on" | "1")
            }),
        }
    }
}
