//! The user's part of Settings | Editor | File Types: file name patterns added to or removed
//! from a file type (`fileTypes.associations`), custom file types with their own syntax
//! highlighting (`fileTypes.custom`) and the ignored files and folders
//! (`fileTypes.ignoredFiles`).
//!
//! Settings store both lists as tables of strings (`objects` settings), so parsing here is
//! lenient: incomplete entries are skipped instead of failing the whole request.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{FileTypeRegistry, FileTypeSpec, DEFAULT_INDENT_SIZE};

/// PHPStorm's default "Ignored Files and Folders" (setting `fileTypes.ignoredFiles`).
pub const DEFAULT_IGNORED_FILES: &str =
    "*.hprof;*.pyc;*.pyo;*.rbc;*.yarb;*~;.DS_Store;.git;.hg;.svn;CVS;__pycache__;_svn;vssver.scc;vssver2.scc";

/// Prefix of the ids of custom file types (`custom-my-language`).
pub const CUSTOM_PREFIX: &str = "custom-";

/// One change of a file type's patterns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Association {
    pub file_type: String,
    pub pattern: String,
    /// `true`: the pattern no longer selects the type (a bundled pattern the user removed).
    pub removed: bool,
}

impl Association {
    /// `{ fileType, pattern, action: "add" | "remove" }`.
    pub fn from_value(value: &Value) -> Option<Self> {
        let file_type = str_field(value, "fileType")?;
        let pattern = str_field(value, "pattern")?;
        Some(Self {
            file_type,
            pattern,
            removed: value.get("action").and_then(Value::as_str) == Some("remove"),
        })
    }
}

fn str_field(value: &Value, key: &str) -> Option<String> {
    let s = value.get(key)?.as_str()?.trim();
    (!s.is_empty()).then(|| s.to_string())
}

fn flag(value: &Value, key: &str, default: bool) -> bool {
    match value.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s == "true",
        _ => default,
    }
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// The syntax of a custom file type, as the editor's generic highlighter needs it
/// (PHPStorm's "New File Type" dialog). Keywords are deduplicated and sorted; with
/// `ignore_case` they are lower case.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomSyntax {
    pub line_comment: String,
    /// The line comment only counts at the start of a line.
    pub line_comment_at_start: bool,
    pub block_comment_start: String,
    pub block_comment_end: String,
    /// E.g. `0x`; empty: no hexadecimal numbers.
    pub hex_prefix: String,
    /// Characters allowed right after a number, e.g. `lLfF`.
    pub number_postfixes: String,
    /// Keyword lists 1–4, each highlighted in its own color.
    pub keywords: Vec<Vec<String>>,
    pub ignore_case: bool,
    pub paired_braces: bool,
    pub paired_brackets: bool,
    pub paired_parens: bool,
    pub string_escapes: bool,
}

/// A user-defined file type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomFileType {
    pub id: String,
    pub name: String,
    pub description: String,
    pub syntax: CustomSyntax,
}

/// The id of a custom file type: `custom-` and the name in lower case with dashes.
pub fn custom_id(name: &str) -> String {
    let mut slug = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    format!("{CUSTOM_PREFIX}{}", slug.trim_end_matches('-'))
}

