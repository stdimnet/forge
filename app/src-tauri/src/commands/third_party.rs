//! Third-party plugins (`forge_plugins`): Install Plugin from Disk, uninstall, the plugin
//! host's permission-checked backend calls, and the Marketplace tab.
//!
//! Plugins live in the user data directory. Their UI bundles run sandboxed in the webview;
//! everything they do outside the sandbox comes through the `plugin_host_*` commands, which
//! check the permissions the user granted on install.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use forge_core::{ForgeError, Result, SettingsScope};
use forge_plugins::host::{self, FetchRequest, FetchResponse, RunOutput};
use forge_plugins::marketplace::{self, Entry, Index, InstalledVersion};
use forge_plugins::permissions::{self, Guard, PermissionLine, Roots};
use forge_plugins::{
    InstalledPlugin, Package, PluginManifest, PluginRef, PluginStore, Source, FORGE_VERSION,
};

use crate::commands::workspace::blocking;
use crate::state::AppState;

const MARKETPLACE_URL_KEY: &str = "plugins.marketplace.url";
const TRUSTED_KEYS_KEY: &str = "plugins.marketplace.trustedKeys";
/// Emitted after a plugin was installed, updated, uninstalled, enabled or disabled; every
/// window reloads its plugins.
pub const PLUGINS_CHANGED_EVENT: &str = "plugins://changed";

pub fn emit_changed(app: &AppHandle) {
    if let Err(e) = app.emit(PLUGINS_CHANGED_EVENT, ()) {
        tracing::warn!("{e}");
    }
}

fn err(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

fn store(state: &AppState) -> Result<&PluginStore> {
    state
        .plugin_store
        .as_ref()
        .ok_or_else(|| err("no user data directory for plugins"))
}

/// An installed plugin with its permission prompt lines.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledView {
    #[serde(flatten)]
    pub plugin: InstalledPlugin,
    pub permission_lines: Vec<PermissionLine>,
    /// Refused in Safe Mode: it may start programs or write project files.
    pub needs_trust: bool,
}

fn needs_trust(p: &Permissions) -> bool {
    !p.process.is_empty()
        || p.fs.write.iter().any(|s| {
            permissions::FsScope::parse(s).is_some_and(|f| f.root == permissions::FsRoot::Project)
        })
}

use forge_plugins::manifest::Permissions;

fn view(plugin: InstalledPlugin) -> InstalledView {
    InstalledView {
        permission_lines: permissions::summary(&plugin.granted),
        needs_trust: needs_trust(&plugin.granted),
        plugin,
    }
}

#[tauri::command]
pub fn plugins_installed(state: State<'_, AppState>) -> Result<Vec<InstalledView>> {
    Ok(store(&state)?
        .list()
        .map_err(err)?
        .into_iter()
        .map(view)
        .collect())
}

/// What the install prompt shows for a package file.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub manifest: PluginManifest,
    pub sha256: String,
    pub permission_lines: Vec<PermissionLine>,
    /// Permissions an update adds to what was granted before.
    pub new_permission_lines: Vec<PermissionLine>,
    pub installed_version: Option<String>,
}

fn inspect(state: &AppState, package: &Package) -> Result<Inspection> {
    let installed = store(state)?.get(&package.manifest.id).map_err(err)?;
    if installed.is_none()
        && state
            .plugins
            .read()
            .expect("plugins lock poisoned")
            .get(&package.manifest.id)
            .is_some()
    {
        return Err(err(format!(
            "`{}` is the id of a built-in plugin",
            package.manifest.id
        )));
    }
    let new = installed
        .as_ref()
        .map(|p| permissions::added(&p.granted, &package.manifest.permissions))
        .unwrap_or_else(|| package.manifest.permissions.clone());
    Ok(Inspection {
        manifest: package.manifest.clone(),
        sha256: package.sha256.clone(),
        permission_lines: permissions::summary(&package.manifest.permissions),
        new_permission_lines: permissions::summary(&new),
        installed_version: installed.map(|p| p.manifest.version),
    })
}

fn read_package(path: &str) -> Result<Package> {
    let bytes = std::fs::read(path).map_err(|e| ForgeError::io(path, e))?;
    Package::read(&bytes, FORGE_VERSION).map_err(err)
}

/// Install Plugin from Disk, step 1: read and validate a `.forgeplugin` file for the
/// permission prompt.
#[tauri::command]
pub async fn plugin_inspect(path: String, state: State<'_, AppState>) -> Result<Inspection> {
    let package = blocking(move || read_package(&path)).await?;
    inspect(&state, &package)
}

