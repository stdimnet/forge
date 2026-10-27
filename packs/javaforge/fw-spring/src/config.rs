//! Spring Boot configuration files: `application.properties` / `application.yml` (and
//! `application-<profile>.*`, `bootstrap.*`) flattened into dotted keys with their
//! positions, plus where the cursor is (a key being typed or a value) for completion.
//! YAML is read line by line from the indentation, so files that are being typed still
//! work. Offsets are byte offsets into the text.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Format {
    Properties,
    Yaml,
}

/// `application-dev.yml` → (`Yaml`, `Some("dev")`); `None` for other files.
pub fn config_file(path: &str) -> Option<(Format, Option<String>)> {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let (stem, ext) = name.rsplit_once('.')?;
    let format = match ext.to_ascii_lowercase().as_str() {
        "properties" => Format::Properties,
        "yml" | "yaml" => Format::Yaml,
        _ => return None,
    };
    for base in ["application", "bootstrap"] {
        if stem == base {
            return Some((format, None));
        }
        if let Some(profile) = stem.strip_prefix(base).and_then(|r| r.strip_prefix('-')) {
            if !profile.is_empty() {
                return Some((format, Some(profile.to_string())));
            }
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// The full dotted key as written (`spring.datasource.url`, `include[0]`).
    pub key: String,
    /// 0-based line of the key.
    pub line: u32,
    /// Byte range of the key text on its line (YAML: the last segment).
    pub key_from: usize,
    pub key_to: usize,
    /// The scalar value (unquoted); `None` for a YAML mapping key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub value_from: usize,
    pub value_to: usize,
    /// YAML (`---`) or properties (`#---`) document index.
    pub document: usize,
}

/// Parse a configuration file in `format`.
pub fn parse(format: Format, text: &str) -> Vec<Entry> {
    match format {
        Format::Properties => parse_properties(text),
        Format::Yaml => parse_yaml(text),
    }
}

/// Lines with their byte start.
fn lines(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = 0;
    for l in text.split('\n') {
        out.push((start, l.strip_suffix('\r').unwrap_or(l)));
        start += l.len() + 1;
    }
    out
}

pub fn parse_properties(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let ls = lines(text);
    let mut doc = 0;
    let mut i = 0;
    while i < ls.len() {
        let (start, line) = ls[i];
        let n = i as u32;
        i += 1;
        let trimmed = line.trim_start();
        if trimmed == "#---" || trimmed == "!---" {
            doc += 1;
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        let indent = line.len() - trimmed.len();
        let (key, key_len) = property_key(trimmed);
        if key.is_empty() {
            continue;
        }
        let key_from = start + indent;
        let rest = &trimmed[key_len..];
        let after_ws = rest.trim_start_matches([' ', '\t']);
        let after_sep = after_ws
            .strip_prefix(['=', ':'])
            .map(|r| r.trim_start_matches([' ', '\t']))
            .unwrap_or(after_ws);
        let value_from = key_from + key_len + (rest.len() - after_sep.len());
        let mut value = after_sep.to_string();
        let mut value_to = start + line.len();
        // Continuation lines.
        while (value.len() - value.trim_end_matches('\\').len()) % 2 == 1 && i < ls.len() {
            value.pop();
            let (s2, l2) = ls[i];
            value.push_str(l2.trim_start());
            value_to = s2 + l2.len();
            i += 1;
        }
        out.push(Entry {
            key,
            line: n,
            key_from,
            key_to: key_from + key_len,
            value: Some(unescape(&value)),
            value_from,
            value_to,
            document: doc,
        });
    }
    out
}

/// The key at the start of a properties line and its length in bytes.
fn property_key(s: &str) -> (String, usize) {
    let mut key = String::new();
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                if let Some((_, n)) = chars.next() {
                    key.push(n);
                }
            }
            '=' | ':' | ' ' | '\t' => return (key, i),
            _ => key.push(c),
        }
    }
    (key, s.len())
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
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
    out
}

/// A YAML line split into indentation, list marker and content.
struct YamlLine<'a> {
    start: usize,
    indent: usize,
    /// Column of the content after `- ` markers.
    content_col: usize,
    list_items: usize,
    content: &'a str,
}

fn strip_comment(s: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut prev_space = true;
    for (i, c) in s.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' && prev_space => return s[..i].trim_end(),
            None => {}
        }
        prev_space = c == ' ' || c == '\t';
    }
    s.trim_end()
}

