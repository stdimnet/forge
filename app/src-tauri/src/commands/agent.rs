//! AI agent CLIs (Claude Code, Gemini CLI) driven headless for the chat tool window.
//! Events of a session are emitted as `agent://<sessionId>` with `AgentEvent` JSON.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use forge_agent_cli::{
    agent_by_id, builtin_agents, compose_prompt, AgentEvent, AgentInfo, AgentSession, EventSink,
    MessageContext, PermissionDecision, StartOptions,
};
use forge_core::{ForgeError, Result};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::state::AppState;

/// Running agent sessions, keyed by Forge's own session id.
#[derive(Default)]
pub struct AgentState {
    sessions: Mutex<HashMap<String, Arc<AgentSession>>>,
    next_id: AtomicU64,
}

impl AgentState {
    async fn get(&self, id: &str) -> Result<Arc<AgentSession>> {
        self.sessions
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| ForgeError::Other(format!("no agent session {id}")))
    }
}

/// Available agent CLIs and whether they are installed. `executables` maps agent ids to
/// user-configured executable paths.
#[tauri::command]
pub fn agent_list(executables: Option<HashMap<String, String>>) -> Vec<AgentInfo> {
    let executables = executables.unwrap_or_default();
    builtin_agents()
        .iter()
        .map(|a| a.info(executables.get(a.id()).map(String::as_str)))
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
    let sink: EventSink = Arc::new(move |event: AgentEvent| {
        if let Err(e) = app.emit(&event_name, &event) {
            tracing::warn!("could not emit {event_name}: {e}");
        }
    });
    let session = AgentSession::start(adapter, cwd, options.unwrap_or_default(), sink).await?;
    agents
        .sessions
        .lock()
        .await
        .insert(session_id.clone(), Arc::new(session));
    Ok(session_id)
}

/// Send a user message with optional editor context (attached files, selection).
#[tauri::command]
pub async fn agent_send(
    session_id: String,
    text: String,
    context: Option<MessageContext>,
    agents: State<'_, AgentState>,
) -> Result<()> {
    let session = agents.get(&session_id).await?;
    let prompt = compose_prompt(&text, &context.unwrap_or_default());
    session.send(&prompt).await
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
    agents
        .get(&session_id)
        .await?
        .respond_permission(&request_id, decision)
        .await
}

/// Stop the running turn; the conversation continues with the next message.
#[tauri::command]
pub async fn agent_interrupt(session_id: String, agents: State<'_, AgentState>) -> Result<()> {
    agents.get(&session_id).await?.interrupt().await;
    Ok(())
}

/// End a session and its process.
#[tauri::command]
pub async fn agent_stop(session_id: String, agents: State<'_, AgentState>) -> Result<()> {
    let session = agents.sessions.lock().await.remove(&session_id);
    if let Some(session) = session {
        session.stop().await;
    }
    Ok(())
}
