//! Workspace and file system IPC. Every call goes to the open project's
//! [`Backend`](forge_backend::Backend): a local folder or a project on a remote host.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use forge_backend::{BackendInfo, WatchGuard};
use forge_core::search::{FileMatches, FindPaged, FindRequest, FindResult};

use forge_core::{FileEntry, FileMatch, ForgeError, FsChanges, Result, Workspace};

use crate::commands::local_history::{self, LocalHistoryState};
use crate::recent::{RecentProject, RecentRemote};
use crate::state::AppState;
use forge_localhistory::Content;

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
            crate::commands::local_history::fs_changed(&handle, &changes);
            crate::commands::db::fs_changed(&handle, &changes);
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
    /// Dev container or WSL distribution (absent for SSH).
    #[serde(skip_serializing_if = "forge_backend::RemoteKind::is_ssh")]
    pub kind: forge_backend::RemoteKind,
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
                kind: r.kind.clone(),
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
    // The project's import style is the refactorings' default.
    crate::commands::imports::import_style(&state);
    crate::commands::local_history::project_opened(app);
    // Excluded folders before the watcher and the indexes look at the project.
    crate::commands::project_structure::apply(app);
    start_watching(app);
    let display = info.display_root();
    match &info.remote {
        None => {
            crate::commands::db::project_opened(app, info.root.clone());
            crate::commands::services::reconfigure(app);
            crate::recent::remember(RecentProject::local(info.name.clone(), display));
        }
        Some(host) => {
            crate::commands::db::remote_project_opened(app);
            crate::commands::services::reconfigure(app);
            crate::recent::remember(RecentProject {
                name: info.name.clone(),
                root: display,
                remote: Some(RecentRemote {
                    host: host.clone(),
                    path: info.root.to_string_lossy().into_owned(),
                }),
                primary: None,
            })
        }
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
                        kind: r.host.kind.clone(),
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
    // Settings | Editor | File Types | Ignored Files and Folders.
    let ignored: String = state
        .read_settings()
        .get_as(IGNORED_FILES_KEY)
        .unwrap_or_else(|| forge_filetypes::DEFAULT_IGNORED_FILES.to_string());
    let mut entries = blocking(move || backend.list_dir(&path)).await?;
    entries.retain(|e| !forge_filetypes::matches_glob_list(&e.name, &ignored));
    Ok(entries)
}

/// The `;`-separated name patterns the Project view hides.
pub const IGNORED_FILES_KEY: &str = "fileTypes.ignoredFiles";

#[tauri::command]
pub async fn fs_read_file(path: String, state: State<'_, AppState>) -> Result<String> {
    let backend = state.backend()?;
    blocking(move || backend.read_file(&path)).await
}

