//! Three-way merge for the merge conflict resolver (PHPStorm's "Merge Revisions" window).
//!
//! [`merge_three_way`] compares the yours (left) and theirs (right) versions of a file with
//! their common base line by line (diff3 chunking) and reports every change as a
//! [`MergeChunk`]: a change on one side only, the same change on both sides, or a conflict.
//! For conflicts where both sides changed different parts of the same lines,
//! [`resolve_simple`] merges the two changes word by word ("Resolve Simple Conflicts").
//!
//! Line ranges are 0-based and end-exclusive. Line breaks stay part of their line, so the
//! texts of the ranges concatenate back to the original text.

use serde::{Deserialize, Serialize};
use similar::{capture_diff_slices, Algorithm, DiffOp};

/// Lines `start..end` (0-based, end-exclusive) of one version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

impl LineRange {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// What a chunk changes compared with the base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChunkKind {
    /// Only the left (yours) side changed: a non-conflicting change.
    Left,
    /// Only the right (theirs) side changed: a non-conflicting change.
    Right,
    /// Both sides made the same change: non-conflicting.
    Both,
    /// Both sides changed the same lines differently.
    Conflict,
}

/// One difference between the base and at least one side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeChunk {
    pub kind: ChunkKind,
    pub base: LineRange,
    pub left: LineRange,
    pub right: LineRange,
    pub base_text: String,
    pub left_text: String,
    pub right_text: String,
    /// The word-level merge of a conflict whose sides changed different parts of the lines
    /// ("Resolve Simple Conflicts"); `None` for real conflicts and for non-conflicts.
    pub simple: Option<String>,
}

impl MergeChunk {
    /// Whether the left side differs from the base in this chunk.
    pub fn left_changed(&self) -> bool {
        self.kind != ChunkKind::Right
    }

    /// Whether the right side differs from the base in this chunk.
    pub fn right_changed(&self) -> bool {
        self.kind != ChunkKind::Left
    }
}

/// The chunks of a three-way merge plus the counts the merge window shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeModel {
    pub chunks: Vec<MergeChunk>,
    /// Non-conflicting changes.
    pub changes: usize,
    pub conflicts: usize,
    /// Conflicts that "Resolve Simple Conflicts" can merge.
    pub simple_conflicts: usize,
}

/// Split `text` into lines that keep their `\n`; the last line may have none.
pub fn split_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// A changed region: `base.0..base.1` of the old sequence became `side.0..side.1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hunk {
    base: (usize, usize),
    side: (usize, usize),
}

impl Hunk {
    fn is_insertion(&self) -> bool {
        self.base.0 == self.base.1
    }

    /// Length change of the side relative to the base.
    fn delta(&self) -> isize {
        (self.side.1 - self.side.0) as isize - (self.base.1 - self.base.0) as isize
    }
}

/// The changed regions between `old` and `new`, adjacent edits joined.
fn hunks<T: Eq + std::hash::Hash + Ord>(old: &[T], new: &[T]) -> Vec<Hunk> {
    let mut out: Vec<Hunk> = Vec::new();
    for op in capture_diff_slices(Algorithm::Myers, old, new) {
        let (o0, o1, n0, n1) = match op {
            DiffOp::Equal { .. } => continue,
            DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => (old_index, old_index + old_len, new_index, new_index),
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => (old_index, old_index, new_index, new_index + new_len),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => (
                old_index,
                old_index + old_len,
                new_index,
                new_index + new_len,
            ),
        };
        match out.last_mut() {
            Some(last) if last.base.1 == o0 && last.side.1 == n0 => {
                last.base.1 = o1;
                last.side.1 = n1;
            }
            _ => out.push(Hunk {
                base: (o0, o1),
                side: (n0, n1),
            }),
        }
    }
    out
}

/// Whether hunk `h` (starting at or after `start`) belongs to the group `start..end`: it
/// overlaps it, or one of them is an insertion at the other's edge (the order of the two
/// changes would be ambiguous).
fn joins(start: usize, end: usize, h: &Hunk) -> bool {
    h.base.0 < end || (h.base.0 == end && (h.is_insertion() || start == end))
}

