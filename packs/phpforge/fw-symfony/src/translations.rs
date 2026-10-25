//! Symfony translations: the message catalogues in `translations/` (YAML, XLIFF, PHP and
//! JSON files named `<domain>.<locale>.<ext>`, ICU files as `<domain>+intl-icu.<locale>.<ext>`)
//! indexed by domain and locale, the translation references in PHP and Twig
//! (`->trans('key', [], 'domain')`, `t('key')`, `new TranslatableMessage('key')`,
//! `{{ 'key'|trans }}`, `{% trans %}key{% endtrans %}`, `{% trans_default_domain %}`),
//! completion and Go to Declaration for keys and domains, the "missing translation"
//! inspection with its "Create translation key" fix, and the folds that show the translated
//! text in place of a key, like PHPStorm.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use serde::{Deserialize, Serialize};
use yaml_rust2::{Yaml, YamlLoader};

use crate::complete::{CompletionItem, CompletionKind, CompletionList, FileKind};
use crate::strings::{call_context, string_at, string_at_from, StringAt};
use crate::{line_col, rel_path, skip_dir, Location, Snapshot};

/// Domain of references without an explicit one.
pub const DEFAULT_DOMAIN: &str = "messages";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TranslationFormat {
    Yaml,
    Xliff,
    Php,
    Json,
}

impl TranslationFormat {
    fn of(ext: &str) -> Option<Self> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "yaml" | "yml" => Self::Yaml,
            "xlf" | "xliff" => Self::Xliff,
            "php" => Self::Php,
            "json" => Self::Json,
            _ => return None,
        })
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Yaml => "yaml",
            Self::Xliff => "xlf",
            Self::Php => "php",
            Self::Json => "json",
        }
    }
}

/// One catalogue file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationFile {
    pub path: String,
    pub domain: String,
    pub locale: String,
    pub format: TranslationFormat,
    /// `messages+intl-icu.en.yaml`
    #[serde(default)]
    pub icu: bool,
}

/// One message of a catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationEntry {
    pub key: String,
    pub value: String,
    pub domain: String,
    pub locale: String,
    pub location: Location,
}

/// All catalogues of a project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Translations {
    /// Project-relative directory of the catalogues (`translations`).
    pub directory: String,
    /// `framework.default_locale` (`en`).
    pub default_locale: String,
    /// `framework.enabled_locales`; empty when not configured.
    pub enabled_locales: Vec<String>,
    pub files: Vec<TranslationFile>,
    pub entries: Vec<TranslationEntry>,
}

impl Translations {
    /// Locales with a catalogue or enabled in the configuration, the default one first.
    pub fn locales(&self) -> Vec<String> {
        let mut set: BTreeSet<String> = self.files.iter().map(|f| f.locale.clone()).collect();
        set.extend(self.enabled_locales.iter().cloned());
        let mut out: Vec<String> = set.into_iter().collect();
        if let Some(i) = out.iter().position(|l| *l == self.default_locale) {
            let d = out.remove(i);
            out.insert(0, d);
        }
        out
    }

    pub fn domains(&self) -> Vec<String> {
        let set: BTreeSet<&str> = self.files.iter().map(|f| f.domain.as_str()).collect();
        set.into_iter().map(String::from).collect()
    }

    /// Locales with a catalogue of `domain`.
    pub fn domain_locales(&self, domain: &str) -> Vec<String> {
        let set: BTreeSet<&str> = self
            .files
            .iter()
            .filter(|f| f.domain == domain)
            .map(|f| f.locale.as_str())
            .collect();
        set.into_iter().map(String::from).collect()
    }

    /// The messages of `key` in `domain`, one per locale.
    pub fn find<'a>(
        &'a self,
        domain: &'a str,
        key: &'a str,
    ) -> impl Iterator<Item = &'a TranslationEntry> + 'a {
        self.entries
            .iter()
            .filter(move |e| e.domain == domain && e.key == key)
    }

    /// The text of `key` in the default locale, else in any locale.
    pub fn text(&self, domain: &str, key: &str) -> Option<&str> {
        let all: Vec<&TranslationEntry> = self
            .entries
            .iter()
            .filter(|e| e.domain == domain && e.key == key)
            .collect();
        all.iter()
            .find(|e| e.locale == self.default_locale)
            .or(all.first())
            .map(|e| e.value.as_str())
    }

    /// Keys per domain with their text per locale, for the tool window.
    pub fn table(&self) -> TranslationTable {
        let mut rows: BTreeMap<(String, String), TranslationRow> = BTreeMap::new();
        for e in &self.entries {
            let row = rows
                .entry((e.domain.clone(), e.key.clone()))
                .or_insert_with(|| TranslationRow {
                    domain: e.domain.clone(),
                    key: e.key.clone(),
                    values: BTreeMap::new(),
                    locations: BTreeMap::new(),
                    missing: Vec::new(),
                });
            row.values.insert(e.locale.clone(), e.value.clone());
            row.locations.insert(e.locale.clone(), e.location.clone());
        }
        let mut rows: Vec<TranslationRow> = rows.into_values().collect();
        for row in &mut rows {
            row.missing = self
                .domain_locales(&row.domain)
                .into_iter()
                .filter(|l| !row.values.contains_key(l))
                .collect();
        }
        TranslationTable {
            default_locale: self.default_locale.clone(),
            locales: self.locales(),
            domains: self.domains(),
            files: self.files.clone(),
            rows,
        }
    }
}

/// Reply of the Translations tab: one row per domain and key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationTable {
    pub default_locale: String,
    pub locales: Vec<String>,
    pub domains: Vec<String>,
    pub files: Vec<TranslationFile>,
    pub rows: Vec<TranslationRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationRow {
    pub domain: String,
    pub key: String,
    /// Locale -> text.
    pub values: BTreeMap<String, String>,
    pub locations: BTreeMap<String, Location>,
    /// Locales of the domain without this key.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
}

// ---------------------------------------------------------------- configuration

fn yaml_get<'a>(y: &'a Yaml, key: &str) -> Option<&'a Yaml> {
    match y {
        Yaml::Hash(h) => h.get(&Yaml::String(key.to_string())),
        _ => None,
    }
}

/// `default_locale`, `enabled_locales` and the translator's `default_path` from
/// `config/packages/{framework,translation}.yaml`.
fn framework_config(root: &Path) -> (Option<String>, Vec<String>, Option<String>) {
    let mut default_locale = None;
    let mut enabled = Vec::new();
    let mut path = None;
    for file in [
        "config/packages/framework.yaml",
        "config/packages/translation.yaml",
    ] {
        let Ok(text) = std::fs::read_to_string(root.join(file)) else {
            continue;
        };
        let Ok(docs) = YamlLoader::load_from_str(&text) else {
            continue;
        };
        for doc in &docs {
            let Some(fw) = yaml_get(doc, "framework") else {
                continue;
            };
            if let Some(Yaml::String(l)) = yaml_get(fw, "default_locale") {
                default_locale = Some(l.clone());
            }
            if let Some(Yaml::Array(a)) = yaml_get(fw, "enabled_locales") {
                enabled = a
                    .iter()
                    .filter_map(|l| l.as_str().map(String::from))
                    .collect();
            }
            if let Some(tr) = yaml_get(fw, "translator") {
                if let Some(Yaml::String(p)) = yaml_get(tr, "default_path") {
                    path = Some(
                        p.trim_start_matches("%kernel.project_dir%")
                            .trim_start_matches('/')
                            .to_string(),
                    );
                }
                if default_locale.is_none() {
                    if let Some(Yaml::Array(f)) = yaml_get(tr, "fallbacks") {
                        default_locale = f.first().and_then(|l| l.as_str()).map(String::from);
                    }
                }
            }
        }
    }
    (default_locale, enabled, path)
}

