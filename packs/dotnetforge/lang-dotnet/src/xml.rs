//! A small XML tree for the files DotNetForge reads (MSBuild projects, `.slnx` solutions,
//! TRX reports): elements with attributes, text and children. Namespaces are dropped
//! (`<Project xmlns="http://schemas.microsoft.com/developer/msbuild/2003">` reads like
//! `<Project>`); element and attribute names keep their case.

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub text: String,
    pub children: Vec<Element>,
    /// 0-based line of the start tag.
    pub line: u32,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// A trimmed, non-empty attribute.
    pub fn attr_nonempty(&self, name: &str) -> Option<&str> {
        self.attr(name).map(str::trim).filter(|v| !v.is_empty())
    }

    /// Children with this name (case-insensitive, like MSBuild).
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children
            .iter()
            .filter(move |c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// The trimmed text of the element and all elements below it.
    pub fn deep_text(&self) -> String {
        let mut out = self.text.clone();
        for c in &self.children {
            out.push_str(&c.deep_text());
        }
        out
    }
}

fn local(name: &str) -> String {
    match name.rsplit_once(':') {
        Some((_, l)) => l.to_string(),
        None => name.to_string(),
    }
}

fn element_from(start: &BytesStart<'_>, line: u32) -> Result<Element, String> {
    let mut el = Element {
        name: local(start.name().as_ref()),
        line,
        ..Element::default()
    };
    for attr in start.attributes().with_checks(false) {
        let attr = attr.map_err(|e| e.to_string())?;
        let key = local(attr.key.as_ref());
        if key == "xmlns" {
            continue;
        }
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|e| e.to_string())?;
        el.attrs.push((key, value.into_owned()));
    }
    Ok(el)
}

fn entity(name: &str) -> Option<char> {
    Some(match name {
        "lt" => '<',
        "gt" => '>',
        "amp" => '&',
        "apos" => '\'',
        "quot" => '"',
        _ => return None,
    })
}

/// Parse a whole document into its root element.
pub fn parse(xml: &str) -> Result<Element, String> {
    let text = xml.trim_start_matches('\u{feff}');
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let line_at = |pos: u64| -> u32 {
        let end = (pos as usize).min(text.len());
        text.as_bytes()[..end].iter().filter(|b| **b == b'\n').count() as u32
    };
    let done = |stack: &mut Vec<Element>, root: &mut Option<Element>, el: Element| match stack
        .last_mut()
    {
        Some(parent) => parent.children.push(el),
        None => {
            root.get_or_insert(el);
        }
    };
    loop {
        let pos = reader.buffer_position();
        match reader.read_event().map_err(|e| e.to_string())? {
            Event::Start(s) => {
                let line = line_at(pos);
                stack.push(element_from(&s, line)?)
            }
            Event::Empty(s) => {
                let el = element_from(&s, line_at(pos))?;
                done(&mut stack, &mut root, el);
            }
            Event::End(_) => {
                let el = stack.pop().ok_or("unbalanced end tag")?;
                done(&mut stack, &mut root, el);
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
                        .or_else(|| entity(&r));
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
    if !stack.is_empty() {
        return Err(format!("unexpected end of the document (<{}> is not closed)", stack.last().map(|e| e.name.as_str()).unwrap_or("")));
    }
    root.ok_or_else(|| "empty document".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_elements_attributes_and_lines() {
        let doc = parse(
            "\u{feff}<?xml version=\"1.0\"?>\n<Project Sdk=\"Microsoft.NET.Sdk\" xmlns=\"x\">\n  <PropertyGroup>\n    <TargetFramework>net8.0</TargetFramework>\n  </PropertyGroup>\n  <ItemGroup><PackageReference Include=\"A &amp; B\" Version=\"1.0\" /></ItemGroup>\n</Project>",
        )
        .unwrap();
        assert_eq!(doc.name, "Project");
        assert_eq!(doc.attr("sdk"), Some("Microsoft.NET.Sdk"));
        assert!(doc.attr("xmlns").is_none());
        assert_eq!(
            doc.child("propertygroup").unwrap().child("TargetFramework").unwrap().text,
            "net8.0"
        );
        let item = &doc.children_named("ItemGroup").next().unwrap().children[0];
        assert_eq!(item.attr("Include"), Some("A & B"));
        assert_eq!(item.line, 5);
        assert!(parse("<a><b></a>").is_err());
        assert!(parse("").is_err());
    }
}
