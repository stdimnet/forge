//! The YAML Rails projects write by hand — locale files and `config/database.yml`: block
//! mappings and sequences, quoted and plain scalars, `|` / `>` block scalars, flow lists,
//! anchors, aliases and `<<:` merge keys. Keys keep their positions for navigation.

use std::collections::HashMap;

use crate::source::utf16_len;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
    Map(Vec<Entry>),
    Seq(Vec<Value>),
    Scalar(String),
    #[default]
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub key: String,
    pub value: Value,
    /// 0-based line and UTF-16 column of the key.
    pub line: u32,
    pub col: u32,
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entry(key).map(|e| &e.value)
    }

    pub fn entry(&self, key: &str) -> Option<&Entry> {
        match self {
            Value::Map(m) => m.iter().find(|e| e.key == key),
            _ => None,
        }
    }

    pub fn entries(&self) -> &[Entry] {
        match self {
            Value::Map(m) => m,
            _ => &[],
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Scalar(s) => Some(s),
            _ => None,
        }
    }

    pub fn is_map(&self) -> bool {
        matches!(self, Value::Map(_))
    }
}

struct Line<'a> {
    no: usize,
    indent: usize,
    /// The content after the indentation, comments removed.
    text: String,
    raw: &'a str,
}

fn strip_comment(s: &str) -> &str {
    let b = s.as_bytes();
    let mut quote = None;
    for (i, &c) in b.iter().enumerate() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                b'"' | b'\''
                    if i == 0 || matches!(b[i - 1], b' ' | b':' | b'[' | b'{' | b',' | b'-') =>
                {
                    quote = Some(c)
                }
                b'#' if i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t' => {
                    return s[..i].trim_end()
                }
                _ => {}
            },
        }
    }
    s.trim_end()
}

struct Parser<'a> {
    lines: Vec<Line<'a>>,
    all: Vec<&'a str>,
    i: usize,
    anchors: HashMap<String, Value>,
}

/// Parse a YAML document (the first one) into a value; unknown syntax is skipped.
pub fn parse(text: &str) -> Value {
    let all: Vec<&str> = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    let mut lines = Vec::new();
    for (no, raw) in all.iter().enumerate() {
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('%') {
            continue;
        }
        if trimmed.starts_with("---") {
            if lines.is_empty() {
                continue;
            }
            break;
        }
        if trimmed.starts_with("...") {
            break;
        }
        let text = strip_comment(trimmed).to_string();
        if text.is_empty() {
            continue;
        }
        lines.push(Line {
            no,
            indent: raw.len() - trimmed.len(),
            text,
            raw,
        });
    }
    let mut p = Parser {
        lines,
        all,
        i: 0,
        anchors: HashMap::new(),
    };
    match p.lines.first().map(|l| l.indent) {
        Some(indent) => p.block(indent),
        None => Value::Null,
    }
}

/// Split `key: value` (quoted keys allowed) into (key, value, quoted).
fn split_key(text: &str) -> Option<(String, &str, bool)> {
    let first = text.chars().next()?;
    if first == '"' || first == '\'' {
        let close = text[1..].find(first)? + 1;
        let rest = text[close + 1..].trim_start();
        let value = rest.strip_prefix(':')?;
        return Some((text[1..close].to_string(), value.trim(), true));
    }
    let mut search = 0;
    loop {
        let i = text[search..].find(':')? + search;
        let after = text[i + 1..].chars().next();
        if after.is_none() || after == Some(' ') || after == Some('\t') {
            return Some((text[..i].trim().to_string(), text[i + 1..].trim(), false));
        }
        search = i + 1;
    }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        let inner = &s[1..s.len() - 1];
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(o) => out.push(o),
                    None => {}
                }
            } else {
                out.push(c);
            }
        }
        return out;
    }
    if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        return s[1..s.len() - 1].replace("''", "'");
    }
    if s == "~" || s == "null" {
        return String::new();
    }
    s.to_string()
}

