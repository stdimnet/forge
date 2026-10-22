//! Per-project state outside the project (`forge_core::project_state`): the workbench
//! layout (user cache dir) and the AI chat history (user data dir).

use tauri::State;

use forge_core::project_state::{self, ChatRecord, ChatSummary, ProjectLayout};
use forge_core::{ForgeError, Result};

use super::workspace::blocking;
use crate::state::AppState;

/// The open project's root as the UI identifies it (`user@host:/path` for remote ones).
fn project_root(state: &AppState) -> Result<String> {
    state
        .project_info()
        .map(|i| i.display_root())
        .ok_or_else(|| ForgeError::Other("no project is open".into()))
}

fn layout_dir() -> Result<std::path::PathBuf> {
    project_state::default_layout_dir()
        .ok_or_else(|| ForgeError::Other("no cache directory".into()))
}

fn chat_dir() -> Result<std::path::PathBuf> {
    project_state::default_chat_dir().ok_or_else(|| ForgeError::Other("no data directory".into()))
}

/// The saved layout of the open project, if any.
#[tauri::command]
pub async fn layout_load(state: State<'_, AppState>) -> Result<Option<ProjectLayout>> {
    let root = project_root(&state)?;
    blocking(move || Ok(project_state::load_layout(&layout_dir()?, &root))).await
}

#[tauri::command]
pub async fn layout_save(layout: ProjectLayout, state: State<'_, AppState>) -> Result<()> {
    let root = project_root(&state)?;
    blocking(move || project_state::save_layout(&layout_dir()?, &root, &layout)).await
}

/// The layout stored with Window | Store Current Layout as Default (tool windows only).
#[tauri::command]
pub async fn layout_default_load() -> Result<Option<ProjectLayout>> {
    blocking(move || Ok(project_state::load_default_layout(&layout_dir()?))).await
}

#[tauri::command]
pub async fn layout_default_save(layout: ProjectLayout) -> Result<()> {
    blocking(move || project_state::save_default_layout(&layout_dir()?, &layout)).await
}

/// The open project's AI chats, newest first.
#[tauri::command]
pub async fn agent_history_list(state: State<'_, AppState>) -> Result<Vec<ChatSummary>> {
    let root = project_root(&state)?;
    blocking(move || Ok(project_state::list_chats(&chat_dir()?, &root))).await
}

#[tauri::command]
pub async fn agent_history_load(id: String, state: State<'_, AppState>) -> Result<ChatRecord> {
    let root = project_root(&state)?;
    blocking(move || project_state::load_chat(&chat_dir()?, &root, &id)).await
}

/// Save a chat; an empty conversation removes it. Returns its history entry.
#[tauri::command]
pub async fn agent_history_save(
    record: ChatRecord,
    state: State<'_, AppState>,
) -> Result<Option<ChatSummary>> {
    let root = project_root(&state)?;
    blocking(move || project_state::save_chat(&chat_dir()?, &root, record)).await
}

#[tauri::command]
pub async fn agent_history_delete(id: String, state: State<'_, AppState>) -> Result<()> {
    let root = project_root(&state)?;
    blocking(move || project_state::delete_chat(&chat_dir()?, &root, &id)).await
}
