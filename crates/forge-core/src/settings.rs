//! Layered settings: built-in defaults < user < project.
//!
//! Keys are dotted (`editor.font.size`) and map to nested TOML tables on disk:
//! the user layer lives in `<config dir>/forge/settings.toml`, the project layer in
//! `<project>/.forge/settings.toml`. Values are JSON values so they cross the IPC boundary
//! unchanged; typed accessors deserialize on read. Writing a value persists its layer and
//! notifies subscribers with the new effective value.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ForgeError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingsScope {
    Default,
    User,
    Project,
}

/// Emitted after a change to the effective value of a key.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SettingsChange {
    pub key: String,
    /// The new effective value (`null` if the key is gone entirely).
    pub value: Value,
    /// Layer the effective value now comes from.
    pub scope: SettingsScope,
}

type Listener = Box<dyn Fn(&SettingsChange) + Send + Sync>;

/// One persisted layer: flat dotted keys plus the file they are saved to.
#[derive(Debug, Default)]
struct Layer {
    path: Option<PathBuf>,
    values: BTreeMap<String, Value>,
}

impl Layer {
    fn load(path: PathBuf) -> Result<Self> {
        let values = match fs::read_to_string(&path) {
            Ok(src) => {
                let table: toml::Table = toml::from_str(&src).map_err(|e| {
                    ForgeError::Other(format!("invalid settings file {}: {e}", path.display()))
                })?;
                let mut flat = BTreeMap::new();
                flatten("", toml::Value::Table(table), &mut flat);
                flat
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(ForgeError::io(&path, e)),
        };
        Ok(Self {
            path: Some(path),
            values,
        })
    }

    fn save(&self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let mut root = toml::Table::new();
        for (key, value) in &self.values {
            insert_dotted(&mut root, key, json_to_toml(value)?);
        }
        let src = toml::to_string_pretty(&root)
            .map_err(|e| ForgeError::Other(format!("cannot serialize settings: {e}")))?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| ForgeError::io(dir, e))?;
        }
        fs::write(path, src).map_err(|e| ForgeError::io(path, e))
    }
}

