//! A small, forgiving Python scanner: tokens grouped into logical lines (brackets and `\`
//! continuations joined), comments dropped, string literals with their content span. The
//! framework analyses read calls, decorators, assignments and class bodies from it; there
//! is no full parser, so unusual code is skipped instead of failing.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Name,
    Str,
    Num,
    Op,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    /// The name, number or operator; the content of a string (escapes kept as written).
    pub text: String,
    /// Byte range of the token in the source.
    pub start: usize,
    pub end: usize,
    /// Strings: byte range of the content between the quotes.
    pub inner: Range<usize>,
    /// 0-based line of the token's start.
    pub line: u32,
    /// An f-string (its content is not a plain value).
    pub fstring: bool,
}

impl Token {
    pub fn is_op(&self, op: &str) -> bool {
        self.kind == Kind::Op && self.text == op
    }
    pub fn is_name(&self, name: &str) -> bool {
        self.kind == Kind::Name && self.text == name
    }
    /// A plain (not f-) string literal.
    pub fn is_plain_str(&self) -> bool {
        self.kind == Kind::Str && !self.fstring
    }
}

/// A logical line: its indentation (columns of the first physical line) and tokens.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Line {
    pub indent: usize,
    pub tokens: Vec<Token>,
}

impl Line {
    pub fn first_line(&self) -> u32 {
        self.tokens.first().map(|t| t.line).unwrap_or(0)
    }
}

const TWO_CHAR_OPS: [&str; 13] = [
    "==", "!=", "<=", ">=", "+=", "-=", "*=", "/=", "|=", "->", "**", ":=", "//",
];

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// Length in bytes of a string prefix plus opening quote at `i` (`rb"`, `f'`, `"""`), and
/// the quote; `None` when no string starts there.
fn string_start(src: &str, i: usize) -> Option<(usize, &'static str, bool, bool)> {
    let rest = &src[i..];
    let mut prefix = 0;
    let mut raw = false;
    let mut f = false;
    for c in rest.chars().take(3) {
        match c {
            'r' | 'R' => raw = true,
            'f' | 'F' => f = true,
            'b' | 'B' | 'u' | 'U' => {}
            _ => break,
        }
        prefix += 1;
    }
    let after = &rest[prefix..];
    let quote = if after.starts_with("\"\"\"") {
        "\"\"\""
    } else if after.starts_with("'''") {
        "'''"
    } else if after.starts_with('"') {
        "\""
    } else if after.starts_with('\'') {
        "'"
    } else {
        return None;
    };
    if prefix > 2 {
        return None;
    }
    Some((prefix + quote.len(), quote, raw, f))
}

