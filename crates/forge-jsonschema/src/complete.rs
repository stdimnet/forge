//! Completion of property names (required ones first, with type and documentation) and
//! values (`enum`, `const`, booleans, `null`, `default`, `examples`, `{}` / `[]` for
//! objects and arrays), with insert texts that write the colon, a value template and the
//! comma before a following property, like PHPStorm.
//!
//! JSON uses the parsed tree to find where the cursor is; YAML uses the indentation of the
//! lines above the cursor, which works on lines that are still being typed.

use serde::Serialize;

use crate::navigate::{self, Hit};
use crate::schema::{Registry, Sch};
use crate::tree::{quote, Node, Seg, Value};
use crate::DocKind;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub label: String,
    /// `property`, `value`, `keyword` (true/false/null) or `snippet` (`{}`, `[]`).
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Markdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    /// LSP snippet syntax (`$1`, `${1:x}`, `$0`).
    pub insert: String,
    pub required: bool,
    pub deprecated: bool,
    /// Open completion again after inserting (a property whose value has choices).
    pub retrigger: bool,
}

/// Byte offsets of the completion answer.
#[derive(Debug, Clone, PartialEq)]
pub struct Completion {
    /// Start of the text that filters the items (after an opening quote).
    pub from: usize,
    /// The range an item replaces.
    pub replace: (usize, usize),
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
enum Place {
    /// Property names of the object at `path`; `colon`: the key already has its colon.
    Key { colon: bool },
    /// A value at `path`.
    Value,
    /// A YAML `- ` item: keys of the item's object or a value of it.
    Item,
}

#[derive(Debug, Clone)]
struct Ctx {
    path: Vec<Seg>,
    place: Place,
    from: usize,
    replace: (usize, usize),
    /// JSON: a comma is needed after the inserted text.
    comma: bool,
    /// YAML: the column keys are written at.
    indent: usize,
    /// The key under the cursor (excluded from the existing keys).
    current: Option<String>,
}

// ---- JSON context -------------------------------------------------------------------

fn closed(text: &str, n: &Node) -> bool {
    let close = match n.value {
        Value::Object(_) => b'}',
        Value::Array(_) => b']',
        _ => return true,
    };
    n.end > n.start + 1 && text.as_bytes().get(n.end - 1) == Some(&close)
}

/// The cursor is inside the token `start..end` (a terminated string ends before its
/// closing quote).
fn inside(text: &str, start: usize, end: usize, offset: usize) -> bool {
    let b = text.as_bytes();
    let quoted = matches!(b.get(start), Some(b'"') | Some(b'\''));
    if quoted {
        let terminated = end >= start + 2 && b.get(end - 1) == b.get(start);
        offset > start && (offset < end || (!terminated && offset == end))
    } else {
        offset >= start && offset <= end && end > start
    }
}

fn word_from(text: &str, start: usize) -> usize {
    match text.as_bytes().get(start) {
        Some(b'"') | Some(b'\'') => start + 1,
        _ => start,
    }
}

/// A comma is needed when another property or item follows `after`.
fn needs_comma(text: &str, after: usize) -> bool {
    let rest = &text[after.min(text.len())..];
    let mut chars = rest.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        if rest[i..].starts_with("//") {
            let nl = rest[i..].find('\n').map_or(rest.len(), |j| i + j);
            while chars.peek().is_some_and(|(j, _)| *j < nl) {
                chars.next();
            }
            continue;
        }
        return c == '"'
            || c == '\''
            || c == '{'
            || c == '['
            || c.is_alphanumeric()
            || c == '_'
            || c == '-';
    }
    false
}

