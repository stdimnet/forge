//! A tolerant JSON parser that keeps byte ranges. It accepts JSON with comments (JSONC) and
//! the common JSON5 extensions (unquoted keys, single quotes, trailing commas, hex numbers,
//! `Infinity`/`NaN`) and keeps going over unfinished input, so completion works while the
//! user types: `{"a": 1, "na` yields a property `na` without colon.

use crate::tree::{Node, Property, Value};

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Open(u8),
    Close(u8),
    Colon,
    Comma,
    Str(String),
    Num(f64),
    Word(String),
    Other,
    Eof,
}

#[derive(Clone)]
struct Lexer<'a> {
    b: &'a [u8],
    src: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn skip_trivia(&mut self) {
        loop {
            while self.pos < self.b.len() && (self.b[self.pos] as char).is_ascii_whitespace() {
                self.pos += 1;
            }
            if self.src[self.pos..].starts_with('\u{feff}') {
                self.pos += 3;
                continue;
            }
            if self.b[self.pos..].starts_with(b"//") {
                while self.pos < self.b.len() && self.b[self.pos] != b'\n' {
                    self.pos += 1;
                }
                continue;
            }
            if self.b[self.pos..].starts_with(b"/*") {
                match self.src[self.pos + 2..].find("*/") {
                    Some(i) => self.pos += i + 4,
                    None => self.pos = self.b.len(),
                }
                continue;
            }
            break;
        }
    }

    /// The next token and its byte range.
    fn next(&mut self) -> (Tok, usize, usize) {
        self.skip_trivia();
        let start = self.pos;
        let Some(&c) = self.b.get(self.pos) else {
            return (Tok::Eof, start, start);
        };
        let tok = match c {
            b'{' | b'[' => {
                self.pos += 1;
                Tok::Open(c)
            }
            b'}' | b']' => {
                self.pos += 1;
                Tok::Close(c)
            }
            b':' => {
                self.pos += 1;
                Tok::Colon
            }
            b',' => {
                self.pos += 1;
                Tok::Comma
            }
            b'"' | b'\'' => Tok::Str(self.string(c)),
            b'-' | b'+' | b'.' | b'0'..=b'9' => self.number(),
            _ if c.is_ascii_alphabetic() || c == b'_' || c == b'$' || c >= 0x80 => {
                let s = self.pos;
                while self.pos < self.b.len() {
                    let ch = self.src[self.pos..].chars().next().unwrap_or(' ');
                    if ch.is_alphanumeric() || ch == '_' || ch == '$' || ch == '-' || ch == '.' {
                        self.pos += ch.len_utf8();
                    } else {
                        break;
                    }
                }
                if self.pos == s {
                    self.pos += self.src[s..].chars().next().map_or(1, char::len_utf8);
                    Tok::Other
                } else {
                    Tok::Word(self.src[s..self.pos].to_string())
                }
            }
            _ => {
                self.pos += self.src[self.pos..]
                    .chars()
                    .next()
                    .map_or(1, char::len_utf8);
                Tok::Other
            }
        };
        (tok, start, self.pos)
    }

    /// A quoted string; an unterminated one ends at the end of its line.
    fn string(&mut self, quote: u8) -> String {
        self.pos += 1;
        let mut out = String::new();
        while self.pos < self.b.len() {
            let c = self.b[self.pos];
            if c == quote {
                self.pos += 1;
                return out;
            }
            if c == b'\n' {
                return out;
            }
            if c == b'\\' && self.pos + 1 < self.b.len() {
                let e = self.b[self.pos + 1];
                self.pos += 2;
                match e {
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    b'r' => out.push('\r'),
                    b'b' => out.push('\u{8}'),
                    b'f' => out.push('\u{c}'),
                    b'0' => out.push('\0'),
                    b'\n' => {}
                    b'u' => {
                        let hex = |p: usize| {
                            self.src
                                .get(p..p + 4)
                                .and_then(|h| u32::from_str_radix(h, 16).ok())
                        };
                        if let Some(hi) = hex(self.pos) {
                            self.pos += 4;
                            if (0xD800..0xDC00).contains(&hi)
                                && self.src[self.pos..].starts_with("\\u")
                            {
                                if let Some(lo) = hex(self.pos + 2) {
                                    self.pos += 6;
                                    let cp = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                                    out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                                    continue;
                                }
                            }
                            out.push(char::from_u32(hi).unwrap_or('\u{fffd}'));
                        }
                    }
                    _ => {
                        let ch = self.src[self.pos - 1..].chars().next().unwrap_or('?');
                        self.pos += ch.len_utf8() - 1;
                        out.push(ch);
                    }
                }
                continue;
            }
            let ch = self.src[self.pos..].chars().next().unwrap_or('?');
            out.push(ch);
            self.pos += ch.len_utf8();
        }
        out
    }

    fn number(&mut self) -> Tok {
        let s = self.pos;
        let mut sign = 1.0;
        if matches!(self.b[self.pos], b'-' | b'+') {
            if self.b[self.pos] == b'-' {
                sign = -1.0;
            }
            self.pos += 1;
        }
        let rest = &self.src[self.pos..];
        for (word, v) in [("Infinity", f64::INFINITY), ("NaN", f64::NAN)] {
            if rest.starts_with(word) {
                self.pos += word.len();
                return Tok::Num(sign * v);
            }
        }
        if rest.starts_with("0x") || rest.starts_with("0X") {
            self.pos += 2;
            let h = self.pos;
            while self.pos < self.b.len() && self.b[self.pos].is_ascii_hexdigit() {
                self.pos += 1;
            }
            return match u64::from_str_radix(&self.src[h..self.pos], 16) {
                Ok(v) => Tok::Num(sign * v as f64),
                Err(_) => Tok::Other,
            };
        }
        while self.pos < self.b.len()
            && (self.b[self.pos].is_ascii_digit()
                || matches!(self.b[self.pos], b'.' | b'e' | b'E')
                || (matches!(self.b[self.pos], b'+' | b'-')
                    && matches!(self.b[self.pos - 1], b'e' | b'E')))
        {
            self.pos += 1;
        }
        match self.src[s..self.pos].trim_start_matches('+').parse::<f64>() {
            Ok(v) => Tok::Num(v),
            Err(_) => Tok::Other,
        }
    }
}

