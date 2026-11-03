//! Settings | Tools | MCP Server: Forge as a Model Context Protocol server for external
//! agents. The protocol, transports, tool catalog and permission policy live in
//! `crates/forge-mcp`; this module is the IDE side ([`McpHost`]): it reads the settings,
//! asks the user, and answers tool calls with the existing services. Tools that need the
//! editor (open editors, selections, undoable edits, problems, run configurations) are
//! forwarded to the UI.
//!
//! Events:
//! - `mcp://status` [`McpStatus`] when the server starts, stops or a client connects;
//! - `mcp://call` [`CallLogEntry`] for every finished tool call;
//! - `mcp://approval` [`ApprovalRequest`]: show an approval card, answered with
//!   `mcp_answer_approval`; `mcp://approval-closed` `{ id }` when nobody answered in time;
//! - `mcp://tool-request` `{ id, tool, arguments }`: run an editor tool in the UI, answered
//!   with `mcp_tool_result`.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_ai_editor::policy::IGNORE_FILES;
use forge_ai_editor::AiIgnore;
use forge_core::{FileKind, ForgeError, Result};
use forge_mcp::adapters::{self, OutputBuffer};
use forge_mcp::engine::{
    Approval, ApprovalRequest, CallLogEntry, Engine, McpHost, ResourceInfo, ServerEvent,
    SessionInfo, ToolOutput,
};
use forge_mcp::protocol::ServerInfo;
use forge_mcp::stdio::{self, Discovery};
use forge_mcp::tools::Runner;
use forge_mcp::{CallPolicy, ClientConfig, McpServer, ToolGroup, ToolSpec, DEFAULT_PORT};
use forge_process::{CommandLine, RunEvent};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::oneshot;

use crate::commands::workspace::blocking;
use crate::state::AppState;

pub const STATUS_EVENT: &str = "mcp://status";
pub const CALL_EVENT: &str = "mcp://call";
pub const APPROVAL_EVENT: &str = "mcp://approval";
pub const APPROVAL_CLOSED_EVENT: &str = "mcp://approval-closed";
pub const TOOL_REQUEST_EVENT: &str = "mcp://tool-request";

pub const KEY_ENABLED: &str = "mcp.enabled";
pub const KEY_PORT: &str = "mcp.port";
pub const KEY_REQUIRE_APPROVAL: &str = "mcp.requireApproval";

/// How long an approval card waits before the call is denied.
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);
/// How long the UI may take for an editor tool (runs wait longer, see `run_configuration`).
const EDITOR_TIMEOUT: Duration = Duration::from_secs(60);
/// Entries kept in the call log.
const LOG_LIMIT: usize = 500;
/// Entries `list_directory` returns at most.
const MAX_LISTED: usize = 2_000;

/// What the settings page and the status bar show.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub enabled: bool,
    pub running: bool,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub error: Option<String>,
    pub sessions: Vec<SessionInfo>,
    /// The token is stored in the OS keychain (client configurations survive restarts).
    pub token_persistent: bool,
    /// The project is in Safe Mode: runs and terminal commands are refused.
    pub safe_mode: bool,
    /// The project turned AI features off: every call is refused.
    pub ai_disabled: bool,
}

type ToolAnswer = std::result::Result<Value, String>;