/// Compare `left` (yours) and `right` (theirs) with their common `base` line by line.
pub fn merge_three_way(base: &str, left: &str, right: &str) -> MergeModel {
    let b = split_lines(base);
    let l = split_lines(left);
    let r = split_lines(right);
    let lh = hunks(&b, &l);
    let rh = hunks(&b, &r);

    let mut chunks = Vec::new();
    let (mut i, mut j) = (0, 0);
    // Line count difference (side - base) of the hunks before the current group.
    let (mut ld, mut rd) = (0isize, 0isize);
    while i < lh.len() || j < rh.len() {
        let take_left = match (lh.get(i), rh.get(j)) {
            (Some(x), Some(y)) => x.base.0 <= y.base.0,
            (Some(_), None) => true,
            _ => false,
        };
        let first = if take_left { lh[i] } else { rh[j] };
        let (start, mut end) = first.base;
        let (li, rj) = (i, j);
        if take_left {
            i += 1;
        } else {
            j += 1;
        }
        loop {
            let mut grew = false;
            if let Some(h) = lh.get(i).filter(|h| joins(start, end, h)) {
                end = end.max(h.base.1);
                i += 1;
                grew = true;
            }
            if let Some(h) = rh.get(j).filter(|h| joins(start, end, h)) {
                end = end.max(h.base.1);
                j += 1;
                grew = true;
            }
            if !grew {
                break;
            }
        }
        let side_range = |delta_before: isize, group: &[Hunk]| {
            let after = delta_before + group.iter().map(Hunk::delta).sum::<isize>();
            let s = (start as isize + delta_before) as usize;
            let e = (end as isize + after) as usize;
            (LineRange::new(s, e), after)
        };
        let (left_range, ld_after) = side_range(ld, &lh[li..i]);
        let (right_range, rd_after) = side_range(rd, &rh[rj..j]);
        ld = ld_after;
        rd = rd_after;

        let text = |lines: &[&str], range: LineRange| lines[range.start..range.end].concat();
        let base_range = LineRange::new(start, end);
        let base_text = text(&b, base_range);
        let left_text = text(&l, left_range);
        let right_text = text(&r, right_range);
        let kind = match (i > li, j > rj) {
            (true, false) => ChunkKind::Left,
            (false, true) => ChunkKind::Right,
            _ if left_text == right_text => ChunkKind::Both,
            _ => ChunkKind::Conflict,
        };
        let simple = (kind == ChunkKind::Conflict)
            .then(|| resolve_simple(&base_text, &left_text, &right_text))
            .flatten();
        chunks.push(MergeChunk {
            kind,
            base: base_range,
            left: left_range,
            right: right_range,
            base_text,
            left_text,
            right_text,
            simple,
        });
    }

    let conflicts = chunks
        .iter()
        .filter(|c| c.kind == ChunkKind::Conflict)
        .count();
    MergeModel {
        changes: chunks.len() - conflicts,
        conflicts,
        simple_conflicts: chunks.iter().filter(|c| c.simple.is_some()).count(),
        chunks,
    }
}

