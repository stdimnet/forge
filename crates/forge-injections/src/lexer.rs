//! Host lexers: the string literals and comments of PHP and JavaScript/TypeScript sources.
//! They only need to be good enough to find literals reliably; everything else is skipped.

use serde::{Deserialize, Serialize};

/// The language a file is written in, as far as injections are concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Host {
    Php,
    Js,
}

impl Host {
    /// The host language of a file: PHP (not Blade) or JavaScript/TypeScript.
    pub fn of_path(path: &str) -> Option<Host> {
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".blade.php") {
            return None;
        }
        let name = lower.rsplit(['/', '\\']).next().unwrap_or(&lower);
        let (_, ext) = name.rsplit_once('.')?;
        match ext {
            "php" | "phtml" | "inc" | "module" => Some(Host::Php),
            "js" | "mjs" | "cjs" | "jsx" | "ts" | "mts" | "cts" | "tsx" => Some(Host::Js),
            _ => None,
        }
    }

    /// Display name, as in the Language Injections settings page.
    pub fn label(self) -> &'static str {
        match self {
            Host::Php => "PHP",
            Host::Js => "JavaScript",
        }
    }
}

/// How a string literal is written; decides its escaping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LiteralKind {
    /// `'…'` in PHP.
    PhpSingle,
    /// `"…"` in PHP (escapes and `$variables`).
    PhpDouble,
    /// `<<<LABEL` in PHP.
    PhpHeredoc,
    /// `<<<'LABEL'` in PHP (no escapes).
    PhpNowdoc,
    /// `'…'` in JavaScript.
    JsSingle,
    /// `"…"` in JavaScript.
    JsDouble,
    /// `` `…` `` in JavaScript (escapes and `${expressions}`).
    JsTemplate,
}

/// A string literal (byte offsets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Literal {
    /// The whole token, quotes or heredoc labels included.
    pub start: usize,
    pub end: usize,
    /// The text between the quotes (a heredoc's body without its last line break).
    pub content_start: usize,
    pub content_end: usize,
    pub kind: LiteralKind,
    /// The text parts of the content: interpolations (`$x`, `{$x}`, `${x}`) are left out.
    pub pieces: Vec<(usize, usize)>,
    /// The label of a heredoc or nowdoc.
    pub label: Option<String>,
}

/// A comment (byte offsets, delimiters included).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comment {
    pub start: usize,
    pub end: usize,
}

/// Literals and comments of a source, in source order (literals nested in JavaScript
/// template expressions follow their template).
#[derive(Debug, Default)]
pub struct Lexed {
    pub literals: Vec<Literal>,
    pub comments: Vec<Comment>,
    /// `true` for every byte inside a literal or comment.
    pub mask: Vec<bool>,
}

impl Lexed {
    fn finish(mut self, len: usize) -> Self {
        let mut mask = vec![false; len];
        for (s, e) in self
            .literals
            .iter()
            .map(|l| (l.start, l.end))
            .chain(self.comments.iter().map(|c| (c.start, c.end)))
        {
            for m in &mut mask[s.min(len)..e.min(len)] {
                *m = true;
            }
        }
        self.literals.sort_by_key(|l| l.start);
        self.comments.sort_by_key(|c| c.start);
        self.mask = mask;
        self
    }
}

pub fn lex(src: &str, host: Host) -> Lexed {
    match host {
        Host::Php => lex_php(src),
        Host::Js => lex_js(src),
    }
}

pub(crate) fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn line_end(b: &[u8], i: usize) -> usize {
    b[i..]
        .iter()
        .position(|&c| c == b'\n')
        .map_or(b.len(), |p| i + p)
}

// ---------------------------------------------------------------------------------- PHP

