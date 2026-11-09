//! A small SQL tokenizer per dialect, the base of the read-only guard and of the data
//! editor's filter check. It knows what each dialect treats as a string, a quoted
//! identifier or a comment: `E'…'` and `$tag$…$tag$` (PostgreSQL), backslash escapes
//! (MySQL, ClickHouse), `[…]` identifiers (SQL Server, SQLite), `q'[…]'` (Oracle), nested
//! block comments (PostgreSQL, SQL Server) and MySQL's executable `/*! … */` comments,
//! whose content is code.
//!
//! Some of these rules depend on server settings (`NO_BACKSLASH_ESCAPES` and `ANSI_QUOTES`
//! in MySQL, `standard_conforming_strings` in PostgreSQL). [`variants`] lists every
//! plausible reading, so a check can require that all of them agree.

use crate::dialect::Dialect;

/// The kind of a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tok<'a> {
    /// A bare word: keyword or identifier as written. In MySQL and SQL Server a leading
    /// `@` or `@@` belongs to the word (`@x`, `@@global`), in SQL Server also `#` (`#tmp`).
    Word(&'a str),
    /// A quoted identifier: `"x"`, `` `x` `` or `[x]`.
    Ident,
    /// A string literal, also `E'…'`, `N'…'`, `q'[…]'` and `$$…$$`.
    Str,
    Number,
    /// `;`, the end of a statement.
    Semi,
    /// Any other character: `(`, `)`, `,`, `=`, `.`, ….
    Punct(char),
}

/// A token with its byte range and its parenthesis depth (before it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token<'a> {
    pub tok: Tok<'a>,
    pub start: usize,
    pub end: usize,
    pub depth: i32,
}

impl<'a> Token<'a> {
    /// The word, if the token is one.
    pub fn word(&self) -> Option<&'a str> {
        match self.tok {
            Tok::Word(w) => Some(w),
            _ => None,
        }
    }

    /// Whether the token is the word `kw` (ASCII case-insensitive).
    pub fn is(&self, kw: &str) -> bool {
        self.word().is_some_and(|w| w.eq_ignore_ascii_case(kw))
    }
}

/// The tokens of a text, without whitespace and comments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lexed<'a> {
    pub tokens: Vec<Token<'a>>,
    /// A string, quoted identifier or block comment is not closed.
    pub unterminated: bool,
}

/// Server settings that change how a text is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LexOptions {
    /// `\` escapes the next character in `'…'` strings.
    pub backslash_single: bool,
    /// `\` escapes the next character in `"…"` (a string in MySQL, unless `ANSI_QUOTES`).
    pub backslash_double: bool,
}

/// Every plausible reading of a dialect: MySQL with and without `NO_BACKSLASH_ESCAPES` and
/// `ANSI_QUOTES`, PostgreSQL with and without `standard_conforming_strings`.
pub fn variants(d: Dialect) -> &'static [LexOptions] {
    const PLAIN: LexOptions = LexOptions {
        backslash_single: false,
        backslash_double: false,
    };
    const BS_SINGLE: LexOptions = LexOptions {
        backslash_single: true,
        backslash_double: false,
    };
    const BS_BOTH: LexOptions = LexOptions {
        backslash_single: true,
        backslash_double: true,
    };
    match d {
        Dialect::MySql | Dialect::ClickHouse => &[BS_BOTH, BS_SINGLE, PLAIN],
        Dialect::Postgres => &[PLAIN, BS_SINGLE],
        _ => &[PLAIN],
    }
}

/// The default reading of a dialect (the first of [`variants`]).
pub fn tokenize_default(sql: &str, d: Dialect) -> Lexed<'_> {
    tokenize(sql, d, variants(d)[0])
}

fn is_word_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c >= 0x80
}

