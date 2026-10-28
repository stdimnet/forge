//! Language servers for many languages (catalog in `forge-langservers`): activation for
//! the open project, the Language Servers settings page, installs with streamed output,
//! project-wide symbol search. Which servers start is
//! planned where the project's files are (`forge_backend::langservers`): on the remote host
//! for remote projects, whose servers then run there.
//!
//! npm servers (typescript-language-server, yaml-language-server, …) are installed by Forge itself
//! (`forge_langservers::managed`): Node.js and the packages are downloaded and verified
//! into Forge's data directory, so nothing has to be installed first. For a remote project
//! the install runs in the agent on the host (`Backend::install_langserver`), where its
//! servers run.
//!
//! Events:
//! - `langservers://install-output` `{ id, stream, line }`
//! - `langservers://install-progress` `{ id, label, done, total }` (downloads, for the
//!   status bar)
//! - `langservers://changed` (rows changed after an apply or an install)

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use forge_backend::langservers::{is_planner_setting, system_rows, LiveServer};
use forge_backend::{InstallRequest, InstallSink, InstallTarget, LangServersRequest};
use forge_core::{ForgeError, Result};
use forge_langservers::install::{InstallOutcome, Stream};
use forge_langservers::managed::{Event, PackageUpdate};
use forge_langservers::{Catalog, ServerRow};
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
        .filter(|(k, _)| is_planner_setting(k))
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
/// project is created and after settings change. Blocks while a remote host plans.
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
    crate::commands::rust::configure_analyzer(manager, ws);
    crate::commands::ruby::configure_lsp(manager, ws);
    crate::commands::dotnet::configure_servers(manager, ws);
}

fn live_states(manager: Option<&LspManager>) -> Vec<LiveServer> {
    let Some(manager) = manager else {
        return Vec::new();
    };
    // The PHP pack registers php-lsp as `php`; its state belongs to php-lsp's row, too.
    let php_row = manager
        .configs()
        .iter()
        .any(|c| c.id == crate::commands::php::PHP_SERVER_ID)
        .then_some(forge_backend::langservers::BUILTIN_PHP_ROW);
    let mut live: Vec<LiveServer> = manager
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
        .collect();
    if let Some(row) = php_row {
        if let Some(php) = live
            .iter()
            .find(|l| l.id == crate::commands::php::PHP_SERVER_ID)
            .cloned()
        {
            live.push(LiveServer {
                id: row.into(),
                ..php
            });
        }
    }
    live
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

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct InstallProgress {
    id: String,
    label: String,
    done: u64,
    total: Option<u64>,
}

/// Download sources from the settings (`langservers.download.*`).
fn download_sources(ws: &AppState) -> (Option<String>, Option<String>) {
    let settings = ws.read_settings();
    let get = |k: &str| settings.get(k).and_then(Value::as_str).map(str::to_owned);
    (
        get("langservers.download.npmRegistry"),
        get("langservers.download.nodeMirror"),
    )
}

/// Streams install steps as output lines of `id` and downloads as progress events.
fn install_sink(app: &AppHandle, id: &str) -> InstallSink {
    let (app, id) = (app.clone(), id.to_string());
    let last_step: Mutex<Option<(String, u64)>> = Mutex::new(None);
    Arc::new(move |e: Event| match e {
        Event::Step { message } => emit_line(&app, &id, Stream::Info, message),
        Event::Output { stream, line } => emit_line(&app, &id, stream, line),
        Event::Progress { label, done, total } => {
            // One event per percent (or per MB without a length) is plenty for a progress bar.
            let step = total.map_or(done >> 20, |t| done * 100 / t.max(1));
            let mut last = last_step.lock().expect("progress lock");
            if last.as_ref() == Some(&(label.clone(), step)) {
                return;
            }
            *last = Some((label.clone(), step));
            drop(last);
            let _ = app.emit(
                "langservers://install-progress",
                InstallProgress {
                    id: id.clone(),
                    label,
                    done,
                    total,
                },
            );
        }
    })
}

/// Install where the project's language servers run: on the remote host for a remote
/// project (the agent downloads Node.js and the packages there), else on this machine.
/// Output streams under `id`.
async fn run_install(
    app: &AppHandle,
    ws: &AppState,
    id: &str,
    target: InstallTarget,
    cancel: Arc<tokio::sync::Notify>,
) -> std::result::Result<InstallOutcome, String> {
    let (registry, node_mirror) = download_sources(ws);
    let req = InstallRequest {
        target,
        registry,
        node_mirror,
    };
    let sink = install_sink(app, id);
    if let Some(host) = ws.project_info().and_then(|i| i.remote) {
        emit_line(
            app,
            id,
            Stream::Info,
            format!("Installing on {}…", host.label()),
        );
        let backend = ws.backend().map_err(|e| e.to_string())?;
        return crate::commands::workspace::blocking(move || {
            backend.install_langserver(req, cancel, sink)
        })
        .await
        .map_err(|e| e.to_string());
    }
    let mut on = move |e: Event| sink(e);
    forge_backend::langservers::install(&req, cancel, &mut on).await
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

/// Register the catalog servers that were just installed.
async fn register_installed(app: &AppHandle) {
    let handle = app.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        let lsp = handle.state::<LspState>();
        let ws = handle.state::<AppState>();
        if let Ok(manager) = lsp.manager(&handle, &ws) {
            apply(&manager, &ws, false);
        }
    })
    .await;
}

/// Run install option `index` of server `id` (catalog commands only, after the user
/// confirmed it in the UI) where the project's servers run: on the remote host for a
/// remote project. Output streams as `langservers://install-output`.
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
    if spec.install.get(index).is_none() {
        return Err(other("unknown install option"));
    }
    let cancel = Arc::new(tokio::sync::Notify::new());
    {
        let mut installs = state.installs.lock().expect("installs lock");
        if installs.contains_key(&id) {
            return Err(other(format!("{} is already being installed", spec.name)));
        }
        installs.insert(id.clone(), cancel.clone());
    }
    let target = InstallTarget::Server {
        id: id.clone(),
        index,
    };
    let outcome = run_install(&app, &ws, &id, target, cancel).await;
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

/// Newer versions of the npm servers Forge installed (on the remote host for a remote
/// project). Asks the registry at most once a day unless `force`; `None` when it did not
/// ask (or checks are off in the settings).
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
    let (registry, node_mirror) = download_sources(&ws);
    if ws.is_remote() {
        let backend = ws.backend()?;
        return crate::commands::workspace::blocking(move || {
            backend.langserver_updates(registry, node_mirror, force)
        })
        .await;
    }
    forge_backend::langservers::check_updates(registry.as_deref(), node_mirror.as_deref(), force)
        .await
        .map_err(other)
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
    let target = InstallTarget::Package {
        spec: package.clone(),
    };
    let outcome = run_install(&app, &ws, &package, target, cancel).await;
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