struct Parser<'a> {
    lex: Lexer<'a>,
    peeked: Option<(Tok, usize, usize)>,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn peek(&mut self) -> &(Tok, usize, usize) {
        if self.peeked.is_none() {
            self.peeked = Some(self.lex.next());
        }
        self.peeked.as_ref().expect("peeked")
    }

    fn bump(&mut self) -> (Tok, usize, usize) {
        self.peek();
        self.peeked.take().expect("peeked")
    }

    /// `"a": ⏎ "b": 1`: the string after the colon starts a new line and is followed by a
    /// colon, so it is the next key and `a` has no value yet.
    fn next_is_key_on_new_line(&mut self, colon: usize) -> bool {
        let (tok, start, _) = self.peek().clone();
        if !matches!(tok, Tok::Str(_) | Tok::Word(_)) || !self.lex.src[colon..start].contains('\n')
        {
            return false;
        }
        let mut ahead = self.lex.clone();
        matches!(ahead.next().0, Tok::Colon)
    }

    fn value(&mut self) -> Option<Node> {
        let (tok, start, end) = self.peek().clone();
        let node = match tok {
            Tok::Open(c) if self.depth < 512 => {
                self.bump();
                self.depth += 1;
                let n = if c == b'{' {
                    self.object(start)
                } else {
                    self.array(start)
                };
                self.depth -= 1;
                n
            }
            Tok::Str(s) => {
                self.bump();
                Node::new(Value::String(s), start, end)
            }
            Tok::Num(n) => {
                self.bump();
                Node::new(Value::Number(n), start, end)
            }
            Tok::Word(w) => {
                self.bump();
                let v = match w.as_str() {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    "null" => Value::Null,
                    _ => Value::Error,
                };
                Node::new(v, start, end)
            }
            Tok::Other => {
                self.bump();
                Node::new(Value::Error, start, end)
            }
            _ => return None,
        };
        Some(node)
    }

    fn object(&mut self, start: usize) -> Node {
        let mut props: Vec<Property> = Vec::new();
        let end;
        loop {
            let (tok, s, e) = self.peek().clone();
            match tok {
                Tok::Close(_) => {
                    self.bump();
                    end = e;
                    break;
                }
                Tok::Eof => {
                    end = self.lex.b.len();
                    break;
                }
                Tok::Comma => {
                    self.bump();
                }
                Tok::Str(key) | Tok::Word(key) => {
                    self.bump();
                    let mut prop = Property {
                        key,
                        key_start: s,
                        key_end: e,
                        colon: None,
                        value: None,
                    };
                    if let (Tok::Colon, cs, _) = self.peek().clone() {
                        self.bump();
                        prop.colon = Some(cs);
                        if !matches!(self.peek().0, Tok::Comma | Tok::Close(_) | Tok::Eof)
                            && !self.next_is_key_on_new_line(cs)
                        {
                            prop.value = self.value();
                        }
                    }
                    props.push(prop);
                }
                Tok::Num(n) => {
                    // JSON5 allows no numeric keys; treat it as a key anyway to keep going.
                    self.bump();
                    props.push(Property {
                        key: crate::tree::format_number(n),
                        key_start: s,
                        key_end: e,
                        colon: None,
                        value: None,
                    });
                }
                Tok::Open(_) => {
                    // A value without a key: parse and drop it.
                    self.value();
                }
                Tok::Colon | Tok::Other => {
                    self.bump();
                }
            }
        }
        Node::new(Value::Object(props), start, end)
    }

    fn array(&mut self, start: usize) -> Node {
        let mut items = Vec::new();
        let end;
        loop {
            let (tok, _, e) = self.peek().clone();
            match tok {
                Tok::Close(_) => {
                    self.bump();
                    end = e;
                    break;
                }
                Tok::Eof => {
                    end = self.lex.b.len();
                    break;
                }
                Tok::Comma | Tok::Colon => {
                    self.bump();
                }
                _ => match self.value() {
                    Some(v) => items.push(v),
                    None => {
                        self.bump();
                    }
                },
            }
        }
        Node::new(Value::Array(items), start, end)
    }
}

