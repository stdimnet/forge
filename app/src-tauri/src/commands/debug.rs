//! PHP debugger (Xdebug over DBGp). Logic lives in `forge-debug`; these commands wire it
//! to the UI. Events are emitted on `debug://listening|session|incoming|ended|output|
//! breakpoints|error` with the tagged `DebugEvent` JSON.
//!
//! Remote projects: breakpoints and servers live in the project's `.forge/` on the host,
//! and "Start Listening" listens on a free local port that the host's `127.0.0.1:<debug
//! port>` is forwarded to over SSH (Xdebug there connects to its own localhost). Paths
//! Xdebug reports are the host's, which are the project's paths, so no mappings are needed.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use forge_backend::{BackendFiles, PortForward};
use forge_core::{ForgeError, Result, Settings};
use forge_dap::{DapManager, DapSession};
use forge_debug::console::Completion;
use forge_debug::validation::{self, Expectation, ValidationReport};
use forge_debug::{
    Breakpoint, BreakpointSpec, BreakpointsSnapshot, ContextName, DebugConfig, DebugError,
    DebugEvent, DebugServer, EventSink, Frame, Property, ServerConfig, SessionInfo, StepKind,
};
use forge_process::output::{RunInput, RunOutput, RunOutputOptions};
use forge_process::RunEvent;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::commands::php_interpreters as interpreters;
use crate::state::AppState;

pub struct DebugState {
    server: Arc<DebugServer>,
    /// Debug Adapter Protocol sessions (every other language); they share the breakpoints,
    /// events and commands of the PHP debugger (see `commands/dap.rs`).
    pub(crate) dap: Arc<DapManager>,
    watches: Mutex<Vec<String>>,
    /// The project the server was opened for (its display root).
    project: Mutex<Option<String>>,
    /// The SSH forward of a remote project's debug port while listening.
    forward: Mutex<Option<PortForward>>,
    /// Servers of the PHP interpreters debug runs went through (their path mappings).
    interpreter_servers: Mutex<Vec<ServerConfig>>,
}

impl DebugState {
    pub fn new(app: AppHandle) -> Self {
        // Breakpoint changes reach the DAP sessions through the shared event sink.
        let dap_slot: Arc<OnceLock<Weak<DapManager>>> = Arc::new(OnceLock::new());
        let hook = Arc::clone(&dap_slot);
        let sink: EventSink = Arc::new(move |event: DebugEvent| {
            if matches!(event, DebugEvent::Breakpoints { .. }) {
                if let Some(dap) = hook.get().and_then(Weak::upgrade) {
                    dap.breakpoints_changed();
                }
            }
            if let Err(e) = app.emit(event.channel(), &event) {
                tracing::warn!("cannot emit debugger event: {e}");
            }
        });
        let server = Arc::new(DebugServer::new(Arc::clone(&sink)));
        let dap = Arc::new(DapManager::new(sink, server.clone()));
        let _ = dap_slot.set(Arc::downgrade(&dap));
        Self {
            server,
            dap,
            watches: Mutex::new(Vec::new()),
            project: Mutex::new(None),
            forward: Mutex::new(None),
            interpreter_servers: Mutex::new(Vec::new()),
        }
    }