#[tauri::command]
pub async fn fs_write_file(path: String, contents: String, app: AppHandle) -> Result<()> {
    let backend = app.state::<AppState>().backend()?;
    blocking(move || {
        let state = app.state::<AppState>();
        let before = local_history::before_write(&state, &*backend, &path);
        backend.write_file(&path, &contents)?;
        local_history::saved(&app.state(), &path, before, contents.into_bytes());
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn fs_create_file(
    path: String,
    state: State<'_, AppState>,
    lh: State<'_, LocalHistoryState>,
) -> Result<()> {
    let backend = state.backend()?;
    blocking(move || backend.create_file(&path).map(|_| path))
        .await
        .map(|path| local_history::created(&lh, &path, Content::Bytes(Vec::new())))
}

#[tauri::command]
pub async fn fs_create_dir(
    path: String,
    state: State<'_, AppState>,
    lh: State<'_, LocalHistoryState>,
) -> Result<()> {
    let backend = state.backend()?;
    blocking(move || backend.create_dir(&path).map(|_| path))
        .await
        .map(|path| local_history::created(&lh, &path, Content::Dir))
}

#[tauri::command]
pub async fn fs_rename(
    from: String,
    to: String,
    state: State<'_, AppState>,
    lh: State<'_, LocalHistoryState>,
) -> Result<()> {
    let backend = state.backend()?;
    let (f, t) = (from.clone(), to.clone());
    blocking(move || backend.rename(&f, &t)).await?;
    local_history::moved(&lh, &from, &to);
    Ok(())
}

/// Delete a file or a folder with its content (the UI asks for confirmation first). Its
/// content goes into Local History first.
#[tauri::command]
pub async fn fs_delete(path: String, app: AppHandle) -> Result<()> {
    let backend = app.state::<AppState>().backend()?;
    blocking(move || {
        let contents = local_history::before_delete(&app.state(), &*backend, &path);
        backend.delete(&path)?;
        local_history::deleted(&app.state(), &path, contents);
        Ok(())
    })
    .await
}

/// Copy a file or a folder with its content (Project view Copy, F5); never replaces.
#[tauri::command]
pub async fn fs_copy(
    from: String,
    to: String,
    state: State<'_, AppState>,
    lh: State<'_, LocalHistoryState>,
) -> Result<()> {
    let backend = state.backend()?;
    let (f, t) = (from.clone(), to.clone());
    blocking(move || crate::commands::refactor_move::copy_path(&backend, &f, &t)).await?;
    local_history::copied(&lh, &from, &to);
    Ok(())
}

/// "Go to File" backend: fuzzy file-name search over the project, honoring `.gitignore`.
#[tauri::command]
pub async fn fs_find_files(
    query: String,
    limit: Option<usize>,
    exact: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Vec<FileMatch>> {
    let backend = state.backend()?;
    blocking(move || backend.find_files(&query, limit.unwrap_or(100), exact.unwrap_or(false))).await
}

/// Find in Files (Ctrl+Shift+F): text search over the project through its backend, so
/// remote projects are searched on their host (logic in `forge-find`).
#[tauri::command]
pub async fn find_in_files(request: FindRequest, state: State<'_, AppState>) -> Result<FindResult> {
    let backend = state.backend()?;
    blocking(move || backend.find_in_files(&request)).await
}

/// Find window: search once, keep the result and return its totals with the first `page_size`
/// rows. The rest comes from `find_page`, so a result with hundreds of thousands of matches does
/// not cross the IPC boundary in one piece.
#[tauri::command]
pub async fn find_in_files_paged(
    request: FindRequest,
    page_size: usize,
    state: State<'_, AppState>,
) -> Result<FindPaged> {
    let backend = state.backend()?;
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    if let Some(previous) = state
        .find_cancel
        .lock()
        .expect("find cancel lock")
        .replace(cancel.clone())
    {
        previous.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let flag = cancel.clone();
    let result = Arc::new(blocking(move || backend.find_in_files_found(&request, &flag)).await?);
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(ForgeError::Other("Search cancelled".into()));
    }
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let first = result.paged(id, page_size);
    *state.find_results.lock().expect("find results lock") = Some((id, result));
    Ok(first)
}

/// Rows `[offset, offset + count)` of the result `find_in_files_paged` returned `id` for.
#[tauri::command]
pub fn find_page(
    id: u64,
    offset: usize,
    count: usize,
    state: State<'_, AppState>,
) -> Result<Vec<FileMatches>> {
    match state
        .find_results
        .lock()
        .expect("find results lock")
        .as_ref()
    {
        Some((current, result)) if *current == id => Ok(result.page(offset, count)),
        _ => Err(ForgeError::Other(
            "The search result was replaced by a newer search".into(),
        )),
    }
}

/// The lines Replace in Files would change, for the confirmation dialog.
#[tauri::command]
pub async fn replace_preview(
    request: FindRequest,
    with: String,
    state: State<'_, AppState>,
) -> Result<Vec<forge_backend::replace::FilePreview>> {
    let backend = state.backend()?;
    blocking(move || forge_backend::replace::preview(&*backend, &request, &with, 200, 20)).await
}

/// Replace every match of `request` with `with`; the changed files are kept for `undo_replace`.
#[tauri::command]
pub async fn replace_in_files(
    request: FindRequest,
    with: String,
    state: State<'_, AppState>,
    lh: State<'_, LocalHistoryState>,
) -> Result<forge_backend::replace::ReplaceDone> {
    let backend = state.backend()?;
    let done =
        blocking(move || forge_backend::replace::replace_all(&*backend, &request, &with)).await?;
    *state.last_replace.lock().expect("last replace lock") = done.undo.clone();
    local_history::replaced(&lh, "Replace in Files", &done.undo);
    Ok(done)
}

/// What `undo_replace` put back.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoneReplace {
    pub restored: usize,
    /// Files left alone because they changed after the replace.
    pub left: Vec<String>,
}

/// Put back the files the last Replace in Files changed.
#[tauri::command]
pub async fn undo_replace(state: State<'_, AppState>) -> Result<UndoneReplace> {
    let backend = state.backend()?;
    let undo = std::mem::take(&mut *state.last_replace.lock().expect("last replace lock"));
    let (restored, left) = blocking(move || forge_backend::replace::undo(&*backend, &undo)).await?;
    Ok(UndoneReplace { restored, left })
}
