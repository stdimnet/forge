//! File, folder and selection history (PHPStorm's Show History and Show History for
//! Selection), blame of a past revision (Annotate Revision) and the files that differ
//! between a revision and the working tree (Compare with Branch for folders).

use forge_core::{ForgeError, Result};
use serde::{Deserialize, Serialize};

use crate::blame::{parse_blame, Blame};
use crate::log::{parse_name_status, ChangedFile};
use crate::{check_rev, Repo};

/// Header of one commit in history output: a record separator, then the fields separated
/// by unit separators, then the per-commit part (name-status or patch).
const HISTORY_FORMAT: &str = "--format=%x1e%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%s%x1f";

/// Paging of a history listing.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HistoryQuery {
    pub skip: usize,
    /// Page size; 0 means a default of 200.
    pub limit: usize,
}

/// One revision in the history of a file or folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRevision {
    pub sha: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    /// Author time, seconds since the Unix epoch.
    pub time: i64,
    pub subject: String,
    /// What the commit changed below the path (workspace-relative). For a file this is the
    /// file itself under its name in that revision, so renames are followed.
    pub files: Vec<ChangedFile>,
}

/// One revision of a block of lines (Show History for Selection).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockRevision {
    pub sha: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub time: i64,
    pub subject: String,
    /// The file's workspace-relative path in this revision.
    pub path: String,
    /// The block before the commit (`None` when the commit added it).
    pub before: Option<String>,
    /// The block after the commit.
    pub after: String,
    /// First line (1-based) of the block after the commit.
    pub start_line: u32,
}

struct Header {
    sha: String,
    parents: Vec<String>,
    author: String,
    email: String,
    time: i64,
    subject: String,
}

/// Split history output into commit headers and the text that follows each header.
fn records(out: &str) -> Vec<(Header, &str)> {
    out.split('\x1e')
        .filter(|r| !r.trim().is_empty())
        .filter_map(|rec| {
            let f: Vec<&str> = rec.splitn(7, '\x1f').collect();
            if f.len() < 7 {
                return None;
            }
            Some((
                Header {
                    sha: f[0].trim().to_string(),
                    parents: f[1].split_whitespace().map(str::to_string).collect(),
                    author: f[2].to_string(),
                    email: f[3].to_string(),
                    time: f[4].trim().parse().unwrap_or(0),
                    subject: f[5].to_string(),
                },
                f[6],
            ))
        })
        .collect()
}

/// Parse `git log -z --name-status` output written with [`HISTORY_FORMAT`].
pub fn parse_file_history(out: &str) -> Vec<FileRevision> {
    records(out)
        .into_iter()
        .map(|(h, rest)| FileRevision {
            sha: h.sha,
            parents: h.parents,
            author: h.author,
            email: h.email,
            time: h.time,
            subject: h.subject,
            files: parse_name_status(rest.trim_start_matches(['\0', '\n'])),
        })
        .collect()
}

/// Parse `git log -L` output written with [`HISTORY_FORMAT`]: every commit is followed by
/// the patch of the tracked block.
pub fn parse_block_history(out: &str) -> Vec<BlockRevision> {
    records(out)
        .into_iter()
        .filter_map(|(h, patch)| {
            let mut path = String::new();
            let mut before = String::new();
            let mut after = String::new();
            let mut added = false;
            let mut start_line = 0;
            let mut in_hunk = false;
            for line in patch.lines() {
                if let Some(p) = line.strip_prefix("+++ b/") {
                    path = p.to_string();
                    in_hunk = false;
                } else if line == "--- /dev/null" {
                    added = true;
                } else if line.starts_with("diff --git ") || line.starts_with("--- ") {
                    in_hunk = false;
                } else if let Some(hunk) = line.strip_prefix("@@ ") {
                    in_hunk = true;
                    if start_line == 0 {
                        start_line = hunk
                            .split_whitespace()
                            .find_map(|r| r.strip_prefix('+'))
                            .and_then(|r| r.split(',').next())
                            .and_then(|n| n.parse().ok())
                            .unwrap_or(1);
                    }
                } else if in_hunk {
                    match line.as_bytes().first() {
                        Some(b'+') => push_line(&mut after, &line[1..]),
                        Some(b'-') => push_line(&mut before, &line[1..]),
                        Some(b' ') => {
                            push_line(&mut before, &line[1..]);
                            push_line(&mut after, &line[1..]);
                        }
                        None => {
                            push_line(&mut before, "");
                            push_line(&mut after, "");
                        }
                        _ => {}
                    }
                }
            }
            if path.is_empty() {
                return None;
            }
            Some(BlockRevision {
                sha: h.sha,
                parents: h.parents,
                author: h.author,
                email: h.email,
                time: h.time,
                subject: h.subject,
                path,
                before: (!added).then_some(before),
                after,
                start_line,
            })
        })
        .collect()
}

fn push_line(buf: &mut String, line: &str) {
    buf.push_str(line);
    buf.push('\n');
}

