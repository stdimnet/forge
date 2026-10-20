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
            servers,
        }
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
