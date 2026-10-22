//! The names a piece of JavaScript/TypeScript or JSX reads from its scope (Extract
//! Component's props): a lexical, best-effort scan. Property names (`a.b`), object keys
//! (`{ key: v }`), keywords and well-known globals are not reads; parameters of arrow
//! functions and functions inside the code are bound. JSX inside expressions is understood
//! (`items.map(item => <li key={item.id}>{item.name}</li>)`): tag names are components the
//! code uses, attribute values and `{…}` children are expressions.

use std::collections::HashSet;

use crate::js::{ident, matching};
use crate::text::is_ident_char;

/// What a scan found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Names {
    /// Identifiers read, in order of first appearance.
    pub reads: Vec<String>,
    /// Names bound inside the code (function parameters).
    pub bound: HashSet<String>,
    /// Capitalized JSX tag names (`<Badge>`, `<Foo.Bar>` as `Foo`).
    pub components: Vec<String>,
}

impl Names {
    fn read(&mut self, name: &str) {
        if !self.reads.iter().any(|r| r == name) {
            self.reads.push(name.to_string());
        }
    }

    /// The names the code takes from outside: reads that are not bound, keywords or globals.
    pub fn free(&self) -> Vec<String> {
        self.reads
            .iter()
            .filter(|r| !self.bound.contains(*r) && !is_reserved(r))
            .cloned()
            .collect()
    }
}

/// Keywords, literals and globals that never become props.
pub fn is_reserved(name: &str) -> bool {
    matches!(
        name,
        "true"
            | "false"
            | "null"
            | "undefined"
            | "this"
            | "new"
            | "typeof"
            | "instanceof"
            | "in"
            | "of"
            | "void"
            | "delete"
            | "await"
            | "async"
            | "return"
            | "if"
            | "else"
            | "const"
            | "let"
            | "var"
            | "function"
            | "class"
            | "extends"
            | "super"
            | "import"
            | "export"
            | "default"
            | "as"
            | "satisfies"
            | "keyof"
            | "yield"
            | "NaN"
            | "Infinity"
            | "console"
            | "Math"
            | "JSON"
            | "Object"
            | "Array"
            | "String"
            | "Number"
            | "Boolean"
            | "Date"
            | "Promise"
            | "Set"
            | "Map"
            | "Symbol"
            | "Error"
            | "RegExp"
            | "Intl"
            | "window"
            | "document"
            | "globalThis"
            | "parseInt"
            | "parseFloat"
            | "isNaN"
            | "encodeURIComponent"
            | "decodeURIComponent"
            | "setTimeout"
            | "clearTimeout"
            | "fetch"
            | "alert"
    )
}

/// The last non-blank character before `i` (within `from..`).
fn prev_char(text: &str, from: usize, i: usize) -> Option<char> {
    text[from..i].trim_end().chars().last()
}

fn next_char(text: &str, i: usize, end: usize) -> Option<char> {
    text[i..end].trim_start().chars().next()
}

/// A `<` at `i` starts a JSX element (where an expression may start).
fn jsx_starts(text: &str, from: usize, i: usize) -> bool {
    let next = text[i + 1..].chars().next();
    if !next.is_some_and(|c| c.is_alphabetic() || c == '>') {
        return false;
    }
    let before = text[from..i].trim_end();
    match before.chars().last() {
        None => true,
        Some(c) if c.is_alphanumeric() || c == '_' || c == '$' => {
            ["return", "yield", "default", "await"].iter().any(|kw| {
                before.ends_with(kw)
                    && !before[..before.len() - kw.len()]
                        .chars()
                        .last()
                        .is_some_and(is_ident_char)
            })
        }
        Some(c) => matches!(
            c,
            '(' | ',' | '=' | ':' | '?' | '&' | '|' | '{' | '[' | '!' | '>'
        ),
    }
}

/// Offset after the string literal at `i` (quotes `'` / `"`).
fn skip_quoted(text: &str, i: usize) -> usize {
    let bytes = text.as_bytes();
    let q = bytes[i];
    let mut j = i + 1;
    while j < bytes.len() && bytes[j] != q {
        if bytes[j] == b'\\' {
            j += 1;
        }
        j += 1;
    }
    (j + 1).min(bytes.len())
}

