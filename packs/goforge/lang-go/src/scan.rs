//! A small Go tokenizer: identifiers, string literals (decoded), punctuation and the line
//! of each token. Comments, numbers and rune literals are skipped. It is tolerant: an
//! unterminated literal ends at the end of the text. Enough for test discovery and
//! `go:generate` directives; gopls does the real analysis.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok<'a> {
    Ident(&'a str),
    /// An interpreted (`"…"`) or raw (`` `…` ``) string, decoded.
    Str(String),
    Punct(char),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token<'a> {
    pub tok: Tok<'a>,
    /// 0-based line of the token's start.
    pub line: u32,
    /// Byte offsets in the text.
    pub start: usize,
    pub end: usize,
}

impl<'a> Token<'a> {
    pub fn is_ident(&self, name: &str) -> bool {
        matches!(self.tok, Tok::Ident(i) if i == name)
    }

    pub fn is_punct(&self, c: char) -> bool {
        self.tok == Tok::Punct(c)
    }

    pub fn ident(&self) -> Option<&'a str> {
        match self.tok {
            Tok::Ident(i) => Some(i),
            _ => None,
        }
    }
}

fn decode_escape(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>, out: &mut String) {
    let Some((_, c)) = chars.next() else {
        return;
    };
    match c {
        'n' => out.push('\n'),
        't' => out.push('\t'),
        'r' => out.push('\r'),
        'a' => out.push('\u{7}'),
        'b' => out.push('\u{8}'),
        'f' => out.push('\u{c}'),
        'v' => out.push('\u{b}'),
        '\\' => out.push('\\'),
        '"' => out.push('"'),
        '\'' => out.push('\''),
        'x' | 'u' | 'U' => {
            let n = match c {
                'x' => 2,
                'u' => 4,
                _ => 8,
            };
            let mut hex = String::new();
            for _ in 0..n {
                match chars.peek() {
                    Some((_, h)) if h.is_ascii_hexdigit() => {
                        hex.push(*h);
                        chars.next();
                    }
                    _ => break,
                }
            }
            if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                out.push(ch);
            }
        }
        other => {
            out.push('\\');
            out.push(other);
        }
    }
}

/// The tokens of a Go source text.
pub fn tokens(text: &str) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    let mut line: u32 = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '\n' => line += 1,
            c if c.is_whitespace() => {}
            '/' if matches!(chars.peek(), Some((_, '/'))) => {
                while let Some((_, c)) = chars.peek() {
                    if *c == '\n' {
                        break;
                    }
                    chars.next();
                }
            }
            '/' if matches!(chars.peek(), Some((_, '*'))) => {
                chars.next();
                let mut prev = ' ';
                for (_, c) in chars.by_ref() {
                    if c == '\n' {
                        line += 1;
                    }
                    if prev == '*' && c == '/' {
                        break;
                    }
                    prev = c;
                }
            }
            '"' => {
                let start_line = line;
                let mut value = String::new();
                let mut end = text.len();
                while let Some((j, c)) = chars.next() {
                    match c {
                        '"' => {
                            end = j + 1;
                            break;
                        }
                        '\\' => decode_escape(&mut chars, &mut value),
                        '\n' => {
                            // Unterminated: stop at the line end.
                            line += 1;
                            end = j;
                            break;
                        }
                        c => value.push(c),
                    }
                }
                out.push(Token {
                    tok: Tok::Str(value),
                    line: start_line,
                    start: i,
                    end,
                });
            }
            '`' => {
                let start_line = line;
                let mut value = String::new();
                let mut end = text.len();
                for (j, c) in chars.by_ref() {
                    match c {
                        '`' => {
                            end = j + 1;
                            break;
                        }
                        '\n' => {
                            line += 1;
                            value.push(c);
                        }
                        '\r' => {}
                        c => value.push(c),
                    }
                }
                out.push(Token {
                    tok: Tok::Str(value),
                    line: start_line,
                    start: i,
                    end,
                });
            }
            '\'' => {
                while let Some((_, c)) = chars.next() {
                    match c {
                        '\'' | '\n' => {
                            if c == '\n' {
                                line += 1;
                            }
                            break;
                        }
                        '\\' => {
                            chars.next();
                        }
                        _ => {}
                    }
                }
            }
            c if c.is_alphabetic() || c == '_' => {
                let mut end = i + c.len_utf8();
                while let Some((j, c)) = chars.peek() {
                    if c.is_alphanumeric() || *c == '_' {
                        end = j + c.len_utf8();
                        chars.next();
                    } else {
                        break;
                    }
                }
                out.push(Token {
                    tok: Tok::Ident(&text[i..end]),
                    line,
                    start: i,
                    end,
                });
            }
            c if c.is_ascii_digit() => {
                while let Some((_, c)) = chars.peek() {
                    if c.is_alphanumeric() || *c == '_' || *c == '.' {
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            c => out.push(Token {
                tok: Tok::Punct(c),
                line,
                start: i,
                end: i + c.len_utf8(),
            }),
        }
    }
    out
}

/// Index of the token that closes the bracket opened at `open` (`(`, `[` or `{`), or the
/// last token when it is never closed.
pub fn matching(tokens: &[Token<'_>], open: usize) -> usize {
    let (o, c) = match tokens[open].tok {
        Tok::Punct('(') => ('(', ')'),
        Tok::Punct('[') => ('[', ']'),
        _ => ('{', '}'),
    };
    let mut depth = 0usize;
    for (i, t) in tokens.iter().enumerate().skip(open) {
        if t.is_punct(o) {
            depth += 1;
        } else if t.is_punct(c) {
            depth -= 1;
            if depth == 0 {
                return i;
            }
        }
    }
    tokens.len().saturating_sub(1)
}

/// The UTF-16 length of a string (editor columns).
pub fn utf16_len(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_comments_and_decodes_strings() {
        let src = "package x // c\n/* a\nb */ var s = \"a\\tb\" + `r\nq` + 'x'\nfunc F() {}";
        let t = tokens(src);
        let idents: Vec<_> = t.iter().filter_map(|t| t.ident()).collect();
        assert_eq!(idents, ["package", "x", "var", "s", "func", "F"]);
        let strs: Vec<_> = t
            .iter()
            .filter_map(|t| match &t.tok {
                Tok::Str(s) => Some((s.as_str(), t.line)),
                _ => None,
            })
            .collect();
        assert_eq!(strs, [("a\tb", 2), ("r\nq", 2)]);
        let f = t.iter().find(|t| t.is_ident("F")).unwrap();
        assert_eq!(f.line, 4);
        let open = t.iter().position(|t| t.is_punct('{')).unwrap();
        assert!(t[matching(&t, open)].is_punct('}'));
    }

    #[test]
    fn unicode_identifiers_and_escapes() {
        let t = tokens("π := \"\\u00e9\\x41\"");
        assert_eq!(t[0].ident(), Some("π"));
        assert_eq!(t.last().unwrap().tok, Tok::Str("éA".into()));
        assert_eq!(utf16_len("é𝄞"), 3);
    }
}
