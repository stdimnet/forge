//! A small XML tree on top of quick-xml for Valgrind's XML output and CMSIS-SVD files. Both
//! are read as a whole, so a tree keeps the typed readers simple. Documents cut off at the
//! end (a Valgrind run that was killed) are closed instead of rejected.

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Element {
    /// Local name (namespace prefixes dropped).
    pub name: String,
    pub attrs: Vec<(String, String)>,
    /// Text and CDATA content, entities resolved.
    pub text: String,
    pub children: Vec<Element>,
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// First child with this name.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    /// Trimmed text of the first child with this name.
    pub fn child_text(&self, name: &str) -> Option<&str> {
        self.child(name)
            .map(|c| c.text.trim())
            .filter(|t| !t.is_empty())
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
}

fn element_from(start: &BytesStart<'_>) -> Result<Element, String> {
    let name = start.name().as_ref().to_string();
    let mut el = Element {
        name: local(&name).to_string(),
        ..Default::default()
    };
    for attr in start.attributes() {
        let attr = attr.map_err(|e| e.to_string())?;
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|e| e.to_string())?;
        let key = attr.key.as_ref().to_string();
        el.attrs.push((local(&key).to_string(), value.into_owned()));
    }
    Ok(el)
}

fn predefined_entity(name: &str) -> Option<char> {
    Some(match name {
        "lt" => '<',
        "gt" => '>',
        "amp" => '&',
        "apos" => '\'',
        "quot" => '"',
        _ => return None,
    })
}

fn attach(stack: &mut [Element], root: &mut Option<Element>, el: Element) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(el),
        None => {
            if root.is_none() {
                *root = Some(el);
            }
        }
    }
}

/// Parse a document into its root element.
pub fn parse(xml: &str) -> Result<Element, String> {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    loop {
        let event = match reader.read_event() {
            Ok(e) => e,
            // A truncated document: keep what was read.
            Err(_) if !stack.is_empty() => break,
            Err(e) => return Err(e.to_string()),
        };
        match event {
            Event::Start(s) => stack.push(element_from(&s)?),
            Event::Empty(s) => {
                let el = element_from(&s)?;
                attach(&mut stack, &mut root, el);
            }
            Event::End(_) => {
                let el = stack.pop().ok_or("unbalanced end tag")?;
                attach(&mut stack, &mut root, el);
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&t.xml10_content());
                }
            }
            Event::CData(c) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&c.xml10_content());
                }
            }
            Event::GeneralRef(r) => {
                if let Some(top) = stack.last_mut() {
                    let ch = r
                        .resolve_char_ref()
                        .map_err(|e| e.to_string())?
                        .or_else(|| predefined_entity(&r));
                    match ch {
                        Some(ch) => top.text.push(ch),
                        None => {
                            top.text.push('&');
                            top.text.push_str(&r);
                            top.text.push(';');
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    while let Some(el) = stack.pop() {
        attach(&mut stack, &mut root, el);
    }
    root.ok_or_else(|| "empty document".to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_and_closes_truncated_documents() {
        let el = super::parse("<a x='1'><b>t &amp; u</b><c/><d>cut").unwrap();
        assert_eq!(el.name, "a");
        assert_eq!(el.attr("x"), Some("1"));
        assert_eq!(el.child_text("b"), Some("t & u"));
        assert_eq!(el.child_text("d"), Some("cut"));
        assert!(super::parse("").is_err());
    }
}