#[derive(Default)]
pub struct McpState {
    server: Mutex<Option<McpServer>>,
    status: Mutex<McpStatus>,
    token: Mutex<Option<String>>,
    token_persistent: AtomicBool,
    log: Mutex<VecDeque<CallLogEntry>>,
    approvals: Mutex<HashMap<u64, oneshot::Sender<Approval>>>,
    requests: Mutex<HashMap<u64, oneshot::Sender<ToolAnswer>>>,
    next_request: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl McpState {
    fn token(&self) -> Option<String> {
        lock(&self.token).clone()
    }
}

fn setting<T: serde::de::DeserializeOwned>(app: &AppHandle, key: &str, default: T) -> T {
    app.state::<AppState>()
        .read_settings()
        .get_as(key)
        .unwrap_or(default)
}

/// The status with the live sessions and project state.
fn current_status(app: &AppHandle) -> McpStatus {
    let state = app.state::<McpState>();
    let mut status = lock(&state.status).clone();
    status.sessions = lock(&state.server)
        .as_ref()
        .map(|s| s.engine().sessions())
        .unwrap_or_default();
    status.token_persistent = state.token_persistent.load(Ordering::Relaxed);
    let app_state = app.state::<AppState>();
    status.safe_mode = app_state.backend().is_ok() && app_state.safe_mode.is_on();
    status.ai_disabled = !setting(app, crate::commands::ai_editor::PROJECT_ENABLED, true);
    status
}

fn emit_status(app: &AppHandle) {
    let _ = app.emit(STATUS_EVENT, current_status(app));
}

/// The bearer token: from the keychain, or a new one.
async fn ensure_token(app: &AppHandle) -> String {
    let state = app.state::<McpState>();
    if let Some(t) = state.token() {
        return t;
    }
    let (token, persistent) = tauri::async_runtime::spawn_blocking(forge_mcp::token::load_or_create)
        .await
        .unwrap_or_else(|_| (forge_mcp::server::new_token(), false));
    state.token_persistent.store(persistent, Ordering::Relaxed);
    *lock(&state.token) = Some(token.clone());
    token
}

/// Start, restart or stop the server as the settings say. Only the first project window
/// runs it.
pub fn apply_server_settings(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let enabled = setting(&app, KEY_ENABLED, false);
        let port = setting::<i64>(&app, KEY_PORT, DEFAULT_PORT as i64);
        let port = u16::try_from(port)
            .ok()
            .filter(|p| *p >= 1024)
            .unwrap_or(DEFAULT_PORT);
        let state = app.state::<McpState>();
        if let Some(server) = lock(&state.server).take() {
            server.stop();
            if let Some(path) = stdio::discovery_path() {
                stdio::remove_discovery(&path, std::process::id());
            }
        }
        let mut status = McpStatus {
            enabled,
            ..McpStatus::default()
        };
        if enabled {
            let token = ensure_token(&app).await;
            let host = Arc::new(TauriHost { app: app.clone() });
            let engine = Arc::new(Engine::new(
                host,
                ServerInfo::forge(app.package_info().version.to_string()),
            ));
            match McpServer::start(port, token, engine).await {
                Ok(server) => {
                    status.running = true;
                    status.port = Some(server.port);
                    status.url = Some(server.url());
                    tracing::info!("MCP server listening on {}", server.url());
                    if let Some(path) = stdio::discovery_path() {
                        let discovery = Discovery {
                            port: server.port,
                            pid: std::process::id(),
                            version: app.package_info().version.to_string(),
                        };
                        if let Err(e) = stdio::write_discovery(&path, &discovery) {
                            tracing::warn!("MCP discovery file not written: {e}");
                        }
                    }
                    *lock(&state.server) = Some(server);
                }
                Err(e) => status.error = Some(format!("Cannot listen on port {port}: {e}")),
            }
        }
        *lock(&state.status) = status;
        emit_status(&app);
    });
}

struct TauriHost {
    app: AppHandle,
}

impl TauriHost {
    fn state(&self) -> State<'_, McpState> {
        self.app.state::<McpState>()
    }

    /// Ask the UI to run an editor tool and wait for its answer.
    async fn editor(&self, tool: &str, arguments: Value, timeout: Duration) -> ToolAnswer {
        let id = self.state().next_request.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        lock(&self.state().requests).insert(id, tx);
        let sent = self.app.emit(
            TOOL_REQUEST_EVENT,
            json!({ "id": id, "tool": tool, "arguments": arguments }),
        );
        let answer = match sent {
            Err(e) => Err(format!("Forge's editor is not available: {e}")),
            Ok(()) => match tokio::time::timeout(timeout, rx).await {
                Ok(Ok(answer)) => answer,
                Ok(Err(_)) => Err("Forge's editor did not answer".into()),
                Err(_) => Err(format!(
                    "Forge's editor did not answer within {} seconds",
                    timeout.as_secs()
                )),
            },
        };
        lock(&self.state().requests).remove(&id);
        answer
    }
}

