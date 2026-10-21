//! The effective format of one file: indentation (code style of its language, EditorConfig,
//! or detected from the content), the line separator for new files, the on-save cleanup,
//! the right margin and visual guides.
//!
//! Precedence, as in PHPStorm: EditorConfig (when enabled) > indentation detected in the
//! file (when enabled) > the language's code style > the file type's default indent.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::editorconfig::{properties_for, EditorConfigFile, EditorConfigProps};
use crate::encoding::Encoding;
use crate::eol::LineSeparator;
use crate::indent::{detect_indent, IndentStyle};
use crate::save::{SaveOptions, StripMode};

/// Setting keys (see `app/src-tauri/plugins/core.toml`) that change a file's format. Code
/// style keys of languages (`<language>.codeStyle.*`) are read too.
pub const SETTING_KEYS: &[&str] = &[
    "editor.stripTrailingSpaces",
    "editor.ensureNewlineAtEof",
    "editor.codeStyle.lineSeparator",
    "editor.codeStyle.hardWrapAt",
    "editor.codeStyle.visualGuides",
    "editor.codeStyle.useEditorConfig",
    "editor.codeStyle.detectIndents",
];

/// Whether a changed setting can change the format of open files.
pub fn affects_format(key: &str) -> bool {
    SETTING_KEYS.contains(&key) || key.contains(".codeStyle.")
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FormatRequest {
    /// Workspace-relative path.
    pub path: String,
    /// Language id of the file type (`php`, `javascript`); selects `<id>.codeStyle.*`.
    pub language_id: Option<String>,
    /// The file's text, for indent detection.
    pub text: Option<String>,
    /// The file type's default indent (`"  "` for YAML) when its language has no code style.
    pub file_type_indent: Option<String>,
    /// Effective setting values by key (the store's snapshot, or just the relevant keys).
    pub settings: Map<String, Value>,
    /// `.editorconfig` files from the project root down to the file's folder.
    pub editorconfigs: Vec<EditorConfigFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IndentSource {
    CodeStyle,
    FileType,
    EditorConfig,
    Detected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Charset {
    pub encoding: Encoding,
    pub bom: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileFormat {
    /// What the editor indents with.
    pub indent: IndentStyle,
    pub indent_source: IndentSource,
    /// Status bar text, e.g. `4 spaces`, `Tab`.
    pub indent_label: String,
    /// The indentation the settings ask for (differs from `indent` when detected).
    pub configured_indent: IndentStyle,
    /// Settings page of the language's code style (`editor.code-style.php`), if any.
    pub code_style_page: Option<String>,
    /// Display name used in "Configure Indents for PHP…".
    pub language_name: Option<String>,
    /// Separator for new files (EditorConfig `end_of_line` or the code style setting).
    pub line_separator: LineSeparator,
    /// Separator EditorConfig demands for this file, if any.
    pub required_line_separator: Option<LineSeparator>,
    /// Encoding EditorConfig asks for (`charset`), if any.
    pub charset: Option<Charset>,
    pub save: SaveOptions,
    /// Right margin (columns).
    pub hard_wrap_at: u32,
    /// Columns of the vertical guides, sorted, without duplicates.
    pub visual_guides: Vec<u32>,
    /// EditorConfig properties applied to the file.
    pub editor_config: bool,
}

struct Get<'a>(&'a Map<String, Value>);

impl Get<'_> {
    fn bool(&self, key: &str, default: bool) -> bool {
        self.0.get(key).and_then(Value::as_bool).unwrap_or(default)
    }
    fn uint(&self, key: &str) -> Option<u64> {
        self.0
            .get(key)
            .and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f as u64)))
    }
    fn str(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(Value::as_str)
    }
}

fn language_name(id: &str) -> String {
    match id {
        "php" => "PHP".into(),
        "javascript" => "JavaScript".into(),
        "typescript" => "TypeScript".into(),
        "html" | "css" | "json" | "yaml" | "xml" | "sql" => id.to_ascii_uppercase(),
        other => {
            let mut c = other.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        }
    }
}

