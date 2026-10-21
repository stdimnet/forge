//! PHP code style (Settings | Editor | Code Style | PHP) beyond indentation: brace placement
//! and blank lines. Indentation is resolved for every language in `forge-text`.
//!
//! - Brace placement is passed to Intelephense's formatter (`intelephense.format.braces`) and
//!   used by the "Generate" actions.
//! - Blank lines are applied by [`tidy_blank_lines`] after Reformat Code (Ctrl+Alt+L): runs of
//!   blank lines are cut to "Keep maximum blank lines", and the blank lines after the
//!   namespace declaration, after the imports and around methods are set. Strings, heredocs,
//!   comments and inline HTML are never changed.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tree_sitter::Node;

use crate::parser::{new_parser, parse_tree};

/// Setting keys (see `app/src-tauri/plugins/php.toml`).
pub const BRACES_KEY: &str = "php.codeStyle.braces";
pub const KEEP_MAX_KEY: &str = "php.codeStyle.blankLines.keepMax";
pub const AFTER_NAMESPACE_KEY: &str = "php.codeStyle.blankLines.afterNamespace";
pub const AFTER_IMPORTS_KEY: &str = "php.codeStyle.blankLines.afterImports";
pub const AROUND_METHODS_KEY: &str = "php.codeStyle.blankLines.aroundMethods";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Braces {
    /// PSR-12: next line for classes and functions, end of line for control structures.
    #[default]
    Psr12,
    /// Next line everywhere.
    Allman,
    /// End of line everywhere.
    #[serde(rename = "kr")]
    KAndR,
}

impl Braces {
    /// Intelephense's `format.braces` value.
    pub fn intelephense(self) -> &'static str {
        match self {
            Braces::Psr12 => "psr12",
            Braces::Allman => "allman",
            Braces::KAndR => "k&r",
        }
    }

    /// Function and method bodies open on their own line.
    pub fn functions_on_next_line(self) -> bool {
        self != Braces::KAndR
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PhpCodeStyle {
    pub braces: Braces,
    pub keep_max_blank_lines: usize,
    pub blank_lines_after_namespace: usize,
    pub blank_lines_after_imports: usize,
    pub blank_lines_around_methods: usize,
}

impl Default for PhpCodeStyle {
    fn default() -> Self {
        Self {
            braces: Braces::Psr12,
            keep_max_blank_lines: 2,
            blank_lines_after_namespace: 1,
            blank_lines_after_imports: 1,
            blank_lines_around_methods: 1,
        }
    }
}

impl PhpCodeStyle {
    /// The style from Forge's settings store (`get` returns the effective value of a key).
    pub fn from_settings(get: impl Fn(&str) -> Option<Value>) -> Self {
        let d = Self::default();
        let num = |key: &str, default: usize| {
            get(key)
                .and_then(|v| v.as_u64())
                .map_or(default, |n| n.min(10) as usize)
        };
        Self {
            braces: get(BRACES_KEY)
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or(d.braces),
            keep_max_blank_lines: num(KEEP_MAX_KEY, d.keep_max_blank_lines),
            blank_lines_after_namespace: num(AFTER_NAMESPACE_KEY, d.blank_lines_after_namespace),
            blank_lines_after_imports: num(AFTER_IMPORTS_KEY, d.blank_lines_after_imports),
            blank_lines_around_methods: num(AROUND_METHODS_KEY, d.blank_lines_around_methods),
        }
    }
}

/// Nodes whose text must stay as it is.
const VERBATIM: &[&str] = &[
    "string",
    "encapsed_string",
    "heredoc",
    "nowdoc",
    "comment",
    "text",
    "shell_command_expression",
];

/// A gap between two nodes whose blank lines are set: exactly `min == max` blank lines, or
/// at least `min` and at most `max`.
struct Gap {
    start: usize,
    end: usize,
    min: usize,
    max: usize,
}

fn collect(
    node: Node<'_>,
    verbatim: &mut Vec<(usize, usize)>,
    gaps: &mut Vec<Gap>,
    style: &PhpCodeStyle,
) {
    if VERBATIM.contains(&node.kind()) {
        verbatim.push((node.start_byte(), node.end_byte()));
        return;
    }
    let mut cursor = node.walk();
    let children: Vec<Node<'_>> = node.named_children(&mut cursor).collect();
    for pair in children.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let blank = |n: usize| Some((n, n));
        let rule = match (a.kind(), b.kind()) {
            // `namespace App;` followed by anything (the braced form has a body instead).
            ("namespace_definition", _) if a.child_by_field_name("body").is_none() => {
                blank(style.blank_lines_after_namespace)
            }
            ("namespace_use_declaration", "namespace_use_declaration") => None,
            ("namespace_use_declaration", _) => blank(style.blank_lines_after_imports),
            ("method_declaration", _) | (_, "method_declaration")
                if node.kind() == "declaration_list" =>
            {
                Some((
                    style.blank_lines_around_methods,
                    style
                        .keep_max_blank_lines
                        .max(style.blank_lines_around_methods),
                ))
            }
            _ => None,
        };
        if let Some((min, max)) = rule {
            gaps.push(Gap {
                start: a.end_byte(),
                end: b.start_byte(),
                min,
                max,
            });
        }
    }
    for child in children {
        collect(child, verbatim, gaps, style);
    }
}