/// Split a keyword list (one keyword per line or separated by spaces).
fn keywords(raw: &str, ignore_case: bool) -> Vec<String> {
    let mut out: Vec<String> = raw
        .split_whitespace()
        .map(|w| {
            if ignore_case {
                w.to_lowercase()
            } else {
                w.to_string()
            }
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

impl CustomFileType {
    /// `{ name, description?, lineComment?, lineCommentAtStart?, blockCommentStart?,
    /// blockCommentEnd?, hexPrefix?, numberPostfixes?, keywords1..keywords4?, ignoreCase?,
    /// pairedBraces?, pairedBrackets?, pairedParens?, stringEscapes? }`; flags are `"true"` /
    /// `"false"` strings (or booleans).
    pub fn from_value(value: &Value) -> Option<Self> {
        let name = str_field(value, "name")?;
        let ignore_case = flag(value, "ignoreCase", false);
        let syntax = CustomSyntax {
            line_comment: text(value, "lineComment"),
            line_comment_at_start: flag(value, "lineCommentAtStart", false),
            block_comment_start: text(value, "blockCommentStart"),
            block_comment_end: text(value, "blockCommentEnd"),
            hex_prefix: text(value, "hexPrefix"),
            number_postfixes: text(value, "numberPostfixes"),
            keywords: (1..=4)
                .map(|i| {
                    keywords(
                        value
                            .get(format!("keywords{i}"))
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        ignore_case,
                    )
                })
                .collect(),
            ignore_case,
            paired_braces: flag(value, "pairedBraces", true),
            paired_brackets: flag(value, "pairedBrackets", true),
            paired_parens: flag(value, "pairedParens", true),
            string_escapes: flag(value, "stringEscapes", true),
        };
        Some(Self {
            id: custom_id(&name),
            description: text(value, "description"),
            name,
            syntax,
        })
    }
}

/// The parsed file type settings of a request.
#[derive(Debug, Clone, Default)]
pub struct UserFileTypes {
    pub associations: Vec<Association>,
    pub custom: Vec<CustomFileType>,
}

impl UserFileTypes {
    pub fn parse(associations: &[Value], custom: &[Value]) -> Self {
        let mut custom: Vec<CustomFileType> = custom
            .iter()
            .filter_map(CustomFileType::from_value)
            .collect();
        // The first definition of a name wins.
        let mut seen = std::collections::BTreeSet::new();
        custom.retain(|c| seen.insert(c.id.clone()));
        Self {
            associations: associations
                .iter()
                .filter_map(Association::from_value)
                .collect(),
            custom,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.associations.is_empty() && self.custom.is_empty()
    }
}

/// `*.ext` with a plain extension (no further wildcards, no path) → `ext`.
fn pattern_extension(pattern: &str) -> Option<&str> {
    let ext = pattern.strip_prefix("*.")?;
    (!ext.is_empty() && !ext.contains(['*', '?', '/'])).then_some(ext)
}

impl FileTypeSpec {
    /// The file name patterns PHPStorm lists for the type: `*.ext` for each extension, then
    /// the file names and name patterns.
    pub fn patterns(&self) -> Vec<String> {
        self.extensions
            .iter()
            .map(|e| format!("*.{e}"))
            .chain(self.file_names.iter().cloned())
            .collect()
    }

    fn has_pattern(&self, pattern: &str) -> bool {
        self.patterns()
            .iter()
            .any(|p| p.eq_ignore_ascii_case(pattern))
    }

    fn remove_pattern(&mut self, pattern: &str) {
        match pattern_extension(pattern) {
            Some(ext) => self.extensions.retain(|e| !e.eq_ignore_ascii_case(ext)),
            None => self.file_names.retain(|n| !n.eq_ignore_ascii_case(pattern)),
        }
    }

    fn add_pattern(&mut self, pattern: &str) {
        if self.has_pattern(pattern) {
            return;
        }
        match pattern_extension(pattern) {
            Some(ext) => self.extensions.push(ext.to_string()),
            None => self.file_names.push(pattern.to_string()),
        }
    }
}

/// One row of Settings | Editor | File Types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTypeRow {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Patterns after the user's changes.
    pub patterns: Vec<String>,
    /// Patterns the user added (shown in bold).
    pub added: Vec<String>,
    /// Bundled or plugin patterns the user removed.
    pub removed: Vec<String>,
    /// Contributing plugin; `None` for bundled and custom types.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
    pub custom: bool,
    /// Full definition of a custom type, for editing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syntax: Option<CustomSyntax>,
    pub indent: u8,
}

impl FileTypeRegistry {
    /// The registry with the user's custom types registered and their pattern changes
    /// applied. A pattern added to one type is taken away from every other type, like
    /// PHPStorm's "reassign wildcard" answer.
    pub fn customized(&self, user: &UserFileTypes) -> FileTypeRegistry {
        let mut out = self.clone();
        for c in &user.custom {
            out.register(
                FileTypeSpec {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    ..Default::default()
                },
                None,
            );
            out.custom.insert(c.id.clone(), c.syntax.clone());
            out.descriptions.insert(c.id.clone(), c.description.clone());
        }
        for a in &user.associations {
            for e in &mut out.entries {
                if e.spec.id == a.file_type {
                    if a.removed {
                        e.spec.remove_pattern(&a.pattern);
                    } else {
                        e.spec.add_pattern(&a.pattern);
                    }
                } else if !a.removed {
                    e.spec.remove_pattern(&a.pattern);
                }
            }
        }
        out
    }

    /// Settings | Editor | File Types: every active file type (a plugin's override replaces
    /// the bundled type of the same id), sorted by name.
    pub fn rows(&self, inactive_plugins: &[String], user: &UserFileTypes) -> Vec<FileTypeRow> {
        let customized = self.customized(user);
        let active = |r: &FileTypeRegistry| -> Vec<(FileTypeSpec, Option<String>)> {
            let enabled: Vec<_> = r
                .entries
                .iter()
                .filter(|e| {
                    e.plugin
                        .as_ref()
                        .is_none_or(|p| !inactive_plugins.contains(p))
                })
                .collect();
            enabled
                .iter()
                .enumerate()
                .filter(|(i, e)| !enabled[i + 1..].iter().any(|l| l.spec.id == e.spec.id))
                .map(|(_, e)| (e.spec.clone(), e.plugin.clone()))
                .collect()
        };
        let before = active(self);
        let mut rows: Vec<FileTypeRow> = active(&customized)
            .into_iter()
            .map(|(spec, plugin)| {
                let original = before
                    .iter()
                    .find(|(s, _)| s.id == spec.id)
                    .map(|(s, _)| s.patterns())
                    .unwrap_or_default();
                let patterns = spec.patterns();
                let has = |list: &[String], p: &str| list.iter().any(|x| x.eq_ignore_ascii_case(p));
                FileTypeRow {
                    added: patterns
                        .iter()
                        .filter(|p| !has(&original, p))
                        .cloned()
                        .collect(),
                    removed: original
                        .iter()
                        .filter(|p| !has(&patterns, p))
                        .cloned()
                        .collect(),
                    description: customized
                        .descriptions
                        .get(&spec.id)
                        .cloned()
                        .unwrap_or_default(),
                    custom: customized.custom.contains_key(&spec.id),
                    syntax: customized.custom.get(&spec.id).cloned(),
                    indent: spec.indent_size.unwrap_or(DEFAULT_INDENT_SIZE),
                    patterns,
                    plugin,
                    id: spec.id,
                    name: spec.name,
                }
            })
            .collect();
        rows.sort_by_key(|r| (r.name.to_lowercase(), r.id.clone()));
        rows
    }
}
