//! Suppression comments, as in PHPStorm:
//!
//! - PHP: `/** @noinspection PhpUnusedAssignment */` (also as a line in a doc comment);
//! - JavaScript / TypeScript: `// noinspection JSUnusedLocalSymbols`;
//! - Python: `# noinspection PyUnusedLocal`;
//! - other languages: their line comment (`// noinspection Id`, `-- noinspection Id`, ...).
//!
//! A comment before a statement suppresses the listed inspections in that statement (a
//! function or class declaration is one statement with its body); a comment at the top of
//! the file (followed by an empty line, or on the `<?php` line) suppresses them in the whole
//! file; a comment after code on the same line suppresses them on that line. `ALL`
//! suppresses every inspection. Ids may carry PHPStorm's `Inspection` suffix.

use serde::{Deserialize, Serialize};

use crate::lang::{self, Insert, Lang, Lexed};
use crate::stmts::{self, BlockKind, Structure};
use crate::text::{indent_of, LineIndex};

/// The id that suppresses every inspection.
pub const ALL: &str = "ALL";

/// A suppressed range of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    pub start: usize,
    /// Exclusive.
    pub end: usize,
    pub ids: Vec<String>,
    pub kind: SuppressKind,
    /// The comment (bytes).
    pub comment: (usize, usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SuppressKind {
    Line,
    Statement,
    Function,
    Class,
    File,
}

/// The suppressions of one file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suppressions {
    pub regions: Vec<Region>,
}

/// Whether suppression id `s` covers inspection `id`.
pub fn id_matches(s: &str, id: &str) -> bool {
    s == ALL || s == id || s.trim_end_matches("Inspection") == id.trim_end_matches("Inspection")
}

/// The ids listed after `noinspection` in a comment's text (several lines of a doc
/// comment may each carry one).
fn comment_ids(comment: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = comment;
    while let Some(p) = rest.find("noinspection") {
        let before = rest[..p].chars().next_back();
        let after = &rest[p + "noinspection".len()..];
        rest = after;
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        if !after.starts_with([' ', '\t']) {
            continue;
        }
        let line = after.split('\n').next().unwrap_or("");
        for raw in line.split([',', ' ', '\t']) {
            let id = raw.trim().trim_end_matches("*/").trim_end_matches("-->");
            let id = id.trim_end_matches("#}").trim_end_matches("--}}");
            if id.is_empty() {
                continue;
            }
            if !id
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
            {
                break;
            }
            out.push(id.to_string());
        }
    }
    out
}

struct Parsed<'a> {
    text: &'a str,
    lang: Lang,
    lexed: Lexed,
    structure: Structure,
}

impl<'a> Parsed<'a> {
    fn new(path: &str, text: &'a str) -> Self {
        let lang = lang::for_path(path);
        let lexed = lang::lex(text, &lang);
        let structure = stmts::scan(text, &lang, &lexed);
        Parsed {
            text,
            lang,
            lexed,
            structure,
        }
    }

    fn is_code_char(&self, i: usize) -> bool {
        self.lexed.is_code(i) && !self.text.as_bytes()[i].is_ascii_whitespace()
    }

    /// Code before `offset` anywhere in the file.
    fn code_before(&self, offset: usize) -> bool {
        (0..offset).any(|i| self.is_code_char(i))
    }

    fn next_code(&self, from: usize) -> Option<usize> {
        (from..self.text.len()).find(|&i| self.is_code_char(i))
    }

    /// Non-blank text before `offset` on its line (code or the `<?php` tag).
    fn text_before_on_line(&self, offset: usize) -> &str {
        let ls = self.text[..offset].rfind('\n').map_or(0, |p| p + 1);
        self.text[ls..offset].trim()
    }

    /// The comment ends a line followed by an empty line (or the end of the file).
    fn followed_by_blank_line(&self, end: usize) -> bool {
        let rest = &self.text[end..];
        let Some(nl) = rest.find('\n') else {
            return rest.trim().is_empty();
        };
        if !rest[..nl].trim().is_empty() {
            return false;
        }
        let next = &rest[nl + 1..];
        blank(next.split('\n').next())
    }
}

/// No line, or a blank one.
fn blank(line: Option<&str>) -> bool {
    line.is_none_or(|l| l.trim().is_empty())
}

impl Suppressions {
    /// Find the suppression comments of a file.
    pub fn parse(path: &str, text: &str) -> Self {
        if !text.contains("noinspection") {
            return Suppressions::default();
        }
        Suppressions::from_parsed(&Parsed::new(path, text))
    }

