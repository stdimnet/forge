//! Template delimiters per (template language, data language) pair, like Smarty's
//! `left_delimiter` or Twig's `tag_variable` / `tag_block` / `tag_comment` lexer options.
//!
//! Each configurable template language declares its delimiter fields with their built-in
//! defaults ([`CONFIGURABLE_TEMPLATES`]). The `templates.delimiters` setting holds only the
//! differences, one entry per pair:
//!
//! ```toml
//! { template = "smarty", language = "latex", left = "[[", right = "]]" }
//! { template = "twig", language = "latex", variableStart = '\VAR{', variableEnd = "}", … }
//! ```
//!
//! Nothing here knows about a particular pair: the LaTeX entries ship as default data in the
//! core manifest (`app/src-tauri/plugins/core.toml`).

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::data::{DataLanguage, DATA_LANGUAGES};

/// The setting with the delimiter entries.
pub const DELIMITERS_SETTING: &str = "templates.delimiters";

/// One delimiter of a template language, e.g. Twig's variable start `{{`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DelimiterField {
    /// Key in the setting's entries, e.g. `variableStart`.
    pub key: &'static str,
    pub label: &'static str,
    pub default: &'static str,
}

/// A template language whose delimiters can be changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ConfigurableTemplate {
    /// File type id, as used in the entries' `template` field.
    pub id: &'static str,
    pub label: &'static str,
    /// Pairs of fields: start and end of each tag kind, in display order.
    pub fields: &'static [DelimiterField],
}

const fn field(key: &'static str, label: &'static str, default: &'static str) -> DelimiterField {
    DelimiterField {
        key,
        label,
        default,
    }
}

/// Twig and Jinja: `tag_variable`, `tag_block` and `tag_comment` (Jinja's `*_start_string`).
const TWIG_FIELDS: &[DelimiterField] = &[
    field("variableStart", "Variable start", "{{"),
    field("variableEnd", "Variable end", "}}"),
    field("blockStart", "Block start", "{%"),
    field("blockEnd", "Block end", "%}"),
    field("commentStart", "Comment start", "{#"),
    field("commentEnd", "Comment end", "#}"),
];

const SMARTY_FIELDS: &[DelimiterField] = &[
    field("left", "Left delimiter", "{"),
    field("right", "Right delimiter", "}"),
];

const GO_FIELDS: &[DelimiterField] = &[
    field("left", "Left delimiter", "{{"),
    field("right", "Right delimiter", "}}"),
];

/// Template languages with configurable delimiters, in display order.
pub const CONFIGURABLE_TEMPLATES: &[ConfigurableTemplate] = &[
    ConfigurableTemplate {
        id: "smarty",
        label: "Smarty",
        fields: SMARTY_FIELDS,
    },
    ConfigurableTemplate {
        id: "twig",
        label: "Twig",
        fields: TWIG_FIELDS,
    },
    ConfigurableTemplate {
        id: "jinja",
        label: "Jinja2",
        fields: TWIG_FIELDS,
    },
    ConfigurableTemplate {
        id: "gotemplate",
        label: "Go Template",
        fields: GO_FIELDS,
    },
];

/// Delimiter values by field key.
pub type Delimiters = BTreeMap<String, String>;

/// The configurable template language with `id`.
pub fn configurable_template(id: &str) -> Option<&'static ConfigurableTemplate> {
    CONFIGURABLE_TEMPLATES.iter().find(|t| t.id == id)
}

/// A string field of an entry; anything that is not a table of strings has none.
fn text<'a>(entry: &'a Value, key: &str) -> Option<&'a str> {
    entry.get(key).and_then(Value::as_str).map(str::trim)
}

/// The delimiters of `template` in files of `data_language`: the defaults, overridden by the
/// non-empty fields of the matching entry. `None` when the template's delimiters are fixed.
pub fn resolve_delimiters(
    template: &str,
    data_language: &str,
    entries: &[Value],
) -> Option<Delimiters> {
    let spec = configurable_template(template)?;
    let entry = entries.iter().find(|e| {
        text(e, "template") == Some(template)
            && text(e, "language").is_some_and(|l| l.eq_ignore_ascii_case(data_language))
    });
    Some(
        spec.fields
            .iter()
            .map(|f| {
                let value = entry
                    .and_then(|e| text(e, f.key))
                    .filter(|v| !v.is_empty())
                    .unwrap_or(f.default);
                (f.key.to_string(), value.to_string())
            })
            .collect(),
    )
}

