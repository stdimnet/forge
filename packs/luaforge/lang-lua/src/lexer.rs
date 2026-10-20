//! Lua 5.1–5.4 / LuaJIT lexer. Error tolerant: unterminated strings and comments end at
//! the end of the line or file, unknown characters become one-character operators.

/// Kind of a [`Token`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tok {
    Name,
    Keyword,
    Number,
    /// A short (`"…"`, `'…'`) or long (`[[…]]`) string literal, delimiters included.
    Str,
    /// Operators and punctuation.
    Op,
}

/// A token as a byte range of the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: Tok,
    pub start: usize,
    pub end: usize,
}

impl Token {
    pub fn text<'a>(&self, src: &'a str) -> &'a str {
        &src[self.start..self.end]
    }
}

/// A comment. `doc` comments start with exactly three dashes (`---`), as EmmyLua and LuaLS
/// annotations do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub start: usize,
    pub end: usize,
    pub doc: bool,
    /// `--[[ … ]]`.
    pub long: bool,
}

impl Comment {
    /// The text after the dashes (or between the long brackets).
    pub fn body<'a>(&self, src: &'a str) -> &'a str {
        let raw = &src[self.start..self.end];
        if self.long {
            let after = &raw[2..];
            let level = long_bracket_level(after.as_bytes(), 0).unwrap_or(0);
            let inner = &after[(level + 2).min(after.len())..];
            let close = format!("]{}]", "=".repeat(level));
            return inner.strip_suffix(close.as_str()).unwrap_or(inner);
        }
        if self.doc {
            &raw[3..]
        } else {
            &raw[2..]
        }
    }
}

pub const KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

const OPS3: &[&str] = &["..."];
const OPS2: &[&str] = &["..", "==", "~=", "<=", ">=", "<<", ">>", "//", "::"];

/// `[[`, `[=[`, …: the number of `=` when `bytes[at..]` opens a long bracket.
fn long_bracket_level(bytes: &[u8], at: usize) -> Option<usize> {
    if bytes.get(at) != Some(&b'[') {
        return None;
    }
    let mut i = at + 1;
    while bytes.get(i) == Some(&b'=') {
        i += 1;
    }
    (bytes.get(i) == Some(&b'[')).then_some(i - at - 1)
}

/// End (exclusive) of a long bracket body that started with `level` `=` signs, searching
/// from `from`; the end of the input when it is not closed.
fn long_bracket_end(bytes: &[u8], from: usize, level: usize) -> usize {
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b']' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] == b'=' {
                j += 1;
            }
            if j - i - 1 == level && bytes.get(j) == Some(&b']') {
                return j + 1;
            }
        }
        i += 1;
    }
    bytes.len()
}

/// Tokens and comments of `src`.
pub fn lex(src: &str) -> (Vec<Token>, Vec<Comment>) {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut comments = Vec::new();
    let mut i = 0;
    // Shebang line.
    if bytes.starts_with(b"#") {
        i = src.find('\n').unwrap_or(src.len());
    }
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        // Comments.
        if c == b'-' && bytes.get(i + 1) == Some(&b'-') {
            if let Some(level) = long_bracket_level(bytes, i + 2) {
                let end = long_bracket_end(bytes, i + 4 + level, level);
                comments.push(Comment {
                    start,
                    end,
                    doc: false,
                    long: true,
                });
                i = end;
                continue;
            }
            let end = src[i..].find('\n').map(|n| i + n).unwrap_or(src.len());
            let end = if end > start && bytes[end - 1] == b'\r' {
                end - 1
            } else {
                end
            };
            let doc = bytes.get(i + 2) == Some(&b'-') && bytes.get(i + 3) != Some(&b'-');
            comments.push(Comment {
                start,
                end,
                doc,
                long: false,
            });
            i = end;
            continue;
        }
        // Names and keywords.
        if c == b'_' || c.is_ascii_alphabetic() || c >= 0x80 {
            while i < bytes.len()
                && (bytes[i] == b'_' || bytes[i].is_ascii_alphanumeric() || bytes[i] >= 0x80)
            {
                i += 1;
            }
            let kind = if KEYWORDS.contains(&&src[start..i]) {
                Tok::Keyword
            } else {
                Tok::Name
            };
            tokens.push(Token {
                kind,
                start,
                end: i,
            });
            continue;
        }
        // Numbers (decimal, hex, exponents, LuaJIT suffixes).
        if c.is_ascii_digit() || (c == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let hex = c == b'0' && matches!(bytes.get(i + 1), Some(b'x' | b'X'));
            if hex {
                i += 2;
            }
            while i < bytes.len() {
                let b = bytes[i];
                let exp = if hex {
                    matches!(b, b'p' | b'P')
                } else {
                    matches!(b, b'e' | b'E')
                };
                if exp && matches!(bytes.get(i + 1), Some(b'+' | b'-')) {
                    i += 2;
                } else if b.is_ascii_alphanumeric() || b == b'.' {
                    i += 1;
                } else {
                    break;
                }
            }
            tokens.push(Token {
                kind: Tok::Number,
                start,
                end: i,
            });
            continue;
        }
        // Strings.
        if c == b'"' || c == b'\'' {
            i += 1;
            while i < bytes.len() && bytes[i] != c && bytes[i] != b'\n' {
                if bytes[i] == b'\\' {
                    // `\z` skips following whitespace including newlines.
                    if bytes.get(i + 1) == Some(&b'z') {
                        i += 2;
                        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                            i += 1;
                        }
                        continue;
                    }
                    i += 1;
                }
                i += 1;
            }
            if i < bytes.len() && bytes[i] == c {
                i += 1;
            }
            tokens.push(Token {
                kind: Tok::Str,
                start,
                end: i.min(bytes.len()),
            });
            i = i.min(bytes.len());
            continue;
        }
        if let Some(level) = long_bracket_level(bytes, i) {
            let end = long_bracket_end(bytes, i + 2 + level, level);
            tokens.push(Token {
                kind: Tok::Str,
                start,
                end,
            });
            i = end;
            continue;
        }
        // Operators.
        let rest = &src[i..];
        let len = if OPS3.iter().any(|o| rest.starts_with(o)) {
            3
        } else if OPS2.iter().any(|o| rest.starts_with(o)) {
            2
        } else {
            rest.chars().next().map(char::len_utf8).unwrap_or(1)
        };
        i += len;
        tokens.push(Token {
            kind: Tok::Op,
            start,
            end: i,
        });
    }
    (tokens, comments)
}

