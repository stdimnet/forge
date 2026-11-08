//! Layered settings: built-in defaults < user < settings for new projects < project.
//!
//! Keys are dotted (`editor.font.size`) and map to nested TOML tables on disk:
//! the user layer lives in `<config dir>/forge/settings.toml`, the project layer in
//! `<project>/.forge/settings.toml`. PHPStorm's "Settings for New Projects" (the defaults of
//! project-level settings) live in `<config dir>/forge/new-project-settings.toml`; they apply
//! to every project that has no value of its own. Values are JSON values so they cross the IPC boundary
//! unchanged; typed accessors deserialize on read. Writing a value persists its layer and
//! notifies subscribers with the new effective value.
//!
//! **User-only settings** ([`Settings::set_user_only`]: every setting whose schema does not
//! say `project = true`) are IDE settings. A project's `.forge/settings.toml` comes with the
//! project's files, so it never decides them: its values for these keys are ignored on read
//! and refused on write. Otherwise any cloned repository could choose the programs and
//! servers Forge uses for the user (the Git executable, the npm registry, the update feed).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::project_files::SharedFiles;
use crate::{ForgeError, Result};

/// The project layer's file, relative to the project root.
pub const PROJECT_SETTINGS_FILE: &str = ".forge/settings.toml";
/// The file of the Settings for New Projects, next to the user settings.
pub const NEW_PROJECT_SETTINGS_FILE: &str = "new-project-settings.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingsScope {
    Default,
    User,
    /// Defaults of project-level settings for projects without their own value
    /// (File | New Projects Setup | Settings for New Projects).
    #[serde(rename = "newProjects")]
    NewProjects,
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

/// Where a layer is saved.
#[derive(Default)]
enum Store {
    /// Values only live in memory.
    #[default]
    None,
    Path(PathBuf),
    /// A project-relative file, e.g. of a project on a remote host.
    Files(SharedFiles, String),
}

/// One persisted layer: flat dotted keys plus the file they are saved to.
#[derive(Default)]
struct Layer {
    store: Store,
    values: BTreeMap<String, Value>,
}

fn parse_layer(src: &str, describe: impl Fn() -> String) -> Result<BTreeMap<String, Value>> {
    let table: toml::Table = toml::from_str(src)
        .map_err(|e| ForgeError::Other(format!("invalid settings file {}: {e}", describe())))?;
    let mut flat = BTreeMap::new();
    flatten("", toml::Value::Table(table), &mut flat);
    Ok(flat)
}

impl Layer {
    fn load(path: PathBuf) -> Result<Self> {
        let values = match fs::read_to_string(&path) {
            Ok(src) => parse_layer(&src, || path.display().to_string())?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(ForgeError::io(&path, e)),
        };
        Ok(Self {
            store: Store::Path(path),
            values,
        })
    }

    fn load_files(files: SharedFiles, rel: &str) -> Result<Self> {
        let values = match files.read(rel)? {
            Some(src) => parse_layer(&src, || files.describe(rel))?,
            None => BTreeMap::new(),
        };
        Ok(Self {
            store: Store::Files(files, rel.to_string()),
            values,
        })
    }

    fn is_persisted(&self) -> bool {
        !matches!(self.store, Store::None)
    }

