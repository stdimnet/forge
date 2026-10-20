//! Language servers for many languages (catalog in `forge-langservers`): activation for
//! the open project, the Language Servers settings page, installs with streamed output,
//! project-wide symbol search and the Intelephense licence key.
//!
//! npm servers (Intelephense, typescript-language-server, …) are installed by Forge itself
//! (`forge_langservers::managed`): Node.js and the packages are downloaded and verified
//! into Forge's data directory, so nothing has to be installed first.
//!
//! Events:
//! - `langservers://install-output` `{ id, stream, line }`
//! - `langservers://install-progress` `{ id, label, done, total }` (downloads, for the
//!   status bar)
//! - `langservers://changed` (rows changed after an apply or an install)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use forge_core::{ForgeError, Result};
use forge_langservers::install::{self, InstallOutcome, Stream};
use forge_langservers::licence::{self, KeyringStore, Licence, LicenceStatus};
use forge_langservers::managed::{self, Event, Installer, PackageUpdate, Sources};
use forge_langservers::plan::current_platform;
use forge_langservers::{
    plan, rows, scan_project, Catalog, ConfigContext, Detector, LiveState, Prefs, ProjectScan,
    ServerRow,
};
use forge_lsp::model::EditorWorkspaceSymbol;
use forge_lsp::{LspManager, LspServerConfig, ServerState};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::lsp::LspState;
use crate::state::AppState;

/// Running installs (cancel handles by server id).
#[derive(Default)]
pub struct LangServersState {
    installs: Mutex<HashMap<String, Arc<tokio::sync::Notify>>>,
}

/// Last project scan; scanning is cheap but not free.
static SCAN: Mutex<Option<(PathBuf, ProjectScan)>> = Mutex::new(None);

fn other(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("forge")
}

fn load_prefs(ws: &AppState) -> Prefs {
    let settings = ws.read_settings();
    let ids = Catalog::builtin().servers.iter().map(|s| s.id.as_str());
    Prefs::load(|k| settings.get(k).cloned(), ids)
}

fn project_scan(root: &Path, limit: usize, refresh: bool) -> ProjectScan {
    let mut cache = SCAN.lock().expect("scan lock");
    if !refresh {
        if let Some((r, scan)) = cache.as_ref() {
            if r == root {
                return scan.clone();
            }
        }
    }
    let scan = scan_project(root, Catalog::builtin(), limit);
    *cache = Some((root.to_path_buf(), scan.clone()));
    scan
}

fn plugin_enabled(ws: &AppState) -> impl Fn(&str) -> bool + '_ {
    move |id: &str| {
        let plugins = ws.plugins.read().expect("plugins lock poisoned");
        // Unknown plugins (none registered yet) do not block a server.
        plugins.get(id).is_none_or(|p| p.enabled)
    }
}

/// Register the enabled and installed catalog servers for the languages of the manager's
/// project and unregister the ones no longer wanted. Called when the LSP manager of a
/// project is created and after settings change. Also applies the Intelephense licence
/// key to a registered Intelephense config.
pub fn register_enabled(manager: &LspManager, ws: &AppState) {
    apply(manager, ws, true);
}

fn apply(manager: &LspManager, ws: &AppState, refresh_scan: bool) {
    let prefs = load_prefs(ws);
    let scan = project_scan(manager.root(), prefs.scan_limit, refresh_scan);
    let detector = Detector::for_project(manager.root());
    let ctx = ConfigContext {
        root: manager.root(),
        cache_dir: cache_dir(),
        detector: &detector,
        prefs: &prefs,
        scan: &scan,
    };
    let decisions = plan(Catalog::builtin(), &ctx, plugin_enabled(ws));
    let registered: HashMap<String, LspServerConfig> = manager
        .configs()
        .into_iter()
        .map(|c| (c.id.clone(), c))
        .collect();
    for d in decisions {
        match d.outcome {
            Ok(config) => {
                if registered.get(&config.id) != Some(&config) {
                    tracing::info!("language server {} registered", config.id);
                    manager.register_server(config);
                }
            }
            Err(_) if registered.contains_key(&d.id) => {
                manager.unregister_server(&d.id);
            }
            Err(_) => {}
        }
    }
    apply_intelephense_licence(manager);
}

/// Put the keychain licence key into a registered Intelephense config (or remove it),
/// restarting Intelephense when that changes anything.
fn apply_intelephense_licence(manager: &LspManager) {
    let Some(mut config) = manager
        .configs()
        .into_iter()
        // The PHP pack registers Intelephense under the generic PHP server id.
        .find(|c| {
            c.id == licence::INTELEPHENSE_ACCOUNT
                || (c.id == crate::commands::php::PHP_SERVER_ID
                    && c.command.contains("intelephense"))
        })
    else {
        return;
    };
    let before = config.initialization_options.clone();
    with_intelephense_licence(&mut config);
    if config.initialization_options != before {
        manager.register_server(config);
    }
}

