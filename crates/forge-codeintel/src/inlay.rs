//! Inlay hints (Settings | Editor | Inlay Hints): the hints language servers report
//! (`textDocument/inlayHint`), sorted into PHPStorm's groups and filtered by the settings.
//!
//! Servers only say whether a hint names a parameter or shows a type. Forge tells the type
//! hints apart from the text around them: a hint right after `)` is a return type, any other
//! type hint is the inferred type of a variable or property.

use serde::{Deserialize, Serialize};

/// LSP `InlayHintKind`.
const LSP_TYPE: u8 = 1;
const LSP_PARAMETER: u8 = 2;

/// Setting keys of the Inlay Hints page (see `app/src-tauri/plugins/core.toml`).
pub const PARAMETER_NAMES_KEY: &str = "editor.inlayHints.parameterNames";
pub const TYPES_KEY: &str = "editor.inlayHints.types";
pub const RETURN_TYPES_KEY: &str = "editor.inlayHints.returnTypes";
/// Per-language switches, `<prefix><group>` with the groups of [`language_group`].
pub const LANGUAGE_KEY_PREFIX: &str = "editor.inlayHints.languages.";

/// A hint as a server reported it (label parts joined), with LSP positions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RawInlayHint {
    pub line: u32,
    /// UTF-16 column.
    pub character: u32,
    pub label: String,
    /// LSP `InlayHintKind`: 1 = type, 2 = parameter.
    pub kind: Option<u8>,
    pub padding_left: bool,
    pub padding_right: bool,
    pub tooltip: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InlayGroup {
    ParameterName,
    VariableType,
    ReturnType,
    Other,
}

/// A hint as the editor draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorInlayHint {
    pub line: u32,
    pub character: u32,
    pub label: String,
    pub group: InlayGroup,
    pub padding_left: bool,
    pub padding_right: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
}

/// Which hints to show (the settings of the page plus the file's language switch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InlayFilter {
    pub language_enabled: bool,
    pub parameter_names: bool,
    pub types: bool,
    pub return_types: bool,
}

impl Default for InlayFilter {
    fn default() -> Self {
        Self {
            language_enabled: true,
            parameter_names: true,
            types: false,
            return_types: true,
        }
    }
}

impl InlayFilter {
    /// The filter for a file of `language_id` from setting values (`get` returns the
    /// effective value of a key; missing values keep the defaults).
    pub fn from_settings(language_id: Option<&str>, get: impl Fn(&str) -> Option<bool>) -> Self {
        let d = Self::default();
        let group = language_group(language_id.unwrap_or(""));
        Self {
            language_enabled: get(&format!("{LANGUAGE_KEY_PREFIX}{group}")).unwrap_or(true),
            parameter_names: get(PARAMETER_NAMES_KEY).unwrap_or(d.parameter_names),
            types: get(TYPES_KEY).unwrap_or(d.types),
            return_types: get(RETURN_TYPES_KEY).unwrap_or(d.return_types),
        }
    }

    fn shows(&self, group: InlayGroup) -> bool {
        self.language_enabled
            && match group {
                InlayGroup::ParameterName => self.parameter_names,
                InlayGroup::VariableType => self.types,
                InlayGroup::ReturnType => self.return_types,
                InlayGroup::Other => self.parameter_names || self.types || self.return_types,
            }
    }
}

/// The language switch a file type belongs to: `php`, `javascript` (JavaScript, TypeScript
/// and their frameworks), `lua` or `other`.
pub fn language_group(language_id: &str) -> &'static str {
    match language_id {
        "php" | "blade" => "php",
        "javascript" | "typescript" | "jsx" | "tsx" | "javascriptreact" | "typescriptreact"
        | "vue" | "svelte" | "astro" => "javascript",
        "lua" => "lua",
        _ => "other",
    }
}

/// The group of a hint, from its kind and the text before it.
pub fn classify(hint: &RawInlayHint, text: &str) -> InlayGroup {
    match hint.kind {
        Some(LSP_PARAMETER) => InlayGroup::ParameterName,
        Some(LSP_TYPE) => {
            let before = text_before(text, hint.line, hint.character);
            if before.trim_end().ends_with(')') {
                InlayGroup::ReturnType
            } else {
                InlayGroup::VariableType
            }
        }
        // Servers that leave out the kind: `name:` labels name parameters.
        _ if hint.label.trim_end().ends_with(':') && !hint.label.starts_with(':') => {
            InlayGroup::ParameterName
        }
        _ => InlayGroup::Other,
    }
}