#[async_trait]
impl McpHost for TauriHost {
    async fn policy(&self) -> CallPolicy {
        let app_state = self.app.state::<AppState>();
        let (enabled_groups, require_approval, ai_enabled) = {
            let settings = app_state.read_settings();
            let groups = ToolGroup::ALL
                .into_iter()
                .filter(|g| {
                    settings
                        .get_bool(&g.setting_key())
                        .unwrap_or(g.default_enabled())
                })
                .collect();
            (
                groups,
                settings.get_bool(KEY_REQUIRE_APPROVAL).unwrap_or(true),
                settings
                    .get_bool(crate::commands::ai_editor::PROJECT_ENABLED)
                    .unwrap_or(true),
            )
        };
        let backend = app_state.backend().ok();
        let project_open = backend.is_some();
        let ignore = match backend {
            Some(b) => blocking(move || {
                let texts: Vec<String> = IGNORE_FILES
                    .iter()
                    .filter_map(|f| b.read_bytes(f).ok())
                    .map(|b| String::from_utf8_lossy(&b).into_owned())
                    .collect();
                Ok(AiIgnore::merged(texts.iter().map(String::as_str)))
            })
            .await
            .unwrap_or_default(),
            None => AiIgnore::default(),
        };
        CallPolicy {
            project_open,
            enabled_groups,
            require_approval,
            safe_mode: app_state.safe_mode.is_on(),
            ai_enabled,
            ignore,
        }
    }

    async fn approve(&self, request: ApprovalRequest) -> Approval {
        let id = request.id;
        let (tx, rx) = oneshot::channel();
        lock(&self.state().approvals).insert(id, tx);
        if self.app.emit(APPROVAL_EVENT, &request).is_err() {
            lock(&self.state().approvals).remove(&id);
            return Approval::Deny;
        }
        let answer = match tokio::time::timeout(APPROVAL_TIMEOUT, rx).await {
            Ok(Ok(a)) => a,
            _ => {
                let _ = self.app.emit(APPROVAL_CLOSED_EVENT, json!({ "id": id }));
                Approval::Deny
            }
        };
        lock(&self.state().approvals).remove(&id);
        answer
    }

    async fn call_tool(&self, spec: &ToolSpec, args: Value) -> std::result::Result<ToolOutput, String> {
        match spec.runner {
            Runner::Editor if spec.name == "replace_text_in_file" => {
                // The editor's text (with unsaved changes) is changed here and applied in
                // the editor as one undo step.
                let path = str_arg(&args, "path");
                let buffer = self
                    .editor("buffer_text", json!({ "path": path }), EDITOR_TIMEOUT)
                    .await?;
                let text = buffer["text"].as_str().ok_or("the file cannot be read")?;
                let (changed, count) = adapters::replace_text(
                    text,
                    args["oldText"].as_str().unwrap_or(""),
                    args["newText"].as_str().unwrap_or(""),
                    args["replaceAll"].as_bool().unwrap_or(false),
                )?;
                self.editor(
                    "apply_text",
                    json!({ "path": path, "text": changed }),
                    EDITOR_TIMEOUT,
                )
                .await?;
                Ok(ToolOutput::Text(format!(
                    "Replaced {count} occurrence{} in {path} and saved it (Undo in Forge reverts it).",
                    if count == 1 { "" } else { "s" }
                )))
            }
            Runner::Editor if spec.name == "write_file" => {
                let path = str_arg(&args, "path");
                self.editor(
                    "apply_text",
                    json!({ "path": path, "text": str_arg(&args, "text"), "create": true }),
                    EDITOR_TIMEOUT,
                )
                .await?;
                Ok(ToolOutput::Text(format!(
                    "Wrote {path} (Undo in Forge reverts it)."
                )))
            }
            Runner::Editor => {
                let timeout = if spec.name == "run_configuration" {
                    Duration::from_secs(args["timeoutSeconds"].as_u64().unwrap_or(300) + 15)
                } else {
                    EDITOR_TIMEOUT
                };
                self.editor(spec.name, args, timeout).await.map(|v| match v {
                    Value::String(s) => ToolOutput::Text(s),
                    other => ToolOutput::Json(other),
                })
            }
            Runner::Backend => backend_tool(&self.app, spec.name, args)
                .await
                .map_err(|e| e.to_string()),
        }
    }

