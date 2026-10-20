//! PHP debugger (Xdebug over DBGp). Logic lives in `forge-debug`; these commands wire it
//! to the UI. Events are emitted on `debug://listening|session|incoming|ended|output|
//! breakpoints|error` with the tagged `DebugEvent` JSON.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use forge_core::{Result, Settings};
use forge_debug::{
    Breakpoint, BreakpointSpec, BreakpointsSnapshot, ContextName, DebugConfig, DebugEvent,
    DebugServer, Frame, Property, ServerConfig, SessionInfo, StepKind,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;

pub struct DebugState {
    server: Arc<DebugServer>,
    watches: Mutex<Vec<String>>,
}

impl DebugState {
    pub fn new(app: AppHandle) -> Self {
        let server = DebugServer::new(Arc::new(move |event: DebugEvent| {
            if let Err(e) = app.emit(event.channel(), &event) {
                tracing::warn!("cannot emit debugger event: {e}");
            }
        }));
        Self {
            server: Arc::new(server),
            watches: Mutex::new(Vec::new()),
        }
    }
}

/// Debugger options from Settings > PHP > Debug.
fn config_from(settings: &Settings) -> DebugConfig {
    let d = DebugConfig::default();
    let int = |key: &str, default: u32| settings.get_as::<u32>(key).unwrap_or(default);
    let flag = |key: &str, default: bool| settings.get_bool(key).unwrap_or(default);
    DebugConfig {
        port: settings.get_as::<u16>("php.debug.port").unwrap_or(d.port),
        break_at_first_line: flag("php.debug.breakAtFirstLine", d.break_at_first_line),
        ignore_unmapped_files: flag("php.debug.ignoreUnmappedFiles", d.ignore_unmapped_files),
        ignore_external_connections: flag(
            "php.debug.ignoreExternalConnections",
            d.ignore_external_connections,
        ),
        max_children: int("php.debug.maxChildren", d.max_children),
        max_depth: int("php.debug.maxDepth", d.max_depth),
        max_data: int("php.debug.maxData", d.max_data),
        command_timeout_ms: d.command_timeout_ms,
    }
}

/// The debugger, synced with the open project and current settings.
fn server(state: &AppState, debug: &DebugState) -> Arc<DebugServer> {
    let root = state.with_workspace(|ws| Ok(ws.root().to_path_buf())).ok();
    if debug.server.project_root() != root {
        debug.server.open_project(root.as_deref());
    }
    debug.server.set_config(config_from(&state.read_settings()));
    Arc::clone(&debug.server)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugStatus {
    pub listening: bool,
    pub port: u16,
    pub sessions: Vec<SessionInfo>,
}

#[tauri::command]
pub fn debug_status(state: State<'_, AppState>, debug: State<'_, DebugState>) -> DebugStatus {
    let server = server(&state, &debug);
    let port = server.listening_port();
    DebugStatus {
        listening: port.is_some(),
        port: port.unwrap_or(server.config().port),
        sessions: server.sessions(),
    }
}

/// "Start Listening for PHP Debug Connections". Returns the port.
#[tauri::command]
pub async fn debug_listen_start(
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<u16> {
    let server = server(&state, &debug);
    let port = server.config().port;
    Ok(server.listen(port).await?)
}

#[tauri::command]
pub fn debug_listen_stop(state: State<'_, AppState>, debug: State<'_, DebugState>) {
    server(&state, &debug).stop_listening();
}

#[tauri::command]
pub fn debug_sessions(
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Vec<SessionInfo> {
    server(&state, &debug).sessions()
}

#[tauri::command]
pub async fn debug_accept_connection(
    session_id: String,
    server_config: ServerConfig,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<()> {
    let server = server(&state, &debug);
    Ok(server.accept_connection(&session_id, server_config).await?)
}

#[tauri::command]
pub async fn debug_ignore_connection(
    session_id: String,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<()> {
    let server = server(&state, &debug);
    Ok(server.ignore_connection(&session_id).await?)
}

/// Resume, Step Over/Into/Out, Force Step Into.
#[tauri::command]
pub fn debug_step(
    session_id: String,
    kind: StepKind,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<()> {
    let session = server(&state, &debug).session(&session_id)?;
    Ok(session.continue_with(kind)?)
}

#[tauri::command]
pub async fn debug_pause(
    session_id: String,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<()> {
    let session = server(&state, &debug).session(&session_id)?;
    Ok(session.pause().await?)
}

#[tauri::command]
pub async fn debug_stop(
    session_id: String,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<()> {
    let session = server(&state, &debug).session(&session_id)?;
    session.stop().await;
    Ok(())
}

#[tauri::command]
pub async fn debug_detach(
    session_id: String,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<()> {
    let session = server(&state, &debug).session(&session_id)?;
    session.detach().await;
    Ok(())
}

#[tauri::command]
pub async fn debug_run_to_cursor(
    session_id: String,
    file: String,
    line: u32,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<()> {
    let session = server(&state, &debug).session(&session_id)?;
    Ok(session.run_to(&file, line).await?)
}

#[tauri::command]
pub async fn debug_stack(
    session_id: String,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<Vec<Frame>> {
    let session = server(&state, &debug).session(&session_id)?;
    Ok(session.stack().await?)
}

#[tauri::command]
pub async fn debug_contexts(
    session_id: String,
    depth: u32,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<Vec<ContextName>> {
    let session = server(&state, &debug).session(&session_id)?;
    Ok(session.contexts(depth).await?)
}

/// Variables of a frame (`depth`) and context (0 locals, 1 superglobals, 2 constants).
#[tauri::command]
pub async fn debug_variables(
    session_id: String,
    depth: u32,
    context: Option<u32>,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<Vec<Property>> {
    let session = server(&state, &debug).session(&session_id)?;
    Ok(session.variables(depth, context.unwrap_or(0)).await?)
}

/// Expand a variable: one page of its children.
#[tauri::command]
pub async fn debug_property(
    session_id: String,
    fullname: String,
    depth: u32,
    context: Option<u32>,
    page: Option<u32>,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<Property> {
    let session = server(&state, &debug).session(&session_id)?;
    Ok(session
        .property(&fullname, depth, context.unwrap_or(0), page.unwrap_or(0))
        .await?)
}

#[tauri::command]
pub async fn debug_eval(
    session_id: String,
    expression: String,
    depth: Option<u32>,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<Property> {
    let session = server(&state, &debug).session(&session_id)?;
    Ok(session.eval(&expression, depth.unwrap_or(0)).await?)
}

#[tauri::command]
pub async fn debug_set_value(
    session_id: String,
    fullname: String,
    value: String,
    depth: Option<u32>,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<()> {
    let session = server(&state, &debug).session(&session_id)?;
    Ok(session
        .set_value(&fullname, &value, depth.unwrap_or(0))
        .await?)
}

// ---- breakpoints ----------------------------------------------------------------------

#[tauri::command]
pub fn debug_breakpoints(
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> BreakpointsSnapshot {
    server(&state, &debug).breakpoints()
}

#[tauri::command]
pub fn debug_breakpoint_add(
    spec: BreakpointSpec,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<Breakpoint> {
    Ok(server(&state, &debug).add_breakpoint(spec)?)
}

#[tauri::command]
pub fn debug_breakpoint_update(
    breakpoint: Breakpoint,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<Breakpoint> {
    Ok(server(&state, &debug).update_breakpoint(breakpoint)?)
}

#[tauri::command]
pub fn debug_breakpoint_remove(
    id: u64,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<()> {
    Ok(server(&state, &debug).remove_breakpoint(id)?)
}

/// Gutter click / Ctrl+F8. Returns the new breakpoint or `null` when one was removed.
#[tauri::command]
pub fn debug_breakpoint_toggle(
    file: String,
    line: u32,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<Option<Breakpoint>> {
    Ok(server(&state, &debug).toggle_line_breakpoint(&file, line)?)
}

#[tauri::command]
pub fn debug_breakpoints_mute(
    muted: bool,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<()> {
    Ok(server(&state, &debug).set_muted(muted)?)
}

// ---- watches, servers, launching ------------------------------------------------------

#[tauri::command]
pub fn debug_watches(debug: State<'_, DebugState>) -> Vec<String> {
    debug.watches.lock().expect("watches lock").clone()
}

#[tauri::command]
pub fn debug_watches_set(watches: Vec<String>, debug: State<'_, DebugState>) {
    *debug.watches.lock().expect("watches lock") = watches;
}

#[tauri::command]
pub fn debug_servers(
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Vec<ServerConfig> {
    server(&state, &debug).servers()
}

#[tauri::command]
pub fn debug_servers_save(
    servers: Vec<ServerConfig>,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<()> {
    Ok(server(&state, &debug).save_servers(servers)?)
}

/// Environment variables that make a PHP process started by Forge connect to the
/// debugger (for Debug run configurations).
#[tauri::command]
pub fn debug_launch_env(
    server_name: Option<String>,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> BTreeMap<String, String> {
    let server = server(&state, &debug);
    let port = server.listening_port().unwrap_or(server.config().port);
    forge_debug::launch_env(port, server_name.as_deref())
        .into_iter()
        .collect()
}