/// The value of a string literal token (escape sequences decoded where simple).
pub fn string_value(src: &str, tok: &Token) -> String {
    let raw = tok.text(src);
    let bytes = raw.as_bytes();
    if let Some(level) = long_bracket_level(bytes, 0) {
        let body = &raw[level + 2..];
        let body = body.strip_prefix('\n').unwrap_or(body);
        let close = level + 2;
        return body[..body.len().saturating_sub(close).min(body.len())].to_string();
    }
    let quote = raw.chars().next().unwrap_or('"');
    let inner = raw[1..].strip_suffix(quote).unwrap_or(&raw[1..]);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<(Tok, &str)> {
        lex(src).0.iter().map(|t| (t.kind, t.text(src))).collect()
    }

    #[test]
    fn names_keywords_numbers_ops() {
        assert_eq!(
            kinds("local x = 0x1F + 3.5e-2 .. y ~= ..."),
            vec![
                (Tok::Keyword, "local"),
                (Tok::Name, "x"),
                (Tok::Op, "="),
                (Tok::Number, "0x1F"),
                (Tok::Op, "+"),
                (Tok::Number, "3.5e-2"),
                (Tok::Op, ".."),
                (Tok::Name, "y"),
                (Tok::Op, "~="),
                (Tok::Op, "..."),
            ]
        );
    }

    #[test]
    fn strings_and_long_strings() {
        let src = r#"a = "x\"y" .. 'z' .. [==[ long ]] still ]==] b"#;
        let toks = kinds(src);
        assert_eq!(toks[2], (Tok::Str, r#""x\"y""#));
        assert_eq!(toks[4], (Tok::Str, "'z'"));
        assert_eq!(toks[6], (Tok::Str, "[==[ long ]] still ]==]"));
        assert_eq!(toks[7], (Tok::Name, "b"));
        let (tokens, _) = lex(src);
        assert_eq!(string_value(src, &tokens[2]), "x\"y");
        assert_eq!(string_value(src, &tokens[6]), " long ]] still ");
    }

    #[test]
    fn comments_doc_and_long() {
        let src = "#!/usr/bin/lua\n--- doc\n-- plain\n---- rule\n--[[ long\n comment ]] x";
        let (tokens, comments) = lex(src);
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            comments.iter().map(|c| (c.doc, c.long)).collect::<Vec<_>>(),
            vec![(true, false), (false, false), (false, false), (false, true)]
        );
        assert_eq!(comments[0].body(src), " doc");
        assert_eq!(comments[3].body(src), " long\n comment ");
    }

    #[test]
    fn unterminated_string_stops_at_line_end() {
        let src = "x = \"open\ny = 1";
        let toks = kinds(src);
        assert_eq!(toks[2], (Tok::Str, "\"open"));
        assert_eq!(toks[3], (Tok::Name, "y"));
    }
}