/// Tokenize `sql` as dialect `d` reads it with the settings `o`.
pub fn tokenize(sql: &str, d: Dialect, o: LexOptions) -> Lexed<'_> {
    let b = sql.as_bytes();
    let mysql = matches!(d, Dialect::MySql | Dialect::ClickHouse);
    let nested_comments = matches!(d, Dialect::Postgres | Dialect::SqlServer);
    let mut out = Lexed::default();
    let mut depth = 0;
    // Inside MySQL's `/*! … */`: the closing `*/` is skipped, the content is code.
    let mut in_exec_comment = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let next = b.get(i + 1).copied();
        match c {
            c if c.is_ascii_whitespace() => i += 1,
            b'*' if in_exec_comment && next == Some(b'/') => {
                in_exec_comment = false;
                i += 2;
            }
            // MySQL: `--` starts a comment only when whitespace or the end follows.
            b'-' if next == Some(b'-')
                && (d != Dialect::MySql
                    || b.get(i + 2)
                        .is_none_or(|c| c.is_ascii_whitespace() || *c < 0x20)) =>
            {
                i = line_end(b, i);
            }
            b'#' if d == Dialect::MySql => i = line_end(b, i),
            b'#' if d == Dialect::ClickHouse && matches!(next, Some(b' ' | b'!')) => {
                i = line_end(b, i)
            }
            b'/' if next == Some(b'*') => {
                let exec = mysql && b.get(i + 2) == Some(&b'!')
                    || d == Dialect::MySql
                        && b.get(i + 2) == Some(&b'M')
                        && b.get(i + 3) == Some(&b'!');
                if exec && !in_exec_comment {
                    // `/*!50001 code */` and MariaDB's `/*M!100100 code */`.
                    i += if b[i + 2] == b'M' { 4 } else { 3 };
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                    in_exec_comment = true;
                    continue;
                }
                match block_comment_end(b, i, nested_comments) {
                    Some(end) => i = end,
                    None => {
                        out.unterminated = true;
                        i = b.len();
                    }
                }
            }
            b'\'' => {
                let end = quoted(b, i, b'\'', o.backslash_single);
                push(&mut out, Tok::Str, i, end.unwrap_or(b.len()), depth);
                out.unterminated |= end.is_none();
                i = end.unwrap_or(b.len());
            }
            b'"' => {
                let tok = if mysql { Tok::Str } else { Tok::Ident };
                let end = quoted(b, i, b'"', mysql && o.backslash_double);
                push(&mut out, tok, i, end.unwrap_or(b.len()), depth);
                out.unterminated |= end.is_none();
                i = end.unwrap_or(b.len());
            }
            b'`' if mysql || d == Dialect::Sqlite => {
                let end = quoted(b, i, b'`', d == Dialect::ClickHouse && o.backslash_double);
                push(&mut out, Tok::Ident, i, end.unwrap_or(b.len()), depth);
                out.unterminated |= end.is_none();
                i = end.unwrap_or(b.len());
            }
            b'[' if matches!(d, Dialect::SqlServer | Dialect::Sqlite) => {
                // `]]` is an escaped `]` in SQL Server.
                let end = quoted_with(b, i + 1, b']', false, d == Dialect::SqlServer);
                push(&mut out, Tok::Ident, i, end.unwrap_or(b.len()), depth);
                out.unterminated |= end.is_none();
                i = end.unwrap_or(b.len());
            }
            b'$' if d == Dialect::Postgres && dollar_tag(b, i).is_some() => {
                let tag_end = dollar_tag(b, i).expect("checked");
                let delim = &sql[i..tag_end];
                let end = sql[tag_end..]
                    .find(delim)
                    .map(|p| tag_end + p + delim.len());
                push(&mut out, Tok::Str, i, end.unwrap_or(b.len()), depth);
                out.unterminated |= end.is_none();
                i = end.unwrap_or(b.len());
            }
            b'@' if matches!(d, Dialect::MySql | Dialect::SqlServer) => {
                let start = i;
                i += 1;
                if b.get(i) == Some(&b'@') {
                    i += 1;
                }
                while i < b.len() && (is_word_byte(b[i]) || b[i] == b'#' || b[i] == b'@') {
                    i += 1;
                }
                push(&mut out, Tok::Word(&sql[start..i]), start, i, depth);
            }
            b'#' if d == Dialect::SqlServer => {
                let start = i;
                while i < b.len() && (is_word_byte(b[i]) || b[i] == b'#') {
                    i += 1;
                }
                push(&mut out, Tok::Word(&sql[start..i]), start, i, depth);
            }
            c if c.is_ascii_digit() => {
                let start = i;
                while i < b.len() && (is_word_byte(b[i]) || b[i] == b'.') {
                    i += 1;
                }
                push(&mut out, Tok::Number, start, i, depth);
            }
            c if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 => {
                let start = i;
                while i < b.len()
                    && (is_word_byte(b[i]) || (b[i] == b'#' && d == Dialect::SqlServer))
                {
                    i += 1;
                }
                let word = &sql[start..i];
                // String prefixes written right before the quote.
                if b.get(i) == Some(&b'\'') {
                    if d == Dialect::Postgres && word.eq_ignore_ascii_case("e") {
                        let end = quoted(b, i, b'\'', true);
                        push(&mut out, Tok::Str, start, end.unwrap_or(b.len()), depth);
                        out.unterminated |= end.is_none();
                        i = end.unwrap_or(b.len());
                        continue;
                    }
                    // `N'…'`, `X'…'`, `B'…'`.
                    if ["n", "x", "b"].iter().any(|p| word.eq_ignore_ascii_case(p)) {
                        let end = quoted(b, i, b'\'', o.backslash_single);
                        push(&mut out, Tok::Str, start, end.unwrap_or(b.len()), depth);
                        out.unterminated |= end.is_none();
                        i = end.unwrap_or(b.len());
                        continue;
                    }
                    if d == Dialect::Oracle
                        && (word.eq_ignore_ascii_case("q") || word.eq_ignore_ascii_case("nq"))
                    {
                        let end = oracle_q_quote(b, i);
                        push(&mut out, Tok::Str, start, end.unwrap_or(b.len()), depth);
                        out.unterminated |= end.is_none();
                        i = end.unwrap_or(b.len());
                        continue;
                    }
                }
                push(&mut out, Tok::Word(word), start, i, depth);
            }
            b';' => {
                push(&mut out, Tok::Semi, i, i + 1, depth);
                i += 1;
            }
            b'(' => {
                push(&mut out, Tok::Punct('('), i, i + 1, depth);
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                push(&mut out, Tok::Punct(')'), i, i + 1, depth);
                i += 1;
            }
            c if c.is_ascii() => {
                push(&mut out, Tok::Punct(c as char), i, i + 1, depth);
                i += 1;
            }
            _ => i += 1,
        }
    }
    out.unterminated |= in_exec_comment;
    out
}

