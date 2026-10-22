//! Clipboard history for Paste from History (`Ctrl+Shift+V`): the texts copied or cut in
//! Forge, newest first, like PHPStorm's `CopyPasteManager`.
//!
//! - Copying a text that is already in the history moves it to the top instead of adding a
//!   duplicate.
//! - The history keeps at most `max` entries (Settings | Editor | General | Limits, "Maximum
//!   number of contents to keep in clipboard"); older entries are dropped.
//! - A copy with several carets remembers the text of each caret, so pasting it with the same
//!   number of carets puts one part at each caret.
//!
//! The history lives in memory for the session only; nothing is written to disk.

use serde::{Deserialize, Serialize};

/// Default of "Maximum number of contents to keep in clipboard".
pub const DEFAULT_MAX_ENTRIES: usize = 20;
/// Upper bound for the setting.
pub const MAX_ENTRIES_LIMIT: usize = 100;
/// Characters of the one-line preview in the list.
const PREVIEW_CHARS: usize = 120;

/// One copied text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipEntry {
    /// The copied text, `\n` line breaks.
    pub text: String,
    /// The text of each caret when it was copied with several carets; empty otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub carets: Vec<String>,
}

impl ClipEntry {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            carets: Vec::new(),
        }
    }
}

/// An entry as the Paste from History popup lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipItem {
    /// Position in the history (0 = newest); the id for `remove` and `promote`.
    pub index: usize,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub carets: Vec<String>,
    /// First non-blank line, trimmed and shortened.
    pub preview: String,
    pub lines: usize,
    pub chars: usize,
}

/// Session clipboard history, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardHistory {
    entries: Vec<ClipEntry>,
    max: usize,
}

impl Default for ClipboardHistory {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_ENTRIES)
    }
}

/// Clamp a configured limit to `1..=MAX_ENTRIES_LIMIT`.
pub fn clamp_max(max: usize) -> usize {
    max.clamp(1, MAX_ENTRIES_LIMIT)
}

fn normalize(text: &str) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_string()
    }
}

impl ClipboardHistory {
    pub fn new(max: usize) -> Self {
        Self {
            entries: Vec::new(),
            max: clamp_max(max),
        }
    }

    pub fn max(&self) -> usize {
        self.max
    }

    /// Change the limit, dropping the oldest entries that no longer fit.
    pub fn set_max(&mut self, max: usize) {
        self.max = clamp_max(max);
        self.entries.truncate(self.max);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[ClipEntry] {
        &self.entries
    }

    /// Put a copied text on top. Empty texts are ignored (returns false); a text already in
    /// the history moves to the top with the carets of the new copy.
    pub fn add(&mut self, entry: ClipEntry) -> bool {
        let text = normalize(&entry.text);
        if text.is_empty() {
            return false;
        }
        let carets: Vec<String> = entry.carets.iter().map(|c| normalize(c)).collect();
        // Caret parts only matter when there are several of them.
        let carets = if carets.len() > 1 { carets } else { Vec::new() };
        self.entries.retain(|e| e.text != text);
        self.entries.insert(0, ClipEntry { text, carets });
        self.entries.truncate(self.max);
        true
    }

    /// Remove the entry at `index` (the Delete key in the popup).
    pub fn remove(&mut self, index: usize) -> Option<ClipEntry> {
        (index < self.entries.len()).then(|| self.entries.remove(index))
    }

    /// Move the entry at `index` to the top (it was pasted) and return it.
    pub fn promote(&mut self, index: usize) -> Option<ClipEntry> {
        if index >= self.entries.len() {
            return None;
        }
        let entry = self.entries.remove(index);
        self.entries.insert(0, entry.clone());
        Some(entry)
    }

    /// The entries matching `query` (case-insensitive; every word must occur), newest first.
    pub fn items(&self, query: &str) -> Vec<ClipItem> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                if words.is_empty() {
                    return true;
                }
                let lower = e.text.to_lowercase();
                words.iter().all(|w| lower.contains(w.as_str()))
            })
            .map(|(index, e)| ClipItem {
                index,
                text: e.text.clone(),
                carets: e.carets.clone(),
                preview: preview(&e.text),
                lines: e.text.lines().count().max(1),
                chars: e.text.chars().count(),
            })
            .collect()
    }
}

