//! Minimal edits between two texts, in UTF-16 offsets.
//!
//! When a shared file changes on disk (git checkout, a formatter, an agent), the host applies
//! the difference to the shared document instead of replacing it, so everyone's carets and
//! selections stay where they are. Offsets count UTF-16 code units, like JavaScript, Yjs
//! and the host's documents (`OffsetKind::Utf16`).
//!
//! The diff runs on lines first (common prefix and suffix are cut off before) and refines
//! replaced lines by characters, so a one-character change in a large file is one small edit.

use similar::{DiffOp, TextDiff, TextDiffConfig};

/// Replace `delete` UTF-16 units at `at` (an offset in the old text) with `insert`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub at: usize,
    pub delete: usize,
    pub insert: String,
}

/// Replaced hunks larger than this (bytes, old + new) are not refined by characters.
const CHAR_DIFF_LIMIT: usize = 64 * 1024;

/// Edits that turn `old` into `new`, ascending and non-overlapping, with offsets into `old`.
/// Apply them from the last to the first so earlier offsets stay valid.
pub fn edits(old: &str, new: &str) -> Vec<Edit> {
    let mut out = Vec::new();
    let mut utf16 = Utf16Cursor::new(old);
    for (start, end, insert) in byte_edits(old, new) {
        let at = utf16.offset(start);
        let delete = utf16.offset(end) - at;
        out.push(Edit { at, delete, insert });
    }
    out
}

/// Apply [`edits`] to a UTF-16 buffer (used by tests and by callers without a document).
pub fn apply(old: &str, edits: &[Edit]) -> String {
    let mut units: Vec<u16> = old.encode_utf16().collect();
    for e in edits.iter().rev() {
        units.splice(e.at..e.at + e.delete, e.insert.encode_utf16());
    }
    String::from_utf16(&units).expect("edits keep surrogate pairs together")
}

/// Turns ascending byte offsets of one text into UTF-16 offsets in one pass.
struct Utf16Cursor<'a> {
    text: &'a str,
    byte: usize,
    unit: usize,
}

impl<'a> Utf16Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            byte: 0,
            unit: 0,
        }
    }

    fn offset(&mut self, byte: usize) -> usize {
        debug_assert!(byte >= self.byte, "offsets must ascend");
        self.unit += self.text[self.byte..byte].encode_utf16().count();
        self.byte = byte;
        self.unit
    }
}

fn common_prefix(a: &str, b: &str) -> usize {
    a.char_indices()
        .zip(b.chars())
        .find(|((_, x), y)| x != y)
        .map(|((i, _), _)| i)
        .unwrap_or_else(|| a.len().min(b.len()))
}

fn common_suffix(a: &str, b: &str) -> usize {
    let mut n = 0;
    for (x, y) in a.chars().rev().zip(b.chars().rev()) {
        if x != y {
            break;
        }
        n += x.len_utf8();
    }
    n
}

/// Byte offset where each slice starts, plus the total length at the end.
fn starts(slices: &[&str]) -> Vec<usize> {
    let mut out = Vec::with_capacity(slices.len() + 1);
    let mut pos = 0;
    out.push(0);
    for s in slices {
        pos += s.len();
        out.push(pos);
    }
    out
}

/// `(old_start, old_end, insert)` in bytes of `old`, ascending.
fn byte_edits(old: &str, new: &str) -> Vec<(usize, usize, String)> {
    if old == new {
        return Vec::new();
    }
    let prefix = common_prefix(old, new);
    let suffix = common_suffix(&old[prefix..], &new[prefix..]);
    let old_mid = &old[prefix..old.len() - suffix];
    let new_mid = &new[prefix..new.len() - suffix];
    if old_mid.is_empty() || new_mid.is_empty() {
        return vec![(prefix, prefix + old_mid.len(), new_mid.to_string())];
    }
    let mut out = Vec::new();
    diff_slices(
        config().diff_lines(old_mid, new_mid),
        prefix,
        true,
        &mut out,
    );
    out
}

/// Diffs of very different large texts give up on minimality after a while; the result is
/// still correct, just coarser. (WebAssembly has no clock, so it has no deadline.)
fn config() -> TextDiffConfig {
    #[allow(unused_mut)]
    let mut c = TextDiff::configure();
    #[cfg(not(target_arch = "wasm32"))]
    c.deadline(std::time::Instant::now() + std::time::Duration::from_millis(300));
    c
}

