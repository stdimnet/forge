//! Editor navigation that is not tied to one keystroke:
//!
//! - [`EditHistory`]: the places where text was changed, across files, behind Last Edit
//!   Location (`Ctrl+Shift+Backspace`), Next Edit Location and Recent Locations' "Show changed
//!   only". It is separate from the Back/Forward navigation history, like PHPStorm's change
//!   places. Nearby consecutive edits merge into one place, and places follow later edits
//!   ([`EditOp::ApplyEdits`], same line mapping as bookmarks), renames and deletions.
//! - [`change_target`]: which change hunk Next Change / Previous Change
//!   (`Ctrl+Alt+Shift+Down/Up`) moves to from the caret line.
//!
//! Lines are 0-based.

use forge_bookmarks::{map_line, LineEdit};
use serde::{Deserialize, Serialize};

/// Consecutive edits in one file at most this many lines apart are one place.
pub const MERGE_LINES: usize = 2;

/// How many places the history keeps by default.
pub const DEFAULT_MAX_PLACES: usize = 50;

/// A place where text was changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditPlace {
    /// Project-relative path with `/` separators.
    pub path: String,
    pub line: usize,
    /// UTF-16 column of the caret after the edit (not mapped through later edits; the editor
    /// clamps it to the line).
    #[serde(default)]
    pub column: usize,
}

impl EditPlace {
    fn near(&self, path: &str, line: usize) -> bool {
        self.path == path && self.line.abs_diff(line) <= MERGE_LINES
    }
}

/// The caret, to skip places that are where the caret already is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Caret {
    pub path: String,
    pub line: usize,
}

/// A change to the edit history, or a move through it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum EditOp {
    /// Text was changed; the caret is at `line`/`column` afterwards.
    Record {
        path: String,
        line: usize,
        #[serde(default)]
        column: usize,
    },
    /// Line edits of one document change (old-document coordinates, ascending).
    ApplyEdits {
        path: String,
        edits: Vec<LineEdit>,
    },
    RenamePath {
        from: String,
        to: String,
    },
    RemovePath {
        path: String,
    },
    /// Last Edit Location.
    Back {
        #[serde(default)]
        caret: Option<Caret>,
    },
    /// Next Edit Location.
    Forward {
        #[serde(default)]
        caret: Option<Caret>,
    },
    Clear,
    /// The number of places to keep.
    SetMax {
        max: usize,
    },
}

/// Places where text was changed, oldest first, with a position for Back/Forward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditHistory {
    places: Vec<EditPlace>,
    /// The place Back/Forward stands on; `places.len()` before the first jump and after
    /// every new edit.
    index: usize,
    max: usize,
}

impl Default for EditHistory {
    fn default() -> Self {
        EditHistory {
            places: Vec::new(),
            index: 0,
            max: DEFAULT_MAX_PLACES,
        }
    }
}

fn under(path: &str, dir: &str) -> bool {
    path == dir || path.strip_prefix(dir).is_some_and(|r| r.starts_with('/'))
}

impl EditHistory {
    /// The places, newest first (Recent Locations' "Show changed only").
    pub fn places(&self) -> Vec<EditPlace> {
        self.places.iter().rev().cloned().collect()
    }

    /// Apply one operation; Back and Forward return the place to go to.
    pub fn apply(&mut self, op: EditOp) -> Option<EditPlace> {
        match op {
            EditOp::Record { path, line, column } => self.record(path, line, column),
            EditOp::ApplyEdits { path, edits } => {
                for place in self.places.iter_mut().filter(|p| p.path == path) {
                    place.line = edits.iter().rev().fold(place.line, map_line);
                }
                self.dedupe();
            }
            EditOp::RenamePath { from, to } => {
                for place in &mut self.places {
                    if under(&place.path, &from) {
                        place.path = format!("{to}{}", &place.path[from.len()..]);
                    }
                }
            }
            EditOp::RemovePath { path } => {
                let before = self.places.len();
                let removed_before_index = self.places[..self.index.min(before)]
                    .iter()
                    .filter(|p| under(&p.path, &path))
                    .count();
                self.places.retain(|p| !under(&p.path, &path));
                self.index = self.index.saturating_sub(removed_before_index);
            }
            EditOp::Back { caret } => return self.navigate(false, caret.as_ref()),
            EditOp::Forward { caret } => return self.navigate(true, caret.as_ref()),
            EditOp::Clear => {
                self.places.clear();
                self.index = 0;
            }
            EditOp::SetMax { max } => {
                self.max = max.max(1);
                self.trim();
            }
        }
        None
    }

