use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use forge_backend::{Backend, BackendFiles, BackendInfo, LocalBackend};
use forge_core::{ForgeError, Settings, SettingsScope, Workspace};
use forge_plugin_api::PluginRegistry;

use crate::plugins::{self, DISABLED_PLUGINS_KEY};

/// Shared application state managed by Tauri.
#[derive(Default)]
pub struct AppState {
    /// The open project's folder when it is on this machine (remote projects have none;
    /// everything that works on project files goes through `backend`).
    workspace: RwLock<Option<Workspace>>,
    /// Where the open project lives: this machine or a remote host.
    backend: RwLock<Option<Arc<dyn Backend>>>,
    pub settings: RwLock<Settings>,
    /// The latest Find window result, so pages are cut from it instead of searching again.
    /// The files the last Replace in Files changed, to undo it.
    pub last_replace: std::sync::Mutex<Vec<forge_backend::replace::Restore>>,
    pub find_cancel: std::sync::Mutex<Option<Arc<std::sync::atomic::AtomicBool>>>,
    pub find_results: std::sync::Mutex<Option<(u64, Arc<forge_backend::Found>)>>,
    pub plugins: RwLock<PluginRegistry>,
    /// Safe Mode of the open project: on while it is not trusted. Commands that start
    /// project programs check it (`require_trusted`).
    pub safe_mode: forge_trust::SafeMode,
    /// Where trusted projects and locations are stored (`None`: nowhere, e.g. no config dir).
    pub trust_path: Option<PathBuf>,
    /// Installed third-party plugins (`<data dir>/forge/plugins`).
    pub plugin_store: Option<forge_plugins::PluginStore>,
}

/// The IDE language setting (`en`, `de`).
pub const LANGUAGE_KEY: &str = "ide.language";

impl AppState {
    /// State with the built-in plugins registered, their setting defaults applied, and the
    /// user settings (including disabled plugins) loaded.
    pub fn new() -> Self {
        let mut registry = plugins::builtin_registry().unwrap_or_else(|e| {
            tracing::error!("{e}");
            PluginRegistry::new()
        });
        let plugin_store = forge_plugins::PluginStore::default_location();
        let third_party = plugin_store
            .as_ref()
            .map(|s| {
                s.list().unwrap_or_else(|e| {
                    tracing::warn!("installed plugins not loaded: {e}");
                    Vec::new()
                })
            })
            .unwrap_or_default();
        for p in &third_party {
            if let Err(e) = registry.register_descriptor(forge_plugins::store::descriptor(p)) {
                tracing::warn!("plugin {} not loaded: {e}", p.manifest.id);
            }
        }
        let mut settings = Settings::new();
        forge_plugin_api::apply_setting_specs(&mut settings, registry.all_settings());
        if let Some(path) = Settings::default_user_path() {
            if let Err(e) = settings.load_user(path) {
                tracing::warn!("user settings not loaded: {e}");
            }
        }
        if let Some(path) = Settings::default_new_projects_path() {
            if let Err(e) = settings.load_new_projects(path) {
                tracing::warn!("settings for new projects not loaded: {e}");
            }
        }
        let state = Self {
            workspace: RwLock::default(),
            backend: RwLock::default(),
            settings: RwLock::new(settings),
            last_replace: std::sync::Mutex::default(),
            find_cancel: std::sync::Mutex::default(),
            find_results: std::sync::Mutex::default(),
            plugins: RwLock::new(registry),
            safe_mode: forge_trust::SafeMode::default(),
            trust_path: forge_trust::TrustStore::default_path(),
            plugin_store,
        };
        let mut disabled: Vec<String> = state
            .read_settings()
            .get_as(DISABLED_PLUGINS_KEY)
            .unwrap_or_default();
        disabled.extend(
            third_party
                .iter()
                .filter(|p| !p.enabled)
                .map(|p| p.manifest.id.clone()),
        );
        let mut registry = state.plugins.write().expect("plugins lock poisoned");
        for id in disabled {
            if let Err(e) = registry.set_enabled(&id, false) {
                tracing::warn!("cannot disable plugin {id}: {e}");
            }
        }
        drop(registry);
        // The IDE language (Settings | Appearance & Behavior | System Settings | Language)
        // applies to Rust-originated text from the start.
        let language: Option<String> = state.read_settings().get_as(LANGUAGE_KEY);
        forge_i18n::set_locale(language.as_deref().unwrap_or("en"));
        // Flatpak: start host tools through `flatpak-spawn --host` unless turned off.
        crate::commands::flatpak::apply_setting(
            state
                .read_settings()
                .get_bool(forge_process::host::SETTING_KEY),
        );
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
        if let Some(store) = &self.plugin_store {
            let registry = self.plugins.read().expect("plugins lock poisoned");
            for p in registry
                .list()
                .iter()
                .filter(|p| p.manifest.pack == forge_plugins::store::THIRD_PARTY_PACK)
            {
                if let Err(e) = store.set_enabled(&p.manifest.id, p.enabled) {
                    tracing::warn!("{e}");
                }
            }
        }
        self.write_settings().set(
            DISABLED_PLUGINS_KEY,
            serde_json::json!(disabled),
            SettingsScope::User,
        )
    }