/// Scan JavaScript code in `start..end`; with `until_brace`, stop at the `}` that closes an
/// enclosing `{` (returns its offset). Returns where the scan stopped.
pub fn scan_code(
    text: &str,
    start: usize,
    end: usize,
    until_brace: bool,
    names: &mut Names,
) -> usize {
    let bytes = text.as_bytes();
    let end = end.min(text.len());
    let mut depth: i32 = 0;
    // Open `(` offsets, to find a parameter list before `=>`.
    let mut parens: Vec<usize> = Vec::new();
    let mut last_group: Option<(usize, usize)> = None;
    let mut last_ident: Option<(usize, usize)> = None;
    let mut i = start;
    while i < end {
        let c = bytes[i];
        match c {
            b'\'' | b'"' => {
                i = skip_quoted(text, i);
                continue;
            }
            b'`' => {
                let mut j = i + 1;
                while j < end && bytes[j] != b'`' {
                    if bytes[j] == b'\\' {
                        j += 2;
                        continue;
                    }
                    if bytes[j] == b'$' && bytes.get(j + 1) == Some(&b'{') {
                        j = scan_code(text, j + 2, end, true, names) + 1;
                        continue;
                    }
                    j += 1;
                }
                i = j + 1;
                continue;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                i = text[i..end].find('\n').map_or(end, |e| i + e);
                continue;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = text[i + 2..end].find("*/").map_or(end, |e| i + e + 4);
                continue;
            }
            b'<' if jsx_starts(text, start, i) => {
                i = scan_jsx_element(text, i, end, names).unwrap_or(i + 1);
                continue;
            }
            b'(' => {
                parens.push(i);
                depth += 1;
            }
            b')' => {
                if let Some(open) = parens.pop() {
                    last_group = Some((open, i));
                }
                depth -= 1;
            }
            b'{' | b'[' => depth += 1,
            b'}' if until_brace && depth == 0 => return i,
            b'}' | b']' => depth -= 1,
            b'=' if bytes.get(i + 1) == Some(&b'>') => {
                // The parameters of an arrow function are bound.
                let before = prev_char(text, start, i);
                if before == Some(')') {
                    if let Some((open, close)) = last_group {
                        bind_all(text, open + 1, close, names);
                    }
                } else if let Some((s, e)) = last_ident {
                    names.bound.insert(text[s..e].to_string());
                }
                i += 2;
                continue;
            }
            _ => {}
        }
        let ch = text[i..].chars().next().unwrap_or(' ');
        if ch.is_alphabetic() || ch == '_' || ch == '$' {
            let (s, e) = ident(text, i).unwrap_or((i, i + ch.len_utf8()));
            let name = &text[s..e];
            let before = prev_char(text, start, s);
            let property = before == Some('.') && !text[start..s].trim_end().ends_with("...");
            let key = next_char(text, e, end) == Some(':')
                && matches!(before, Some('{') | Some(','))
                && !text[e..end].trim_start().starts_with("::");
            if name == "function" {
                // `function name(a, b) {`: the parameters are bound.
                let mut j = e;
                if let Some((_, ne)) = ident(
                    text,
                    text[j..end]
                        .find(|c: char| !c.is_whitespace())
                        .map_or(j, |k| j + k),
                ) {
                    j = ne;
                }
                if let Some(open) = text[j..end].find('(').map(|k| j + k) {
                    if let Some(close) = matching(text, open) {
                        bind_all(text, open + 1, close, names);
                    }
                }
            } else if !property && !key {
                names.read(name);
            }
            last_ident = Some((s, e));
            i = e;
            continue;
        }
        if c.is_ascii_digit() {
            while i < end
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.' || bytes[i] == b'_')
            {
                i += 1;
            }
            continue;
        }
        if !c.is_ascii_whitespace() {
            last_ident = None;
        }
        i += 1;
    }
    end
}

/// Every identifier in `start..end` (a parameter list) is bound, except default values'
/// and types' names after `=` / `:` at the top level.
fn bind_all(text: &str, start: usize, end: usize, names: &mut Names) {
    let mut i = start;
    let mut skip = false;
    let mut depth = 0;
    while i < end {
        let ch = text[i..].chars().next().unwrap_or(' ');
        match ch {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => depth -= 1,
            ',' if depth <= 1 => skip = false,
            '=' | ':' if depth == 0 => skip = true,
            _ => {}
        }
        if (ch.is_alphabetic() || ch == '_' || ch == '$') && !skip {
            let (s, e) = ident(text, i).unwrap_or((i, i + ch.len_utf8()));
            // `{ a: b }` destructuring binds `b`, not the key `a`.
            if next_char(text, e, end) != Some(':') {
                names.bound.insert(text[s..e].to_string());
            }
            i = e;
            continue;
        }
        i += ch.len_utf8();
    }
}

