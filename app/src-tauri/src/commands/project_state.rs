//! Per-project state outside the project (`forge_core::project_state`): the workbench
//! layout (user cache dir) and the AI chat history (user data dir).

use tauri::State;

use forge_core::project_state::{
    self, ChatRecord, ChatSummary, NamedLayout, ProjectLayout, LAYOUTS_SETTING,
};
use forge_core::{ForgeError, Result, SettingsScope};

use super::workspace::blocking;
use crate::state::AppState;

/// The open project's root as the UI identifies it (`user@host:/path` for remote ones).
fn project_root(state: &AppState) -> Result<String> {
    state
        .project_info()
        .map(|i| i.display_root())
        .ok_or_else(|| ForgeError::no_project())
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

/// The layouts saved with Window | Layouts | Save Current Layout as New… (user setting
/// `workbench.layouts`), in the order they were saved.
#[tauri::command]
pub fn layouts_list(state: State<'_, AppState>) -> Vec<NamedLayout> {
    named_layouts(&state)
}

fn named_layouts(state: &AppState) -> Vec<NamedLayout> {
    project_state::named_layouts(state.read_settings().get(LAYOUTS_SETTING))
}

fn write_layouts(state: &AppState, layouts: &[NamedLayout]) -> Result<()> {
    state.write_settings().set(
        LAYOUTS_SETTING,
        project_state::named_layouts_value(layouts),
        SettingsScope::User,
    )
}

/// Save the tool window part of `layout` as `name`; with `replace` an existing layout of
/// that name is overwritten, otherwise that is an error.
#[tauri::command]
pub fn layouts_save(
    name: String,
    layout: ProjectLayout,
    replace: bool,
    state: State<'_, AppState>,
) -> Result<NamedLayout> {
    let mut layouts = named_layouts(&state);
    let saved = project_state::save_named_layout(&mut layouts, &name, &layout, replace)?;
    write_layouts(&state, &layouts)?;
    Ok(saved)
}

/// Window | Layouts | Delete Layout.
#[tauri::command]
pub fn layouts_delete(name: String, state: State<'_, AppState>) -> Result<bool> {
    let mut layouts = named_layouts(&state);
    if !project_state::delete_named_layout(&mut layouts, &name) {
        return Ok(false);
    }
    write_layouts(&state, &layouts)?;
    Ok(true)
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