fn json_ctx(text: &str, n: &Node, offset: usize, path: &mut Vec<Seg>) -> Option<Ctx> {
    match &n.value {
        Value::Object(props) => {
            if offset <= n.start || (closed(text, n) && offset >= n.end) {
                return None;
            }
            for p in props {
                if inside(text, p.key_start, p.key_end, offset) {
                    let replace = (p.key_start, p.key_end);
                    return Some(Ctx {
                        path: path.clone(),
                        place: Place::Key {
                            colon: p.colon.is_some(),
                        },
                        from: word_from(text, p.key_start),
                        replace,
                        comma: p.colon.is_none() && needs_comma(text, p.key_end),
                        indent: 0,
                        current: Some(p.key.clone()),
                    });
                }
                let Some(colon) = p.colon else { continue };
                if offset <= colon {
                    continue;
                }
                match &p.value {
                    Some(v) if matches!(v.value, Value::Object(_) | Value::Array(_)) => {
                        if offset > v.start && (offset < v.end || !closed(text, v)) {
                            path.push(Seg::Key(p.key.clone()));
                            let r = json_ctx(text, v, offset, path);
                            path.pop();
                            return r;
                        }
                    }
                    Some(v) if inside(text, v.start, v.end, offset) || offset == v.start => {
                        let mut p2 = path.clone();
                        p2.push(Seg::Key(p.key.clone()));
                        return Some(Ctx {
                            path: p2,
                            place: Place::Value,
                            from: word_from(text, v.start),
                            replace: (v.start, v.end),
                            comma: needs_comma(text, v.end)
                                && !text[v.end..].trim_start().starts_with(','),
                            indent: 0,
                            current: None,
                        });
                    }
                    None => {
                        // `"key": |` before the next property or the closing brace.
                        let next = props
                            .iter()
                            .map(|q| q.key_start)
                            .find(|s| *s > colon)
                            .unwrap_or(n.end);
                        if offset <= next {
                            let mut p2 = path.clone();
                            p2.push(Seg::Key(p.key.clone()));
                            return Some(Ctx {
                                path: p2,
                                place: Place::Value,
                                from: offset,
                                replace: (offset, offset),
                                comma: needs_comma(text, offset),
                                indent: 0,
                                current: None,
                            });
                        }
                    }
                    _ => {}
                }
            }
            Some(Ctx {
                path: path.clone(),
                place: Place::Key { colon: false },
                from: offset,
                replace: (offset, offset),
                comma: needs_comma(text, offset),
                indent: 0,
                current: None,
            })
        }
        Value::Array(items) => {
            if offset <= n.start || (closed(text, n) && offset >= n.end) {
                return None;
            }
            let mut index = 0;
            for (i, item) in items.iter().enumerate() {
                if matches!(item.value, Value::Object(_) | Value::Array(_)) {
                    if offset > item.start && (offset < item.end || !closed(text, item)) {
                        path.push(Seg::Index(i));
                        let r = json_ctx(text, item, offset, path);
                        path.pop();
                        return r;
                    }
                } else if inside(text, item.start, item.end, offset) || offset == item.start {
                    let mut p2 = path.clone();
                    p2.push(Seg::Index(i));
                    return Some(Ctx {
                        path: p2,
                        place: Place::Value,
                        from: word_from(text, item.start),
                        replace: (item.start, item.end),
                        comma: false,
                        indent: 0,
                        current: None,
                    });
                }
                if item.end <= offset {
                    index = i + 1;
                }
            }
            let mut p2 = path.clone();
            p2.push(Seg::Index(index));
            Some(Ctx {
                path: p2,
                place: Place::Value,
                from: offset,
                replace: (offset, offset),
                comma: needs_comma(text, offset),
                indent: 0,
                current: None,
            })
        }
        _ => {
            if inside(text, n.start, n.end, offset) || offset == n.start {
                Some(Ctx {
                    path: path.clone(),
                    place: Place::Value,
                    from: word_from(text, n.start),
                    replace: (n.start, n.end),
                    comma: false,
                    indent: 0,
                    current: None,
                })
            } else {
                None
            }
        }
    }
}

// ---- YAML context -------------------------------------------------------------------

struct Line {
    indent: usize,
    key_indent: usize,
    key: Option<String>,
    key_start: usize,
    dash: bool,
}

fn yaml_key(s: &str) -> Option<(String, usize, usize)> {
    // Quoted key.
    if let Some(q) = s.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let end = s[1..].find(q)? + 1;
        let rest = s[end + 1..].trim_start();
        return (rest.starts_with(':') && (rest.len() == 1 || rest[1..].starts_with([' ', '\t'])))
            .then(|| (s[1..end].to_string(), 1, end));
    }
    let first = s.chars().next()?;
    if "#[]{},&*!|>'\"%@`".contains(first)
        || ((first == '-' || first == '?') && s[1..].starts_with([' ', '\t']))
    {
        return None;
    }
    let b = s.as_bytes();
    for i in 0..b.len() {
        if b[i] == b'#' && i > 0 && (b[i - 1] == b' ' || b[i - 1] == b'\t') {
            return None;
        }
        if b[i] == b':' && (i + 1 == b.len() || b[i + 1] == b' ' || b[i + 1] == b'\t') {
            let key = s[..i].trim_end();
            return (!key.is_empty()).then(|| (key.to_string(), 0, key.len()));
        }
    }
    None
}

