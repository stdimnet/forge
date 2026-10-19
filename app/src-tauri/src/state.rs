use std::sync::RwLock;

use forge_core::{ForgeError, Settings, SettingsScope, Workspace};
use forge_plugin_api::PluginRegistry;

use crate::plugins::{self, DISABLED_PLUGINS_KEY};

/// Shared application state managed by Tauri.
#[derive(Default)]
pub struct AppState {
    workspace: RwLock<Option<Workspace>>,
    pub settings: RwLock<Settings>,
    pub plugins: RwLock<PluginRegistry>,
}

impl AppState {
    /// State with the built-in plugins registered, their setting defaults applied, and the
    /// user settings (including disabled plugins) loaded.
    pub fn new() -> Self {
        let registry = plugins::builtin_registry().unwrap_or_else(|e| {
            tracing::error!("{e}");
            PluginRegistry::new()
        });
        let mut settings = Settings::new();
        settings.set_defaults(
            registry
                .all_settings()
                .map(|s| (s.key.clone(), s.default.clone())),
        );
        if let Some(path) = Settings::default_user_path() {
            if let Err(e) = settings.load_user(path) {
                tracing::warn!("user settings not loaded: {e}");
            }
        }
        let state = Self {
            workspace: RwLock::default(),
            settings: RwLock::new(settings),
            plugins: RwLock::new(registry),
        };
        let disabled: Vec<String> = state
            .read_settings()
            .get_as(DISABLED_PLUGINS_KEY)
            .unwrap_or_default();
        let mut registry = state.plugins.write().expect("plugins lock poisoned");
        for id in disabled {
            if let Err(e) = registry.set_enabled(&id, false) {
                tracing::warn!("cannot disable plugin {id}: {e}");
            }
        }
        drop(registry);
        state
    }

    pub fn read_settings(&self) -> std::sync::RwLockReadGuard<'_, Settings> {
        self.settings.read().expect("settings lock poisoned")
    }

    pub fn write_settings(&self) -> std::sync::RwLockWriteGuard<'_, Settings> {
        self.settings.write().expect("settings lock poisoned")
    }

    /// Persist the ids of currently disabled plugins in the user settings.
    pub fn save_disabled_plugins(&self) -> forge_core::Result<()> {
        let disabled: Vec<String> = self
            .plugins
            .read()
            .expect("plugins lock poisoned")
            .list()
            .iter()
            .filter(|p| !p.enabled)
            .map(|p| p.manifest.id.clone())
            .collect();
        self.write_settings().set(
            DISABLED_PLUGINS_KEY,
            serde_json::json!(disabled),
            SettingsScope::User,
        )
    }

    pub fn set_workspace(&self, ws: Workspace) {
        if let Err(e) = self.write_settings().load_project(ws.root()) {
            tracing::warn!("project settings not loaded: {e}");
        }
        *self.workspace.write().expect("workspace lock poisoned") = Some(ws);
    }

    /// Run `f` with the open workspace, or fail if no project is open.
    pub fn with_workspace<T>(
        &self,
        f: impl FnOnce(&Workspace) -> forge_core::Result<T>,
    ) -> forge_core::Result<T> {
        let guard = self.workspace.read().expect("workspace lock poisoned");
        match guard.as_ref() {
            Some(ws) => f(ws),
            None => Err(ForgeError::Other("no project is open".into())),
        }
    }
}
