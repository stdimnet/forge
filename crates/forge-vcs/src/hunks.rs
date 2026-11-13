//! Change hunks of one file for partial commits, like PHPStorm's line status markers: the
//! working copy is compared with the committed version line by line and every run of changed
//! lines becomes a hunk that can be included in or excluded from a commit, or moved to
//! another changelist.
//!
//! Hunk ids stay the same while other parts of the file are edited: they are made of the
//! hunk's position in the *base* (committed) version and its content.

use serde::{Deserialize, Serialize};
use similar::{capture_diff_slices, Algorithm, DiffOp};

/// One run of changed lines. Line numbers are 0-based; a pure insertion has `old_len == 0`
/// and a pure deletion `new_len == 0`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeHunk {
    /// Stable id: base position and a hash of the removed and added lines.
    pub id: String,
    pub old_start: usize,
    pub old_len: usize,
    pub new_start: usize,
    pub new_len: usize,
    /// The base lines the hunk replaces, without line breaks.
    pub removed: Vec<String>,
    /// The lines the hunk adds, without line breaks.
    pub added: Vec<String>,
}

/// Lines with their line breaks (the last one may have none).
fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

fn strip_eol(line: &str) -> &str {
    line.strip_suffix('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .unwrap_or(line)
}

/// FNV-1a, enough to tell hunks apart (ids are not security relevant).
fn fnv(parts: &[&str]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in parts {
        for b in p.bytes().chain(std::iter::once(0xff)) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// The hunks that turn `base` into `current`, in order. Lines are compared without their
/// line break style, so a file whose separators changed is not one big hunk.
pub fn change_hunks(base: &str, current: &str) -> Vec<ChangeHunk> {
    let old = lines(base);
    let new = lines(current);
    let old_keys = keys(&old);
    let new_keys = keys(&new);
    let mut out: Vec<ChangeHunk> = Vec::new();
    for op in capture_diff_slices(Algorithm::Myers, &old_keys, &new_keys) {
        let (o, ol, n, nl) = match op {
            DiffOp::Equal { .. } => continue,
            DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => (old_index, old_len, new_index, 0),
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => (old_index, 0, new_index, new_len),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => (old_index, old_len, new_index, new_len),
        };
        // Adjacent operations (a delete followed by an insert) form one hunk.
        if let Some(last) = out.last_mut() {
            if last.old_start + last.old_len == o && last.new_start + last.new_len == n {
                last.old_len += ol;
                last.new_len += nl;
                continue;
            }
        }
        out.push(ChangeHunk {
            id: String::new(),
            old_start: o,
            old_len: ol,
            new_start: n,
            new_len: nl,
            removed: Vec::new(),
            added: Vec::new(),
        });
    }
    for h in &mut out {
        h.removed = old[h.old_start..h.old_start + h.old_len]
            .iter()
            .map(|l| strip_eol(l).to_string())
            .collect();
        h.added = new[h.new_start..h.new_start + h.new_len]
            .iter()
            .map(|l| strip_eol(l).to_string())
            .collect();
        let mut parts: Vec<&str> = h.removed.iter().map(String::as_str).collect();
        parts.push("\u{1}");
        parts.extend(h.added.iter().map(String::as_str));
        h.id = format!("{}-{:016x}", h.old_start, fnv(&parts));
    }
    out
}

/// Comparison keys: lines without their line break style. A last line without a line break
/// differs from the same line with one (so lines never run together when hunks are applied).
fn keys(lines: &[&str]) -> Vec<String> {
    lines
        .iter()
        .map(|l| {
            if l.ends_with('\n') {
                strip_eol(l).to_string()
            } else {
                format!("{l}\u{0}")
            }
        })
        .collect()
}

/// `base` with only the hunks whose id is in `include` applied: what a partial commit
/// stores for the file. Line breaks of the applied lines come from `current`.
pub fn apply_hunks(base: &str, current: &str, include: &[String]) -> String {
    let old = lines(base);
    let new = lines(current);
    let mut out = String::with_capacity(base.len().max(current.len()));
    let mut pos = 0usize;
    for h in change_hunks(base, current) {
        old[pos..h.old_start].iter().for_each(|l| out.push_str(l));
        let src = if include.contains(&h.id) {
            &new[h.new_start..h.new_start + h.new_len]
        } else {
            &old[h.old_start..h.old_start + h.old_len]
        };
        src.iter().for_each(|l| out.push_str(l));
        pos = h.old_start + h.old_len;
    }
    old[pos..].iter().for_each(|l| out.push_str(l));
    out
}

/// The ids of all hunks of a file.
pub fn hunk_ids(base: &str, current: &str) -> Vec<String> {
    change_hunks(base, current)
        .into_iter()
        .map(|h| h.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "a\nb\nc\nd\ne\nf\ng\nh\n";
    const CURRENT: &str = "a\nB\nc\nd\ne\nf\nh\ni\n";

    #[test]
    fn splits_changes_into_hunks() {
        let hunks = change_hunks(BASE, CURRENT);
        let ranges: Vec<_> = hunks
            .iter()
            .map(|h| (h.old_start, h.old_len, h.new_start, h.new_len))
            .collect();
        assert_eq!(ranges, vec![(1, 1, 1, 1), (6, 1, 6, 0), (8, 0, 7, 1)]);
        assert_eq!(hunks[0].removed, vec!["b"]);
        assert_eq!(hunks[0].added, vec!["B"]);
        assert_eq!(hunks[2].added, vec!["i"]);
        assert!(change_hunks(BASE, BASE).is_empty());
        assert_eq!(change_hunks("", "x\n").len(), 1);
    }

    #[test]
    fn ids_survive_edits_elsewhere() {
        let first = change_hunks(BASE, CURRENT);
        let edited = change_hunks(BASE, "a\nB\nc\nd\nE\nf\nh\ni\n");
        assert_eq!(edited.len(), 4);
        assert_eq!(edited[0].id, first[0].id);
        assert_eq!(edited[3].id, first[2].id);
        assert_ne!(edited[1].id, first[1].id);
    }

    #[test]
    fn applies_only_included_hunks() {
        let hunks = change_hunks(BASE, CURRENT);
        assert_eq!(apply_hunks(BASE, CURRENT, &[]), BASE);
        let all: Vec<String> = hunks.iter().map(|h| h.id.clone()).collect();
        assert_eq!(apply_hunks(BASE, CURRENT, &all), CURRENT);
        assert_eq!(
            apply_hunks(BASE, CURRENT, std::slice::from_ref(&hunks[0].id)),
            "a\nB\nc\nd\ne\nf\ng\nh\n"
        );
        assert_eq!(
            apply_hunks(BASE, CURRENT, &[hunks[1].id.clone(), hunks[2].id.clone()]),
            "a\nb\nc\nd\ne\nf\nh\ni\n"
        );
    }

    #[test]
    fn keeps_crlf_and_ignores_separator_changes() {
        let base = "a\r\nb\r\nc\r\n";
        let current = "a\r\nB\r\nc\r\n";
        let hunks = change_hunks(base, current);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].added, vec!["B"]);
        assert_eq!(
            apply_hunks(base, current, &hunk_ids(base, current)),
            current
        );
        // Only the separators changed: no hunks.
        assert!(change_hunks("a\nb\n", "a\r\nb\r\n").is_empty());
    }

    #[test]
    fn handles_a_missing_final_line_break() {
        let hunks = change_hunks("a\nb", "a\nb\nc\n");
        assert_eq!(hunks.len(), 1);
        assert_eq!(
            apply_hunks("a\nb", "a\nb\nc\n", &hunk_ids("a\nb", "a\nb\nc\n")),
            "a\nb\nc\n"
        );
        assert_eq!(apply_hunks("a\nb", "a\nb\nc\n", &[]), "a\nb");
    }
}