fn parse_line(text: &str) -> Option<Line> {
    let trimmed = text.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let indent = text.len() - trimmed.len();
    let mut rest = trimmed;
    let mut key_indent = indent;
    let mut dash = false;
    while rest.starts_with('-') && (rest.len() == 1 || rest[1..].starts_with([' ', '\t'])) {
        dash = true;
        let after = rest[1..].trim_start();
        key_indent += rest.len() - after.len();
        rest = after;
    }
    match yaml_key(rest) {
        Some((key, s, _)) => Some(Line {
            indent,
            key_indent,
            key: Some(key),
            key_start: key_indent + s,
            dash,
        }),
        None if dash => Some(Line {
            indent,
            key_indent,
            key: None,
            key_start: key_indent,
            dash: true,
        }),
        None => None,
    }
}

#[derive(Clone)]
struct Entry {
    indent: usize,
    key: Option<String>,
}

fn advance(stack: &mut Vec<Entry>, l: &Line) {
    if l.dash {
        while stack
            .last()
            .is_some_and(|e| e.indent > l.indent || (e.indent == l.indent && e.key.is_none()))
        {
            stack.pop();
        }
        stack.push(Entry {
            indent: l.indent,
            key: None,
        });
    } else {
        while stack.last().is_some_and(|e| e.indent >= l.key_indent) {
            stack.pop();
        }
    }
    if let Some(k) = &l.key {
        while stack.last().is_some_and(|e| e.indent >= l.key_indent) {
            stack.pop();
        }
        stack.push(Entry {
            indent: l.key_indent,
            key: Some(k.clone()),
        });
    }
}

/// Turn `key`/`-` entries into a path, choosing sequence items from the tree by position.
fn to_path(entries: &[Entry], doc: Option<&Node>, offset: usize) -> Vec<Seg> {
    let mut node = doc;
    let mut path = Vec::new();
    for e in entries {
        match &e.key {
            Some(k) => {
                path.push(Seg::Key(k.clone()));
                node = node.and_then(|n| n.get(k));
            }
            None => {
                let items = node.and_then(Node::as_array).unwrap_or(&[]);
                let i = items.iter().rposition(|it| it.start <= offset).unwrap_or(0);
                path.push(Seg::Index(i));
                node = items.get(i);
            }
        }
    }
    path
}

