//! IntelliJ color scheme files (`.icls`, also inside `.jar` settings exports):
//!
//! ```xml
//! <scheme name="Monokai" version="142" parent_scheme="Darcula">
//!   <colors>
//!     <option name="CARET_COLOR" value="f8f8f0" />
//!   </colors>
//!   <attributes>
//!     <option name="DEFAULT_KEYWORD">
//!       <value>
//!         <option name="FOREGROUND" value="f92672" />
//!         <option name="FONT_TYPE" value="1" />
//!       </value>
//!     </option>
//!     <option name="PHP_KEYWORD" baseAttributes="DEFAULT_KEYWORD" />
//!   </attributes>
//! </scheme>
//! ```
//!
//! Import maps the IntelliJ attributes onto Forge's ([`crate::model::ATTRIBUTES`], first
//! matching name wins; `baseAttributes` references are followed); attributes the file does not
//! set come from the bundled scheme of its parent (Darcula → Ember, anything else → Anvil).

use std::collections::{BTreeMap, HashMap};

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use crate::model::{normalize_color, Bundled, Effect, TextAttributes, ATTRIBUTES};
use crate::scheme::{SchemeSettings, UserScheme};

#[derive(Debug, Default)]
struct Element {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Element>,
}

impl Element {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    /// `<option name="…">` children.
    fn options(&self) -> impl Iterator<Item = (&str, &Element)> {
        self.children
            .iter()
            .filter(|c| c.name == "option")
            .filter_map(|c| Some((c.attr("name")?, c)))
    }
}

fn element_from(start: &BytesStart<'_>) -> Result<Element, String> {
    let mut el = Element {
        name: start.name().as_ref().to_string(),
        ..Element::default()
    };
    for attr in start.attributes() {
        let attr = attr.map_err(|e| e.to_string())?;
        let value = attr
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|e| e.to_string())?;
        el.attrs
            .push((attr.key.as_ref().to_string(), value.into_owned()));
    }
    Ok(el)
}

