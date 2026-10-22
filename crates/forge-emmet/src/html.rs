//! A tolerant HTML scanner: tags with their attributes, element pairs, and the state at a
//! position (text, inside a tag, comment, script, style). Comments, `<?php ?>` blocks and
//! the raw content of `<script>` / `<style>` are skipped.

use crate::snippets::is_void;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    /// Byte range of the whole tag (`<` .. after `>`).
    pub from: usize,
    pub to: usize,
    pub closing: bool,
    pub self_closing: bool,
}

/// What surrounds a position in an HTML-like document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Text,
    /// Inside `<…>`.
    Tag,
    Comment,
    /// Inside `<?php … ?>`.
    Php,
    Script,
    /// Inside `<style>`; the content starts at the byte.
    Style(usize),
}

/// End of a tag that starts at `from` (after its `>`), honoring quoted attribute values.
fn tag_end(text: &str, from: usize) -> Option<usize> {
    let b = text.as_bytes();
    let mut quote: Option<u8> = None;
    let mut i = from + 1;
    while i < b.len() {
        let c = b[i];
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == b'"' || c == b'\'' => quote = Some(c),
            None if c == b'>' => return Some(i + 1),
            None if c == b'<' => return None,
            None => {}
        }
        i += 1;
    }
    None
}

fn tag_name(text: &str, from: usize) -> (String, bool) {
    let rest = &text[from + 1..];
    let closing = rest.starts_with('/');
    let rest = rest.trim_start_matches('/');
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ':' | '.'))
        .collect();
    (name, closing)
}

fn starts_tag(b: &[u8], i: usize) -> bool {
    b[i] == b'<'
        && b.get(i + 1)
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'/')
}

/// Scan up to `limit`: the tags before it and the state at `limit`.
fn scan(text: &str, limit: usize) -> (Vec<Tag>, State) {
    let b = text.as_bytes();
    let limit = limit.min(b.len());
    let mut tags = Vec::new();
    let mut i = 0;
    while i < limit {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        if text[i..].starts_with("<!--") {
            match text[i + 4..].find("-->") {
                Some(e) if i + 4 + e + 3 <= limit => i = i + 4 + e + 3,
                _ => return (tags, State::Comment),
            }
            continue;
        }
        if text[i..].starts_with("<?") {
            match text[i + 2..].find("?>") {
                Some(e) if i + 2 + e + 2 <= limit => i = i + 2 + e + 2,
                _ => return (tags, State::Php),
            }
            continue;
        }
        if text[i..].starts_with("<!") {
            match text[i..].find('>') {
                Some(e) if i + e < limit => i += e + 1,
                _ => return (tags, State::Tag),
            }
            continue;
        }
        if starts_tag(b, i) {
            let Some(end) = tag_end(text, i).filter(|e| *e <= limit) else {
                return (tags, State::Tag);
            };
            let (name, closing) = tag_name(text, i);
            let self_closing = text[..end].ends_with("/>");
            tags.push(Tag {
                name: name.clone(),
                from: i,
                to: end,
                closing,
                self_closing,
            });
            i = end;
            let lower = name.to_ascii_lowercase();
            if !closing && !self_closing && (lower == "script" || lower == "style") {
                let close = format!("</{lower}");
                let found = text[end..].to_ascii_lowercase().find(&close);
                match found {
                    Some(e) if end + e < limit => i = end + e,
                    _ if lower == "script" => return (tags, State::Script),
                    _ => return (tags, State::Style(end)),
                }
            }
            continue;
        }
        i += 1;
    }
    (tags, State::Text)
}

/// The state at `byte`.
pub fn state_at(text: &str, byte: usize) -> State {
    scan(text, byte).1
}

/// All tags of the document.
pub fn tags(text: &str) -> Vec<Tag> {
    scan(text, text.len()).0
}

/// An element: its opening tag and, unless void or self-closing, its closing tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    pub open: (usize, usize),
    pub close: Option<(usize, usize)>,
}

impl Element {
    pub fn end(&self) -> usize {
        self.close.map_or(self.open.1, |c| c.1)
    }
}

