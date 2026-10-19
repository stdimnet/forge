//! Parser for `git blame --porcelain`.

use std::collections::HashMap;

use serde::Serialize;

pub(crate) const UNCOMMITTED: &str = "0000000000000000000000000000000000000000";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlameCommit {
    pub sha: String,
    pub author: String,
    pub email: String,
    /// Author time, seconds since the Unix epoch.
    pub time: i64,
    pub summary: String,
    /// Lines changed locally and not committed yet.
    pub uncommitted: bool,
}

/// Blame for a whole file: unique commits plus, per line (0-based), an index into `commits`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Blame {
    pub commits: Vec<BlameCommit>,
    pub lines: Vec<usize>,
}

impl Blame {
    pub fn commit_for_line(&self, line: usize) -> Option<&BlameCommit> {
        self.lines.get(line).and_then(|&i| self.commits.get(i))
    }
}

pub fn parse_blame(out: &str) -> Blame {
    let mut blame = Blame::default();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut current: Option<usize> = None;
    // Header lines (key value) belong to the commit of the last "<sha> <orig> <final>" line.
    for line in out.lines() {
        if let Some(_content) = line.strip_prefix('\t') {
            if let Some(i) = current {
                blame.lines.push(i);
            }
            continue;
        }
        let mut parts = line.splitn(2, ' ');
        let first = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("");
        if first.len() == 40 && first.bytes().all(|b| b.is_ascii_hexdigit()) {
            let i = *index.entry(first.to_string()).or_insert_with(|| {
                blame.commits.push(BlameCommit {
                    sha: first.to_string(),
                    author: String::new(),
                    email: String::new(),
                    time: 0,
                    summary: String::new(),
                    uncommitted: first == UNCOMMITTED,
                });
                blame.commits.len() - 1
            });
            current = Some(i);
            continue;
        }
        let Some(c) = current.and_then(|i| blame.commits.get_mut(i)) else {
            continue;
        };
        match first {
            "author" => c.author = rest.to_string(),
            "author-mail" => {
                c.email = rest
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_string()
            }
            "author-time" => c.time = rest.parse().unwrap_or(0),
            "summary" => c.summary = rest.to_string(),
            _ => {}
        }
    }
    blame
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
1111111111111111111111111111111111111111 1 1 2
author Marlon
author-mail <m@x.io>
author-time 1700000000
author-tz +0100
committer Marlon
committer-mail <m@x.io>
committer-time 1700000000
committer-tz +0100
summary Add user model
filename User.php
\t<?php
1111111111111111111111111111111111111111 2 2
\t
0000000000000000000000000000000000000000 3 3 1
author Not Committed Yet
author-mail <not.committed.yet>
author-time 1700000500
author-tz +0100
summary Version of User.php from User.php
filename User.php
\tclass User {}
";

    #[test]
    fn parses_porcelain() {
        let b = parse_blame(SAMPLE);
        assert_eq!(b.commits.len(), 2);
        assert_eq!(b.lines, vec![0, 0, 1]);
        let c = b.commit_for_line(0).unwrap();
        assert_eq!(c.author, "Marlon");
        assert_eq!(c.email, "m@x.io");
        assert_eq!(c.time, 1_700_000_000);
        assert_eq!(c.summary, "Add user model");
        assert!(b.commit_for_line(2).unwrap().uncommitted);
    }
}