/// Tokenize `src` into logical lines.
pub fn lines(src: &str) -> Vec<Line> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut cur = Line::default();
    let mut depth: i32 = 0;
    let mut line: u32 = 0;
    let mut line_start = 0usize;
    let mut i = 0usize;
    let mut continued = false;
    let push = |cur: &mut Line, out: &mut Vec<Line>| {
        if !cur.tokens.is_empty() {
            out.push(std::mem::take(cur));
        }
    };
    while i < src.len() {
        let c = src[i..].chars().next().expect("char");
        let len = c.len_utf8();
        match c {
            '\n' => {
                line += 1;
                i += 1;
                line_start = i;
                if depth <= 0 && !continued {
                    depth = 0;
                    push(&mut cur, &mut out);
                }
                continued = false;
                continue;
            }
            '\\' if bytes.get(i + 1) == Some(&b'\n') || src[i + 1..].starts_with("\r\n") => {
                continued = true;
                i += 1;
                continue;
            }
            '#' => {
                while i < src.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            c if c.is_whitespace() => {
                i += len;
                continue;
            }
            _ => {}
        }
        if cur.tokens.is_empty() {
            cur.indent = src[line_start..i].chars().count();
        }
        if let Some((open, quote, raw, f)) = string_start(src, i) {
            let content_start = i + open;
            let mut j = content_start;
            let mut end = None;
            while j < src.len() {
                if src[j..].starts_with(quote) {
                    end = Some(j);
                    break;
                }
                let ch = src[j..].chars().next().expect("char");
                if ch == '\\' {
                    // Raw strings keep the backslash, but it still protects a quote.
                    let _ = raw;
                    j += 1;
                    if j < src.len() {
                        let next = src[j..].chars().next().expect("char");
                        if next == '\n' {
                            line += 1;
                        }
                        j += next.len_utf8();
                    }
                    continue;
                }
                if ch == '\n' {
                    if quote.len() == 1 {
                        break; // unterminated single-quoted string
                    }
                    line += 1;
                }
                j += ch.len_utf8();
            }
            let start_line = line - src[content_start..j].matches('\n').count() as u32;
            let close = end.map(|e| e + quote.len()).unwrap_or(j);
            cur.tokens.push(Token {
                kind: Kind::Str,
                text: src[content_start..j].to_string(),
                start: i,
                end: close,
                inner: content_start..j,
                line: start_line,
                fstring: f,
            });
            if end.is_none() && quote.len() == 1 && j < src.len() {
                // Let the newline end the logical line.
                i = j;
            } else {
                i = close;
            }
            if src[line_start.min(i)..i].contains('\n') {
                line_start = src[..i].rfind('\n').map(|p| p + 1).unwrap_or(0);
            }
            continue;
        }
        if is_ident_start(c) {
            let start = i;
            while i < src.len() {
                let ch = src[i..].chars().next().expect("char");
                if !is_ident(ch) {
                    break;
                }
                i += ch.len_utf8();
            }
            cur.tokens.push(Token {
                kind: Kind::Name,
                text: src[start..i].to_string(),
                start,
                end: i,
                inner: start..i,
                line,
                fstring: false,
            });
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            while i < src.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.' || bytes[i] == b'_')
            {
                i += 1;
            }
            cur.tokens.push(Token {
                kind: Kind::Num,
                text: src[start..i].to_string(),
                start,
                end: i,
                inner: start..i,
                line,
                fstring: false,
            });
            continue;
        }
        let two = src.get(i..i + 2).unwrap_or("");
        let op = if TWO_CHAR_OPS.contains(&two) {
            two.to_string()
        } else {
            c.to_string()
        };
        match op.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth -= 1,
            _ => {}
        }
        let start = i;
        i += op.len();
        cur.tokens.push(Token {
            kind: Kind::Op,
            text: op,
            start,
            end: i,
            inner: start..i,
            line,
            fstring: false,
        });
    }
    push(&mut cur, &mut out);
    out
}

/// One argument of a call: `name=` for keyword arguments and its tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg<'a> {
    pub keyword: Option<&'a str>,
    pub tokens: &'a [Token],
}

impl<'a> Arg<'a> {
    /// The value of a plain string literal argument (adjacent literals joined).
    pub fn string(&self) -> Option<String> {
        if self.tokens.is_empty() || !self.tokens.iter().all(Token::is_plain_str) {
            return None;
        }
        Some(self.tokens.iter().map(|t| t.text.as_str()).collect())
    }
    /// The single string token of the argument, for spans.
    pub fn string_token(&self) -> Option<&'a Token> {
        match self.tokens {
            [t] if t.is_plain_str() => Some(t),
            _ => None,
        }
    }
    /// `a.b.c` when the argument is just a dotted name.
    pub fn dotted(&self) -> Option<String> {
        dotted_exact(self.tokens)
    }
    /// The plain strings of a list or tuple literal (`["GET", "POST"]`).
    pub fn strings(&self) -> Vec<String> {
        self.tokens
            .iter()
            .filter(|t| t.is_plain_str())
            .map(|t| t.text.clone())
            .collect()
    }
    /// A call argument (`include("x")`): the callee and its arguments.
    pub fn call(&self) -> Option<(String, Vec<Arg<'a>>)> {
        let open = self.tokens.iter().position(|t| t.is_op("("))?;
        let callee = dotted_exact(&self.tokens[..open])?;
        let (args, close) = call_args(self.tokens, open)?;
        // `views.Detail.as_view()` is a call too; only accept a call that ends the argument.
        (close + 1 == self.tokens.len()).then_some((callee, args))
    }
    pub fn is_true(&self) -> bool {
        matches!(self.tokens, [t] if t.is_name("True"))
    }
    pub fn is_false(&self) -> bool {
        matches!(self.tokens, [t] if t.is_name("False"))
    }
}