/// Elements paired from the tags (unclosed elements are dropped, stray closing tags ignored).
pub fn elements(text: &str) -> Vec<Element> {
    let mut out = Vec::new();
    let mut stack: Vec<&Tag> = Vec::new();
    let all = tags(text);
    for t in &all {
        if t.closing {
            if let Some(pos) = stack
                .iter()
                .rposition(|o| o.name.eq_ignore_ascii_case(&t.name))
            {
                let open = stack[pos];
                stack.truncate(pos);
                out.push(Element {
                    name: open.name.clone(),
                    open: (open.from, open.to),
                    close: Some((t.from, t.to)),
                });
            }
        } else if t.self_closing || is_void(&t.name) {
            out.push(Element {
                name: t.name.clone(),
                open: (t.from, t.to),
                close: None,
            });
        } else {
            stack.push(t);
        }
    }
    out.sort_by_key(|e| e.open.0);
    out
}

/// The innermost element around `byte`.
pub fn element_at(text: &str, byte: usize) -> Option<Element> {
    elements(text)
        .into_iter()
        .filter(|e| e.open.0 <= byte && byte < e.end())
        .min_by_key(|e| e.end() - e.open.0)
}

/// An attribute of an opening tag: name range and value range (inside the quotes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagAttr {
    pub name: String,
    pub name_range: (usize, usize),
    pub value: Option<(usize, usize)>,
}

/// Attributes of the opening tag at `from..to` (byte offsets in `text`).
pub fn attributes(text: &str, from: usize, to: usize) -> Vec<TagAttr> {
    let b = text.as_bytes();
    let (name, _) = tag_name(text, from);
    let mut i = from + 1 + name.len();
    let end = to.saturating_sub(1);
    let mut out = Vec::new();
    while i < end {
        while i < end && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        let s = i;
        while i < end && !b[i].is_ascii_whitespace() && b[i] != b'=' && b[i] != b'>' && b[i] != b'/'
        {
            i += 1;
        }
        if s == i {
            i += 1;
            continue;
        }
        let mut attr = TagAttr {
            name: text[s..i].to_string(),
            name_range: (s, i),
            value: None,
        };
        if i < end && b[i] == b'=' {
            i += 1;
            if i < end && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                let vs = i + 1;
                i = vs;
                while i < end && b[i] != q {
                    i += 1;
                }
                attr.value = Some((vs, i));
                i += 1;
            } else {
                let vs = i;
                while i < end && !b[i].is_ascii_whitespace() && b[i] != b'>' {
                    i += 1;
                }
                attr.value = Some((vs, i));
            }
        }
        out.push(attr);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states() {
        let t =
            "<div class=\"a\">x<!-- c --><?php $a ?><script>let a = 1;</script><style>p{}</style>";
        assert_eq!(state_at(t, 2), State::Tag);
        assert_eq!(state_at(t, 16), State::Text);
        assert_eq!(state_at(t, 21), State::Comment);
        assert_eq!(state_at(t, 33), State::Php);
        assert_eq!(state_at(t, 50), State::Script);
        let style = t.find("p{}").unwrap();
        assert_eq!(state_at(t, style + 2), State::Style(style));
        assert_eq!(state_at(t, t.len()), State::Text);
        assert_eq!(state_at("<a title=\"x>y\" ", 15), State::Tag);
    }

    #[test]
    fn pairs_elements() {
        let t = "<ul>\n  <li><a href=\"#\">x</a><br></li>\n</ul><img src=x />";
        let els = elements(t);
        let names: Vec<&str> = els.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["ul", "li", "a", "br", "img"]);
        let a = element_at(t, t.find('x').unwrap()).unwrap();
        assert_eq!(a.name, "a");
        let li = element_at(t, t.find("<br>").unwrap() + 4).unwrap();
        assert_eq!(li.name, "li");
        assert!(element_at("<p>x", 3).is_none());
    }

    #[test]
    fn reads_attributes() {
        let t = "<a href=\"/x\" class='b c' data-x=1 hidden>";
        let attrs = attributes(t, 0, t.len());
        let names: Vec<&str> = attrs.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["href", "class", "data-x", "hidden"]);
        let (s, e) = attrs[1].value.unwrap();
        assert_eq!(&t[s..e], "b c");
        let (s, e) = attrs[2].value.unwrap();
        assert_eq!(&t[s..e], "1");
        assert!(attrs[3].value.is_none());
    }
}
