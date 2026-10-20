use serde::Serialize;
use tauri::{AppHandle, State};

use forge_core::{FileEntry, FileMatch, Result, Workspace};

use crate::state::AppState;

#[derive(Serialize)]
pub struct WorkspaceInfo {
    pub name: String,
    pub root: String,
}

#[tauri::command]
pub fn workspace_open(
    path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WorkspaceInfo> {
    let ws = Workspace::open(&path)?;
    let root = ws.root().to_path_buf();
    let info = WorkspaceInfo {
        name: ws.name(),
        root: root.display().to_string(),
    };
    state.set_workspace(ws);
    // After `set_workspace`: detection also registers the project's language servers.
    crate::commands::project::emit_detection(&app, &root);
    crate::recent::remember(crate::recent::RecentProject {
        name: info.name.clone(),
        root: info.root.clone(),
    });
    Ok(info)
}

/// The project opened at startup (command line argument), if any.
#[tauri::command]
pub fn workspace_current(state: State<'_, AppState>) -> Option<WorkspaceInfo> {
    state
        .with_workspace(|ws| {
            Ok(WorkspaceInfo {
                name: ws.name(),
                root: ws.root().display().to_string(),
            })
        })
        .ok()
}

/// A recent project with its detected framework or language for the Welcome screen badge.
#[derive(Serialize)]
pub struct RecentProjectInfo {
    pub name: String,
    pub root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<forge_project::Framework>,
}

/// Recently opened projects that still exist on disk.
#[tauri::command]
pub async fn workspace_recent() -> Vec<RecentProjectInfo> {
    tauri::async_runtime::spawn_blocking(|| {
        crate::recent::load()
            .into_iter()
            .filter(|p| std::path::Path::new(&p.root).is_dir())
            .map(|p| RecentProjectInfo {
                primary: forge_project::detect(std::path::Path::new(&p.root)).primary,
                name: p.name,
                root: p.root,
            })
            .collect()
    })
    .await
    .unwrap_or_default()
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