/// One-line preview: the first non-blank line, trimmed, shortened with "…".
pub fn preview(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let mut out: String = line.chars().take(PREVIEW_CHARS).collect();
    if line.chars().count() > PREVIEW_CHARS {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(h: &ClipboardHistory) -> Vec<&str> {
        h.entries().iter().map(|e| e.text.as_str()).collect()
    }

    #[test]
    fn newest_first_and_dedupes() {
        let mut h = ClipboardHistory::new(10);
        assert!(h.add(ClipEntry::new("a")));
        assert!(h.add(ClipEntry::new("b")));
        assert!(h.add(ClipEntry::new("a")));
        assert_eq!(texts(&h), ["a", "b"]);
    }

    #[test]
    fn ignores_empty_text() {
        let mut h = ClipboardHistory::default();
        assert!(!h.add(ClipEntry::new("")));
        assert!(h.is_empty());
        // Whitespace is a real copy (an indent, a line break).
        assert!(h.add(ClipEntry::new("  ")));
    }

    #[test]
    fn keeps_at_most_max_entries() {
        let mut h = ClipboardHistory::new(3);
        for t in ["1", "2", "3", "4", "5"] {
            h.add(ClipEntry::new(t));
        }
        assert_eq!(texts(&h), ["5", "4", "3"]);
        h.set_max(2);
        assert_eq!(texts(&h), ["5", "4"]);
        h.set_max(0);
        assert_eq!(h.max(), 1);
        assert_eq!(texts(&h), ["5"]);
        h.set_max(10_000);
        assert_eq!(h.max(), MAX_ENTRIES_LIMIT);
    }

    #[test]
    fn normalizes_line_breaks() {
        let mut h = ClipboardHistory::default();
        h.add(ClipEntry::new("a\r\nb"));
        h.add(ClipEntry::new("a\nb"));
        assert_eq!(texts(&h), ["a\nb"]);
    }

    #[test]
    fn keeps_caret_parts_of_multi_caret_copies() {
        let mut h = ClipboardHistory::default();
        h.add(ClipEntry {
            text: "one\ntwo\nthree".into(),
            carets: vec!["one\ntwo".into(), "three".into()],
        });
        assert_eq!(h.entries()[0].carets, ["one\ntwo", "three"]);
        // A single caret has nothing to distribute.
        h.add(ClipEntry {
            text: "x".into(),
            carets: vec!["x".into()],
        });
        assert!(h.entries()[0].carets.is_empty());
        // Copying the same text again with one caret replaces the parts.
        h.add(ClipEntry::new("one\ntwo\nthree"));
        assert_eq!(texts(&h), ["one\ntwo\nthree", "x"]);
        assert!(h.entries()[0].carets.is_empty());
    }

    #[test]
    fn removes_and_promotes() {
        let mut h = ClipboardHistory::default();
        for t in ["a", "b", "c"] {
            h.add(ClipEntry::new(t));
        }
        assert_eq!(h.promote(2).unwrap().text, "a");
        assert_eq!(texts(&h), ["a", "c", "b"]);
        assert_eq!(h.remove(1).unwrap().text, "c");
        assert_eq!(texts(&h), ["a", "b"]);
        assert!(h.remove(5).is_none());
        assert!(h.promote(5).is_none());
    }

    #[test]
    fn filters_items_by_every_word() {
        let mut h = ClipboardHistory::default();
        h.add(ClipEntry::new("public function index()"));
        h.add(ClipEntry::new("return view('users');"));
        h.add(ClipEntry::new("$user->save();"));
        let all = h.items("");
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].index, 0);
        let found = h.items("FUNCTION  idx");
        assert!(found.is_empty());
        let found = h.items("function index");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].index, 2);
        assert_eq!(h.items("user").len(), 2);
    }

    #[test]
    fn previews_first_non_blank_line() {
        assert_eq!(preview("\n   \n    foo();\nbar"), "foo();");
        assert_eq!(preview(""), "");
        let long = "x".repeat(200);
        let p = preview(&long);
        assert_eq!(p.chars().count(), PREVIEW_CHARS + 1);
        assert!(p.ends_with('…'));
        let mut h = ClipboardHistory::default();
        h.add(ClipEntry::new("a\nb\n"));
        let item = &h.items("")[0];
        assert_eq!((item.lines, item.chars), (2, 4));
    }
}
