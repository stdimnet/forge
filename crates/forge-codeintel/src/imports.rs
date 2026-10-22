//! Optimize Imports (Ctrl+Alt+O) and Auto Import, the parts every language shares: the
//! notifications ("Removed 2 imports, rearranged imports", "Imports are already
//! optimized"), the editor change for an optimized text, and when a language server's
//! on-the-fly import edit may be applied (only away from what the user is editing).
//! PHP computes the import changes itself (`phpforge_lang_php::imports`); JavaScript,
//! TypeScript and Vue use the TypeScript server's `source.organizeImports`,
//! `source.addMissingImports` and `source.removeUnusedImports` actions.

use serde::{Deserialize, Serialize};

use crate::edit::{position_at, Range, TextEdit};
use crate::Position;

/// What Optimize Imports did to the file in the editor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeAnswer {
    /// The change to the editor's text (one replacement), `None` when nothing changes.
    pub edit: Option<TextEdit>,
    /// The notification.
    pub message: String,
}

impl OptimizeAnswer {
    pub fn new(old: &str, new: Option<&str>, message: String) -> Self {
        Self {
            edit: new.and_then(|n| minimal_edit(old, n)),
            message,
        }
    }
}

/// PHPStorm's notification after Optimize Imports: "Removed 2 imports, rearranged
/// imports", "Added 1 import", "Imports are already optimized".
pub fn summary(removed: usize, rearranged: bool, added: usize) -> String {
    let plural = |n: usize| if n == 1 { "import" } else { "imports" };
    let mut parts = Vec::new();
    if removed > 0 {
        parts.push(format!("removed {removed} {}", plural(removed)));
    }
    if added > 0 {
        parts.push(format!("added {added} {}", plural(added)));
    }
    if rearranged {
        parts.push("rearranged imports".to_string());
    }
    if parts.is_empty() {
        return "Imports are already optimized".into();
    }
    capitalize(&parts.join(", "))
}

/// The notification after optimizing the imports of a folder.
pub fn folder_summary(files: usize, changed: usize, removed: usize) -> String {
    let plural = |n: usize, one: &str, many: &str| {
        if n == 1 {
            format!("{n} {one}")
        } else {
            format!("{n} {many}")
        }
    };
    if changed == 0 {
        return format!(
            "Imports are already optimized ({} checked)",
            plural(files, "file", "files")
        );
    }
    let mut text = format!("Optimized imports in {}", plural(changed, "file", "files"));
    if removed > 0 {
        text.push_str(&format!(
            ": removed {}",
            plural(removed, "import", "imports")
        ));
    }
    text
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// One replacement turning `old` into `new` (the changed middle part; LSP positions on
/// `old`), so the editor keeps the caret and the scroll position. `None` when equal.
pub fn minimal_edit(old: &str, new: &str) -> Option<TextEdit> {
    if old == new {
        return None;
    }
    let common = |a: &mut dyn Iterator<Item = (char, char)>| -> usize {
        a.take_while(|(x, y)| x == y)
            .map(|(x, _)| x.len_utf8())
            .sum()
    };
    let prefix = common(&mut old.chars().zip(new.chars()));
    let suffix = common(&mut old[prefix..].chars().rev().zip(new[prefix..].chars().rev()));
    let units = |s: &str| s.encode_utf16().count();
    let start = units(&old[..prefix]);
    let end = units(&old[..old.len() - suffix]);
    Some(TextEdit {
        range: Range::new(position_at(old, start), position_at(old, end)),
        new_text: new[prefix..new.len() - suffix].to_string(),
    })
}

/// Whether an on-the-fly import edit (a language server's "add missing imports" or
/// "remove unused imports") may be applied now: not when it changes the caret's line or
/// the lines edited last, where the user is still typing.
pub fn on_the_fly_allowed(edits: &[TextEdit], caret: Position, edited: Option<Range>) -> bool {
    let busy = |line: u32| {
        line == caret.line || edited.is_some_and(|r| r.start.line <= line && line <= r.end.line)
    };
    edits.iter().all(|e| {
        let (from, to) = (e.range.start.line, e.range.end.line);
        // A pure insertion at the start of a line does not touch that line's text.
        let to = if e.range.end.character == 0 && to > from {
            to - 1
        } else {
            to
        };
        !(from..=to).any(busy)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::apply_text_edits;

    #[test]
    fn summaries() {
        assert_eq!(summary(0, false, 0), "Imports are already optimized");
        assert_eq!(summary(1, false, 0), "Removed 1 import");
        assert_eq!(summary(2, true, 0), "Removed 2 imports, rearranged imports");
        assert_eq!(summary(0, true, 3), "Added 3 imports, rearranged imports");
        assert_eq!(
            folder_summary(4, 0, 0),
            "Imports are already optimized (4 files checked)"
        );
        assert_eq!(
            folder_summary(4, 2, 3),
            "Optimized imports in 2 files: removed 3 imports"
        );
        assert_eq!(folder_summary(1, 1, 0), "Optimized imports in 1 file");
    }

    #[test]
    fn minimal_edit_replaces_only_the_changed_part() {
        let old = "<?php\nuse B;\nuse A;\n\nclass ü {}\n";
        let new = "<?php\nuse A;\nuse B;\n\nclass ü {}\n";
        let e = minimal_edit(old, new).unwrap();
        assert_eq!(e.range.start, Position::new(1, 4));
        assert_eq!(e.range.end, Position::new(2, 5));
        assert_eq!(apply_text_edits(old, &[e]).unwrap(), new);
        assert!(minimal_edit(old, old).is_none());
        // Removal at the end, repeated characters, multi-byte characters.
        for (a, b) in [
            ("aaa", "aa"),
            ("abc", "abcabc"),
            ("xäy", "xöy"),
            ("", "a"),
            ("a", ""),
        ] {
            let e = minimal_edit(a, b).unwrap();
            assert_eq!(apply_text_edits(a, &[e]).unwrap(), b, "{a} -> {b}");
        }
    }

    #[test]
    fn on_the_fly_edits_stay_away_from_the_caret_and_the_last_edit() {
        let insert_at = |line: u32| TextEdit {
            range: Range::new(Position::new(line, 0), Position::new(line, 0)),
            new_text: "import { a } from \"a\";\n".into(),
        };
        let remove_line = |line: u32| TextEdit {
            range: Range::new(Position::new(line, 0), Position::new(line + 1, 0)),
            new_text: String::new(),
        };
        let caret = Position::new(10, 4);
        assert!(on_the_fly_allowed(&[insert_at(0)], caret, None));
        assert!(on_the_fly_allowed(&[remove_line(2)], caret, None));
        assert!(!on_the_fly_allowed(&[remove_line(10)], caret, None));
        let edited = Range::new(Position::new(2, 0), Position::new(3, 5));
        assert!(!on_the_fly_allowed(&[remove_line(2)], caret, Some(edited)));
        assert!(on_the_fly_allowed(&[remove_line(5)], caret, Some(edited)));
    }
}
