//! Parsing and text helpers for the repository management actions of the Git menu:
//! stashes (`git stash list`), remotes (`git remote -v`) and `.gitignore` entries.

use serde::Serialize;

/// Format for `git stash list`: reflog selector, committer time and reflog subject.
pub(crate) const STASH_FORMAT: &str = "--format=%gd%x1f%ct%x1f%gs";

/// One entry of `git stash list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StashEntry {
    /// Position in the stash list: `0` is `stash@{0}`, the most recent.
    pub index: usize,
    /// Branch the changes were stashed on, if git recorded one.
    pub branch: Option<String>,
    /// The stash message (`WIP on …` stashes report the commit subject).
    pub message: String,
    /// Seconds since the Unix epoch.
    pub time: i64,
}

pub fn parse_stash_list(out: &str) -> Vec<StashEntry> {
    out.lines()
        .filter_map(|line| {
            let mut f = line.splitn(3, '\x1f');
            let selector = f.next()?;
            let time = f.next()?.trim().parse().unwrap_or(0);
            let subject = f.next().unwrap_or_default();
            let index = selector
                .strip_prefix("stash@{")?
                .strip_suffix('}')?
                .parse()
                .ok()?;
            let (branch, message) = split_stash_subject(subject);
            Some(StashEntry {
                index,
                branch,
                message,
                time,
            })
        })
        .collect()
}

/// `On main: message` / `WIP on main: 1a2b3c4 subject` into branch and message.
fn split_stash_subject(subject: &str) -> (Option<String>, String) {
    let rest = subject
        .strip_prefix("WIP on ")
        .or_else(|| subject.strip_prefix("On "));
    match rest.and_then(|r| r.split_once(": ")) {
        Some((branch, msg)) if branch != "(no branch)" => (Some(branch.to_string()), msg.into()),
        Some((_, msg)) => (None, msg.to_string()),
        None => (None, subject.to_string()),
    }
}

/// A configured remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    pub name: String,
    pub fetch_url: String,
    /// Only set when it differs from the fetch URL.
    pub push_url: Option<String>,
}

/// Parse `git remote -v` (`name<TAB>url (fetch|push)` lines), keeping git's order.
pub fn parse_remotes(out: &str) -> Vec<Remote> {
    let mut remotes: Vec<Remote> = Vec::new();
    for line in out.lines() {
        let Some((name, rest)) = line.split_once('\t') else {
            continue;
        };
        let (url, kind) = match rest.rsplit_once(' ') {
            Some((url, kind)) => (url, kind),
            None => (rest, "(fetch)"),
        };
        let at = match remotes.iter().position(|r| r.name == name) {
            Some(i) => i,
            None => {
                remotes.push(Remote {
                    name: name.to_string(),
                    fetch_url: String::new(),
                    push_url: None,
                });
                remotes.len() - 1
            }
        };
        let r = &mut remotes[at];
        if kind == "(push)" {
            r.push_url = Some(url.to_string());
        } else {
            r.fetch_url = url.to_string();
        }
    }
    for r in &mut remotes {
        if r.push_url.as_deref() == Some(r.fetch_url.as_str()) {
            r.push_url = None;
        }
    }
    remotes
}

/// A `.gitignore` line that matches exactly `path` (relative to the `.gitignore`), anchored
/// with a leading `/` and with glob characters escaped. Directories get a trailing `/`.
pub fn ignore_pattern(path: &str, dir: bool) -> String {
    let mut out = String::from("/");
    for c in path.trim_matches('/').chars() {
        if matches!(c, '\\' | '*' | '?' | '[') {
            out.push('\\');
        }
        out.push(c);
    }
    if out.ends_with(' ') {
        out.insert(out.len() - 1, '\\');
    }
    if dir {
        out.push('/');
    }
    out
}

/// `existing` with `patterns` appended, one per line, skipping lines it already has.
/// `None` when nothing needs to be added.
pub fn append_ignore_patterns(existing: &str, patterns: &[String]) -> Option<String> {
    let have: Vec<&str> = existing.lines().map(str::trim_end).collect();
    let mut added: Vec<&str> = Vec::new();
    for p in patterns {
        if !have.contains(&p.as_str()) && !added.contains(&p.as_str()) {
            added.push(p);
        }
    }
    if added.is_empty() {
        return None;
    }
    let mut out = existing.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    for p in added {
        out.push_str(p);
        out.push('\n');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stash_list_with_and_without_branch() {
        let out = "stash@{0}\x1f1700000100\x1fOn main: half-done checkout\n\
                   stash@{1}\x1f1700000000\x1fWIP on feature/x: 1a2b3c4 Add totals\n\
                   stash@{2}\x1f1690000000\x1fWIP on (no branch): 1a2b3c4 Detached\n";
        let list = parse_stash_list(out);
        assert_eq!(list.len(), 3);
        assert_eq!(
            list[0],
            StashEntry {
                index: 0,
                branch: Some("main".into()),
                message: "half-done checkout".into(),
                time: 1_700_000_100,
            }
        );
        assert_eq!(list[1].branch.as_deref(), Some("feature/x"));
        assert_eq!(list[1].message, "1a2b3c4 Add totals");
        assert_eq!(list[2].branch, None);
        assert!(parse_stash_list("").is_empty());
    }

    #[test]
    fn parses_remotes_and_folds_equal_push_urls() {
        let out = "origin\tgit@github.com:acme/shop.git (fetch)\n\
                   origin\tgit@github.com:acme/shop.git (push)\n\
                   mirror\thttps://example.com/a b.git (fetch)\n\
                   mirror\tssh://push.example.com/a.git (push)\n";
        let r = parse_remotes(out);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].name, "origin");
        assert_eq!(r[0].push_url, None);
        assert_eq!(r[1].fetch_url, "https://example.com/a b.git");
        assert_eq!(
            r[1].push_url.as_deref(),
            Some("ssh://push.example.com/a.git")
        );
    }

    #[test]
    fn ignore_patterns_are_anchored_and_escaped() {
        assert_eq!(
            ignore_pattern("storage/debug.log", false),
            "/storage/debug.log"
        );
        assert_eq!(ignore_pattern("build", true), "/build/");
        assert_eq!(ignore_pattern("a[1]*?.txt", false), "/a\\[1]\\*\\?.txt");
        assert_eq!(ignore_pattern("trailing ", false), "/trailing\\ ");
    }

    #[test]
    fn appends_only_missing_patterns() {
        let existing = "/vendor/\n.env";
        let out = append_ignore_patterns(
            existing,
            &["/vendor/".into(), "/x.log".into(), "/x.log".into()],
        );
        assert_eq!(out.as_deref(), Some("/vendor/\n.env\n/x.log\n"));
        assert_eq!(append_ignore_patterns("/a\n", &["/a".into()]), None);
        assert_eq!(
            append_ignore_patterns("", &["/a".into()]).as_deref(),
            Some("/a\n")
        );
    }
}
