//! Language servers for many languages (catalog in `forge-langservers`): activation for
//! the open project, the Language Servers settings page, installs with streamed output,
//! project-wide symbol search and the Intelephense licence key. Which servers start is
//! planned where the project's files are (`forge_backend::langservers`): on the remote host
//! for remote projects, whose servers then run there.
//!
//! Events:
//! - `langservers://install-output` `{ id, stream, line }`
//! - `langservers://changed` (rows changed after an apply or an install)

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use forge_backend::langservers::{system_rows, LiveServer, SETTINGS_PREFIX};
use forge_backend::LangServersRequest;
use forge_core::{ForgeError, Result};
use forge_langservers::install::{self, InstallOutcome, Stream};
use forge_langservers::licence::{self, KeyringStore, Licence, LicenceStatus};
use forge_langservers::{Catalog, Detector, ServerRow};
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

fn other(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

/// What the planner needs from the app: the `langservers.*` settings, disabled plugins and
/// the running servers.
fn request(ws: &AppState, manager: Option<&LspManager>, rescan: bool) -> LangServersRequest {
    let settings = ws
        .read_settings()
        .effective()
        .into_iter()
        .filter(|(k, _)| k.starts_with(SETTINGS_PREFIX))
        .collect();
    let disabled_plugins = ws
        .plugins
        .read()
        .expect("plugins lock poisoned")
        .list()
        .iter()
        .filter(|p| !p.enabled)
        .map(|p| p.manifest.id.clone())
        .collect();
    LangServersRequest {
        settings,
        disabled_plugins,
        live: live_states(manager),
        rescan,
    }
}

/// Register the enabled and installed catalog servers for the languages of the manager's
/// project and unregister the ones no longer wanted. Called when the LSP manager of a
/// project is created and after settings change. Also applies the Intelephense licence
/// key to a registered Intelephense config. Blocks while a remote host plans.
pub fn register_enabled(manager: &LspManager, ws: &AppState) {
    apply(manager, ws, true);
}

fn apply(manager: &LspManager, ws: &AppState, rescan: bool) {
    let answer = ws
        .backend()
        .and_then(|b| b.langservers(request(ws, Some(manager), rescan)));
    let answer = match answer {
        Ok(a) => a,
        Err(e) => {
            tracing::warn!("language servers not planned: {e}");
            return;
        }
    };
    let registered: HashMap<String, LspServerConfig> = manager
        .configs()
        .into_iter()
        .map(|c| (c.id.clone(), c))
        .collect();
    for planned in answer.servers {
        match planned.config {
            Some(config) => {
                if registered.get(&config.id) != Some(&config) {
                    tracing::info!("language server {} registered", config.id);
                    manager.register_server(config);
                }
            }
            None if registered.contains_key(&planned.id) => {
                manager.unregister_server(&planned.id);
            }
            None => {}
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

fn live_states(manager: Option<&LspManager>) -> Vec<LiveServer> {
    let Some(manager) = manager else {
        return Vec::new();
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
            LiveServer {
                id: s.id,
                running,
                detail,
            }
        })
        .collect()
}

fn build_rows(manager: Option<&LspManager>, ws: &AppState) -> Vec<ServerRow> {
    let req = request(ws, manager, false);
    match ws.backend() {
        Ok(backend) => backend
            .langservers(req)
            .map(|a| a.rows)
            .unwrap_or_else(|e| {
                tracing::warn!("language servers not listed: {e}");
                Vec::new()
            }),
        Err(_) => system_rows(&req),
    }
}

/// Rows of Settings > Languages & Frameworks > Language Servers.
#[tauri::command]
pub async fn langservers_list(app: AppHandle) -> Result<Vec<ServerRow>> {
    crate::commands::workspace::blocking(move || {
        let lsp = app.state::<LspState>();
        let ws = app.state::<AppState>();
        let manager = lsp.manager(&app, &ws).ok();
        Ok(build_rows(manager.as_deref(), &ws))
    })
    .await
}

/// Re-plan after settings changed: start newly enabled servers, stop disabled ones.
#[tauri::command]
pub async fn langservers_apply(app: AppHandle) -> Result<Vec<ServerRow>> {
    crate::commands::workspace::blocking(move || {
        let lsp = app.state::<LspState>();
        let ws = app.state::<AppState>();
        let manager = lsp.manager(&app, &ws).ok();
        if let Some(m) = manager.as_deref() {
            apply(m, &ws, true);
        }
        let rows = build_rows(manager.as_deref(), &ws);
        let _ = app.emit("langservers://changed", ());
        Ok(rows)
    })
    .await
}

#[derive(Serialize, Clone)]
struct InstallLine {
    id: String,
    stream: Stream,
    line: String,
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
    if let Some(host) = ws.project_info().and_then(|i| i.remote) {
        return Err(other(format!(
            "Language servers of this project run on {}. Install them there from the terminal (Alt+F12).",
            host.label()
        )));
    }
    let spec = Catalog::builtin()
        .server(&id)
        .or_else(|| Catalog::pack_servers().server(&id))
        .ok_or_else(|| other(format!("unknown language server {id}")))?;
    let option = spec
        .install
        .get(index)
        .ok_or_else(|| other("unknown install option"))?;
    let detector = Detector::system();
    let (program, args) = install::resolve_command(option, &detector).map_err(other)?;
    let cancel = Arc::new(tokio::sync::Notify::new());
    {
        let mut installs = state.installs.lock().expect("installs lock");
        if installs.contains_key(&id) {
            return Err(other(format!("{} is already being installed", spec.name)));
        }
        installs.insert(id.clone(), cancel.clone());
    }
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
    emit(
        Stream::Info,
        match &outcome {
            InstallOutcome::Succeeded => format!("{} installed.", spec.name),
            InstallOutcome::Failed { code: Some(c) } => format!("Failed with exit code {c}."),
            InstallOutcome::Failed { code: None } => "Failed.".into(),
            InstallOutcome::Cancelled => "Cancelled.".into(),
        },
    );
    state.installs.lock().expect("installs lock").remove(&id);
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