    fn record(&mut self, path: String, line: usize, column: usize) {
        // Merge with the newest place when the edit is next to it, like PHPStorm.
        if self.places.last().is_some_and(|p| p.near(&path, line)) {
            self.places.pop();
        }
        self.places.push(EditPlace { path, line, column });
        self.trim();
        self.index = self.places.len();
    }

    fn trim(&mut self) {
        if self.places.len() > self.max {
            let drop = self.places.len() - self.max;
            self.places.drain(..drop);
            self.index = self.index.saturating_sub(drop);
        }
        self.index = self.index.min(self.places.len());
    }

    /// After lines were removed, places can land on the same line: keep the newest one.
    fn dedupe(&mut self) {
        let mut i = self.places.len();
        while i > 0 {
            i -= 1;
            let dup = self.places[i + 1..]
                .iter()
                .any(|p| p.path == self.places[i].path && p.line == self.places[i].line);
            if dup {
                self.places.remove(i);
                if self.index > i {
                    self.index -= 1;
                }
            }
        }
    }

    fn navigate(&mut self, forward: bool, caret: Option<&Caret>) -> Option<EditPlace> {
        let here = |p: &EditPlace| caret.is_some_and(|c| p.near(&c.path, c.line));
        let mut i = self.index;
        loop {
            if forward {
                if i + 1 >= self.places.len() {
                    return None;
                }
                i += 1;
            } else {
                if i == 0 {
                    return None;
                }
                i -= 1;
            }
            if !here(&self.places[i]) {
                self.index = i;
                return Some(self.places[i].clone());
            }
        }
    }
}

/// A change hunk of the editor's gutter: lines `start..end` of the document (`start == end`
/// for deleted lines, marked at `start`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

impl LineRange {
    fn last(self) -> usize {
        self.end.max(self.start + 1) - 1
    }
    fn contains(self, line: usize) -> bool {
        self.start <= line && line <= self.last()
    }
}