    async fn list_resources(&self) -> Vec<ResourceInfo> {
        let Ok(open) = self.editor("get_open_editors", json!({}), EDITOR_TIMEOUT).await else {
            return Vec::new();
        };
        open["editors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| e["path"].as_str())
            .map(|path| ResourceInfo {
                path: path.to_string(),
                name: path.rsplit('/').next().unwrap_or(path).to_string(),
                mime_type: None,
            })
            .collect()
    }

    async fn read_resource(&self, path: &str) -> std::result::Result<String, String> {
        let backend = self.app.state::<AppState>().backend().map_err(|e| e.to_string())?;
        let path = path.to_string();
        blocking(move || backend.read_file(&path))
            .await
            .map_err(|e| e.to_string())
    }

    fn on_event(&self, event: ServerEvent) {
        match event {
            ServerEvent::SessionsChanged => emit_status(&self.app),
            ServerEvent::Call(entry) => {
                {
                    let state = self.state();
                    let mut log = lock(&state.log);
                    log.push_back(entry.clone());
                    while log.len() > LOG_LIMIT {
                        log.pop_front();
                    }
                }
                let _ = self.app.emit(CALL_EVENT, entry);
            }
        }
    }
}

fn str_arg(args: &Value, name: &str) -> String {
    args[name].as_str().unwrap_or("").to_string()
}

fn u32_arg(args: &Value, name: &str) -> u32 {
    args[name].as_u64().unwrap_or(1).max(1) as u32 - 1
}

fn to_json<T: Serialize>(v: T) -> Result<ToolOutput> {
    serde_json::to_value(v)
        .map(ToolOutput::Json)
        .map_err(|e| ForgeError::Other(e.to_string()))
}

/// Tools answered by the Rust services.
async fn backend_tool(app: &AppHandle, name: &str, args: Value) -> Result<ToolOutput> {
    let state = app.state::<AppState>();
    let backend = state.backend()?;
    match name {
        "read_file" => {
            let path = str_arg(&args, "path");
            let text = blocking(move || backend.read_file(&path)).await?;
            Ok(ToolOutput::Text(adapters::line_range(
                &text,
                args["startLine"].as_u64(),
                args["endLine"].as_u64(),
            )))
        }
        "list_directory" => {
            let root = forge_mcp::policy::normalize_path(&str_arg(&args, "path"))
                .map_err(ForgeError::Other)?;
            let depth = args["depth"].as_u64().unwrap_or(1).clamp(1, 5) as usize;
            let entries = blocking(move || {
                let mut out = Vec::new();
                let mut queue = VecDeque::from([(root, 1usize)]);
                while let Some((dir, level)) = queue.pop_front() {
                    for e in backend.list_dir(&dir)? {
                        if out.len() >= MAX_LISTED {
                            return Ok((out, true));
                        }
                        let is_dir = e.kind == FileKind::Dir;
                        if is_dir && level < depth && !e.ignored && !e.excluded {
                            queue.push_back((e.path.clone(), level + 1));
                        }
                        out.push(json!({
                            "path": e.path,
                            "type": if is_dir { "directory" } else { "file" },
                            "ignored": e.ignored,
                            "excluded": e.excluded,
                        }));
                    }
                }
                Ok((out, false))
            })
            .await?;
            Ok(ToolOutput::Json(
                json!({ "entries": entries.0, "truncated": entries.1 }),
            ))
        }
        "find_files" => {
            let query = str_arg(&args, "query");
            let limit = args["limit"].as_u64().unwrap_or(50) as usize;
            let found = blocking(move || backend.find_files(&query, limit, false)).await?;
            let files: Vec<Value> = found
                .into_iter()
                .map(|f| json!({ "path": f.path, "name": f.name }))
                .collect();
            Ok(ToolOutput::Json(json!({ "files": files })))
        }
        "search_in_files" => {
            let mut req: forge_core::search::FindRequest = serde_json::from_value(args.clone())
                .map_err(|e| ForgeError::Other(format!("invalid arguments: {e}")))?;
            req.limit = args["limit"].as_u64().unwrap_or(200).clamp(1, 2000) as usize;
            let result = blocking(move || backend.find_in_files(&req)).await?;
            let value = serde_json::to_value(result).map_err(|e| ForgeError::Other(e.to_string()))?;
            Ok(ToolOutput::Json(adapters::find_result(&value)))
        }
        "find_symbol" => {
            let query = str_arg(&args, "query");
            let types_only = args["classesOnly"].as_bool().unwrap_or(false);
            let limit = args["limit"].as_u64().unwrap_or(50) as usize;
            let hits = blocking(move || backend.find_symbols(&query, types_only, limit, &[], false))
                .await?;
            let value = serde_json::to_value(hits).map_err(|e| ForgeError::Other(e.to_string()))?;
            Ok(ToolOutput::Json(adapters::symbol_hits(&value)))
        }
        "go_to_definition" => {
            let answer = crate::commands::codeintel::code_goto_declaration(
                str_arg(&args, "path"),
                u32_arg(&args, "line"),
                u32_arg(&args, "column"),
                None,
                app.clone(),
                app.state(),
                app.state(),
                app.state(),
            )
            .await?;
            let value = serde_json::to_value(answer).map_err(|e| ForgeError::Other(e.to_string()))?;
            Ok(ToolOutput::Json(adapters::declaration(&value)))
        }
        "find_usages" => {
            let answer = crate::commands::codeintel::code_find_usages(
                str_arg(&args, "path"),
                u32_arg(&args, "line"),
                u32_arg(&args, "column"),
                None,
                None,
                None,
                None,
                app.clone(),
                app.state(),
                app.state(),
            )
            .await?;
            let value = serde_json::to_value(answer).map_err(|e| ForgeError::Other(e.to_string()))?;
            Ok(ToolOutput::Json(adapters::usages(&value)))
        }
        "run_inspections" => {
            let path = forge_mcp::policy::normalize_path(&str_arg(&args, "path"))
                .map_err(ForgeError::Other)?;
            let is_dir = {
                let (b, p) = (backend.clone(), path.clone());
                path.is_empty() || blocking(move || Ok(b.list_dir(&p).is_ok())).await?
            };
            let scope = crate::inspections::InspectScope {
                kind: match (path.is_empty(), is_dir) {
                    (true, _) => "project",
                    (false, true) => "directory",
                    (false, false) => "file",
                }
                .into(),
                path: (!path.is_empty()).then(|| path.clone()),
                label: if path.is_empty() {
                    "Project Files".into()
                } else {
                    path.clone()
                },
                ..Default::default()
            };
            let request = crate::commands::inspections::InspectRequest {
                scope,
                ..Default::default()
            };
            let report = crate::commands::inspections::inspections_run(
                request,
                app.state(),
                app.state(),
                app.clone(),
            )
            .await?;
            let value = serde_json::to_value(report).map_err(|e| ForgeError::Other(e.to_string()))?;
            Ok(ToolOutput::Json(adapters::inspection_report(&value)))
        }
        "git_status" => to_json(blocking(move || backend.repo()?.status()).await?),
        "git_diff" => {
            let paths: Vec<String> = args["paths"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            let staged = args["staged"].as_bool().unwrap_or(false);
            let diff = blocking(move || backend.repo()?.diff_text(&paths, staged)).await?;
            Ok(ToolOutput::Text(if diff.trim().is_empty() {
                "No changes.".into()
            } else {
                diff
            }))
        }
        "execute_terminal_command" => {
            state.require_trusted("Terminal commands from MCP clients")?;
            terminal_command(backend, &str_arg(&args, "command"), args["timeoutSeconds"].as_u64())
                .await
        }
        "list_data_sources" => {
            let list = app.state::<crate::commands::db::DbState>().service.list();
            let sources: Vec<Value> = list
                .iter()
                .map(|d| {
                    json!({
                        "id": d.source.id,
                        "name": d.source.name,
                        "driver": d.source.driver,
                        "database": d.source.database,
                        "readOnly": d.source.options.read_only,
                        "production": d.production,
                        "summary": d.summary,
                    })
                })
                .collect();
            Ok(ToolOutput::Json(json!({ "dataSources": sources })))
        }
        "run_sql_query" => run_sql_query(app, &args).await,
        other => Err(ForgeError::Other(format!("{other} is not available"))),
    }
}

/// Run a shell command in the project root and wait for it.
async fn terminal_command(
    backend: Arc<dyn forge_backend::Backend>,
    command: &str,
    timeout: Option<u64>,
) -> Result<ToolOutput> {
    let timeout = Duration::from_secs(timeout.unwrap_or(60).clamp(1, 600));
    let cmd = if cfg!(windows) {
        CommandLine::new("cmd").args(["/C", command])
    } else {
        CommandLine::new("sh").args(["-c", command])
    };
    let cmd = CommandLine {
        cwd: backend.info().root.clone(),
        ..cmd
    };
    let output = Arc::new(Mutex::new(OutputBuffer::default()));
    let (exit_tx, exit_rx) = oneshot::channel::<Option<i32>>();
    let exit_tx = Mutex::new(Some(exit_tx));
    let sink_output = output.clone();
    let sink: forge_process::RunSink = Arc::new(move |event| match event {
        RunEvent::Output { text, .. } => lock(&sink_output).push(&text),
        RunEvent::Exited { code, .. } => {
            if let Some(tx) = lock(&exit_tx).take() {
                let _ = tx.send(code);
            }
        }
        RunEvent::Started { .. } => {}
    });
    let process = {
        let b = backend.clone();
        blocking(move || b.start_process(&cmd, sink)).await?
    };
    let (exit_code, timed_out) = match tokio::time::timeout(timeout, exit_rx).await {
        Ok(code) => (code.ok().flatten(), false),
        Err(_) => {
            process.kill(Duration::from_secs(2));
            (None, true)
        }
    };
    let text = lock(&output).text();
    Ok(ToolOutput::Json(json!({
        "exitCode": exit_code,
        "timedOut": timed_out,
        "output": text,
    })))
}

/// A read-only query: anything that writes is refused before it reaches the database, and
/// the data source's own guards (read-only, production) still apply.
async fn run_sql_query(app: &AppHandle, args: &Value) -> Result<ToolOutput> {
    let svc = app.state::<crate::commands::db::DbState>().service.clone();
    let wanted = str_arg(args, "dataSource");
    let sql = str_arg(args, "sql");
    let info = svc
        .list()
        .into_iter()
        .find(|d| d.source.id == wanted || d.source.name.eq_ignore_ascii_case(&wanted))
        .ok_or_else(|| ForgeError::Other(format!("No data source '{wanted}'")))?;
    let policy = forge_db::guard::GuardPolicy {
        read_only: true,
        production: info.production,
        name: info.source.name.clone(),
    };
    let verdict = forge_db::guard::guard(&sql, info.source.dialect(), &policy);
    if let Some(blocked) = verdict.blocked {
        return Err(ForgeError::Other(format!(
            "{blocked} MCP clients can only run read-only queries."
        )));
    }
    let max_rows = args["maxRows"].as_u64().unwrap_or(100).clamp(1, 1000) as u32;
    let session = svc
        .open_session_with(&info.source.id, forge_db::SessionMode::Own)
        .await
        .map_err(|e| ForgeError::Other(e.to_string()))?;
    let result = svc
        .execute_with(&session.id, &sql, Some(max_rows), false)
        .await;
    let _ = svc.close_session(&session.id).await;
    let results = result.map_err(|e| ForgeError::Other(e.to_string()))?;
    to_json(json!({ "dataSource": info.source.name, "results": results }))
}

#[tauri::command]
pub fn mcp_status(app: AppHandle) -> McpStatus {
    current_status(&app)
}

/// "Copy client configuration": snippets for the running server. `None` while it is off.
#[tauri::command]
pub async fn mcp_client_configs(app: AppHandle) -> Option<Vec<ClientConfig>> {
    let url = current_status(&app).url?;
    let token = ensure_token(&app).await;
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "forge".into());
    Some(forge_mcp::configs(&url, &token, &exe))
}

/// A new token; connected clients and copied configurations stop working.
#[tauri::command]
pub async fn mcp_regenerate_token(app: AppHandle) -> Result<()> {
    let (token, persistent) = tauri::async_runtime::spawn_blocking(forge_mcp::token::regenerate)
        .await
        .map_err(|e| ForgeError::Other(e.to_string()))?;
    let state = app.state::<McpState>();
    state.token_persistent.store(persistent, Ordering::Relaxed);
    *lock(&state.token) = Some(token);
    apply_server_settings(&app);
    Ok(())
}

#[tauri::command]
pub fn mcp_call_log(state: State<'_, McpState>) -> Vec<CallLogEntry> {
    lock(&state.log).iter().cloned().collect()
}

#[tauri::command]
pub fn mcp_clear_log(state: State<'_, McpState>) {
    lock(&state.log).clear();
}

/// The answer of an approval card.
#[tauri::command]
pub fn mcp_answer_approval(id: u64, answer: Approval, state: State<'_, McpState>) -> bool {
    lock(&state.approvals)
        .remove(&id)
        .is_some_and(|tx| tx.send(answer).is_ok())
}

/// The UI's answer to `mcp://tool-request`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub result: Option<Value>,
    pub error: Option<String>,
}