/// Put an installed plugin into the registry (replacing an older version) with its setting
/// defaults.
fn register(state: &AppState, plugin: &InstalledPlugin) -> Result<()> {
    let mut registry = state.plugins.write().expect("plugins lock poisoned");
    registry.unregister(&plugin.manifest.id);
    registry.register_descriptor(forge_plugins::store::descriptor(plugin))?;
    if !plugin.enabled {
        registry.set_enabled(&plugin.manifest.id, false)?;
    }
    drop(registry);
    forge_plugin_api::apply_setting_specs(
        &mut state.write_settings(),
        &plugin.manifest.contributes.settings,
    );
    Ok(())
}

/// Install Plugin from Disk, step 2, after the user accepted the permissions of the file
/// with hash `sha256` (a file changed since the prompt is refused).
#[tauri::command]
pub async fn plugin_install_file(
    path: String,
    sha256: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<InstalledView> {
    let file = path.clone();
    let package = blocking(move || read_package(&file)).await?;
    if package.sha256 != sha256 {
        return Err(err(
            "the file changed after it was checked; install it again",
        ));
    }
    inspect(&state, &package)?;
    let installed = store(&state)?
        .install(&package, Source::Disk { file: path })
        .map_err(err)?;
    register(&state, &installed)?;
    emit_changed(&app);
    Ok(view(installed))
}

#[tauri::command]
pub fn plugin_uninstall(id: String, state: State<'_, AppState>, app: AppHandle) -> Result<()> {
    store(&state)?.uninstall(&id).map_err(err)?;
    state
        .plugins
        .write()
        .expect("plugins lock poisoned")
        .unregister(&id);
    state.save_disabled_plugins()?;
    emit_changed(&app);
    Ok(())
}

fn enabled_plugin(state: &AppState, id: &str) -> Result<InstalledPlugin> {
    let plugin = store(state)?
        .get(id)
        .map_err(err)?
        .ok_or_else(|| err(format!("plugin `{id}` is not installed")))?;
    let enabled = state
        .plugins
        .read()
        .expect("plugins lock poisoned")
        .is_enabled(id);
    if !enabled {
        return Err(err(format!("plugin `{id}` is disabled")));
    }
    Ok(plugin)
}

/// The UI bundle of an enabled plugin. Plugins that may start programs or write project
/// files are not loaded in Safe Mode.
#[tauri::command]
pub fn plugin_bundle(id: String, state: State<'_, AppState>) -> Result<String> {
    let plugin = enabled_plugin(&state, &id)?;
    if needs_trust(&plugin.granted) {
        state.require_trusted(&format!("load the plugin {}", plugin.manifest.name))?;
    }
    let main = plugin
        .manifest
        .main
        .as_deref()
        .ok_or_else(|| err(format!("plugin `{id}` has no UI bundle")))?;
    let bytes = store(&state)?.read_file(&id, main).map_err(err)?;
    String::from_utf8(bytes).map_err(|_| err(format!("{main} is not UTF-8")))
}

/// An icon theme of an enabled plugin with its SVGs (`forge_plugins::icon_themes`). Themes
/// are data only, so they load in Safe Mode too.
#[tauri::command]
pub fn icon_theme_load(
    id: String,
    state: State<'_, AppState>,
) -> Result<forge_plugins::icon_themes::LoadedIconTheme> {
    let store = store(&state)?;
    let owner = store
        .list()
        .map_err(err)?
        .into_iter()
        .find(|p| {
            p.manifest
                .contributes
                .icon_themes
                .iter()
                .any(|t| t.id == id)
        })
        .ok_or_else(|| {
            err(format!(
                "no installed plugin contributes the icon theme `{id}`"
            ))
        })?;
    let plugin = enabled_plugin(&state, &owner.manifest.id)?;
    forge_plugins::icon_themes::load(&plugin.manifest, &id, |path| {
        store.read_file(&plugin.manifest.id, path)
    })
    .map_err(err)
}

fn roots(state: &AppState, id: &str) -> Result<Roots> {
    Ok(Roots {
        project: state.local_workspace().map(|w| w.root().to_path_buf()),
        plugin_data: store(state)?.data_dir(id),
    })
}

/// `forge.fs.readFile`: a text file in a granted scope.
#[tauri::command]
pub async fn plugin_host_read_file(
    plugin_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<String> {
    let plugin = enabled_plugin(&state, &plugin_id)?;
    let roots = roots(&state, &plugin_id)?;
    let real = Guard {
        plugin_id: &plugin_id,
        granted: &plugin.granted,
    }
    .check_fs(Path::new(&path), false, &roots)
    .map_err(err)?;
    blocking(move || {
        let bytes = std::fs::read(&real).map_err(|e| ForgeError::io(&real, e))?;
        if bytes.len() > host::MAX_BODY {
            return Err(err("the file is larger than 16 MB"));
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    })
    .await
}

/// `forge.fs.writeFile`: writing project files is refused in Safe Mode.
#[tauri::command]
pub async fn plugin_host_write_file(
    plugin_id: String,
    path: String,
    contents: String,
    state: State<'_, AppState>,
) -> Result<()> {
    let plugin = enabled_plugin(&state, &plugin_id)?;
    let roots = roots(&state, &plugin_id)?;
    let real = Guard {
        plugin_id: &plugin_id,
        granted: &plugin.granted,
    }
    .check_fs(Path::new(&path), true, &roots)
    .map_err(err)?;
    if roots.project.as_ref().is_some_and(|p| real.starts_with(p)) {
        state.require_trusted(&format!("let {} write project files", plugin.manifest.name))?;
    }
    blocking(move || {
        if let Some(parent) = real.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ForgeError::io(parent, e))?;
        }
        std::fs::write(&real, contents).map_err(|e| ForgeError::io(&real, e))
    })
    .await
}

/// `forge.http.fetch`: requests to granted hosts only.
#[tauri::command]
pub async fn plugin_host_fetch(
    plugin_id: String,
    request: FetchRequest,
    state: State<'_, AppState>,
) -> Result<FetchResponse> {
    let plugin = enabled_plugin(&state, &plugin_id)?;
    host::fetch(&plugin_id, &plugin.granted, request, false)
        .await
        .map_err(err)
}

/// `forge.process.run`: granted programs, in the project folder, never in Safe Mode.
#[tauri::command]
pub async fn plugin_host_run(
    plugin_id: String,
    program: String,
    args: Vec<String>,
    state: State<'_, AppState>,
) -> Result<RunOutput> {
    let plugin = enabled_plugin(&state, &plugin_id)?;
    state.require_trusted(&format!("let {} start {program}", plugin.manifest.name))?;
    let cwd: Option<PathBuf> = state.local_workspace().map(|w| w.root().to_path_buf());
    blocking(move || {
        host::run(&plugin_id, &plugin.granted, &program, &args, cwd.as_deref()).map_err(err)
    })
    .await
}

fn setting_guard(state: &AppState, plugin_id: &str, key: &str) -> Result<()> {
    let plugin = enabled_plugin(state, plugin_id)?;
    Guard {
        plugin_id,
        granted: &plugin.granted,
    }
    .check_setting(key)
    .map_err(err)
}

/// `forge.settings.get`: the plugin's own settings, or granted keys.
#[tauri::command]
pub fn plugin_host_setting_get(
    plugin_id: String,
    key: String,
    state: State<'_, AppState>,
) -> Result<Option<Value>> {
    setting_guard(&state, &plugin_id, &key)?;
    Ok(state.read_settings().get(&key).cloned())
}

/// `forge.settings.set`: user settings only.
#[tauri::command]
pub fn plugin_host_setting_set(
    plugin_id: String,
    key: String,
    value: Value,
    state: State<'_, AppState>,
) -> Result<()> {
    setting_guard(&state, &plugin_id, &key)?;
    let spec = state
        .plugins
        .read()
        .expect("plugins lock poisoned")
        .all_settings()
        .find(|s| s.key == key)
        .cloned();
    if let (Some(spec), false) = (spec, value.is_null()) {
        spec.kind.validate(&value).map_err(err)?;
    }
    state.write_settings().set(&key, value, SettingsScope::User)
}

// ---- Marketplace

/// The last index read, so Install installs what the tab showed.
static INDEX: Mutex<Option<(String, Index)>> = Mutex::new(None);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceView {
    pub url: String,
    pub name: String,
    pub entries: Vec<Entry>,
    pub warnings: Vec<String>,
    /// Plugins imported settings asked for that are not installed.
    pub wanted: Vec<PluginRef>,
}

fn marketplace_url(state: &AppState) -> String {
    state
        .read_settings()
        .get_as::<String>(MARKETPLACE_URL_KEY)
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| marketplace::DEFAULT_URL.to_string())
}