/// `messages+intl-icu.en.yaml` -> (`messages`, `en`, yaml, icu).
pub fn parse_file_name(name: &str) -> Option<(String, String, TranslationFormat, bool)> {
    let (rest, ext) = name.rsplit_once('.')?;
    let format = TranslationFormat::of(ext)?;
    let (domain, locale) = rest.rsplit_once('.')?;
    if domain.is_empty() || locale.is_empty() {
        return None;
    }
    let (domain, icu) = match domain.strip_suffix("+intl-icu") {
        Some(d) => (d, true),
        None => (domain, false),
    };
    Some((domain.to_string(), locale.to_string(), format, icu))
}

/// Index the catalogues of a project.
pub fn scan(root: &Path) -> Translations {
    let (default_locale, enabled_locales, path) = framework_config(root);
    let directory = path.unwrap_or_else(|| "translations".into());
    let mut out = Translations {
        directory: directory.clone(),
        default_locale: default_locale.unwrap_or_else(|| "en".into()),
        enabled_locales,
        files: Vec::new(),
        entries: Vec::new(),
    };
    let base = root.join(&directory);
    if !base.is_dir() {
        return out;
    }
    for entry in walkdir::WalkDir::new(&base)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| !(e.file_type().is_dir() && skip_dir(&e.file_name().to_string_lossy())))
        .flatten()
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        let Some((domain, locale, format, icu)) = parse_file_name(&name) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let path = rel_path(root, entry.path());
        for (key, value, line, column) in parse_catalogue(format, &text) {
            out.entries.push(TranslationEntry {
                key,
                value,
                domain: domain.clone(),
                locale: locale.clone(),
                location: Location::new(&path, line, column),
            });
        }
        out.files.push(TranslationFile {
            path,
            domain,
            locale,
            format,
            icu,
        });
    }
    out
}

/// `(key, text, line, column)` of every message in a catalogue.
pub fn parse_catalogue(format: TranslationFormat, text: &str) -> Vec<(String, String, u32, u32)> {
    match format {
        TranslationFormat::Yaml => yaml_tree(text)
            .into_iter()
            .filter_map(|n| {
                let v = n.value?;
                Some((n.path.join("."), v, n.line, n.column))
            })
            .collect(),
        TranslationFormat::Xliff => parse_xliff(text),
        TranslationFormat::Php => parse_php_array(text),
        TranslationFormat::Json => parse_json(text),
    }
}

// ---------------------------------------------------------------- YAML

/// A mapping key of a YAML catalogue, found line by line so positions are exact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct YamlNode {
    pub path: Vec<String>,
    /// `None` for a nested mapping.
    pub value: Option<String>,
    pub line: u32,
    pub column: u32,
    pub indent: usize,
    /// Last line of the value (or of the nested mapping).
    pub end_line: u32,
}

/// Read a quoted scalar at the start of `s`: (content, rest after the closing quote).
fn quoted(s: &str) -> Option<(String, &str)> {
    let q = s.chars().next()?;
    if q != '\'' && q != '"' {
        return None;
    }
    let mut out = String::new();
    let mut chars = s[1..].char_indices();
    while let Some((i, c)) = chars.next() {
        if c == q {
            if q == '\'' && s[1 + i + 1..].starts_with('\'') {
                out.push('\'');
                chars.next();
                continue;
            }
            return Some((out, &s[1 + i + 1..]));
        }
        if c == '\\' && q == '"' {
            match chars.next() {
                Some((_, 'n')) => out.push('\n'),
                Some((_, 't')) => out.push('\t'),
                Some((_, o)) => out.push(o),
                None => {}
            }
            continue;
        }
        out.push(c);
    }
    None
}

fn strip_comment(s: &str) -> &str {
    match s.find(" #") {
        Some(i) => &s[..i],
        None => s,
    }
}

pub(crate) fn yaml_tree(text: &str) -> Vec<YamlNode> {
    let lines: Vec<&str> = text.lines().collect();
    let mut nodes: Vec<YamlNode> = Vec::new();
    // Indexes of open mappings.
    let mut stack: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with("---")
            || trimmed.starts_with("...")
            || trimmed.starts_with("- ")
        {
            i += 1;
            continue;
        }
        let (key, rest) = if let Some((k, r)) = quoted(trimmed) {
            let Some(r) = r.trim_start().strip_prefix(':') else {
                i += 1;
                continue;
            };
            (k, r)
        } else {
            let Some(colon) = trimmed
                .char_indices()
                .find(|&(j, c)| {
                    c == ':' && trimmed[j + 1..].chars().next().is_none_or(|n| n == ' ')
                })
                .map(|(j, _)| j)
            else {
                i += 1;
                continue;
            };
            (
                trimmed[..colon].trim_end().to_string(),
                &trimmed[colon + 1..],
            )
        };
        while let Some(&top) = stack.last() {
            if nodes[top].indent >= indent {
                stack.pop();
            } else {
                break;
            }
        }
        let mut path: Vec<String> = stack
            .last()
            .map(|&t| nodes[t].path.clone())
            .unwrap_or_default();
        path.push(key);
        let rest = rest.trim();
        let line_no = i as u32;
        let mut end = line_no;
        let value = if rest.is_empty() || rest.starts_with('#') {
            None
        } else if rest.starts_with('|') || rest.starts_with('>') {
            let folded = rest.starts_with('>');
            let mut parts: Vec<&str> = Vec::new();
            let mut j = i + 1;
            let mut block_indent = None;
            while j < lines.len() {
                let l = lines[j];
                let t = l.trim_start();
                let ind = l.len() - t.len();
                if !t.is_empty() && ind <= indent {
                    break;
                }
                if !t.is_empty() && block_indent.is_none() {
                    block_indent = Some(ind);
                }
                parts.push(
                    l.get(block_indent.unwrap_or(ind)..)
                        .unwrap_or("")
                        .trim_end(),
                );
                j += 1;
            }
            while parts.last().is_some_and(|p| p.is_empty()) {
                parts.pop();
            }
            end = (i + parts.len()) as u32;
            i = j - 1;
            Some(parts.join(if folded { " " } else { "\n" }))
        } else if let Some((v, _)) = quoted(rest) {
            Some(v)
        } else {
            Some(strip_comment(rest).trim().to_string())
        };
        for &s in &stack {
            nodes[s].end_line = nodes[s].end_line.max(end);
        }
        let is_map = value.is_none();
        nodes.push(YamlNode {
            path,
            value,
            line: line_no,
            column: indent as u32,
            indent,
            end_line: end,
        });
        if is_map {
            stack.push(nodes.len() - 1);
        }
        i += 1;
    }
    nodes
}

// ---------------------------------------------------------------- XLIFF

fn xml_attr(e: &quick_xml::events::BytesStart, name: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| {
            a.normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|v| v.into_owned())
        })
}

fn entity(name: &str) -> &'static str {
    match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        _ => "",
    }
}