#[tauri::command]
pub fn mcp_tool_result(id: u64, answer: ToolResult, state: State<'_, McpState>) -> bool {
    let value = match (answer.error, answer.result) {
        (Some(e), _) => Err(e),
        (None, r) => Ok(r.unwrap_or(Value::Null)),
    };
    lock(&state.requests)
        .remove(&id)
        .is_some_and(|tx| tx.send(value).is_ok())
}

/// End a client's session (its next request fails; clients reconnect with `initialize`).
#[tauri::command]
pub fn mcp_disconnect(session: String, app: AppHandle) -> bool {
    let ended = lock(&app.state::<McpState>().server)
        .as_ref()
        .is_some_and(|s| s.engine().end_session(&session));
    emit_status(&app);
    ended
}

/// `forge --mcp-stdio`: the stdio proxy to the running Forge. Returns the exit code.
pub fn stdio_main(args: &[String]) -> Option<i32> {
    if args.get(1).map(String::as_str) != Some(forge_mcp::STDIO_FLAG) {
        return None;
    }
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("forge --mcp-stdio: {e}");
            return Some(1);
        }
    };
    let result = runtime.block_on(async {
        let input = tokio::io::BufReader::new(tokio::io::stdin());
        stdio::run_proxy(input, tokio::io::stdout(), || {
            stdio::resolve_target(stdio::discovery_path(), forge_mcp::token::stored)
        })
        .await
    });
    Some(match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("forge --mcp-stdio: {e}");
            1
        }
    })
}