/// Words (letters, digits, `_`), runs of blanks, line breaks and single other characters.
fn tokenize(text: &str) -> Vec<&str> {
    #[derive(PartialEq)]
    enum Class {
        Word,
        Blank,
        Other,
    }
    let class = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            Class::Word
        } else if c.is_whitespace() && c != '\n' && c != '\r' {
            Class::Blank
        } else {
            Class::Other
        }
    };
    let mut out = Vec::new();
    let mut start = 0;
    let mut current: Option<Class> = None;
    for (i, c) in text.char_indices() {
        let k = class(c);
        let joins = current.as_ref().is_some_and(|cur| *cur == k && k != Class::Other)
            // Keep `\r\n` together.
            || (c == '\n' && text[..i].ends_with('\r') && start + 1 == i);
        if !joins && i > start {
            out.push(&text[start..i]);
            start = i;
        }
        current = Some(k);
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// PHPStorm's "Resolve Simple Conflicts": merge a conflict word by word when the two sides
/// changed different parts of its lines. `None` when the changes touch the same words (or
/// an insertion meets the other side's change), which needs a human.
pub fn resolve_simple(base: &str, left: &str, right: &str) -> Option<String> {
    if left == right || right == base {
        return Some(left.to_string());
    }
    if left == base {
        return Some(right.to_string());
    }
    let b = tokenize(base);
    let l = tokenize(left);
    let r = tokenize(right);
    let lh = hunks(&b, &l);
    let rh = hunks(&b, &r);
    let same =
        |x: &Hunk, y: &Hunk| x.base == y.base && l[x.side.0..x.side.1] == r[y.side.0..y.side.1];
    for x in &lh {
        for y in &rh {
            if same(x, y) {
                continue;
            }
            let touch = if x.is_insertion() || y.is_insertion() {
                x.base.0 <= y.base.1 && y.base.0 <= x.base.1
            } else {
                x.base.0 < y.base.1 && y.base.0 < x.base.1
            };
            if touch {
                return None;
            }
        }
    }
    // Apply both sides' hunks in base order; identical changes only once.
    let mut edits: Vec<(Hunk, &[&str])> = lh.iter().map(|h| (*h, &l[h.side.0..h.side.1])).collect();
    for y in &rh {
        if !lh.iter().any(|x| same(x, y)) {
            edits.push((*y, &r[y.side.0..y.side.1]));
        }
    }
    edits.sort_by_key(|(h, _)| h.base);
    let mut out = String::with_capacity(base.len().max(left.len()).max(right.len()));
    let mut pos = 0;
    for (h, tokens) in edits {
        out.extend(b[pos..h.base.0].iter().copied());
        out.extend(tokens.iter().copied());
        pos = h.base.1;
    }
    out.extend(b[pos..].iter().copied());
    Some(out)
}

/// The result of merging automatically: every non-conflicting change applied, simple
/// conflicts resolved when `simple` is set, and the remaining conflicts left at the base text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoMerge {
    pub text: String,
    pub unresolved: usize,
}

/// Apply what [`merge_three_way`] can decide alone ("Apply All Non-Conflicting Changes",
/// plus "Resolve Simple Conflicts" with `simple`).
pub fn auto_merge(base: &str, left: &str, right: &str, simple: bool) -> AutoMerge {
    let model = merge_three_way(base, left, right);
    let lines = split_lines(base);
    let mut text = String::with_capacity(base.len());
    let mut pos = 0;
    let mut unresolved = 0;
    for c in &model.chunks {
        text.push_str(&lines[pos..c.base.start].concat());
        match (c.kind, &c.simple) {
            (ChunkKind::Left | ChunkKind::Both, _) => text.push_str(&c.left_text),
            (ChunkKind::Right, _) => text.push_str(&c.right_text),
            (ChunkKind::Conflict, Some(s)) if simple => text.push_str(s),
            (ChunkKind::Conflict, _) => {
                unresolved += 1;
                text.push_str(&c.base_text);
            }
        }
        pos = c.base.end;
    }
    text.push_str(&lines[pos..].concat());
    AutoMerge { text, unresolved }
}

/// Normalize line breaks to `\n`; returns whether the text used `\r\n`.
pub fn normalize_eol(text: &str) -> (String, bool) {
    if text.contains("\r\n") {
        (text.replace("\r\n", "\n"), true)
    } else {
        (text.to_string(), false)
    }
}

/// Write `\n` line breaks back as `\r\n` when the file used them.
pub fn restore_eol(text: &str, crlf: bool) -> String {
    if crlf {
        text.replace("\r\n", "\n").replace('\n', "\r\n")
    } else {
        text.to_string()
    }
}

