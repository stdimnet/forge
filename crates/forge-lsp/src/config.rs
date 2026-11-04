//! Language server configuration as contributed by language packs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How to start and talk to one language server.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspServerConfig {
    /// Stable id, e.g. `"intelephense"`.
    pub id: String,
    /// Human readable name for the status bar; defaults to the id.
    #[serde(default)]
    pub name: Option<String>,
    /// LSP language ids this server handles, e.g. `["php"]`. When it has one id per entry
    /// of `file_extensions`, the id at the index of the matching extension is announced in
    /// `didOpen` (`tsx` → `typescriptreact`); otherwise the first one is used.
    pub language_ids: Vec<String>,
    /// File extensions without the dot, e.g. `["php", "phtml"]`. Matched case-insensitively;
    /// compound extensions like `blade.php` are supported. An entry starting with `=` is a
    /// whole file name (`=Dockerfile`), which beats any extension match.
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
    /// Environment variables of the server process on top of Forge's own (`JAVA_HOME` of
    /// the Kotlin servers, JVM options).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// Ask the server for diagnostics (`textDocument/diagnostic`, LSP 3.17 pull model)
    /// after opening and changing documents, for servers that do not publish them
    /// (kotlin-lsp). Off for the others: announcing pull support makes some servers stop
    /// publishing.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pull_diagnostics: bool,
    /// Notifications sent right after `initialized` (the Roslyn server loads nothing until
    /// it gets `solution/open` or `project/open`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after_initialized: Vec<LspNotification>,
}

/// A notification of [`LspServerConfig::after_initialized`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspNotification {
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl LspServerConfig {
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }

    /// Length of the longest configured extension matching `path`, if any. Longer matches
    /// (`blade.php`) win over shorter ones (`php`) when routing.
    pub fn match_len(&self, path: &Path) -> Option<usize> {
        self.best_match(path).map(|(_, len)| len)
    }

    /// Index into `file_extensions` and length of the best match for `path`.
    fn best_match(&self, path: &Path) -> Option<(usize, usize)> {
        let name = path.file_name()?.to_str()?.to_ascii_lowercase();
        self.file_extensions
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| {
                if let Some(exact) = entry.strip_prefix('=') {
                    // Whole file names rank above every extension.
                    return (exact.eq_ignore_ascii_case(&name)).then_some((i, name.len() + 1));
                }
                let ext = entry.trim_start_matches('.').to_ascii_lowercase();
                (name.len() > ext.len() + 1
                    && name.ends_with(ext.as_str())
                    && name.as_bytes()[name.len() - ext.len() - 1] == b'.')
                    .then_some((i, ext.len()))
            })
            .max_by_key(|(i, len)| (*len, std::cmp::Reverse(*i)))
    }

    /// Language id announced in `textDocument/didOpen`.
    pub fn language_id_for(&self, path: &Path) -> String {
        if self.language_ids.len() == self.file_extensions.len() {
            if let Some((i, _)) = self.best_match(path) {
                return self.language_ids[i].clone();
            }
        }
        self.language_ids
            .first()
            .cloned()
            .unwrap_or_else(|| self.id.clone())
    }

    /// Value for a `workspace/configuration` section (dot separated) from `settings`.
    pub fn setting(&self, section: Option<&str>) -> Value {
        setting_of(&self.settings, section)
    }
}

/// Value for a `workspace/configuration` section (dot separated) of a settings tree.
pub fn setting_of(settings: &Value, section: Option<&str>) -> Value {
    let Some(section) = section.filter(|s| !s.is_empty()) else {
        return settings.clone();
    };
    section
        .split('.')
        .try_fold(settings, |v, key| v.get(key))
        .cloned()
        .unwrap_or(Value::Null)
}

/// Recursively merge `patch` into `base`: objects merge key by key, everything else is
/// replaced.
pub fn merge_settings(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                match b.get_mut(k) {
                    Some(existing) => merge_settings(existing, v),
                    None => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, p) => *b = p.clone(),
    }
}

/// Look up an executable on `PATH` (with `PATHEXT` handling on Windows); in Flatpak on the
/// host's `PATH`.
pub fn find_on_path(command: &str) -> Option<PathBuf> {
    if forge_hostspawn::active() {
        return forge_hostspawn::which(command);
    }
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
            ..Default::default()
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
    fn matches_whole_file_names_and_picks_language_ids_per_extension() {
        let c = LspServerConfig {
            language_ids: vec![
                "typescript".into(),
                "typescriptreact".into(),
                "dockerfile".into(),
            ],
            file_extensions: vec!["ts".into(), "tsx".into(), "=Dockerfile".into()],
            ..cfg()
        };
        assert_eq!(c.match_len(Path::new("/a/Dockerfile")), Some(11));
        assert_eq!(c.match_len(Path::new("/a/x.Dockerfile")), None);
        assert_eq!(
            c.language_id_for(Path::new("/a/App.tsx")),
            "typescriptreact"
        );
        assert_eq!(c.language_id_for(Path::new("/a/main.ts")), "typescript");
        assert_eq!(c.language_id_for(Path::new("/a/dockerfile")), "dockerfile");
        // Without one id per extension the first id is used.
        assert_eq!(cfg().language_id_for(Path::new("/a/x.blade.php")), "php");
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
        assert!(c.env.is_empty() && !c.pull_diagnostics);
        let c: LspServerConfig = serde_json::from_value(json!({
            "id": "k", "languageIds": ["kotlin"], "fileExtensions": ["kt"], "command": "k",
            "env": {"JAVA_HOME": "/jdk"}, "pullDiagnostics": true
        }))
        .unwrap();
        assert_eq!(c.env["JAVA_HOME"], "/jdk");
        assert!(c.pull_diagnostics);
    }
}