    /// How a new project's search index is built (setting `search.indexing.lowPriority`).
    fn index_options(&self) -> forge_backend::IndexOptions {
        forge_backend::IndexOptions {
            low_priority: self
                .read_settings()
                .get_as("search.indexing.lowPriority")
                .unwrap_or(true),
        }
    }

    /// Open a local folder as the project.
    pub fn set_workspace(&self, ws: Workspace) {
        let backend: Option<Arc<dyn Backend>> = match LocalBackend::open(ws.root()) {
            Ok(b) => Some(Arc::new(b.with_search_index(self.index_options()))),
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
        self.refresh_safe_mode();
    }

    /// Open a project on a remote host. Its `.forge/settings.toml` is read and written on the
    /// host. Blocks while the host answers (call off the async runtime).
    pub fn set_remote_project(&self, backend: Arc<dyn Backend>) {
        let files = Arc::new(BackendFiles(backend.clone()));
        {
            let mut settings = self.write_settings();
            if let Err(e) = settings.load_project_files(files) {
                tracing::warn!("project settings not loaded: {e}");
                settings.close_project();
            }
        }
        *self.workspace.write().expect("workspace lock poisoned") = None;
        *self.backend.write().expect("backend lock poisoned") = Some(backend);
        self.refresh_safe_mode();
    }
    /// Open the host's project of a Code With Me session as a guest. Project settings are
    /// read from the host (read-only for the guest). Blocks while the host answers (call off
    /// the async runtime).
    pub fn set_collab_project(&self, backend: Arc<dyn Backend>) {
        self.set_remote_project(backend);
    }

    /// Close the open project (a Code With Me session ended).
    pub fn close_project(&self) {
        self.write_settings().close_project();
        *self.workspace.write().expect("workspace lock poisoned") = None;
        *self.backend.write().expect("backend lock poisoned") = None;
        self.refresh_safe_mode();
    }

    /// The open project's folder when it is on this machine (`None` for remote projects).
    /// Only for things that are about this machine, like which local project a file
    /// opened from outside belongs to.
    pub fn local_workspace(&self) -> Option<Workspace> {
        self.workspace
            .read()
            .expect("workspace lock poisoned")
            .clone()
    }

    /// The open project's backend.
    pub fn backend(&self) -> forge_core::Result<Arc<dyn Backend>> {
        self.backend
            .read()
            .expect("backend lock poisoned")
            .clone()
            .ok_or_else(ForgeError::no_project)
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

    /// The project's files are not on this machine: an SSH host, or the host of a Code With
    /// Me session.
    pub fn is_remote(&self) -> bool {
        self.project_info().is_some_and(|i| !i.is_local())
    }

    /// The trusted projects and locations (empty when the store cannot be read).
    pub fn trust_store(&self) -> forge_trust::TrustStore {
        let Some(path) = &self.trust_path else {
            return Default::default();
        };
        forge_trust::TrustStore::load(path).unwrap_or_else(|e| {
            tracing::warn!("trusted projects not loaded: {e}");
            Default::default()
        })
    }

    pub fn save_trust_store(&self, store: &forge_trust::TrustStore) -> forge_core::Result<()> {
        match &self.trust_path {
            Some(path) => store.save(path),
            None => Err(ForgeError::Other(
                "no config folder to store trusted projects in".into(),
            )),
        }
    }

    /// The open project's trust key (`None` without a project).
    pub fn project_trust_key(&self) -> Option<String> {
        self.project_info().map(|info| trust_key(&info))
    }

    /// Turn Safe Mode on when the open project is not trusted, off otherwise. Returns
    /// whether it is on.
    ///
    /// A Code With Me guest is always in Safe Mode: the host's project is not this
    /// machine's to trust, and nothing of it runs here (the host runs what the guest may run).
    pub fn refresh_safe_mode(&self) -> bool {
        let on = self
            .project_info()
            .is_some_and(|info| needs_safe_mode(&info, &self.trust_store()));
        self.safe_mode.set(on);
        on
    }

    /// `Err` when the open project is in Safe Mode; `what` names the refused action.
    pub fn require_trusted(&self, what: &str) -> forge_core::Result<()> {
        self.safe_mode.check(what)
    }
}

/// Whether a project opens in Safe Mode: it is not trusted, or it is the host's project of
/// a Code With Me session (always).
pub fn needs_safe_mode(info: &BackendInfo, store: &forge_trust::TrustStore) -> bool {
    info.collab.is_some() || !store.is_trusted(&trust_key(info))
}

/// A project's trust key: the canonical local path, or `ssh://user@host:port/path`. A Code
/// With Me guest's project is `cwm://host/name`: its root is a placeholder (`/<name>`) that
/// must never stand for a local folder of the same name.
pub fn trust_key(info: &BackendInfo) -> String {
    if info.collab.is_some() {
        return info.display_root();
    }
    let root = info.root.to_string_lossy();
    if let Some(key) = info
        .remote
        .as_ref()
        .and_then(|r| crate::commands::devenv::trust_key(r, &root))
    {
        return key;
    }
    match &info.remote {
        Some(r) => forge_trust::remote_key(&r.user, &r.host, r.port, &info.root.to_string_lossy()),
        None => forge_trust::local_key(&info.root),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_backend::CollabHost;

    fn guest_info(name: &str) -> BackendInfo {
        BackendInfo {
            name: name.into(),
            root: PathBuf::from(format!("/{name}")),
            remote: None,
            collab: Some(CollabHost {
                name: "Robert".into(),
                label: "Robert (workstation)".into(),
            }),
        }
    }

    #[test]
    fn a_guest_project_never_aliases_a_local_folder() {
        assert_eq!(trust_key(&guest_info("shop")), "cwm://Robert/shop");
        let mut store = forge_trust::TrustStore::default();
        // A trusted local folder named like the placeholder root.
        store.trust("/shop").unwrap();
        assert!(needs_safe_mode(&guest_info("shop"), &store));
    }

    #[test]
    fn local_projects_follow_the_store() {
        let local = BackendInfo {
            name: "shop".into(),
            root: PathBuf::from("/opt/shop"),
            remote: None,
            collab: None,
        };
        let mut store = forge_trust::TrustStore::default();
        assert!(needs_safe_mode(&local, &store));
        store.trust(&trust_key(&local)).unwrap();
        assert!(!needs_safe_mode(&local, &store));
    }

    #[test]
    fn closing_the_project_ends_safe_mode() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState {
            trust_path: Some(dir.path().join("trusted-projects.toml")),
            ..AppState::default()
        };
        state.set_workspace(forge_core::Workspace::open(dir.path()).unwrap());
        assert!(state.safe_mode.is_on());
        state.close_project();
        assert!(!state.safe_mode.is_on());
    }
}