fn yaml_ctx(text: &str, doc: Option<&Node>, offset: usize) -> Option<Ctx> {
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
    let line = &text[line_start..line_end];
    let before = &text[line_start..offset];
    if before.trim_start().starts_with('#') {
        return None;
    }
    // Lines of this document above the cursor.
    let doc_start = std::iter::once(0)
        .chain(text[..line_start].match_indices('\n').map(|(i, _)| i + 1))
        .rfind(|&i| i < line_start && text[i..].starts_with("---"))
        .map_or(0, |i| {
            text[i..].find('\n').map_or(text.len(), |j| i + j + 1)
        });
    let mut stack: Vec<Entry> = Vec::new();
    for l in text[doc_start.min(line_start)..line_start].lines() {
        if let Some(l) = parse_line(l) {
            advance(&mut stack, &l);
        }
    }
    let ancestors = |stack: &[Entry], indent: usize| -> Vec<Entry> {
        stack
            .iter()
            .filter(|e| e.indent < indent)
            .cloned()
            .collect()
    };
    let parsed = parse_line(line);
    // Value position: `key: |`.
    if let Some(l) = parsed.as_ref().filter(|l| l.key.is_some()) {
        let colon = line[l.key_start..].find(':').map(|i| l.key_start + i);
        if let Some(colon) = colon.filter(|c| line_start + c < offset) {
            advance(&mut stack, l);
            let path = to_path(&ancestors(&stack, l.key_indent + 1), doc, offset);
            let value_start = line_start
                + colon
                + 1
                + (line[colon + 1..].len() - line[colon + 1..].trim_start().len());
            let raw = &text[value_start.min(line_end)..line_end];
            let cut = raw.find(" #").unwrap_or(raw.len());
            let value_end = value_start + raw[..cut].trim_end().len();
            let (from, replace) = if offset < value_start {
                (offset, (offset, offset))
            } else {
                (
                    word_from(text, value_start).min(offset),
                    (value_start, value_end.max(offset)),
                )
            };
            return Some(Ctx {
                path,
                place: Place::Value,
                from,
                replace,
                comma: false,
                indent: l.key_indent,
                current: None,
            });
        }
    }
    // Key position.
    let ws = before.len() - before.trim_start().len();
    let mut indent = ws;
    let mut rest = &before[ws..];
    let mut dash = false;
    while rest.starts_with('-') && (rest.len() == 1 || rest[1..].starts_with([' ', '\t'])) {
        dash = true;
        let after = rest[1..].trim_start();
        indent += rest.len() - after.len();
        rest = after;
    }
    if dash {
        advance(
            &mut stack,
            &Line {
                indent: ws,
                key_indent: indent,
                key: None,
                key_start: indent,
                dash: true,
            },
        );
    }
    let word_start = line_start + indent;
    let typed = &text[word_start..offset];
    if typed.contains(':')
        || (typed.contains(['{', '[', '"', '\'']) && !typed.starts_with(['"', '\'']))
    {
        return None;
    }
    let path = to_path(&ancestors(&stack, indent), doc, offset);
    let (colon, end) = match parsed.as_ref().and_then(|l| l.key.as_ref().map(|_| l)) {
        Some(l) if l.key_start == indent => {
            let c = line[l.key_start..]
                .find(':')
                .map(|i| l.key_start + i)
                .unwrap_or(line.len());
            (true, line_start + line[..c].trim_end().len())
        }
        _ => {
            let word_end = line[offset - line_start..]
                .find(|c: char| c.is_whitespace() || c == ':')
                .map_or(line_end, |i| offset + i);
            (false, word_end)
        }
    };
    let current =
        (end > word_start).then(|| text[word_start..end].trim_matches(['"', '\'']).to_string());
    Some(Ctx {
        path,
        place: if dash && !colon {
            Place::Item
        } else {
            Place::Key { colon }
        },
        from: word_from(text, word_start).min(offset),
        replace: (word_start, end.max(offset)),
        comma: false,
        indent,
        current,
    })
}

// ---- Items --------------------------------------------------------------------------

fn snippet_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('$', "\\$")
        .replace('}', "\\}")
}

