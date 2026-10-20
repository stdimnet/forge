//! A tiny DOM for DBGp messages. Responses are small, so building a tree is simpler than
//! streaming and keeps the typed parsers in `protocol.rs` readable.

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use crate::error::{DebugError, DebugResult};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Element {
    /// Qualified name as written, e.g. `xdebug:message`.
    pub name: String,
    pub attrs: Vec<(String, String)>,
    /// Concatenated text and CDATA content (entities resolved).
    pub text: String,
    pub children: Vec<Element>,
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

impl Element {
    /// Name without namespace prefix.
    pub fn local_name(&self) -> &str {
        local(&self.name)
    }

    /// Attribute by qualified or local name.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .or_else(|| self.attrs.iter().find(|(k, _)| local(k) == name))
            .map(|(_, v)| v.as_str())
    }

    pub fn attr_u32(&self, name: &str) -> Option<u32> {
        self.attr(name).and_then(|v| v.trim().parse().ok())
    }

    /// First child with this local name.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.local_name() == name)
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.local_name() == name)
    }
}

fn element_from(start: &BytesStart<'_>) -> DebugResult<Element> {
    let mut el = Element {
        name: start.name().as_ref().to_string(),
        ..Default::default()
    };
    for attr in start.attributes() {
        let attr = attr.map_err(|e| DebugError::Xml(e.to_string()))?;
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|e| DebugError::Xml(e.to_string()))?;
        el.attrs
            .push((attr.key.as_ref().to_string(), value.into_owned()));
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

/// Parse one XML document into its root element.
pub fn parse(xml: &str) -> DebugResult<Element> {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let push_done = |stack: &mut Vec<Element>, root: &mut Option<Element>, el: Element| match stack
        .last_mut()
    {
        Some(parent) => parent.children.push(el),
        None => {
            if root.is_none() {
                *root = Some(el);
            }
        }
    };
    loop {
        let event = reader
            .read_event()
            .map_err(|e| DebugError::Xml(e.to_string()))?;
        match event {
            Event::Start(s) => stack.push(element_from(&s)?),
            Event::Empty(s) => {
                let el = element_from(&s)?;
                push_done(&mut stack, &mut root, el);
            }
            Event::End(_) => {
                let el = stack
                    .pop()
                    .ok_or_else(|| DebugError::Xml("unbalanced end tag".into()))?;
                push_done(&mut stack, &mut root, el);
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
                        .map_err(|e| DebugError::Xml(e.to_string()))?
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
    root.ok_or_else(|| DebugError::Xml("empty document".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_attributes_children_text_and_entities() {
        let el = parse(
            r#"<?xml version="1.0" encoding="iso-8859-1"?>
<response xmlns="urn:debugger_protocol_v1" xmlns:xdebug="https://xdebug.org/dbgp/xdebug" command="eval" transaction_id="7"><property type="string" size="5" encoding="base64"><![CDATA[aGVsbG8=]]></property><xdebug:message filename="file:///a&amp;b.php" lineno="3">a &lt; b &#x41;</xdebug:message></response>"#,
        )
        .unwrap();
        assert_eq!(el.name, "response");
        assert_eq!(el.attr_u32("transaction_id"), Some(7));
        assert_eq!(el.child("property").unwrap().text, "aGVsbG8=");
        let msg = el.child("message").unwrap();
        assert_eq!(msg.name, "xdebug:message");
        assert_eq!(msg.attr("filename"), Some("file:///a&b.php"));
        assert_eq!(msg.text, "a < b A");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("").is_err());
        assert!(parse("<a><b></a>").is_err());
    }
}
