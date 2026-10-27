//! Profiles as PHPStorm inspection profile XML (`.idea/inspectionProfiles/*.xml`, the format
//! Forge stores project profiles in and the command-line inspector reads) and as Forge
//! JSON.
//!
//! ```xml
//! <component name="InspectionProjectProfileManager">
//!   <profile version="1.0">
//!     <option name="myName" value="Project Default" />
//!     <inspection_tool class="PhpUnusedAssignment" enabled="true" level="WEAK WARNING" enabled_by_default="true">
//!       <scope name="Tests" level="WARNING" enabled="false" />
//!       <option name="reportParams" value="true" />
//!     </inspection_tool>
//!   </profile>
//! </component>
//! ```

use std::collections::BTreeMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::profile::{Profile, ProfileLevel, ScopeSettings, ToolSettings};
use crate::registry::Registry;
use crate::severity;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfileFormat {
    Xml,
    Json,
}

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

    fn find(&self, name: &str) -> Option<&Element> {
        if self.name == name {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(name))
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
            .map_err(|e| format!("not a valid inspection profile: {e}"))?
        {
            Event::Start(s) => stack.push(element_from(&s)?),
            Event::Empty(s) => {
                let el = element_from(&s)?;
                done(&mut stack, el);
            }
            Event::End(_) => {
                let el = stack.pop().ok_or("not a valid inspection profile")?;
                done(&mut stack, el);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    root.ok_or_else(|| "the file is empty".into())
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            c => out.push(c),
        }
    }
    out
}

fn option_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The profile as PHPStorm XML. Inspections the profile does not change are left out
/// (as PHPStorm does); `registry` supplies the defaults the attributes need.
pub fn to_xml(profile: &Profile, registry: &Registry) -> String {
    let component = match profile.level {
        ProfileLevel::Project => "InspectionProjectProfileManager",
        ProfileLevel::Ide => "InspectionProfileManager",
    };
    let mut out = format!(
        "<component name=\"{component}\">\n  <profile version=\"1.0\">\n    <option name=\"myName\" value=\"{}\" />\n",
        xml_escape(&profile.name)
    );
    for (id, t) in &profile.tools {
        let spec = registry.get(id);
        let base = t.enabled.unwrap_or_else(|| spec.is_none_or(|s| s.enabled));
        let anywhere = base || t.scopes.iter().any(|s| s.enabled);
        let level = t
            .severity
            .clone()
            .or_else(|| spec.and_then(|s| (!s.external).then(|| s.severity.clone())));
        out.push_str(&format!(
            "    <inspection_tool class=\"{}\" enabled=\"{anywhere}\"",
            xml_escape(id)
        ));
        if let Some(l) = &level {
            out.push_str(&format!(" level=\"{}\"", xml_escape(l)));
        }
        out.push_str(&format!(" enabled_by_default=\"{base}\""));
        if t.scopes.is_empty() && t.options.is_empty() {
            out.push_str(" />\n");
            continue;
        }
        out.push_str(">\n");
        for s in &t.scopes {
            out.push_str(&format!("      <scope name=\"{}\"", xml_escape(&s.scope)));
            if let Some(l) = s.severity.as_ref().or(level.as_ref()) {
                out.push_str(&format!(" level=\"{}\"", xml_escape(l)));
            }
            out.push_str(&format!(" enabled=\"{}\" />\n", s.enabled));
        }
        for (k, v) in &t.options {
            out.push_str(&format!(
                "      <option name=\"{}\" value=\"{}\" />\n",
                xml_escape(k),
                xml_escape(&option_text(v))
            ));
        }
        out.push_str("    </inspection_tool>\n");
    }
    out.push_str("  </profile>\n</component>\n");
    out
}

fn bool_attr(el: &Element, name: &str) -> Option<bool> {
    el.attr(name).map(|v| v.eq_ignore_ascii_case("true"))
}

/// A profile from PHPStorm XML (a profile file, or a `component` around it). Options of
/// inspections Forge knows are typed by their specs; the rest stay strings.
pub fn from_xml(text: &str, registry: &Registry) -> Result<Profile, String> {
    let root = parse_xml(text)?;
    let profile = root
        .find("profile")
        .ok_or("not an inspection profile (no <profile> element)")?;
    let name = profile
        .children
        .iter()
        .find(|c| c.name == "option" && c.attr("name") == Some("myName"))
        .and_then(|c| c.attr("value"))
        .unwrap_or("Imported")
        .to_string();
    let level = match root.attr("name") {
        Some("InspectionProjectProfileManager") => ProfileLevel::Project,
        _ => ProfileLevel::Ide,
    };
    let mut tools = BTreeMap::new();
    for tool in profile
        .children
        .iter()
        .filter(|c| c.name == "inspection_tool")
    {
        let Some(id) = tool.attr("class") else {
            continue;
        };
        let spec = registry.get(id);
        let enabled = bool_attr(tool, "enabled_by_default").or_else(|| bool_attr(tool, "enabled"));
        let level = tool.attr("level").map(severity::normalize);
        let mut settings = ToolSettings {
            enabled,
            severity: level.clone(),
            ..ToolSettings::default()
        };
        for c in &tool.children {
            match c.name.as_str() {
                "scope" => {
                    let Some(scope) = c.attr("name") else {
                        continue;
                    };
                    let sev = c.attr("level").map(severity::normalize);
                    settings.scopes.push(ScopeSettings {
                        scope: scope.into(),
                        enabled: bool_attr(c, "enabled").unwrap_or(true),
                        severity: sev.filter(|s| Some(s) != level.as_ref()),
                    });
                }
                "option" => {
                    let (Some(k), Some(v)) = (c.attr("name"), c.attr("value")) else {
                        continue;
                    };
                    let raw = Value::String(v.into());
                    let value = spec
                        .and_then(|s| s.options.iter().find(|o| o.id == k))
                        .and_then(|o| o.coerce(&raw))
                        .unwrap_or(raw);
                    settings.options.insert(k.into(), value);
                }
                _ => {}
            }
        }
        tools.insert(id.to_string(), settings);
    }
    let mut p = Profile { name, level, tools };
    p.compact(registry);
    Ok(p)
}