    fn from_parsed(p: &Parsed) -> Self {
        let text = p.text;
        let mut regions: Vec<Region> = Vec::new();
        for &(start, end) in &p.lexed.comments {
            let ids = comment_ids(&text[start..end]);
            if ids.is_empty() {
                continue;
            }
            let ls = text[..start].rfind('\n').map_or(0, |x| x + 1);
            let trailing = (ls..start).any(|i| p.is_code_char(i));
            if trailing {
                let le = text[end..].find('\n').map_or(text.len(), |x| end + x);
                regions.push(Region {
                    start: ls,
                    end: le,
                    ids,
                    kind: SuppressKind::Line,
                    comment: (start, end),
                });
                continue;
            }
            let at_top = !p.code_before(start);
            if at_top
                && (!p.text_before_on_line(start).is_empty()
                    || p.followed_by_blank_line(end)
                    || p.next_code(end).is_none())
            {
                regions.push(Region {
                    start: 0,
                    end: text.len(),
                    ids,
                    kind: SuppressKind::File,
                    comment: (start, end),
                });
                continue;
            }
            let Some(s) = p.next_code(end) else {
                continue;
            };
            let (range, kind) = match p.structure.statement_from(s) {
                Some(st) => {
                    let kind = if p
                        .structure
                        .blocks
                        .iter()
                        .any(|b| b.start == st.start && b.kind == BlockKind::Class)
                    {
                        SuppressKind::Class
                    } else if p
                        .structure
                        .blocks
                        .iter()
                        .any(|b| b.start == st.start && b.kind == BlockKind::Function)
                    {
                        SuppressKind::Function
                    } else {
                        SuppressKind::Statement
                    };
                    ((st.start, st.end), kind)
                }
                None => {
                    let le = text[s..].find('\n').map_or(text.len(), |x| s + x);
                    ((s, le), SuppressKind::Statement)
                }
            };
            // Up to the end of the line: trailing comments belong to the statement.
            let end_line = text[range.1..]
                .find('\n')
                .map_or(text.len(), |x| range.1 + x);
            regions.push(Region {
                start: range.0,
                end: end_line,
                ids,
                kind,
                comment: (start, end),
            });
        }
        Suppressions { regions }
    }

    /// Whether inspection `id` is suppressed at byte `offset`.
    pub fn is_suppressed(&self, id: &str, offset: usize) -> bool {
        self.regions.iter().any(|r| {
            r.start <= offset && offset <= r.end && r.ids.iter().any(|s| id_matches(s, id))
        })
    }

    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }
}

/// A text change (bytes, or UTF-16 code units after [`to_utf16`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextChange {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// An Alt+Enter action that suppresses an inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppressAction {
    pub kind: SuppressKind,
    /// `Suppress for statement`.
    pub title: String,
    pub changes: Vec<TextChange>,
}

fn comment_text(lang: &Lang, ids: &str) -> String {
    match lang.insert {
        Insert::PhpDoc => format!("/** @noinspection {ids} */"),
        Insert::Line(prefix) => format!("{prefix}noinspection {ids}"),
        Insert::Wrap(open, close) => format!("{open}noinspection {ids}{close}"),
    }
}

/// The comment right above `line_start` when it is a suppression comment of the same
/// style: its id list gets the new id instead of a second comment.
fn extend_existing(p: &Parsed, line_start: usize, id: &str) -> Option<TextChange> {
    if line_start == 0 {
        return None;
    }
    let lines = LineIndex::new(p.text);
    let prev = lines.line_of(line_start - 1);
    let prev_text = lines.line_text(prev);
    let trimmed = prev_text.trim();
    if !trimmed.contains("noinspection") || comment_ids(trimmed).is_empty() {
        return None;
    }
    if comment_ids(trimmed).iter().any(|s| id_matches(s, id)) {
        return Some(TextChange {
            start: line_start,
            end: line_start,
            text: String::new(),
        });
    }
    // Insert after the last id.
    let ls = lines.line_start(prev);
    let after = prev_text.find("noinspection")? + "noinspection".len();
    let rest = &prev_text[after..];
    let ids_end = rest
        .char_indices()
        .skip_while(|(_, c)| c.is_whitespace())
        .take_while(|(_, c)| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ',' | ' '))
        .last()
        .map(|(i, c)| i + c.len_utf8())?;
    let ids_end = after + rest[..ids_end].trim_end().len();
    Some(TextChange {
        start: ls + ids_end,
        end: ls + ids_end,
        text: format!(",{id}"),
    })
}

