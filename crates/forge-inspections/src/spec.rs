//! What packs contribute: inspections and the mapping of external tools' problems onto them.
//!
//! ```toml
//! [[contributions.inspections]]
//! id = "PhpUnusedAssignment"
//! name = "Unused assignment"
//! group = "PHP/Data flow"
//! language = "php"
//! severity = "WARNING"
//! description = "Reports assignments whose value is never read."
//!
//! [[contributions.inspectionMappings]]
//! source = "clippy"
//! inspection = "RsClippy"
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// An inspection (PHPStorm's inspection tool). Its `id` is the short name suppression
/// comments and profiles use (`PhpUnusedAssignment`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectionSpec {
    pub id: String,
    /// Display name (`Unused assignment`).
    pub name: String,
    /// The group path in the Settings tree, `/`-separated; the first segment is the
    /// language or area (`PHP/Data flow`, `General`).
    pub group: String,
    /// The language whose comments suppress it (`php`, `javascript`, `python`); empty for
    /// any language.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub language: String,
    /// The default severity level (`WARNING`).
    #[serde(default = "default_severity")]
    pub severity: String,
    /// Enabled in a fresh profile.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Shown on the Settings page (plain text, may contain simple HTML).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<OptionSpec>,
    /// Its quick fixes are safe for Code Cleanup.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cleanup: bool,
    /// Problems come from an external tool that reports its own severities: they keep them
    /// until the profile sets one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub external: bool,
    /// Runs in Inspect Code / the command-line inspector (`false`: editor-only, e.g.
    /// problems that need a running language server or tool).
    #[serde(default = "yes", skip_serializing_if = "Clone::clone")]
    pub batch: bool,
}

fn default_severity() -> String {
    crate::severity::WARNING.into()
}

fn yes() -> bool {
    true
}

/// The type of an inspection option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OptionKind {
    Bool,
    Int,
    String,
    Enum,
}

/// A choice of an enum option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionChoice {
    pub value: String,
    pub label: String,
}

/// An option of an inspection (shown under its description).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionSpec {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: OptionKind,
    pub default: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<OptionChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
}

impl OptionSpec {
    /// `value` coerced to the option's type, or `None` when it does not fit.
    pub fn coerce(&self, value: &Value) -> Option<Value> {
        match self.kind {
            OptionKind::Bool => match value {
                Value::Bool(_) => Some(value.clone()),
                Value::String(s) if s == "true" || s == "false" => Some(Value::Bool(s == "true")),
                _ => None,
            },
            OptionKind::Int => {
                let n = match value {
                    Value::Number(n) => n.as_i64(),
                    Value::String(s) => s.trim().parse().ok(),
                    _ => None,
                }?;
                let n = n
                    .max(self.min.unwrap_or(i64::MIN))
                    .min(self.max.unwrap_or(i64::MAX));
                Some(Value::from(n))
            }
            OptionKind::String => match value {
                Value::String(_) => Some(value.clone()),
                Value::Number(_) | Value::Bool(_) => Some(Value::String(value.to_string())),
                _ => None,
            },
            OptionKind::Enum => {
                let s = value.as_str()?;
                self.choices
                    .iter()
                    .any(|c| c.value == s)
                    .then(|| value.clone())
            }
        }
    }
}

/// Maps problems of an external tool onto an inspection: problems published with this
/// `source` (the editor's diagnostics source: `lsp`, `clippy`, `golangci-lint`, ...) and,
/// when given, this `code` (a trailing `*` matches a prefix).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolMapping {
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub inspection: String,
}

impl ToolMapping {
    /// How well it matches (`None`: not at all; higher is more specific).
    pub fn matches(&self, source: &str, code: Option<&str>) -> Option<usize> {
        if self.source != source {
            return None;
        }
        match (&self.code, code) {
            (None, _) => Some(0),
            (Some(want), Some(code)) => match want.strip_suffix('*') {
                Some(prefix) => code.starts_with(prefix).then_some(1 + prefix.len()),
                None => (want == code).then_some(usize::MAX),
            },
            (Some(_), None) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn options_coerce_values() {
        let int = OptionSpec {
            id: "min".into(),
            label: "Minimum".into(),
            kind: OptionKind::Int,
            default: json!(10),
            choices: vec![],
            min: Some(1),
            max: Some(100),
        };
        assert_eq!(int.coerce(&json!("500")), Some(json!(100)));
        assert_eq!(int.coerce(&json!(true)), None);
        let e = OptionSpec {
            kind: OptionKind::Enum,
            choices: vec![OptionChoice {
                value: "a".into(),
                label: "A".into(),
            }],
            ..int
        };
        assert_eq!(e.coerce(&json!("a")), Some(json!("a")));
        assert_eq!(e.coerce(&json!("b")), None);
    }

    #[test]
    fn mappings_prefer_specific_codes() {
        let any = ToolMapping {
            source: "lsp".into(),
            code: None,
            inspection: "A".into(),
        };
        let prefix = ToolMapping {
            source: "lsp".into(),
            code: Some("P10*".into()),
            inspection: "B".into(),
        };
        assert_eq!(any.matches("lsp", Some("P1013")), Some(0));
        assert!(prefix.matches("lsp", Some("P1013")) > any.matches("lsp", Some("P1013")));
        assert_eq!(prefix.matches("lsp", Some("X1")), None);
        assert_eq!(prefix.matches("clippy", Some("P10")), None);
    }

    #[test]
    fn specs_parse_from_toml_like_json() {
        let s: InspectionSpec = serde_json::from_value(json!({
            "id": "X", "name": "X", "group": "PHP/General"
        }))
        .unwrap();
        assert!(s.enabled && s.batch && !s.external);
        assert_eq!(s.severity, "WARNING");
    }
}