fn yaml_line(start: usize, raw: &str) -> YamlLine<'_> {
    let indent = raw.len() - raw.trim_start_matches(' ').len();
    let mut col = indent;
    let mut rest = &raw[indent..];
    let mut items = 0;
    while rest == "-" || rest.starts_with("- ") {
        items += 1;
        let after = rest[1..].trim_start_matches(' ');
        col += rest.len() - after.len();
        rest = after;
    }
    YamlLine {
        start,
        indent,
        content_col: col,
        list_items: items,
        content: strip_comment(rest),
    }
}

/// `key: value` → (key, value offset in `content`); keys may be quoted.
fn split_key(content: &str) -> Option<(String, usize, usize)> {
    let bytes = content.as_bytes();
    let (key, key_end) = if bytes.first().is_some_and(|b| *b == b'"' || *b == b'\'') {
        let q = bytes[0] as char;
        let close = content[1..].find(q)? + 1;
        (content[1..close].to_string(), close + 1)
    } else {
        let mut end = None;
        for (i, c) in content.char_indices() {
            if c == ':' && content[i + 1..].chars().next().is_none_or(|n| n == ' ' || n == '\t') {
                end = Some(i);
                break;
            }
            if i == 0 && matches!(c, '[' | '{' | '"' | '\'' | '|' | '>' | '&' | '*' | '!') {
                return None;
            }
        }
        let end = end?;
        (content[..end].trim_end().to_string(), end)
    };
    let after = content[key_end..].strip_prefix(':')?;
    let value_off = key_end + 1 + (after.len() - after.trim_start().len());
    Some((key, key_end, value_off))
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    if v.len() >= 2 && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\''))) {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

#[derive(Debug, Clone)]
struct Frame {
    /// Column of the key (or of a list item's content).
    col: usize,
    key: String,
    /// Next list index under this key.
    next_item: usize,
}

fn join(stack: &[Frame]) -> String {
    let mut out = String::new();
    for f in stack {
        if f.key.starts_with('[') || out.is_empty() {
            out.push_str(&f.key);
        } else {
            out.push('.');
            out.push_str(&f.key);
        }
    }
    out
}

/// Pop frames that the line at `col` closes. A list item at the key's own column belongs
/// to that key.
fn pop_to(stack: &mut Vec<Frame>, col: usize, list_item: bool) {
    while let Some(top) = stack.last() {
        let keep = top.col < col || (list_item && top.col == col && !top.key.starts_with('['));
        if keep {
            break;
        }
        stack.pop();
    }
}

pub fn parse_yaml(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut doc = 0;
    let mut block: Option<usize> = None;
    for (n, (start, raw)) in lines(text).into_iter().enumerate() {
        let l = yaml_line(start, raw);
        if let Some(col) = block {
            if raw.trim().is_empty() || l.indent > col {
                continue;
            }
            block = None;
        }
        if raw.trim_end() == "---" || raw.starts_with("--- ") {
            doc += 1;
            stack.clear();
            continue;
        }
        if l.content.is_empty() && l.list_items == 0 || raw.trim_start().starts_with('#') || raw.trim_end() == "..." {
            continue;
        }
        if l.list_items > 0 {
            pop_to(&mut stack, l.indent, true);
            for k in 0..l.list_items {
                let index = stack.last_mut().map(|f| {
                    let i = f.next_item;
                    f.next_item += 1;
                    i
                });
                let col = if k + 1 == l.list_items { l.content_col } else { l.indent + 2 * (k + 1) };
                stack.push(Frame {
                    col,
                    key: format!("[{}]", index.unwrap_or(0)),
                    next_item: 0,
                });
            }
            // The item's own mapping key sits at the content column: keep the item frame.
            if let Some(f) = stack.last_mut() {
                f.col = l.content_col.saturating_sub(1);
            }
        } else {
            pop_to(&mut stack, l.content_col, false);
        }
        let content_start = l.start + l.content_col;
        match split_key(l.content) {
            Some((key, key_end, value_off)) => {
                let value_text = &l.content[value_off.min(l.content.len())..];
                let key_from = content_start + l.content[..key_end].find(key.as_str()).unwrap_or(0);
                stack.push(Frame {
                    col: l.content_col,
                    key: key.clone(),
                    next_item: 0,
                });
                let full = join(&stack);
                let is_block = value_text.starts_with('|') || value_text.starts_with('>');
                let mapping = value_text.is_empty();
                if is_block {
                    block = Some(l.content_col);
                }
                out.push(Entry {
                    key: full,
                    line: n as u32,
                    key_from,
                    key_to: key_from + key.len(),
                    value: if mapping {
                        None
                    } else if is_block {
                        Some(String::new())
                    } else {
                        Some(unquote(value_text))
                    },
                    value_from: content_start + value_off,
                    value_to: content_start + l.content.len(),
                    document: doc,
                });
                if !mapping {
                    stack.pop();
                }
            }
            None if l.list_items > 0 => {
                // A scalar list item.
                let full = join(&stack);
                out.push(Entry {
                    key: full,
                    line: n as u32,
                    key_from: content_start,
                    key_to: content_start,
                    value: Some(unquote(l.content)),
                    value_from: content_start,
                    value_to: content_start + l.content.len(),
                    document: doc,
                });
                stack.pop();
            }
            None => {}
        }
    }
    out
}

/// Where the cursor is in a configuration file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// Typing a key: `parent` is the dotted path above (YAML), `typed` the text typed so
    /// far, which replaces `from..to`; `indent` is the key's column.
    Key {
        parent: String,
        typed: String,
        from: usize,
        to: usize,
        indent: usize,
    },
    /// Typing the value of `key`; `typed` replaces `from..to`.
    Value {
        key: String,
        typed: String,
        from: usize,
        to: usize,
    },
}

