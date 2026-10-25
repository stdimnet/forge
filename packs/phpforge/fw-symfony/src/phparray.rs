//! A small reader for PHP array literals in method bodies (`getSubscribedEvents()`,
//! `$resolver->setDefaults([...])`): strings, numbers, other expressions as raw text and
//! nested arrays, with byte offsets.

/// A value of an array literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhpValue {
    Str(String),
    Int(i64),
    /// Anything else, as written (`KernelEvents::REQUEST`, `Order::class`, `$x`, `fn() => 1`).
    Expr(String),
    Array(Vec<ArrayItem>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayItem {
    pub key: Option<PhpValue>,
    pub value: PhpValue,
    /// Byte offset of the item (its key, else its value).
    pub offset: usize,
}

impl PhpValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Source-like text: strings without quotes, expressions as written.
    pub fn text(&self) -> String {
        match self {
            Self::Str(s) | Self::Expr(s) => s.clone(),
            Self::Int(i) => i.to_string(),
            Self::Array(_) => "[…]".into(),
        }
    }

    pub fn items(&self) -> &[ArrayItem] {
        match self {
            Self::Array(a) => a,
            _ => &[],
        }
    }
}

struct Reader<'a> {
    text: &'a str,
    bytes: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn skip_ws(&mut self) {
        while self.i < self.bytes.len() {
            let c = self.bytes[self.i];
            if c.is_ascii_whitespace() {
                self.i += 1;
            } else if self.text[self.i..].starts_with("//")
                || (c == b'#' && self.bytes.get(self.i + 1) != Some(&b'['))
            {
                self.i = self.text[self.i..]
                    .find('\n')
                    .map(|p| self.i + p)
                    .unwrap_or(self.bytes.len());
            } else if self.text[self.i..].starts_with("/*") {
                self.i = self.text[self.i..]
                    .find("*/")
                    .map(|p| self.i + p + 2)
                    .unwrap_or(self.bytes.len());
            } else {
                break;
            }
        }
    }

    fn string(&mut self) -> Option<String> {
        let q = self.bytes[self.i];
        let mut j = self.i + 1;
        let mut s = String::new();
        while j < self.bytes.len() {
            let c = self.bytes[j];
            if c == b'\\' && j + 1 < self.bytes.len() {
                let n = self.bytes[j + 1];
                if n == q || n == b'\\' {
                    s.push(n as char);
                    j += 2;
                    continue;
                }
            }
            if c == q {
                self.i = j + 1;
                return Some(s);
            }
            let ch = self.text[j..].chars().next()?;
            s.push(ch);
            j += ch.len_utf8();
        }
        None
    }

    /// Raw expression up to `,`, `=>` or the closing bracket at depth 0.
    fn expr(&mut self, close: u8) -> String {
        let start = self.i;
        let mut depth = 0i32;
        while self.i < self.bytes.len() {
            let c = self.bytes[self.i];
            match c {
                b'\'' | b'"' => {
                    if self.string().is_none() {
                        self.i = self.bytes.len();
                    }
                    continue;
                }
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' if depth > 0 => depth -= 1,
                b')' | b']' | b'}' => break,
                b',' if depth == 0 => break,
                b';' if depth == 0 => break,
                b'=' if depth == 0 && self.bytes.get(self.i + 1) == Some(&b'>') => break,
                _ => {}
            }
            let _ = close;
            self.i += 1;
        }
        self.text[start..self.i].trim().to_string()
    }

    fn value(&mut self, close: u8) -> Option<PhpValue> {
        self.skip_ws();
        let c = *self.bytes.get(self.i)?;
        if c == b'\'' || c == b'"' {
            let start = self.i;
            let s = self.string()?;
            // `'a' . 'b'` and similar: keep as an expression.
            let save = self.i;
            self.skip_ws();
            if matches!(self.bytes.get(self.i), Some(b'.') | Some(b'?')) {
                self.i = start;
                return Some(PhpValue::Expr(self.expr(close)));
            }
            self.i = save;
            return Some(PhpValue::Str(s));
        }
        if c == b'[' {
            self.i += 1;
            return Some(PhpValue::Array(self.items(b']')));
        }
        if self.text[self.i..].starts_with("array(") {
            self.i += 6;
            return Some(PhpValue::Array(self.items(b')')));
        }
        let e = self.expr(close);
        if let Ok(n) = e.replace('_', "").parse::<i64>() {
            return Some(PhpValue::Int(n));
        }
        Some(PhpValue::Expr(e))
    }

    fn items(&mut self, close: u8) -> Vec<ArrayItem> {
        let mut out = Vec::new();
        loop {
            self.skip_ws();
            let Some(&c) = self.bytes.get(self.i) else {
                break;
            };
            if c == close {
                self.i += 1;
                break;
            }
            if c == b',' {
                self.i += 1;
                continue;
            }
            let offset = self.i;
            let before = self.i;
            let Some(first) = self.value(close) else {
                break;
            };
            self.skip_ws();
            if self.text[self.i..].starts_with("=>") {
                self.i += 2;
                let Some(value) = self.value(close) else {
                    break;
                };
                out.push(ArrayItem {
                    key: Some(first),
                    value,
                    offset,
                });
            } else {
                out.push(ArrayItem {
                    key: None,
                    value: first,
                    offset,
                });
            }
            if self.i == before {
                // No progress (unexpected token): stop.
                break;
            }
        }
        out
    }
}