/// `<<<ID`, `<<< "ID"` or `<<<'ID'` followed by a newline: (header length, label, nowdoc).
fn heredoc_header(src: &str, i: usize) -> Option<(usize, &str, bool)> {
    let b = src.as_bytes();
    let mut j = i + 3;
    while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
        j += 1;
    }
    let quote = match b.get(j) {
        Some(&q @ (b'\'' | b'"')) => {
            j += 1;
            Some(q)
        }
        _ => None,
    };
    let id_start = j;
    if !b
        .get(j)
        .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
    {
        return None;
    }
    while j < b.len() && is_word(b[j]) {
        j += 1;
    }
    let id = &src[id_start..j];
    if let Some(q) = quote {
        if b.get(j) != Some(&q) {
            return None;
        }
        j += 1;
    }
    if b.get(j) == Some(&b'\r') {
        j += 1;
    }
    (b.get(j) == Some(&b'\n')).then_some((j + 1 - i, id, quote == Some(b'\'')))
}

/// The text parts of a PHP double-quoted string or heredoc body: `$var`, `$var->prop`,
/// `$var[…]`, `{$…}` and `${…}` are interpolations.
fn php_pieces(b: &[u8], start: usize, end: usize) -> Vec<(usize, usize)> {
    let mut pieces = Vec::new();
    let mut s = start;
    let mut j = start;
    let cut = |from: usize, to: usize, s: &mut usize, pieces: &mut Vec<(usize, usize)>| {
        if from > *s {
            pieces.push((*s, from));
        }
        *s = to;
    };
    while j < end {
        match b[j] {
            b'\\' => j += 2,
            b'$' if b
                .get(j + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_') =>
            {
                let from = j;
                j += 1;
                while j < end && is_word(b[j]) {
                    j += 1;
                }
                if b[j.min(end)..end].starts_with(b"->")
                    && b.get(j + 2)
                        .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
                {
                    j += 2;
                    while j < end && is_word(b[j]) {
                        j += 1;
                    }
                } else if j < end && b[j] == b'[' {
                    while j < end && b[j] != b']' {
                        j += 1;
                    }
                    j = (j + 1).min(end);
                }
                cut(from, j, &mut s, &mut pieces);
            }
            b'{' if b.get(j + 1) == Some(&b'$') => {
                let from = j;
                j = matching_brace(b, j, end);
                cut(from, j, &mut s, &mut pieces);
            }
            b'$' if b.get(j + 1) == Some(&b'{') => {
                let from = j;
                j = matching_brace(b, j + 1, end);
                cut(from, j, &mut s, &mut pieces);
            }
            _ => j += 1,
        }
    }
    if end > s {
        pieces.push((s, end));
    }
    pieces
}

/// Index after the `}` matching the `{` at `open` (or `end`).
fn matching_brace(b: &[u8], open: usize, end: usize) -> usize {
    let mut depth = 0;
    let mut j = open;
    while j < end {
        match b[j] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return j + 1;
                }
            }
            _ => {}
        }
        j += 1;
    }
    end
}