/// A client that sends the access token stored for `url` (only to the index's host).
fn marketplace_client(url: &str) -> Result<marketplace::Client> {
    Ok(marketplace::Client::new()
        .map_err(err)?
        .with_token(forge_plugins::secrets::marketplace_token(url)))
}

fn installed_versions(state: &AppState) -> Result<Vec<InstalledVersion>> {
    Ok(store(state)?
        .list()
        .map_err(err)?
        .into_iter()
        .map(|p| InstalledVersion {
            id: p.manifest.id,
            version: p.manifest.version,
        })
        .collect())
}

/// The Marketplace tab: the index at `plugins.marketplace.url`, filtered by `query`.
#[tauri::command]
pub async fn marketplace_list(
    query: String,
    refresh: bool,
    state: State<'_, AppState>,
) -> Result<MarketplaceView> {
    let url = marketplace_url(&state);
    let wanted = store(&state)?.wanted().map_err(err)?;
    let cached = INDEX
        .lock()
        .expect("index lock poisoned")
        .clone()
        .filter(|(u, _)| *u == url && !refresh);
    let (index, warnings) = match cached {
        Some((_, index)) => (index, Vec::new()),
        None => {
            let client = marketplace_client(&url)?;
            let (index, warnings) = client.index(&url).await.map_err(err)?;
            *INDEX.lock().expect("index lock poisoned") = Some((url.clone(), index.clone()));
            (index, warnings)
        }
    };
    let entries = marketplace::entries(&index, FORGE_VERSION, &installed_versions(&state)?);
    Ok(MarketplaceView {
        url,
        name: index.name.clone(),
        entries: marketplace::search(&entries, &query),
        warnings,
        wanted,
    })
}