/// Scan the JSX element starting with `<` at `lt`; returns the offset after it, or `None`
/// when it is not a well-formed element before `end`.
pub fn scan_jsx_element(text: &str, lt: usize, end: usize, names: &mut Names) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = lt + 1;
    // The tag name (empty for a fragment `<>`).
    let name_start = i;
    while i < end
        && (is_ident_char(text[i..].chars().next()?) || matches!(bytes[i], b'.' | b'-' | b':'))
    {
        i += text[i..].chars().next()?.len_utf8();
    }
    let name = &text[name_start..i];
    if name.chars().next().is_some_and(|c| c.is_uppercase()) {
        let first = name.split('.').next().unwrap_or(name).to_string();
        names.read(&first);
        if !names.components.contains(&first) {
            names.components.push(first);
        }
    } else if let Some(obj) = name.split_once('.').map(|(o, _)| o) {
        names.read(obj);
    }
    // Attributes.
    loop {
        while i < end && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= end {
            return None;
        }
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'>') => return Some(i + 2),
            b'>' => {
                i += 1;
                break;
            }
            b'{' => {
                // `{...props}`
                let close = scan_code(text, i + 1, end, true, names);
                if close >= end {
                    return None;
                }
                i = close + 1;
            }
            _ => {
                let s = i;
                while i < end
                    && !bytes[i].is_ascii_whitespace()
                    && !matches!(bytes[i], b'=' | b'>' | b'/')
                {
                    i += 1;
                }
                if i == s {
                    return None;
                }
                if bytes.get(i) == Some(&b'=') {
                    i += 1;
                    match bytes.get(i) {
                        Some(b'"') | Some(b'\'') => i = skip_quoted(text, i),
                        Some(b'{') => {
                            let close = scan_code(text, i + 1, end, true, names);
                            if close >= end {
                                return None;
                            }
                            i = close + 1;
                        }
                        _ => return None,
                    }
                }
            }
        }
    }
    // Children up to the closing tag.
    loop {
        if i >= end {
            return None;
        }
        match bytes[i] {
            b'<' if bytes.get(i + 1) == Some(&b'/') => {
                let close = text[i..end].find('>')? + i;
                let closing = text[i + 2..close].trim();
                if closing != name {
                    return None;
                }
                return Some(close + 1);
            }
            b'<' => i = scan_jsx_element(text, i, end, names)?,
            b'{' => {
                let close = scan_code(text, i + 1, end, true, names);
                if close >= end {
                    return None;
                }
                i = close + 1;
            }
            _ => i += text[i..].chars().next()?.len_utf8(),
        }
    }
}

/// The JSX elements that make up `start..end` exactly (blanks around and between them
/// allowed): their names, or `None` when the range is not whole elements.
pub fn scan_jsx_elements(text: &str, start: usize, end: usize, names: &mut Names) -> Option<usize> {
    let mut i = start;
    let mut count = 0;
    loop {
        while i < end && text.as_bytes()[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= end {
            return (count > 0).then_some(count);
        }
        if text.as_bytes()[i] != b'<' {
            return None;
        }
        i = scan_jsx_element(text, i, end, names)?;
        count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn free(code: &str) -> Vec<String> {
        let mut n = Names::default();
        scan_code(code, 0, code.len(), false, &mut n);
        n.free()
    }

    #[test]
    fn reads_of_expressions() {
        assert_eq!(free("user.name + total * rate"), ["user", "total", "rate"]);
        assert_eq!(free("items.map((item, i) => item.price * i)"), ["items"]);
        assert_eq!(free("list.filter(x => x > min)"), ["list", "min"]);
        assert_eq!(free("{ id: user.id, label }"), ["user", "label"]);
        assert_eq!(
            free("`${first} ${last.name}` + 'x' + \"y\""),
            ["first", "last"]
        );
        assert_eq!(free("ok ? Math.max(a, 1) : null"), ["ok", "a"]);
        assert_eq!(free("[...rows, extra]"), ["rows", "extra"]);
        assert_eq!(free("function (e) { save(e) }"), ["save"]);
        assert_eq!(free("count++"), ["count"]);
    }

    #[test]
    fn jsx_elements() {
        let code = "<ul className=\"list\">\n  {items.map(item => <li key={item.id} onClick={() => select(item)}>{item.name}</li>)}\n  <Badge count={count} {...rest} />\n</ul>";
        let mut n = Names::default();
        assert_eq!(scan_jsx_elements(code, 0, code.len(), &mut n), Some(1));
        assert_eq!(n.free(), ["items", "select", "Badge", "count", "rest"]);
        assert_eq!(n.components, ["Badge"]);
        let two = "<b>{a}</b>\n<i>it's {b}</i>";
        let mut n = Names::default();
        assert_eq!(scan_jsx_elements(two, 0, two.len(), &mut n), Some(2));
        assert_eq!(n.free(), ["a", "b"]);
        let mut n = Names::default();
        assert_eq!(scan_jsx_elements("<b>{a}</i>", 0, 10, &mut n), None);
        assert_eq!(scan_jsx_elements("x <b/>", 0, 6, &mut n), None);
        assert_eq!(scan_jsx_elements("<><b/></>", 0, 9, &mut n), Some(1));
    }
}