fn lex_php(src: &str) -> Lexed {
    let b = src.as_bytes();
    let mut out = Lexed::default();
    let mut in_php = false;
    let mut i = 0;
    while i < b.len() {
        if !in_php {
            match src[i..].find("<?") {
                Some(p) => {
                    i += p + 2;
                    if src[i..].starts_with("php") {
                        i += 3;
                    } else if src[i..].starts_with('=') {
                        i += 1;
                    }
                    in_php = true;
                    continue;
                }
                None => break,
            }
        }
        let c = b[i];
        if c == b'?' && b.get(i + 1) == Some(&b'>') {
            in_php = false;
            i += 2;
            continue;
        }
        if (c == b'/' && b.get(i + 1) == Some(&b'/')) || (c == b'#' && b.get(i + 1) != Some(&b'['))
        {
            // Line comments end at the line break or at `?>`.
            let mut end = line_end(b, i);
            if let Some(p) = src[i..end].find("?>") {
                end = i + p;
            }
            out.comments.push(Comment { start: i, end });
            i = end;
            continue;
        }
        if c == b'/' && b.get(i + 1) == Some(&b'*') {
            let end = src[i + 2..].find("*/").map_or(b.len(), |p| i + 2 + p + 2);
            out.comments.push(Comment { start: i, end });
            i = end;
            continue;
        }
        if c == b'\'' || c == b'"' || c == b'`' {
            let mut j = i + 1;
            while j < b.len() && b[j] != c {
                j += if b[j] == b'\\' { 2 } else { 1 };
            }
            let content_end = j.min(b.len());
            let end = (j + 1).min(b.len());
            if c != b'`' {
                let kind = if c == b'\'' {
                    LiteralKind::PhpSingle
                } else {
                    LiteralKind::PhpDouble
                };
                let pieces = if kind == LiteralKind::PhpSingle {
                    vec![(i + 1, content_end)]
                } else {
                    php_pieces(b, i + 1, content_end)
                };
                out.literals.push(Literal {
                    start: i,
                    end,
                    content_start: i + 1,
                    content_end,
                    kind,
                    pieces,
                    label: None,
                });
            }
            i = end;
            continue;
        }
        if b[i..].starts_with(b"<<<") {
            if let Some((header, id, nowdoc)) = heredoc_header(src, i) {
                let body_start = i + header;
                // The closing label: on its own line, optionally indented.
                let mut line = body_start;
                let mut close = None;
                while line < b.len() {
                    let mut k = line;
                    while k < b.len() && (b[k] == b' ' || b[k] == b'\t') {
                        k += 1;
                    }
                    if src[k..].starts_with(id) && !b.get(k + id.len()).is_some_and(|c| is_word(*c))
                    {
                        close = Some((line, k + id.len()));
                        break;
                    }
                    line = line_end(b, line) + 1;
                }
                let (body_end, end) = close.unwrap_or((b.len(), b.len()));
                let mut content_end = body_end.saturating_sub(1).max(body_start);
                if content_end > body_start && b[content_end - 1] == b'\r' {
                    content_end -= 1;
                }
                let content_end = content_end.min(b.len());
                let kind = if nowdoc {
                    LiteralKind::PhpNowdoc
                } else {
                    LiteralKind::PhpHeredoc
                };
                let pieces = if nowdoc {
                    vec![(body_start, content_end)]
                } else {
                    php_pieces(b, body_start, content_end)
                };
                out.literals.push(Literal {
                    start: i,
                    end,
                    content_start: body_start.min(content_end),
                    content_end,
                    kind,
                    pieces,
                    label: Some(id.to_string()),
                });
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out.finish(b.len())
}

// ----------------------------------------------------------------------------- JavaScript

/// Words after which a `/` starts a regular expression.
const REGEX_AFTER_WORDS: &[&str] = &[
    "return",
    "typeof",
    "instanceof",
    "in",
    "of",
    "new",
    "delete",
    "void",
    "throw",
    "case",
    "do",
    "else",
    "yield",
    "await",
];

struct JsLexer<'a> {
    src: &'a str,
    b: &'a [u8],
    out: Lexed,
}

impl JsLexer<'_> {
    /// Lex code from `i`; with `in_template` stop after the `}` closing a `${`.
    fn code(&mut self, mut i: usize, in_template: bool) -> usize {
        let b = self.b;
        let mut depth = 0usize;
        // The last significant byte and whether it ended a word.
        let mut last: Option<u8> = None;
        let mut last_word: &str = "";
        while i < b.len() {
            let c = b[i];
            if c.is_ascii_whitespace() {
                i += 1;
                continue;
            }
            if c == b'/' && b.get(i + 1) == Some(&b'/') {
                let end = line_end(b, i);
                self.out.comments.push(Comment { start: i, end });
                i = end;
                continue;
            }
            if c == b'/' && b.get(i + 1) == Some(&b'*') {
                let end = self.src[i + 2..]
                    .find("*/")
                    .map_or(b.len(), |p| i + 2 + p + 2);
                self.out.comments.push(Comment { start: i, end });
                i = end;
                continue;
            }
            if c == b'\'' || c == b'"' {
                let mut j = i + 1;
                while j < b.len() && b[j] != c && b[j] != b'\n' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                if j < b.len() && b[j] == c {
                    self.out.literals.push(Literal {
                        start: i,
                        end: j + 1,
                        content_start: i + 1,
                        content_end: j,
                        kind: if c == b'\'' {
                            LiteralKind::JsSingle
                        } else {
                            LiteralKind::JsDouble
                        },
                        pieces: vec![(i + 1, j)],
                        label: None,
                    });
                    i = j + 1;
                } else {
                    // Unterminated (or JSX text such as `it's`): skip the rest of the line.
                    i = j.min(b.len());
                }
                last = Some(b'"');
                last_word = "";
                continue;
            }
            if c == b'`' {
                i = self.template(i);
                last = Some(b'`');
                last_word = "";
                continue;
            }
            if c == b'/' {
                let regex = match last {
                    None => true,
                    Some(p) if is_word(p) || p == b'$' => REGEX_AFTER_WORDS.contains(&last_word),
                    Some(p) => !matches!(p, b')' | b']' | b'"' | b'`' | b'}'),
                };
                if regex {
                    if let Some(end) = self.regex_end(i) {
                        i = end;
                        last = Some(b'"');
                        last_word = "";
                        continue;
                    }
                }
            }
            if is_word(c) || c == b'$' {
                let s = i;
                while i < b.len() && (is_word(b[i]) || b[i] == b'$') {
                    i += 1;
                }
                last = Some(b[i - 1]);
                last_word = &self.src[s..i];
                continue;
            }
            match c {
                b'{' => depth += 1,
                b'}' if depth > 0 => depth -= 1,
                b'}' if in_template => return i + 1,
                _ => {}
            }
            last = Some(c);
            last_word = "";
            i += 1;
        }
        i
    }

    /// End of a regular expression literal starting at `i`, `None` when it is not one.
    fn regex_end(&self, i: usize) -> Option<usize> {
        let b = self.b;
        let mut j = i + 1;
        let mut class = false;
        while j < b.len() {
            match b[j] {
                b'\\' => j += 1,
                b'\n' => return None,
                b'[' => class = true,
                b']' => class = false,
                b'/' if !class => break,
                _ => {}
            }
            j += 1;
        }
        if j >= b.len() {
            return None;
        }
        j += 1;
        while j < b.len() && b[j].is_ascii_alphabetic() {
            j += 1;
        }
        Some(j)
    }

    /// A template literal at `i`; returns the index after it.
    fn template(&mut self, i: usize) -> usize {
        let b = self.b;
        let mut pieces = Vec::new();
        let mut s = i + 1;
        let mut j = i + 1;
        while j < b.len() && b[j] != b'`' {
            if b[j] == b'\\' {
                j += 2;
            } else if b[j] == b'$' && b.get(j + 1) == Some(&b'{') {
                if j > s {
                    pieces.push((s, j));
                }
                j = self.code(j + 2, true);
                s = j;
            } else {
                j += 1;
            }
        }
        let content_end = j.min(b.len());
        if content_end > s {
            pieces.push((s, content_end));
        }
        let end = (j + 1).min(b.len());
        self.out.literals.push(Literal {
            start: i,
            end,
            content_start: i + 1,
            content_end,
            kind: LiteralKind::JsTemplate,
            pieces,
            label: None,
        });
        end
    }
}

fn lex_js(src: &str) -> Lexed {
    let mut lexer = JsLexer {
        src,
        b: src.as_bytes(),
        out: Lexed::default(),
    };
    lexer.code(0, false);
    let out = std::mem::take(&mut lexer.out);
    out.finish(src.len())
}