/// The completion context at byte `offset`.
pub fn place_at(format: Format, text: &str, offset: usize) -> Option<Place> {
    let offset = offset.min(text.len());
    let line_start = text[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = text[offset..].find('\n').map(|i| offset + i).unwrap_or(text.len());
    let line = &text[line_start..line_end];
    let before = &text[line_start..offset];
    match format {
        Format::Properties => {
            let trimmed = before.trim_start();
            if trimmed.starts_with('#') || trimmed.starts_with('!') {
                return None;
            }
            let indent = before.len() - trimmed.len();
            let (key, key_len) = property_key(&line[indent..]);
            if key_len >= trimmed.len() {
                // Still in the key.
                let to = line_start + indent + key_len;
                return Some(Place::Key {
                    parent: String::new(),
                    typed: trimmed.to_string(),
                    from: line_start + indent,
                    to: to.max(offset),
                    indent,
                });
            }
            let rest = &line[indent + key_len..];
            let after = rest.trim_start_matches([' ', '\t']);
            let after = after.strip_prefix(['=', ':']).unwrap_or(after).trim_start_matches([' ', '\t']);
            let from = line_end - after.len();
            if offset < from {
                return None;
            }
            Some(Place::Value {
                key,
                typed: text[from..offset].to_string(),
                from,
                to: line_end,
            })
        }
        Format::Yaml => {
            let l = yaml_line(line_start, line);
            if line.trim_start().starts_with('#') {
                return None;
            }
            let col = offset - line_start;
            // The path above: parse the text up to this line, then pop to the column.
            let above = &text[..line_start];
            let entries = parse_yaml(above);
            let mut parent = parent_path(above, entries.last(), l.content_col);
            if l.list_items > 0 {
                if col < l.content_col {
                    return None;
                }
                let content = &line[l.content_col..];
                if split_key(&content[..(col - l.content_col).min(content.len())]).is_none() && !content.contains(": ") {
                    // A scalar item: its value.
                    let key = parent_path(above, entries.last(), l.indent);
                    return Some(Place::Value {
                        key,
                        typed: line[l.content_col..col].to_string(),
                        from: line_start + l.content_col,
                        to: line_end,
                    });
                }
                parent = format!("{}[0]", parent_path(above, entries.last(), l.indent));
            }
            let content = &line[l.content_col.min(line.len())..];
            let content_before = &before[l.content_col.min(before.len())..];
            if let Some((key, _key_end, value_off)) = split_key(content_before).or_else(|| {
                split_key(content).filter(|(_, ke, _)| *ke < content_before.len())
            }) {
                if content_before.len() >= value_off.min(content.len()) || content_before.len() > key.len() {
                    let from = line_start + l.content_col + value_off.min(content.len());
                    let from = from.min(offset);
                    let full = if parent.is_empty() { key } else { format!("{parent}.{key}") };
                    return Some(Place::Value {
                        key: full,
                        typed: text[from..offset].to_string(),
                        from,
                        to: line_end,
                    });
                }
            }
            if col < l.content_col {
                return None;
            }
            let key_len = content.find(':').unwrap_or(content.len());
            Some(Place::Key {
                parent,
                typed: content_before.to_string(),
                from: line_start + l.content_col,
                to: (line_start + l.content_col + key_len).max(offset),
                indent: l.content_col,
            })
        }
    }
}

/// The dotted path of the mapping a line at `col` belongs to, from the lines above.
fn parent_path(above: &str, _last: Option<&Entry>, col: usize) -> String {
    // Re-run the indentation stack over the lines above and pop to `col`.
    let mut stack: Vec<Frame> = Vec::new();
    let mut block: Option<usize> = None;
    for (start, raw) in lines(above) {
        let l = yaml_line(start, raw);
        if let Some(c) = block {
            if raw.trim().is_empty() || l.indent > c {
                continue;
            }
            block = None;
        }
        if raw.trim_end() == "---" {
            stack.clear();
            continue;
        }
        if l.content.is_empty() && l.list_items == 0 || raw.trim_start().starts_with('#') {
            continue;
        }
        if l.list_items > 0 {
            pop_to(&mut stack, l.indent, true);
            for _ in 0..l.list_items {
                let index = stack.last_mut().map(|f| {
                    let i = f.next_item;
                    f.next_item += 1;
                    i
                });
                stack.push(Frame {
                    col: l.content_col.saturating_sub(1),
                    key: format!("[{}]", index.unwrap_or(0)),
                    next_item: 0,
                });
            }
        } else {
            pop_to(&mut stack, l.content_col, false);
        }
        if let Some((key, _, value_off)) = split_key(l.content) {
            let value = &l.content[value_off.min(l.content.len())..];
            if value.starts_with('|') || value.starts_with('>') {
                block = Some(l.content_col);
            }
            if value.is_empty() {
                stack.push(Frame {
                    col: l.content_col,
                    key,
                    next_item: 0,
                });
            }
        } else if l.list_items > 0 {
            stack.pop();
        }
    }
    pop_to(&mut stack, col, false);
    join(&stack)
}

/// Canonical form of a key for relaxed binding: lower case, `-` between words, no `_`
/// (`spring.jpa.showSql`, `SPRING_JPA_SHOW_SQL` and `spring.jpa.show_sql` are all
/// `spring.jpa.show-sql`). List indices (`[0]`) are dropped; map keys in brackets stay.
pub fn canonical(key: &str) -> String {
    let mut out = String::with_capacity(key.len() + 4);
    let mut prev_lower = false;
    let mut chars = key.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            let mut inner = String::new();
            for n in chars.by_ref() {
                if n == ']' {
                    break;
                }
                inner.push(n);
            }
            if !inner.chars().all(|d| d.is_ascii_digit()) {
                out.push('.');
                out.push_str(&inner.to_ascii_lowercase());
            }
            prev_lower = false;
            continue;
        }
        if c.is_ascii_uppercase() {
            if prev_lower {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
            prev_lower = false;
        } else if c == '_' {
            out.push('-');
            prev_lower = false;
        } else {
            out.push(c);
            prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        }
    }
    out
}

