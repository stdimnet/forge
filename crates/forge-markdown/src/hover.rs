//! Language server hovers (Markdown as php-lsp & co. send it) split into the
//! parts of a PHPStorm-style documentation popup: owner FQCN, signature, description and
//! `Params:` / `Returns:` / `Throws:` sections.

use serde::{Deserialize, Serialize};

use crate::ast::{Block, Inline};
use crate::parse::{parse, parse_inline, Flavor};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoverSection {
    pub title: String,
    pub entries: Vec<Vec<Inline>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HoverDoc {
    /// Owner of the symbol, e.g. `App\Models\User` (empty for global functions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<String>,
    /// The PHP signature, highlighted by the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    pub body: Vec<Block>,
    pub sections: Vec<HoverSection>,
    /// Set (possibly empty) when the symbol is deprecated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<Vec<Inline>>,
}

pub(crate) fn section_title(tag: &str) -> String {
    match tag {
        "param" => "Params:".into(),
        "return" | "returns" => "Returns:".into(),
        "throws" => "Throws:".into(),
        "see" | "link" => "See also:".into(),
        "since" => "Since:".into(),
        "var" => "Type:".into(),
        "example" => "Example:".into(),
        _ => {
            let mut c = tag.chars();
            match c.next() {
                Some(f) => format!("{}{}:", f.to_uppercase(), c.as_str()),
                None => ":".into(),
            }
        }
    }
}

fn clean_signature(code: &str) -> String {
    let s = code.trim_start();
    let s = s.strip_prefix("<?php").unwrap_or(s).trim();
    let s = match s.strip_suffix('}') {
        Some(rest) if rest.trim_end().ends_with('{') => rest.trim_end().trim_end_matches('{'),
        _ => s,
    };
    s.trim().to_string()
}

/// `**Name**` / `__Name__` alone on a line.
fn header(line: &str) -> Option<&str> {
    let t = line.trim();
    for m in ["**", "__"] {
        if let Some(inner) = t.strip_prefix(m).and_then(|r| r.strip_suffix(m)) {
            if !inner.is_empty() {
                return Some(inner);
            }
        }
    }
    None
}

/// `_@param_ rest`, `@return rest`, `**@throws** rest`: (tag, rest).
fn tag_line(line: &str) -> Option<(String, &str)> {
    let t = line.trim_start();
    let t = t.trim_start_matches(['_', '*']);
    let rest = t.strip_prefix('@')?;
    let name_len = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .unwrap_or(rest.len());
    if name_len == 0 || !rest.as_bytes()[0].is_ascii_alphabetic() {
        return None;
    }
    let name = rest[..name_len].trim_end_matches('_');
    let after = rest[name_len..].trim_start_matches(['_', '*']);
    if !after.is_empty() && !after.starts_with(char::is_whitespace) {
        return None;
    }
    Some((name.to_ascii_lowercase(), after.trim_start()))
}

fn fence_of(line: &str) -> Option<(&str, &str)> {
    let t = line.trim_start();
    let ticks = t.len() - t.trim_start_matches('`').len();
    (ticks >= 3).then(|| (&t[..ticks], t[ticks..].trim()))
}

/// Split hover Markdown into the parts shown in the popup.
pub fn parse_hover(markdown: &str) -> HoverDoc {
    let mut doc = HoverDoc::default();
    let text = markdown.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut body: Vec<&str> = Vec::new();
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();
    let mut seen_content = false;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some((ticks, lang)) = fence_of(line) {
            let is_sig = doc.signature.is_none()
                && (lang.is_empty() || lang.eq_ignore_ascii_case("php"))
                && !lang.contains(char::is_whitespace);
            if is_sig {
                let mut code = Vec::new();
                i += 1;
                while i < lines.len() && !lines[i].trim().starts_with(ticks) {
                    code.push(lines[i]);
                    i += 1;
                }
                doc.signature = Some(clean_signature(&code.join("\n")));
                seen_content = true;
                i += 1;
                continue;
            }
            // Other code blocks stay in the body untouched.
            body.push(line);
            i += 1;
            while i < lines.len() {
                body.push(lines[i]);
                let end = lines[i].trim().starts_with(ticks);
                i += 1;
                if end {
                    break;
                }
            }
            continue;
        }
        if let Some(name) = header(line).filter(|_| !seen_content && doc.definition.is_none()) {
            let name = name.replace("\\\\", "\\");
            // Members show their class, classes their namespace, global functions nothing.
            doc.definition = Some(if let Some(i) = name.rfind("::") {
                name[..i].to_string()
            } else if let Some(i) = name.rfind('\\') {
                name[..i].to_string()
            } else {
                String::new()
            });
            seen_content = true;
            i += 1;
            continue;
        }
        if let Some((name, rest)) = tag_line(line) {
            let rest = rest.trim_end().trim_end_matches('\\').trim_end();
            if name == "deprecated" {
                doc.deprecated = Some(parse_inline(rest, Flavor::Docs));
                i += 1;
                continue;
            }
            let title = section_title(&name);
            match sections.iter_mut().find(|(t, _)| *t == title) {
                Some((_, entries)) => entries.push(rest.to_string()),
                None => sections.push((title, vec![rest.to_string()])),
            }
            i += 1;
            continue;
        }
        if !line.trim().is_empty() {
            seen_content = true;
        }
        body.push(line);
        i += 1;
    }
    let body: Vec<&str> = body
        .into_iter()
        .map(|l| match l.trim() {
            "---" | "***" | "___" => "",
            _ => l,
        })
        .collect();
    doc.body = parse(body.join("\n").trim(), Flavor::Docs);
    const ORDER: [&str; 6] = [
        "Type:",
        "Params:",
        "Returns:",
        "Throws:",
        "Since:",
        "See also:",
    ];
    let rank = |t: &str| ORDER.iter().position(|o| *o == t).unwrap_or(98);
    sections.sort_by_key(|(t, _)| rank(t));
    doc.sections = sections
        .into_iter()
        .map(|(title, entries)| HoverSection {
            title,
            entries: entries
                .iter()
                .map(|e| parse_inline(e, Flavor::Docs))
                .collect(),
        })
        .collect();
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTELEPHENSE_HOVER: &str = "__Illuminate\\Database\\Eloquent\\Concerns\\HasRelationships::hasMany__\n\n```php\n<?php\npublic function hasMany(string $related): HasMany { }\n```\n\nDefine a one-to-many relationship.\n\n_@param_ `class-string` `$related`  \n_@return_ `HasMany`  \n_@deprecated_ Use `hasOne` instead.";

    #[test]
    fn splits_owner_signature_description_and_tag_sections() {
        let doc = parse_hover(INTELEPHENSE_HOVER);
        assert_eq!(
            doc.definition.as_deref(),
            Some("Illuminate\\Database\\Eloquent\\Concerns\\HasRelationships")
        );
        assert_eq!(
            doc.signature.as_deref(),
            Some("public function hasMany(string $related): HasMany")
        );
        assert_eq!(
            doc.body,
            [Block::Paragraph {
                inline: vec![Inline::Text {
                    text: "Define a one-to-many relationship.".into()
                }]
            }]
        );
        let titles: Vec<&str> = doc.sections.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Params:", "Returns:"]);
        assert_eq!(
            Inline::text_of(&doc.sections[0].entries[0]),
            "class-string $related"
        );
        assert_eq!(
            Inline::text_of(doc.deprecated.as_deref().unwrap()),
            "Use hasOne instead."
        );
    }

    #[test]
    fn shows_the_namespace_for_classes_and_nothing_for_global_functions() {
        assert_eq!(
            parse_hover("__App\\Models\\User__").definition.as_deref(),
            Some("App\\Models")
        );
        assert_eq!(
            parse_hover("__array_map__\n\ntext").definition.as_deref(),
            Some("")
        );
    }

    #[test]
    fn keeps_other_code_blocks_in_the_body() {
        let doc = parse_hover("```php\nfunction f() { }\n```\n\n```js\nlet a = 1;\n```");
        assert_eq!(doc.signature.as_deref(), Some("function f()"));
        assert!(matches!(&doc.body[0], Block::Code { lang, .. } if lang == "js"));
    }
}
