//! Branches and tags from `git for-each-ref`, plus recently checked out branches from the
//! reflog.

use serde::Serialize;

#[cfg_attr(not(feature = "git"), allow(dead_code))]
pub(crate) const REF_FORMAT: &str = "--format=%(refname)%1f%(objectname)%1f%(upstream:short)%1f%(upstream:track,nobracket)%1f%(committerdate:unix)%1f%(HEAD)%1f%(worktreepath)%1f%(contents:subject)";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfo {
    /// Short name: `main`, `origin/main`, `v1.0`.
    pub name: String,
    pub sha: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Committer time of the tip, seconds since the Unix epoch.
    pub time: i64,
    pub subject: String,
    pub current: bool,
    /// Absolute path of another worktree that has this branch checked out (`None` when it
    /// is checked out nowhere or in this worktree).
    pub worktree: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Branches {
    /// Current branch, `None` when HEAD is detached.
    pub current: Option<String>,
    pub local: Vec<BranchInfo>,
    pub remote: Vec<BranchInfo>,
    pub tags: Vec<BranchInfo>,
    /// Recently checked out local branches, most recent first (excluding the current one).
    pub recent: Vec<String>,
}

pub fn parse_refs(out: &str) -> Branches {
    let mut b = Branches::default();
    for line in out.lines().filter(|l| !l.is_empty()) {
        let f: Vec<&str> = line.splitn(8, '\x1f').collect();
        if f.len() < 8 {
            continue;
        }
        let current = f[5] == "*";
        let (ahead, behind) = parse_track(f[3]);
        let mut info = BranchInfo {
            name: String::new(),
            sha: f[1].to_string(),
            upstream: (!f[2].is_empty()).then(|| f[2].to_string()),
            ahead,
            behind,
            time: f[4].parse().unwrap_or(0),
            subject: f[7].to_string(),
            current,
            worktree: (!current && !f[6].is_empty()).then(|| f[6].to_string()),
        };
        if let Some(n) = f[0].strip_prefix("refs/heads/") {
            info.name = n.to_string();
            if info.current {
                b.current = Some(info.name.clone());
            }
            b.local.push(info);
        } else if let Some(n) = f[0].strip_prefix("refs/remotes/") {
            if n.ends_with("/HEAD") {
                continue;
            }
            info.name = n.to_string();
            b.remote.push(info);
        } else if let Some(n) = f[0].strip_prefix("refs/tags/") {
            info.name = n.to_string();
            b.tags.push(info);
        }
    }
    b
}

/// `ahead 2, behind 1` / `ahead 3` / `gone` / empty.
fn parse_track(s: &str) -> (u32, u32) {
    let (mut ahead, mut behind) = (0, 0);
    for part in s.split(", ") {
        if let Some(n) = part.strip_prefix("ahead ") {
            ahead = n.trim().parse().unwrap_or(0);
        } else if let Some(n) = part.strip_prefix("behind ") {
            behind = n.trim().parse().unwrap_or(0);
        }
    }
    (ahead, behind)
}

/// Branch names from `git reflog --format=%gs` lines like
/// `checkout: moving from main to feature/x`, most recent first, deduplicated.
pub fn parse_recent_checkouts(
    out: &str,
    local: &[BranchInfo],
    current: Option<&str>,
) -> Vec<String> {
    let mut recent: Vec<String> = Vec::new();
    for line in out.lines() {
        let Some(rest) = line.strip_prefix("checkout: moving from ") else {
            continue;
        };
        let Some((from, to)) = rest.rsplit_once(" to ") else {
            continue;
        };
        for name in [to, from] {
            let known = local.iter().any(|b| b.name == name);
            if known && Some(name) != current && !recent.iter().any(|r| r == name) {
                recent.push(name.to_string());
            }
        }
        if recent.len() >= 5 {
            break;
        }
    }
    recent.truncate(5);
    recent
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_refs() {
        let out = "refs/heads/main\x1fabc\x1forigin/main\x1fahead 2, behind 1\x1f1700000000\x1f*\x1f/src/shop\x1fFix bug\n\
                   refs/heads/feature/x\x1fdef\x1f\x1f\x1f1690000000\x1f \x1f/src/shop-x\x1fWIP\n\
                   refs/remotes/origin/HEAD\x1fabc\x1f\x1f\x1f1700000000\x1f \x1f\x1fFix bug\n\
                   refs/remotes/origin/main\x1fabc\x1f\x1f\x1f1700000000\x1f \x1f\x1fFix bug\n\
                   refs/tags/v1.0\x1fabc\x1f\x1f\x1f1700000000\x1f \x1f\x1fRelease\n";
        let b = parse_refs(out);
        assert_eq!(b.current.as_deref(), Some("main"));
        assert_eq!(b.local.len(), 2);
        assert_eq!((b.local[0].ahead, b.local[0].behind), (2, 1));
        assert_eq!(b.local[0].upstream.as_deref(), Some("origin/main"));
        assert_eq!(b.local[1].upstream, None);
        // Checked out here: no other worktree; checked out elsewhere: that worktree.
        assert_eq!(b.local[0].worktree, None);
        assert_eq!(b.local[1].worktree.as_deref(), Some("/src/shop-x"));
        assert_eq!(b.remote.len(), 1);
        assert_eq!(b.remote[0].name, "origin/main");
        assert_eq!(b.tags[0].name, "v1.0");
    }

    #[test]
    fn recent_checkouts() {
        let local: Vec<BranchInfo> = ["main", "dev", "feature/x"]
            .iter()
            .map(|n| BranchInfo {
                name: n.to_string(),
                sha: String::new(),
                upstream: None,
                ahead: 0,
                behind: 0,
                time: 0,
                subject: String::new(),
                current: false,
                worktree: None,
            })
            .collect();
        let out = "checkout: moving from dev to main\ncommit: x\ncheckout: moving from feature/x to dev\ncheckout: moving from main to 1234abc\n";
        let recent = parse_recent_checkouts(out, &local, Some("main"));
        assert_eq!(recent, vec!["dev", "feature/x"]);
    }
}