/// The change that puts a suppression comment for `id` in front of the code at `start`.
fn insert_before(p: &Parsed, start: usize, id: &str) -> TextChange {
    let text = p.text;
    let ls = text[..start].rfind('\n').map_or(0, |x| x + 1);
    let line = &text[ls..text[ls..].find('\n').map_or(text.len(), |x| ls + x)];
    let indent = indent_of(line);
    if let Some(c) = extend_existing(p, ls, id) {
        return c;
    }
    // PHP: into the doc comment right above.
    if p.lang.insert == Insert::PhpDoc && ls > 0 {
        let above = text[..ls - 1].trim_end();
        if above.ends_with("*/") {
            if let Some(open) = above.rfind("/**") {
                let doc = &above[open..];
                if doc.contains('\n') {
                    let close = above.len() - 2;
                    let cl = text[..close].rfind('\n').map_or(0, |x| x + 1);
                    if text[cl..close].trim().is_empty() {
                        return TextChange {
                            start: cl,
                            end: cl,
                            text: format!("{indent} * @noinspection {id}\n"),
                        };
                    }
                } else {
                    let body = doc.trim_start_matches("/**").trim_end_matches("*/").trim();
                    return TextChange {
                        start: open,
                        end: above.len(),
                        text: format!(
                            "/**\n{indent} * {body}\n{indent} * @noinspection {id}\n{indent} */"
                        ),
                    };
                }
            }
        }
    }
    TextChange {
        start: ls,
        end: ls,
        text: format!("{indent}{}\n", comment_text(&p.lang, id)),
    }
}

/// The change for "Suppress for file".
fn insert_file(p: &Parsed, id: &str) -> TextChange {
    let text = p.text;
    // An existing file-level suppression gets the id.
    let file_level = Suppressions::from_parsed(p)
        .regions
        .into_iter()
        .find(|r| r.kind == SuppressKind::File);
    if let Some(r) = file_level {
        let (s, e) = r.comment;
        if r.ids.iter().any(|x| id_matches(x, id)) {
            return TextChange {
                start: s,
                end: s,
                text: String::new(),
            };
        }
        let le = text[s..e].find('\n').map_or(e, |x| s + x);
        let line = &text[s..le];
        let after = line
            .find("noinspection")
            .map_or(0, |x| x + "noinspection".len());
        let tail = line[after..]
            .trim_end()
            .trim_end_matches("*/")
            .trim_end_matches("-->")
            .trim_end_matches("#}")
            .trim_end_matches("--}}")
            .trim_end();
        let at = s + after + tail.len();
        return TextChange {
            start: at,
            end: at,
            text: format!(",{id}"),
        };
    }
    let comment = comment_text(&p.lang, id);
    if p.lang.php {
        if let Some(tag) = text.find("<?php") {
            let le = text[tag..].find('\n').map_or(text.len(), |x| tag + x);
            let next_blank = blank(text[le..].split('\n').nth(1));
            let tail = if next_blank { "" } else { "\n" };
            return TextChange {
                start: le,
                end: le,
                text: format!("\n\n{comment}{tail}"),
            };
        }
    }
    // After a shebang and encoding / license lines at the top.
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if t.starts_with("#!") || (at == 0 && t.starts_with("# -*-")) {
            at += line.len();
        } else {
            break;
        }
    }
    let next_blank = blank(text[at..].split('\n').next());
    let sep = if next_blank { "\n" } else { "\n\n" };
    TextChange {
        start: at,
        end: at,
        text: format!("{comment}{sep}"),
    }
}

/// The Suppress actions for a problem of `id` at byte `offset` (PHPStorm's order:
/// statement, function, class, file).
pub fn actions(path: &str, text: &str, offset: usize, id: &str) -> Vec<SuppressAction> {
    let p = Parsed::new(path, text);
    let mut out = Vec::new();
    let mut seen: Vec<usize> = Vec::new();
    let mut push =
        |out: &mut Vec<SuppressAction>, kind: SuppressKind, title: &str, start: usize| {
            if seen.contains(&start) {
                return;
            }
            seen.push(start);
            out.push(SuppressAction {
                kind,
                title: title.into(),
                changes: vec![insert_before(&p, start, id)],
            });
        };
    if let Some(s) = p.structure.statement_at(offset) {
        let is_decl = p
            .structure
            .blocks
            .iter()
            .any(|b| b.start == s.start && b.kind != BlockKind::Other);
        if !is_decl {
            push(
                &mut out,
                SuppressKind::Statement,
                "Suppress for statement",
                s.start,
            );
        }
    }
    if let Some(b) = p.structure.block_at(offset, BlockKind::Function) {
        push(
            &mut out,
            SuppressKind::Function,
            "Suppress for function",
            b.start,
        );
    }
    if let Some(b) = p.structure.block_at(offset, BlockKind::Class) {
        push(&mut out, SuppressKind::Class, "Suppress for class", b.start);
    }
    out.push(SuppressAction {
        kind: SuppressKind::File,
        title: "Suppress for file".into(),
        changes: vec![insert_file(&p, id)],
    });
    out
}