/// Export in `format`.
pub fn export(profile: &Profile, registry: &Registry, format: ProfileFormat) -> String {
    match format {
        ProfileFormat::Xml => to_xml(profile, registry),
        ProfileFormat::Json => serde_json::to_string_pretty(profile).unwrap_or_default(),
    }
}

/// Import from XML or JSON (recognized by the first character).
pub fn import(text: &str, registry: &Registry) -> Result<Profile, String> {
    let t = text.trim_start_matches('\u{feff}').trim_start();
    if t.starts_with('{') {
        let mut p: Profile =
            serde_json::from_str(t).map_err(|e| format!("not a valid profile: {e}"))?;
        p.compact(registry);
        Ok(p)
    } else {
        from_xml(t, registry)
    }
}

/// The file name PHPStorm gives a profile (`Project Default` -> `Project_Default.xml`).
pub fn file_name(name: &str) -> String {
    let base: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{base}.xml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{InspectionSpec, OptionKind, OptionSpec};
    use serde_json::json;

    fn registry() -> Registry {
        Registry::new(
            vec![InspectionSpec {
                id: "PhpUnusedAssignment".into(),
                name: "Unused assignment".into(),
                group: "PHP/Data flow".into(),
                language: "php".into(),
                severity: "WARNING".into(),
                enabled: true,
                description: String::new(),
                options: vec![OptionSpec {
                    id: "reportParams".into(),
                    label: "Report parameters".into(),
                    kind: OptionKind::Bool,
                    default: json!(false),
                    choices: vec![],
                    min: None,
                    max: None,
                }],
                cleanup: false,
                external: false,
                batch: true,
            }],
            vec![],
        )
    }

    #[test]
    fn xml_round_trip() {
        let r = registry();
        let mut p = Profile::new("Project & Co", ProfileLevel::Project);
        p.tools.insert(
            "PhpUnusedAssignment".into(),
            ToolSettings {
                enabled: Some(false),
                severity: Some("WEAK WARNING".into()),
                scopes: vec![ScopeSettings {
                    scope: "Tests".into(),
                    enabled: true,
                    severity: Some("ERROR".into()),
                }],
                options: [("reportParams".to_string(), json!(true))].into(),
            },
        );
        p.tools.insert(
            "SomethingElse".into(),
            ToolSettings {
                enabled: Some(false),
                ..ToolSettings::default()
            },
        );
        let xml = to_xml(&p, &r);
        assert!(
            xml.contains("<component name=\"InspectionProjectProfileManager\">"),
            "{xml}"
        );
        assert!(xml.contains("value=\"Project &amp; Co\""), "{xml}");
        assert!(xml.contains("class=\"PhpUnusedAssignment\" enabled=\"true\" level=\"WEAK WARNING\" enabled_by_default=\"false\""), "{xml}");
        let back = from_xml(&xml, &r).unwrap();
        assert_eq!(back, p);
        assert_eq!(import(&export(&p, &r, ProfileFormat::Json), &r).unwrap(), p);
    }

    #[test]
    fn reads_phpstorm_files() {
        let r = registry();
        let xml = r#"<component name="InspectionProjectProfileManager">
  <profile version="1.0">
    <option name="myName" value="Project Default" />
    <inspection_tool class="PhpUnusedAssignment" enabled="true" level="WARNING" enabled_by_default="true">
      <option name="reportParams" value="false" />
    </inspection_tool>
    <inspection_tool class="PhpUnused" enabled="false" level="WEAK WARNING" enabled_by_default="false" />
  </profile>
</component>"#;
        let p = import(xml, &r).unwrap();
        assert_eq!(p.name, "Project Default");
        assert_eq!(p.level, ProfileLevel::Project);
        // Same as the defaults: nothing stored.
        assert!(!p.tools.contains_key("PhpUnusedAssignment"));
        assert_eq!(p.tools["PhpUnused"].enabled, Some(false));
        assert!(import("<nope/>", &r).is_err());
        assert!(import("{", &r).is_err());
        assert_eq!(file_name("Project Default"), "Project_Default.xml");
    }
}