/// Apply the blank line settings to a PHP file. `None` when nothing changes (or the file
/// cannot be parsed).
pub fn tidy_blank_lines(src: &str, style: &PhpCodeStyle) -> Option<String> {
    let mut parser = new_parser();
    let tree = parse_tree(&mut parser, src)?;
    let mut verbatim = Vec::new();
    let mut gaps = Vec::new();
    collect(tree.root_node(), &mut verbatim, &mut gaps, style);
    let protected = |at: usize| verbatim.iter().any(|(s, e)| *s < at && at < *e);

    // Structural gaps first: only gaps of pure whitespace that span lines.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for g in &gaps {
        let gap = &src[g.start..g.end];
        if !gap.chars().all(char::is_whitespace) || !gap.contains('\n') {
            continue;
        }
        let blank = gap.matches('\n').count() - 1;
        let want = blank.clamp(g.min, g.max);
        if want != blank {
            let indent = &gap[gap.rfind('\n').map_or(0, |i| i + 1)..];
            let eol = if gap.contains("\r\n") { "\r\n" } else { "\n" };
            edits.push((g.start, g.end, format!("{}{indent}", eol.repeat(want + 1))));
        }
    }

    // Then every other run of blank lines longer than "Keep maximum blank lines".
    let lines: Vec<(usize, &str)> = {
        let mut at = 0;
        src.split_inclusive('\n')
            .map(|l| {
                let start = at;
                at += l.len();
                (start, l)
            })
            .collect()
    };
    let mut i = 0;
    while i < lines.len() {
        let is_blank = |(s, l): (usize, &str)| l.trim().is_empty() && !protected(s);
        if !is_blank(lines[i]) {
            i += 1;
            continue;
        }
        let first = i;
        while i < lines.len() && is_blank(lines[i]) {
            i += 1;
        }
        // Trailing blank lines at the end of the file are left to "ensure line break".
        let run = i - first;
        if run > style.keep_max_blank_lines && i < lines.len() {
            let start = lines[first + style.keep_max_blank_lines].0;
            let end = lines[i].0;
            let inside_gap = edits.iter().any(|(s, e, _)| *s < end && start < *e);
            if !inside_gap {
                edits.push((start, end, String::new()));
            }
        }
    }

    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|(s, _, _)| std::cmp::Reverse(*s));
    let mut out = src.to_string();
    for (s, e, text) in edits {
        out.replace_range(s..e, &text);
    }
    (out != src).then_some(out)
}

/// Generated member blocks (written with braces on the next line) in the given style.
pub fn apply_braces(block: &str, braces: Braces) -> String {
    if braces.functions_on_next_line() {
        block.to_string()
    } else {
        block.replace("\n{\n", " {\n").replace("\n{}", " {}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_settings() {
        let get = |k: &str| match k {
            BRACES_KEY => Some(json!("kr")),
            KEEP_MAX_KEY => Some(json!(0)),
            AROUND_METHODS_KEY => Some(json!(99)),
            _ => None,
        };
        let s = PhpCodeStyle::from_settings(get);
        assert_eq!(s.braces, Braces::KAndR);
        assert_eq!(s.braces.intelephense(), "k&r");
        assert_eq!(s.keep_max_blank_lines, 0);
        assert_eq!(s.blank_lines_around_methods, 10);
        assert_eq!(s.blank_lines_after_imports, 1);
        assert_eq!(
            PhpCodeStyle::from_settings(|_| None),
            PhpCodeStyle::default()
        );
    }

    #[test]
    fn sets_blank_lines_after_namespace_and_imports() {
        let src = "<?php\nnamespace App;\nuse A\\B;\nuse A\\C;\n\n\n\n\nclass X\n{\n}\n";
        let out = tidy_blank_lines(src, &PhpCodeStyle::default()).unwrap();
        assert_eq!(
            out,
            "<?php\nnamespace App;\n\nuse A\\B;\nuse A\\C;\n\nclass X\n{\n}\n"
        );
        let two = PhpCodeStyle {
            blank_lines_after_imports: 2,
            ..PhpCodeStyle::default()
        };
        assert!(tidy_blank_lines(&out, &two)
            .unwrap()
            .contains("use A\\C;\n\n\nclass X"));
    }

    #[test]
    fn keeps_methods_apart_and_limits_blank_lines() {
        let src = "<?php\nclass X\n{\n    private $a;\n    public function f()\n    {\n        $x = 1;\n\n\n\n        return $x;\n    }\n    public function g() {}\n}\n";
        let out = tidy_blank_lines(src, &PhpCodeStyle::default()).unwrap();
        assert_eq!(
            out,
            "<?php\nclass X\n{\n    private $a;\n\n    public function f()\n    {\n        $x = 1;\n\n\n        return $x;\n    }\n\n    public function g() {}\n}\n"
        );
        // Already tidy: nothing to do.
        assert_eq!(tidy_blank_lines(&out, &PhpCodeStyle::default()), None);
    }

    #[test]
    fn never_touches_strings_heredocs_and_comments() {
        let src = "<?php\n$s = <<<EOT\na\n\n\n\n\nb\nEOT;\n/*\n\n\n\n*/\n$t = 'x\n\n\n\ny';\n";
        assert_eq!(tidy_blank_lines(src, &PhpCodeStyle::default()), None);
    }

    #[test]
    fn places_generated_braces() {
        let block = "public function f(): int\n{\n    return 1;\n}";
        assert_eq!(apply_braces(block, Braces::Psr12), block);
        assert_eq!(
            apply_braces(block, Braces::KAndR),
            "public function f(): int {\n    return 1;\n}"
        );
    }
}
