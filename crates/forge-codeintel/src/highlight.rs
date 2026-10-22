//! Highlight usages in the editor: the automatic highlighting of the element at the caret
//! (Settings | Editor | General) and Highlight Usages in File (Ctrl+Shift+F7), with read
//! and write usages told apart. Language servers' `documentHighlight` and the PHP index
//! answer first; this module adds what needs no index: the exit points of a function when
//! the caret is on `return` / `throw`, and a lexical fallback (the identifier's occurrences
//! in its scope, comments and strings left out).

use serde::{Deserialize, Serialize};

use crate::edit::{Position, Range};
use crate::usage_options::ElementKind;
use crate::usages::{classify_usage, UsageKind};

/// How an occurrence uses the element (LSP's `DocumentHighlightKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HighlightKind {
    Read,
    Write,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageHighlight {
    pub range: Range,
    pub kind: HighlightKind,
}

/// What a highlight shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HighlightTarget {
    #[default]
    Usages,
    /// `return` / `throw` statements of the function around the caret.
    ExitPoints,
    /// The methods of a class that implement the interface after `implements`.
    ImplementingMethods,
    /// The methods of a class that override methods of the class after `extends`.
    OverridingMethods,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightAnswer {
    pub target: HighlightTarget,
    /// `total`, `User::orders()`, the function of the exit points, the interface.
    pub title: String,
    pub highlights: Vec<UsageHighlight>,
    /// `lsp`, `index` or `text`.
    pub source: String,
    /// The element, for the Find Usages dialog.
    #[serde(default)]
    pub element: ElementKind,
    /// "3 usages of 'total' found", for the status bar.
    pub summary: String,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl HighlightAnswer {
    pub fn new(
        target: HighlightTarget,
        title: impl Into<String>,
        mut highlights: Vec<UsageHighlight>,
        source: &str,
    ) -> Self {
        highlights.sort_by_key(|h| h.range.start);
        highlights.dedup_by_key(|h| h.range.start);
        let title = title.into();
        let n = highlights.len();
        let summary = match target {
            HighlightTarget::Usages => {
                format!("{} of '{title}' found", plural(n, "usage", "usages"))
            }
            HighlightTarget::ExitPoints => {
                format!(
                    "{} of '{title}' found",
                    plural(n, "exit point", "exit points")
                )
            }
            HighlightTarget::ImplementingMethods => format!(
                "{} implementing '{title}' found",
                plural(n, "method", "methods")
            ),
            HighlightTarget::OverridingMethods => format!(
                "{} overriding '{title}' found",
                plural(n, "method", "methods")
            ),
        };
        HighlightAnswer {
            target,
            title,
            highlights,
            source: source.into(),
            element: ElementKind::Symbol,
            summary,
        }
    }

    pub fn with_element(mut self, element: ElementKind) -> Self {
        self.element = element;
        self
    }
}

/// Read or write, from the line around a usage (`$total = 0`, `$this->total += 5`).
pub fn access_kind(line: &str, start: u32, end: u32) -> HighlightKind {
    if classify_usage(line, start, end) == UsageKind::Write {
        HighlightKind::Write
    } else {
        HighlightKind::Read
    }
}

/// `UsageHighlight`s for ranges of `text`: writes from the line, the declaration as a write.
pub fn classify_ranges(
    text: &str,
    ranges: &[Range],
    declaration: Option<Range>,
) -> Vec<UsageHighlight> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<UsageHighlight> = ranges
        .iter()
        .map(|r| {
            let line = lines
                .get(r.start.line as usize)
                .map(|l| l.strip_suffix('\r').unwrap_or(l))
                .unwrap_or("");
            let end = if r.end.line == r.start.line {
                r.end.character
            } else {
                line.encode_utf16().count() as u32
            };
            UsageHighlight {
                range: *r,
                kind: if declaration.is_some_and(|d| d.start == r.start) {
                    HighlightKind::Write
                } else {
                    access_kind(line, r.start.character, end)
                },
            }
        })
        .collect();
    if let Some(d) = declaration {
        if !out.iter().any(|h| h.range.start == d.start) {
            out.push(UsageHighlight {
                range: d,
                kind: HighlightKind::Write,
            });
        }
    }
    out
}

