//! Export and import of schemes (the ⚙ menu of the scheme selector): Forge's JSON format
//! with every option, and the subset of PHPStorm's code style XML (`<code_scheme>`, also
//! inside `.idea/codeStyles/Project.xml`) that maps onto Forge's options. Options PHPStorm
//! has no name for are written to a `<ForgeCodeStyleSettings>` element, which PHPStorm
//! ignores, so a scheme survives the round trip.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::model::{model, sanitize, OptionKind, OptionSpec, Values, XmlConv, XmlMap, XmlSection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Json,
    Xml,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Json => "json",
            ExportFormat::Xml => "xml",
        }
    }
}

/// A scheme read from a file: its name and every option's value (defaults for options the
/// file does not set).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedScheme {
    pub name: String,
    pub values: Values,
    /// How many options the file set.
    pub applied: usize,
}

const FORGE_ELEMENT: &str = "ForgeCodeStyleSettings";

/// Write a scheme in `format`.
pub fn export(name: &str, values: &Values, format: ExportFormat) -> String {
    match format {
        ExportFormat::Json => export_json(name, values),
        ExportFormat::Xml => export_xml(name, values),
    }
}

fn export_json(name: &str, values: &Values) -> String {
    let m = model();
    let mut out = serde_json::Map::new();
    for (k, v) in values {
        if m.spec(k).is_some() {
            out.insert(k.clone(), v.clone());
        }
    }
    let doc = json!({ "forgeCodeStyle": 1, "name": name, "values": out });
    serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n"
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
    out
}

/// PHPStorm's value of `v` for `map`, or `None` when PHPStorm has no equivalent.
fn to_xml(map: &XmlMap, v: &Value) -> Option<String> {
    match map.conv {
        XmlConv::Bool => v.as_bool().map(|b| b.to_string()),
        XmlConv::Int => v.as_i64().map(|n| n.to_string()),
        XmlConv::Text => v.as_str().map(str::to_string),
        XmlConv::Brace => match v.as_str()? {
            "endOfLine" => Some("1".into()),
            "nextLine" => Some("2".into()),
            _ => None,
        },
        XmlConv::LineSeparator => match v.as_str()? {
            "lf" => Some("\n".into()),
            "crlf" => Some("\r\n".into()),
            _ => None,
        },
        XmlConv::Map(pairs) => {
            let ours = v
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| v.to_string());
            pairs
                .iter()
                .find(|(o, _)| *o == ours)
                .map(|(_, t)| (*t).to_string())
        }
    }
}

fn from_xml(spec: &OptionSpec, map: &XmlMap, raw: &str) -> Option<Value> {
    let v = match map.conv {
        XmlConv::Bool => json!(raw.trim() == "true"),
        XmlConv::Int => json!(raw.trim().parse::<i64>().ok()?),
        XmlConv::Text => json!(raw),
        XmlConv::Brace => match raw.trim() {
            "1" => json!("endOfLine"),
            "2" | "3" | "4" | "5" => json!("nextLine"),
            _ => return None,
        },
        XmlConv::LineSeparator => match raw {
            "\n" => json!("lf"),
            "\r\n" => json!("crlf"),
            "" => json!("system"),
            _ => return None,
        },
        XmlConv::Map(pairs) => {
            let ours = pairs.iter().find(|(_, t)| *t == raw.trim())?.0;
            match spec.kind {
                OptionKind::Bool => json!(ours == "true"),
                _ => json!(ours),
            }
        }
    };
    sanitize(spec, &v)
}

fn option_line(indent: usize, name: &str, value: &str) -> String {
    format!(
        "{}<option name=\"{}\" value=\"{}\" />\n",
        "  ".repeat(indent),
        escape(name),
        escape(value)
    )
}

