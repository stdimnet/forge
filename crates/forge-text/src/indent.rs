//! Indentation: the style a file is edited with and detection from the file's content
//! (PHPStorm's "Detect and use existing file indents for editing").

use serde::{Deserialize, Serialize};

/// How a file is indented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndentStyle {
    /// Indent with tab characters.
    pub use_tabs: bool,
    /// Columns per indent level.
    pub size: u8,
    /// Width of a tab character.
    pub tab_size: u8,
    /// Extra indent of continuation lines (method chains, wrapped arguments).
    pub continuation: u8,
}

impl Default for IndentStyle {
    fn default() -> Self {
        Self {
            use_tabs: false,
            size: 4,
            tab_size: 4,
            continuation: 4,
        }
    }
}

impl IndentStyle {
    pub fn spaces(size: u8) -> Self {
        Self {
            use_tabs: false,
            size,
            tab_size: size.max(1),
            continuation: size,
        }
    }

    /// Status bar text: `Tab`, `4 spaces`, `1 space`.
    pub fn label(&self) -> String {
        if self.use_tabs {
            "Tab".into()
        } else if self.size == 1 {
            "1 space".into()
        } else {
            format!("{} spaces", self.size)
        }
    }

    /// One indent level as text (CodeMirror's `indentUnit`).
    pub fn unit(&self) -> String {
        if self.use_tabs {
            "\t".into()
        } else {
            " ".repeat(self.size.max(1) as usize)
        }
    }

    /// The same indentation kind (tabs or spaces and size) as `other`.
    pub fn same_kind(&self, other: &IndentStyle) -> bool {
        self.use_tabs == other.use_tabs && (self.use_tabs || self.size == other.size)
    }
}

/// Lines needed before a guess is trusted.
const MIN_INDENTED_LINES: usize = 2;
/// Lines looked at (detection runs on open; big files only need their head).
const MAX_LINES: usize = 10_000;

/// Detect the indentation of `text`: tabs or spaces, and for spaces the most common step
/// between nested lines. `None` when the file has too few indented lines to tell.
/// `fallback` provides the tab size and continuation indent.
pub fn detect_indent(text: &str, fallback: IndentStyle) -> Option<IndentStyle> {
    let mut tabs = 0usize;
    let mut spaced = 0usize;
    // Histogram of indent steps between consecutive non-blank lines (index = step).
    let mut steps = [0usize; 9];
    let mut prev_spaces = 0usize;
    let mut in_block_comment = false;
    for line in text.lines().take(MAX_LINES) {
        let trimmed = line.trim_start_matches([' ', '\t']);
        if trimmed.is_empty() {
            continue;
        }
        let lead = &line[..line.len() - trimmed.len()];
        // ` * doc` lines inside block comments are aligned by one space; skip them.
        let comment_line = in_block_comment || trimmed.starts_with('*');
        if trimmed.starts_with("/*") && !trimmed.contains("*/") {
            in_block_comment = true;
        } else if in_block_comment && trimmed.contains("*/") {
            in_block_comment = false;
        }
        if comment_line {
            continue;
        }
        if lead.starts_with('\t') {
            tabs += 1;
            continue;
        }
        let spaces = lead.chars().take_while(|c| *c == ' ').count();
        if spaces > 0 {
            spaced += 1;
        }
        let step = spaces.abs_diff(prev_spaces);
        if (2..=8).contains(&step) {
            steps[step] += 1;
        }
        prev_spaces = spaces;
    }
    if tabs + spaced < MIN_INDENTED_LINES {
        return None;
    }
    if tabs > spaced {
        return Some(IndentStyle {
            use_tabs: true,
            ..fallback
        });
    }
    // The most frequent step; steps that divide a frequent bigger step get its votes too
    // (2-space files have many 4-column jumps after closing blocks), ties pick the larger.
    let mut best = (0usize, 0usize);
    for size in [2usize, 3, 4, 8] {
        let mut votes = steps[size];
        if size * 2 <= 8 {
            votes += steps[size * 2] / 2;
        }
        if votes > best.0 || (votes == best.0 && votes > 0 && size == 4) {
            best = (votes, size);
        }
    }
    if best.0 == 0 {
        return None;
    }
    Some(IndentStyle {
        use_tabs: false,
        size: best.1 as u8,
        ..fallback
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHP4: &str =
        "<?php\n\nclass A\n{\n    public function x()\n    {\n        return 1;\n    }\n}\n";
    const JS2: &str = "function a() {\n  if (x) {\n    y();\n  }\n  return 1;\n}\n";

    #[test]
    fn detects_spaces_and_their_size() {
        let d = detect_indent(PHP4, IndentStyle::default()).unwrap();
        assert_eq!((d.use_tabs, d.size), (false, 4));
        let d = detect_indent(JS2, IndentStyle::default()).unwrap();
        assert_eq!((d.use_tabs, d.size, d.tab_size), (false, 2, 4));
    }

    #[test]
    fn detects_tabs() {
        let d =
            detect_indent("a {\n\tb;\n\tc {\n\t\td;\n\t}\n}\n", IndentStyle::spaces(2)).unwrap();
        assert!(d.use_tabs);
        assert_eq!(d.label(), "Tab");
        assert_eq!(d.unit(), "\t");
    }

    #[test]
    fn ignores_doc_comment_alignment_and_needs_evidence() {
        let text = "/**\n * Doc\n * more\n */\nfunction a() {\n    b();\n    c();\n}\n";
        assert_eq!(detect_indent(text, IndentStyle::default()).unwrap().size, 4);
        assert_eq!(detect_indent("a\nb\n", IndentStyle::default()), None);
        assert_eq!(detect_indent("a\n  b\n", IndentStyle::default()), None);
    }

    #[test]
    fn labels() {
        assert_eq!(IndentStyle::spaces(4).label(), "4 spaces");
        assert_eq!(IndentStyle::spaces(1).label(), "1 space");
        assert_eq!(IndentStyle::spaces(2).unit(), "  ");
        assert!(IndentStyle::spaces(2).same_kind(&IndentStyle {
            tab_size: 8,
            ..IndentStyle::spaces(2)
        }));
    }
}