/// Read the array literal starting at `offset` (at `[` or `array(`).
pub fn parse_array_at(text: &str, offset: usize) -> Option<PhpValue> {
    let mut r = Reader {
        text,
        bytes: text.as_bytes(),
        i: offset,
    };
    r.skip_ws();
    match r.bytes.get(r.i)? {
        b'[' => {
            r.i += 1;
            Some(PhpValue::Array(r.items(b']')))
        }
        b'a' if text[r.i..].starts_with("array(") => {
            r.i += 6;
            Some(PhpValue::Array(r.items(b')')))
        }
        _ => None,
    }
}

/// The array after the first `needle` in `text[from..to]` (`return [`, `setDefaults(`).
pub fn array_after(text: &str, from: usize, to: usize, needle: &str) -> Option<(usize, PhpValue)> {
    let to = to.min(text.len());
    let at = text.get(from..to)?.find(needle)? + from + needle.len();
    let rest = &text[at..to];
    let start = at + (rest.len() - rest.trim_start().len());
    parse_array_at(text, start).map(|v| (start, v))
}

/// The arguments of every `needle(` call in `text[from..to]` as values
/// (`setDefined(['a', 'b'])`, `setDefined('a')`).
pub fn call_args(text: &str, from: usize, to: usize, needle: &str) -> Vec<Vec<PhpValue>> {
    let to = to.min(text.len());
    let mut out = Vec::new();
    let Some(slice) = text.get(from..to) else {
        return out;
    };
    let mut pos = 0;
    while let Some(p) = slice[pos..].find(needle) {
        let open = from + pos + p + needle.len();
        pos += p + needle.len();
        if text.as_bytes().get(open.wrapping_sub(1)) != Some(&b'(') {
            continue;
        }
        let mut r = Reader {
            text,
            bytes: text.as_bytes(),
            i: open,
        };
        let args: Vec<PhpValue> = r.items(b')').into_iter().map(|i| i.value).collect();
        out.push(args);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_arrays() {
        let src = "return [\n  KernelEvents::REQUEST => ['onRequest', 10],\n  'kernel.response' => [['a', -5], ['b']],\n  Order::class => 'onOrder', // note\n];";
        let v = parse_array_at(src, 7).unwrap();
        let items = v.items();
        assert_eq!(items.len(), 3);
        assert_eq!(
            items[0].key,
            Some(PhpValue::Expr("KernelEvents::REQUEST".into()))
        );
        assert_eq!(items[0].value.items()[1].value, PhpValue::Int(10));
        assert_eq!(
            items[1].value.items()[0].value.items()[1].value,
            PhpValue::Int(-5)
        );
        assert_eq!(items[2].value, PhpValue::Str("onOrder".into()));
        assert_eq!(&src[items[2].offset..items[2].offset + 5], "Order");
    }

    #[test]
    fn reads_call_arguments() {
        let src = "$r->setDefined(['a', 'b']); $r->setDefined('c'); $r->setRequired(['d']);";
        let calls = call_args(src, 0, src.len(), "setDefined(");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1][0], PhpValue::Str("c".into()));
        let (_, d) = array_after(src, 0, src.len(), "setRequired(").unwrap();
        assert_eq!(d.items()[0].value.as_str(), Some("d"));
    }
}