fn export_xml(name: &str, values: &Values) -> String {
    let m = model();
    let get = |lang: &str, key: &str| values.get(&format!("{lang}.{key}"));
    let mut out = format!("<code_scheme name=\"{}\" version=\"173\">\n", escape(name));
    let mut forge = String::new();
    let mut unmapped = |lang: &str, o: &OptionSpec, v: &Value| {
        let text = v
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| v.to_string());
        forge.push_str(&option_line(2, &format!("{lang}.{}", o.key), &text));
    };
    for o in &m.general.options {
        let Some(v) = get(&m.general.id, &o.key) else {
            continue;
        };
        match o.xml.as_ref().and_then(|x| to_xml(x, v)) {
            Some(text) => out.push_str(&option_line(1, o.xml.unwrap().name, &text)),
            None if o.key == "lineSeparator" => {}
            None => unmapped(&m.general.id, o, v),
        }
    }
    for l in &m.languages {
        let mut custom = String::new();
        let mut common = String::new();
        let mut indent = String::new();
        for o in &l.options {
            let Some(v) = get(&l.id, &o.key) else {
                continue;
            };
            let Some((x, text)) = o.xml.as_ref().and_then(|x| Some((x, to_xml(x, v)?))) else {
                unmapped(&l.id, o, v);
                continue;
            };
            match x.section {
                XmlSection::Custom => custom.push_str(&option_line(2, x.name, &text)),
                XmlSection::Indent => indent.push_str(&option_line(3, x.name, &text)),
                XmlSection::Common | XmlSection::Root => {
                    common.push_str(&option_line(2, x.name, &text))
                }
            }
        }
        if !custom.is_empty() {
            out.push_str(&format!(
                "  <{}>\n{custom}  </{}>\n",
                l.xml_custom, l.xml_custom
            ));
        }
        out.push_str(&format!(
            "  <codeStyleSettings language=\"{}\">\n{common}",
            l.xml_language
        ));
        if !indent.is_empty() {
            out.push_str(&format!(
                "    <indentOptions>\n{indent}    </indentOptions>\n"
            ));
        }
        out.push_str("  </codeStyleSettings>\n");
    }
    if !forge.is_empty() {
        out.push_str(&format!(
            "  <{FORGE_ELEMENT}>\n{forge}  </{FORGE_ELEMENT}>\n"
        ));
    }
    out.push_str("</code_scheme>\n");
    out
}

/// Read a scheme exported as JSON or XML (detected from the content).
pub fn import(text: &str) -> Result<ImportedScheme, String> {
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with('{') {
        import_json(trimmed)
    } else if trimmed.starts_with('<') {
        import_xml(trimmed)
    } else {
        Err("The file is neither a Forge code style (JSON) nor a code style XML file".into())
    }
}

fn import_json(text: &str) -> Result<ImportedScheme, String> {
    let doc: Value = serde_json::from_str(text).map_err(|e| format!("Invalid JSON: {e}"))?;
    let m = model();
    let mut values = m.defaults();
    let mut applied = 0;
    if let Some(map) = doc.get("values").and_then(Value::as_object) {
        for (k, v) in map {
            if let Some((_, spec)) = m.spec(k) {
                if let Some(v) = sanitize(spec, v) {
                    values.insert(k.clone(), v);
                    applied += 1;
                }
            }
        }
    }
    if applied == 0 {
        return Err("The file contains no code style options".into());
    }
    let name = doc
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Imported")
        .trim()
        .to_string();
    Ok(ImportedScheme {
        name,
        values,
        applied,
    })
}

