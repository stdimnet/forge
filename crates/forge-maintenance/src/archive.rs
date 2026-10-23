//! The settings archive of File | Manage IDE Settings | Export / Import Settings: a zip file
//! with the chosen groups of the user settings (`settings.toml`), whole configuration files
//! (`ai.toml`, `ssh.toml`, …) and a manifest (`forge-settings.json`) naming the groups.
//! Secrets never are part of it: API keys and passwords live in the OS keychain.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zip::write::SimpleFileOptions;

use crate::groups::{group, group_of_key, settings_groups, GroupCount, OTHER};

/// The archive's manifest entry.
pub const MANIFEST: &str = "forge-settings.json";
/// The archive's entry with the user settings of the chosen key groups.
pub const SETTINGS_ENTRY: &str = "settings.toml";
/// Entries larger than this are refused on import.
const MAX_ENTRY: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format: u32,
    /// Forge version that wrote the archive.
    #[serde(default)]
    version: String,
    groups: Vec<String>,
}

/// Write the archive: the user `values` of the chosen key groups and the chosen files
/// (`files`: name in the config dir → content; missing ones are skipped).
pub fn export_settings(
    values: &BTreeMap<String, Value>,
    files: &BTreeMap<String, Vec<u8>>,
    groups: &[String],
    version: &str,
) -> Result<Vec<u8>, String> {
    let chosen = |id: &str| groups.iter().any(|g| g == id);
    let selected: BTreeMap<String, Value> = values
        .iter()
        .filter(|(k, _)| chosen(group_of_key(k)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut exported = Vec::new();
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut put = |name: &str, bytes: &[u8]| -> Result<(), String> {
        zip.start_file(name, options).map_err(|e| e.to_string())?;
        zip.write_all(bytes).map_err(|e| e.to_string())
    };
    for g in settings_groups().iter().filter(|g| chosen(g.id)) {
        match g.file {
            Some(name) => {
                if let Some(bytes) = files.get(name) {
                    put(name, bytes)?;
                    exported.push(g.id.to_string());
                }
            }
            None if selected.keys().any(|k| group_of_key(k) == g.id) => {
                exported.push(g.id.to_string())
            }
            None => {}
        }
    }
    if exported.is_empty() {
        return Err("Nothing to export: the selected settings are all at their defaults".into());
    }
    if !selected.is_empty() {
        let toml = forge_core::settings_to_toml(&selected).map_err(|e| e.to_string())?;
        put(SETTINGS_ENTRY, toml.as_bytes())?;
    }
    let manifest = Manifest {
        format: 1,
        version: version.to_string(),
        groups: exported,
    };
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    put(MANIFEST, &json)?;
    let cursor = zip.finish().map_err(|e| e.to_string())?;
    Ok(cursor.into_inner())
}

/// A settings archive that was read for import.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SettingsArchive {
    pub values: BTreeMap<String, Value>,
    pub files: BTreeMap<String, Vec<u8>>,
}

/// Read an archive written by [`export_settings`].
pub fn read_archive(bytes: &[u8]) -> Result<SettingsArchive, String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("Not a settings archive: {e}"))?;
    let mut entries = BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let known = name == MANIFEST
            || name == SETTINGS_ENTRY
            || settings_groups()
                .iter()
                .any(|g| g.file == Some(name.as_str()));
        if !known {
            continue;
        }
        if entry.size() > MAX_ENTRY {
            return Err(format!("{name} is too large"));
        }
        let mut buf = Vec::new();
        entry
            .by_ref()
            .take(MAX_ENTRY + 1)
            .read_to_end(&mut buf)
            .map_err(|e| e.to_string())?;
        entries.insert(name, buf);
    }
    if !entries.contains_key(MANIFEST) {
        return Err("Not a settings archive: forge-settings.json is missing".into());
    }
    let values = match entries.remove(SETTINGS_ENTRY) {
        Some(bytes) => {
            let src = String::from_utf8(bytes).map_err(|_| "settings.toml is not UTF-8")?;
            forge_core::parse_settings_toml(&src).map_err(|e| e.to_string())?
        }
        None => BTreeMap::new(),
    };
    entries.remove(MANIFEST);
    Ok(SettingsArchive {
        values,
        files: entries,
    })
}

/// What an import changes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportPlan {
    /// Settings to store in the user layer; `Value::Null` removes a key the imported group
    /// does not set (the group is replaced as a whole).
    pub changes: Vec<(String, Value)>,
    /// Configuration files to write into `<config dir>/forge`.
    pub files: Vec<(String, Vec<u8>)>,
}

