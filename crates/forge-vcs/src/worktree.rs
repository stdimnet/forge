//! Git worktrees (`git worktree list --porcelain`) and the options of "New Worktree…".

use serde::{Deserialize, Serialize};

/// One worktree of the repository.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    /// Absolute path (on the project's machine).
    pub path: String,
    /// Checked out commit; `None` for a bare repository.
    pub head: Option<String>,
    /// Checked out branch (short name); `None` when detached or bare.
    pub branch: Option<String>,
    pub detached: bool,
    pub bare: bool,
    /// The repository's main worktree (listed first by git).
    pub main: bool,
    /// The worktree this project is in.
    pub current: bool,
    pub locked: bool,
    pub lock_reason: Option<String>,
    /// Its directory is gone; `git worktree prune` removes it.
    pub prunable: bool,
}

/// Parse `git worktree list --porcelain` (records separated by blank lines).
pub fn parse_worktrees(out: &str) -> Vec<Worktree> {
    let mut list: Vec<Worktree> = Vec::new();
    let mut cur: Option<Worktree> = None;
    for line in out.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if let Some(w) = cur.take() {
                list.push(w);
            }
            continue;
        }
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        if key == "worktree" {
            if let Some(w) = cur.take() {
                list.push(w);
            }
            cur = Some(Worktree {
                path: value.to_string(),
                ..Worktree::default()
            });
            continue;
        }
        let Some(w) = cur.as_mut() else { continue };
        match key {
            "HEAD" => w.head = Some(value.to_string()),
            "branch" => {
                w.branch = Some(
                    value
                        .strip_prefix("refs/heads/")
                        .unwrap_or(value)
                        .to_string(),
                )
            }
            "detached" => w.detached = true,
            "bare" => w.bare = true,
            "locked" => {
                w.locked = true;
                w.lock_reason = Some(value.trim().to_string()).filter(|r| !r.is_empty());
            }
            "prunable" => w.prunable = true,
            _ => {}
        }
    }
    if let Some(first) = list.first_mut() {
        first.main = true;
    }
    list
}

/// What "New Worktree…" creates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewWorktree {
    /// Directory of the new worktree (absolute, or relative to the repository root).
    pub path: String,
    /// Existing branch to check out.
    pub branch: Option<String>,
    /// New branch to create (`-b`); `base` is its start point.
    pub new_branch: Option<String>,
    /// Start point of a new branch, or the commit to check out detached.
    pub base: Option<String>,
    /// Check out `base` without a branch.
    pub detach: bool,
}

impl NewWorktree {
    /// Arguments after `git`: `worktree add [-b <new>|--detach] <path> [<commit-ish>]`.
    pub fn args(&self) -> Result<Vec<String>, String> {
        let path = self.path.trim();
        if path.is_empty() {
            return Err("Directory is empty".into());
        }
        let opt = |v: &Option<String>| -> Result<Option<String>, String> {
            match v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                Some(s) if s.starts_with('-') || s.contains(['\0', '\n']) => {
                    Err(format!("invalid revision: {s:?}"))
                }
                other => Ok(other.map(str::to_string)),
            }
        };
        if path.starts_with('-') || path.contains(['\0', '\n']) {
            return Err(format!("invalid directory: {path:?}"));
        }
        let branch = opt(&self.branch)?;
        let new_branch = opt(&self.new_branch)?;
        let base = opt(&self.base)?;
        let mut args = vec!["worktree".to_string(), "add".to_string()];
        match (new_branch, branch) {
            (Some(new), _) => {
                args.extend(["-b".into(), new, path.into()]);
                args.extend(base);
            }
            (None, Some(branch)) if !self.detach => args.extend([path.into(), branch]),
            (None, existing) => {
                let commit = base
                    .or(existing)
                    .ok_or_else(|| "Choose a branch or a commit".to_string())?;
                args.extend(["--detach".into(), path.into(), commit]);
            }
        }
        Ok(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_list() {
        let out = "worktree /src/shop\nHEAD aaa\nbranch refs/heads/main\n\n\
worktree /src/shop-feature\nHEAD bbb\nbranch refs/heads/feature/x\nlocked on a USB drive\n\n\
worktree /tmp/gone\nHEAD ccc\ndetached\nprunable gitdir file points to non-existent location\n\n";
        let w = parse_worktrees(out);
        assert_eq!(w.len(), 3);
        assert!(w[0].main && !w[1].main);
        assert_eq!(w[0].branch.as_deref(), Some("main"));
        assert_eq!(w[1].branch.as_deref(), Some("feature/x"));
        assert!(w[1].locked);
        assert_eq!(w[1].lock_reason.as_deref(), Some("on a USB drive"));
        assert!(w[2].detached && w[2].prunable && w[2].branch.is_none());
        assert_eq!(w[2].head.as_deref(), Some("ccc"));

        let bare = parse_worktrees("worktree /srv/shop.git\nbare\n\nworktree /src/a\nHEAD a\nbranch refs/heads/a\nlocked\n");
        assert!(bare[0].bare && bare[0].head.is_none());
        assert!(bare[1].locked && bare[1].lock_reason.is_none());
    }

    #[test]
    fn builds_add_arguments() {
        let new = NewWorktree {
            path: "/src/shop-x".into(),
            new_branch: Some("feature/x".into()),
            base: Some("main".into()),
            ..NewWorktree::default()
        };
        assert_eq!(
            new.args().unwrap(),
            vec!["worktree", "add", "-b", "feature/x", "/src/shop-x", "main"]
        );
        let existing = NewWorktree {
            path: "../shop-y".into(),
            branch: Some("feature/y".into()),
            ..NewWorktree::default()
        };
        assert_eq!(
            existing.args().unwrap(),
            vec!["worktree", "add", "../shop-y", "feature/y"]
        );
        let detached = NewWorktree {
            path: "/w".into(),
            base: Some("v1.0".into()),
            detach: true,
            ..NewWorktree::default()
        };
        assert_eq!(
            detached.args().unwrap(),
            vec!["worktree", "add", "--detach", "/w", "v1.0"]
        );
        assert!(NewWorktree::default().args().is_err());
        let bad = NewWorktree {
            path: "/w".into(),
            branch: Some("--force".into()),
            ..NewWorktree::default()
        };
        assert!(bad.args().is_err());
    }
}