// ---- A small XML reader (elements and attributes; text is ignored) -----------------------

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

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';') else {
            break;
        };
        let entity = &rest[1..end];
        let c = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            e if e.starts_with("#x") => u32::from_str_radix(&e[2..], 16)
                .ok()
                .and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn parse_xml(text: &str) -> Result<Element, String> {
    let mut stack: Vec<Element> = vec![Element::default()];
    let mut rest = text;
    while let Some(i) = rest.find('<') {
        rest = &rest[i..];
        let skip_to = |rest: &str, end: &str| rest.find(end).map(|e| e + end.len());
        let consumed = if rest.starts_with("<!--") {
            skip_to(rest, "-->")
        } else if rest.starts_with("<![CDATA[") {
            skip_to(rest, "]]>")
        } else if rest.starts_with("<?") {
            skip_to(rest, "?>")
        } else if rest.starts_with("<!") {
            skip_to(rest, ">")
        } else {
            None
        };
        if let Some(n) = consumed {
            rest = &rest[n..];
            continue;
        }
        // A tag: find its end outside quoted attribute values.
        let mut end = None;
        let mut quote: Option<char> = None;
        for (j, c) in rest.char_indices().skip(1) {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') => quote = Some(c),
                (None, '>') => {
                    end = Some(j);
                    break;
                }
                _ => {}
            }
        }
        let end = end.ok_or("Unterminated tag in the XML file")?;
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if let Some(name) = tag.strip_prefix('/') {
            let name = name.trim();
            let el = stack.pop().ok_or("Unbalanced XML")?;
            if el.name != name || stack.is_empty() {
                return Err(format!("Unexpected </{name}> in the XML file"));
            }
            stack.last_mut().unwrap().children.push(el);
            continue;
        }
        let self_closing = tag.ends_with('/');
        let tag = tag.trim_end_matches('/');
        let name_end = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
        let mut el = Element {
            name: tag[..name_end].to_string(),
            ..Element::default()
        };
        let mut attrs = &tag[name_end..];
        loop {
            attrs = attrs.trim_start();
            let Some(eq) = attrs.find('=') else {
                break;
            };
            let key = attrs[..eq].trim().to_string();
            attrs = attrs[eq + 1..].trim_start();
            let Some(q) = attrs.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                break;
            };
            let Some(close) = attrs[1..].find(q) else {
                break;
            };
            el.attrs.push((key, unescape(&attrs[1..1 + close])));
            attrs = &attrs[close + 2..];
        }
        if self_closing {
            stack.last_mut().unwrap().children.push(el);
        } else {
            stack.push(el);
        }
    }
    // Tolerate unclosed elements at the end.
    while stack.len() > 1 {
        let el = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(el);
    }
    Ok(stack.pop().unwrap_or_default())
}