impl SettingsArchive {
    /// The groups the archive holds, for the import dialog.
    pub fn groups(&self) -> Vec<GroupCount> {
        let files: Vec<&str> = self.files.keys().map(String::as_str).collect();
        crate::groups::count_groups(self.values.keys().map(String::as_str), &files)
    }

    /// Replace the chosen groups of the `current` user settings with the archive's.
    pub fn plan(&self, current: &BTreeMap<String, Value>, groups: &[String]) -> ImportPlan {
        let chosen = |id: &str| groups.iter().any(|g| g == id);
        let mut changes: Vec<(String, Value)> = current
            .keys()
            .filter(|k| chosen(group_of_key(k)) && !self.values.contains_key(*k))
            // Plugins stay as they are unless the archive says otherwise.
            .filter(|k| {
                self.values
                    .keys()
                    .any(|v| group_of_key(v) == group_of_key(k))
            })
            .map(|k| (k.clone(), Value::Null))
            .collect();
        changes.extend(
            self.values
                .iter()
                .filter(|(k, _)| chosen(group_of_key(k)))
                .filter(|(k, v)| current.get(*k) != Some(*v))
                .map(|(k, v)| (k.clone(), v.clone())),
        );
        let files = self
            .files
            .iter()
            .filter(|(name, _)| {
                settings_groups()
                    .iter()
                    .any(|g| g.file == Some(name.as_str()) && chosen(g.id))
            })
            .map(|(n, b)| (n.clone(), b.clone()))
            .collect();
        ImportPlan { changes, files }
    }
}

/// Whether `id` names a group (for validating requests).
pub fn is_group(id: &str) -> bool {
    id == OTHER || group(id).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn user() -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("editor.font.size".to_string(), json!(16)),
            ("editor.tabs.limit".to_string(), json!(20)),
            ("keymap.active".to_string(), json!("macOS")),
            (
                "fileTypes.custom".to_string(),
                json!([{ "name": "DSL", "lineComment": "#" }]),
            ),
            ("plugins.disabled".to_string(), json!(["vim"])),
        ])
    }

    #[test]
    fn export_and_import_round_trip() {
        let files = BTreeMap::from([
            (
                "ssh.toml".to_string(),
                b"[[configs]]\nhost = \"a\"\n".to_vec(),
            ),
            ("ai.toml".to_string(), b"default = \"x\"\n".to_vec()),
        ]);
        let groups: Vec<String> = ["editor", "fileTypes", "ssh", "newProjects"]
            .map(String::from)
            .to_vec();
        let zip = export_settings(&user(), &files, &groups, "1.0.0").unwrap();
        let archive = read_archive(&zip).unwrap();
        assert_eq!(archive.values.len(), 3);
        assert_eq!(
            archive.values["fileTypes.custom"],
            user()["fileTypes.custom"]
        );
        assert!(!archive.values.contains_key("keymap.active"));
        assert_eq!(archive.files.keys().collect::<Vec<_>>(), ["ssh.toml"]);
        let ids: Vec<_> = archive.groups().iter().map(|g| g.id).collect();
        assert_eq!(ids, ["editor", "fileTypes", "ssh"]);

        // Importing "Editor" replaces that group: the font size comes back, a value the
        // archive does not have is removed, other groups stay.
        let mut current = user();
        current.insert("editor.font.size".into(), json!(12));
        current.insert("editor.caret.blinking".into(), json!(false));
        let plan = archive.plan(&current, &["editor".to_string(), "ssh".to_string()]);
        assert_eq!(
            plan.changes,
            vec![
                ("editor.caret.blinking".to_string(), Value::Null),
                ("editor.font.size".to_string(), json!(16)),
            ]
        );
        assert_eq!(plan.files.len(), 1);
        assert!(archive.plan(&current, &[]).changes.is_empty());
    }

    #[test]
    fn rejects_foreign_and_empty_archives() {
        assert!(read_archive(b"not a zip").is_err());
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("other.txt", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"x").unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        assert!(read_archive(&bytes).unwrap_err().contains("missing"));
        assert!(
            export_settings(&BTreeMap::new(), &BTreeMap::new(), &["editor".into()], "1").is_err()
        );
        assert!(is_group("other") && is_group("keymap") && !is_group("nope"));
    }
}
