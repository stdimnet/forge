//! User preferences for language servers, read from Forge settings.
//!
//! Global options are declared in `app/src-tauri/plugins/langservers.toml`. Per-server
//! values live under `langservers.servers.<id>`:
//!
//! - `enabled` (bool): overrides the catalog default,
//! - `path` (string): custom executable, skips detection,
//! - `settings` (string): JSON merged into the server's settings; a top-level
//!   `initializationOptions` object is merged into the initialization options instead.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Serialize;
use serde_json::Value;

use crate::scan::DEFAULT_SCAN_LIMIT;

pub const AUTO_START_KEY: &str = "langservers.autoStart";
pub const SCAN_LIMIT_KEY: &str = "langservers.scanLimit";
pub const LUA_ENVIRONMENT_KEY: &str = "langservers.lua.environment";
pub const LUA_VERSION_KEY: &str = "langservers.lua.version";
/// Extra `workspace.library` folders of lua-language-server.
pub const LUA_LIBRARIES_KEY: &str = "langservers.lua.libraries";
/// Settings | Editor | General | Auto Import | JavaScript and TypeScript: how import paths
/// are written and whether TypeScript gets `import type`.
pub const JS_IMPORT_PATHS_KEY: &str = "javascript.autoImport.pathStyle";
pub const TS_TYPE_IMPORTS_KEY: &str = "javascript.autoImport.typeImports";
/// The project's Python interpreter (PyForge): a local executable path or a configured
/// interpreter's name.
pub const PYTHON_INTERPRETER_KEY: &str = "python.interpreter";
/// Start basedpyright, ruff and ty from the selected environment when it has them.
pub const PYTHON_ENV_TOOLS_KEY: &str = "python.lsp.useEnvironmentTools";

/// Settings key of a per-server value (`enabled`, `path` or `settings`).
pub fn server_key(id: &str, field: &str) -> String {
    format!("langservers.servers.{id}.{field}")
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerPrefs {
    pub enabled: Option<bool>,
    pub path: Option<PathBuf>,
    /// Raw JSON as typed by the user.
    pub settings_json: Option<String>,
}

impl ServerPrefs {
    /// The parsed extra settings; `Err` with a message when the JSON is invalid.
    pub fn extra_settings(&self) -> Result<Option<Value>, String> {
        match self.settings_json.as_deref().map(str::trim) {
            None | Some("") => Ok(None),
            Some(src) => match serde_json::from_str::<Value>(src) {
                Ok(v @ Value::Object(_)) => Ok(Some(v)),
                Ok(_) => Err("extra settings must be a JSON object".into()),
                Err(e) => Err(format!("invalid JSON: {e}")),
            },
        }
    }
}

/// Lua runtime the Lua language server is configured for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LuaEnvironment {
    /// LÖVE or Neovim when the project looks like one, else plain Lua.
    Auto,
    Plain,
    Love,
    Neovim,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prefs {
    /// Start servers for the languages found in a project when it opens.
    pub auto_start: bool,
    pub scan_limit: usize,
    pub lua_environment: LuaEnvironment,
    /// `Lua 5.1` … `Lua 5.4`, `LuaJIT`.
    pub lua_version: String,
    /// Extra library folders for lua-language-server.
    pub lua_libraries: Vec<String>,
    /// The TypeScript server's `importModuleSpecifierPreference`: `shortest`,
    /// `project-relative`, `relative` or `non-relative`.
    pub js_import_paths: String,
    /// The TypeScript server's `preferTypeOnlyAutoImports`.
    pub ts_type_imports: bool,
    /// The project's local Python executable (`python.pythonPath` of Pyright-based servers).
    pub python_path: Option<PathBuf>,
    /// Look for Python language servers in the interpreter's environment first.
    pub python_env_tools: bool,
    pub servers: HashMap<String, ServerPrefs>,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            auto_start: true,
            scan_limit: DEFAULT_SCAN_LIMIT,
            lua_environment: LuaEnvironment::Auto,
            lua_version: "Lua 5.4".into(),
            lua_libraries: Vec::new(),
            js_import_paths: "shortest".into(),
            ts_type_imports: false,
            python_path: None,
            python_env_tools: true,
            servers: HashMap::new(),
        }
    }
}

