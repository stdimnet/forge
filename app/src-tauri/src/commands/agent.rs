//! AI agent CLIs (Claude Code, Gemini CLI) driven headless for the chat tool window.
//! Events of a session are emitted as `agent://<sessionId>`: the raw `AgentEvent` JSON and,
//! after each, a `chat` update (`forge_agent_cli::ChatUpdate`) with the rendered blocks of the
//! current turn that the UI shows.
//!
//! For a project on a remote host the CLI runs on that host (found there, started through
//! the project backend's process API), so it works on the project's files with the login
//! it has there.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forge_agent_cli::cli::AllowScope;
use forge_agent_cli::host::SpawnFuture;
use forge_agent_cli::{
    agent_by_id, builtin_agents, compose_prompt, configured_executable, AgentCli, AgentEvent,
    AgentHost, AgentInfo, AgentSession, Chat, ChatUpdate, CommandSpec, EventSink, LocalHost,
    MessageContext, PermissionDecision, PermissionStatus, SpawnedAgent, StartOptions,
};
use forge_backend::process_io::spawn_stdio;
use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_process::CommandLine;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Runs agent CLIs where a remote project lives, through its backend.
struct BackendHost {
    backend: Arc<dyn Backend>,
}

impl AgentHost for BackendHost {
    fn detect(&self, adapter: &dyn AgentCli, override_path: Option<&str>) -> Option<PathBuf> {
        let names: Vec<String> = adapter
            .executable_names()
            .iter()
            .map(|n| n.to_string())
            .collect();
        self.backend
            .find_system_executable(&names, override_path)
            .unwrap_or_else(|e| {
                tracing::warn!("cannot look for {} on the host: {e}", adapter.name());
                None
            })
    }

    fn spawn(&self, spec: CommandSpec, cwd: PathBuf) -> SpawnFuture {
        let backend = self.backend.clone();
        Box::pin(async move {
            let mut cmd = CommandLine::new(spec.program.to_string_lossy()).args(spec.args);
            cmd.cwd = cwd;
            let p = blocking(move || spawn_stdio(backend.as_ref(), &cmd)).await?;
            let process = p.process.clone();
            let exit = p.exit;
            Ok(SpawnedAgent {
                stdin: Box::new(p.stdin),
                stdout: Box::new(p.stdout),
                stderr: Box::new(p.stderr),
                exit: Box::pin(async move { exit.await.ok().flatten() }),
                kill: Box::new(move || {
                    // Remote kills wait for the host.
                    std::thread::spawn(move || process.kill(Duration::from_secs(2)));
                }),
            })
        })
    }

    fn remote_label(&self) -> Option<String> {
        self.backend.info().remote.as_ref().map(|r| r.label())
    }
}

/// Where agent CLIs of the open project run.
fn host(state: &AppState) -> Arc<dyn AgentHost> {
    match state.backend() {
        Ok(backend) if backend.info().remote.is_some() => Arc::new(BackendHost { backend }),
        _ => Arc::new(LocalHost),
    }
}

struct Session {
    agent: Arc<AgentSession>,
    chat: Arc<std::sync::Mutex<Chat>>,
    app: AppHandle,
    event: String,
}

impl Session {
    fn emit(&self, update: &ChatUpdate) {
        emit(&self.app, &self.event, update);
    }
}

/// Running agent sessions, keyed by Forge's own session id.
#[derive(Default)]
pub struct AgentState {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    next_id: AtomicU64,
}

impl AgentState {
    async fn get(&self, id: &str) -> Result<Arc<Session>> {
        self.sessions
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| ForgeError::Other(format!("no agent session {id}")))
    }
}