/// Whether file content looks binary (a NUL byte in the first 8000 bytes, like git).
pub fn looks_binary(content: &str) -> bool {
    content.bytes().take(8000).any(|b| b == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "<?php\n\nclass Cart\n{\n    public function total(): int\n    {\n        return 0;\n    }\n\n    public function count(): int\n    {\n        return 0;\n    }\n}\n";

    fn kinds(m: &MergeModel) -> Vec<ChunkKind> {
        m.chunks.iter().map(|c| c.kind).collect()
    }

    #[test]
    fn identical_versions_have_no_chunks() {
        let m = merge_three_way(BASE, BASE, BASE);
        assert!(m.chunks.is_empty());
        assert_eq!((m.changes, m.conflicts), (0, 0));
    }

    #[test]
    fn changes_on_different_lines_do_not_conflict() {
        let left = BASE.replace(
            "        return 0;\n    }\n\n    public function count",
            "        return 42;\n    }\n\n    public function count",
        );
        let right = BASE.replace("class Cart\n", "final class Cart\n");
        let m = merge_three_way(BASE, &left, &right);
        assert_eq!(kinds(&m), vec![ChunkKind::Right, ChunkKind::Left]);
        assert_eq!((m.changes, m.conflicts), (2, 0));
        let right_chunk = &m.chunks[0];
        assert_eq!(right_chunk.base, LineRange::new(2, 3));
        assert_eq!(right_chunk.right_text, "final class Cart\n");
        assert_eq!(right_chunk.left_text, "class Cart\n");
        let left_chunk = &m.chunks[1];
        assert_eq!(left_chunk.base, LineRange::new(6, 7));
        assert_eq!(left_chunk.left, LineRange::new(6, 7));
        assert_eq!(left_chunk.left_text, "        return 42;\n");

        let merged = auto_merge(BASE, &left, &right, false);
        assert_eq!(merged.unresolved, 0);
        assert!(merged.text.contains("final class Cart") && merged.text.contains("return 42;"));
    }

    #[test]
    fn side_ranges_follow_inserted_and_deleted_lines() {
        // Left inserts two lines at the top, right deletes a line further down.
        let left = format!("// a\n// b\n{BASE}");
        let right = BASE.replace("\n\nclass Cart", "\nclass Cart");
        let m = merge_three_way(BASE, &left, &right);
        assert_eq!(kinds(&m), vec![ChunkKind::Left, ChunkKind::Right]);
        assert_eq!(m.chunks[0].base, LineRange::new(0, 0));
        assert_eq!(m.chunks[0].left, LineRange::new(0, 2));
        assert_eq!(m.chunks[0].right, LineRange::new(0, 0));
        let del = &m.chunks[1];
        assert_eq!(del.base, LineRange::new(1, 2));
        assert_eq!(del.left, LineRange::new(3, 4));
        assert_eq!(del.right, LineRange::new(1, 1));
        assert_eq!(del.right_text, "");
        let merged = auto_merge(BASE, &left, &right, false);
        assert_eq!(merged.text, format!("// a\n// b\n{right}"));
    }

    #[test]
    fn the_same_change_on_both_sides_is_not_a_conflict() {
        let both = BASE.replace("return 0;\n    }\n}", "return 1;\n    }\n}");
        let m = merge_three_way(BASE, &both, &both);
        assert_eq!(kinds(&m), vec![ChunkKind::Both]);
        assert_eq!(m.conflicts, 0);
        assert_eq!(auto_merge(BASE, &both, &both, false).text, both);
    }

    #[test]
    fn different_changes_of_the_same_line_conflict() {
        let left = BASE.replace("class Cart\n", "class ShoppingCart\n");
        let right = BASE.replace("class Cart\n", "class Basket\n");
        let m = merge_three_way(BASE, &left, &right);
        assert_eq!(kinds(&m), vec![ChunkKind::Conflict]);
        let c = &m.chunks[0];
        assert_eq!(c.base_text, "class Cart\n");
        assert_eq!(c.left_text, "class ShoppingCart\n");
        assert_eq!(c.right_text, "class Basket\n");
        assert_eq!(c.simple, None);
        assert_eq!((m.changes, m.conflicts, m.simple_conflicts), (0, 1, 0));
        let merged = auto_merge(BASE, &left, &right, true);
        assert_eq!((merged.text.as_str(), merged.unresolved), (BASE, 1));
    }

    #[test]
    fn insertions_at_the_same_place_conflict() {
        let left = BASE.replace(
            "{\n    public function total",
            "{\n    private int $a = 0;\n\n    public function total",
        );
        let right = BASE.replace(
            "{\n    public function total",
            "{\n    private int $b = 1;\n\n    public function total",
        );
        let m = merge_three_way(BASE, &left, &right);
        assert_eq!(m.conflicts, 1);
        let c = &m.chunks[0];
        assert!(c.base.is_empty());
        assert_eq!(
            c.simple, None,
            "two insertions at one place are never simple"
        );
    }

    #[test]
    fn adjacent_insertion_and_change_conflict() {
        // Right inserts a line right where left changes the next one.
        let left = BASE.replace(
            "        return 0;\n    }\n\n    public function count",
            "        return 7;\n    }\n\n    public function count",
        );
        let right = BASE.replace(
            "    {\n        return 0;\n    }\n\n    public function count",
            "    {\n        // total\n        return 0;\n    }\n\n    public function count",
        );
        let m = merge_three_way(BASE, &left, &right);
        assert_eq!(kinds(&m), vec![ChunkKind::Conflict]);
    }

    #[test]
    fn simple_conflicts_merge_different_words_of_a_line() {
        let base = "    $total = $price * $qty;\n";
        let left = "    $total = $netPrice * $qty;\n";
        let right = "    $total = $price * $quantity;\n";
        assert_eq!(
            resolve_simple(base, left, right).as_deref(),
            Some("    $total = $netPrice * $quantity;\n")
        );
        // The same word changed differently is a real conflict.
        assert_eq!(
            resolve_simple(base, left, "    $total = $grossPrice * $qty;\n"),
            None
        );
        // Both sides made the same word change: applied once.
        assert_eq!(
            resolve_simple(base, left, "    $total = $netPrice * $quantity;\n").as_deref(),
            Some("    $total = $netPrice * $quantity;\n")
        );
    }

    #[test]
    fn simple_conflicts_in_the_merge_model() {
        let base = format!("{BASE}// $a = foo($x, $y);\n");
        let left = format!("{BASE}// $a = bar($x, $y);\n");
        let right = format!("{BASE}// $a = foo($x, $z);\n");
        let m = merge_three_way(&base, &left, &right);
        assert_eq!((m.conflicts, m.simple_conflicts), (1, 1));
        assert_eq!(
            m.chunks[0].simple.as_deref(),
            Some("// $a = bar($x, $z);\n")
        );
        let merged = auto_merge(&base, &left, &right, true);
        assert_eq!(merged.unresolved, 0);
        assert!(merged.text.ends_with("// $a = bar($x, $z);\n"));
    }

    #[test]
    fn simple_conflicts_spanning_lines() {
        let base = "a = 1\nb = 2\n";
        // Changes of adjacent lines are separate chunks; merged as one text they are
        // still a simple conflict.
        let left = "a = 10\nb = 2\n";
        let right = "a = 1\nb = 20\n";
        let m = merge_three_way(base, left, right);
        assert_eq!(m.conflicts, 0, "adjacent line changes are separate chunks");
        assert_eq!(
            resolve_simple(base, left, right).as_deref(),
            Some("a = 10\nb = 20\n")
        );
    }

    #[test]
    fn tokenizes_words_blanks_and_punctuation() {
        assert_eq!(
            tokenize("  $user->name ??= 'x';\r\n"),
            vec![
                "  ", "$", "user", "-", ">", "name", " ", "?", "?", "=", " ", "'", "x", "'", ";",
                "\r\n"
            ]
        );
    }

    #[test]
    fn files_without_a_final_line_break() {
        let m = merge_three_way("a\nb", "a\nb\n", "a\nc");
        assert_eq!(kinds(&m), vec![ChunkKind::Conflict]);
        assert_eq!(m.chunks[0].base_text, "b");
    }

    #[test]
    fn line_breaks_round_trip() {
        let (text, crlf) = normalize_eol("a\r\nb\r\n");
        assert_eq!((text.as_str(), crlf), ("a\nb\n", true));
        assert_eq!(restore_eol(&text, true), "a\r\nb\r\n");
        assert_eq!(restore_eol("a\nb", false), "a\nb");
        assert!(looks_binary("PNG\0\u{1}"));
        assert!(!looks_binary("plain text"));
    }
}
