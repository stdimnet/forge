//! A small JSON reader that keeps byte offsets, so `package.json` scripts can get gutter
//! icons on their lines. Accepts what `tsconfig.json` allows too: comments and trailing
//! commas.

/// A parsed value with the byte range it covers.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub value: Value,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Object(Vec<Member>),
    Array(Vec<Node>),
    String(String),
    /// Numbers, booleans and `null`, as written.
    Literal(String),
}

/// An object member; `key_start` is the offset of the key's opening quote.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub key: String,
    pub key_start: usize,
    pub value: Node,
}

impl Node {
    /// The member value of an object.
    pub fn get(&self, key: &str) -> Option<&Node> {
        self.member(key).map(|m| &m.value)
    }

    pub fn member(&self, key: &str) -> Option<&Member> {
        match &self.value {
            Value::Object(members) => members.iter().find(|m| m.key == key),
            _ => None,
        }
    }

    pub fn members(&self) -> &[Member] {
        match &self.value {
            Value::Object(members) => members,
            _ => &[],
        }
    }

    pub fn items(&self) -> &[Node] {
        match &self.value {
            Value::Array(items) => items,
            _ => &[],
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match &self.value {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Follow a path of object keys.
    pub fn pointer(&self, keys: &[&str]) -> Option<&Node> {
        keys.iter().try_fold(self, |node, key| node.get(key))
    }
}

/// Parse a whole document; `None` when it is not valid (lenient) JSON.
pub fn parse(text: &str) -> Option<Node> {
    let mut p = Parser {
        src: text.as_bytes(),
        text,
        pos: 0,
    };
    let node = p.value()?;
    p.skip_ws();
    (p.pos == p.src.len()).then_some(node)
}

struct Parser<'a> {
    src: &'a [u8],
    text: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        loop {
            while self.pos < self.src.len() && self.src[self.pos].is_ascii_whitespace() {
                self.pos += 1;
            }
            if self.src[self.pos..].starts_with(b"//") {
                while self.pos < self.src.len() && self.src[self.pos] != b'\n' {
                    self.pos += 1;
                }
            } else if self.src[self.pos..].starts_with(b"/*") {
                match self.text[self.pos + 2..].find("*/") {
                    Some(i) => self.pos += i + 4,
                    None => self.pos = self.src.len(),
                }
            } else {
                // A byte-order mark at the start of the file.
                if self.pos == 0 && self.text.starts_with('\u{feff}') {
                    self.pos = 3;
                    continue;
                }
                return;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn value(&mut self) -> Option<Node> {
        self.skip_ws();
        let start = self.pos;
        let value = match self.peek()? {
            b'{' => self.object()?,
            b'[' => self.array()?,
            b'"' => Value::String(self.string()?),
            _ => {
                while self
                    .peek()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'+' | b'.'))
                {
                    self.pos += 1;
                }
                if self.pos == start {
                    return None;
                }
                Value::Literal(self.text[start..self.pos].to_string())
            }
        };
        Some(Node {
            value,
            start,
            end: self.pos,
        })
    }

    fn object(&mut self) -> Option<Value> {
        self.pos += 1;
        let mut members = Vec::new();
        loop {
            self.skip_ws();
            match self.peek()? {
                b'}' => {
                    self.pos += 1;
                    return Some(Value::Object(members));
                }
                b'"' => {
                    let key_start = self.pos;
                    let key = self.string()?;
                    self.skip_ws();
                    if self.peek()? != b':' {
                        return None;
                    }
                    self.pos += 1;
                    let value = self.value()?;
                    members.push(Member {
                        key,
                        key_start,
                        value,
                    });
                    self.skip_ws();
                    match self.peek()? {
                        b',' => self.pos += 1,
                        b'}' => {}
                        _ => return None,
                    }
                }
                _ => return None,
            }
        }
    }

    fn array(&mut self) -> Option<Value> {
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            if self.peek()? == b']' {
                self.pos += 1;
                return Some(Value::Array(items));
            }
            items.push(self.value()?);
            self.skip_ws();
            match self.peek()? {
                b',' => self.pos += 1,
                b']' => {}
                _ => return None,
            }
        }
    }

    fn string(&mut self) -> Option<String> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            let rest = &self.text[self.pos..];
            let i = rest.find(['"', '\\'])?;
            out.push_str(&rest[..i]);
            self.pos += i;
            if self.src[self.pos] == b'"' {
                self.pos += 1;
                return Some(out);
            }
            // Escape sequence.
            let esc = *self.src.get(self.pos + 1)?;
            self.pos += 2;
            match esc {
                b'n' => out.push('\n'),
                b't' => out.push('\t'),
                b'r' => out.push('\r'),
                b'b' => out.push('\u{8}'),
                b'f' => out.push('\u{c}'),
                b'u' => {
                    let hex = self.text.get(self.pos..self.pos + 4)?;
                    self.pos += 4;
                    let code = u32::from_str_radix(hex, 16).ok()?;
                    out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                }
                other => out.push(other as char),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_offsets() {
        let src = r#"{ "scripts": { "dev": "vite", "build": "tsc && vite build" } }"#;
        let doc = parse(src).unwrap();
        let scripts = doc.get("scripts").unwrap();
        let build = scripts.member("build").unwrap();
        assert_eq!(&src[build.key_start..build.key_start + 7], "\"build\"");
        assert_eq!(build.value.as_str(), Some("tsc && vite build"));
    }

    #[test]
    fn comments_trailing_commas_and_escapes() {
        let src = "// tsconfig\n{\n  /* paths */ \"a\": [1, true, null,],\n  \"b\": \"x\\\"y\\u00e9\",\n}";
        let doc = parse(src).unwrap();
        assert_eq!(doc.get("a").unwrap().items().len(), 3);
        assert_eq!(doc.get("b").unwrap().as_str(), Some("x\"yé"));
    }

    #[test]
    fn rejects_broken_documents() {
        assert!(parse("{ \"a\": }").is_none());
        assert!(parse("{ \"a\": 1").is_none());
        assert!(parse("").is_none());
    }
}