/// Flatten nested TOML tables into dotted keys. Arrays and scalars are leaves.
fn flatten(prefix: &str, value: toml::Value, out: &mut BTreeMap<String, Value>) {
    match value {
        toml::Value::Table(t) => {
            for (k, v) in t {
                let key = if prefix.is_empty() {
                    k
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(&key, v, out);
            }
        }
        // TOML values always map onto JSON (datetimes become strings).
        other => {
            let json = serde_json::to_value(other).unwrap_or(Value::Null);
            out.insert(prefix.to_string(), json);
        }
    }
}

fn insert_dotted(root: &mut toml::Table, key: &str, value: toml::Value) {
    let mut parts: Vec<&str> = key.split('.').collect();
    let last = parts.pop().unwrap_or(key);
    let mut table = root;
    for part in parts {
        let entry = table
            .entry(part)
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if !entry.is_table() {
            // A key cannot be both a value and a table; the nested key wins.
            *entry = toml::Value::Table(toml::Table::new());
        }
        table = entry.as_table_mut().expect("just ensured a table");
    }
    table.insert(last.to_string(), value);
}

fn json_to_toml(value: &Value) -> Result<toml::Value> {
    toml::Value::try_from(value)
        .map_err(|e| ForgeError::Other(format!("value cannot be stored in settings: {e}")))
}

/// Same JSON kind (numbers of any flavour count as one kind).
fn same_kind(a: &Value, b: &Value) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// The layered settings store.
#[derive(Default)]
pub struct Settings {
    defaults: BTreeMap<String, Value>,
    user: Layer,
    project: Layer,
    listeners: Vec<Listener>,
}

impl Settings {
    /// Settings with no files attached (values only live in memory until a layer is loaded).
    pub fn new() -> Self {
        Self::default()
    }

    /// `<config dir>/forge/settings.toml`, e.g. `~/.config/forge/settings.toml` on Linux.
    pub fn default_user_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("forge").join("settings.toml"))
    }

    /// `<root>/.forge/settings.toml`.
    pub fn project_path(root: &Path) -> PathBuf {
        root.join(".forge").join("settings.toml")
    }

    /// Register built-in defaults (typically from the plugin settings schema).
    pub fn set_defaults(&mut self, defaults: impl IntoIterator<Item = (String, Value)>) {
        self.defaults.extend(defaults);
    }

    /// Load (or create on first write) the user settings file.
    pub fn load_user(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        self.user = Layer::load(path.into())?;
        Ok(())
    }

    /// Load the settings of the project at `root`, replacing any previous project layer.
    pub fn load_project(&mut self, root: &Path) -> Result<()> {
        self.project = Layer::load(Self::project_path(root))?;
        Ok(())
    }

    pub fn has_project(&self) -> bool {
        self.project.path.is_some()
    }

    /// Call `f` after every change of an effective value.
    pub fn subscribe(&mut self, f: impl Fn(&SettingsChange) + Send + Sync + 'static) {
        self.listeners.push(Box::new(f));
    }

    fn layer(&self, scope: SettingsScope) -> Option<&Layer> {
        match scope {
            SettingsScope::Default => None,
            SettingsScope::User => Some(&self.user),
            SettingsScope::Project => Some(&self.project),
        }
    }

    /// The effective value of `key` and the layer it comes from.
    pub fn resolve(&self, key: &str) -> Option<(&Value, SettingsScope)> {
        [SettingsScope::Project, SettingsScope::User]
            .into_iter()
            .find_map(|s| self.layer(s)?.values.get(key).map(|v| (v, s)))
            .or_else(|| self.defaults.get(key).map(|v| (v, SettingsScope::Default)))
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.resolve(key).map(|(v, _)| v)
    }

    /// Typed read, e.g. `settings.get_as::<u32>("editor.font.size")`.
    pub fn get_as<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(Value::as_bool)
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(Value::as_i64)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }

    /// Values explicitly stored in one layer (defaults for [`SettingsScope::Default`]).
    pub fn layer_values(&self, scope: SettingsScope) -> &BTreeMap<String, Value> {
        match self.layer(scope) {
            Some(l) => &l.values,
            None => &self.defaults,
        }
    }

    /// All effective values, merged across layers.
    pub fn effective(&self) -> BTreeMap<String, Value> {
        let mut out = self.defaults.clone();
        out.extend(self.user.values.clone());
        out.extend(self.project.values.clone());
        out
    }

    /// Store `value` for `key` in `scope` and persist that layer. `Value::Null` removes the
    /// key from the layer so the value is inherited again. Values must keep the JSON type of
    /// their default.
    pub fn set(&mut self, key: &str, value: Value, scope: SettingsScope) -> Result<()> {
        if key.is_empty() || key.split('.').any(str::is_empty) {
            return Err(ForgeError::Other(format!("invalid settings key `{key}`")));
        }
        if let Some(default) = self.defaults.get(key) {
            if !value.is_null() && !same_kind(default, &value) {
                return Err(ForgeError::Other(format!(
                    "`{key}` expects a value like {default}, got {value}"
                )));
            }
        }
        match scope {
            SettingsScope::Default => {
                return Err(ForgeError::Other("built-in defaults are read-only".into()))
            }
            SettingsScope::Project if !self.has_project() => {
                return Err(ForgeError::Other("no project is open".into()))
            }
            _ => {}
        }
        if !value.is_null() {
            json_to_toml(&value)?;
        }
        let before = self.get(key).cloned();
        let layer = match scope {
            SettingsScope::Project => &mut self.project,
            _ => &mut self.user,
        };
        if value.is_null() {
            layer.values.remove(key);
        } else {
            layer.values.insert(key.to_string(), value);
        }
        layer.save()?;
        let after = self.resolve(key);
        if before.as_ref() != after.map(|(v, _)| v) {
            let change = SettingsChange {
                key: key.to_string(),
                value: after.map(|(v, _)| v.clone()).unwrap_or(Value::Null),
                scope: after.map_or(SettingsScope::Default, |(_, s)| s),
            };
            for l in &self.listeners {
                l(&change);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    fn settings(dir: &Path) -> Settings {
        let mut s = Settings::new();
        s.set_defaults([
            ("editor.font.size".to_string(), json!(14)),
            ("editor.font.family".to_string(), json!("JetBrains Mono")),
            ("appearance.theme".to_string(), json!("dark")),
        ]);
        s.load_user(dir.join("user/settings.toml")).unwrap();
        s
    }

    #[test]
    fn layers_override_in_order() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = settings(tmp.path());
        assert_eq!(s.get_i64("editor.font.size"), Some(14));

        s.set("editor.font.size", json!(16), SettingsScope::User)
            .unwrap();
        assert_eq!(s.get_as::<u32>("editor.font.size"), Some(16));

        let project = tmp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        s.load_project(&project).unwrap();
        s.set("editor.font.size", json!(18), SettingsScope::Project)
            .unwrap();
        assert_eq!(
            s.resolve("editor.font.size"),
            Some((&json!(18), SettingsScope::Project))
        );

        // Removing the project value falls back to the user value.
        s.set("editor.font.size", Value::Null, SettingsScope::Project)
            .unwrap();
        assert_eq!(s.get_i64("editor.font.size"), Some(16));
        assert_eq!(s.effective()["appearance.theme"], json!("dark"));
    }

    #[test]
    fn persists_as_nested_toml_and_reloads() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = settings(tmp.path());
        s.set(
            "editor.font.family",
            json!("Fira Code"),
            SettingsScope::User,
        )
        .unwrap();
        s.set(
            "php.includePaths",
            json!(["vendor", "lib"]),
            SettingsScope::User,
        )
        .unwrap();
        let src = fs::read_to_string(tmp.path().join("user/settings.toml")).unwrap();
        assert!(src.contains("[editor.font]"), "{src}");
        assert!(src.contains("family = \"Fira Code\""), "{src}");

        let reloaded = settings(tmp.path());
        assert_eq!(reloaded.get_str("editor.font.family"), Some("Fira Code"));
        assert_eq!(
            reloaded.get_as::<Vec<String>>("php.includePaths"),
            Some(vec!["vendor".into(), "lib".into()])
        );
    }

    #[test]
    fn rejects_type_mismatches_and_bad_scopes() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = settings(tmp.path());
        assert!(s
            .set("editor.font.size", json!("big"), SettingsScope::User)
            .is_err());
        assert!(s
            .set("editor.font.size", json!(12), SettingsScope::Default)
            .is_err());
        assert!(s
            .set("editor.font.size", json!(12), SettingsScope::Project)
            .is_err());
        assert!(s.set("bad..key", json!(1), SettingsScope::User).is_err());
    }

    #[test]
    fn notifies_on_effective_changes_only() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = settings(tmp.path());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        s.subscribe(move |c| sink.lock().unwrap().push(c.clone()));

        s.set("appearance.theme", json!("light"), SettingsScope::User)
            .unwrap();
        // Same value again: no event.
        s.set("appearance.theme", json!("light"), SettingsScope::User)
            .unwrap();
        s.set("appearance.theme", Value::Null, SettingsScope::User)
            .unwrap();

        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].value, json!("light"));
        assert_eq!(seen[0].scope, SettingsScope::User);
        assert_eq!(seen[1].value, json!("dark"));
        assert_eq!(seen[1].scope, SettingsScope::Default);
    }

    #[test]
    fn reports_invalid_files() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.toml");
        fs::write(&path, "not = [valid").unwrap();
        assert!(Settings::new().load_user(&path).is_err());
    }
}
