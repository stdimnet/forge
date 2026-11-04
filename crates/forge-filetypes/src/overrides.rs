//! File type overrides (File | File Properties | Override File Type, as in PHPStorm): the
//! user picks the type of one file by hand. Overrides are stored per project in the
//! `fileTypes.overrides` setting (`{ path, fileType }`, the path relative to the project
//! root as the editor knows it) and win over every other rule of the detection. They follow
//! the file when it is renamed or moved ([`edit_overrides`]).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{FileTypeRegistry, UserFileTypes, PLAIN_TEXT};

/// Override value that makes a file plain text.
pub const PLAIN_TEXT_ID: &str = "text";

/// One entry of the `fileTypes.overrides` setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTypeOverride {
    pub path: String,
    pub file_type: String,
}

/// `a\b/./c` → `a/b/c`, without a leading `./` or trailing `/`.
fn normalize(path: &str) -> String {
    let path = path.replace('\\', "/");
    let mut out: Vec<&str> = Vec::new();
    for (i, part) in path.split('/').enumerate() {
        if part == "." || (part.is_empty() && i > 0) {
            continue;
        }
        out.push(part);
    }
    out.join("/")
}

/// The valid entries of the setting (the last one of a path wins).
pub fn parse_overrides(values: &[Value]) -> Vec<FileTypeOverride> {
    let mut out: Vec<FileTypeOverride> = Vec::new();
    for v in values {
        let Ok(o) = serde_json::from_value::<FileTypeOverride>(v.clone()) else {
            continue;
        };
        let (path, file_type) = (normalize(&o.path), o.file_type.trim().to_string());
        if path.is_empty() || file_type.is_empty() {
            continue;
        }
        out.retain(|x| x.path != path);
        out.push(FileTypeOverride { path, file_type });
    }
    out
}

/// The file type chosen for `path`, if any.
pub fn override_for<'a>(overrides: &'a [FileTypeOverride], path: &str) -> Option<&'a str> {
    let path = normalize(path);
    overrides
        .iter()
        .find(|o| o.path == path)
        .map(|o| o.file_type.as_str())
}

/// A change to the overrides.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum OverrideEdit {
    /// Override File Type.
    #[serde(rename_all = "camelCase")]
    Set { path: String, file_type: String },
    /// Revert File Type Override.
    Unset { path: String },
    /// The file or a folder above it was renamed or moved (`to`), or deleted (`None`).
    Moved { from: String, to: Option<String> },
}

/// Request of the `filetypes_edit_overrides` IPC command (and its WebAssembly twin).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditOverridesRequest {
    /// The current `fileTypes.overrides` value.
    #[serde(default)]
    pub overrides: Vec<Value>,
    pub edit: OverrideEdit,
}

/// The overrides after `edit`, ready to be stored as the new setting value.
pub fn edit_overrides(overrides: &[Value], edit: &OverrideEdit) -> Vec<FileTypeOverride> {
    let mut list = parse_overrides(overrides);
    match edit {
        OverrideEdit::Set { path, file_type } => {
            let path = normalize(path);
            list.retain(|o| o.path != path);
            if !path.is_empty() && !file_type.trim().is_empty() {
                list.push(FileTypeOverride {
                    path,
                    file_type: file_type.trim().to_string(),
                });
            }
        }
        OverrideEdit::Unset { path } => {
            let path = normalize(path);
            list.retain(|o| o.path != path);
        }
        OverrideEdit::Moved { from, to } => {
            let from = normalize(from);
            let inside = |p: &str| p == from || p.starts_with(&format!("{from}/"));
            match to.as_deref().map(normalize) {
                None => list.retain(|o| !inside(&o.path)),
                Some(to) => {
                    for o in &mut list {
                        if inside(&o.path) {
                            o.path = format!("{to}{}", &o.path[from.len()..]);
                        }
                    }
                    // A moved override replaces one that was at the target.
                    let mut seen = std::collections::BTreeSet::new();
                    let mut kept: Vec<FileTypeOverride> = list
                        .into_iter()
                        .rev()
                        .filter(|o| seen.insert(o.path.clone()))
                        .collect();
                    kept.reverse();
                    list = kept;
                }
            }
        }
    }
    list
}

/// The pattern Associate with File Type adds for `path`: `*.ext`, or the whole file name
/// when it has no extension (`artisan`, `Jenkinsfile`, `.babelrc`).
pub fn association_pattern(path: &str) -> String {
    let name = normalize(path);
    let name = name.rsplit('/').next().unwrap_or_default();
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => format!("*{}", &name[i..]),
        _ => name.to_string(),
    }
}