/// Add the Intelephense licence key from the keychain to `config`'s initialization options,
/// or make sure none is sent (free mode). For pack code that builds Intelephense configs.
pub fn with_intelephense_licence(config: &mut LspServerConfig) {
    let key = Licence::new(&KeyringStore).key().unwrap_or_else(|e| {
        tracing::warn!("cannot read the Intelephense licence key: {e}");
        None
    });
    licence::apply_licence(&mut config.initialization_options, key.as_deref());
}

/// The licence key for PHP's own config builder (`lang-php`'s `PhpLspSettings`).
pub fn intelephense_licence_key() -> Option<String> {
    Licence::new(&KeyringStore).key().ok().flatten()
}

fn live_states(manager: Option<&LspManager>) -> HashMap<String, LiveState> {
    let Some(manager) = manager else {
        return HashMap::new();
    };
    manager
        .statuses()
        .into_iter()
        .map(|s| {
            let (running, detail) = match s.state {
                ServerState::Ready => (true, None),
                ServerState::Starting => (true, Some("Starting…".to_string())),
                ServerState::Busy {
                    message,
                    percentage,
                } => (
                    true,
                    Some(match percentage {
                        Some(p) => format!("{message} {p}%"),
                        None => message,
                    }),
                ),
                ServerState::Failed { error } => (false, Some(error)),
                ServerState::NotStarted | ServerState::Stopped => (false, None),
            };
            (s.id, LiveState { running, detail })
        })
        .collect()
}

fn build_rows(manager: Option<&LspManager>, ws: &AppState) -> Vec<ServerRow> {
    let prefs = load_prefs(ws);
    let root = manager
        .map(|m| m.root().to_path_buf())
        .or_else(|| ws.with_workspace(|w| Ok(w.root().to_path_buf())).ok());
    let scan = root
        .as_deref()
        .map(|r| project_scan(r, prefs.scan_limit, false))
        .unwrap_or_default();
    let detector = root
        .as_deref()
        .map_or_else(Detector::system, Detector::for_project);
    let fallback_root = PathBuf::new();
    let ctx = ConfigContext {
        root: root.as_deref().unwrap_or(&fallback_root),
        cache_dir: cache_dir(),
        detector: &detector,
        prefs: &prefs,
        scan: &scan,
    };
    let live = live_states(manager);
    let platform = current_platform();
    let decisions = if root.is_some() {
        plan(Catalog::builtin(), &ctx, plugin_enabled(ws))
    } else {
        vec![]
    };
    let mut out = rows(Catalog::builtin(), &ctx, &decisions, &live, platform);
    // Servers the PHP pack starts itself: listed for status and install hints only.
    let packs = rows(Catalog::pack_servers(), &ctx, &[], &live, platform);
    out.extend(packs.into_iter().map(|mut r| {
        r.source = "php".into();
        r.enabled = true;
        r.status = match r.status {
            forge_langservers::RowStatus::Disabled => forge_langservers::RowStatus::Installed,
            s => s,
        };
        r
    }));
    out
}