/// What to insert after `"key": ` (JSON) for a property with these schemas.
fn json_value_template(schemas: &[Sch<'_>]) -> String {
    if let Some(d) = schemas.iter().find_map(|s| s.get("default")) {
        return match &d.value {
            Value::String(s) => format!("\"${{1:{}}}\"", snippet_escape(s)),
            Value::Object(_) | Value::Array(_) => {
                if d.to_json().len() <= 2 {
                    format!("{}$1{}", &d.to_json()[..1], &d.to_json()[1..])
                } else {
                    snippet_escape(&d.to_json())
                }
            }
            _ => format!("${{1:{}}}", snippet_escape(&d.to_json())),
        };
    }
    let label = navigate::type_label(schemas).unwrap_or_default();
    match label.split(" | ").next().unwrap_or("") {
        "string" => "\"$1\"".into(),
        "object" => "{$1}".into(),
        "array" => "[$1]".into(),
        "null" => "null".into(),
        _ => "$1".into(),
    }
}

fn yaml_key_insert(name: &str, schemas: &[Sch<'_>], indent: usize) -> String {
    let label = navigate::type_label(schemas).unwrap_or_default();
    let pad = " ".repeat(indent + 2);
    let key = yaml_scalar(name);
    match label.as_str() {
        "object" => format!("{key}:\n{pad}$1"),
        "array" => format!("{key}:\n{pad}- $1"),
        _ => match schemas.iter().find_map(|s| s.get("default")) {
            Some(d) if !matches!(d.value, Value::Object(_) | Value::Array(_)) => {
                format!("{key}: ${{1:{}}}", snippet_escape(&yaml_value(d)))
            }
            _ => format!("{key}: $1"),
        },
    }
}

/// A scalar written for YAML: plain when it reads back as the same string.
fn yaml_scalar(s: &str) -> String {
    let plain_ok = !s.is_empty()
        && !s.starts_with([
            ' ', '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"',
            '%', '@', '`',
        ])
        && !s.ends_with(' ')
        && !s.contains(": ")
        && !s.contains(" #")
        && !s.contains('\n')
        && matches!(crate::yaml::resolve_plain(s), Value::String(_));
    if plain_ok {
        s.to_string()
    } else {
        quote(s)
    }
}

fn yaml_value(n: &Node) -> String {
    match &n.value {
        Value::String(s) => yaml_scalar(s),
        _ => n.to_json(),
    }
}

struct Builder<'a> {
    kind: DocKind,
    items: Vec<Item>,
    ctx: &'a Ctx,
}

impl Builder<'_> {
    fn add(&mut self, item: Item) {
        if !self
            .items
            .iter()
            .any(|i| i.label == item.label && i.kind == item.kind)
        {
            self.items.push(item);
        }
    }

    /// A value literal: JSON text, or YAML text.
    fn value(&mut self, n: &Node, kind: &'static str, detail: Option<String>, doc: Option<String>) {
        let (label, mut insert) = match self.kind {
            DocKind::Json => {
                let label = match &n.value {
                    Value::String(s) => s.clone(),
                    _ => n.to_json(),
                };
                (label, snippet_escape(&n.to_json()))
            }
            DocKind::Yaml => {
                let label = match &n.value {
                    Value::String(s) => s.clone(),
                    _ => n.to_json(),
                };
                (label, snippet_escape(&yaml_value(n)))
            }
        };
        if self.ctx.comma {
            insert.push(',');
        }
        self.add(Item {
            label,
            kind,
            detail,
            documentation: doc,
            insert,
            required: false,
            deprecated: false,
            retrigger: false,
        });
    }

    fn values(&mut self, schemas: &[Sch<'_>]) {
        let desc = navigate::description(schemas).map(str::to_string);
        for s in schemas {
            if let Some(values) = s.get("enum").and_then(Node::as_array) {
                let docs = s
                    .get("markdownEnumDescriptions")
                    .or_else(|| s.get("enumDescriptions"))
                    .and_then(Node::as_array);
                for (i, v) in values.iter().enumerate() {
                    let doc = docs
                        .and_then(|d| d.get(i))
                        .and_then(Node::as_str)
                        .map(str::to_string)
                        .or_else(|| desc.clone());
                    self.value(v, "value", None, doc);
                }
            }
            if let Some(c) = s.get("const") {
                self.value(c, "value", None, desc.clone());
            }
        }
        let label = navigate::type_label(schemas).unwrap_or_default();
        for t in label.split(" | ") {
            match t {
                "boolean" => {
                    for b in [true, false] {
                        self.value(&Node::new(Value::Bool(b), 0, 0), "keyword", None, None);
                    }
                }
                "null" => self.value(&Node::new(Value::Null, 0, 0), "keyword", None, None),
                "object" | "array" if self.kind == DocKind::Json => {
                    let (l, i) = if t == "object" {
                        ("{}", "{$1}")
                    } else {
                        ("[]", "[$1]")
                    };
                    let mut insert = i.to_string();
                    if self.ctx.comma {
                        insert.push(',');
                    }
                    self.add(Item {
                        label: l.into(),
                        kind: "snippet",
                        detail: Some(t.into()),
                        documentation: None,
                        insert,
                        required: false,
                        deprecated: false,
                        retrigger: false,
                    });
                }
                _ => {}
            }
        }
        for s in schemas {
            if let Some(d) = s.get("default") {
                self.value(d, "value", Some("default".into()), desc.clone());
            }
            for e in s.get("examples").and_then(Node::as_array).unwrap_or(&[]) {
                if !matches!(e.value, Value::Object(_) | Value::Array(_)) {
                    self.value(e, "value", Some("example".into()), desc.clone());
                }
            }
        }
    }

    fn properties<'r>(
        &mut self,
        reg: &'r Registry,
        schemas: &[Sch<'r>],
        node: Option<&Node>,
        colon: bool,
    ) {
        let existing: Vec<&str> = node
            .and_then(Node::as_object)
            .unwrap_or(&[])
            .iter()
            .map(|p| p.key.as_str())
            .filter(|k| Some(*k) != self.ctx.current.as_deref())
            .collect();
        let mut required: Vec<&str> = Vec::new();
        for s in schemas {
            for r in s.get("required").and_then(Node::as_array).unwrap_or(&[]) {
                if let Some(r) = r.as_str() {
                    required.push(r);
                }
            }
        }
        let mut names: Vec<(String, Vec<Sch<'r>>)> = Vec::new();
        for s in schemas {
            for p in s.get("properties").and_then(Node::as_object).unwrap_or(&[]) {
                if names.iter().any(|(n, _)| n == &p.key) || existing.contains(&p.key.as_str()) {
                    continue;
                }
                let ats = navigate::property(reg, schemas, &p.key);
                let subs: Vec<Sch<'r>> = ats
                    .iter()
                    .flat_map(|a| navigate::expand(reg, a.sch, None))
                    .collect();
                if ats.iter().all(|a| a.sch.boolean() == Some(false)) {
                    continue;
                }
                names.push((p.key.clone(), subs));
            }
            let enum_names = s
                .get("propertyNames")
                .and_then(|p| p.get("enum"))
                .and_then(Node::as_array)
                .unwrap_or(&[]);
            for n in enum_names.iter().filter_map(Node::as_str) {
                if !names.iter().any(|(x, _)| x == n) && !existing.contains(&n) {
                    names.push((n.to_string(), Vec::new()));
                }
            }
        }
        // Required properties first, each group in schema order.
        names.sort_by_key(|(n, _)| !required.contains(&n.as_str()));
        for (name, subs) in names {
            let deprecated = subs.iter().any(|s| {
                s.get("deprecated").and_then(Node::as_bool) == Some(true)
                    || s.get("deprecationMessage").is_some()
            });
            let insert = if colon {
                match self.kind {
                    DocKind::Json => snippet_escape(&quote(&name)),
                    DocKind::Yaml => snippet_escape(&yaml_scalar(&name)),
                }
            } else {
                match self.kind {
                    DocKind::Json => {
                        let mut s = format!(
                            "{}: {}",
                            snippet_escape(&quote(&name)),
                            json_value_template(&subs)
                        );
                        if self.ctx.comma {
                            s.push(',');
                        }
                        s
                    }
                    DocKind::Yaml => {
                        yaml_key_insert(&snippet_escape(&name), &subs, self.ctx.indent)
                    }
                }
            };
            let label = navigate::type_label(&subs).unwrap_or_default();
            let retrigger = !colon
                && (label == "boolean"
                    || subs
                        .iter()
                        .any(|s| s.get("enum").is_some() || s.get("const").is_some()));
            self.add(Item {
                label: name.clone(),
                kind: "property",
                detail: navigate::type_label(&subs),
                documentation: navigate::description(&subs).map(str::to_string),
                insert,
                required: required.contains(&name.as_str()),
                deprecated,
                retrigger,
            });
        }
    }
}

/// Completion at byte `offset` of `text` (`docs`: the parsed documents).
pub fn complete(
    reg: &Registry,
    root: Sch<'_>,
    kind: DocKind,
    text: &str,
    docs: &[Node],
    offset: usize,
) -> Option<Completion> {
    let doc = crate::document_at(docs, offset);
    let ctx = match kind {
        DocKind::Json => match doc {
            Some(d) => json_ctx(text, d, offset, &mut Vec::new())?,
            None => Ctx {
                path: Vec::new(),
                place: Place::Value,
                from: offset,
                replace: (offset, offset),
                comma: false,
                indent: 0,
                current: None,
            },
        },
        DocKind::Yaml => yaml_ctx(text, doc, offset)?,
    };
    let mut b = Builder {
        kind,
        items: Vec::new(),
        ctx: &ctx,
    };
    let (ats, node) = navigate::at_path(reg, root, doc, &ctx.path);
    let mut schemas: Vec<Sch<'_>> = Vec::new();
    for at in &ats {
        for s in navigate::expand(reg, at.sch, node) {
            if !schemas.iter().any(|x| x.same(&s)) {
                schemas.push(s);
            }
        }
    }
    match ctx.place {
        Place::Key { colon } => b.properties(reg, &schemas, node, colon),
        Place::Value => b.values(&schemas),
        Place::Item => {
            b.properties(reg, &schemas, node, false);
            b.values(&schemas);
        }
    }
    if b.items.is_empty() {
        return None;
    }
    Some(Completion {
        from: ctx.from,
        replace: ctx.replace,
        items: b.items,
    })
}

/// The key or value under `offset` in a document, for hover and navigation.
pub fn hit_at(docs: &[Node], offset: usize) -> Option<Hit<'_>> {
    navigate::locate(crate::document_at(docs, offset)?, offset)
}
