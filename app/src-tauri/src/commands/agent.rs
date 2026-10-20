//! AI agent CLIs (Claude Code, Gemini CLI) driven headless for the chat tool window.
//! Events of a session are emitted as `agent://<sessionId>`: the raw `AgentEvent` JSON and,
//! after each, a `chat` update (`forge_agent_cli::ChatUpdate`) with the rendered blocks of the
//! current turn that the UI shows.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_agent_cli::cli::AllowScope;
use forge_agent_cli::{
    agent_by_id, builtin_agents, compose_prompt, configured_executable, AgentEvent, AgentInfo,
    AgentSession, Chat, ChatUpdate, EventSink, MessageContext, PermissionDecision,
    PermissionStatus, StartOptions,
};
use forge_core::{ForgeError, Result};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::state::AppState;

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

/// Available agent CLIs and whether they are installed. `executables` maps agent ids to
/// executable paths to check (e.g. unsaved values in the Settings dialog); otherwise the
/// paths from Settings > Tools > AI Agents are used.
#[tauri::command]
pub fn agent_list(
    executables: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Vec<AgentInfo> {
    builtin_agents()
        .iter()
        .map(|a| {
            let path = match &executables {
                Some(map) => map.get(a.id()).cloned(),
                None => configured_executable(a.id(), |k| setting(&state, k)),
            };
            a.info(path.as_deref())
        })
        .collect()
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
    let adapter = agent_by_id(&agent_id)
        .ok_or_else(|| ForgeError::Other(format!("unknown agent {agent_id}")))?;
    let cwd = state.with_workspace(|ws| Ok(ws.root().to_path_buf()))?;
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
    let agent = AgentSession::start(adapter, cwd, options, sink).await?;
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