/// Rows of Settings > Languages & Frameworks > Language Servers.
#[tauri::command]
pub fn langservers_list(
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Vec<ServerRow> {
    let manager = lsp.manager(&app, &ws).ok();
    build_rows(manager.as_deref(), &ws)
}

/// Re-plan after settings changed: start newly enabled servers, stop disabled ones.
#[tauri::command]
pub fn langservers_apply(
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Vec<ServerRow> {
    let manager = lsp.manager(&app, &ws).ok();
    if let Some(m) = manager.as_deref() {
        apply(m, &ws, true);
    }
    let rows = build_rows(manager.as_deref(), &ws);
    let _ = app.emit("langservers://changed", ());
    rows
}

#[derive(Serialize, Clone)]
struct InstallLine {
    id: String,
    stream: Stream,
    line: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct InstallProgress {
    id: String,
    label: String,
    done: u64,
    total: Option<u64>,
}

/// Download sources from the settings (`langservers.download.*`).
fn sources(ws: &AppState) -> Sources {
    let settings = ws.read_settings();
    let get = |k: &str| settings.get(k).and_then(Value::as_str).map(str::to_owned);
    Sources::new(
        get("langservers.download.npmRegistry").as_deref(),
        get("langservers.download.nodeMirror").as_deref(),
    )
}

/// Install npm `packages` with Forge's managed installer, streaming its steps as output
/// lines of `id` and its downloads as progress events.
async fn managed_install(
    app: &AppHandle,
    ws: &AppState,
    id: &str,
    packages: &[String],
    cancel: Arc<tokio::sync::Notify>,
) -> std::result::Result<InstallOutcome, String> {
    let installer = Installer::system(sources(ws))?;
    let mut last_step: Option<(String, u64)> = None;
    let mut on = |e: Event| match e {
        Event::Step { message } => emit_line(app, id, Stream::Info, message),
        Event::Output { stream, line } => emit_line(app, id, stream, line),
        Event::Progress { label, done, total } => {
            // One event per percent (or per MB without a length) is plenty for a progress bar.
            let step = total.map_or(done >> 20, |t| done * 100 / t.max(1));
            if last_step.as_ref() == Some(&(label.clone(), step)) {
                return;
            }
            last_step = Some((label.clone(), step));
            let _ = app.emit(
                "langservers://install-progress",
                InstallProgress {
                    id: id.to_string(),
                    label,
                    done,
                    total,
                },
            );
        }
    };
    tokio::select! {
        r = installer.install(packages, Arc::new(tokio::sync::Notify::new()), &mut on) => r,
        _ = cancel.notified() => Ok(InstallOutcome::Cancelled),
    }
}

fn emit_line(app: &AppHandle, id: &str, stream: Stream, line: String) {
    let _ = app.emit(
        "langservers://install-output",
        InstallLine {
            id: id.to_string(),
            stream,
            line,
        },
    );
}

/// Register what was just installed: catalog servers and the PHP pack's Intelephense.
async fn register_installed(app: &AppHandle) {
    let handle = app.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        let lsp = handle.state::<LspState>();
        let ws = handle.state::<AppState>();
        if let Ok(manager) = lsp.manager(&handle, &ws) {
            apply(&manager, &ws, false);
        }
        crate::commands::php::sync_lsp_server(&handle, None);
    })
    .await;
}

/// Run install option `index` of server `id` (catalog commands only, after the user
/// confirmed it in the UI). Output streams as `langservers://install-output`.
#[tauri::command]
pub async fn langservers_install(
    id: String,
    index: usize,
    app: AppHandle,
    state: State<'_, LangServersState>,
    ws: State<'_, AppState>,
) -> Result<InstallOutcome> {
    let spec = Catalog::builtin()
        .server(&id)
        .or_else(|| Catalog::pack_servers().server(&id))
        .ok_or_else(|| other(format!("unknown language server {id}")))?;
    let option = spec
        .install
        .get(index)
        .ok_or_else(|| other("unknown install option"))?;
    let detector = Detector::system();
    // npm packages: Forge's own installer (no Node.js or npm needed on the machine).
    let packages = detector
        .managed_dir()
        .and_then(|_| managed::npm_packages(option));
    let resolved = match packages {
        Some(_) => None,
        None => Some(install::resolve_command(option, &detector).map_err(other)?),
    };
    let cancel = Arc::new(tokio::sync::Notify::new());
    {
        let mut installs = state.installs.lock().expect("installs lock");
        if installs.contains_key(&id) {
            return Err(other(format!("{} is already being installed", spec.name)));
        }
        installs.insert(id.clone(), cancel.clone());
    }
    let Some((program, args)) = resolved else {
        let packages = packages.unwrap_or_default();
        let outcome = managed_install(&app, &ws, &id, &packages, cancel).await;
        state.installs.lock().expect("installs lock").remove(&id);
        let outcome = match outcome {
            Ok(o) => o,
            Err(e) => {
                emit_line(&app, &id, Stream::Info, e.clone());
                let _ = app.emit("langservers://changed", ());
                return Err(other(e));
            }
        };
        emit_line(&app, &id, Stream::Info, outcome_line(&outcome, &spec.name));
        if outcome == InstallOutcome::Succeeded {
            register_installed(&app).await;
        }
        let _ = app.emit("langservers://changed", ());
        return Ok(outcome);
    };
    let emit = |stream: Stream, line: String| {
        let _ = app.emit(
            "langservers://install-output",
            InstallLine {
                id: id.clone(),
                stream,
                line,
            },
        );
    };
    emit(
        Stream::Info,
        format!("$ {}", install::display_command(option, &detector)),
    );
    let outcome = install::run(program, args, detector.dirs().to_vec(), cancel, &emit).await;
    emit(Stream::Info, outcome_line(&outcome, &spec.name));
    state.installs.lock().expect("installs lock").remove(&id);
    let _ = app.emit("langservers://changed", ());
    Ok(outcome)
}

fn outcome_line(outcome: &InstallOutcome, name: &str) -> String {
    match outcome {
        InstallOutcome::Succeeded => format!("{name} installed."),
        InstallOutcome::Failed { code: Some(c) } => format!("Failed with exit code {c}."),
        InstallOutcome::Failed { code: None } => "Failed.".into(),
        InstallOutcome::Cancelled => "Cancelled.".into(),
    }
}

/// Newer versions of the npm servers Forge installed. Asks the registry at most once a
/// day unless `force`; `None` when it did not ask (or checks are off in the settings).
#[tauri::command]
pub async fn langservers_check_updates(
    force: Option<bool>,
    ws: State<'_, AppState>,
) -> Result<Option<Vec<PackageUpdate>>> {
    let force = force.unwrap_or(false);
    let enabled = ws
        .read_settings()
        .get("langservers.updates.check")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if !enabled && !force {
        return Ok(None);
    }
    let installer = Installer::system(sources(&ws)).map_err(other)?;
    Ok(installer
        .check_updates(force, managed::updates::now_secs())
        .await)
}

/// Update one package Forge installed to its latest version, then restart the servers so
/// they run the new files. Output streams under the package name.
#[tauri::command]
pub async fn langservers_update(
    package: String,
    app: AppHandle,
    state: State<'_, LangServersState>,
    ws: State<'_, AppState>,
) -> Result<InstallOutcome> {
    let cancel = Arc::new(tokio::sync::Notify::new());
    {
        let mut installs = state.installs.lock().expect("installs lock");
        if installs.contains_key(&package) {
            return Err(other(format!("{package} is already being updated")));
        }
        installs.insert(package.clone(), cancel.clone());
    }
    let outcome =
        managed_install(&app, &ws, &package, std::slice::from_ref(&package), cancel).await;
    state
        .installs
        .lock()
        .expect("installs lock")
        .remove(&package);
    let outcome = outcome.map_err(other)?;
    emit_line(
        &app,
        &package,
        Stream::Info,
        outcome_line(&outcome, &package),
    );
    if outcome == InstallOutcome::Succeeded {
        if let Ok(manager) = app.state::<LspState>().manager(&app, &ws) {
            // Registering again stops a running server; it restarts on next use.
            for config in manager.configs() {
                manager.register_server(config);
            }
        }
        register_installed(&app).await;
    }
    let _ = app.emit("langservers://changed", ());
    Ok(outcome)
}

#[tauri::command]
pub fn langservers_cancel_install(id: String, state: State<'_, LangServersState>) {
    if let Some(cancel) = state.installs.lock().expect("installs lock").get(&id) {
        cancel.notify_one();
    }
}

/// Project-wide symbol search over every running language server (`workspace/symbol`).
/// Paths are workspace relative like everywhere else.
#[tauri::command]
pub async fn lsp_workspace_symbols(
    query: String,
    limit: Option<usize>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Vec<EditorWorkspaceSymbol>> {
    let manager = match lsp.manager(&app, &ws) {
        Ok(m) => m,
        Err(_) => return Ok(vec![]),
    };
    let root = manager.root().to_path_buf();
    let mut hits = manager.workspace_symbols(&query).await;
    for h in &mut hits {
        if let Ok(rel) = Path::new(&h.path).strip_prefix(&root) {
            h.path = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
        }
    }
    hits.truncate(limit.unwrap_or(500));
    Ok(hits)
}

/// Move a plaintext licence key from the settings files into the keychain (once).
fn migrate_plaintext_licence(ws: &AppState) {
    let plaintext = ws
        .read_settings()
        .get(licence::LEGACY_SETTING_KEY)
        .and_then(Value::as_str)
        .map(str::to_owned);
    match Licence::new(&KeyringStore).migrate(plaintext.as_deref()) {
        Ok(true) => {
            let mut settings = ws.write_settings();
            for scope in [
                forge_core::SettingsScope::User,
                forge_core::SettingsScope::Project,
            ] {
                if settings
                    .layer_values(scope)
                    .contains_key(licence::LEGACY_SETTING_KEY)
                {
                    if let Err(e) = settings.set(licence::LEGACY_SETTING_KEY, Value::Null, scope) {
                        tracing::warn!("cannot remove the plaintext licence key: {e}");
                    }
                }
            }
        }
        Ok(false) => {}
        Err(e) => tracing::warn!("cannot migrate the Intelephense licence key: {e}"),
    }
}

/// "Free" or "Premium (…1234)"; migrates a plaintext key from older settings first.
#[tauri::command]
pub fn intelephense_licence_status(ws: State<'_, AppState>) -> LicenceStatus {
    migrate_plaintext_licence(&ws);
    Licence::new(&KeyringStore).status()
}

/// Store (or with `null`/empty remove) the licence key in the keychain and restart
/// Intelephense when it changed.
#[tauri::command]
pub fn intelephense_set_licence(
    key: Option<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<LicenceStatus> {
    let l = Licence::new(&KeyringStore);
    let changed = l.set(key.as_deref()).map_err(other)?;
    if changed {
        if let Ok(manager) = lsp.manager(&app, &ws) {
            apply_intelephense_licence(&manager);
        }
    }
    Ok(l.status())
}