fn diff_slices(
    diff: TextDiff<'_, '_, '_, str>,
    base: usize,
    refine: bool,
    out: &mut Vec<(usize, usize, String)>,
) {
    let old_at = starts(diff.old_slices());
    let new_at = starts(diff.new_slices());
    let old_text: String = diff.old_slices().concat();
    let new_text: String = diff.new_slices().concat();
    // Positions follow a cursor over the old text: an insert's `old_index` may point into a
    // range that the previous op deleted.
    let mut cursor = 0;
    for op in diff.ops() {
        let (o, n) = (op.old_range(), op.new_range());
        let os = cursor;
        let oe = os + (old_at[o.end] - old_at[o.start]);
        let (ns, ne) = (new_at[n.start], new_at[n.end]);
        cursor = if matches!(op, DiffOp::Equal { .. }) {
            old_at[o.end]
        } else {
            oe
        };
        match op {
            DiffOp::Equal { .. } => {}
            DiffOp::Replace { .. } if refine && (oe - os) + (ne - ns) <= CHAR_DIFF_LIMIT => {
                let (a, b) = (&old_text[os..oe], &new_text[ns..ne]);
                let p = common_prefix(a, b);
                let s = common_suffix(&a[p..], &b[p..]);
                let (am, bm) = (&a[p..a.len() - s], &b[p..b.len() - s]);
                if am.is_empty() || bm.is_empty() {
                    out.push((base + os + p, base + os + p + am.len(), bm.to_string()));
                } else {
                    diff_slices(config().diff_chars(am, bm), base + os + p, false, out);
                }
            }
            _ => out.push((base + os, base + oe, new_text[ns..ne].to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(old: &str, new: &str) -> Vec<Edit> {
        let e = edits(old, new);
        assert_eq!(apply(old, &e), new, "{old:?} -> {new:?} with {e:?}");
        for w in e.windows(2) {
            assert!(w[0].at + w[0].delete <= w[1].at, "overlapping {e:?}");
        }
        e
    }

    #[test]
    fn equal_texts_need_no_edits() {
        assert!(check("same\n", "same\n").is_empty());
    }

    #[test]
    fn one_character_change_is_one_small_edit() {
        let old = "<?php\n\nclass OrderController\n{\n    public function index() {}\n}\n";
        let new = old.replace("index", "indexx");
        let e = check(old, &new);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].delete, 0);
        assert_eq!(e[0].insert, "x");
    }

    #[test]
    fn offsets_count_utf16_units() {
        let e = check("a😀b", "a😀cb");
        assert_eq!(
            e,
            vec![Edit {
                at: 3,
                delete: 0,
                insert: "c".into()
            }]
        );
        check("😀😀", "😀");
        check("é\r\nx", "é\nx");
    }

    #[test]
    fn lines_are_inserted_deleted_and_replaced() {
        check("a\nb\nc\n", "a\nc\n");
        check("a\nc\n", "a\nb\nc\n");
        check("one\ntwo\nthree\n", "one\nTWO\nthree\nfour\n");
        check("", "new file\n");
        check("old file\n", "");
        check("x", "y");
    }

    #[test]
    fn large_hunks_are_replaced_whole() {
        let old: String = (0..5000).map(|i| format!("line {i}\n")).collect();
        let new: String = (0..5000).map(|i| format!("LINE {}\n", i * 7)).collect();
        check(&old, &new);
    }

    mod props {
        use super::super::*;
        use proptest::prelude::*;

        /// Text with ASCII, CRLF, umlauts and characters outside the BMP (surrogate pairs).
        fn text() -> impl Strategy<Value = String> {
            proptest::collection::vec(
                prop_oneof![
                    Just("a"),
                    Just("b"),
                    Just("\n"),
                    Just("\r\n"),
                    Just("ü"),
                    Just("😀"),
                    Just("𝄞"),
                    Just(" "),
                    Just("fn x() {}\n"),
                ],
                0..60,
            )
            .prop_map(|parts| parts.concat())
        }

        proptest! {
            #[test]
            fn applying_the_diff_yields_the_new_text(old in text(), new in text()) {
                let e = edits(&old, &new);
                prop_assert_eq!(apply(&old, &e), new);
            }

            #[test]
            fn small_changes_of_a_text_stay_small(base in text(), at in 0usize..60, ins in text()) {
                let cut = base.char_indices().map(|(i, _)| i).nth(at).unwrap_or(base.len());
                let new = format!("{}{ins}{}", &base[..cut], &base[cut..]);
                let e = edits(&base, &new);
                prop_assert_eq!(apply(&base, &e), new);
                let deleted: usize = e.iter().map(|x| x.delete).sum();
                prop_assert!(deleted <= ins.encode_utf16().count() + 4);
            }
        }
    }
}