/// The hunk Next Change (`forward`) or Previous Change moves to from `line`: the first hunk
/// below (or last above) that does not contain the caret, wrapping around at the end of the
/// file like PHPStorm. `ranges` are in document order.
pub fn change_target(ranges: &[LineRange], line: usize, forward: bool) -> Option<usize> {
    if forward {
        ranges
            .iter()
            .position(|r| r.start > line && !r.contains(line))
            .or_else(|| (!ranges.is_empty()).then_some(0))
    } else {
        ranges
            .iter()
            .rposition(|r| r.last() < line)
            .or_else(|| ranges.len().checked_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(h: &mut EditHistory, path: &str, line: usize) {
        h.apply(EditOp::Record {
            path: path.into(),
            line,
            column: 0,
        });
    }

    fn caret(path: &str, line: usize) -> Option<Caret> {
        Some(Caret {
            path: path.into(),
            line,
        })
    }

    fn lines(h: &EditHistory) -> Vec<(String, usize)> {
        h.places().into_iter().map(|p| (p.path, p.line)).collect()
    }

    #[test]
    fn nearby_edits_merge_into_the_newest_place() {
        let mut h = EditHistory::default();
        rec(&mut h, "a.php", 10);
        rec(&mut h, "a.php", 11);
        rec(&mut h, "a.php", 12);
        rec(&mut h, "a.php", 40);
        rec(&mut h, "b.php", 40);
        rec(&mut h, "a.php", 41);
        assert_eq!(
            lines(&h),
            [
                ("a.php".into(), 41),
                ("b.php".into(), 40),
                ("a.php".into(), 40),
                ("a.php".into(), 12)
            ]
        );
    }

    #[test]
    fn back_and_forward_walk_the_places_and_skip_the_caret() {
        let mut h = EditHistory::default();
        rec(&mut h, "a.php", 10);
        rec(&mut h, "b.php", 5);
        rec(&mut h, "a.php", 50);
        // The caret is still at the newest edit: the first jump goes to the one before.
        let p = h
            .apply(EditOp::Back {
                caret: caret("a.php", 50),
            })
            .unwrap();
        assert_eq!((p.path.as_str(), p.line), ("b.php", 5));
        let p = h
            .apply(EditOp::Back {
                caret: caret("b.php", 5),
            })
            .unwrap();
        assert_eq!(p.line, 10);
        assert_eq!(
            h.apply(EditOp::Back {
                caret: caret("a.php", 10)
            }),
            None
        );
        let p = h
            .apply(EditOp::Forward {
                caret: caret("a.php", 10),
            })
            .unwrap();
        assert_eq!(p.path, "b.php");
        let p = h
            .apply(EditOp::Forward {
                caret: caret("b.php", 5),
            })
            .unwrap();
        assert_eq!(p.line, 50);
        assert_eq!(h.apply(EditOp::Forward { caret: None }), None);
        // Away from the newest edit, Back goes to it first.
        let mut h2 = EditHistory::default();
        rec(&mut h2, "a.php", 10);
        rec(&mut h2, "a.php", 50);
        let p = h2
            .apply(EditOp::Back {
                caret: caret("a.php", 0),
            })
            .unwrap();
        assert_eq!(p.line, 50);
    }

    #[test]
    fn a_new_edit_resets_the_position() {
        let mut h = EditHistory::default();
        rec(&mut h, "a.php", 10);
        rec(&mut h, "a.php", 50);
        h.apply(EditOp::Back { caret: None });
        h.apply(EditOp::Back { caret: None });
        rec(&mut h, "c.php", 1);
        assert_eq!(h.apply(EditOp::Forward { caret: None }), None);
        let p = h
            .apply(EditOp::Back {
                caret: caret("c.php", 1),
            })
            .unwrap();
        assert_eq!(p.line, 50);
    }

    #[test]
    fn places_follow_later_edits() {
        let mut h = EditHistory::default();
        rec(&mut h, "a.php", 10);
        rec(&mut h, "a.php", 30);
        rec(&mut h, "b.php", 30);
        // Three lines inserted at line 2 of a.php.
        h.apply(EditOp::ApplyEdits {
            path: "a.php".into(),
            edits: vec![LineEdit {
                line: 2,
                removed: 0,
                added: 3,
                at_line_start: false,
            }],
        });
        assert_eq!(
            lines(&h),
            [
                ("b.php".into(), 30),
                ("a.php".into(), 33),
                ("a.php".into(), 13)
            ]
        );
        // Removing lines 12..=33 puts both places on line 12; they collapse into one.
        h.apply(EditOp::ApplyEdits {
            path: "a.php".into(),
            edits: vec![LineEdit {
                line: 12,
                removed: 22,
                added: 0,
                at_line_start: true,
            }],
        });
        assert_eq!(lines(&h), [("b.php".into(), 30), ("a.php".into(), 12)]);
    }

    #[test]
    fn renames_deletions_and_the_limit() {
        let mut h = EditHistory::default();
        rec(&mut h, "src/a.php", 1);
        rec(&mut h, "src/b.php", 1);
        rec(&mut h, "srcx/c.php", 1);
        h.apply(EditOp::RenamePath {
            from: "src".into(),
            to: "lib".into(),
        });
        assert_eq!(h.places()[2].path, "lib/a.php");
        assert_eq!(h.places()[0].path, "srcx/c.php");
        h.apply(EditOp::RemovePath { path: "lib".into() });
        assert_eq!(h.places().len(), 1);
        h.apply(EditOp::SetMax { max: 2 });
        for i in 0..5 {
            rec(&mut h, "x.php", i * 10);
        }
        assert_eq!(lines(&h), [("x.php".into(), 40), ("x.php".into(), 30)]);
        h.apply(EditOp::Clear);
        assert!(h.places().is_empty());
    }

    #[test]
    fn ops_parse_from_json() {
        let op: EditOp =
            serde_json::from_str(r#"{"op":"back","caret":{"path":"a","line":3}}"#).unwrap();
        assert_eq!(
            op,
            EditOp::Back {
                caret: caret("a", 3)
            }
        );
        let op: EditOp = serde_json::from_str(
            r#"{"op":"applyEdits","path":"a","edits":[{"line":1,"removed":0,"added":2}]}"#,
        )
        .unwrap();
        assert!(matches!(op, EditOp::ApplyEdits { .. }));
    }

    #[test]
    fn change_targets_wrap_around() {
        let r = |start, end| LineRange { start, end };
        let ranges = [r(3, 5), r(10, 10), r(20, 22)];
        assert_eq!(change_target(&ranges, 0, true), Some(0));
        assert_eq!(change_target(&ranges, 3, true), Some(1));
        assert_eq!(change_target(&ranges, 4, true), Some(1));
        assert_eq!(change_target(&ranges, 10, true), Some(2));
        assert_eq!(change_target(&ranges, 21, true), Some(0));
        assert_eq!(change_target(&ranges, 21, false), Some(1));
        assert_eq!(change_target(&ranges, 10, false), Some(0));
        assert_eq!(change_target(&ranges, 5, false), Some(0));
        assert_eq!(change_target(&ranges, 4, false), Some(2));
        assert_eq!(change_target(&ranges, 0, false), Some(2));
        assert_eq!(change_target(&[], 0, true), None);
        assert_eq!(change_target(&[], 0, false), None);
    }
}
