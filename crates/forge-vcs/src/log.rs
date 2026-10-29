//! Commit history: parser for a NUL/RS separated `git log` format, ref decorations and
//! per-commit changed files.

use serde::{Deserialize, Serialize};

use crate::signing::SignatureStatus;

/// Field separator (unit separator) and record separator used in the log format.
pub const LOG_FORMAT: &str = "--format=%H%x1f%P%x1f%D%x1f%an%x1f%ae%x1f%at%x1f%G?%x1f%s%x1e";
/// `git show -s` format of a commit's details (author, committer, signature, message).
pub const COMMIT_META_FORMAT: &str =
    "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%cn%x1f%ct%x1f%G?%x1f%GS%x1f%GK%x1f%B";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RefKind {
    /// `HEAD` itself (detached) or the branch HEAD points to.
    Head,
    Local,
    Remote,
    Tag,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefLabel {
    pub name: String,
    pub kind: RefKind,
    /// True for the branch HEAD currently points to.
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub sha: String,
    pub parents: Vec<String>,
    pub refs: Vec<RefLabel>,
    pub author: String,
    pub email: String,
    /// Author time, seconds since the Unix epoch.
    pub time: i64,
    pub subject: String,
    /// GPG/SSH signature status (`%G?`).
    #[serde(skip_serializing_if = "SignatureStatus::is_none")]
    pub signature: SignatureStatus,
}

/// What to list in the log, with paging.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LogQuery {
    pub skip: usize,
    /// Page size; 0 means a default of 200.
    pub limit: usize,
    /// Show all refs (like the Git tool window's default) instead of HEAD only.
    pub all: bool,
    /// Restrict to one branch/revision.
    pub rev: Option<String>,
    /// Restrict to commits touching this workspace-relative path.
    pub path: Option<String>,
    /// Case-insensitive search in commit messages.
    pub text: Option<String>,
    pub author: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    pub orig_path: Option<String>,
    /// Single-letter git status: A, M, D, R, C, T.
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitDetails {
    pub sha: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub committer: String,
    pub commit_time: i64,
    /// Full message (subject + body).
    pub message: String,
    pub files: Vec<ChangedFile>,
    /// GPG/SSH signature status (`%G?`).
    pub signature: SignatureStatus,
    /// The signer as the signature names it (`%GS`), e.g. `Marlon <marlon@acme.dev>`.
    pub signer: Option<String>,
    /// The key that made the signature (`%GK`).
    pub signing_key: Option<String>,
}

pub fn parse_log(out: &str) -> Vec<Commit> {
    out.split('\x1e')
        .map(|r| r.trim_start_matches('\n'))
        .filter(|r| !r.is_empty())
        .filter_map(|rec| {
            let f: Vec<&str> = rec.splitn(8, '\x1f').collect();
            if f.len() < 8 {
                return None;
            }
            Some(Commit {
                sha: f[0].to_string(),
                parents: f[1].split_whitespace().map(str::to_string).collect(),
                refs: parse_decorations(f[2]),
                author: f[3].to_string(),
                email: f[4].to_string(),
                time: f[5].trim().parse().unwrap_or(0),
                signature: SignatureStatus::from_code(f[6]),
                subject: f[7].trim_end_matches('\n').to_string(),
            })
        })
        .collect()
}

/// Parse `%D` output produced with `--decorate=full`, e.g.
/// `HEAD -> refs/heads/main, refs/remotes/origin/main, tag: refs/tags/v1.0`.
pub fn parse_decorations(d: &str) -> Vec<RefLabel> {
    let mut refs = Vec::new();
    for part in d.split(", ").map(str::trim).filter(|p| !p.is_empty()) {
        if let Some(target) = part.strip_prefix("HEAD -> ") {
            if let Some(mut r) = classify(target) {
                r.current = true;
                refs.push(r);
            }
        } else if part == "HEAD" {
            refs.push(RefLabel {
                name: "HEAD".into(),
                kind: RefKind::Head,
                current: true,
            });
        } else if let Some(tag) = part.strip_prefix("tag: ") {
            refs.push(RefLabel {
                name: tag.trim_start_matches("refs/tags/").to_string(),
                kind: RefKind::Tag,
                current: false,
            });
        } else if let Some(r) = classify(part) {
            refs.push(r);
        }
    }
    refs
}

fn classify(full: &str) -> Option<RefLabel> {
    let (name, kind) = if let Some(n) = full.strip_prefix("refs/heads/") {
        (n, RefKind::Local)
    } else if let Some(n) = full.strip_prefix("refs/remotes/") {
        if n.ends_with("/HEAD") {
            return None;
        }
        (n, RefKind::Remote)
    } else if let Some(n) = full.strip_prefix("refs/tags/") {
        (n, RefKind::Tag)
    } else if full.starts_with("refs/stash") {
        return None;
    } else {
        (full, RefKind::Local)
    };
    Some(RefLabel {
        name: name.to_string(),
        kind,
        current: false,
    })
}

/// A full object id: 40 (SHA-1) or 64 (SHA-256) hex digits.
pub fn is_object_id(token: &str) -> bool {
    matches!(token.len(), 40 | 64) && token.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Parse `git diff-tree -r -z --name-status -M` output.
pub fn parse_name_status(out: &str) -> Vec<ChangedFile> {
    let mut files = Vec::new();
    let mut it = out.split('\0').filter(|s| !s.is_empty()).peekable();
    // `diff-tree --root <commit>` starts with the commit id.
    if it.peek().is_some_and(|t| is_object_id(t)) {
        it.next();
    }
    while let Some(status) = it.next() {
        let letter = status.chars().next().unwrap_or('M');
        if letter == 'R' || letter == 'C' {
            let (Some(from), Some(to)) = (it.next(), it.next()) else {
                break;
            };
            files.push(ChangedFile {
                path: to.to_string(),
                orig_path: Some(from.to_string()),
                status: letter.to_string(),
            });
        } else {
            let Some(path) = it.next() else { break };
            files.push(ChangedFile {
                path: path.to_string(),
                orig_path: None,
                status: letter.to_string(),
            });
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_log_records() {
        let out = "aaa\x1fbbb ccc\x1fHEAD -> refs/heads/main, refs/remotes/origin/main, tag: refs/tags/v1.0\x1fMarlon\x1fm@x.io\x1f1700000000\x1fG\x1fMerge branch 'x'\x1e\n\
                   bbb\x1f\x1f\x1fAnna\x1fa@x.io\x1f1690000000\x1fN\x1fInitial commit\x1e\n";
        let log = parse_log(out);
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].parents, vec!["bbb", "ccc"]);
        assert_eq!(log[0].refs.len(), 3);
        assert_eq!(log[0].refs[0].name, "main");
        assert!(log[0].refs[0].current);
        assert_eq!(log[0].refs[1].kind, RefKind::Remote);
        assert_eq!(log[0].refs[1].name, "origin/main");
        assert_eq!(log[0].refs[2].kind, RefKind::Tag);
        assert_eq!(log[0].refs[2].name, "v1.0");
        assert!(log[1].parents.is_empty());
        assert_eq!(log[1].time, 1_690_000_000);
        assert_eq!(log[1].subject, "Initial commit");
        assert_eq!(log[0].signature, SignatureStatus::Good);
        assert_eq!(log[1].signature, SignatureStatus::None);
    }

    #[test]
    fn subject_may_contain_separators_of_other_formats() {
        let out = "a\x1f\x1f\x1fX\x1fx@x\x1f1\x1fN\x1ffix: a, b -> c\x1e";
        assert_eq!(parse_log(out)[0].subject, "fix: a, b -> c");
    }

    #[test]
    fn skips_remote_head_and_detached() {
        let refs = parse_decorations("HEAD, refs/remotes/origin/HEAD, refs/heads/dev");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].kind, RefKind::Head);
        assert_eq!(refs[1].name, "dev");
    }

    #[test]
    fn parses_name_status() {
        let files = parse_name_status("M\0a.php\0R087\0old.php\0new.php\0A\0b c.php\0");
        assert_eq!(files.len(), 3);
        assert_eq!(files[1].status, "R");
        assert_eq!(files[1].orig_path.as_deref(), Some("old.php"));
        assert_eq!(files[2].path, "b c.php");
    }
}