impl Prefs {
    /// Read preferences through `get` (a settings lookup) for the given server ids.
    pub fn load<'a>(
        get: impl Fn(&str) -> Option<Value>,
        server_ids: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let defaults = Prefs::default();
        let string = |key: &str| {
            get(key)
                .and_then(|v| v.as_str().map(str::to_owned))
                .filter(|s| !s.trim().is_empty())
        };
        let servers = server_ids
            .into_iter()
            .filter_map(|id| {
                let p = ServerPrefs {
                    enabled: get(&server_key(id, "enabled")).and_then(|v| v.as_bool()),
                    path: string(&server_key(id, "path")).map(|s| PathBuf::from(s.trim())),
                    settings_json: string(&server_key(id, "settings")),
                };
                (p != ServerPrefs::default()).then(|| (id.to_string(), p))
            })
            .collect();
        Prefs {
            auto_start: get(AUTO_START_KEY)
                .and_then(|v| v.as_bool())
                .unwrap_or(defaults.auto_start),
            scan_limit: get(SCAN_LIMIT_KEY)
                .and_then(|v| v.as_u64())
                .map(|n| n.clamp(100, 1_000_000) as usize)
                .unwrap_or(defaults.scan_limit),
            lua_environment: match string(LUA_ENVIRONMENT_KEY).as_deref() {
                Some("love") => LuaEnvironment::Love,
                Some("neovim") => LuaEnvironment::Neovim,
                Some("plain") => LuaEnvironment::Plain,
                _ => LuaEnvironment::Auto,
            },
            lua_version: string(LUA_VERSION_KEY).unwrap_or(defaults.lua_version),
            lua_libraries: get(LUA_LIBRARIES_KEY)
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default()
                .iter()
                .filter_map(|v| v.as_str().map(str::trim).filter(|s| !s.is_empty()))
                .map(str::to_string)
                .collect(),
            js_import_paths: match string(JS_IMPORT_PATHS_KEY).as_deref() {
                Some(p @ ("project-relative" | "relative" | "non-relative")) => p.to_string(),
                _ => defaults.js_import_paths,
            },
            ts_type_imports: get(TS_TYPE_IMPORTS_KEY)
                .and_then(|v| v.as_bool())
                .unwrap_or(defaults.ts_type_imports),
            // Only a path names a local executable; a configured interpreter (Docker, SSH)
            // is not something the servers can run.
            python_path: string(PYTHON_INTERPRETER_KEY)
                .map(|p| p.trim().to_string())
                .filter(|p| p.contains('/') || p.contains('\\'))
                .map(PathBuf::from)
                .filter(|p| p.is_absolute()),
            python_env_tools: get(PYTHON_ENV_TOOLS_KEY)
                .and_then(|v| v.as_bool())
                .unwrap_or(defaults.python_env_tools),
            servers,
        }
    }

    /// The selected Python environment's executable folder, searched for language
    /// servers first (empty when it is off or no local interpreter is selected).
    pub fn python_tool_dirs(&self) -> Vec<PathBuf> {
        if !self.python_env_tools {
            return Vec::new();
        }
        self.python_path
            .as_deref()
            .and_then(std::path::Path::parent)
            .map(|d| vec![d.to_path_buf()])
            .unwrap_or_default()
    }

    pub fn server(&self, id: &str) -> ServerPrefs {
        self.servers.get(id).cloned().unwrap_or_default()
    }

    /// Effective enablement: the user's choice, else the catalog default.
    pub fn is_enabled(&self, id: &str, default: bool) -> bool {
        self.servers
            .get(id)
            .and_then(|p| p.enabled)
            .unwrap_or(default)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[cfg(unix)]
    #[test]
    fn python_interpreter_paths() {
        let load = |v: Value, tools: bool| {
            Prefs::load(
                |k| match k {
                    PYTHON_INTERPRETER_KEY => Some(v.clone()),
                    PYTHON_ENV_TOOLS_KEY => Some(json!(tools)),
                    _ => None,
                },
                [],
            )
        };
        let p = load(json!("/work/shop/.venv/bin/python"), true);
        assert_eq!(
            p.python_tool_dirs(),
            vec![PathBuf::from("/work/shop/.venv/bin")]
        );
        assert!(load(json!("/work/shop/.venv/bin/python"), false)
            .python_tool_dirs()
            .is_empty());
        // A configured interpreter's name is not a local executable.
        assert_eq!(load(json!("Docker python:3.12"), true).python_path, None);
        assert_eq!(load(json!("venv/bin/python"), true).python_path, None);
    }

    #[test]
    fn reads_global_and_per_server_values() {
        let values: HashMap<String, Value> = [
            (AUTO_START_KEY, json!(false)),
            (SCAN_LIMIT_KEY, json!(5)),
            (LUA_ENVIRONMENT_KEY, json!("love")),
            (LUA_LIBRARIES_KEY, json!(["libs", " ", "~/lua"])),
            ("langservers.servers.gopls.enabled", json!(false)),
            ("langservers.servers.gopls.path", json!(" /opt/gopls ")),
            ("langservers.servers.zls.settings", json!("{\"zls\": {}}")),
            ("langservers.servers.taplo.path", json!("")),
            (TS_TYPE_IMPORTS_KEY, json!(true)),
            (JS_IMPORT_PATHS_KEY, json!("bogus")),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        let p = Prefs::load(
            |k| values.get(k).cloned(),
            ["gopls", "zls", "taplo", "clangd"],
        );
        assert!(!p.auto_start);
        assert_eq!(p.scan_limit, 100, "clamped");
        assert_eq!(p.lua_environment, LuaEnvironment::Love);
        assert_eq!(p.lua_version, "Lua 5.4");
        assert_eq!(p.lua_libraries, vec!["libs", "~/lua"]);
        assert_eq!(p.js_import_paths, "shortest");
        assert!(p.ts_type_imports);
        assert!(!p.is_enabled("gopls", true));
        assert!(p.is_enabled("clangd", true));
        assert!(!p.is_enabled("clangd", false));
        assert_eq!(p.server("gopls").path, Some(PathBuf::from("/opt/gopls")));
        assert_eq!(
            p.server("zls").extra_settings().unwrap(),
            Some(json!({"zls": {}}))
        );
        assert!(
            !p.servers.contains_key("taplo"),
            "empty values are no preference"
        );
    }

    #[test]
    fn invalid_extra_settings_are_reported() {
        let p = ServerPrefs {
            settings_json: Some("{nope".into()),
            ..Default::default()
        };
        assert!(p.extra_settings().unwrap_err().starts_with("invalid JSON"));
        let p = ServerPrefs {
            settings_json: Some("[1]".into()),
            ..Default::default()
        };
        assert!(p.extra_settings().is_err());
    }
}