    /// Make the debugger map paths of runs through a PHP interpreter (Docker, Compose,
    /// SSH, WSL) with the interpreter's mappings.
    pub fn add_interpreter_server(&self, server: ServerConfig) {
        let mut list = self.interpreter_servers.lock().expect("servers lock");
        list.retain(|s| s.id != server.id && s.name != server.name);
        list.push(server);
        self.server.set_interpreter_servers(list.clone());
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

/// The DAP session `id`, if it is one (Xdebug sessions are `xdebug-<n>`).
pub(crate) fn dap_session(debug: &DebugState, id: &str) -> Option<Arc<DapSession>> {
    debug.dap.session(id).ok()
}

/// The debugger, synced with the open project and current settings. Breakpoints and
/// servers are read through the project backend (on the host for remote projects).
pub(crate) fn server(state: &AppState, debug: &DebugState) -> Arc<DebugServer> {
    let info = state.project_info();
    let key = info.as_ref().map(|i| i.display_root());
    let mut current = debug.project.lock().expect("project lock");
    if *current != key {
        *current = key;
        let project = match (info, state.backend()) {
            (Some(info), Ok(backend)) => Some((
                info.root,
                Arc::new(BackendFiles(backend)) as forge_core::SharedFiles,
            )),
            _ => None,
        };
        debug.server.open_project_files(project);
        // A forward belongs to the previous project's connection.
        debug.forward.lock().expect("forward lock").take();
    }
    drop(current);
    debug.server.set_config(config_from(&state.read_settings()));
    Arc::clone(&debug.server)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugStatus {
    pub listening: bool,
    pub port: u16,
    pub sessions: Vec<SessionInfo>,
    /// For a remote project: the host whose port is forwarded here (`deploy@shop.example.com`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
}

#[tauri::command]
pub fn debug_status(state: State<'_, AppState>, debug: State<'_, DebugState>) -> DebugStatus {
    let server = server(&state, &debug);
    let port = server.listening_port();
    let via = debug
        .forward
        .lock()
        .expect("forward lock")
        .as_ref()
        .filter(|_| port.is_some())
        .map(|f| f.via.clone());
    let mut sessions = server.sessions();
    sessions.extend(debug.dap.sessions());
    DebugStatus {
        listening: port.is_some(),
        port: port.unwrap_or(server.config().port),
        sessions,
        via,
    }
}

/// "Start Listening for PHP Debug Connections". Returns the port debuggers connect to (on
/// the remote host for a remote project).
#[tauri::command]
pub async fn debug_listen_start(
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<u16> {
    state.require_trusted("The debugger")?;
    let server = server(&state, &debug);
    let port = server.config().port;
    let tunnels = state.backend().ok().and_then(|b| b.tunnels());
    debug.forward.lock().expect("forward lock").take();
    let Some(tunnels) = tunnels else {
        return Ok(server.listen(port).await?);
    };
    let (shown, forward) = server
        .listen_with(0, |local| async move {
            let forward = tunnels.forward_remote(port, local).await.map_err(|e| {
                DebugError::Other(format!(
                    "Cannot listen on port {port} of the remote host: {e}"
                ))
            })?;
            Ok((forward.port, forward))
        })
        .await?;
    *debug.forward.lock().expect("forward lock") = Some(forward);
    Ok(shown)
}

#[tauri::command]
pub fn debug_listen_stop(state: State<'_, AppState>, debug: State<'_, DebugState>) {
    server(&state, &debug).stop_listening();
    debug.forward.lock().expect("forward lock").take();
}

#[tauri::command]
pub fn debug_sessions(
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Vec<SessionInfo> {
    let mut sessions = server(&state, &debug).sessions();
    sessions.extend(debug.dap.sessions());
    sessions
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

/// Resume, Step Over/Into/Out, Force Step Into, and instruction steps (DAP).
#[tauri::command]
pub async fn debug_step(
    session_id: String,
    kind: StepKind,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<()> {
    if let Some(s) = dap_session(&debug, &session_id) {
        return Ok(s.continue_with(kind).await?);
    }
    let session = server(&state, &debug).session(&session_id)?;
    Ok(session.continue_with(kind)?)
}

#[tauri::command]
pub async fn debug_pause(
    session_id: String,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<()> {
    if let Some(s) = dap_session(&debug, &session_id) {
        return Ok(s.pause().await?);
    }
    let session = server(&state, &debug).session(&session_id)?;
    Ok(session.pause().await?)
}

#[tauri::command]
pub async fn debug_stop(
    session_id: String,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<()> {
    if let Some(s) = dap_session(&debug, &session_id) {
        s.stop().await;
        return Ok(());
    }
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
    if let Some(s) = dap_session(&debug, &session_id) {
        s.detach().await;
        return Ok(());
    }
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
    if let Some(s) = dap_session(&debug, &session_id) {
        return Ok(s.run_to(&file, line).await?);
    }
    let session = server(&state, &debug).session(&session_id)?;
    Ok(session.run_to(&file, line).await?)
}

/// Frames of the suspended script; DAP sessions take the thread to show (`threadId`).
#[tauri::command]
pub async fn debug_stack(
    session_id: String,
    thread_id: Option<i64>,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<Vec<Frame>> {
    if let Some(s) = dap_session(&debug, &session_id) {
        return Ok(s.stack(thread_id).await?);
    }
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
    if let Some(s) = dap_session(&debug, &session_id) {
        return Ok(s.contexts(depth).await?);
    }
    let session = server(&state, &debug).session(&session_id)?;
    Ok(session.contexts(depth).await?)
}

/// Variables of a frame (`depth`) and context (0 locals, 1 superglobals, 2 constants; DAP:
/// the scope index, 0 = first scope plus the others as groups).
#[tauri::command]
pub async fn debug_variables(
    session_id: String,
    depth: u32,
    context: Option<u32>,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<Vec<Property>> {
    if let Some(s) = dap_session(&debug, &session_id) {
        return Ok(s.variables(depth, context.unwrap_or(0)).await?);
    }
    let session = server(&state, &debug).session(&session_id)?;
    Ok(session.variables(depth, context.unwrap_or(0)).await?)
}

/// Expand a variable: one page of its children (DAP: all children of `reference`).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn debug_property(
    session_id: String,
    fullname: String,
    depth: u32,
    context: Option<u32>,
    page: Option<u32>,
    reference: Option<i64>,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<Property> {
    if let Some(s) = dap_session(&debug, &session_id) {
        let reference = reference
            .ok_or_else(|| ForgeError::Other(format!("'{fullname}' has no children to expand")))?;
        let children = s.children(reference).await?;
        return Ok(Property {
            name: fullname.clone(),
            fullname,
            r#type: "dap".into(),
            has_children: true,
            num_children: children.len() as u32,
            children,
            reference: Some(reference),
            ..Default::default()
        });
    }
    let session = server(&state, &debug).session(&session_id)?;
    Ok(session
        .property(&fullname, depth, context.unwrap_or(0), page.unwrap_or(0))
        .await?)
}

/// Evaluate an expression; DAP sessions take the `context` (`watch`, `repl`, `hover`).
#[tauri::command]
pub async fn debug_eval(
    session_id: String,
    expression: String,
    depth: Option<u32>,
    context: Option<String>,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<Property> {
    if let Some(s) = dap_session(&debug, &session_id) {
        let context = context.unwrap_or_else(|| "repl".into());
        return Ok(s.eval(&expression, depth.unwrap_or(0), &context).await?);
    }
    let session = server(&state, &debug).session(&session_id)?;
    Ok(session.eval(&expression, depth.unwrap_or(0)).await?)
}

/// Set Value; DAP variables are addressed by their `container` reference and `name`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn debug_set_value(
    session_id: String,
    fullname: String,
    value: String,
    depth: Option<u32>,
    container: Option<i64>,
    name: Option<String>,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<()> {
    if let Some(s) = dap_session(&debug, &session_id) {
        return Ok(s
            .set_value(
                container,
                name.as_deref(),
                &fullname,
                &value,
                depth.unwrap_or(0),
            )
            .await?);
    }
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
pub async fn debug_breakpoint_add(
    spec: BreakpointSpec,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<Breakpoint> {
    Ok(server(&state, &debug).add_breakpoint(spec)?)
}

#[tauri::command]
pub async fn debug_breakpoint_update(
    breakpoint: Breakpoint,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<Breakpoint> {
    Ok(server(&state, &debug).update_breakpoint(breakpoint)?)
}

#[tauri::command]
pub async fn debug_breakpoint_remove(
    id: u64,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<()> {
    Ok(server(&state, &debug).remove_breakpoint(id)?)
}

/// Gutter click / Ctrl+F8. Returns the new breakpoint or `null` when one was removed.
#[tauri::command]
pub async fn debug_breakpoint_toggle(
    file: String,
    line: u32,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<Option<Breakpoint>> {
    Ok(server(&state, &debug).toggle_line_breakpoint(&file, line)?)
}

#[tauri::command]
pub async fn debug_breakpoints_mute(
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

/// Completion in the debugger console and the Evaluate dialog: variables after `$`,
/// properties after `->` (the caret is at the end of `text`).
#[tauri::command]
pub async fn debug_complete(
    session_id: String,
    text: String,
    depth: Option<u32>,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<Vec<Completion>> {
    if let Some(s) = dap_session(&debug, &session_id) {
        return Ok(s.complete(&text, depth.unwrap_or(0)).await?);
    }
    let session = server(&state, &debug).session(&session_id)?;
    Ok(session.complete(&text, depth.unwrap_or(0)).await?)
}

// ---- PHP Web Page -----------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPageStarted {
    pub run_id: String,
    pub command_line: String,
    /// The page the UI opens in the browser.
    pub url: String,
}

/// The configured server `server_id` (by id or name; `None`: no server).
fn find_server(servers: &[ServerConfig], server_id: Option<&str>) -> Result<Option<ServerConfig>> {
    match server_id.filter(|s| !s.is_empty()) {
        Some(id) => servers
            .iter()
            .find(|s| s.id == id || s.name == id)
            .cloned()
            .map(Some)
            .ok_or_else(|| {
                ForgeError::Other(format!(
                    "Server '{id}' is not configured (Settings | PHP | Servers)"
                ))
            }),
        None => Ok(None),
    }
}

/// The URL of `startUrl` on a server (PHP HTTP Request configurations), with the Xdebug
/// trigger when `debug`.
#[tauri::command]
pub fn debug_page_url(
    server_id: Option<String>,
    start_url: String,
    debug: Option<bool>,
    state: State<'_, AppState>,
    debug_state: State<'_, DebugState>,
) -> Result<String> {
    let servers = server(&state, &debug_state).servers();
    let srv = find_server(&servers, server_id.as_deref())?;
    Ok(forge_debug::web::page_url(
        srv.as_ref(),
        &start_url,
        debug.unwrap_or(false),
    ))
}

/// Start a "PHP Web Page" configuration: the URL of `startUrl` on the server `serverId`
/// (with `XDEBUG_SESSION_START` when debugging), reported on `run://<runId>` like a
/// finished process. The UI opens the returned URL in the browser.
#[tauri::command]
pub fn debug_web_page_start(
    run_id: String,
    server_id: Option<String>,
    start_url: String,
    debug: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
    debug_state: State<'_, DebugState>,
) -> Result<WebPageStarted> {
    state.require_trusted("Running PHP web pages")?;
    let servers = server(&state, &debug_state).servers();
    let srv = find_server(&servers, server_id.as_deref())?;
    let debugging = debug.unwrap_or(false);
    let url = forge_debug::web::page_url(srv.as_ref(), &start_url, debugging);
    let event = format!("run://{run_id}");
    let mut output = RunOutput::new(&RunOutputOptions::default());
    emit_run(
        &app,
        &event,
        &RunEvent::Started {
            pid: 0,
            command_line: url.clone(),
        },
    );
    let text = if debugging {
        format!("Opening {url}\nThe debug session starts when Xdebug connects.\n")
    } else {
        format!("Opening {url}\n")
    };
    for rendered in output.handle(&RunInput::Output {
        stream: "stdout".into(),
        text,
    }) {
        emit_run(&app, &event, &rendered);
    }
    emit_run(
        &app,
        &event,
        &RunEvent::Exited {
            code: Some(0),
            signal: None,
        },
    );
    Ok(WebPageStarted {
        run_id,
        command_line: url.clone(),
        url,
    })
}

fn emit_run<T: Serialize>(app: &AppHandle, event: &str, payload: &T) {
    if let Err(e) = app.emit(event, payload) {
        tracing::warn!("cannot emit {event}: {e}");
    }
}

// ---- Web Server Debug Validation --------------------------------------------------------

fn expectation(state: &AppState, debug: &DebugState, cli: bool) -> Expectation {
    let srv = server(state, debug);
    Expectation {
        ide_port: srv.listening_port().unwrap_or(srv.config().port),
        cli,
    }
}

/// Validate the debugger configuration of a CLI interpreter (by name; empty = the
/// project's) from its `php -i`.
#[tauri::command]
pub async fn debug_validate_cli(
    interpreter: Option<String>,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<ValidationReport> {
    let expect = expectation(&state, &debug, true);
    let interp = interpreters::resolve(&state, interpreter.as_deref().unwrap_or(""));
    let text = interpreters::phpinfo(&state, interp).await?;
    if !text.contains("PHP Version") {
        let first: String = text.trim().chars().take(300).collect();
        return Err(ForgeError::Other(format!(
            "The interpreter did not print phpinfo(): {first}"
        )));
    }
    Ok(validation::validate(
        &validation::parse_phpinfo(&text),
        expect,
    ))
}

/// Validate a web server: place the validation script in `documentRoot` (a project
/// folder, with the user's consent), request it under `url` (the URL of that folder) and
/// delete it again.
#[tauri::command]
pub async fn debug_validate_web(
    document_root: String,
    url: String,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<ValidationReport> {
    let expect = expectation(&state, &debug, false);
    let backend = state.backend()?;
    let token = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let script = validation::script_name(token);
    let dir = document_root.trim().trim_matches('/').replace('\\', "/");
    let rel = if dir.is_empty() || dir == "." {
        script.clone()
    } else {
        format!("{dir}/{script}")
    };
    let write_backend = backend.clone();
    let write_rel = rel.clone();
    crate::commands::workspace::blocking(move || {
        write_backend.write_file(&write_rel, validation::VALIDATION_SCRIPT)
    })
    .await?;
    let request = forge_scratches::http::HttpRequest {
        index: 0,
        line: 0,
        end_line: 0,
        name: None,
        method: "GET".into(),
        url: validation::script_url(&url, &script),
        version: None,
        headers: Vec::new(),
        body: None,
        handler: None,
        handler_file: None,
    };
    let send_from_host = state
        .read_settings()
        .get_as::<String>(crate::commands::scratches::SEND_FROM_KEY)
        .is_none_or(|v| v != "local");
    let tunnels = backend.tunnels().filter(|_| send_from_host);
    let timeout = std::time::Duration::from_secs(30);
    let sent = match &tunnels {
        Some(t) => {
            let mut note = String::new();
            crate::commands::scratches::send_from_host_of(t.as_ref(), &request, &mut note).await
        }
        None => forge_scratches::client::send(&request, timeout).await,
    };
    // The script never stays behind, whatever the answer was.
    let delete_rel = rel.clone();
    let deleted = crate::commands::workspace::blocking(move || backend.delete(&delete_rel)).await;
    if let Err(e) = deleted {
        tracing::warn!("cannot delete the validation script {rel}: {e}");
    }
    let resp =
        sent.map_err(|e| ForgeError::Other(format!("Cannot request {}: {e}", request.url)))?;
    if resp.status == 404 {
        return Err(ForgeError::Other(format!(
            "{} answered 404 Not Found: the URL does not point to the document root.",
            request.url
        )));
    }
    let env = validation::parse_script_output(&resp.body).map_err(ForgeError::Other)?;
    Ok(validation::validate(&env, expect))
}