/// `a.b.c` when `tokens` are exactly a dotted name.
pub fn dotted_exact(tokens: &[Token]) -> Option<String> {
    if tokens.is_empty() || tokens.len().is_multiple_of(2) {
        return None;
    }
    let mut out = String::new();
    for (i, t) in tokens.iter().enumerate() {
        if i % 2 == 0 {
            if t.kind != Kind::Name {
                return None;
            }
            out.push_str(&t.text);
        } else if t.is_op(".") {
            out.push('.');
        } else {
            return None;
        }
    }
    Some(out)
}

/// The dotted name that ends right before `idx` (`app.route` before `(`), with its first
/// token's index.
pub fn dotted_before(tokens: &[Token], idx: usize) -> Option<(String, usize)> {
    let mut i = idx;
    let mut parts: Vec<&str> = Vec::new();
    loop {
        if i == 0 || tokens[i - 1].kind != Kind::Name {
            return None;
        }
        i -= 1;
        parts.push(&tokens[i].text);
        if i >= 1 && tokens[i - 1].is_op(".") {
            i -= 1;
            continue;
        }
        break;
    }
    parts.reverse();
    Some((parts.join("."), i))
}

/// The arguments of the call whose `(` is at `open`, and the index of its `)`.
pub fn call_args(tokens: &[Token], open: usize) -> Option<(Vec<Arg<'_>>, usize)> {
    let mut depth = 0i32;
    let mut args = Vec::new();
    let mut start = open + 1;
    for (i, t) in tokens.iter().enumerate().skip(open) {
        if t.kind != Kind::Op {
            continue;
        }
        match t.text.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => {
                depth -= 1;
                if depth == 0 {
                    if start < i {
                        args.push(make_arg(&tokens[start..i]));
                    }
                    return Some((args, i));
                }
            }
            "," if depth == 1 => {
                if start < i {
                    args.push(make_arg(&tokens[start..i]));
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    None
}

fn make_arg(tokens: &[Token]) -> Arg<'_> {
    if tokens.len() >= 2 && tokens[0].kind == Kind::Name && tokens[1].is_op("=") {
        Arg {
            keyword: Some(&tokens[0].text),
            tokens: &tokens[2..],
        }
    } else {
        Arg {
            keyword: None,
            tokens,
        }
    }
}

/// The items of a list, tuple or set literal whose opening bracket is at `open`.
pub fn list_items(tokens: &[Token], open: usize) -> Vec<&[Token]> {
    let mut depth = 0i32;
    let mut items = Vec::new();
    let mut start = open + 1;
    for (i, t) in tokens.iter().enumerate().skip(open) {
        if t.kind != Kind::Op {
            continue;
        }
        match t.text.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => {
                depth -= 1;
                if depth == 0 {
                    if start < i {
                        items.push(&tokens[start..i]);
                    }
                    return items;
                }
            }
            "," if depth == 1 => {
                if start < i {
                    items.push(&tokens[start..i]);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    items
}

pub fn keyword<'a>(args: &'a [Arg<'a>], name: &str) -> Option<&'a Arg<'a>> {
    args.iter().find(|a| a.keyword == Some(name))
}

pub fn positional<'a>(args: &'a [Arg<'a>], n: usize) -> Option<&'a Arg<'a>> {
    args.iter().filter(|a| a.keyword.is_none()).nth(n)
}

/// `name = <value>` or `name: T = <value>` at the start of a line: the name and the index
/// of the first value token.
pub fn assignment(line: &Line) -> Option<(&str, usize)> {
    let t = &line.tokens;
    if t.len() >= 3 && t[0].kind == Kind::Name && (t[1].is_op("=") || t[1].is_op("+=")) {
        return Some((&t[0].text, 2));
    }
    if t.len() >= 3 && t[0].kind == Kind::Name && t[1].is_op(":") {
        let eq = t.iter().position(|x| x.is_op("="))?;
        return (eq + 1 < t.len()).then_some((&t[0].text, eq + 1));
    }
    None
}

/// `def name` / `async def name` / `class Name` of a line: (keyword, name).
pub fn definition(line: &Line) -> Option<(&str, &Token)> {
    let t = &line.tokens;
    let i = usize::from(t.first().is_some_and(|x| x.is_name("async")));
    match (t.get(i), t.get(i + 1)) {
        (Some(k), Some(n)) if (k.is_name("def") || k.is_name("class")) && n.kind == Kind::Name => {
            Some((&k.text, n))
        }
        _ => None,
    }
}

/// The decorators above each definition: for line index `def_line`, the lines starting
/// with `@` right before it.
pub fn decorators(lines: &[Line], def_line: usize) -> &[Line] {
    let mut start = def_line;
    while start > 0
        && lines[start - 1]
            .tokens
            .first()
            .is_some_and(|t| t.is_op("@"))
    {
        start -= 1;
    }
    &lines[start..def_line]
}

/// Byte offset → (0-based line, UTF-16 column).
#[derive(Debug, Clone)]
pub struct LineIndex<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        LineIndex { text, starts }
    }
    pub fn position(&self, offset: usize) -> (u32, u32) {
        let offset = offset.min(self.text.len());
        let line = self
            .starts
            .partition_point(|&s| s <= offset)
            .saturating_sub(1);
        let start = self.starts[line];
        let col = self.text[start..offset].encode_utf16().count() as u32;
        (line as u32, col)
    }
}

/// Byte offset of a UTF-16 offset.
pub fn byte_offset(text: &str, utf16: usize) -> usize {
    pyforge_lang_python::completion::byte_offset(text, utf16 as u32)
}

/// UTF-16 offset of a byte offset.
pub fn utf16_offset(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].encode_utf16().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_brackets_and_drops_comments() {
        let src = "urlpatterns = [\n    path('a/', views.a, name='a'),  # first\n    path(\"b/\", views.b),\n]\nx = 1\n";
        let l = lines(src);
        assert_eq!(l.len(), 2);
        let strs: Vec<_> = l[0]
            .tokens
            .iter()
            .filter(|t| t.kind == Kind::Str)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(strs, ["a/", "a", "b/"]);
        assert_eq!(l[1].tokens[0].line, 4);
        assert_eq!(&src[l[0].tokens[5].inner.clone()], "a/");
    }

    #[test]
    fn strings_with_prefixes_and_triple_quotes() {
        let src = "x = r'^a\\'b$'\ny = \"\"\"doc\nstring\"\"\"\nz = f\"{a}\"\n@app.get('/p')\nasync def h(): pass\n";
        let l = lines(src);
        assert_eq!(l.len(), 5);
        assert_eq!(l[0].tokens[2].text, "^a\\'b$");
        assert_eq!(l[1].tokens[2].text, "doc\nstring");
        assert!(l[2].tokens[2].fstring);
        assert_eq!(l[3].tokens[0].line, 4);
        let (k, n) = definition(&l[4]).unwrap();
        assert_eq!((k, n.text.as_str()), ("def", "h"));
        assert_eq!(decorators(&l, 4).len(), 1);
    }

    #[test]
    fn indentation_and_continuations() {
        let src = "class A(models.Model):\n    name = models.CharField(\n        max_length=10)\n    x = 1 + \\\n        2\n";
        let l = lines(src);
        assert_eq!(l.len(), 3);
        assert_eq!(l[0].indent, 0);
        assert_eq!(l[1].indent, 4);
        assert_eq!(l[2].indent, 4);
    }

    #[test]
    fn call_arguments() {
        let src =
            "path('p/<int:pk>/', include(('shop.urls', 'shop'), namespace='shop'), name='x')\n";
        let l = lines(src);
        let t = &l[0].tokens;
        let (args, close) = call_args(t, 1).unwrap();
        assert_eq!(close, t.len() - 1);
        assert_eq!(args.len(), 3);
        assert_eq!(args[0].string().as_deref(), Some("p/<int:pk>/"));
        let (callee, inner) = args[1].call().unwrap();
        assert_eq!(callee, "include");
        assert_eq!(inner[0].strings(), ["shop.urls", "shop"]);
        assert_eq!(
            keyword(&inner, "namespace").unwrap().string().as_deref(),
            Some("shop")
        );
        assert_eq!(
            keyword(&args, "name").unwrap().string().as_deref(),
            Some("x")
        );
        assert_eq!(dotted_before(t, 1).unwrap().0, "path");
    }

    #[test]
    fn positions_are_utf16() {
        let text = "a = 'ä'\nb = '😀x'\n";
        let idx = LineIndex::new(text);
        let x = text.find('x').unwrap();
        assert_eq!(idx.position(x), (1, 7));
        assert_eq!(byte_offset(text, utf16_offset(text, x)), x);
    }
}