fn import_xml(text: &str) -> Result<ImportedScheme, String> {
    let root = parse_xml(text)?;
    let scheme = root
        .find("code_scheme")
        .ok_or("No <code_scheme> element found in the XML file")?;
    let m = model();
    let mut values = m.defaults();
    let mut applied = 0;
    let mut set = |lang: &str, spec: &OptionSpec, v: Value| {
        values.insert(format!("{lang}.{}", spec.key), v);
        applied += 1;
    };
    let options = |el: &Element| -> Vec<(String, String)> {
        el.children
            .iter()
            .filter(|c| c.name == "option")
            .filter_map(|c| Some((c.attr("name")?.to_string(), c.attr("value")?.to_string())))
            .collect()
    };
    let apply = |lang: &crate::model::LanguageSpec,
                 section: XmlSection,
                 opts: &[(String, String)],
                 set: &mut dyn FnMut(&str, &OptionSpec, Value)| {
        for o in &lang.options {
            let Some(x) = o.xml.as_ref().filter(|x| x.section == section) else {
                continue;
            };
            if let Some((_, raw)) = opts.iter().find(|(n, _)| n == x.name) {
                if let Some(v) = from_xml(o, x, raw) {
                    set(&lang.id, o, v);
                }
            }
        }
    };
    apply(&m.general, XmlSection::Root, &options(scheme), &mut set);
    for child in &scheme.children {
        if child.name == "codeStyleSettings" {
            let Some(lang) = child.attr("language").and_then(|l| {
                m.languages
                    .iter()
                    .find(|s| s.xml_language.eq_ignore_ascii_case(l))
            }) else {
                continue;
            };
            apply(lang, XmlSection::Common, &options(child), &mut set);
            if let Some(indent) = child.children.iter().find(|c| c.name == "indentOptions") {
                apply(lang, XmlSection::Indent, &options(indent), &mut set);
            }
        } else if child.name == FORGE_ELEMENT {
            for (k, raw) in options(child) {
                let Some((lang, spec)) = m.spec(&k) else {
                    continue;
                };
                let v = match spec.kind {
                    OptionKind::Bool => json!(raw == "true"),
                    OptionKind::Int { .. } => json!(raw.parse::<i64>().unwrap_or_default()),
                    _ => json!(raw),
                };
                if let Some(v) = sanitize(spec, &v) {
                    set(&lang.id, spec, v);
                }
            }
        } else if let Some(lang) = m.languages.iter().find(|l| l.xml_custom == child.name) {
            apply(lang, XmlSection::Custom, &options(child), &mut set);
        }
    }
    if applied == 0 {
        return Err("The XML file contains no code style options Forge knows".into());
    }
    let name = scheme
        .attr("name")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Imported")
        .trim()
        .to_string();
    Ok(ImportedScheme {
        name,
        values,
        applied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheme::apply_preset;

    fn wordpress() -> Values {
        apply_preset(&model().defaults(), "php", "wordpress").unwrap()
    }

    #[test]
    fn json_round_trip() {
        let values = wordpress();
        let text = export("WP", &values, ExportFormat::Json);
        assert!(text.contains("\"forgeCodeStyle\": 1"));
        let back = import(&text).unwrap();
        assert_eq!(back.name, "WP");
        assert_eq!(back.values, values);
    }

    #[test]
    fn xml_round_trip_keeps_every_option() {
        let mut values = wordpress();
        values.insert("general.lineSeparator".into(), json!("crlf"));
        values.insert("sql.case.keywords".into(), json!("lower"));
        let text = export("WordPress & co", &values, ExportFormat::Xml);
        assert!(text.starts_with("<code_scheme name=\"WordPress &amp; co\" version=\"173\">"));
        assert!(text.contains("<option name=\"LINE_SEPARATOR\" value=\"&#13;&#10;\" />"));
        assert!(text.contains("<option name=\"USE_TAB_CHARACTER\" value=\"true\" />"));
        assert!(text.contains("<option name=\"CLASS_BRACE_STYLE\" value=\"1\" />"));
        // Long array syntax has no PHPStorm option: kept in Forge's own element.
        assert!(text.contains("<option name=\"php.codeConversion.arraySyntax\" value=\"long\" />"));
        let back = import(&text).unwrap();
        assert_eq!(back.name, "WordPress & co");
        assert_eq!(back.values, values);
    }

    #[test]
    fn imports_a_phpstorm_project_file() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<component name="ProjectCodeStyleConfiguration">
  <code_scheme name="Project" version="173">
    <option name="RIGHT_MARGIN" value="100" />
    <!-- a comment -->
    <PHPCodeStyleSettings>
      <option name="CONCAT_SPACES" value="false" />
      <option name="FORCE_SHORT_DECLARATION_ARRAY_STYLE" value="true" />
      <option name="UNKNOWN_THING" value="true" />
    </PHPCodeStyleSettings>
    <codeStyleSettings language="PHP">
      <option name="KEEP_BLANK_LINES_IN_CODE" value="1" />
      <option name="METHOD_BRACE_STYLE" value="1" />
      <indentOptions>
        <option name="INDENT_SIZE" value="2" />
        <option name="USE_TAB_CHARACTER" value="true" />
      </indentOptions>
    </codeStyleSettings>
    <codeStyleSettings language="Groovy"><option name="X" value="1"/></codeStyleSettings>
  </code_scheme>
</component>"#;
        let s = import(xml).unwrap();
        assert_eq!(s.name, "Project");
        assert_eq!(s.applied, 7);
        assert_eq!(s.values["general.hardWrapAt"], json!(100));
        assert_eq!(s.values["php.spaces.aroundConcatenation"], json!(false));
        assert_eq!(s.values["php.codeConversion.arraySyntax"], json!("short"));
        assert_eq!(s.values["php.blankLines.keepMax"], json!(1));
        assert_eq!(s.values["php.wrapping.functionBraces"], json!("endOfLine"));
        assert_eq!(s.values["php.indent"], json!(2));
        assert_eq!(s.values["php.useTabs"], json!(true));
        assert_eq!(s.values["php.tabSize"], json!(4));
    }

    #[test]
    fn rejects_files_without_options() {
        assert!(import("hello").is_err());
        assert!(import("{\"values\": {\"x.y\": 1}}").is_err());
        assert!(import("<code_scheme name=\"x\"></code_scheme>").is_err());
        assert!(import("<a><b></a>").is_err());
    }
}