fn push<'a>(out: &mut Lexed<'a>, tok: Tok<'a>, start: usize, end: usize, depth: i32) {
    out.tokens.push(Token {
        tok,
        start,
        end,
        depth,
    });
}

fn line_end(b: &[u8], i: usize) -> usize {
    b[i..]
        .iter()
        .position(|&c| c == b'\n')
        .map(|p| i + p + 1)
        .unwrap_or(b.len())
}

/// The end of the block comment at `i` (`/*`), nested ones counted when `nested`.
fn block_comment_end(b: &[u8], i: usize, nested: bool) -> Option<usize> {
    let mut level = 0;
    let mut j = i;
    while j + 1 < b.len() {
        if b[j] == b'/' && b[j + 1] == b'*' && (nested || level == 0) {
            level += 1;
            j += 2;
        } else if b[j] == b'*' && b[j + 1] == b'/' {
            level -= 1;
            j += 2;
            if level == 0 {
                return Some(j);
            }
        } else {
            j += 1;
        }
    }
    None
}

/// The end (after the closing quote) of the quoted text at `i`; a doubled quote is an
/// escaped one, and so is `\` + any character when `backslash`.
fn quoted(b: &[u8], i: usize, q: u8, backslash: bool) -> Option<usize> {
    quoted_with(b, i + 1, q, backslash, true)
}

fn quoted_with(b: &[u8], from: usize, q: u8, backslash: bool, doubled: bool) -> Option<usize> {
    let mut j = from;
    while j < b.len() {
        if backslash && b[j] == b'\\' {
            j += 2;
            continue;
        }
        if b[j] == q {
            if doubled && b.get(j + 1) == Some(&q) {
                j += 2;
                continue;
            }
            return Some(j + 1);
        }
        j += 1;
    }
    None
}