/// Convert byte offsets of changes to UTF-16 code units (CodeMirror positions).
pub fn to_utf16(text: &str, changes: &mut [TextChange]) {
    for c in changes {
        c.start = text[..c.start.min(text.len())].encode_utf16().count();
        c.end = text[..c.end.min(text.len())].encode_utf16().count();
    }
}

/// Apply changes (bytes) to a text, for tests and Code Cleanup.
pub fn apply(text: &str, changes: &[TextChange]) -> String {
    let mut sorted: Vec<&TextChange> = changes.iter().collect();
    sorted.sort_by_key(|c| std::cmp::Reverse((c.start, c.end)));
    let mut out = text.to_string();
    for c in sorted {
        out.replace_range(c.start..c.end, &c.text);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suppressed(path: &str, text: &str, id: &str, needle: &str) -> bool {
        Suppressions::parse(path, text).is_suppressed(id, text.find(needle).unwrap())
    }

    const PHP: &str = "<?php\n\nclass A\n{\n    /**\n     * Docs.\n     * @noinspection PhpUnusedAssignment\n     */\n    public function f()\n    {\n        $a = 1;\n        /** @noinspection PhpConstantCondition */\n        if (true) {\n            $b = 2;\n        }\n        $c = 3;\n    }\n\n    public function g()\n    {\n        $d = 4;\n    }\n}\n";

    #[test]
    fn php_doc_comments_suppress_their_statement() {
        assert!(suppressed("a.php", PHP, "PhpUnusedAssignment", "$a = 1"));
        assert!(suppressed(
            "a.php",
            PHP,
            "PhpUnusedAssignmentInspection",
            "$c = 3"
        ));
        assert!(!suppressed("a.php", PHP, "PhpUnusedAssignment", "$d = 4"));
        assert!(suppressed("a.php", PHP, "PhpConstantCondition", "true"));
        assert!(suppressed("a.php", PHP, "PhpConstantCondition", "$b = 2"));
        assert!(!suppressed("a.php", PHP, "PhpConstantCondition", "$c = 3"));
        assert!(!suppressed("a.php", PHP, "Other", "$a = 1"));
    }

    #[test]
    fn file_line_and_all_suppressions() {
        let php = "<?php\n\n/** @noinspection PhpDeprecation */\n\nnamespace A;\n\nfoo();\n";
        assert!(suppressed("a.php", php, "PhpDeprecation", "foo"));
        let tag = "<?php /** @noinspection ALL */\nfoo();\n";
        assert!(suppressed("a.php", tag, "Anything", "foo"));
        let js = "// noinspection JSUnusedLocalSymbols,JSUnresolvedReference\n\nconst a = 1\n";
        assert!(suppressed("a.ts", js, "JSUnresolvedReference", "a = 1"));
        let line = "fn main() {\n    let x = 1; // noinspection RsClippy\n    let y = 2;\n}\n";
        assert!(suppressed("a.rs", line, "RsClippy", "x = 1"));
        assert!(!suppressed("a.rs", line, "RsClippy", "y = 2"));
        let py = "import os\n\n# noinspection PyUnusedLocal\ndef f(a):\n    x = 1\n\ny = 2\n";
        assert!(suppressed("a.py", py, "PyUnusedLocal", "x = 1"));
        assert!(!suppressed("a.py", py, "PyUnusedLocal", "y = 2"));
        // In a string it is not a comment.
        let s = "<?php\n$a = '/** @noinspection X */';\n$b = 1;\n";
        assert!(!suppressed("a.php", s, "X", "$b"));
    }

    fn apply_action(path: &str, text: &str, needle: &str, id: &str, kind: SuppressKind) -> String {
        let a = actions(path, text, text.find(needle).unwrap(), id);
        let action = a
            .iter()
            .find(|a| a.kind == kind)
            .unwrap_or_else(|| panic!("{kind:?} in {a:?}"));
        let out = apply(text, &action.changes);
        assert!(
            Suppressions::parse(path, &out).is_suppressed(id, out.find(needle).unwrap()),
            "{out}"
        );
        out
    }

    #[test]
    fn php_actions_insert_comments() {
        let src = "<?php\n\nclass A\n{\n    /**\n     * Docs.\n     */\n    public function f()\n    {\n        $a = 1;\n        $b = 2;\n    }\n}\n";
        let out = apply_action(
            "a.php",
            src,
            "$a = 1",
            "PhpUnusedAssignment",
            SuppressKind::Statement,
        );
        assert!(
            out.contains("        /** @noinspection PhpUnusedAssignment */\n        $a = 1;"),
            "{out}"
        );
        assert!(!Suppressions::parse("a.php", &out)
            .is_suppressed("PhpUnusedAssignment", out.find("$b").unwrap()));
        let out = apply_action(
            "a.php",
            src,
            "$a = 1",
            "PhpUnusedAssignment",
            SuppressKind::Function,
        );
        assert!(
            out.contains("     * Docs.\n     * @noinspection PhpUnusedAssignment\n     */"),
            "{out}"
        );
        let out = apply_action(
            "a.php",
            src,
            "$a = 1",
            "PhpUnusedAssignment",
            SuppressKind::Class,
        );
        assert!(
            out.contains("<?php\n\n/** @noinspection PhpUnusedAssignment */\nclass A"),
            "{out}"
        );
        let out = apply_action(
            "a.php",
            src,
            "$a = 1",
            "PhpUnusedAssignment",
            SuppressKind::File,
        );
        assert!(
            out.starts_with("<?php\n\n/** @noinspection PhpUnusedAssignment */\n\nclass A"),
            "{out}"
        );
        // A second id joins the existing comment.
        let twice = apply_action(
            "a.php",
            &out,
            "$a = 1",
            "PhpConstantCondition",
            SuppressKind::File,
        );
        assert!(
            twice.contains("@noinspection PhpUnusedAssignment,PhpConstantCondition */"),
            "{twice}"
        );
        let single = "<?php\nfunction f()\n{\n    /** @var int $a */\n    $a = 1;\n}\n";
        let out = apply_action("a.php", single, "$a = 1", "X", SuppressKind::Statement);
        assert!(
            out.contains(
                "    /**\n     * @var int $a\n     * @noinspection X\n     */\n    $a = 1;"
            ),
            "{out}"
        );
    }

    #[test]
    fn other_language_actions() {
        let ts = "function f() {\n  const a = 1\n  return 2\n}\n";
        let out = apply_action(
            "a.ts",
            ts,
            "a = 1",
            "JSUnusedLocalSymbols",
            SuppressKind::Statement,
        );
        assert!(
            out.contains("  // noinspection JSUnusedLocalSymbols\n  const a = 1"),
            "{out}"
        );
        let out = apply_action(
            "a.ts",
            ts,
            "a = 1",
            "JSUnusedLocalSymbols",
            SuppressKind::Function,
        );
        assert!(
            out.starts_with("// noinspection JSUnusedLocalSymbols\nfunction f()"),
            "{out}"
        );
        let out = apply_action("a.ts", ts, "a = 1", "X", SuppressKind::File);
        assert!(out.starts_with("// noinspection X\n\nfunction"), "{out}");
        let py = "#!/usr/bin/env python\nclass C:\n    def f(self):\n        x = 1\n";
        let out = apply_action("a.py", py, "x = 1", "PyUnusedLocal", SuppressKind::Function);
        assert!(
            out.contains("    # noinspection PyUnusedLocal\n    def f"),
            "{out}"
        );
        let out = apply_action("a.py", py, "x = 1", "PyUnusedLocal", SuppressKind::File);
        assert!(
            out.starts_with("#!/usr/bin/env python\n# noinspection PyUnusedLocal\n\nclass"),
            "{out}"
        );
        let out = apply_action(
            "a.go",
            "package a\n\nfunc f() {\n\tx := 1\n}\n",
            "x := 1",
            "GoUnused",
            SuppressKind::Statement,
        );
        assert!(
            out.contains("\t// noinspection GoUnused\n\tx := 1"),
            "{out}"
        );
        let mut c = vec![TextChange {
            start: 6,
            end: 6,
            text: "x".into(),
        }];
        to_utf16("ä😀b", &mut c);
        assert_eq!(c[0].start, 3);
    }
}