/// XLIFF 1.2 `<trans-unit resname=…><source/><target/>` and 2.0
/// `<unit name=…><segment><source/><target/>`. The key is `resname`/`name`, else the source.
fn parse_xliff(text: &str) -> Vec<(String, String, u32, u32)> {
    let mut reader = Reader::from_str(text);
    let mut out = Vec::new();
    // (name attribute, id, start offset)
    let mut unit: Option<(Option<String>, Option<String>, usize)> = None;
    let mut source = String::new();
    let mut target: Option<String> = None;
    let mut field: Option<&'static str> = None;
    loop {
        let before = reader.buffer_position() as usize;
        let Ok(event) = reader.read_event() else {
            break;
        };
        match event {
            Event::Start(ref e) => match e.name().as_ref() {
                "trans-unit" => {
                    unit = Some((xml_attr(e, "resname"), xml_attr(e, "id"), before));
                    source.clear();
                    target = None;
                }
                "unit" => {
                    unit = Some((xml_attr(e, "name"), xml_attr(e, "id"), before));
                    source.clear();
                    target = None;
                }
                "source" if unit.is_some() => field = Some("source"),
                "target" if unit.is_some() => {
                    field = Some("target");
                    target.get_or_insert_with(String::new);
                }
                _ => {}
            },
            Event::Text(ref t) => push_xml_text(&mut source, &mut target, field, t),
            Event::CData(ref t) => push_xml_text(&mut source, &mut target, field, t),
            Event::GeneralRef(ref r) => {
                push_xml_text(&mut source, &mut target, field, entity(r));
            }
            Event::End(ref e) => match e.name().as_ref() {
                "source" | "target" => field = None,
                "trans-unit" | "unit" => {
                    if let Some((name, _id, start)) = unit.take() {
                        let key = name
                            .filter(|n| !n.is_empty())
                            .unwrap_or_else(|| source.trim().to_string());
                        if !key.is_empty() {
                            let value = target
                                .take()
                                .filter(|t| !t.trim().is_empty())
                                .unwrap_or_else(|| source.clone());
                            // Report the line of the `<trans-unit` tag itself.
                            let tag = text[start..].find('<').map(|o| start + o).unwrap_or(start);
                            let (line, col) = line_col(text, tag);
                            out.push((key, value.trim().to_string(), line, col));
                        }
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }
    out
}

fn push_xml_text(source: &mut String, target: &mut Option<String>, field: Option<&str>, t: &str) {
    match field {
        Some("source") => source.push_str(t),
        Some("target") => target.get_or_insert_with(String::new).push_str(t),
        _ => {}
    }
}

// ---------------------------------------------------------------- PHP and JSON

/// A tiny tokenizer for `return ['key' => 'text', 'nested' => ['a' => 'b']];`.
fn parse_php_array(text: &str) -> Vec<(String, String, u32, u32)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    // Prefix per open array; `None` for list-style arrays.
    let mut stack: Vec<Option<String>> = Vec::new();
    // Pending key: (key, offset).
    let mut key: Option<(String, usize)> = None;
    let mut i = 0;
    let start = text.find("return").map(|p| p + 6).unwrap_or(0);
    i = i.max(start);
    let read_string = |i: usize| -> Option<(String, usize)> {
        let q = bytes[i];
        let mut j = i + 1;
        let mut s = String::new();
        while j < bytes.len() {
            let c = bytes[j];
            if c == b'\\' && j + 1 < bytes.len() {
                let n = bytes[j + 1];
                if n == q || n == b'\\' {
                    s.push(n as char);
                    j += 2;
                    continue;
                }
                if q == b'"' && n == b'n' {
                    s.push('\n');
                    j += 2;
                    continue;
                }
            }
            if c == q {
                return Some((s, j + 1));
            }
            let ch = text[j..].chars().next()?;
            s.push(ch);
            j += ch.len_utf8();
        }
        None
    };
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                i = text[i..].find('\n').map(|p| i + p).unwrap_or(bytes.len());
            }
            b'#' => {
                i = text[i..].find('\n').map(|p| i + p).unwrap_or(bytes.len());
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = text[i..]
                    .find("*/")
                    .map(|p| i + p + 2)
                    .unwrap_or(bytes.len());
            }
            b'[' => {
                let prefix = key.take().map(|(k, _)| k);
                let parent = stack.last().cloned().flatten();
                stack.push(Some(match (parent, prefix) {
                    (Some(p), Some(k)) if !p.is_empty() => format!("{p}.{k}"),
                    (_, Some(k)) => k,
                    (Some(p), None) => p,
                    (None, None) => String::new(),
                }));
                i += 1;
            }
            b'a' if text[i..].starts_with("array(") => {
                let prefix = key.take().map(|(k, _)| k);
                let parent = stack.last().cloned().flatten();
                stack.push(Some(match (parent, prefix) {
                    (Some(p), Some(k)) if !p.is_empty() => format!("{p}.{k}"),
                    (_, Some(k)) => k,
                    (Some(p), None) => p,
                    (None, None) => String::new(),
                }));
                i += 6;
            }
            b']' | b')' => {
                stack.pop();
                key = None;
                if stack.is_empty() {
                    break;
                }
                i += 1;
            }
            b'\'' | b'"' => {
                let Some((s, next)) = read_string(i) else {
                    break;
                };
                let after = text[next..].trim_start();
                if after.starts_with("=>") {
                    key = Some((s, i));
                } else if let Some((k, at)) = key.take() {
                    let prefix = stack.last().cloned().flatten().unwrap_or_default();
                    let full = if prefix.is_empty() {
                        k
                    } else {
                        format!("{prefix}.{k}")
                    };
                    let (line, col) = line_col(text, at);
                    out.push((full, s, line, col));
                }
                i = next;
            }
            b',' => {
                key = None;
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// Flat or nested JSON objects.
fn parse_json(text: &str) -> Vec<(String, String, u32, u32)> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut cursor = 0usize;
    fn walk(
        v: &serde_json::Value,
        prefix: &str,
        text: &str,
        cursor: &mut usize,
        out: &mut Vec<(String, String, u32, u32)>,
    ) {
        let Some(map) = v.as_object() else {
            return;
        };
        for (k, child) in map {
            let quoted = serde_json::to_string(k).unwrap_or_default();
            let at = text[*cursor..]
                .find(&quoted)
                .map(|p| *cursor + p)
                .unwrap_or(*cursor);
            *cursor = (at + quoted.len()).min(text.len());
            let full = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            match child {
                serde_json::Value::Object(_) => walk(child, &full, text, cursor, out),
                serde_json::Value::String(s) => {
                    let (line, col) = line_col(text, at);
                    out.push((full, s.clone(), line, col));
                }
                other => {
                    let (line, col) = line_col(text, at);
                    out.push((full, other.to_string(), line, col));
                }
            }
        }
    }
    walk(&value, "", text, &mut cursor, &mut out);
    out
}

// ---------------------------------------------------------------- references

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransRefKind {
    Key,
    Domain,
}

/// A translation key or domain in a PHP or Twig buffer. Offsets are bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransRef {
    pub kind: TransRefKind,
    /// Content range (without quotes).
    pub start: usize,
    pub end: usize,
    /// Range of the whole literal, quotes included (the fold range).
    pub literal_start: usize,
    pub literal_end: usize,
    pub prefix: String,
    pub value: String,
    /// Effective domain (explicit, `trans_default_domain`, or `messages`).
    pub domain: String,
}

/// Arguments after the one that ends at `from`, up to the closing `)` of the call.
fn following_args(text: &str, from: usize) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut i = from;
    let mut depth = 0i32;
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut started = false;
    while i < bytes.len() && i < from + 2000 {
        let c = bytes[i];
        match c {
            b'\'' | b'"' => {
                let q = c;
                let s = i;
                i += 1;
                while i < bytes.len() && bytes[i] != q {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                if started {
                    cur.push_str(&text[s..(i + 1).min(text.len())]);
                }
                i += 1;
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' if depth > 0 => depth -= 1,
            b')' | b']' | b'}' => {
                if started {
                    args.push(cur.trim().to_string());
                }
                return args;
            }
            b',' if depth == 0 => {
                if started {
                    args.push(cur.trim().to_string());
                    cur.clear();
                }
                started = true;
                i += 1;
                continue;
            }
            b';' if depth == 0 => return args,
            _ => {}
        }
        if started {
            cur.push(c as char);
        }
        i += 1;
    }
    args
}

/// `'admin'` -> `admin`; `domain: 'admin'` -> `admin`.
fn literal(arg: &str) -> Option<String> {
    let a = arg.trim();
    let a = match a.split_once(':') {
        Some((name, v)) if name.trim().chars().all(|c| c.is_alphanumeric() || c == '_') => v.trim(),
        _ => a,
    };
    let (s, rest) = quoted(a)?;
    rest.trim().is_empty().then_some(s)
}

/// Named `domain:` argument among `args`.
fn named_domain(args: &[String]) -> Option<String> {
    args.iter().find_map(|a| {
        let (n, _) = a.split_once(':')?;
        (n.trim() == "domain").then(|| literal(a)).flatten()
    })
}

/// `{% trans_default_domain 'admin' %}` in effect at `offset`.
fn twig_default_domain(text: &str, offset: usize) -> Option<String> {
    let before = &text[..offset.min(text.len())];
    let i = before.rfind("trans_default_domain")?;
    let rest = before[i + "trans_default_domain".len()..].trim_start();
    quoted(rest).map(|(d, _)| d)
}

fn in_twig_block(text: &str, offset: usize) -> Option<usize> {
    let before = &text[..offset];
    let tag = before.rfind("{%");
    let expr = before.rfind("{{");
    let (pos, close) = match (tag, expr) {
        (Some(t), Some(e)) if t > e => (t, "%}"),
        (Some(_), Some(e)) => (e, "}}"),
        (Some(t), None) => (t, "%}"),
        (None, Some(e)) => (e, "}}"),
        (None, None) => return None,
    };
    (!before[pos + 2..].contains(close)).then_some(pos + 2)
}

fn make_ref(s: &StringAt, kind: TransRefKind, domain: String) -> TransRef {
    TransRef {
        kind,
        start: s.start,
        end: s.end,
        literal_start: s.open,
        literal_end: if s.closed { s.end + 1 } else { s.end },
        prefix: s.prefix.clone(),
        value: s.value.clone(),
        domain,
    }
}

/// Classify a string literal of a PHP buffer.
fn php_ref(text: &str, s: &StringAt) -> Option<TransRef> {
    let call = call_context(text, s.open)?;
    let f = call.function.as_str();
    let translating = (f == "trans" && call.is_method)
        || (f == "trans" && !call.is_method)
        || (f == "t" && !call.is_method)
        || f == "TranslatableMessage";
    if !translating {
        return None;
    }
    if call.named.as_deref() == Some("domain") || (call.named.is_none() && call.arg == 2) {
        return Some(make_ref(s, TransRefKind::Domain, s.value.clone()));
    }
    if call.arg != 0
        || call
            .named
            .as_deref()
            .is_some_and(|n| n != "message" && n != "id")
    {
        return None;
    }
    let args = following_args(text, s.end + usize::from(s.closed));
    let domain = named_domain(&args)
        .or_else(|| args.get(1).and_then(|a| literal(a)))
        .unwrap_or_else(|| DEFAULT_DOMAIN.to_string());
    Some(make_ref(s, TransRefKind::Key, domain))
}

/// Classify a string literal inside a Twig `{{ }}` / `{% %}` block.
fn twig_ref(text: &str, s: &StringAt, block_start: usize) -> Option<TransRef> {
    let default = || twig_default_domain(text, s.open).unwrap_or_else(|| DEFAULT_DOMAIN.into());
    // `'key'|trans(…)`
    let after_pos = s.end + usize::from(s.closed);
    let after = text[after_pos..].trim_start();
    if let Some(rest) = after.strip_prefix('|') {
        let rest = rest.trim_start();
        if rest.starts_with("trans")
            && !rest[5..]
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            let call = rest[5..].trim_start();
            let domain = if call.starts_with('(') {
                let open = text.len() - call.len();
                let args = following_args(text, open + 1);
                // `following_args` skips the first argument: re-split from the `(`.
                let mut all = vec![String::new()];
                all.extend(args);
                let first = text[open + 1..]
                    .split([',', ')'])
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                all[0] = first;
                named_domain(&all).or_else(|| all.get(1).and_then(|a| literal(a)))
            } else {
                None
            };
            return Some(make_ref(
                s,
                TransRefKind::Key,
                domain.unwrap_or_else(default),
            ));
        }
    }
    if let Some(call) = call_context(text, s.open) {
        // `|trans({}, 'admin')`: the filter's domain argument.
        if call.function == "trans" {
            let name_at = text[..s.open].rfind("trans").unwrap_or(0);
            if text[..name_at].trim_end().ends_with('|') && call.arg == 1 {
                return Some(make_ref(s, TransRefKind::Domain, s.value.clone()));
            }
        }
        if call.function == "t" && !call.is_method {
            if call.arg == 2 || call.named.as_deref() == Some("domain") {
                return Some(make_ref(s, TransRefKind::Domain, s.value.clone()));
            }
            if call.arg == 0 {
                let args = following_args(text, after_pos);
                let domain = named_domain(&args)
                    .or_else(|| args.get(1).and_then(|a| literal(a)))
                    .unwrap_or_else(default);
                return Some(make_ref(s, TransRefKind::Key, domain));
            }
        }
    }
    // `{% trans from 'admin' %}` / `{% trans_default_domain 'admin' %}`
    let head = text[block_start..s.open].trim_start_matches('-').trim();
    if head == "trans_default_domain"
        || (head.starts_with("trans ") || head.starts_with("trans\t"))
            && head.trim_end().ends_with("from")
    {
        return Some(make_ref(s, TransRefKind::Domain, s.value.clone()));
    }
    None
}

/// `{% trans %}key{% endtrans %}` blocks: (body start, body end, domain).
fn trans_blocks(text: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(p) = text[from..].find("{%") {
        let open = from + p;
        let Some(close) = text[open..].find("%}").map(|c| open + c) else {
            break;
        };
        let head = text[open + 2..close].trim_matches(['-', '~', ' ', '\t', '\n']);
        from = close + 2;
        if head != "trans" && !head.starts_with("trans ") {
            continue;
        }
        let domain = head
            .find(" from ")
            .and_then(|i| quoted(head[i + 6..].trim_start()).map(|(d, _)| d))
            .unwrap_or_else(|| {
                twig_default_domain(text, open).unwrap_or_else(|| DEFAULT_DOMAIN.into())
            });
        let Some(end) = text[from..].find("{%").map(|e| from + e) else {
            break;
        };
        if text[end + 2..]
            .trim_start_matches(['-', ' '])
            .starts_with("endtrans")
        {
            out.push((from, end, domain));
        }
    }
    out
}

/// The `{% trans %}` body key containing `offset`.
fn trans_block_at(text: &str, offset: usize) -> Option<TransRef> {
    trans_blocks(text)
        .into_iter()
        .find(|(s, e, _)| *s <= offset && offset <= *e)
        .map(|(s, e, domain)| {
            let body = &text[s..e];
            let lead = body.len() - body.trim_start().len();
            let start = s + lead;
            let end = s + body.trim_end().len();
            let end = end.max(start);
            TransRef {
                kind: TransRefKind::Key,
                start,
                end,
                literal_start: start,
                literal_end: end,
                prefix: text[start..offset.clamp(start, end)].to_string(),
                value: text[start..end].to_string(),
                domain,
            }
        })
}

/// The translation reference at `offset`.
pub fn reference_at(path: &str, text: &str, offset: usize) -> Option<TransRef> {
    let offset = offset.min(text.len());
    match FileKind::of(path) {
        FileKind::Php => {
            let s = string_at(text, offset)?;
            php_ref(text, &s)
        }
        FileKind::Twig => {
            if let Some(r) = trans_block_at(text, offset) {
                return Some(r);
            }
            let start = in_twig_block(text, offset)?;
            let s = string_at_from(text, offset, start)?;
            twig_ref(text, &s, start)
        }
        _ => None,
    }
}

/// Every string literal of a buffer (Twig: inside `{{ }}` / `{% %}` only).
fn literals(path: &str, text: &str) -> Vec<(StringAt, usize)> {
    let twig = FileKind::of(path) == FileKind::Twig;
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut block: Option<(usize, &'static str)> = None;
    while i < bytes.len() {
        let c = bytes[i];
        if twig {
            match block {
                None => {
                    if text[i..].starts_with("{{") {
                        block = Some((i + 2, "}}"));
                        i += 2;
                    } else if text[i..].starts_with("{%") {
                        block = Some((i + 2, "%}"));
                        i += 2;
                    } else if text[i..].starts_with("{#") {
                        i = text[i..]
                            .find("#}")
                            .map(|p| i + p + 2)
                            .unwrap_or(bytes.len());
                    } else {
                        i += 1;
                    }
                    continue;
                }
                Some((_, close)) if text[i..].starts_with(close) => {
                    block = None;
                    i += 2;
                    continue;
                }
                _ => {}
            }
        } else {
            if c == b'/' && bytes.get(i + 1) == Some(&b'/')
                || c == b'#' && bytes.get(i + 1) != Some(&b'[')
            {
                i = text[i..].find('\n').map(|p| i + p).unwrap_or(bytes.len());
                continue;
            }
            if c == b'/' && bytes.get(i + 1) == Some(&b'*') {
                i = text[i..]
                    .find("*/")
                    .map(|p| i + p + 2)
                    .unwrap_or(bytes.len());
                continue;
            }
        }
        if c == b'\'' || c == b'"' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != c && bytes[j] != b'\n' {
                if bytes[j] == b'\\' {
                    j += 1;
                }
                j += 1;
            }
            let closed = j < bytes.len() && bytes[j] == c;
            let end = j.min(bytes.len());
            if closed {
                let value = text[i + 1..end].to_string();
                out.push((
                    StringAt {
                        quote: c as char,
                        open: i,
                        start: i + 1,
                        end,
                        prefix: value.clone(),
                        value,
                        closed,
                    },
                    block.map(|b| b.0).unwrap_or(0),
                ));
            }
            i = end + 1;
            continue;
        }
        i += 1;
    }
    out
}

/// All translation references of a buffer.
pub fn references(path: &str, text: &str) -> Vec<TransRef> {
    let kind = FileKind::of(path);
    if kind != FileKind::Php && kind != FileKind::Twig {
        return Vec::new();
    }
    let mut out: Vec<TransRef> = literals(path, text)
        .into_iter()
        .filter_map(|(s, block)| match kind {
            FileKind::Php => php_ref(text, &s),
            _ => twig_ref(text, &s, block),
        })
        .collect();
    if kind == FileKind::Twig {
        for (s, e, domain) in trans_blocks(text) {
            let body = &text[s..e];
            let key = body.trim();
            if key.is_empty() || key.contains("{{") || key.contains('%') {
                continue;
            }
            let start = s + (body.len() - body.trim_start().len());
            out.push(TransRef {
                kind: TransRefKind::Key,
                start,
                end: start + key.len(),
                literal_start: start,
                literal_end: start + key.len(),
                prefix: key.to_string(),
                value: key.to_string(),
                domain,
            });
        }
    }
    out.sort_by_key(|r| r.start);
    out
}

/// Whether a key is a literal worth checking (no interpolation or concatenation).
fn checkable(key: &str) -> bool {
    !key.is_empty() && !key.contains(['$', '{', '}', '#'])
}

// ---------------------------------------------------------------- editor services

/// Completion of keys (with their text) and domains.
pub fn complete(snap: &Snapshot, path: &str, text: &str, offset: usize) -> Option<CompletionList> {
    let r = reference_at(path, text, offset)?;
    let tr = &snap.translations;
    let p = r.prefix.to_ascii_lowercase();
    let mut items: Vec<CompletionItem> = match r.kind {
        TransRefKind::Key => {
            let mut seen = BTreeSet::new();
            tr.entries
                .iter()
                .filter(|e| e.domain == r.domain && e.key.to_ascii_lowercase().contains(&p))
                .filter(|e| seen.insert(e.key.clone()))
                .map(|e| CompletionItem {
                    label: e.key.clone(),
                    kind: CompletionKind::Translation,
                    detail: tr.text(&r.domain, &e.key).unwrap_or("").to_string(),
                    info: r.domain.clone(),
                    apply: None,
                })
                .collect()
        }
        TransRefKind::Domain => tr
            .domains()
            .into_iter()
            .filter(|d| d.to_ascii_lowercase().contains(&p))
            .map(|d| CompletionItem {
                detail: tr.domain_locales(&d).join(", "),
                label: d,
                kind: CompletionKind::Domain,
                info: String::new(),
                apply: None,
            })
            .collect(),
    };
    items.sort_by(|a, b| {
        let ap = !a.label.to_ascii_lowercase().starts_with(&p);
        let bp = !b.label.to_ascii_lowercase().starts_with(&p);
        ap.cmp(&bp).then(a.label.cmp(&b.label))
    });
    Some(CompletionList {
        from: r.start,
        items,
    })
}

/// The key in every locale, or the catalogues of a domain.
pub fn goto(snap: &Snapshot, path: &str, text: &str, offset: usize) -> Option<Vec<Location>> {
    let r = reference_at(path, text, offset)?;
    let tr = &snap.translations;
    let mut out: Vec<Location> = match r.kind {
        TransRefKind::Key => tr
            .find(&r.domain, &r.value)
            .map(|e| e.location.clone())
            .collect(),
        TransRefKind::Domain => tr
            .files
            .iter()
            .filter(|f| f.domain == r.value)
            .map(|f| Location::new(&f.path, 0, 0))
            .collect(),
    };
    let locales = tr.locales();
    let rank = |l: &Location| {
        tr.files
            .iter()
            .find(|f| f.path == l.path)
            .and_then(|f| locales.iter().position(|x| *x == f.locale))
            .unwrap_or(usize::MAX)
    };
    out.sort_by_key(rank);
    Some(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProblemSeverity {
    Warning,
    Weak,
}

/// A missing translation. Offsets are bytes here and UTF-16 over IPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationProblem {
    pub from: usize,
    pub to: usize,
    pub severity: ProblemSeverity,
    pub message: String,
    pub domain: String,
    pub key: String,
    /// Locales the "Create translation key" fix writes to.
    pub locales: Vec<String>,
}

/// Inspection: keys missing in all locales (warning) or in some of them (weak warning).
pub fn problems(snap: &Snapshot, path: &str, text: &str) -> Vec<TranslationProblem> {
    let tr = &snap.translations;
    if tr.files.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for r in references(path, text) {
        if r.kind != TransRefKind::Key || !checkable(&r.value) {
            continue;
        }
        let have: BTreeSet<String> = tr
            .find(&r.domain, &r.value)
            .map(|e| e.locale.clone())
            .collect();
        let mut wanted = tr.domain_locales(&r.domain);
        if wanted.is_empty() {
            wanted.push(tr.default_locale.clone());
        }
        let missing: Vec<String> = wanted.into_iter().filter(|l| !have.contains(l)).collect();
        if missing.is_empty() {
            continue;
        }
        let (severity, message) = if have.is_empty() {
            (
                ProblemSeverity::Warning,
                format!(
                    "Missing translation key '{}' in domain '{}'",
                    r.value, r.domain
                ),
            )
        } else {
            (
                ProblemSeverity::Weak,
                format!(
                    "Translation '{}' is missing for locale{} {}",
                    r.value,
                    if missing.len() == 1 { "" } else { "s" },
                    missing.join(", ")
                ),
            )
        };
        out.push(TranslationProblem {
            from: r.start,
            to: r.end,
            severity,
            message,
            domain: r.domain.clone(),
            key: r.value.clone(),
            locales: missing,
        });
    }
    out
}

/// A key folded to its translated text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationFold {
    /// Range of the literal (quotes included).
    pub from: usize,
    pub to: usize,
    /// The text in the default locale.
    pub text: String,
    pub key: String,
}

/// Longest text shown in a fold.
const FOLD_MAX: usize = 60;

/// Folds for the keys of a buffer that have a translation.
pub fn folds(snap: &Snapshot, path: &str, text: &str) -> Vec<TranslationFold> {
    let tr = &snap.translations;
    references(path, text)
        .into_iter()
        .filter(|r| r.kind == TransRefKind::Key && r.literal_end > r.literal_start)
        .filter_map(|r| {
            let value = tr.text(&r.domain, &r.value)?;
            let one_line = value.split_whitespace().collect::<Vec<_>>().join(" ");
            let shown = if one_line.chars().count() > FOLD_MAX {
                let cut: String = one_line.chars().take(FOLD_MAX - 1).collect();
                format!("{cut}…")
            } else {
                one_line
            };
            Some(TranslationFold {
                from: r.literal_start,
                to: r.literal_end,
                text: shown,
                key: r.value,
            })
        })
        .collect()
}

// ---------------------------------------------------------------- Create translation key

/// One change of the "Create translation key" fix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogueEdit {
    pub path: String,
    /// The file does not exist yet.
    pub create: bool,
    /// Byte offset of the insertion (0 for new files).
    pub offset: usize,
    pub insert: String,
    /// 0-based line of the new key after the edit.
    pub line: u32,
}

fn yaml_scalar(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn yaml_key(k: &str) -> String {
    let plain = !k.is_empty()
        && !k.contains(": ")
        && !k.contains(" #")
        && !k.ends_with(':')
        && !k.starts_with([
            '!', '&', '*', '-', '?', '{', '}', '[', ']', ',', '#', '|', '>', '@', '%', '\'', '"',
            '`', ' ',
        ]);
    if plain {
        k.to_string()
    } else {
        yaml_scalar(k)
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn php_quote(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// Offset of the start of line `n` (or the end of the text).
fn line_offset(text: &str, n: u32) -> usize {
    if n == 0 {
        return 0;
    }
    text.match_indices('\n')
        .nth(n as usize - 1)
        .map(|(i, _)| i + 1)
        .unwrap_or(text.len())
}

/// Insert `key: value` into a YAML catalogue, nested under the longest existing parent
/// mapping when the file uses nested keys.
pub fn yaml_insert(text: &str, key: &str, value: &str) -> (usize, String, u32) {
    let nodes = yaml_tree(text);
    let segments: Vec<&str> = key.split('.').collect();
    let nested = nodes.iter().any(|n| n.value.is_none());
    let mut parent: Option<&YamlNode> = None;
    if nested {
        for k in (1..segments.len()).rev() {
            let want: Vec<String> = segments[..k].iter().map(|s| s.to_string()).collect();
            if let Some(n) = nodes.iter().find(|n| n.value.is_none() && n.path == want) {
                parent = Some(n);
                break;
            }
        }
    }
    let (rest, indent, at_line) = match parent {
        Some(p) => {
            let child = nodes
                .iter()
                .find(|n| n.path.len() == p.path.len() + 1 && n.path.starts_with(&p.path))
                .map(|n| n.indent)
                .unwrap_or(p.indent + 4);
            (&segments[p.path.len()..], child, p.end_line + 1)
        }
        None => (&segments[..], 0, u32::MAX),
    };
    let step = nodes
        .iter()
        .find(|n| n.indent > 0)
        .map(|n| n.indent)
        .unwrap_or(4);
    let mut insert = String::new();
    let parts: Vec<&str> = if nested { rest.to_vec() } else { vec![key] };
    let joined;
    let parts = if parts.is_empty() {
        joined = [key];
        &joined[..]
    } else {
        &parts[..]
    };
    for (i, seg) in parts.iter().enumerate() {
        let pad = " ".repeat(indent + i * step);
        if i + 1 == parts.len() {
            insert.push_str(&format!("{pad}{}: {}\n", yaml_key(seg), yaml_scalar(value)));
        } else {
            insert.push_str(&format!("{pad}{}:\n", yaml_key(seg)));
        }
    }
    let offset = if at_line == u32::MAX {
        text.len()
    } else {
        line_offset(text, at_line)
    };
    if offset == text.len() && !text.is_empty() && !text.ends_with('\n') {
        insert.insert(0, '\n');
    }
    let line = text[..offset].matches('\n').count() as u32
        + u32::from(insert.starts_with('\n'))
        + parts.len() as u32
        - 1;
    (offset, insert, line)
}

fn xliff_insert(text: &str, key: &str, value: &str) -> (usize, String, u32) {
    let v2 = text.contains("urn:oasis:names:tc:xliff:document:2");
    let (close, unit_tag) = if v2 {
        ("</file>", "<unit")
    } else {
        ("</body>", "<trans-unit")
    };
    let offset = text
        .rfind(close)
        .map(|i| text[..i].rfind('\n').map(|n| n + 1).unwrap_or(i))
        .unwrap_or(text.len());
    let indent = text
        .rfind(unit_tag)
        .map(|i| {
            let ls = text[..i].rfind('\n').map(|n| n + 1).unwrap_or(0);
            text[ls..i].to_string()
        })
        .unwrap_or_else(|| "      ".into());
    let k = xml_escape(key);
    let v = xml_escape(value);
    let insert = if v2 {
        format!(
            "{indent}<unit id=\"{k}\" name=\"{k}\">\n{indent}  <segment>\n{indent}    <source>{k}</source>\n{indent}    <target>{v}</target>\n{indent}  </segment>\n{indent}</unit>\n"
        )
    } else {
        format!(
            "{indent}<trans-unit id=\"{k}\" resname=\"{k}\">\n{indent}  <source>{k}</source>\n{indent}  <target>{v}</target>\n{indent}</trans-unit>\n"
        )
    };
    let line = text[..offset].matches('\n').count() as u32;
    (offset, insert, line)
}

fn php_insert(text: &str, key: &str, value: &str) -> (usize, String, u32) {
    let end = text
        .rfind("];")
        .or_else(|| text.rfind(");"))
        .unwrap_or(text.len());
    let ls = text[..end].rfind('\n').map(|n| n + 1).unwrap_or(end);
    let before = text[..ls].trim_end();
    let mut insert = String::new();
    let mut offset = ls;
    if !before.ends_with(',') && !before.ends_with('[') && !before.ends_with('(') {
        // Add the missing comma after the last element.
        offset = before.len();
        insert.push_str(",\n");
    } else if ls == end {
        insert.push('\n');
    }
    insert.push_str(&format!(
        "    {} => {},\n",
        php_quote(key),
        php_quote(value)
    ));
    let line = text[..offset].matches('\n').count() as u32
        + u32::from(insert.starts_with(",\n") || insert.starts_with('\n'));
    (offset, insert, line)
}

fn json_insert(text: &str, key: &str, value: &str) -> (usize, String, u32) {
    let end = text.rfind('}').unwrap_or(text.len());
    let before = text[..end].trim_end();
    let empty = before.ends_with('{');
    let offset = before.len();
    let k = serde_json::to_string(key).unwrap_or_default();
    let v = serde_json::to_string(value).unwrap_or_default();
    let insert = if empty {
        format!("\n    {k}: {v}\n")
    } else {
        format!(",\n    {k}: {v}")
    };
    let line = text[..offset].matches('\n').count() as u32 + 1;
    (offset, insert, line)
}

/// The edits that add `key` to `domain` in each of `locales`: into the existing catalogue,
/// else into a new `translations/<domain>.<locale>.<ext>` in the project's main format.
/// `read` returns the current content of a catalogue.
pub fn create_key_edits(
    tr: &Translations,
    domain: &str,
    key: &str,
    locales: &[String],
    read: &dyn Fn(&str) -> Option<String>,
) -> Vec<CatalogueEdit> {
    let mut counts: BTreeMap<TranslationFormat, usize> = BTreeMap::new();
    for f in &tr.files {
        *counts.entry(f.format).or_default() += 1;
    }
    let main = counts
        .iter()
        .max_by_key(|(_, n)| **n)
        .map(|(f, _)| *f)
        .unwrap_or(TranslationFormat::Yaml);
    let mut out = Vec::new();
    for locale in locales {
        let existing = tr
            .files
            .iter()
            .filter(|f| f.domain == domain && f.locale == *locale)
            .min_by_key(|f| f.icu);
        let value = key.to_string();
        match existing.and_then(|f| read(&f.path).map(|t| (f, t))) {
            Some((f, text)) => {
                let (offset, insert, line) = match f.format {
                    TranslationFormat::Yaml => yaml_insert(&text, key, &value),
                    TranslationFormat::Xliff => xliff_insert(&text, key, &value),
                    TranslationFormat::Php => php_insert(&text, key, &value),
                    TranslationFormat::Json => json_insert(&text, key, &value),
                };
                out.push(CatalogueEdit {
                    path: f.path.clone(),
                    create: false,
                    offset,
                    insert,
                    line,
                });
            }
            None => {
                let path = format!(
                    "{}/{domain}.{locale}.{}",
                    tr.directory.trim_end_matches('/'),
                    main.extension()
                );
                let skeleton = match main {
                    TranslationFormat::Yaml => String::new(),
                    TranslationFormat::Xliff => format!(
                        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<xliff xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" version=\"1.2\">\n  <file source-language=\"{}\" target-language=\"{locale}\" datatype=\"plaintext\" original=\"file.ext\">\n    <body>\n    </body>\n  </file>\n</xliff>\n",
                        tr.default_locale
                    ),
                    TranslationFormat::Php => "<?php\n\nreturn [\n];\n".to_string(),
                    TranslationFormat::Json => "{\n}\n".to_string(),
                };
                let (offset, insert, line) = match main {
                    TranslationFormat::Yaml => {
                        // New YAML catalogues are nested, like Symfony's own.
                        let segments: Vec<&str> = key.split('.').collect();
                        let mut s = String::new();
                        for (i, seg) in segments.iter().enumerate() {
                            let pad = " ".repeat(i * 4);
                            if i + 1 == segments.len() {
                                s.push_str(&format!(
                                    "{pad}{}: {}\n",
                                    yaml_key(seg),
                                    yaml_scalar(&value)
                                ));
                            } else {
                                s.push_str(&format!("{pad}{}:\n", yaml_key(seg)));
                            }
                        }
                        (0, s, segments.len() as u32 - 1)
                    }
                    TranslationFormat::Xliff => xliff_insert(&skeleton, key, &value),
                    TranslationFormat::Php => php_insert(&skeleton, key, &value),
                    TranslationFormat::Json => json_insert(&skeleton, key, &value),
                };
                let mut content = skeleton.clone();
                content.insert_str(offset, &insert);
                out.push(CatalogueEdit {
                    path,
                    create: true,
                    offset: 0,
                    insert: content,
                    line,
                });
            }
        }
    }
    out
}

/// Apply an edit to the current content of its file (empty for new files).
pub fn apply_edit(current: &str, edit: &CatalogueEdit) -> String {
    if edit.create {
        return edit.insert.clone();
    }
    let at = edit.offset.min(current.len());
    let mut s = String::with_capacity(current.len() + edit.insert.len());
    s.push_str(&current[..at]);
    s.push_str(&edit.insert);
    s.push_str(&current[at..]);
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fixture, scan as scan_project, Config};

    const YAML: &str = "# Shop messages\norder:\n    title: 'Your orders'\n    status:\n        paid: Paid\n        open: \"Open\"\n    empty: >\n        You have no\n        orders yet.\ncheckout.pay: 'Pay now'\n";

    #[test]
    fn yaml_catalogues_are_flattened_with_lines() {
        let e = parse_catalogue(TranslationFormat::Yaml, YAML);
        let keys: Vec<(&str, &str, u32)> = e
            .iter()
            .map(|(k, v, l, _)| (k.as_str(), v.as_str(), *l))
            .collect();
        assert_eq!(
            keys,
            vec![
                ("order.title", "Your orders", 2),
                ("order.status.paid", "Paid", 4),
                ("order.status.open", "Open", 5),
                ("order.empty", "You have no orders yet.", 6),
                ("checkout.pay", "Pay now", 9),
            ]
        );
    }

    #[test]
    fn xliff_php_and_json_catalogues() {
        let xlf = r#"<?xml version="1.0"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file source-language="en" target-language="de" datatype="plaintext" original="file.ext">
    <body>
      <trans-unit id="a1" resname="order.title">
        <source>order.title</source>
        <target>Ihre Bestellungen</target>
      </trans-unit>
      <trans-unit id="a2">
        <source>Symfony &amp; Twig</source>
        <target><![CDATA[Symfony & Twig]]></target>
      </trans-unit>
    </body>
  </file>
</xliff>
"#;
        let e = parse_catalogue(TranslationFormat::Xliff, xlf);
        assert_eq!(e[0].0, "order.title");
        assert_eq!(e[0].1, "Ihre Bestellungen");
        assert_eq!(e[0].2, 4);
        assert_eq!(e[1].0, "Symfony & Twig");

        let php = "<?php\n\nreturn [\n    'order' => [\n        'title' => 'Bestellungen',\n    ],\n    'it\\'s' => \"ok\",\n];\n";
        let e = parse_catalogue(TranslationFormat::Php, php);
        assert_eq!(e[0], ("order.title".into(), "Bestellungen".into(), 4, 8));
        assert_eq!(e[1].0, "it's");

        let json = "{\n  \"order\": {\"title\": \"Commandes\"},\n  \"pay\": \"Payer\"\n}\n";
        let e = parse_catalogue(TranslationFormat::Json, json);
        assert_eq!(e[0].0, "order.title");
        assert_eq!(e[1], ("pay".into(), "Payer".into(), 2, 2));
    }

    #[test]
    fn file_names() {
        assert_eq!(
            parse_file_name("messages+intl-icu.fr.yaml"),
            Some((
                "messages".into(),
                "fr".into(),
                TranslationFormat::Yaml,
                true
            ))
        );
        assert_eq!(
            parse_file_name("validators.de.xlf").map(|x| x.0),
            Some("validators".into())
        );
        assert!(parse_file_name("README.md").is_none());
    }

    fn at(src: &str) -> (String, usize) {
        let pos = src.find('¦').unwrap();
        (src.replacen('¦', "", 1), pos)
    }

    #[test]
    fn references_in_php_and_twig() {
        let (src, pos) = at("$this->translator->trans('order.ti¦tle', [], 'admin');");
        let r = reference_at("src/X.php", &src, pos).unwrap();
        assert_eq!(
            (r.kind, r.value.as_str(), r.domain.as_str()),
            (TransRefKind::Key, "order.title", "admin")
        );

        let (src, pos) = at("$t->trans('x', domain: 'adm¦')");
        assert_eq!(
            reference_at("src/X.php", &src, pos).unwrap().kind,
            TransRefKind::Domain
        );

        let (src, pos) = at("return t('order.st¦');");
        let r = reference_at("src/X.php", &src, pos).unwrap();
        assert_eq!(
            (r.prefix.as_str(), r.domain.as_str()),
            ("order.st", "messages")
        );

        let (src, pos) =
            at("{% trans_default_domain 'shop' %}\n<h1>{{ 'order.ti¦tle'|trans }}</h1>");
        let r = reference_at("t.html.twig", &src, pos).unwrap();
        assert_eq!(r.domain, "shop");

        let (src, pos) = at("{{ 'order.title'|trans({'%n%': 1}, 'adm¦in') }}");
        assert_eq!(
            reference_at("t.html.twig", &src, pos).unwrap().kind,
            TransRefKind::Domain
        );

        let (src, pos) = at("{{ 'order.ti¦tle'|trans({}, 'admin') }}");
        assert_eq!(
            reference_at("t.html.twig", &src, pos).unwrap().domain,
            "admin"
        );

        let (src, pos) = at("{% trans %}order.ti¦tle{% endtrans %}");
        assert_eq!(
            reference_at("t.html.twig", &src, pos).unwrap().value,
            "order.title"
        );

        let (src, pos) = at("$this->render('order/in¦dex.html.twig')");
        assert!(reference_at("src/X.php", &src, pos).is_none());

        let refs = references(
            "t.html.twig",
            "{{ 'a'|trans }} {{ 'b'|upper }} {% trans from 'admin' %}c{% endtrans %} {{ t('d') }}",
        );
        let keys: Vec<(&str, &str)> = refs
            .iter()
            .filter(|r| r.kind == TransRefKind::Key)
            .map(|r| (r.value.as_str(), r.domain.as_str()))
            .collect();
        assert_eq!(
            keys,
            vec![("a", "messages"), ("c", "admin"), ("d", "messages")]
        );
    }

    fn project() -> tempfile::TempDir {
        let dir = fixture::project();
        let r = dir.path();
        fixture::write(r, "config/packages/translation.yaml", "framework:\n    default_locale: en\n    translator:\n        default_path: '%kernel.project_dir%/translations'\n");
        fixture::write(r, "translations/messages.en.yaml", YAML);
        fixture::write(
            r,
            "translations/messages.de.yaml",
            "order:\n    title: Ihre Bestellungen\n",
        );
        fixture::write(r, "translations/admin.en.xlf", "<?xml version=\"1.0\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\">\n  <file source-language=\"en\" datatype=\"plaintext\" original=\"file.ext\">\n    <body>\n      <trans-unit id=\"1\" resname=\"dashboard.title\">\n        <source>dashboard.title</source>\n        <target>Dashboard</target>\n      </trans-unit>\n    </body>\n  </file>\n</xliff>\n");
        dir
    }

    #[test]
    fn index_completion_goto_problems_and_folds() {
        let dir = project();
        let snap = scan_project(dir.path(), &Config::default());
        let tr = &snap.translations;
        assert_eq!(tr.locales(), vec!["en", "de"]);
        assert_eq!(tr.domains(), vec!["admin", "messages"]);
        assert_eq!(tr.text("messages", "order.title"), Some("Your orders"));

        let (src, pos) = at("$translator->trans('order.st¦')");
        let list = complete(&snap, "src/X.php", &src, pos).unwrap();
        let labels: Vec<&str> = list.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["order.status.open", "order.status.paid"]);
        assert_eq!(list.items[1].detail, "Paid");

        let (src, pos) = at("{{ 'x'|trans({}, '¦') }}");
        let list = complete(&snap, "t.html.twig", &src, pos).unwrap();
        assert_eq!(list.items[0].label, "admin");

        let (src, pos) = at("$translator->trans('order.ti¦tle')");
        let locs = goto(&snap, "src/X.php", &src, pos).unwrap();
        assert_eq!(locs[0].path, "translations/messages.en.yaml");
        assert_eq!(locs[0].line, 2);
        assert_eq!(locs[1].path, "translations/messages.de.yaml");

        let src = "<?php\n$t->trans('order.title');\n$t->trans('order.status.paid');\n$t->trans('nope');\n$t->trans('dashboard.title', [], 'admin');\n$t->trans($dynamic);";
        let p = problems(&snap, "src/X.php", src);
        assert_eq!(p.len(), 2, "{p:?}");
        assert_eq!(p[0].severity, ProblemSeverity::Weak);
        assert_eq!(p[0].locales, vec!["de"]);
        assert_eq!(p[1].severity, ProblemSeverity::Warning);
        assert_eq!(
            p[1].message,
            "Missing translation key 'nope' in domain 'messages'"
        );
        assert_eq!(p[1].locales, vec!["de", "en"]);

        let f = folds(
            &snap,
            "templates/x.html.twig",
            "<h1>{{ 'order.empty'|trans }}</h1>",
        );
        assert_eq!(f[0].text, "You have no orders yet.");
        assert_eq!((f[0].from, f[0].to), (7, 20));
    }

    #[test]
    fn create_key_in_existing_and_new_catalogues() {
        let dir = project();
        let snap = scan_project(dir.path(), &Config::default());
        let root = dir.path().to_path_buf();
        let read = move |p: &str| std::fs::read_to_string(root.join(p)).ok();
        let edits = create_key_edits(
            &snap.translations,
            "messages",
            "order.status.shipped",
            &["en".into(), "fr".into()],
            &read,
        );
        let en = apply_edit(&read("translations/messages.en.yaml").unwrap(), &edits[0]);
        assert!(
            en.contains(
                "        open: \"Open\"\n        shipped: 'order.status.shipped'\n    empty:"
            ),
            "{en}"
        );
        let added = parse_catalogue(TranslationFormat::Yaml, &en);
        let shipped = added
            .iter()
            .find(|e| e.0 == "order.status.shipped")
            .unwrap();
        assert_eq!(shipped.2, edits[0].line);
        assert!(edits[1].create);
        assert_eq!(edits[1].path, "translations/messages.fr.yaml");
        assert_eq!(
            edits[1].insert,
            "order:\n    status:\n        shipped: 'order.status.shipped'\n"
        );
        assert_eq!(edits[1].line, 2);

        let edits = create_key_edits(
            &snap.translations,
            "admin",
            "menu.users",
            &["en".into()],
            &read,
        );
        let xlf = apply_edit(&read("translations/admin.en.xlf").unwrap(), &edits[0]);
        let units = parse_catalogue(TranslationFormat::Xliff, &xlf);
        assert_eq!(units[1].0, "menu.users");
        assert_eq!(units[1].2, edits[0].line);

        let (o, s, l) = php_insert("<?php\nreturn [\n    'a' => 'b'\n];\n", "c", "d");
        let php = apply_edit(
            "<?php\nreturn [\n    'a' => 'b'\n];\n",
            &CatalogueEdit {
                path: String::new(),
                create: false,
                offset: o,
                insert: s,
                line: l,
            },
        );
        let e = parse_catalogue(TranslationFormat::Php, &php);
        assert_eq!(e[1], ("c".into(), "d".into(), l, 4));

        let (o, s, l) = json_insert("{\n    \"a\": \"b\"\n}\n", "c", "d");
        let json = apply_edit(
            "{\n    \"a\": \"b\"\n}\n",
            &CatalogueEdit {
                path: String::new(),
                create: false,
                offset: o,
                insert: s,
                line: l,
            },
        );
        let e = parse_catalogue(TranslationFormat::Json, &json);
        assert_eq!(e[1].2, l);
    }
}