// ---- Lexer: what is code, comment or string ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    /// Prose and data: no comments or strings.
    Plain,
    Php,
    CLike,
    Lua,
    Hash,
}

fn lang_of(path: &str) -> Lang {
    let p = path.to_ascii_lowercase();
    let ext = p.rsplit('.').next().unwrap_or("");
    match ext {
        "php" | "phtml" | "inc" => Lang::Php,
        "lua" => Lang::Lua,
        "md" | "txt" | "json" | "html" | "htm" | "xml" | "csv" | "log" | "svg" => Lang::Plain,
        _ if !p.rsplit('/').next().unwrap_or("").contains('.') => Lang::Plain,
        "py" | "rb" | "sh" | "bash" | "zsh" | "yaml" | "yml" | "toml" | "pl" | "r" => Lang::Hash,
        _ => Lang::CLike,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Code,
    Comment,
    /// A string without interpolation.
    Str,
    /// A string with `$variables` (PHP double quotes, heredoc).
    Interp,
}

/// The class of every byte of `text`.
fn classify_bytes(text: &str, lang: Lang) -> Vec<Class> {
    let b = text.as_bytes();
    let mut out = vec![Class::Code; b.len()];
    let mut i = 0;
    if lang == Lang::Plain {
        return out;
    }
    let line_comment = |i: usize| -> bool {
        match lang {
            Lang::Plain => false,
            Lang::Php => b[i..].starts_with(b"//") || (b[i] == b'#' && b.get(i + 1) != Some(&b'[')),
            Lang::CLike => b[i..].starts_with(b"//"),
            Lang::Lua => b[i..].starts_with(b"--") && !b[i..].starts_with(b"--[["),
            Lang::Hash => b[i] == b'#',
        }
    };
    while i < b.len() {
        let c = b[i];
        if line_comment(i) {
            let end = text[i..].find('\n').map_or(b.len(), |n| i + n);
            out[i..end].fill(Class::Comment);
            i = end;
        } else if (lang == Lang::Lua && b[i..].starts_with(b"--[["))
            || (matches!(lang, Lang::Php | Lang::CLike) && b[i..].starts_with(b"/*"))
        {
            let (open, close) = if lang == Lang::Lua {
                (4, "]]")
            } else {
                (2, "*/")
            };
            let end = text[i + open..]
                .find(close)
                .map_or(b.len(), |n| i + open + n + close.len());
            out[i..end].fill(Class::Comment);
            i = end;
        } else if c == b'"' || c == b'\'' || (c == b'`' && lang == Lang::CLike) {
            let class = if c == b'"' && lang == Lang::Php {
                Class::Interp
            } else {
                Class::Str
            };
            // Only PHP strings and template literals span lines.
            let multiline = lang == Lang::Php || c == b'`';
            let mut j = i + 1;
            while j < b.len() && b[j] != c && (multiline || b[j] != b'\n') {
                if b[j] == b'\\' {
                    j += 1;
                }
                j += 1;
            }
            let end = (j + 1).min(b.len());
            out[i..end].fill(class);
            i = end;
        } else if lang == Lang::Php && b[i..].starts_with(b"<<<") {
            // Heredoc / nowdoc: `<<<EOT` … `EOT`.
            let head_end = text[i..].find('\n').map_or(b.len(), |n| i + n);
            let label = text[i + 3..head_end].trim();
            let nowdoc = label.starts_with('\'');
            let label = label.trim_matches(|c| c == '\'' || c == '"');
            // The closing label starts a later line.
            let mut end = b.len();
            let mut at = head_end;
            while !label.is_empty() && at < b.len() {
                let start = at + 1;
                let line_end = text[start.min(b.len())..]
                    .find('\n')
                    .map_or(b.len(), |n| start + n);
                let line = &text[start.min(line_end)..line_end];
                let indent = line.len() - line.trim_start().len();
                if line.trim_start().starts_with(label) {
                    end = start + indent + label.len();
                    break;
                }
                at = line_end;
            }
            out[i..end].fill(if nowdoc { Class::Str } else { Class::Interp });
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// Byte offsets of line starts, to turn bytes into LSP positions.
struct Lines<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Lines { text, starts }
    }

    fn position(&self, byte: usize) -> Position {
        let byte = byte.min(self.text.len());
        let line = self.starts.partition_point(|&s| s <= byte) - 1;
        let start = self.starts[line];
        Position::new(
            line as u32,
            self.text[start..byte].encode_utf16().count() as u32,
        )
    }

    fn byte(&self, p: Position) -> usize {
        let Some(&start) = self.starts.get(p.line as usize) else {
            return self.text.len();
        };
        let mut units = 0u32;
        for (i, c) in self.text[start..].char_indices() {
            if units >= p.character || c == '\n' {
                return start + i;
            }
            units += c.len_utf16() as u32;
        }
        self.text.len()
    }

    fn range(&self, start: usize, end: usize) -> Range {
        Range::new(self.position(start), self.position(end))
    }

    fn line_of(&self, byte: usize) -> (&'a str, usize) {
        let line = self.starts.partition_point(|&s| s <= byte) - 1;
        let start = self.starts[line];
        let end = self.text[start..]
            .find('\n')
            .map_or(self.text.len(), |n| start + n);
        (self.text[start..end].trim_end_matches('\r'), start)
    }
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}

/// The identifier at byte `at` (a leading `$` included): its byte range.
fn word_at(text: &str, at: usize) -> Option<(usize, usize)> {
    let b = text.as_bytes();
    let mut i = at.min(b.len());
    if i == b.len() || !(is_ident(b[i]) || b[i] == b'$') {
        if i > 0 && is_ident(b[i - 1]) {
            i -= 1;
        } else {
            return None;
        }
    }
    let mut start = i;
    while start > 0 && is_ident(b[start - 1]) {
        start -= 1;
    }
    if b[start] != b'$' && start > 0 && b[start - 1] == b'$' {
        start -= 1;
    }
    let mut end = if b[i] == b'$' { i + 1 } else { i };
    while end < b.len() && is_ident(b[end]) {
        end += 1;
    }
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    while !text.is_char_boundary(end) {
        end += 1;
    }
    (end > start && &text[start..end] != "$").then_some((start, end))
}

const KEYWORDS: &[&str] = &[
    "abstract",
    "and",
    "array",
    "as",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "clone",
    "const",
    "continue",
    "declare",
    "default",
    "do",
    "echo",
    "else",
    "elseif",
    "end",
    "enum",
    "export",
    "extends",
    "false",
    "final",
    "finally",
    "fn",
    "for",
    "foreach",
    "from",
    "function",
    "global",
    "goto",
    "if",
    "implements",
    "import",
    "in",
    "include",
    "instanceof",
    "insteadof",
    "interface",
    "let",
    "local",
    "match",
    "namespace",
    "new",
    "nil",
    "not",
    "null",
    "of",
    "or",
    "print",
    "private",
    "protected",
    "public",
    "readonly",
    "require",
    "return",
    "static",
    "switch",
    "then",
    "this",
    "throw",
    "trait",
    "true",
    "try",
    "typeof",
    "until",
    "use",
    "var",
    "void",
    "while",
    "yield",
];

/// A function: the `function` keyword, its parameter list and its body (byte ranges).
#[derive(Debug, Clone, Copy)]
struct FunctionSpan {
    keyword: usize,
    name: Option<(usize, usize)>,
    params: (usize, usize),
    body: (usize, usize),
}

/// The bracket matching `open` at `at` (code bytes only).
fn matching(text: &str, classes: &[Class], at: usize, open: u8, close: u8) -> Option<usize> {
    let b = text.as_bytes();
    let mut depth = 0usize;
    for i in at..b.len() {
        if classes[i] != Class::Code {
            continue;
        }
        if b[i] == open {
            depth += 1;
        } else if b[i] == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// Functions declared with the `function` keyword (PHP, JavaScript).
fn functions(text: &str, classes: &[Class]) -> Vec<FunctionSpan> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    for (at, _) in text.match_indices("function") {
        let end = at + "function".len();
        if classes[at] != Class::Code
            || (at > 0 && (is_ident(b[at - 1]) || b[at - 1] == b'$'))
            || b.get(end).is_some_and(|&c| is_ident(c))
        {
            continue;
        }
        let Some(open) = text[end..].find('(').map(|n| end + n) else {
            continue;
        };
        let name = text[end..open].trim().trim_start_matches('&').trim();
        let name = (!name.is_empty() && name.bytes().all(is_ident)).then(|| {
            let s = end + text[end..open].find(name).unwrap_or(0);
            (s, s + name.len())
        });
        if text[end..open].contains(['{', ';', '}']) {
            continue;
        }
        let Some(close) = matching(text, classes, open, b'(', b')') else {
            continue;
        };
        // The body: the next `{` before a `;` (abstract and interface methods have none).
        let mut i = close + 1;
        let mut body = None;
        while i < b.len() {
            if classes[i] == Class::Code {
                if b[i] == b';' {
                    break;
                }
                if b[i] == b'{' {
                    body = matching(text, classes, i, b'{', b'}').map(|e| (i, e + 1));
                    break;
                }
            }
            i += 1;
        }
        if let Some(body) = body {
            out.push(FunctionSpan {
                keyword: at,
                name,
                params: (open, close + 1),
                body,
            });
        }
    }
    out
}

/// The innermost function whose body (or, with `with_params`, parameter list) contains `at`.
fn enclosing(spans: &[FunctionSpan], at: usize, with_params: bool) -> Option<FunctionSpan> {
    spans
        .iter()
        .filter(|f| {
            let start = if with_params { f.keyword } else { f.body.0 };
            start <= at && at < f.body.1
        })
        .min_by_key(|f| f.body.1 - f.body.0)
        .copied()
}

/// Exit points (Highlight Usages on `return` / `throw`): the `return` and `throw`
/// statements of the function around the caret, nested functions left out.
pub fn exit_points(text: &str, path: &str, pos: Position) -> Option<HighlightAnswer> {
    let lang = lang_of(path);
    if !matches!(lang, Lang::Php | Lang::CLike) {
        return None;
    }
    let lines = Lines::new(text);
    let at = lines.byte(pos);
    let (s, e) = word_at(text, at)?;
    if !matches!(&text[s..e], "return" | "throw") {
        return None;
    }
    let classes = classify_bytes(text, lang);
    if classes[s] != Class::Code {
        return None;
    }
    let spans = functions(text, &classes);
    let f = enclosing(&spans, s, false)?;
    let nested: Vec<(usize, usize)> = spans
        .iter()
        .filter(|n| n.keyword > f.body.0 && n.body.1 <= f.body.1)
        .map(|n| (n.keyword, n.body.1))
        .collect();
    let b = text.as_bytes();
    let mut highlights = Vec::new();
    for word in ["return", "throw"] {
        for (i, _) in text[f.body.0..f.body.1].match_indices(word) {
            let i = f.body.0 + i;
            let end = i + word.len();
            if classes[i] != Class::Code
                || is_ident(b[i - 1])
                || b[i - 1] == b'$'
                || b[i - 1] == b'>'
                || b.get(end).is_some_and(|&c| is_ident(c))
                || nested.iter().any(|&(ns, ne)| ns <= i && i < ne)
            {
                continue;
            }
            // The statement up to its `;` (or the end of the body).
            let mut j = end;
            let mut depth = 0i32;
            while j < f.body.1 - 1 {
                if classes[j] == Class::Code {
                    match b[j] {
                        b'(' | b'[' | b'{' => depth += 1,
                        b')' | b']' | b'}' => depth -= 1,
                        b';' if depth <= 0 => break,
                        _ => {}
                    }
                    if depth < 0 {
                        break;
                    }
                }
                j += 1;
            }
            let stmt_end = i + text[i..j].trim_end().len();
            highlights.push(UsageHighlight {
                range: lines.range(i, stmt_end),
                kind: HighlightKind::Read,
            });
        }
    }
    let title = f
        .name
        .map(|(s, e)| text[s..e].to_string())
        .unwrap_or_else(|| "function".into());
    Some(HighlightAnswer::new(
        HighlightTarget::ExitPoints,
        title,
        highlights,
        "text",
    ))
}

/// The lexical fallback: occurrences of the identifier at the caret, outside comments and
/// strings (PHP variables also inside interpolated strings), within the function for
/// variables. `None` on keywords, numbers, comments and strings.
pub fn lexical_highlights(text: &str, path: &str, pos: Position) -> Option<HighlightAnswer> {
    let lang = lang_of(path);
    let lines = Lines::new(text);
    let at = lines.byte(pos);
    let (s, e) = word_at(text, at)?;
    let word = &text[s..e];
    let bare = word.trim_start_matches('$');
    if bare.is_empty()
        || bare.as_bytes()[0].is_ascii_digit()
        || (!word.starts_with('$') && KEYWORDS.contains(&bare.to_ascii_lowercase().as_str()))
    {
        return None;
    }
    let variable = lang == Lang::Php && word.starts_with('$');
    if variable && word == "$this" {
        return None;
    }
    let classes = classify_bytes(text, lang);
    let allowed = |c: Class| c == Class::Code || (variable && c == Class::Interp);
    if !allowed(classes[s]) {
        return None;
    }
    let spans = if lang == Lang::Php || lang == Lang::CLike {
        functions(text, &classes)
    } else {
        Vec::new()
    };
    let (from, to) = if variable {
        enclosing(&spans, s, true)
            .map(|f| (f.keyword, f.body.1))
            .unwrap_or((0, text.len()))
    } else {
        (0, text.len())
    };
    let b = text.as_bytes();
    let mut highlights = Vec::new();
    for (i, _) in text[from..to].match_indices(word) {
        let i = from + i;
        let end = i + word.len();
        if !allowed(classes[i])
            || b.get(end).is_some_and(|&c| is_ident(c))
            || (i > 0 && (is_ident(b[i - 1]) || (!variable && b[i - 1] == b'$')))
        {
            continue;
        }
        // Inside a nested function, a variable of the same name is another variable.
        if variable
            && enclosing(&spans, s, true).map(|f| f.keyword)
                != enclosing(&spans, i, true).map(|f| f.keyword)
        {
            continue;
        }
        let (line, line_start) = lines.line_of(i);
        let start_col = text[line_start..i].encode_utf16().count() as u32;
        let end_col = start_col + word.encode_utf16().count() as u32;
        let before = line[..i - line_start].trim_end();
        let in_params = spans.iter().any(|f| f.params.0 < i && i < f.params.1);
        let declares = before.ends_with("function")
            || (variable && in_params)
            || before.ends_with(" as")
            || (variable && before.ends_with("=>") && line.trim_start().starts_with("foreach"))
            || (variable && (before.ends_with("global") || before.ends_with("static")))
            || [
                "const",
                "let",
                "var",
                "local",
                "class",
                "interface",
                "trait",
                "enum",
            ]
            .iter()
            .any(|k| before.ends_with(k) && !before.ends_with(&format!("{k}:")));
        let kind = if declares {
            HighlightKind::Write
        } else {
            access_kind(line, start_col, end_col)
        };
        highlights.push(UsageHighlight {
            range: lines.range(i, end),
            kind,
        });
    }
    if highlights.is_empty() {
        return None;
    }
    let element = if variable {
        ElementKind::Variable
    } else {
        ElementKind::Symbol
    };
    Some(
        HighlightAnswer::new(HighlightTarget::Usages, word, highlights, "text")
            .with_element(element),
    )
}

/// LSP `DocumentHighlightKind` (1 text, 2 read, 3 write) to [`HighlightKind`].
pub fn lsp_highlight_kind(kind: Option<u8>) -> HighlightKind {
    match kind {
        Some(2) => HighlightKind::Read,
        Some(3) => HighlightKind::Write,
        _ => HighlightKind::Text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Position of the `n`th (0-based) occurrence of `needle`, plus `offset` bytes.
    fn pos(text: &str, needle: &str, n: usize) -> Position {
        let at = text.match_indices(needle).nth(n).expect("needle").0;
        Lines::new(text).position(at + 1)
    }

    fn spans(text: &str, a: &HighlightAnswer) -> Vec<(String, HighlightKind)> {
        let lines = Lines::new(text);
        a.highlights
            .iter()
            .map(|h| {
                (
                    text[lines.byte(h.range.start)..lines.byte(h.range.end)].to_string(),
                    h.kind,
                )
            })
            .collect()
    }

    const PHP: &str = r#"<?php
class Order
{
    private int $total = 0;

    public function add(int $amount): int
    {
        // $amount is checked
        $total = $amount;
        $total += 1;
        $log = "added $amount";
        $f = function ($amount) { return $amount * 2; };
        if ($amount < 0) {
            throw new \InvalidArgumentException('negative $amount');
        }
        return $this->total + $total;
    }

    public function other(): void
    {
        $total = 5;
    }
}
"#;

    #[test]
    fn variables_stay_in_their_function_and_skip_comments() {
        let a = lexical_highlights(PHP, "Order.php", pos(PHP, "$total = $amount", 0)).unwrap();
        let found = spans(PHP, &a);
        assert_eq!(
            found,
            vec![
                ("$total".into(), HighlightKind::Write),
                ("$total".into(), HighlightKind::Write),
                ("$total".into(), HighlightKind::Read),
            ]
        );
        assert_eq!(a.summary, "3 usages of '$total' found");
        assert_eq!(a.element, ElementKind::Variable);

        let a = lexical_highlights(PHP, "Order.php", pos(PHP, "$amount)", 0)).unwrap();
        let kinds: Vec<HighlightKind> = a.highlights.iter().map(|h| h.kind).collect();
        // Parameter (write), assignment source, the interpolated string, the `if`; not the
        // comment, the single-quoted string or the closure's own `$amount`.
        assert_eq!(
            kinds,
            vec![
                HighlightKind::Write,
                HighlightKind::Read,
                HighlightKind::Read,
                HighlightKind::Read
            ]
        );
    }

    #[test]
    fn keywords_and_comments_have_no_usages() {
        assert!(lexical_highlights(PHP, "Order.php", pos(PHP, "public", 0)).is_none());
        assert!(lexical_highlights(PHP, "Order.php", pos(PHP, "checked", 0)).is_none());
        let a = lexical_highlights(PHP, "Order.php", pos(PHP, "Order", 0)).unwrap();
        assert_eq!(a.highlights.len(), 1);
        assert_eq!(a.highlights[0].kind, HighlightKind::Write, "a declaration");
    }

    #[test]
    fn exit_points_of_the_function() {
        let a = exit_points(PHP, "Order.php", pos(PHP, "return $this", 0)).unwrap();
        assert_eq!(a.target, HighlightTarget::ExitPoints);
        assert_eq!(a.title, "add");
        let found: Vec<String> = spans(PHP, &a).into_iter().map(|(s, _)| s).collect();
        assert_eq!(
            found,
            vec![
                "throw new \\InvalidArgumentException('negative $amount')".to_string(),
                "return $this->total + $total".to_string(),
            ],
            "the closure's return is not an exit point of add()"
        );
        assert_eq!(a.summary, "2 exit points of 'add' found");
        assert!(exit_points(PHP, "Order.php", pos(PHP, "$total", 0)).is_none());
    }

    #[test]
    fn javascript_identifiers() {
        let js = "const total = 1;\n// total\nlet x = total + 2;\nx = `total`;\n";
        let a = lexical_highlights(js, "a.ts", pos(js, "total", 2)).unwrap();
        assert_eq!(
            spans(js, &a),
            vec![
                ("total".into(), HighlightKind::Write),
                ("total".into(), HighlightKind::Read)
            ]
        );
    }

    #[test]
    fn ranges_from_an_index_are_classified() {
        let text = "<?php\n$a = 1;\necho $a;\n";
        let ranges = [Range::on_line(1, 0, 2), Range::on_line(2, 5, 7)];
        let h = classify_ranges(text, &ranges, None);
        assert_eq!(h[0].kind, HighlightKind::Write);
        assert_eq!(h[1].kind, HighlightKind::Read);
        let h = classify_ranges(text, &ranges[1..], Some(ranges[0]));
        assert_eq!(h.len(), 2);
        assert_eq!(lsp_highlight_kind(Some(3)), HighlightKind::Write);
    }
}
