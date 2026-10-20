//! Tags and attributes in HTML templates (Vue `<template>`, Angular templates) and JSX.
//! A lexical scanner, not a full parser: it finds `<Name attr=… {expr}>` and keeps byte
//! ranges, which is what navigation needs.

/// One opening or closing tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub name_start: usize,
    pub name_end: usize,
    pub closing: bool,
    pub attrs: Vec<Attr>,
    /// Offset after `>` (or the end of the scanned range for an unfinished tag).
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    /// As written: `title`, `:title`, `@select`, `[user]`, `(selected)`, `*ngIf`.
    pub name: String,
    pub start: usize,
    pub end: usize,
    /// Byte range of the value without quotes or braces.
    pub value: Option<(usize, usize)>,
}

/// What is under a byte offset inside a tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagHit<'a> {
    Name(&'a Tag),
    Attr(&'a Tag, &'a Attr),
}

/// Scan `text[from..to]` for tags. With `jsx`, a `<` only starts a tag where an expression
/// may start (`a < b` and `useState<User>()` are skipped).
pub fn scan_tags(text: &str, from: usize, to: usize, jsx: bool) -> Vec<Tag> {
    let bytes = text.as_bytes();
    let to = to.min(text.len());
    let mut out = Vec::new();
    let mut i = from;
    while i < to {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if !jsx && text[i..].starts_with("<!--") {
            i = text[i..to].find("-->").map_or(to, |e| i + e + 3);
            continue;
        }
        if jsx && !jsx_tag_position(text, i) {
            i += 1;
            continue;
        }
        match parse_tag(text, i, to, jsx) {
            Some(tag) => {
                i = tag.end.max(i + 1);
                out.push(tag);
            }
            None => i += 1,
        }
    }
    out
}

/// Whether `<` at `at` can start a JSX element: after `(`, `,`, `=`, `{`, `}`, `>`, `?`, `:`,
/// `&&`, `||`, `return` or at the start of a line, but not after an identifier or `)`.
fn jsx_tag_position(text: &str, at: usize) -> bool {
    let before = text[..at].trim_end();
    let Some(prev) = before.chars().last() else {
        return true;
    };
    if prev.is_alphanumeric() || prev == '_' || prev == '$' {
        return ["return", "yield", "default", "await"].iter().any(|kw| {
            before.ends_with(kw)
                && !before[..before.len() - kw.len()]
                    .chars()
                    .last()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.')
        });
    }
    !matches!(prev, ')' | ']' | '.' | '"' | '\'' | '`')
}

fn is_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b':' | b'$')
}

fn parse_tag(text: &str, lt: usize, to: usize, jsx: bool) -> Option<Tag> {
    let bytes = text.as_bytes();
    let mut i = lt + 1;
    let closing = bytes.get(i) == Some(&b'/');
    if closing {
        i += 1;
    }
    if !bytes.get(i)?.is_ascii_alphabetic() {
        return None;
    }
    let name_start = i;
    while i < to && is_name_char(bytes[i]) {
        i += 1;
    }
    let name_end = i;
    let name = text[name_start..name_end].to_string();
    let mut attrs = Vec::new();
    loop {
        while i < to && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= to {
            break;
        }
        match bytes[i] {
            b'>' => {
                i += 1;
                break;
            }
            b'/' if bytes.get(i + 1) == Some(&b'>') => {
                i += 2;
                break;
            }
            b'<' => break,
            b'{' if jsx => {
                // `{...props}` spread.
                i = skip_braces(text, i, to);
            }
            _ => {
                let start = i;
                while i < to
                    && !bytes[i].is_ascii_whitespace()
                    && !matches!(bytes[i], b'=' | b'>' | b'{' | b'"' | b'\'')
                    && !(bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'>'))
                {
                    i += 1;
                }
                if i == start {
                    i += 1;
                    continue;
                }
                let end = i;
                let mut value = None;
                let mut j = i;
                while j < to && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if bytes.get(j) == Some(&b'=') {
                    j += 1;
                    while j < to && bytes[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    match bytes.get(j) {
                        Some(&q @ (b'"' | b'\'')) => {
                            let close = text[j + 1..to].find(q as char).map_or(to, |e| j + 1 + e);
                            value = Some((j + 1, close));
                            j = (close + 1).min(to);
                        }
                        Some(b'{') if jsx => {
                            let close = skip_braces(text, j, to);
                            value = Some((j + 1, close.saturating_sub(1).max(j + 1)));
                            j = close;
                        }
                        _ => {
                            let vs = j;
                            while j < to && !bytes[j].is_ascii_whitespace() && bytes[j] != b'>' {
                                j += 1;
                            }
                            value = Some((vs, j));
                        }
                    }
                    i = j;
                }
                attrs.push(Attr {
                    name: text[start..end].to_string(),
                    start,
                    end,
                    value,
                });
            }
        }
    }
    Some(Tag {
        name,
        name_start,
        name_end,
        closing,
        attrs,
        end: i.min(to),
    })
}

/// Offset after the `}` matching the `{` at `open` (strings and template literals skipped).
pub fn skip_braces(text: &str, open: usize, to: usize) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    while i < to {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + 1;
                }
            }
            q @ (b'"' | b'\'' | b'`') => {
                i += 1;
                while i < to && bytes[i] != q {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    to
}

/// The tag name or attribute name under `byte` (the end of a name counts too).
pub fn hit(tags: &[Tag], byte: usize) -> Option<TagHit<'_>> {
    for tag in tags {
        if byte >= tag.name_start && byte <= tag.name_end {
            return Some(TagHit::Name(tag));
        }
        if let Some(a) = tag.attrs.iter().find(|a| byte >= a.start && byte <= a.end) {
            return Some(TagHit::Attr(tag, a));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_tags_with_framework_attributes() {
        let src = r#"<!-- <ignored> --><user-card :user="u" @select="pick" [item]='x' (saved)="s()" disabled></user-card>"#;
        let tags = scan_tags(src, 0, src.len(), false);
        assert_eq!(tags.len(), 2);
        let names: Vec<_> = tags[0].attrs.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, [":user", "@select", "[item]", "(saved)", "disabled"]);
        let (s, e) = tags[0].attrs[0].value.unwrap();
        assert_eq!(&src[s..e], "u");
        assert!(tags[1].closing);
    }

    #[test]
    fn jsx_tags_skip_comparisons_and_generics() {
        let src = "const [u] = useState<User>(null);\nif (a < b) {}\nreturn (\n  <Layout title={`Hi ${n}`} {...rest}>\n    <Button onClick={() => go({ a: 1 })} />\n  </Layout>\n);";
        let tags = scan_tags(src, 0, src.len(), true);
        let names: Vec<_> = tags.iter().map(|t| (t.name.as_str(), t.closing)).collect();
        assert_eq!(
            names,
            [("Layout", false), ("Button", false), ("Layout", true)]
        );
        assert_eq!(tags[0].attrs[0].name, "title");
        assert_eq!(tags[1].attrs[0].name, "onClick");
    }

    #[test]
    fn hit_finds_names_and_attributes() {
        let src = "<UserCard name=\"x\" />";
        let tags = scan_tags(src, 0, src.len(), true);
        assert!(matches!(hit(&tags, 3), Some(TagHit::Name(_))));
        assert!(matches!(hit(&tags, 11), Some(TagHit::Attr(_, a)) if a.name == "name"));
        assert!(hit(&tags, 16).is_none());
    }
}
