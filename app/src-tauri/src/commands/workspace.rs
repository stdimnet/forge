use serde::Serialize;
use tauri::State;

use forge_core::{FileEntry, FileMatch, Result, Workspace};

use crate::state::AppState;

#[derive(Serialize)]
pub struct WorkspaceInfo {
    pub name: String,
    pub root: String,
}

#[tauri::command]
pub fn workspace_open(path: String, state: State<'_, AppState>) -> Result<WorkspaceInfo> {
    let ws = Workspace::open(&path)?;
    let info = WorkspaceInfo {
        name: ws.name(),
        root: ws.root().display().to_string(),
    };
    state.set_workspace(ws);
    Ok(info)
}

#[tauri::command]
pub fn fs_list_dir(path: String, state: State<'_, AppState>) -> Result<Vec<FileEntry>> {
    state.with_workspace(|ws| ws.list_dir(&path))
}

#[tauri::command]
pub fn fs_read_file(path: String, state: State<'_, AppState>) -> Result<String> {
    state.with_workspace(|ws| ws.read_file(&path))
}

#[tauri::command]
pub fn fs_write_file(path: String, contents: String, state: State<'_, AppState>) -> Result<()> {
    state.with_workspace(|ws| ws.write_file(&path, &contents))
}

/// "Go to File" backend: fuzzy file-name search over the project, honoring `.gitignore`.
#[tauri::command]
pub fn fs_find_files(
    query: String,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<FileMatch>> {
    state.with_workspace(|ws| ws.find_files(&query, limit.unwrap_or(100)))
}