/// `camelCase` / `snake_case` → `kebab-case` (a property name from a field).
pub fn kebab(name: &str) -> String {
    canonical(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_and_profiles() {
        assert_eq!(config_file("src/main/resources/application.yml"), Some((Format::Yaml, None)));
        assert_eq!(
            config_file("application-dev.properties"),
            Some((Format::Properties, Some("dev".into())))
        );
        assert_eq!(config_file("bootstrap-k8s.yaml").unwrap().1.as_deref(), Some("k8s"));
        assert_eq!(config_file("app.yml"), None);
        assert_eq!(config_file("application.json"), None);
    }

    #[test]
    fn properties_keys_values_and_continuations() {
        let text = "# comment\nserver.port=8080\nspring.application.name : shop\nlogging.level.org.hibernate.SQL  debug\nmulti=a,\\\n  b\n#---\nspring.config.activate.on-profile=dev\n";
        let e = parse_properties(text);
        assert_eq!(e.len(), 5);
        assert_eq!((e[0].key.as_str(), e[0].value.as_deref(), e[0].line), ("server.port", Some("8080"), 1));
        assert_eq!(&text[e[0].key_from..e[0].key_to], "server.port");
        assert_eq!(&text[e[0].value_from..e[0].value_to], "8080");
        assert_eq!(e[1].value.as_deref(), Some("shop"));
        assert_eq!(e[2].key, "logging.level.org.hibernate.SQL");
        assert_eq!(e[2].value.as_deref(), Some("debug"));
        assert_eq!(e[3].value.as_deref(), Some("a,b"));
        assert_eq!((e[4].document, e[0].document), (1, 0));
    }

    #[test]
    fn yaml_nesting_lists_blocks_and_documents() {
        let text = "spring:\n  application:\n    name: shop # the name\n  profiles:\n    include:\n    - db\n    - \"cache\"\n  datasource.url: jdbc:h2:mem:test\nserver:\n  port: 8080\n  description: |\n    multi\n    line\nmanagement:\n  endpoints:\n    web:\n      exposure:\n        include: health,beans\nlist:\n  - name: a\n    port: 1\n  - name: b\n---\nspring:\n  config:\n    activate:\n      on-profile: dev\n";
        let e = parse_yaml(text);
        let keys: Vec<(&str, Option<&str>)> = e.iter().map(|x| (x.key.as_str(), x.value.as_deref())).collect();
        assert!(keys.contains(&("spring.application.name", Some("shop"))));
        assert!(keys.contains(&("spring.profiles.include[0]", Some("db"))));
        assert!(keys.contains(&("spring.profiles.include[1]", Some("cache"))));
        assert!(keys.contains(&("spring.datasource.url", Some("jdbc:h2:mem:test"))));
        assert!(keys.contains(&("server.port", Some("8080"))));
        assert!(keys.contains(&("server.description", Some(""))));
        assert!(keys.contains(&("management.endpoints.web.exposure.include", Some("health,beans"))));
        assert!(keys.contains(&("list[0].name", Some("a"))));
        assert!(keys.contains(&("list[0].port", Some("1"))));
        assert!(keys.contains(&("list[1].name", Some("b"))));
        assert!(!keys.iter().any(|(k, _)| k.contains("multi")));
        let dev = e.iter().find(|x| x.key == "spring.config.activate.on-profile").unwrap();
        assert_eq!((dev.document, dev.value.as_deref()), (1, Some("dev")));
        let port = e.iter().find(|x| x.key == "server.port").unwrap();
        assert_eq!(&text[port.key_from..port.key_to], "port");
        assert_eq!(&text[port.value_from..port.value_to], "8080");
    }

    #[test]
    fn cursor_places() {
        let props = "server.port=80\nspring.jpa.sh";
        assert_eq!(
            place_at(Format::Properties, props, props.len()),
            Some(Place::Key { parent: String::new(), typed: "spring.jpa.sh".into(), from: 15, to: props.len(), indent: 0 })
        );
        match place_at(Format::Properties, props, 13).unwrap() {
            Place::Value { key, typed, .. } => assert_eq!((key.as_str(), typed.as_str()), ("server.port", "8")),
            other => panic!("{other:?}"),
        }
        let yaml = "spring:\n  jpa:\n    show\nserver:\n  port: 8\n";
        let at = yaml.find("show").unwrap() + 4;
        match place_at(Format::Yaml, yaml, at).unwrap() {
            Place::Key { parent, typed, indent, .. } => {
                assert_eq!((parent.as_str(), typed.as_str(), indent), ("spring.jpa", "show", 4))
            }
            other => panic!("{other:?}"),
        }
        let at = yaml.find("port: 8").unwrap() + 7;
        match place_at(Format::Yaml, yaml, at).unwrap() {
            Place::Value { key, typed, .. } => assert_eq!((key.as_str(), typed.as_str()), ("server.port", "8")),
            other => panic!("{other:?}"),
        }
        let root = "spring:\n  main:\n    banner-mode: off\nser";
        match place_at(Format::Yaml, root, root.len()).unwrap() {
            Place::Key { parent, typed, .. } => assert_eq!((parent.as_str(), typed.as_str()), ("", "ser")),
            other => panic!("{other:?}"),
        }
        let empty_value = "spring:\n  main:\n    banner-mode: ";
        match place_at(Format::Yaml, empty_value, empty_value.len()).unwrap() {
            Place::Value { key, typed, .. } => assert_eq!((key.as_str(), typed.as_str()), ("spring.main.banner-mode", "")),
            other => panic!("{other:?}"),
        }
        let item = "spring:\n  profiles:\n    active:\n      - d";
        match place_at(Format::Yaml, item, item.len()).unwrap() {
            Place::Value { key, typed, .. } => assert_eq!((key.as_str(), typed.as_str()), ("spring.profiles.active", "d")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn relaxed_binding() {
        assert_eq!(canonical("spring.jpa.showSql"), "spring.jpa.show-sql");
        assert_eq!(canonical("spring.jpa.show_sql"), "spring.jpa.show-sql");
        assert_eq!(canonical("SPRING_JPA_SHOW_SQL").replace('-', "."), "spring.jpa.show.sql");
        assert_eq!(canonical("my.list[0].name"), "my.list.name");
        assert_eq!(canonical("my.map[Some.Key]"), "my.map.some.key");
        assert_eq!(canonical("logging.level.org.hibernate.SQL"), "logging.level.org.hibernate.sql");
        assert_eq!(kebab("maxPoolSize"), "max-pool-size");
    }
}