/// Download, verify (SHA-256, trusted signatures) and install a plugin from the
/// marketplace, after the user accepted the permissions the listing showed.
#[tauri::command]
pub async fn marketplace_install(
    id: String,
    version: Option<String>,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<InstalledView> {
    let url = marketplace_url(&state);
    let (index_url, index) = INDEX
        .lock()
        .expect("index lock poisoned")
        .clone()
        .filter(|(u, _)| *u == url)
        .ok_or_else(|| err("open the Marketplace tab first"))?;
    let listed = marketplace::find_version(&index, &id, version.as_deref(), FORGE_VERSION)
        .map_err(err)?
        .clone();
    let keys: Vec<String> = state
        .read_settings()
        .get_as(TRUSTED_KEYS_KEY)
        .unwrap_or_default();
    let client = marketplace_client(&index_url)?;
    let bytes = client
        .download(&index_url, &listed, &keys)
        .await
        .map_err(err)?;
    let package = Package::read(&bytes, FORGE_VERSION).map_err(err)?;
    forge_plugins::check_listing(&package, &id, &listed).map_err(err)?;
    inspect(&state, &package)?;
    let installed = store(&state)?
        .install(&package, Source::Marketplace { index: index_url })
        .map_err(err)?;
    register(&state, &installed)?;
    emit_changed(&app);
    Ok(view(installed))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketplaceTokenStatus {
    /// The index URL the token belongs to.
    pub url: String,
    /// Where the token is sent (the index's host; `github.com` for GitHub's hosts).
    pub scope: String,
    /// Whether the keychain holds a token for it. The token itself never leaves Rust.
    pub set: bool,
}

/// The access token state of the configured marketplace.
#[tauri::command]
pub fn marketplace_token_status(state: State<'_, AppState>) -> Result<MarketplaceTokenStatus> {
    let url = marketplace_url(&state);
    Ok(MarketplaceTokenStatus {
        scope: marketplace::token_scope(&url).unwrap_or_default(),
        set: forge_plugins::secrets::marketplace_token(&url).is_some(),
        url,
    })
}

/// Store the configured marketplace's access token in the OS keychain (`null` or empty
/// removes it). The next listing reads the index again with it.
#[tauri::command]
pub fn marketplace_token_set(token: Option<String>, state: State<'_, AppState>) -> Result<()> {
    let url = marketplace_url(&state);
    forge_plugins::secrets::set_marketplace_token(&url, token.as_deref()).map_err(err)?;
    *INDEX.lock().expect("index lock poisoned") = None;
    Ok(())
}

/// Installed plugins with a newer compatible version in the marketplace.
#[tauri::command]
pub async fn marketplace_updates(state: State<'_, AppState>) -> Result<Vec<Entry>> {
    let view = marketplace_list(String::new(), true, state).await?;
    Ok(view
        .entries
        .into_iter()
        .filter(|e| e.update_available)
        .collect())
}

/// Forget the plugins imported settings asked for.
#[tauri::command]
pub fn plugins_wanted_dismiss(state: State<'_, AppState>) -> Result<()> {
    store(&state)?.dismiss_wanted().map_err(err)
}