/// One file type of the Override File Type list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverrideChoice {
    pub id: String,
    pub name: String,
    /// The contributing plugin is disabled or not used by the project: picking the type
    /// activates its file type definition for that file only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inactive_plugin: Option<String>,
}

/// Answer of the `filetypes_override_choices` IPC command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverrideChoices {
    /// Every file type by name, Plain text included.
    pub choices: Vec<OverrideChoice>,
    /// The type `fileTypes.overrides` sets for the file, if any.
    pub current: Option<String>,
    /// Pattern for Associate with File Type.
    pub pattern: String,
}

impl FileTypeRegistry {
    /// Override File Type: every registered type (custom ones too), sorted by name. Types of
    /// inactive plugins are listed with that plugin, unless an active type has the same id.
    pub fn override_choices(
        &self,
        path: &str,
        inactive_plugins: &[String],
        user: &UserFileTypes,
        overrides: &[Value],
    ) -> OverrideChoices {
        let registry = self.customized(user);
        let mut choices: Vec<OverrideChoice> = vec![OverrideChoice {
            id: PLAIN_TEXT_ID.into(),
            name: PLAIN_TEXT.into(),
            inactive_plugin: None,
        }];
        let active = registry.active(inactive_plugins);
        for t in &active {
            choices.push(OverrideChoice {
                id: t.id.clone(),
                name: t.name.clone(),
                inactive_plugin: None,
            });
        }
        for e in registry.entries.iter().rev() {
            if choices.iter().any(|c| c.id == e.spec.id) {
                continue;
            }
            choices.push(OverrideChoice {
                id: e.spec.id.clone(),
                name: e.spec.name.clone(),
                inactive_plugin: e.plugin.clone(),
            });
        }
        choices.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        let list = parse_overrides(overrides);
        OverrideChoices {
            choices,
            current: override_for(&list, path).map(str::to_string),
            pattern: association_pattern(path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn values(list: &[FileTypeOverride]) -> Vec<Value> {
        list.iter()
            .map(|o| serde_json::to_value(o).unwrap())
            .collect()
    }

    #[test]
    fn edits_follow_renames_and_moves() {
        let start = vec![
            json!({ "path": "bin/console", "fileType": "php" }),
            json!({ "path": "./scripts\\deploy", "fileType": "shell" }),
            json!({ "path": "", "fileType": "php" }),
            json!({ "bad": true }),
        ];
        let list = parse_overrides(&start);
        assert_eq!(list.len(), 2);
        assert_eq!(override_for(&list, "scripts/deploy"), Some("shell"));

        // Renaming the file.
        let moved = edit_overrides(
            &start,
            &OverrideEdit::Moved {
                from: "bin/console".into(),
                to: Some("bin/app".into()),
            },
        );
        assert_eq!(override_for(&moved, "bin/app"), Some("php"));
        assert_eq!(override_for(&moved, "bin/console"), None);

        // Moving a folder above it; `scriptsX` is not inside `scripts`.
        let with_sibling = edit_overrides(
            &values(&moved),
            &OverrideEdit::Set {
                path: "scriptsX/run".into(),
                file_type: "python".into(),
            },
        );
        let moved = edit_overrides(
            &values(&with_sibling),
            &OverrideEdit::Moved {
                from: "scripts".into(),
                to: Some("tools/scripts".into()),
            },
        );
        assert_eq!(override_for(&moved, "tools/scripts/deploy"), Some("shell"));
        assert_eq!(override_for(&moved, "scriptsX/run"), Some("python"));

        // Deleting removes it; Unset reverts.
        let gone = edit_overrides(
            &values(&moved),
            &OverrideEdit::Moved {
                from: "tools".into(),
                to: None,
            },
        );
        assert_eq!(override_for(&gone, "tools/scripts/deploy"), None);
        let reverted = edit_overrides(
            &values(&gone),
            &OverrideEdit::Unset {
                path: "bin/app".into(),
            },
        );
        assert_eq!(reverted.len(), 1);
    }

    #[test]
    fn moving_onto_an_overridden_file_keeps_one_entry() {
        let start = vec![
            json!({ "path": "a", "fileType": "php" }),
            json!({ "path": "b", "fileType": "shell" }),
        ];
        let moved = edit_overrides(
            &start,
            &OverrideEdit::Moved {
                from: "b".into(),
                to: Some("a".into()),
            },
        );
        assert_eq!(moved.len(), 1);
        assert_eq!(override_for(&moved, "a"), Some("shell"));
    }

    #[test]
    fn association_patterns() {
        assert_eq!(association_pattern("src/thing.module"), "*.module");
        assert_eq!(association_pattern("artisan"), "artisan");
        assert_eq!(association_pattern("conf/.babelrc"), ".babelrc");
        assert_eq!(association_pattern("weird."), "weird.");
    }
}