    /// Read a local layer's file again (another Forge process may have written it). A file
    /// that cannot be read or parsed keeps the values in memory.
    fn reread(&mut self) {
        let Store::Path(path) = &self.store else {
            return;
        };
        let values = match fs::read_to_string(path) {
            Ok(src) => match parse_layer(&src, || path.display().to_string()) {
                Ok(values) => values,
                Err(_) => return,
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(_) => return,
        };
        self.values = values;
    }

    fn save(&self) -> Result<()> {
        if !self.is_persisted() {
            return Ok(());
        }
        let src = settings_to_toml(&self.values)?;
        match &self.store {
            Store::None => Ok(()),
            Store::Path(path) => {
                if let Some(dir) = path.parent() {
                    fs::create_dir_all(dir).map_err(|e| ForgeError::io(dir, e))?;
                }
                fs::write(path, src).map_err(|e| ForgeError::io(path, e))
            }
            Store::Files(files, rel) => files.write(rel, &src),
        }
    }
}

/// Parse a settings file into flat dotted keys (as the layers hold them).
pub fn parse_settings_toml(src: &str) -> Result<BTreeMap<String, Value>> {
    parse_layer(src, || "settings".to_string())
}

/// Write flat dotted keys as a nested settings file (as the layers save them).
pub fn settings_to_toml(values: &BTreeMap<String, Value>) -> Result<String> {
    let mut root = toml::Table::new();
    for (key, value) in values {
        insert_dotted(&mut root, key, json_to_toml(value)?);
    }
    toml::to_string_pretty(&root)
        .map_err(|e| ForgeError::Other(format!("cannot serialize settings: {e}")))
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
    new_projects: Layer,
    project: Layer,
    /// Keys the project layer never decides (see the module docs).
    user_only: std::collections::BTreeSet<String>,
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

    /// `<config dir>/forge/new-project-settings.toml`: Settings for New Projects.
    pub fn default_new_projects_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("forge").join(NEW_PROJECT_SETTINGS_FILE))
    }

    /// `<root>/.forge/settings.toml`.
    pub fn project_path(root: &Path) -> PathBuf {
        root.join(".forge").join("settings.toml")
    }

    /// Register built-in defaults (typically from the plugin settings schema).
    pub fn set_defaults(&mut self, defaults: impl IntoIterator<Item = (String, Value)>) {
        self.defaults.extend(defaults);
    }

    /// Mark `keys` as user-only: IDE settings that a project's `.forge/settings.toml` never
    /// decides. Their project values are ignored and writing one is refused. The app marks
    /// every schema setting that is not `project = true`.
    pub fn set_user_only(&mut self, keys: impl IntoIterator<Item = String>) {
        self.user_only.extend(keys);
    }

    /// Whether the project layer never decides `key` (see [`Settings::set_user_only`]).
    pub fn is_user_only(&self, key: &str) -> bool {
        self.user_only.contains(key)
    }

    /// Load (or create on first write) the user settings file.
    pub fn load_user(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        self.user = Layer::load(path.into())?;
        Ok(())
    }

    /// Load (or create on first write) the Settings for New Projects.
    pub fn load_new_projects(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        self.new_projects = Layer::load(path.into())?;
        Ok(())
    }

    /// Load the settings of the project at `root`, replacing any previous project layer.
    pub fn load_project(&mut self, root: &Path) -> Result<()> {
        self.project = Layer::load(Self::project_path(root))?;
        Ok(())
    }

    /// Load the project layer through `files` (a project on a remote host reads and writes
    /// its `.forge/settings.toml` through the project backend).
    pub fn load_project_files(&mut self, files: SharedFiles) -> Result<()> {
        self.project = Layer::load_files(files, PROJECT_SETTINGS_FILE)?;
        Ok(())
    }

    /// Forget the project layer (no project is open).
    pub fn close_project(&mut self) {
        self.project = Layer::default();
    }

    pub fn has_project(&self) -> bool {
        self.project.is_persisted()
    }

    /// Read the user settings and the Settings for New Projects from disk again: every
    /// project window is a Forge process of its own, and another one may have changed them.
    /// Listeners hear about each effective value that changed.
    pub fn reload_shared(&mut self) {
        self.reload_layers(true, true);
    }

    fn reload_layers(&mut self, user: bool, new_projects: bool) {
        let keys: std::collections::BTreeSet<String> = self
            .user
            .values
            .keys()
            .chain(self.new_projects.values.keys())
            .cloned()
            .collect();
        let before: BTreeMap<String, Option<Value>> = keys
            .iter()
            .map(|k| (k.clone(), self.get(k).cloned()))
            .collect();
        if user {
            self.user.reread();
        }
        if new_projects {
            self.new_projects.reread();
        }
        let mut changed: std::collections::BTreeSet<String> = self
            .user
            .values
            .keys()
            .chain(self.new_projects.values.keys())
            .filter(|k| !before.contains_key(*k))
            .cloned()
            .collect();
        for (key, old) in &before {
            if self.get(key) != old.as_ref() {
                changed.insert(key.clone());
            }
        }
        for key in changed {
            let after = self.resolve(&key);
            let change = SettingsChange {
                value: after.map(|(v, _)| v.clone()).unwrap_or(Value::Null),
                scope: after.map_or(SettingsScope::Default, |(_, s)| s),
                key,
            };
            for l in &self.listeners {
                l(&change);
            }
        }
    }

    /// Call `f` after every change of an effective value.
    pub fn subscribe(&mut self, f: impl Fn(&SettingsChange) + Send + Sync + 'static) {
        self.listeners.push(Box::new(f));
    }

    fn layer(&self, scope: SettingsScope) -> Option<&Layer> {
        match scope {
            SettingsScope::Default => None,
            SettingsScope::User => Some(&self.user),
            SettingsScope::NewProjects => Some(&self.new_projects),
            SettingsScope::Project => Some(&self.project),
        }
    }

    /// The effective value of `key` and the layer it comes from. The project layer is
    /// skipped for user-only keys.
    pub fn resolve(&self, key: &str) -> Option<(&Value, SettingsScope)> {
        let user_only = self.is_user_only(key);
        [
            SettingsScope::Project,
            SettingsScope::NewProjects,
            SettingsScope::User,
        ]
        .into_iter()
        .filter(|s| !(user_only && *s == SettingsScope::Project))
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

    /// Typed read that ignores the project's own `.forge/settings.toml` (Safe Mode: an
    /// untrusted project must not choose the programs Forge starts, such as the shell).
    pub fn get_without_project_as<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        [SettingsScope::NewProjects, SettingsScope::User]
            .into_iter()
            .find_map(|s| self.layer(s)?.values.get(key))
            .or_else(|| self.defaults.get(key))
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
        out.extend(self.new_projects.values.clone());
        out.extend(
            self.project
                .values
                .iter()
                .filter(|(k, _)| !self.is_user_only(k))
                .map(|(k, v)| (k.clone(), v.clone())),
        );
        out
    }

    /// Store `value` for `key` in `scope` and persist that layer. `Value::Null` removes the
    /// key from the layer so the value is inherited again. Values must keep the JSON type of
    /// their default.
    pub fn set(&mut self, key: &str, value: Value, scope: SettingsScope) -> Result<()> {
        self.set_many(vec![(key.to_string(), value, scope)])
    }

    /// Store several values (as [`Settings::set`] does), saving each changed layer once.
    /// Nothing is stored when one of the values is invalid.
    pub fn set_many(&mut self, changes: Vec<(String, Value, SettingsScope)>) -> Result<()> {
        for (key, value, scope) in &changes {
            if key.is_empty() || key.split('.').any(str::is_empty) {
                return Err(ForgeError::Other(format!("invalid settings key `{key}`")));
            }
            if let Some(default) = self.defaults.get(key) {
                if !value.is_null() && !same_kind(default, value) {
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
                // Removing a stray value is fine; storing one would have no effect.
                SettingsScope::Project if self.is_user_only(key) && !value.is_null() => {
                    return Err(ForgeError::Other(format!(
                        "`{key}` is an IDE setting and cannot be stored in the project"
                    )))
                }
                _ => {}
            }
            if !value.is_null() {
                json_to_toml(value)?;
            }
        }
        // Another project window may have written the shared layers since they were read:
        // start from the file, so its changes are not overwritten.
        let writes = |s: SettingsScope| changes.iter().any(|(_, _, scope)| *scope == s);
        let (shared_user, shared_new) = (
            writes(SettingsScope::User),
            writes(SettingsScope::NewProjects),
        );
        if shared_user || shared_new {
            self.reload_layers(shared_user, shared_new);
        }
        let before: Vec<Option<Value>> = changes
            .iter()
            .map(|(key, _, _)| self.get(key).cloned())
            .collect();
        let (mut user, mut new_projects, mut project) = (false, false, false);
        for (key, value, scope) in &changes {
            let layer = match scope {
                SettingsScope::Project => {
                    project = true;
                    &mut self.project
                }
                SettingsScope::NewProjects => {
                    new_projects = true;
                    &mut self.new_projects
                }
                _ => {
                    user = true;
                    &mut self.user
                }
            };
            if value.is_null() {
                layer.values.remove(key);
            } else {
                layer.values.insert(key.clone(), value.clone());
            }
        }
        if user {
            self.user.save()?;
        }
        if new_projects {
            self.new_projects.save()?;
        }
        if project {
            self.project.save()?;
        }
        let mut notified = std::collections::BTreeSet::new();
        for ((key, _, _), before) in changes.iter().zip(before) {
            if !notified.insert(key.clone()) {
                continue;
            }
            let after = self.resolve(key);
            if before.as_ref() != after.map(|(v, _)| v) {
                let change = SettingsChange {
                    key: key.clone(),
                    value: after.map(|(v, _)| v.clone()).unwrap_or(Value::Null),
                    scope: after.map_or(SettingsScope::Default, |(_, s)| s),
                };
                for l in &self.listeners {
                    l(&change);
                }
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

    #[test]
    fn two_processes_do_not_overwrite_each_others_user_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let open = || {
            let mut s = Settings::new();
            s.load_user(&path).unwrap();
            s
        };
        let mut a = open();
        let mut b = open();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        b.subscribe(move |c| log.lock().unwrap().push((c.key.clone(), c.value.clone())));
        a.set("appearance.theme", json!("light"), SettingsScope::User)
            .unwrap();
        // B writes another key: A's change stays in the file, and B learns about it.
        b.set("editor.font.size", json!(16), SettingsScope::User)
            .unwrap();
        assert_eq!(b.get("appearance.theme"), Some(&json!("light")));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("light") && text.contains("16"), "{text}");
        assert!(seen
            .lock()
            .unwrap()
            .contains(&("appearance.theme".to_string(), json!("light"))));
        // A picks B's change up when it reloads (the window got the focus).
        a.reload_shared();
        assert_eq!(a.get("editor.font.size"), Some(&json!(16)));
        // A removed key is reported too.
        std::fs::write(&path, "[editor.font]\nsize = 16\n").unwrap();
        a.reload_shared();
        assert_eq!(a.get("appearance.theme"), None);
    }

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
    fn projects_never_decide_user_only_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = settings(tmp.path());
        s.set_defaults([
            ("updates.endpoint".to_string(), json!("")),
            ("vcs.git.path".to_string(), json!("")),
        ]);
        s.set_user_only(["updates.endpoint".to_string(), "vcs.git.path".to_string()]);
        // A cloned repository ships its own `.forge/settings.toml`.
        let project = tmp.path().join("project");
        fs::create_dir_all(project.join(".forge")).unwrap();
        fs::write(
            project.join(".forge/settings.toml"),
            "[updates]\nendpoint = \"https://evil.example/latest.json\"\n\
             [vcs.git]\npath = \"./.forge/x.sh\"\n\
             [editor.font]\nsize = 20\n",
        )
        .unwrap();
        s.load_project(&project).unwrap();
        assert_eq!(s.get_str("updates.endpoint"), Some(""));
        assert_eq!(s.get_as::<String>("vcs.git.path").as_deref(), Some(""));
        assert_eq!(s.resolve("vcs.git.path").unwrap().1, SettingsScope::Default);
        assert!(!s.effective().values().any(|v| v == &json!("./.forge/x.sh")));
        // Keys without the mark still follow the project.
        assert_eq!(s.get_i64("editor.font.size"), Some(20));
        // The user's own value applies.
        s.set("vcs.git.path", json!("/usr/bin/git"), SettingsScope::User)
            .unwrap();
        assert_eq!(s.get_str("vcs.git.path"), Some("/usr/bin/git"));
        // Storing one in the project is refused; removing the stray value works.
        assert!(s
            .set("vcs.git.path", json!("./x"), SettingsScope::Project)
            .is_err());
        s.set("vcs.git.path", Value::Null, SettingsScope::Project)
            .unwrap();
        assert!(!s
            .layer_values(SettingsScope::Project)
            .contains_key("vcs.git.path"));
    }

    #[test]
    fn reads_without_the_project_layer() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = settings(tmp.path());
        let project = tmp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        s.load_project(&project).unwrap();
        s.set(
            "terminal.shellPath",
            json!("./evil.sh"),
            SettingsScope::Project,
        )
        .unwrap();
        assert_eq!(s.get_str("terminal.shellPath"), Some("./evil.sh"));
        assert_eq!(
            s.get_without_project_as::<String>("terminal.shellPath"),
            None
        );
        s.set("terminal.shellPath", json!("/bin/zsh"), SettingsScope::User)
            .unwrap();
        assert_eq!(
            s.get_without_project_as::<String>("terminal.shellPath")
                .as_deref(),
            Some("/bin/zsh")
        );
        assert_eq!(
            s.get_without_project_as::<i64>("editor.font.size"),
            Some(14)
        );
    }

    #[test]
    fn set_many_saves_once_and_notifies_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = settings(tmp.path());
        let project = tmp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        s.load_project(&project).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        s.subscribe(move |c| sink.lock().unwrap().push(c.key.clone()));
        s.set_many(vec![
            ("php.codeStyle.indent".into(), json!(2), SettingsScope::User),
            (
                "php.codeStyle.useTabs".into(),
                json!(true),
                SettingsScope::Project,
            ),
            ("editor.font.size".into(), json!(14), SettingsScope::User),
        ])
        .unwrap();
        // The font size did not change.
        assert_eq!(
            *seen.lock().unwrap(),
            vec!["php.codeStyle.indent", "php.codeStyle.useTabs"]
        );
        let user = fs::read_to_string(tmp.path().join("user/settings.toml")).unwrap();
        assert!(user.contains("indent = 2"));
        assert!(fs::read_to_string(project.join(PROJECT_SETTINGS_FILE))
            .unwrap()
            .contains("useTabs = true"));
        // One invalid value: nothing is stored.
        assert!(s
            .set_many(vec![
                ("a.b".into(), json!(1), SettingsScope::User),
                ("editor.font.size".into(), json!("x"), SettingsScope::User),
            ])
            .is_err());
        assert_eq!(s.get("a.b"), None);
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
    fn settings_for_new_projects_apply_to_projects_without_own_values() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = settings(tmp.path());
        let defaults_file = tmp.path().join("user").join(NEW_PROJECT_SETTINGS_FILE);
        s.load_new_projects(&defaults_file).unwrap();
        s.set("editor.font.size", json!(16), SettingsScope::User)
            .unwrap();
        s.set("editor.font.size", json!(20), SettingsScope::NewProjects)
            .unwrap();
        // The defaults for projects win over the user value of a project-level setting.
        assert_eq!(
            s.resolve("editor.font.size"),
            Some((&json!(20), SettingsScope::NewProjects))
        );
        assert!(fs::read_to_string(&defaults_file)
            .unwrap()
            .contains("size = 20"));

        // A newly opened project without its own value gets the default ...
        let fresh = tmp.path().join("fresh");
        fs::create_dir_all(&fresh).unwrap();
        s.load_project(&fresh).unwrap();
        assert_eq!(s.get_i64("editor.font.size"), Some(20));
        assert_eq!(s.effective()["editor.font.size"], json!(20));

        // ... a project with its own value keeps it.
        let own = tmp.path().join("own");
        fs::create_dir_all(own.join(".forge")).unwrap();
        fs::write(
            own.join(PROJECT_SETTINGS_FILE),
            "[editor.font]\nsize = 11\n",
        )
        .unwrap();
        s.load_project(&own).unwrap();
        assert_eq!(
            s.resolve("editor.font.size"),
            Some((&json!(11), SettingsScope::Project))
        );

        // Reloaded from disk, and removable again.
        let mut again = settings(tmp.path());
        again.load_new_projects(&defaults_file).unwrap();
        assert_eq!(again.get_i64("editor.font.size"), Some(20));
        again
            .set("editor.font.size", Value::Null, SettingsScope::NewProjects)
            .unwrap();
        assert_eq!(again.get_i64("editor.font.size"), Some(16));
        assert_eq!(
            serde_json::to_value(SettingsScope::NewProjects).unwrap(),
            json!("newProjects")
        );
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
    fn persists_lists_of_tables() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = settings(tmp.path());
        let entries = json!([
            { "template": "smarty", "language": "latex", "left": "<<", "right": ">>" },
            { "template": "twig", "language": "latex", "blockStart": "\\BLOCK{" },
        ]);
        s.set("templates.delimiters", entries.clone(), SettingsScope::User)
            .unwrap();
        let src = fs::read_to_string(tmp.path().join("user/settings.toml")).unwrap();
        assert!(src.contains("[[templates.delimiters]]"), "{src}");

        let reloaded = settings(tmp.path());
        assert_eq!(reloaded.get("templates.delimiters"), Some(&entries));
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
    fn project_layer_goes_through_project_files() {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = settings(tmp.path());
        let files = Arc::new(crate::project_files::MemoryFiles::new());
        files.insert(PROJECT_SETTINGS_FILE, "[editor.font]\nsize = 20\n");
        s.load_project_files(files.clone()).unwrap();
        assert_eq!(s.get_i64("editor.font.size"), Some(20));
        s.set("appearance.theme", json!("light"), SettingsScope::Project)
            .unwrap();
        let saved = files.get(PROJECT_SETTINGS_FILE).unwrap();
        assert!(saved.contains("theme = \"light\""), "{saved}");
        assert!(saved.contains("size = 20"), "{saved}");

        files.insert(PROJECT_SETTINGS_FILE, "not = [valid");
        assert!(s.load_project_files(files).is_err());
        s.close_project();
        assert!(!s.has_project());
        assert_eq!(s.get_i64("editor.font.size"), Some(14));
    }

    #[test]
    fn reports_invalid_files() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.toml");
        fs::write(&path, "not = [valid").unwrap();
        assert!(Settings::new().load_user(&path).is_err());
    }
}
