//! Workspace and file system IPC. Every call goes to the open project's
//! [`Backend`](forge_backend::Backend): a local folder or a project on a remote host.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use forge_backend::{BackendInfo, WatchGuard};
use forge_core::{FileEntry, FileMatch, ForgeError, FsChanges, Result, Workspace};

use crate::recent::{RecentProject, RecentRemote};
use crate::state::AppState;

/// Emitted with [`FsChanges`] when files change outside the editor.
pub const FS_CHANGED_EVENT: &str = "fs://changed";

/// The file watcher of the open project.
#[derive(Default)]
pub struct WatchState(Mutex<Option<WatchGuard>>);

/// Watch the open project (replacing the previous project's watcher): changes go to the UI
/// as `fs://changed`; the backend already updated the PHP index.
pub fn start_watching(app: &AppHandle) {
    let state = app.state::<WatchState>();
    *state.0.lock().expect("watch lock") = None; // stop the old watcher first
    let Ok(backend) = app.state::<AppState>().backend() else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let handle = app.clone();
        let watcher = backend.watch(Arc::new(move |changes: FsChanges| {
            if let Err(e) = handle.emit(FS_CHANGED_EVENT, &changes) {
                tracing::warn!("cannot emit {FS_CHANGED_EVENT}: {e}");
            }
        }));
        match watcher {
            Ok(w) => *app.state::<WatchState>().0.lock().expect("watch lock") = Some(w),
            Err(e) => tracing::warn!("{e}"),
        }
    });
}

/// A remote project's host, for the UI.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInfo {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// `user@host` (with `:port` when not 22).
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_id: Option<String>,
    /// The project folder on the host.
    pub path: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
    pub name: String,
    /// Local path, or `user@host:/path` for a remote project. Identifies the project.
    pub root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<RemoteInfo>,
}

impl From<&BackendInfo> for WorkspaceInfo {
    fn from(info: &BackendInfo) -> Self {
        WorkspaceInfo {
            name: info.name.clone(),
            root: info.display_root(),
            remote: info.remote.as_ref().map(|r| RemoteInfo {
                host: r.host.clone(),
                port: r.port,
                user: r.user.clone(),
                label: r.label(),
                config_id: r.config_id.clone(),
                path: info.root.to_string_lossy().into_owned(),
            }),
        }
    }
}

/// Everything that follows opening a project (local or remote): detection (which also
/// registers language servers), the file watcher, data sources and the recent list.
pub fn project_opened(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Some(info) = state.project_info() else {
        return;
    };
    crate::commands::project::emit_detection(app);
    start_watching(app);
    let display = info.display_root();
    match &info.remote {
        None => {
            crate::commands::db::project_opened(app, info.root.clone());
            crate::commands::services::reconfigure(app);
            crate::recent::remember(RecentProject::local(info.name.clone(), display));
        }
        Some(host) => crate::recent::remember(RecentProject {
            name: info.name.clone(),
            root: display,
            remote: Some(RecentRemote {
                host: host.clone(),
                path: info.root.to_string_lossy().into_owned(),
            }),
            primary: None,
        }),
    }
}

#[tauri::command]
pub fn workspace_open(
    path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WorkspaceInfo> {
    let ws = Workspace::open(&path)?;
    crate::commands::remote::close_project_connection(&app);
    state.set_workspace(ws);
    let info = state
        .project_info()
        .ok_or_else(|| ForgeError::Other(format!("cannot open {path}")))?;
    project_opened(&app);
    Ok(WorkspaceInfo::from(&info))
}

/// The open project (e.g. passed on the command line), if any.
#[tauri::command]
pub fn workspace_current(state: State<'_, AppState>) -> Option<WorkspaceInfo> {
    state.project_info().as_ref().map(WorkspaceInfo::from)
}

/// A recent project with its detected framework or language for the Welcome screen badge.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProjectInfo {
    pub name: String,
    pub root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<forge_project::Framework>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<RemoteInfo>,
}

/// Recently opened projects: local ones that still exist on disk, and remote ones.
#[tauri::command]
pub async fn workspace_recent() -> Vec<RecentProjectInfo> {
    tauri::async_runtime::spawn_blocking(|| {
        crate::recent::load()
            .into_iter()
            .filter_map(|p| match p.remote {
                Some(r) => Some(RecentProjectInfo {
                    remote: Some(RemoteInfo {
                        label: r.host.label(),
                        host: r.host.host,
                        port: r.host.port,
                        user: r.host.user,
                        config_id: r.host.config_id,
                        path: r.path,
                    }),
                    primary: p.primary,
                    name: p.name,
                    root: p.root,
                }),
                None => std::path::Path::new(&p.root)
                    .is_dir()
                    .then(|| RecentProjectInfo {
                        primary: forge_project::detect(std::path::Path::new(&p.root)).primary,
                        name: p.name,
                        root: p.root,
                        remote: None,
                    }),
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// Run blocking backend work off the main thread.
pub async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| ForgeError::Other(format!("task failed: {e}")))?
}

#[tauri::command]
pub async fn fs_list_dir(path: String, state: State<'_, AppState>) -> Result<Vec<FileEntry>> {
    let backend = state.backend()?;
    blocking(move || backend.list_dir(&path)).await
}

#[tauri::command]
pub async fn fs_read_file(path: String, state: State<'_, AppState>) -> Result<String> {
    let backend = state.backend()?;
    blocking(move || backend.read_file(&path)).await
}

#[tauri::command]
pub async fn fs_write_file(
    path: String,
    contents: String,
    state: State<'_, AppState>,
) -> Result<()> {
    let backend = state.backend()?;
    blocking(move || backend.write_file(&path, &contents)).await
}

#[tauri::command]
pub fn fs_create_file(path: String, state: State<'_, AppState>) -> Result<()> {
    state.with_workspace(|ws| ws.create_file(&path))
}

#[tauri::command]
pub fn fs_create_dir(path: String, state: State<'_, AppState>) -> Result<()> {
    state.with_workspace(|ws| ws.create_dir(&path))
}

#[tauri::command]
pub fn fs_rename(from: String, to: String, state: State<'_, AppState>) -> Result<()> {
    state.with_workspace(|ws| ws.rename(&from, &to))
}

/// Delete a file or a folder with its content (the UI asks for confirmation first).
#[tauri::command]
pub fn fs_delete(path: String, state: State<'_, AppState>) -> Result<()> {
    state.with_workspace(|ws| ws.delete(&path))
}

/// "Go to File" backend: fuzzy file-name search over the project, honoring `.gitignore`.
#[tauri::command]
pub async fn fs_find_files(
    query: String,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<FileMatch>> {
    let backend = state.backend()?;
    blocking(move || backend.find_files(&query, limit.unwrap_or(100))).await
}