/// Resolve the format of `req.path`.
pub fn resolve_format(req: &FormatRequest) -> FileFormat {
    let get = Get(&req.settings);
    let file_type_indent = req
        .file_type_indent
        .as_deref()
        .map(|s| {
            if s.starts_with('\t') {
                IndentStyle {
                    use_tabs: true,
                    ..IndentStyle::default()
                }
            } else {
                IndentStyle::spaces(s.len().clamp(1, 16) as u8)
            }
        })
        .unwrap_or_default();

    // The language's code style (`php.codeStyle.indent`, …).
    let prefix = req
        .language_id
        .as_deref()
        .map(|l| format!("{l}.codeStyle."));
    let has_code_style = prefix
        .as_ref()
        .is_some_and(|p| req.settings.keys().any(|k| k.starts_with(p.as_str())));
    let mut configured = file_type_indent;
    let mut source = IndentSource::FileType;
    if let (true, Some(p)) = (has_code_style, &prefix) {
        let num = |k: &str, d: u8| get.uint(&format!("{p}{k}")).map_or(d, |n| n.min(16) as u8);
        let size = num("indent", 4).max(1);
        configured = IndentStyle {
            use_tabs: get.bool(&format!("{p}useTabs"), false),
            size,
            tab_size: num("tabSize", 4).max(1),
            continuation: num("continuationIndent", size),
        };
        source = IndentSource::CodeStyle;
    }

    let ec: EditorConfigProps = if get.bool("editor.codeStyle.useEditorConfig", true) {
        properties_for(&req.path, &req.editorconfigs)
    } else {
        EditorConfigProps::default()
    };

    let mut indent = configured;
    if ec.use_tabs.is_some() || ec.indent_size.is_some() || ec.tab_width.is_some() {
        if let Some(t) = ec.use_tabs {
            indent.use_tabs = t;
        }
        if let Some(n) = ec.indent_size {
            indent.size = n;
            indent.continuation = n;
        }
        if let Some(n) = ec.tab_width.or(ec.indent_size.filter(|_| indent.use_tabs)) {
            indent.tab_size = n;
        }
        source = IndentSource::EditorConfig;
    } else if get.bool("editor.codeStyle.detectIndents", true) {
        if let Some(found) = req
            .text
            .as_deref()
            .and_then(|t| detect_indent(t, configured))
        {
            if !found.same_kind(&configured) {
                indent = found;
                if !found.use_tabs {
                    indent.continuation = found.size;
                }
                source = IndentSource::Detected;
            }
        }
    }

    let setting_separator = get
        .str("editor.codeStyle.lineSeparator")
        .and_then(LineSeparator::parse)
        .unwrap_or_default();
    let mut save = SaveOptions {
        strip_trailing_spaces: get
            .str("editor.stripTrailingSpaces")
            .and_then(StripMode::parse)
            .unwrap_or_default(),
        ensure_newline_at_eof: get.bool("editor.ensureNewlineAtEof", true),
    };
    if let Some(trim) = ec.trim_trailing_whitespace {
        save.strip_trailing_spaces = if trim {
            StripMode::All
        } else {
            StripMode::None
        };
    }
    if let Some(eof) = ec.insert_final_newline {
        save.ensure_newline_at_eof = eof;
    }

    let hard_wrap_at = ec
        .max_line_length
        .or_else(|| get.uint("editor.codeStyle.hardWrapAt").map(|n| n as u32))
        .unwrap_or(120);
    let mut visual_guides: Vec<u32> = get
        .str("editor.codeStyle.visualGuides")
        .unwrap_or("")
        .split([',', ';', ' '])
        .filter_map(|s| s.trim().parse().ok())
        .filter(|n| *n > 0 && *n <= 1000)
        .collect();
    visual_guides.sort_unstable();
    visual_guides.dedup();

    FileFormat {
        indent_label: format!(
            "{}{}",
            indent.label(),
            if source == IndentSource::Detected {
                "*"
            } else {
                ""
            }
        ),
        indent,
        indent_source: source,
        configured_indent: configured,
        code_style_page: req
            .language_id
            .as_ref()
            .filter(|_| has_code_style)
            .map(|l| format!("editor.code-style.{l}")),
        language_name: req.language_id.as_deref().map(language_name),
        line_separator: ec.end_of_line.unwrap_or(setting_separator),
        required_line_separator: ec.end_of_line,
        charset: ec.charset.map(|(encoding, bom)| Charset { encoding, bom }),
        save,
        hard_wrap_at,
        visual_guides,
        editor_config: !ec.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn settings(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap()
    }

    fn php_request(text: &str) -> FormatRequest {
        FormatRequest {
            path: "app/User.php".into(),
            language_id: Some("php".into()),
            text: Some(text.into()),
            file_type_indent: Some("    ".into()),
            settings: settings(json!({
                "php.codeStyle.useTabs": false,
                "php.codeStyle.indent": 4,
                "php.codeStyle.tabSize": 4,
                "php.codeStyle.continuationIndent": 8,
                "editor.codeStyle.visualGuides": "80, 120, 80",
            })),
            editorconfigs: vec![],
        }
    }

    #[test]
    fn uses_the_language_code_style() {
        let f = resolve_format(&php_request("<?php\n"));
        assert_eq!(f.indent_source, IndentSource::CodeStyle);
        assert_eq!(f.indent.continuation, 8);
        assert_eq!(f.indent_label, "4 spaces");
        assert_eq!(f.code_style_page.as_deref(), Some("editor.code-style.php"));
        assert_eq!(f.language_name.as_deref(), Some("PHP"));
        assert_eq!(f.visual_guides, vec![80, 120]);
        assert_eq!(f.hard_wrap_at, 120);
        assert_eq!(f.save.strip_trailing_spaces, StripMode::Modified);
        assert!(f.save.ensure_newline_at_eof);
    }

    #[test]
    fn detects_a_different_indent_unless_disabled() {
        let text = "<?php\nfunction a() {\n  if (x) {\n    y();\n  }\n}\n";
        let f = resolve_format(&php_request(text));
        assert_eq!(f.indent_source, IndentSource::Detected);
        assert_eq!(f.indent_label, "2 spaces*");
        assert_eq!(f.configured_indent.size, 4);
        let mut req = php_request(text);
        req.settings
            .insert("editor.codeStyle.detectIndents".into(), json!(false));
        assert_eq!(resolve_format(&req).indent_label, "4 spaces");
    }

    #[test]
    fn editorconfig_wins() {
        let mut req = php_request("<?php\n");
        req.editorconfigs = vec![EditorConfigFile {
            dir: String::new(),
            content: "[*.php]\nindent_style = tab\ntab_width = 8\nend_of_line = crlf\ntrim_trailing_whitespace = false\ninsert_final_newline = false\nmax_line_length = 100\ncharset = latin1\n".into(),
        }];
        let f = resolve_format(&req);
        assert_eq!(f.indent_source, IndentSource::EditorConfig);
        assert_eq!(f.indent_label, "Tab");
        assert_eq!(f.indent.tab_size, 8);
        assert_eq!(f.line_separator, LineSeparator::Crlf);
        assert_eq!(f.save.strip_trailing_spaces, StripMode::None);
        assert!(!f.save.ensure_newline_at_eof);
        assert_eq!(f.hard_wrap_at, 100);
        assert_eq!(f.charset.as_ref().unwrap().encoding, Encoding::Iso8859_1);
        req.settings
            .insert("editor.codeStyle.useEditorConfig".into(), json!(false));
        let f = resolve_format(&req);
        assert_eq!(f.indent_source, IndentSource::CodeStyle);
        assert!(!f.editor_config);
    }

    #[test]
    fn falls_back_to_the_file_type_indent() {
        let f = resolve_format(&FormatRequest {
            path: "a.yaml".into(),
            language_id: Some("yaml".into()),
            file_type_indent: Some("  ".into()),
            settings: settings(
                json!({ "editor.codeStyle.lineSeparator": "crlf", "editor.stripTrailingSpaces": "all" }),
            ),
            ..Default::default()
        });
        assert_eq!(f.indent_source, IndentSource::FileType);
        assert_eq!(f.indent_label, "2 spaces");
        assert_eq!(f.line_separator, LineSeparator::Crlf);
        assert_eq!(f.code_style_page, None);
        assert_eq!(f.save.strip_trailing_spaces, StripMode::All);
        assert!(affects_format("php.codeStyle.indent"));
        assert!(!affects_format("editor.font.size"));
    }
}
