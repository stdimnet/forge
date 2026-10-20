//! Parser for `git status --porcelain=v2 -z --branch`.

use serde::Serialize;

/// State of a file on one side (index or working tree).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileState {
    Unmodified,
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    Ignored,
    Conflicted,
}

impl FileState {
    fn from_code(c: char) -> Self {
        match c {
            'M' => Self::Modified,
            'A' => Self::Added,
            'D' => Self::Deleted,
            'R' => Self::Renamed,
            'C' => Self::Copied,
            'T' => Self::TypeChanged,
            'U' => Self::Conflicted,
            _ => Self::Unmodified,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEntry {
    /// Path relative to the workspace root, `/`-separated.
    pub path: String,
    /// Source path of a rename or copy.
    pub orig_path: Option<String>,
    /// Change between HEAD and the index (staged).
    pub index: FileState,
    /// Change between the index and the working tree (unstaged).
    pub worktree: FileState,
    /// Unmerged path (merge/rebase conflict).
    pub conflict: bool,
}

impl StatusEntry {
    pub fn is_staged(&self) -> bool {
        !self.conflict
            && !matches!(
                self.index,
                FileState::Unmodified | FileState::Untracked | FileState::Ignored
            )
    }

    pub fn is_unstaged(&self) -> bool {
        !self.conflict && self.worktree != FileState::Unmodified && !self.is_untracked()
    }

    pub fn is_untracked(&self) -> bool {
        self.worktree == FileState::Untracked
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    /// Current branch, `None` when HEAD is detached.
    pub branch: Option<String>,
    /// Commit HEAD points at, `None` on an unborn branch.
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub entries: Vec<StatusEntry>,
}

/// Parse NUL-separated porcelain v2 output (with `--branch`).
pub fn parse_status(out: &str) -> RepoStatus {
    let mut st = RepoStatus::default();
    let mut records = out.split('\0').filter(|r| !r.is_empty());
    while let Some(rec) = records.next() {
        let Some((kind, rest)) = rec.split_once(' ') else {
            continue;
        };
        match kind {
            "#" => parse_header(rest, &mut st),
            "1" => {
                // <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>
                let f: Vec<&str> = rest.splitn(8, ' ').collect();
                if f.len() == 8 {
                    st.entries.push(ordinary(f[0], f[7], None));
                }
            }
            "2" => {
                // <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path>\0<origPath>
                let f: Vec<&str> = rest.splitn(9, ' ').collect();
                let orig = records.next().map(str::to_string);
                if f.len() == 9 {
                    st.entries.push(ordinary(f[0], f[8], orig));
                }
            }
            "u" => {
                // <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>
                let f: Vec<&str> = rest.splitn(10, ' ').collect();
                if f.len() == 10 {
                    st.entries.push(StatusEntry {
                        path: f[9].to_string(),
                        orig_path: None,
                        index: FileState::Conflicted,
                        worktree: FileState::Conflicted,
                        conflict: true,
                    });
                }
            }
            "?" => st.entries.push(StatusEntry {
                path: rest.to_string(),
                orig_path: None,
                index: FileState::Untracked,
                worktree: FileState::Untracked,
                conflict: false,
            }),
            "!" => st.entries.push(StatusEntry {
                path: rest.to_string(),
                orig_path: None,
                index: FileState::Ignored,
                worktree: FileState::Ignored,
                conflict: false,
            }),
            _ => {}
        }
    }
    st
}

fn ordinary(xy: &str, path: &str, orig_path: Option<String>) -> StatusEntry {
    let mut c = xy.chars();
    let x = c.next().unwrap_or('.');
    let y = c.next().unwrap_or('.');
    StatusEntry {
        path: path.to_string(),
        orig_path,
        index: FileState::from_code(x),
        worktree: FileState::from_code(y),
        conflict: false,
    }
}

fn parse_header(rest: &str, st: &mut RepoStatus) {
    let (key, value) = rest.split_once(' ').unwrap_or((rest, ""));
    match key {
        "branch.oid" if value != "(initial)" => st.head = Some(value.to_string()),
        "branch.head" if value != "(detached)" => st.branch = Some(value.to_string()),
        "branch.upstream" => st.upstream = Some(value.to_string()),
        "branch.ab" => {
            for part in value.split_whitespace() {
                if let Some(n) = part.strip_prefix('+') {
                    st.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix('-') {
                    st.behind = n.parse().unwrap_or(0);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headers_and_entries() {
        let out = [
            "# branch.oid 1234567890abcdef",
            "# branch.head feature/login",
            "# branch.upstream origin/feature/login",
            "# branch.ab +2 -1",
            "1 .M N... 100644 100644 100644 aaa bbb src/a file.php",
            "1 A. N... 000000 100644 100644 000 bbb new.php",
            "2 R. N... 100644 100644 100644 aaa aaa R100 renamed.php",
            "old name.php",
            "u UU N... 100644 100644 100644 100644 a b c conflict.php",
            "? untracked dir/x.txt",
            "",
        ]
        .join("\0");
        let st = parse_status(&out);
        assert_eq!(st.branch.as_deref(), Some("feature/login"));
        assert_eq!(st.head.as_deref(), Some("1234567890abcdef"));
        assert_eq!(st.upstream.as_deref(), Some("origin/feature/login"));
        assert_eq!((st.ahead, st.behind), (2, 1));
        assert_eq!(st.entries.len(), 5);

        let a = &st.entries[0];
        assert_eq!(a.path, "src/a file.php");
        assert!(a.is_unstaged() && !a.is_staged());

        assert!(st.entries[1].is_staged());
        assert_eq!(st.entries[1].index, FileState::Added);

        let r = &st.entries[2];
        assert_eq!(r.index, FileState::Renamed);
        assert_eq!(r.orig_path.as_deref(), Some("old name.php"));

        assert!(st.entries[3].conflict);
        assert!(st.entries[4].is_untracked());
        assert_eq!(st.entries[4].path, "untracked dir/x.txt");
    }

    #[test]
    fn handles_initial_and_detached() {
        let st = parse_status("# branch.oid (initial)\0# branch.head (detached)\0");
        assert_eq!(st.head, None);
        assert_eq!(st.branch, None);
    }
}