/// What the delimiter settings editor offers: the configurable templates and the data
/// languages.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DelimiterCatalog {
    pub templates: &'static [ConfigurableTemplate],
    pub data_languages: &'static [DataLanguage],
}

/// The catalog of the `filetypes_template_delimiters` IPC command.
pub fn delimiter_catalog() -> DelimiterCatalog {
    DelimiterCatalog {
        templates: CONFIGURABLE_TEMPLATES,
        data_languages: DATA_LANGUAGES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn get<'a>(d: &'a Option<Delimiters>, key: &str) -> &'a str {
        d.as_ref().unwrap()[key].as_str()
    }

    fn latex_entries() -> Vec<Value> {
        vec![
            json!({ "template": "smarty", "language": "latex", "left": "[[", "right": "]]" }),
            json!({
                "template": "twig", "language": "latex",
                "variableStart": "\\VAR{", "variableEnd": "}",
                "blockStart": "\\BLOCK{", "blockEnd": "}",
                "commentStart": "\\#{", "commentEnd": "}"
            }),
        ]
    }

    #[test]
    fn uses_the_language_defaults_without_an_entry() {
        let d = resolve_delimiters("smarty", "html", &[]);
        assert_eq!(get(&d, "left"), "{");
        assert_eq!(get(&d, "right"), "}");
        let d = resolve_delimiters("twig", "latex", &latex_entries()[..1]);
        assert_eq!(get(&d, "variableStart"), "{{");
        assert_eq!(get(&d, "blockStart"), "{%");
        assert_eq!(get(&d, "commentStart"), "{#");
        assert_eq!(
            get(&resolve_delimiters("gotemplate", "html", &[]), "left"),
            "{{"
        );
    }

    #[test]
    fn applies_the_entry_of_the_pair() {
        let entries = latex_entries();
        let d = resolve_delimiters("smarty", "latex", &entries);
        assert_eq!((get(&d, "left"), get(&d, "right")), ("[[", "]]"));
        assert_eq!(
            get(&resolve_delimiters("smarty", "html", &entries), "left"),
            "{"
        );
        let d = resolve_delimiters("twig", "LaTeX", &entries);
        assert_eq!(get(&d, "variableStart"), "\\VAR{");
        assert_eq!(get(&d, "blockStart"), "\\BLOCK{");
        assert_eq!(get(&d, "commentEnd"), "}");
        // Jinja has its own entries; Twig's do not apply to it.
        assert_eq!(
            get(
                &resolve_delimiters("jinja", "latex", &entries),
                "blockStart"
            ),
            "{%"
        );
    }

    #[test]
    fn fills_missing_empty_and_malformed_fields_from_the_defaults() {
        let entries = vec![
            json!("smarty"),
            json!({ "template": "twig", "language": "sql", "blockStart": " <% ", "blockEnd": "", "commentStart": 3 }),
        ];
        let d = resolve_delimiters("twig", "sql", &entries).unwrap();
        let expected: Delimiters = [
            ("variableStart", "{{"),
            ("variableEnd", "}}"),
            ("blockStart", "<%"),
            ("blockEnd", "%}"),
            ("commentStart", "{#"),
            ("commentEnd", "#}"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        assert_eq!(d, expected);
    }

    #[test]
    fn fixed_templates_have_no_delimiters() {
        assert_eq!(resolve_delimiters("latte", "html", &latex_entries()), None);
        assert_eq!(resolve_delimiters("php", "html", &[]), None);
    }

    #[test]
    fn catalog_lists_templates_with_paired_fields() {
        let c = delimiter_catalog();
        assert_eq!(c.templates.len(), 4);
        assert!(c.templates.iter().all(|t| t.fields.len() % 2 == 0));
        let json = serde_json::to_value(c).unwrap();
        assert_eq!(json["templates"][1]["fields"][0]["key"], "variableStart");
        assert_eq!(json["dataLanguages"][2]["label"], "LaTeX");
    }
}