/// Parse a JSON document; `None` for an empty one.
pub fn parse(text: &str) -> Option<Node> {
    let mut p = Parser {
        lex: Lexer {
            b: text.as_bytes(),
            src: text,
            pos: 0,
        },
        peeked: None,
        depth: 0,
    };
    p.value()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_values_with_ranges() {
        let t = r#"{"a": [1, 2.5, true, null], "b": {"c": "xé"}}"#;
        let n = parse(t).unwrap();
        assert_eq!((n.start, n.end), (0, t.len()));
        let a = n.get("a").unwrap().as_array().unwrap();
        assert_eq!(a.len(), 4);
        assert_eq!(a[1].as_f64(), Some(2.5));
        assert_eq!(a[0].type_name(), "integer");
        let c = n.get("b").unwrap().get("c").unwrap();
        assert_eq!(c.as_str(), Some("xé"));
        assert_eq!(&t[c.start..c.end], r#""xé""#);
        let p = n.property("b").unwrap();
        assert_eq!(&t[p.key_start..p.key_end], "\"b\"");
    }

    #[test]
    fn accepts_comments_and_json5() {
        let t = "// c\n{ /* x */ name: 'a', 'n': 0x1F, list: [1,2,], inf: -Infinity, }";
        let n = parse(t).unwrap();
        assert_eq!(n.get("name").unwrap().as_str(), Some("a"));
        assert_eq!(n.get("n").unwrap().as_f64(), Some(31.0));
        assert_eq!(n.get("list").unwrap().as_array().unwrap().len(), 2);
        assert_eq!(n.get("inf").unwrap().as_f64(), Some(f64::NEG_INFINITY));
    }

    #[test]
    fn keeps_unfinished_input() {
        let n = parse("{\"a\": 1, \"na").unwrap();
        let props = n.as_object().unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(props[1].key, "na");
        assert!(props[1].colon.is_none());
        let n = parse("{\"a\": ,\n \"b\": tru }").unwrap();
        assert!(n.property("a").unwrap().value.is_none());
        assert_eq!(n.get("b").unwrap().value, Value::Error);
        let n = parse("{\"a\": \n\"b\": 1}").unwrap();
        assert!(n.property("a").unwrap().value.is_none());
        assert_eq!(n.get("b").unwrap().as_f64(), Some(1.0));
        assert!(parse("").is_none());
        assert!(parse("[[[").is_some());
    }
}