/// The text of `line` before UTF-16 column `character`.
fn text_before(text: &str, line: u32, character: u32) -> &str {
    let Some(l) = text.split('\n').nth(line as usize) else {
        return "";
    };
    let mut units = 0u32;
    for (i, c) in l.char_indices() {
        if units >= character {
            return &l[..i];
        }
        units += c.len_utf16() as u32;
    }
    l
}

/// The hints to draw: classified, filtered, sorted by position and without duplicates
/// (two servers reporting the same hint).
pub fn editor_hints(
    hints: Vec<RawInlayHint>,
    text: &str,
    filter: &InlayFilter,
) -> Vec<EditorInlayHint> {
    let mut out: Vec<EditorInlayHint> = hints
        .into_iter()
        .filter(|h| !h.label.trim().is_empty())
        .filter_map(|h| {
            let group = classify(&h, text);
            filter.shows(group).then(|| EditorInlayHint {
                line: h.line,
                character: h.character,
                label: h.label.trim().to_string(),
                group,
                padding_left: h.padding_left,
                padding_right: h.padding_right,
                tooltip: h.tooltip,
            })
        })
        .collect();
    out.sort_by(|a, b| (a.line, a.character, &a.label).cmp(&(b.line, b.character, &b.label)));
    out.dedup_by(|a, b| a.line == b.line && a.character == b.character && a.label == b.label);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hint(line: u32, character: u32, label: &str, kind: Option<u8>) -> RawInlayHint {
        RawInlayHint {
            line,
            character,
            label: label.into(),
            kind,
            ..RawInlayHint::default()
        }
    }

    const SRC: &str = "function total(a, b) {\n  const sum = add(1, null);\n}\n";

    #[test]
    fn classifies_by_kind_and_position() {
        assert_eq!(
            classify(&hint(1, 18, "x:", Some(2)), SRC),
            InlayGroup::ParameterName
        );
        // After `)`: the function's return type.
        assert_eq!(
            classify(&hint(0, 20, ": number", Some(1)), SRC),
            InlayGroup::ReturnType
        );
        // After a variable name: its type.
        assert_eq!(
            classify(&hint(1, 11, ": number", Some(1)), SRC),
            InlayGroup::VariableType
        );
        assert_eq!(
            classify(&hint(1, 18, "count:", None), SRC),
            InlayGroup::ParameterName
        );
        assert_eq!(classify(&hint(1, 18, "…", None), SRC), InlayGroup::Other);
    }

    #[test]
    fn counts_utf16_columns() {
        assert_eq!(text_before("é😀x)", 0, 3), "é😀");
        assert_eq!(text_before("ab", 5, 1), "");
    }

    #[test]
    fn filters_sorts_and_dedups() {
        let hints = vec![
            hint(1, 21, "b:", Some(2)),
            hint(1, 18, "a:", Some(2)),
            hint(1, 18, "a:", Some(2)),
            hint(1, 11, ": number", Some(1)),
            hint(0, 20, ": number", Some(1)),
            hint(0, 0, "  ", None),
        ];
        let filter = InlayFilter::default();
        let out = editor_hints(hints.clone(), SRC, &filter);
        let labels: Vec<_> = out.iter().map(|h| (h.line, h.label.as_str())).collect();
        // Variable types are off by default, like in PHPStorm.
        assert_eq!(labels, vec![(0, ": number"), (1, "a:"), (1, "b:")]);
        let off = InlayFilter {
            language_enabled: false,
            ..filter
        };
        assert!(editor_hints(hints, SRC, &off).is_empty());
    }

    #[test]
    fn reads_the_settings_of_the_files_language() {
        let get = |k: &str| match k {
            "editor.inlayHints.languages.lua" => Some(false),
            TYPES_KEY => Some(true),
            _ => None,
        };
        let lua = InlayFilter::from_settings(Some("lua"), get);
        assert!(!lua.language_enabled && lua.types && lua.parameter_names);
        assert!(InlayFilter::from_settings(Some("typescript"), get).language_enabled);
        assert_eq!(language_group("tsx"), "javascript");
        assert_eq!(language_group("yaml"), "other");
    }
}