fn emit<T: serde::Serialize>(app: &AppHandle, event: &str, payload: &T) {
    if let Err(e) = app.emit(event, payload) {
        tracing::warn!("could not emit {event}: {e}");
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// A string setting's effective value.
fn setting(state: &AppState, key: &str) -> Option<String> {
    state.read_settings().get_str(key).map(String::from)
}

/// Available agent CLIs and whether they are installed (on the remote host for a remote
/// project). `executables` maps agent ids to executable paths to check (e.g. unsaved values
/// in the Settings dialog); otherwise the paths from Settings > Tools > AI Agents are used.
#[tauri::command]
pub async fn agent_list(
    executables: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<Vec<AgentInfo>> {
    let host = host(&state);
    let paths: Vec<Option<String>> = builtin_agents()
        .iter()
        .map(|a| match &executables {
            Some(map) => map.get(a.id()).cloned(),
            None => configured_executable(a.id(), |k| setting(&state, k)),
        })
        .collect();
    blocking(move || {
        Ok(builtin_agents()
            .iter()
            .zip(paths)
            .map(|(a, path)| a.info_with(host.detect(a.as_ref(), path.as_deref())))
            .collect())
    })
    .await
}

/// Start a session in the open project; returns the id used for events and later calls.
#[tauri::command]
pub async fn agent_start(
    agent_id: String,
    options: Option<StartOptions>,
    app: AppHandle,
    state: State<'_, AppState>,
    agents: State<'_, AgentState>,
) -> Result<String> {
    // Agent CLIs run the project's hooks, MCP servers and commands.
    state.require_trusted("Starting AI agents")?;
    let adapter = agent_by_id(&agent_id)
        .ok_or_else(|| ForgeError::Other(format!("unknown agent {agent_id}")))?;
    let cwd = state.project_root()?;
    let host = host(&state);
    let n = agents.next_id.fetch_add(1, Ordering::SeqCst);
    let session_id = format!("{agent_id}-{n}");
    let event_name = format!("agent://{session_id}");
    let chat = Arc::new(std::sync::Mutex::new(Chat::new()));
    let sink: EventSink = {
        let (app, chat, event_name) = (app.clone(), chat.clone(), event_name.clone());
        Arc::new(move |event: AgentEvent| {
            let update = chat.lock().expect("chat poisoned").apply(&event, now_ms());
            emit(&app, &event_name, &event);
            emit(&app, &event_name, &update);
        })
    };
    let options = options
        .unwrap_or_default()
        .with_settings(&agent_id, |k| setting(&state, k));
    let executable = {
        let (host, adapter, path) = (host.clone(), adapter.clone(), options.executable.clone());
        blocking(move || Ok(host.detect(adapter.as_ref(), path.as_deref()))).await?
    };
    let agent = AgentSession::start_on(host, adapter, executable, cwd, options, sink).await?;
    let session = Session {
        agent: Arc::new(agent),
        chat,
        app,
        event: event_name,
    };
    agents
        .sessions
        .lock()
        .await
        .insert(session_id.clone(), Arc::new(session));
    Ok(session_id)
}

/// Send a user message with optional editor context (attached files, selection).
/// `turn_id` names the turn in the `chat` updates.
#[tauri::command]
pub async fn agent_send(
    session_id: String,
    text: String,
    context: Option<MessageContext>,
    turn_id: Option<u64>,
    agents: State<'_, AgentState>,
) -> Result<()> {
    let session = agents.get(&session_id).await?;
    let context = context.unwrap_or_default();
    let prompt = compose_prompt(&text, &context);
    let update = session.chat.lock().expect("chat poisoned").begin_turn(
        turn_id.unwrap_or_default(),
        text.trim(),
        context,
    );
    session.emit(&update);
    session.agent.send(&prompt).await
}

/// Answer a `permissionRequest` event: `{ behavior: "allow", scope?: "once" | "session",
/// updatedInput? }` or `{ behavior: "deny", message? }`.
#[tauri::command]
pub async fn agent_respond_permission(
    session_id: String,
    request_id: String,
    decision: PermissionDecision,
    agents: State<'_, AgentState>,
) -> Result<()> {
    let session = agents.get(&session_id).await?;
    let status = match &decision {
        PermissionDecision::Deny { .. } => PermissionStatus::Denied,
        PermissionDecision::Allow {
            scope: AllowScope::Session,
            ..
        } => PermissionStatus::AllowedForSession,
        PermissionDecision::Allow { .. } => PermissionStatus::Allowed,
    };
    let update = session
        .chat
        .lock()
        .expect("chat poisoned")
        .set_permission(&request_id, status);
    if let Some(update) = update {
        session.emit(&update);
    }
    session
        .agent
        .respond_permission(&request_id, decision)
        .await
}

/// Stop the running turn; the conversation continues with the next message.
#[tauri::command]
pub async fn agent_interrupt(session_id: String, agents: State<'_, AgentState>) -> Result<()> {
    agents.get(&session_id).await?.agent.interrupt().await;
    Ok(())
}

/// End a session and its process.
#[tauri::command]
pub async fn agent_stop(session_id: String, agents: State<'_, AgentState>) -> Result<()> {
    let session = agents.sessions.lock().await.remove(&session_id);
    if let Some(session) = session {
        session.agent.stop().await;
    }
    Ok(())
}
