use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use forge_backend::{Backend, BackendInfo, LocalBackend};
use forge_core::{ForgeError, Settings, SettingsScope, Workspace};
use forge_plugin_api::PluginRegistry;

use crate::plugins::{self, DISABLED_PLUGINS_KEY};

/// Shared application state managed by Tauri.
#[derive(Default)]
pub struct AppState {
    /// The open project's folder when it is on this machine (features that only work
    /// locally use it; remote projects have none).
    workspace: RwLock<Option<Workspace>>,
    /// Where the open project lives: this machine or a remote host.
    backend: RwLock<Option<Arc<dyn Backend>>>,
    pub settings: RwLock<Settings>,
    /// The latest Find window result, so pages are cut from it instead of searching again.
    pub find_results: std::sync::Mutex<Option<(u64, Arc<forge_core::search::FindResult>)>>,
    pub plugins: RwLock<PluginRegistry>,
}

/// Error for features that only work with projects on this machine.
pub fn local_only() -> ForgeError {
    ForgeError::Other("This feature is not available in remote projects yet".into())
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
            backend: RwLock::default(),
            settings: RwLock::new(settings),
            find_results: std::sync::Mutex::default(),
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

    /// Open a local folder as the project.
    pub fn set_workspace(&self, ws: Workspace) {
        let backend: Option<Arc<dyn Backend>> = match LocalBackend::open(ws.root()) {
            Ok(b) => Some(Arc::new(b.with_search_index())),
            Err(e) => {
                tracing::warn!("cannot open {}: {e}", ws.root().display());
                None
            }
        };
        if let Err(e) = self.write_settings().load_project(ws.root()) {
            tracing::warn!("project settings not loaded: {e}");
        }
        *self.workspace.write().expect("workspace lock poisoned") = Some(ws);
        *self.backend.write().expect("backend lock poisoned") = backend;
    }

    /// Open a project on a remote host. Project settings stay at their user values.
    pub fn set_remote_project(&self, backend: Arc<dyn Backend>) {
        *self.workspace.write().expect("workspace lock poisoned") = None;
        *self.backend.write().expect("backend lock poisoned") = Some(backend);
    }

    /// Run `f` with the open workspace, or fail if no project is open or the project is
    /// on a remote host.
    pub fn with_workspace<T>(
        &self,
        f: impl FnOnce(&Workspace) -> forge_core::Result<T>,
    ) -> forge_core::Result<T> {
        let guard = self.workspace.read().expect("workspace lock poisoned");
        match guard.as_ref() {
            Some(ws) => f(ws),
            None if self.is_remote() => Err(local_only()),
            None => Err(ForgeError::Other("no project is open".into())),
        }
    }

    /// The open project's backend.
    pub fn backend(&self) -> forge_core::Result<Arc<dyn Backend>> {
        self.backend
            .read()
            .expect("backend lock poisoned")
            .clone()
            .ok_or_else(|| ForgeError::Other("no project is open".into()))
    }

    /// What the open project is, if one is open.
    pub fn project_info(&self) -> Option<BackendInfo> {
        self.backend
            .read()
            .expect("backend lock poisoned")
            .as_ref()
            .map(|b| b.info().clone())
    }

    /// Absolute project root on the machine that serves it.
    pub fn project_root(&self) -> forge_core::Result<PathBuf> {
        Ok(self.backend()?.info().root.clone())
    }

    pub fn is_remote(&self) -> bool {
        self.project_info().is_some_and(|i| i.remote.is_some())
    }
}
