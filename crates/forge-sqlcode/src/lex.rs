//! An error-tolerant SQL tokenizer with byte offsets: words, quoted identifiers, strings,
//! numbers, comments, user parameters and punctuation, following the dialect's quoting
//! rules (backticks, brackets, `#` comments, backslash escapes, dollar quotes).

use crate::lang::SqlDialect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A plain word: keyword or unquoted name.
    Word,
    /// `"name"`, `` `name` `` or `[name]`.
    Quoted,
    Str,
    Number,
    Comment,
    /// `?`, `:name`, `$1`, `${name}`, `#{name}`.
    Param,
    /// `@var` / `@@global` (SQL Server, MySQL variables).
    Variable,
    Op,
    Open,
    Close,
    Comma,
    Semi,
    Dot,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
}

impl Token {
    pub fn text<'a>(&self, src: &'a str) -> &'a str {
        &src[self.start..self.end]
    }

    /// Whether the token is the word `kw` (any case).
    pub fn is_kw(&self, src: &str, kw: &str) -> bool {
        self.kind == Kind::Word && self.text(src).eq_ignore_ascii_case(kw)
    }

    /// Whether it can be a name: a word or a quoted identifier.
    pub fn is_name(&self) -> bool {
        matches!(self.kind, Kind::Word | Kind::Quoted)
    }

    /// The name without quotes (`"a""b"` → `a"b`).
    pub fn name(&self, src: &str) -> String {
        let t = self.text(src);
        if self.kind != Kind::Quoted || t.len() < 2 {
            return t.to_string();
        }
        let (open, inner) = (t.as_bytes()[0], &t[1..t.len() - 1]);
        match open {
            b'"' => inner.replace("\"\"", "\""),
            b'`' => inner.replace("``", "`"),
            b'[' => inner.replace("]]", "]"),
            _ => inner.to_string(),
        }
    }

    pub fn is_significant(&self) -> bool {
        self.kind != Kind::Comment
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Tokens of `src` (whitespace dropped, comments kept).
pub fn lex(src: &str, d: SqlDialect) -> Vec<Token> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let push = |out: &mut Vec<Token>, kind, start, end| out.push(Token { kind, start, end });
    while i < b.len() {
        let c = b[i];
        let start = i;
        let rest = &src[i..];
        let ch = rest.chars().next().unwrap_or(' ');
        if ch.is_whitespace() {
            i += ch.len_utf8();
            continue;
        }
        // Comments.
        if rest.starts_with("--") || (c == b'#' && d.hash_comments() && !rest.starts_with("#{")) {
            let end = rest.find('\n').map(|p| i + p).unwrap_or(b.len());
            push(&mut out, Kind::Comment, start, end);
            i = end;
            continue;
        }
        if rest.starts_with("/*") {
            let end = rest[2..].find("*/").map(|p| i + p + 4).unwrap_or(b.len());
            push(&mut out, Kind::Comment, start, end);
            i = end;
            continue;
        }
        // Strings (with E'', N'', X'' prefixes) and quoted identifiers.
        let prefixed = matches!(c, b'E' | b'e' | b'N' | b'n' | b'X' | b'x' | b'B' | b'b')
            && b.get(i + 1) == Some(&b'\'');
        if c == b'\'' || prefixed || (c == b'"' && d.double_quoted_strings()) {
            let q = if prefixed { b'\'' } else { c };
            let mut j = if prefixed { i + 2 } else { i + 1 };
            let backslash = d.backslash_escapes() || (prefixed && (c == b'E' || c == b'e'));
            while j < b.len() {
                if backslash && b[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if b[j] == q {
                    if b.get(j + 1) == Some(&q) {
                        j += 2;
                        continue;
                    }
                    j += 1;
                    break;
                }
                j += 1;
            }
            let end = j.min(b.len());
            push(&mut out, Kind::Str, start, end);
            i = end;
            continue;
        }
        let close = match c {
            b'"' => Some(b'"'),
            b'`' if d.backticks() => Some(b'`'),
            b'[' if d.brackets() => Some(b']'),
            _ => None,
        };
        if let Some(q) = close {
            let mut j = i + 1;
            while j < b.len() {
                if b[j] == q {
                    if b.get(j + 1) == Some(&q) {
                        j += 2;
                        continue;
                    }
                    j += 1;
                    break;
                }
                if b[j] == b'\n' && q == b']' {
                    break;
                }
                j += 1;
            }
            let end = j.min(b.len());
            push(&mut out, Kind::Quoted, start, end);
            i = end;
            continue;
        }
        // Dollar quotes ($$ … $$, $tag$ … $tag$) and parameters ($1, ${name}).
        if c == b'$' {
            if d.dollar_quotes() {
                let tag_len = rest[1..]
                    .find(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
                    .map(|p| p + 1);
                if let Some(tl) = tag_len {
                    let tag = &rest[..=tl];
                    if tag.ends_with('$') && !tag[1..tl].starts_with(|c: char| c.is_ascii_digit()) {
                        let end = rest[tag.len()..]
                            .find(tag)
                            .map(|p| i + tag.len() + p + tag.len())
                            .unwrap_or(b.len());
                        push(&mut out, Kind::Str, start, end);
                        i = end;
                        continue;
                    }
                }
            }
            if rest.starts_with("${") {
                let end = rest.find('}').map(|p| i + p + 1).unwrap_or(b.len());
                push(&mut out, Kind::Param, start, end);
                i = end;
                continue;
            }
            let digits = rest[1..].bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 {
                push(&mut out, Kind::Param, start, i + 1 + digits);
                i += 1 + digits;
                continue;
            }
        }
        if rest.starts_with("#{") {
            let end = rest.find('}').map(|p| i + p + 1).unwrap_or(b.len());
            push(&mut out, Kind::Param, start, end);
            i = end;
            continue;
        }
        if c == b'?' && !matches!(b.get(i + 1), Some(b'|' | b'&')) {
            push(&mut out, Kind::Param, start, i + 1);
            i += 1;
            continue;
        }
        if c == b':'
            && b.get(i + 1) != Some(&b':')
            && !matches!(out.last(), Some(t) if t.kind == Kind::Op && t.end == i && &src[t.start..t.end] == ":")
        {
            let name = rest[1..].chars().next().filter(|c| is_ident_start(*c));
            if name.is_some() {
                let len = rest[1..]
                    .find(|ch: char| !is_ident_char(ch))
                    .unwrap_or(rest.len() - 1);
                push(&mut out, Kind::Param, start, i + 1 + len);
                i += 1 + len;
                continue;
            }
        }
        if c == b'@' {
            let len = rest
                .find(|ch: char| !(is_ident_char(ch) || ch == '@'))
                .unwrap_or(rest.len());
            if len > 1 {
                push(&mut out, Kind::Variable, start, i + len);
                i += len;
                continue;
            }
        }
        if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let mut j = i;
            if rest.starts_with("0x") || rest.starts_with("0X") {
                j += 2;
                while j < b.len() && b[j].is_ascii_hexdigit() {
                    j += 1;
                }
            } else {
                while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'.') {
                    j += 1;
                }
                if j < b.len() && (b[j] == b'e' || b[j] == b'E') {
                    let k = j + 1 + usize::from(matches!(b.get(j + 1), Some(b'+' | b'-')));
                    if b.get(k).is_some_and(u8::is_ascii_digit) {
                        j = k;
                        while j < b.len() && b[j].is_ascii_digit() {
                            j += 1;
                        }
                    }
                }
            }
            push(&mut out, Kind::Number, start, j);
            i = j;
            continue;
        }
        if is_ident_start(ch) {
            let len = rest.find(|c: char| !is_ident_char(c)).unwrap_or(rest.len());
            push(&mut out, Kind::Word, start, i + len);
            i += len;
            continue;
        }
        let (kind, len) = match c {
            b'(' => (Kind::Open, 1),
            b')' => (Kind::Close, 1),
            b',' => (Kind::Comma, 1),
            b';' => (Kind::Semi, 1),
            b'.' => (Kind::Dot, 1),
            _ => {
                const OPS: &[&str] = &[
                    "->>", "<=>", "::", "<=", ">=", "<>", "!=", "||", "->", ":=", "=>", "#>>",
                    "#>", "@>", "<@", "&&", "<<", ">>", "=", "<", ">", "+", "-", "*", "/", "%",
                    "|", "&", "^", "~", "!", ":",
                ];
                match OPS.iter().find(|o| rest.starts_with(**o)) {
                    Some(o) => (Kind::Op, o.len()),
                    None => (Kind::Other, ch.len_utf8()),
                }
            }
        };
        push(&mut out, kind, start, i + len);
        i += len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str, d: SqlDialect) -> Vec<(Kind, &str)> {
        lex(src, d)
            .into_iter()
            .map(|t| (t.kind, &src[t.start..t.end]))
            .collect()
    }

    #[test]
    fn tokens_and_quotes() {
        let k = kinds(
            "SELECT u.\"Name\", `x` FROM t -- c\nWHERE a::int >= 1.5e3 AND b = 'it''s'",
            SqlDialect::Postgres,
        );
        assert_eq!(k[1], (Kind::Word, "u"));
        assert_eq!(k[2], (Kind::Dot, "."));
        assert_eq!(k[3], (Kind::Quoted, "\"Name\""));
        assert!(k.contains(&(Kind::Comment, "-- c")));
        assert!(k.contains(&(Kind::Op, "::")));
        assert!(k.contains(&(Kind::Number, "1.5e3")));
        assert!(k.contains(&(Kind::Str, "'it''s'")));
    }

    #[test]
    fn dialect_quirks() {
        let my = kinds("SELECT \"a\" # note\n, 'x\\'y'", SqlDialect::MySql);
        assert_eq!(my[1], (Kind::Str, "\"a\""));
        assert_eq!(my[2], (Kind::Comment, "# note"));
        assert_eq!(my[4], (Kind::Str, "'x\\'y'"));
        let ms = kinds("SELECT [order id] FROM [dbo].[t]", SqlDialect::SqlServer);
        assert_eq!(ms[1], (Kind::Quoted, "[order id]"));
        let pg = kinds("SELECT $$a;b$$, $fn$x$fn$, $1", SqlDialect::Postgres);
        assert_eq!(pg[1], (Kind::Str, "$$a;b$$"));
        assert_eq!(pg[3], (Kind::Str, "$fn$x$fn$"));
        assert_eq!(pg[5], (Kind::Param, "$1"));
    }

    #[test]
    fn parameters() {
        let k = kinds(
            "WHERE a = ? AND b = :name AND c = ${x} AND d = #{y} AND e::text = @v",
            SqlDialect::MySql,
        );
        let params: Vec<&str> = k
            .iter()
            .filter(|(k, _)| *k == Kind::Param)
            .map(|(_, s)| *s)
            .collect();
        assert_eq!(params, ["?", ":name", "${x}", "#{y}"]);
        assert!(k.contains(&(Kind::Variable, "@v")));
    }

    #[test]
    fn unquotes_names() {
        let src = "\"a\"\"b\"";
        let t = lex(src, SqlDialect::Generic)[0];
        assert_eq!(t.name(src), "a\"b");
    }
}
