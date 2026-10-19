//! Language server configuration as contributed by language packs.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How to start and talk to one language server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspServerConfig {
    /// Stable id, e.g. `"intelephense"`.
    pub id: String,
    /// Human readable name for the status bar; defaults to the id.
    #[serde(default)]
    pub name: Option<String>,
    /// LSP language ids this server handles, e.g. `["php"]`. The first one is used for
    /// `didOpen` unless an extension-specific id is found in `file_extensions` order.
    pub language_ids: Vec<String>,
    /// File extensions without the dot, e.g. `["php", "phtml"]`. Matched case-insensitively;
    /// compound extensions like `blade.php` are supported.
    pub file_extensions: Vec<String>,
    /// Executable (looked up on `PATH` when not absolute).
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Sent as `initializationOptions` in the `initialize` request.
    #[serde(default)]
    pub initialization_options: Value,
    /// Settings tree answered to `workspace/configuration` and pushed with
    /// `workspace/didChangeConfiguration`, e.g. `{"intelephense": {"files": {...}}}`.
    #[serde(default)]
    pub settings: Value,
}

impl LspServerConfig {
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }

    /// Length of the longest configured extension matching `path`, if any. Longer matches
    /// (`blade.php`) win over shorter ones (`php`) when routing.
    pub fn match_len(&self, path: &Path) -> Option<usize> {
        let name = path.file_name()?.to_str()?.to_ascii_lowercase();
        self.file_extensions
            .iter()
            .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
            .filter(|ext| {
                name.len() > ext.len() + 1
                    && name.ends_with(ext.as_str())
                    && name.as_bytes()[name.len() - ext.len() - 1] == b'.'
            })
            .map(|ext| ext.len())
            .max()
    }

    /// Language id announced in `textDocument/didOpen`.
    pub fn language_id_for(&self, _path: &Path) -> String {
        self.language_ids
            .first()
            .cloned()
            .unwrap_or_else(|| self.id.clone())
    }

    /// Value for a `workspace/configuration` section (dot separated) from `settings`.
    pub fn setting(&self, section: Option<&str>) -> Value {
        let Some(section) = section.filter(|s| !s.is_empty()) else {
            return self.settings.clone();
        };
        section
            .split('.')
            .try_fold(&self.settings, |v, key| v.get(key))
            .cloned()
            .unwrap_or(Value::Null)
    }
}

/// Look up an executable on `PATH` (with `PATHEXT` handling on Windows).
pub fn find_on_path(command: &str) -> Option<PathBuf> {
    let candidate = Path::new(command);
    if candidate.components().count() > 1 {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(|s| s.to_ascii_lowercase())
            .chain(std::iter::once(String::new()))
            .collect()
    } else {
        vec![String::new()]
    };
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        exts.iter()
            .map(|ext| dir.join(format!("{command}{ext}")))
            .find(|p| p.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg() -> LspServerConfig {
        LspServerConfig {
            id: "x".into(),
            name: None,
            language_ids: vec!["php".into()],
            file_extensions: vec!["php".into(), ".blade.php".into()],
            command: "x".into(),
            args: vec![],
            initialization_options: Value::Null,
            settings: json!({"intelephense": {"files": {"maxSize": 5}}}),
        }
    }

    #[test]
    fn matches_extensions() {
        let c = cfg();
        assert_eq!(c.match_len(Path::new("/a/User.PHP")), Some(3));
        assert_eq!(c.match_len(Path::new("/a/x.blade.php")), Some(9));
        assert_eq!(c.match_len(Path::new("/a/php")), None);
        assert_eq!(c.match_len(Path::new("/a/x.phpx")), None);
        assert_eq!(c.match_len(Path::new("/a/xphp")), None);
    }

    #[test]
    fn resolves_setting_sections() {
        let c = cfg();
        assert_eq!(c.setting(Some("intelephense.files.maxSize")), json!(5));
        assert_eq!(c.setting(Some("intelephense.nope")), Value::Null);
        assert_eq!(c.setting(None), c.settings);
    }

    #[test]
    fn deserializes_camel_case() {
        let c: LspServerConfig = serde_json::from_value(json!({
            "id": "a", "languageIds": ["lua"], "fileExtensions": ["lua"], "command": "lua-ls"
        }))
        .unwrap();
        assert_eq!(c.display_name(), "a");
        assert!(c.args.is_empty());
    }
}