/// The end of a PostgreSQL dollar-quote opener at `i` (`$$`, `$tag$`); `$1` is a parameter.
fn dollar_tag(b: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    if b.get(j).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] >= 0x80) {
        j += 1;
    }
    (b.get(j) == Some(&b'$')).then_some(j + 1)
}

/// Oracle's alternative quoting: `q'[…]'`, `q'{…}'`, `q'<…>'`, `q'(…)'` or `q'!…!'` with
/// any other delimiter; `i` is at the quote.
fn oracle_q_quote(b: &[u8], i: usize) -> Option<usize> {
    let open = *b.get(i + 1)?;
    let close = match open {
        b'[' => b']',
        b'{' => b'}',
        b'<' => b'>',
        b'(' => b')',
        c if c.is_ascii_whitespace() => return None,
        c => c,
    };
    let mut j = i + 2;
    while j + 1 < b.len() {
        if b[j] == close && b[j + 1] == b'\'' {
            return Some(j + 2);
        }
        j += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(sql: &str, d: Dialect) -> Vec<String> {
        tokenize_default(sql, d)
            .tokens
            .iter()
            .map(|t| match t.tok {
                Tok::Word(w) => w.to_string(),
                Tok::Ident => "<id>".into(),
                Tok::Str => "<str>".into(),
                Tok::Number => "<num>".into(),
                Tok::Semi => ";".into(),
                Tok::Punct(c) => c.to_string(),
            })
            .collect()
    }

    #[test]
    fn dialect_strings_and_identifiers() {
        assert_eq!(
            kinds(r"select E'\'' ; drop", Dialect::Postgres),
            ["select", "<str>", ";", "drop"]
        );
        assert_eq!(
            kinds("select $a$ ' $b$ $a$, $1", Dialect::Postgres),
            ["select", "<str>", ",", "$", "<num>"]
        );
        assert_eq!(
            kinds("select N'it''s', [a]]b] x", Dialect::SqlServer),
            ["select", "<str>", ",", "<id>", "x"]
        );
        assert_eq!(
            kinds("select q'[it's]' from dual", Dialect::Oracle),
            ["select", "<str>", "from", "dual"]
        );
        assert_eq!(
            kinds(r#"select "a\"b", `c` x"#, Dialect::MySql),
            ["select", "<str>", ",", "<id>", "x"]
        );
        assert_eq!(
            kinds("select @x, @@global.y, #t", Dialect::SqlServer),
            ["select", "@x", ",", "@@global", ".", "y", ",", "#t"]
        );
        assert_eq!(
            kinds("select 'Kundenü'", Dialect::MySql),
            ["select", "<str>"]
        );
    }

    #[test]
    fn dialect_comments() {
        assert_eq!(kinds("/* a /* b */ c */ x", Dialect::Postgres), ["x"]);
        assert_eq!(
            kinds("/* a /* b */ c */ x", Dialect::MySql),
            ["c", "*", "/", "x"]
        );
        assert_eq!(
            kinds("1--1\n", Dialect::MySql),
            ["<num>", "-", "-", "<num>"]
        );
        assert_eq!(kinds("1 -- x\n2", Dialect::MySql), ["<num>", "<num>"]);
        assert_eq!(kinds("1 # x\n2", Dialect::MySql), ["<num>", "<num>"]);
        assert_eq!(
            kinds("select /*!50000 drop */ 1", Dialect::MySql),
            ["select", "drop", "<num>"]
        );
        assert!(tokenize_default("select 'x", Dialect::MySql).unterminated);
        assert!(tokenize_default("select /* x", Dialect::MySql).unterminated);
    }

    #[test]
    fn backslash_readings() {
        let semis = |o: LexOptions| {
            tokenize(r"select 'a\'; drop", Dialect::MySql, o)
                .tokens
                .iter()
                .filter(|t| t.tok == Tok::Semi)
                .count()
        };
        let v = variants(Dialect::MySql);
        assert_eq!(semis(v[0]), 0);
        assert_eq!(semis(v[2]), 1);
    }
}
