use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;
use tauri::State;

use forge_core::{ForgeError, Result, SettingsScope};
use forge_plugin_api::{PluginInfo, SettingSpec};

use crate::state::AppState;

/// All settings as the UI needs them: effective values plus what each layer overrides.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub values: BTreeMap<String, Value>,
    pub user: BTreeMap<String, Value>,
    pub project: BTreeMap<String, Value>,
    pub has_project: bool,
}

#[tauri::command]
pub fn settings_get_all(state: State<'_, AppState>) -> SettingsSnapshot {
    let s = state.read_settings();
    SettingsSnapshot {
        values: s.effective(),
        user: s.layer_values(SettingsScope::User).clone(),
        project: s.layer_values(SettingsScope::Project).clone(),
        has_project: s.has_project(),
    }
}

/// Store a setting in the `user` or `project` layer; `null` removes the override.
/// Change events reach the UI as `settings://changed`.
#[tauri::command]
pub fn settings_set(
    key: String,
    value: Value,
    scope: SettingsScope,
    state: State<'_, AppState>,
) -> Result<()> {
    if !value.is_null() {
        let plugins = state.plugins.read().expect("plugins lock poisoned");
        let spec = plugins.all_settings().find(|s| s.key == key);
        if let Some(spec) = spec {
            spec.kind
                .validate(&value)
                .map_err(|e| ForgeError::Other(format!("{}: {e}", spec.title)))?;
        }
    }
    state.write_settings().set(&key, value, scope)
}

/// Settings schema contributed by enabled plugins.
#[tauri::command]
pub fn settings_schema(state: State<'_, AppState>) -> Vec<SettingSpec> {
    let plugins = state.plugins.read().expect("plugins lock poisoned");
    plugins.settings_schema().cloned().collect()
}

#[tauri::command]
pub fn plugins_list(state: State<'_, AppState>) -> Vec<PluginInfo> {
    let plugins = state.plugins.read().expect("plugins lock poisoned");
    plugins.list().to_vec()
}

/// Enable or disable a plugin (cascading over dependencies); returns the changed ids.
#[tauri::command]
pub fn plugin_set_enabled(
    id: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<Vec<String>> {
    let changed = state
        .plugins
        .write()
        .expect("plugins lock poisoned")
        .set_enabled(&id, enabled)?;
    state.save_disabled_plugins()?;
    Ok(changed)
}
