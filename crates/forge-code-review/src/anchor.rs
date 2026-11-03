//! Findings matched to the diff: a finding on a line the diff shows gets a [`Position`]
//! (and a start line for ranges), so it can become an inline comment. Models are often a
//! line or two off; a line close to a shown line snaps to it. Findings on files that are not
//! part of the change are dropped.

use forge_forges::{ChangedFile, Position};
use forge_vcs::LineKind;
use serde::{Deserialize, Serialize};

use crate::parse::{RawFinding, Severity};

/// How far (in lines) a finding may be from a shown line and still snap to it.
pub const SNAP_DISTANCE: u32 = 3;

/// A finding of AI Review, placed on the diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Stable for the same path, line and title (dismissals survive a re-run).
    pub id: String,
    pub path: String,
    /// Line in the new file as the model reported it.
    pub line: u32,
    pub end_line: Option<u32>,
    pub severity: Severity,
    pub title: String,
    /// Markdown.
    pub explanation: String,
    pub suggestion: Option<String>,
    /// Where an inline comment goes; `None` when the line is not shown in the diff.
    pub position: Option<Position>,
    /// First line of a multi-line comment (same hunk as `position`).
    pub start_line: Option<u32>,
}

/// New-file line numbers shown in the diff, per hunk.
fn shown_lines(file: &ChangedFile) -> Vec<Vec<u32>> {
    file.hunks
        .iter()
        .map(|h| {
            let mut n = h.new_start;
            let mut out = Vec::new();
            for l in &h.lines {
                if l.kind != LineKind::Removed {
                    out.push(n);
                    n += 1;
                }
            }
            out
        })
        .collect()
}

/// The changed file a model's path refers to (exact, old path, or a path suffix).
pub fn find_file<'a>(files: &'a [ChangedFile], path: &str) -> Option<&'a ChangedFile> {
    let path = path.trim().trim_start_matches("./").trim_start_matches('/');
    files
        .iter()
        .find(|f| f.path == path)
        .or_else(|| files.iter().find(|f| f.old_path.as_deref() == Some(path)))
        .or_else(|| {
            let mut hits = files.iter().filter(|f| {
                f.path.ends_with(&format!("/{path}")) || path.ends_with(&format!("/{}", f.path))
            });
            let first = hits.next();
            // An ambiguous suffix (two `User.php`) matches nothing.
            hits.next().is_none().then_some(first).flatten()
        })
}

/// Hunk index and the shown line nearest to `line` (within [`SNAP_DISTANCE`]).
fn nearest(shown: &[Vec<u32>], line: u32) -> Option<(usize, u32)> {
    shown
        .iter()
        .enumerate()
        .flat_map(|(i, lines)| lines.iter().map(move |l| (i, *l)))
        .min_by_key(|(_, l)| l.abs_diff(line))
        .filter(|(_, l)| l.abs_diff(line) <= SNAP_DISTANCE)
}

/// A short stable id (FNV-1a of path, line and title).
pub fn finding_id(path: &str, line: u32, title: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in format!("{path}\0{line}\0{title}").bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("f{:012x}", h & 0xffff_ffff_ffff)
}

/// Place raw findings on `files`, sorted by path and line, without duplicates.
pub fn anchor(raw: Vec<RawFinding>, files: &[ChangedFile]) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    for r in raw {
        let Some(file) = find_file(files, &r.path) else {
            continue;
        };
        let shown = shown_lines(file);
        let end = nearest(&shown, r.end_line.unwrap_or(r.line));
        let start = nearest(&shown, r.line);
        let (position, start_line) = match (start, end) {
            (Some((hs, s)), Some((he, e))) if hs == he && s < e => (Some(e), Some(s)),
            (Some((_, s)), _) => (Some(s), None),
            (None, Some((_, e))) => (Some(e), None),
            (None, None) => (None, None),
        };
        let id = finding_id(&file.path, r.line, &r.title);
        if out.iter().any(|f| f.id == id) {
            continue;
        }
        out.push(Finding {
            id,
            path: file.path.clone(),
            line: r.line,
            end_line: r.end_line,
            severity: r.severity,
            title: r.title,
            explanation: r.explanation,
            suggestion: r.suggestion,
            position: position.map(|l| Position {
                path: file.path.clone(),
                old_path: file.old_path.clone(),
                new_line: Some(l),
                old_line: None,
            }),
            start_line,
        });
    }
    out.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::tests::{file, hunk};

    fn raw(path: &str, line: u32, end: Option<u32>) -> RawFinding {
        RawFinding {
            path: path.into(),
            line,
            end_line: end,
            title: format!("t{line}"),
            explanation: "e".into(),
            ..RawFinding::default()
        }
    }

    fn files() -> Vec<ChangedFile> {
        vec![
            file(
                "app/Models/User.php",
                vec![
                    hunk(
                        10,
                        &[(" ", "a"), ("-", "b"), ("+", "c"), ("+", "d"), (" ", "e")],
                    ),
                    hunk(40, &[("+", "x")]),
                ],
            ),
            file("app/Http/User.php", vec![hunk(1, &[("+", "y")])]),
        ]
    }

    #[test]
    fn places_lines_ranges_and_snaps() {
        let f = anchor(
            vec![
                raw("app/Models/User.php", 11, None),
                raw("app/Models/User.php", 11, Some(13)),
                raw("app/Models/User.php", 42, None),
                raw("app/Models/User.php", 25, None),
                raw("app/Models/User.php", 12, Some(40)),
            ],
            &files(),
        );
        let pos = |i: usize| f[i].position.as_ref().and_then(|p| p.new_line);
        assert_eq!(f.len(), 4, "same path, line and title is one finding");
        assert_eq!((f[0].line, pos(0), f[0].start_line), (11, Some(11), None));
        // A range across hunks is a single-line comment.
        assert_eq!((f[1].line, pos(1), f[1].start_line), (12, Some(12), None));
        // Line 25 is far from any shown line: listed, not placed.
        assert_eq!((f[2].line, pos(2)), (25, None));
        // Line 42 snaps to 40, the last shown line of the second hunk.
        assert_eq!((f[3].line, pos(3)), (42, Some(40)));

        let range = anchor(vec![raw("app/Models/User.php", 10, Some(13))], &files());
        assert_eq!(
            (
                range[0].start_line,
                range[0].position.as_ref().unwrap().new_line
            ),
            (Some(10), Some(13))
        );
    }

    #[test]
    fn matches_paths_by_suffix_and_drops_unknown_files() {
        let fs = files();
        assert_eq!(
            find_file(&fs, "Models/User.php").unwrap().path,
            "app/Models/User.php"
        );
        assert!(find_file(&fs, "User.php").is_none(), "ambiguous");
        assert!(anchor(vec![raw("src/Other.php", 1, None)], &fs).is_empty());
        assert_eq!(finding_id("a", 1, "t"), finding_id("a", 1, "t"));
        assert_ne!(finding_id("a", 1, "t"), finding_id("a", 2, "t"));
    }
}