impl Repo {
    /// History of a file (following renames) or of a folder: the commits that changed it,
    /// newest first, with what each commit changed below the path.
    pub fn file_history(
        &self,
        path: &str,
        dir: bool,
        q: &HistoryQuery,
    ) -> Result<Vec<FileRevision>> {
        if !self.has_head() {
            return Ok(Vec::new());
        }
        let limit = if q.limit == 0 { 200 } else { q.limit };
        let spec = if path.is_empty() {
            if self.prefix.is_empty() {
                ".".to_string()
            } else {
                self.prefix.clone()
            }
        } else {
            self.ws_to_repo(path)
        };
        let mut args: Vec<String> = vec![
            "log".into(),
            "-z".into(),
            "--name-status".into(),
            "-M".into(),
            HISTORY_FORMAT.into(),
            format!("--max-count={limit}"),
            format!("--skip={}", q.skip),
        ];
        if !dir {
            args.push("--follow".into());
        }
        args.extend(["HEAD".into(), "--".into(), spec]);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut revs = parse_file_history(&self.git(&refs)?);
        for rev in &mut revs {
            for f in &mut rev.files {
                f.path = self.repo_to_ws(&f.path);
                f.orig_path = f.orig_path.as_deref().map(|p| self.repo_to_ws(p));
            }
        }
        Ok(revs)
    }

    /// History of lines `start..=end` (1-based, as in HEAD) of a file: every commit that
    /// changed them, with the block before and after (`git log -L`).
    pub fn block_history(
        &self,
        path: &str,
        start: u32,
        end: u32,
        limit: usize,
    ) -> Result<Vec<BlockRevision>> {
        if start == 0 || end < start {
            return Err(ForgeError::Other(format!(
                "invalid line range {start}..{end}"
            )));
        }
        if !self.has_head() {
            return Ok(Vec::new());
        }
        let limit = if limit == 0 { 100 } else { limit };
        let range = format!("-L{start},{end}:{}", self.ws_to_repo(path));
        let max = format!("--max-count={limit}");
        let out = self.git(&[
            "log",
            "--no-ext-diff",
            "--no-color",
            HISTORY_FORMAT,
            &max,
            &range,
            "HEAD",
        ])?;
        let mut revs = parse_block_history(&out);
        for r in &mut revs {
            r.path = self.repo_to_ws(&r.path);
        }
        Ok(revs)
    }

    /// Blame of `path` as it was in `rev` (Annotate Revision).
    pub fn blame_at(&self, rev: &str, path: &str) -> Result<Blame> {
        check_rev(rev)?;
        let rp = self.ws_to_repo(path);
        Ok(parse_blame(&self.git(&[
            "blame",
            "--porcelain",
            rev,
            "--",
            &rp,
        ])?))
    }

    /// Files below `path` (a folder, `""` for the workspace) that differ between `rev` and
    /// the working tree (Compare with Branch on a folder). Statuses read from `rev` to the
    /// working tree: `A` exists only locally, `D` only in `rev`.
    pub fn changes_since(&self, rev: &str, path: &str) -> Result<Vec<ChangedFile>> {
        check_rev(rev)?;
        let spec = if path.is_empty() {
            if self.prefix.is_empty() {
                ".".to_string()
            } else {
                self.prefix.clone()
            }
        } else {
            self.ws_to_repo(path)
        };
        let out = self.git(&[
            "diff",
            "--no-ext-diff",
            "-z",
            "--name-status",
            "-M",
            rev,
            "--",
            &spec,
        ])?;
        Ok(parse_name_status(&out)
            .into_iter()
            .map(|mut c| {
                c.path = self.repo_to_ws(&c.path);
                c.orig_path = c.orig_path.as_deref().map(|p| self.repo_to_ws(p));
                c
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_file_history_with_renames() {
        let out = "\x1eaaa\x1fbbb\x1fAnna\x1fa@x\x1f20\x1ffour\x1f\0\nM\0g.txt\0\
                   \x1ebbb\x1fccc\x1fAnna\x1fa@x\x1f10\x1fthree rename\x1f\0\nR100\0f.txt\0g.txt\0\
                   \x1eccc\x1f\x1fAnna\x1fa@x\x1f5\x1fone\x1f\0\nA\0f.txt\0";
        let h = parse_file_history(out);
        assert_eq!(h.len(), 3);
        assert_eq!(h[0].sha, "aaa");
        assert_eq!(h[0].files[0].path, "g.txt");
        assert_eq!(h[1].files[0].status, "R");
        assert_eq!(h[1].files[0].orig_path.as_deref(), Some("f.txt"));
        assert!(h[2].parents.is_empty());
        assert_eq!(h[2].files[0].status, "A");
        assert_eq!(h[2].time, 5);
    }

    #[test]
    fn parses_block_history() {
        let out = "\x1eaaa\x1fbbb\x1fA\x1fa@x\x1f20\x1ftwo\x1f\n\ndiff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -2,2 +2,2 @@\n-b\n+B\n c\n\
                   \x1ebbb\x1f\x1fA\x1fa@x\x1f10\x1fone\x1f\n\ndiff --git a/f.txt b/f.txt\n--- /dev/null\n+++ b/f.txt\n@@ -0,0 +2,2 @@\n+b\n+c\n";
        let h = parse_block_history(out);
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].path, "f.txt");
        assert_eq!(h[0].before.as_deref(), Some("b\nc\n"));
        assert_eq!(h[0].after, "B\nc\n");
        assert_eq!(h[0].start_line, 2);
        assert_eq!(h[1].before, None);
        assert_eq!(h[1].after, "b\nc\n");
    }
}