fn parse_xml(text: &str) -> Result<Element, String> {
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let mut done = |stack: &mut Vec<Element>, el: Element| match stack.last_mut() {
        Some(parent) => parent.children.push(el),
        None => {
            root.get_or_insert(el);
        }
    };
    loop {
        match reader
            .read_event()
            .map_err(|e| format!("not a valid color scheme file: {e}"))?
        {
            Event::Start(s) => stack.push(element_from(&s)?),
            Event::Empty(s) => {
                let el = element_from(&s)?;
                done(&mut stack, el);
            }
            Event::End(_) => {
                let el = stack.pop().ok_or("not a valid color scheme file")?;
                done(&mut stack, el);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err("the color scheme file ends too early".into());
    }
    root.ok_or_else(|| "the file is empty".into())
}

/// The text attributes of one `<option>` of the `<attributes>` section.
fn read_value(option: &Element) -> TextAttributes {
    let mut out = TextAttributes::default();
    let Some(value) = option.child("value") else {
        return out;
    };
    let mut effect_type: Option<u8> = None;
    for (name, o) in value.options() {
        let v = o.attr("value").unwrap_or_default();
        match name {
            "FOREGROUND" => out.foreground = normalize_color(v),
            "BACKGROUND" => out.background = normalize_color(v),
            "EFFECT_COLOR" => out.effect_color = normalize_color(v),
            "EFFECT_TYPE" => effect_type = v.trim().parse().ok(),
            "FONT_TYPE" => {
                let font: u8 = v.trim().parse().unwrap_or(0);
                out.bold = font & 1 != 0;
                out.italic = font & 2 != 0;
            }
            _ => {}
        }
    }
    // IntelliJ's default effect is a border, drawn when there is an effect color.
    if out.effect_color.is_some() {
        out.effect = Effect::from_icls(effect_type.unwrap_or(0));
    }
    out.normalized()
}

/// A parsed .icls file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub scheme: UserScheme,
    /// How many of Forge's attributes the file sets.
    pub applied: usize,
}

/// Read an .icls file into a user scheme (named after the scheme in the file).
pub fn import_icls(text: &str) -> Result<Imported, String> {
    let root = parse_xml(text.trim_start_matches('\u{feff}'))?;
    if root.name != "scheme" {
        return Err("not an IntelliJ color scheme (.icls) file".into());
    }
    let name = root
        .attr("name")
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or("Imported");
    // Drop the "_@user_" prefix of PHPStorm's copies of bundled schemes.
    let name = crate::scheme::clean_name(name.strip_prefix("_@user_").unwrap_or(name));
    let parent = Bundled::from_icls_parent(root.attr("parent_scheme").unwrap_or(""));

    let colors: HashMap<&str, String> = root
        .child("colors")
        .into_iter()
        .flat_map(Element::options)
        .filter_map(|(n, o)| Some((n, normalize_color(o.attr("value")?)?)))
        .collect();
    let options: HashMap<&str, &Element> = root
        .child("attributes")
        .into_iter()
        .flat_map(Element::options)
        .collect();
    // Follow `baseAttributes` (a few steps; cycles end there).
    let resolve = |name: &str| -> Option<TextAttributes> {
        let mut current = *options.get(name)?;
        for _ in 0..8 {
            if current.child("value").is_some() {
                return Some(read_value(current));
            }
            current = *options.get(current.attr("baseAttributes")?)?;
        }
        None
    };

    let mut attributes = BTreeMap::new();
    for spec in ATTRIBUTES {
        let found = spec.icls.iter().find_map(|n| match n.strip_prefix('@') {
            Some(color) => colors.get(color).map(|c| {
                if spec.fields.foreground {
                    TextAttributes::fg(c)
                } else {
                    TextAttributes::bg(c)
                }
            }),
            None => resolve(n),
        });
        let Some(mut value) = found else {
            continue;
        };
        // Keep only what the attribute supports.
        let f = spec.fields;
        if !f.foreground {
            value.foreground = None;
        }
        if !f.background {
            value.background = None;
        }
        if !f.font {
            value.bold = false;
            value.italic = false;
        }
        if !f.effect {
            value.effect = None;
            value.effect_color = None;
        }
        if value != parent.attribute(spec.key) {
            attributes.insert(spec.key.to_string(), value);
        }
    }
    let applied = attributes.len();
    Ok(Imported {
        scheme: UserScheme {
            name,
            parent,
            attributes,
        },
        applied,
    })
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn hex(c: &str) -> String {
    c.trim_start_matches('#').to_string()
}

/// Scheme `name` as an .icls file with every attribute (bundled values included, so it looks
/// the same in PHPStorm whatever the parent scheme there).
pub fn export_icls(settings: &SchemeSettings, name: &str) -> String {
    let base = settings.base_of(name);
    let mut out = format!(
        "<scheme name=\"{}\" version=\"142\" parent_scheme=\"{}\">\n",
        xml_escape(name),
        base.icls_parent()
    );
    out.push_str("  <metaInfo>\n    <property name=\"created\">Forge</property>\n  </metaInfo>\n");
    let mut colors = String::new();
    let mut attrs = String::new();
    for spec in ATTRIBUTES {
        let value = settings.attribute_of(name, spec.key).0;
        let Some(first) = spec.icls.first() else {
            continue;
        };
        if let Some(color_name) = first.strip_prefix('@') {
            if let Some(c) = value.foreground.as_ref().or(value.background.as_ref()) {
                colors.push_str(&format!(
                    "    <option name=\"{color_name}\" value=\"{}\" />\n",
                    hex(c)
                ));
            }
            continue;
        }
        let mut opts: Vec<(&str, String)> = Vec::new();
        if let Some(c) = &value.foreground {
            opts.push(("FOREGROUND", hex(c)));
        }
        if let Some(c) = &value.background {
            opts.push(("BACKGROUND", hex(c)));
        }
        let font = u8::from(value.bold) | (u8::from(value.italic) << 1);
        if font != 0 {
            opts.push(("FONT_TYPE", font.to_string()));
        }
        if let Some(e) = value.effect {
            let color = value
                .effect_color
                .as_ref()
                .or(value.foreground.as_ref())
                .map(|c| hex(c))
                .unwrap_or_else(|| "808080".into());
            opts.push(("EFFECT_COLOR", color));
            opts.push(("EFFECT_TYPE", e.icls_id().to_string()));
        }
        attrs.push_str(&format!("    <option name=\"{first}\">\n      <value>\n"));
        for (k, v) in opts {
            attrs.push_str(&format!("        <option name=\"{k}\" value=\"{v}\" />\n"));
        }
        attrs.push_str("      </value>\n    </option>\n");
    }
    out.push_str("  <colors>\n");
    out.push_str(&colors);
    out.push_str("  </colors>\n  <attributes>\n");
    out.push_str(&attrs);
    out.push_str("  </attributes>\n</scheme>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONOKAI: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<scheme name="Monokai &amp; Co" version="142" parent_scheme="Darcula">
  <colors>
    <option name="CARET_COLOR" value="f8f8f0" />
    <option name="SELECTION_BACKGROUND" value="49483e" />
    <option name="LINE_NUMBERS_COLOR" value="90908a" />
  </colors>
  <attributes>
    <option name="TEXT">
      <value>
        <option name="FOREGROUND" value="f8f8f2" />
        <option name="BACKGROUND" value="272822" />
      </value>
    </option>
    <option name="DEFAULT_KEYWORD">
      <value>
        <option name="FOREGROUND" value="f92672" />
        <option name="FONT_TYPE" value="1" />
      </value>
    </option>
    <option name="DEFAULT_STRING">
      <value>
        <option name="FOREGROUND" value="e6db74" />
        <option name="EFFECT_COLOR" value="ff0000" />
        <option name="EFFECT_TYPE" value="2" />
      </value>
    </option>
    <option name="DEFAULT_LINE_COMMENT">
      <value>
        <option name="FOREGROUND" value="75715e" />
        <option name="FONT_TYPE" value="2" />
      </value>
    </option>
    <option name="PHP_VAR" baseAttributes="DEFAULT_LOCAL_VARIABLE" />
    <option name="DEFAULT_LOCAL_VARIABLE">
      <value>
        <option name="FOREGROUND" value="fd971f" />
      </value>
    </option>
    <option name="DEFAULT_TAG">
      <value>
        <option name="EFFECT_COLOR" value="a6e22e" />
      </value>
    </option>
  </attributes>
</scheme>"#;

    #[test]
    fn imports_an_intellij_scheme() {
        let imported = import_icls(MONOKAI).unwrap();
        let s = &imported.scheme;
        assert_eq!(s.name, "Monokai & Co");
        assert_eq!(s.parent, Bundled::Ember);
        let a = |k: &str| s.attributes.get(k).cloned().unwrap_or_default();
        assert_eq!(a("TEXT").background.as_deref(), Some("#272822"));
        assert_eq!(a("CARET").foreground.as_deref(), Some("#f8f8f0"));
        assert_eq!(a("SELECTION").background.as_deref(), Some("#49483e"));
        assert_eq!(a("LINE_NUMBERS").foreground.as_deref(), Some("#90908a"));
        let kw = a("KEYWORD");
        assert_eq!(kw.foreground.as_deref(), Some("#f92672"));
        assert!(kw.bold && !kw.italic);
        let st = a("STRING");
        assert_eq!(st.effect, Some(Effect::WaveUnderline));
        assert_eq!(st.effect_color.as_deref(), Some("#ff0000"));
        assert!(a("COMMENT").italic);
        // PHP_VAR follows its base attributes.
        assert_eq!(a("VARIABLE").foreground.as_deref(), Some("#fd971f"));
        // An effect color without a type is a border.
        assert_eq!(a("TAG").effect, Some(Effect::Bordered));
        assert_eq!(imported.applied, s.attributes.len());
        assert!(imported.applied >= 9);
    }

    #[test]
    fn maps_light_parents_to_anvil_and_strips_user_prefixes() {
        let s = import_icls(
            r#"<scheme name="_@user_Default" parent_scheme="Default"><attributes/></scheme>"#,
        )
        .unwrap()
        .scheme;
        assert_eq!(s.name, "Default");
        assert_eq!(s.parent, Bundled::Anvil);
        assert!(s.attributes.is_empty());
    }

    #[test]
    fn names_with_tabs_or_line_breaks_are_imported_with_spaces() {
        let s = import_icls(r#"<scheme name="My&#9;Scheme&#10;" parent_scheme="Darcula"/>"#)
            .unwrap()
            .scheme;
        assert_eq!(s.name, "My Scheme");
        // Importing it next to an existing scheme of that name picks a numbered one.
        let settings = SchemeSettings::from_json(&serde_json::json!({
            "editor.colorScheme.schemes": [{ "name": "My Scheme", "parent": "Ember" }]
        }));
        let mut raw = s.clone();
        raw.name = "My\tScheme".into();
        let writes = settings.add_imported(raw);
        assert_eq!(writes[1].value, serde_json::json!("My Scheme 2"));
    }

    #[test]
    fn rejects_other_files() {
        assert!(import_icls("<code_scheme name=\"x\"/>").is_err());
        assert!(import_icls("not xml <").is_err());
        assert!(import_icls("").is_err());
        assert!(import_icls("<scheme><colors>").is_err());
    }

    #[test]
    fn exports_and_imports_the_same_scheme() {
        let imported = import_icls(MONOKAI).unwrap().scheme;
        let settings = SchemeSettings {
            active: imported.name.clone(),
            schemes: vec![imported.clone()],
        };
        let text = export_icls(&settings, &imported.name);
        assert!(text.starts_with(
            "<scheme name=\"Monokai &amp; Co\" version=\"142\" parent_scheme=\"Darcula\">"
        ));
        assert!(text.contains("<option name=\"CARET_COLOR\" value=\"f8f8f0\" />"));
        assert!(text.contains("<option name=\"FONT_TYPE\" value=\"1\" />"));
        let again = import_icls(&text).unwrap().scheme;
        assert_eq!(again, imported);
        // A bundled scheme exports with every attribute and comes back unchanged.
        let anvil = export_icls(&SchemeSettings::default(), "Anvil");
        let back = import_icls(&anvil).unwrap().scheme;
        assert_eq!(back.parent, Bundled::Anvil);
        assert!(back.attributes.is_empty(), "{:?}", back.attributes);
    }
}