fn split_flow(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote = None;
    let mut depth = 0;
    for c in inner.chars() {
        match quote {
            Some(q) if c == q => {
                quote = None;
                cur.push(c);
            }
            Some(_) => cur.push(c),
            None => match c {
                '"' | '\'' => {
                    quote = Some(c);
                    cur.push(c);
                }
                '[' | '{' => {
                    depth += 1;
                    cur.push(c);
                }
                ']' | '}' => {
                    depth -= 1;
                    cur.push(c);
                }
                ',' if depth == 0 => out.push(std::mem::take(&mut cur).trim().to_string()),
                _ => cur.push(c),
            },
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

impl Parser<'_> {
    fn scalar(&self, v: &str, line: u32, col: u32) -> Value {
        let v = v.trim();
        if let Some(name) = v.strip_prefix('*') {
            return self.anchors.get(name.trim()).cloned().unwrap_or_default();
        }
        if v.starts_with('[') && v.ends_with(']') {
            return Value::Seq(
                split_flow(&v[1..v.len() - 1])
                    .iter()
                    .map(|x| self.scalar(x, line, col))
                    .collect(),
            );
        }
        if v.starts_with('{') && v.ends_with('}') {
            let mut entries = Vec::new();
            for part in split_flow(&v[1..v.len() - 1]) {
                if let Some((k, val, _)) = split_key(&part) {
                    entries.push(Entry {
                        key: k,
                        value: self.scalar(val, line, col),
                        line,
                        col,
                    });
                }
            }
            return Value::Map(entries);
        }
        if v.is_empty() || v == "~" || v == "null" {
            return Value::Null;
        }
        Value::Scalar(unquote(v))
    }

    /// `|` / `>` block scalar body after line `after` (more indented than `indent`).
    fn block_scalar(&mut self, header: &str, after: usize, indent: usize) -> Value {
        let folded = header.starts_with('>');
        let keep_last = !header.contains('-');
        let mut body: Vec<&str> = Vec::new();
        let mut j = after + 1;
        let mut base: Option<usize> = None;
        while j < self.all.len() {
            let raw = self.all[j];
            let t = raw.trim_start();
            if t.is_empty() {
                body.push("");
                j += 1;
                continue;
            }
            let ind = raw.len() - t.len();
            if ind <= indent {
                break;
            }
            let b = *base.get_or_insert(ind);
            body.push(raw.get(b.min(ind)..).unwrap_or(t));
            j += 1;
        }
        while body.last() == Some(&"") {
            body.pop();
        }
        while self.i < self.lines.len() && self.lines[self.i].no < j {
            self.i += 1;
        }
        let mut text = if folded {
            body.join(" ")
        } else {
            body.join("\n")
        };
        if keep_last && !text.is_empty() {
            text.push('\n');
        }
        Value::Scalar(text)
    }

    fn block(&mut self, indent: usize) -> Value {
        let Some(first) = self.lines.get(self.i) else {
            return Value::Null;
        };
        if first.text == "-" || first.text.starts_with("- ") {
            return self.sequence(indent);
        }
        let mut entries: Vec<Entry> = Vec::new();
        while let Some(l) = self.lines.get(self.i) {
            if l.indent < indent {
                break;
            }
            if l.indent > indent || l.text.starts_with("- ") || l.text == "-" {
                self.i += 1;
                continue;
            }
            let (no, text) = (l.no, l.text.clone());
            let line_indent = utf16_len(&l.raw[..l.indent]);
            self.i += 1;
            let Some((key, rest, quoted)) = split_key(&text) else {
                continue;
            };
            let (anchor, rest) = match rest.strip_prefix('&') {
                Some(r) => {
                    let end = r.find(char::is_whitespace).unwrap_or(r.len());
                    (Some(r[..end].to_string()), r[end..].trim())
                }
                None => (None, rest),
            };
            let line = no as u32;
            let col = line_indent + u32::from(quoted);
            let value = if rest.is_empty() {
                match self.lines.get(self.i) {
                    Some(n) if n.indent > indent => {
                        let ni = n.indent;
                        self.block(ni)
                    }
                    Some(n)
                        if n.indent == indent && (n.text.starts_with("- ") || n.text == "-") =>
                    {
                        self.sequence(indent)
                    }
                    _ => Value::Null,
                }
            } else if rest.starts_with('|') || rest.starts_with('>') {
                self.block_scalar(rest, no, indent)
            } else {
                self.scalar(rest, line, col)
            };
            if let Some(a) = anchor {
                self.anchors.insert(a, value.clone());
            }
            if key == "<<" {
                let merged: Vec<Entry> = match &value {
                    Value::Map(m) => m.clone(),
                    Value::Seq(s) => s.iter().flat_map(|v| v.entries().to_vec()).collect(),
                    _ => Vec::new(),
                };
                for e in merged {
                    if !entries.iter().any(|x| x.key == e.key) {
                        entries.push(e);
                    }
                }
                continue;
            }
            let entry = Entry {
                key,
                value,
                line,
                col,
            };
            match entries.iter_mut().find(|e| e.key == entry.key) {
                Some(e) => *e = entry,
                None => entries.push(entry),
            }
        }
        Value::Map(entries)
    }

    fn sequence(&mut self, indent: usize) -> Value {
        let mut items = Vec::new();
        while let Some(l) = self.lines.get(self.i) {
            if l.indent != indent || !(l.text == "-" || l.text.starts_with("- ")) {
                if l.indent > indent {
                    self.i += 1;
                    continue;
                }
                break;
            }
            let rest = l.text[1..].trim_start().to_string();
            let (no, item_indent) = (l.no, l.indent + (l.text.len() - rest.len()));
            if rest.is_empty() {
                self.i += 1;
                match self.lines.get(self.i) {
                    Some(n) if n.indent > indent => {
                        let ni = n.indent;
                        items.push(self.block(ni));
                    }
                    _ => items.push(Value::Null),
                }
            } else if split_key(&rest).is_some()
                && !rest.starts_with('"')
                && !rest.starts_with('\'')
            {
                // `- key: value` starts a mapping at the item's indentation.
                self.lines[self.i].indent = item_indent;
                self.lines[self.i].text = rest;
                items.push(self.block(item_indent));
            } else {
                self.i += 1;
                items.push(self.scalar(&rest, no as u32, item_indent as u32));
            }
        }
        Value::Seq(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_scalars_and_positions() {
        let v = parse(
            "# Locale\nen:\n  articles:\n    index:\n      title: \"Listing articles\" # x\n      empty: 'It''s empty'\n  hello: Hello #world\n  'quoted key': yes\n",
        );
        let en = v.get("en").unwrap();
        let idx = en.get("articles").unwrap().get("index").unwrap();
        assert_eq!(idx.get("title").unwrap().as_str(), Some("Listing articles"));
        assert_eq!(idx.get("empty").unwrap().as_str(), Some("It's empty"));
        let e = idx.entry("title").unwrap();
        assert_eq!((e.line, e.col), (4, 6));
        assert_eq!(
            en.get("hello").unwrap().as_str(),
            Some("Hello #world".split(' ').next().unwrap())
        );
        assert_eq!(en.entry("quoted key").unwrap().col, 3);
    }

    #[test]
    fn sequences_block_scalars_and_flow() {
        let v = parse(
            "en:\n  date:\n    day_names:\n    - Sunday\n    - Monday\n    abbr: [Sun, Mon]\n  body: |\n    Line one\n    Line two\n  folded: >-\n    a\n    b\n  items:\n    - name: x\n      price: 1\n    - name: y\n  after: z\n",
        );
        let en = v.get("en").unwrap();
        let date = en.get("date").unwrap();
        assert_eq!(
            date.get("day_names"),
            Some(&Value::Seq(vec![
                Value::Scalar("Sunday".into()),
                Value::Scalar("Monday".into())
            ]))
        );
        assert!(matches!(date.get("abbr"), Some(Value::Seq(s)) if s.len() == 2));
        assert_eq!(
            en.get("body").unwrap().as_str(),
            Some("Line one\nLine two\n")
        );
        assert_eq!(en.get("folded").unwrap().as_str(), Some("a b"));
        let Some(Value::Seq(items)) = en.get("items") else {
            panic!()
        };
        assert_eq!(items[1].get("name").unwrap().as_str(), Some("y"));
        assert_eq!(en.get("after").unwrap().as_str(), Some("z"));
    }

    #[test]
    fn anchors_and_merge_keys() {
        let v = parse(
            "default: &default\n  adapter: postgresql\n  pool: 5\n\ndevelopment:\n  <<: *default\n  database: blog_dev\n  pool: 10\n",
        );
        let dev = v.get("development").unwrap();
        assert_eq!(dev.get("adapter").unwrap().as_str(), Some("postgresql"));
        assert_eq!(dev.get("database").unwrap().as_str(), Some("blog_dev"));
        assert_eq!(dev.get("pool").unwrap().as_str(), Some("10"));
    }
}
