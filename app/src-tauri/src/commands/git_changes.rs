//! Changelists, partial commits and patches (Commit tool window, Git ▸ Patch): thin wrappers
//! over `forge_vcs`. Git and the project files are reached through the backend (local or
//! remote); the changelists live in Forge's per-project data directory, never in the project.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_vcs::patch::{preview_patch, PatchPreviewFile};
use forge_vcs::{
    ApplyPatchResult, ChangeHunk, ChangelistView, Changelists, CommitFile, CommitOptions, Repo,
};
use tauri::{AppHandle, Emitter, State};

use crate::commands::git::{apply_git_path, CHANGED_EVENT};
use crate::commands::workbench::project_state_file;
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Serializes read-modify-write cycles of the changelist store.
static STORE_LOCK: Mutex<()> = Mutex::new(());

/// `<data dir>/forge/projects/<project>/changelists.json`.
fn store_file(state: &AppState) -> Result<PathBuf> {
    let info = state.project_info().ok_or_else(ForgeError::no_project)?;
    project_state_file(&info.name, &info.display_root(), "changelists.json")
        .ok_or_else(|| ForgeError::Other("no data directory on this system".into()))
}

fn load(file: &Path) -> Changelists {
    std::fs::read_to_string(file)
        .map(|t| Changelists::from_json(&t))
        .unwrap_or_default()
}

fn save(file: &Path, cl: &Changelists) -> Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| ForgeError::io(dir, e))?;
    }
    std::fs::write(file, cl.to_json()).map_err(|e| ForgeError::io(file, e))
}

/// Run `f` with the repository and the backend (to read working copies) off the main thread.
async fn with_repo_files<T, F>(state: &AppState, f: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&Repo, &Arc<dyn Backend>) -> Result<T> + Send + 'static,
{
    apply_git_path(state);
    let backend = state.backend()?;
    blocking(move || f(&backend.repo()?, &backend)).await
}

fn changed(app: &AppHandle) {
    if let Err(e) = app.emit(CHANGED_EVENT, ()) {
        tracing::warn!("could not emit {CHANGED_EVENT}: {e}");
    }
}

/// Hunk ids of a file's working copy against HEAD (empty when it cannot be read).
fn hunk_ids(repo: &Repo, backend: &Arc<dyn Backend>, path: &str) -> Vec<String> {
    backend
        .read_optional(path)
        .ok()
        .flatten()
        .and_then(|text| repo.file_hunks(path, &text).ok())
        .map(|h| h.into_iter().map(|h| h.id).collect())
        .unwrap_or_default()
}

/// The changed files that changelists hold: tracked changes without conflicts.
fn changed_paths(repo: &Repo) -> Result<Vec<String>> {
    let mut paths: Vec<String> = repo
        .status()?
        .entries
        .into_iter()
        .filter(|e| !e.conflict && !e.is_untracked() && e.worktree != forge_vcs::FileState::Ignored)
        .map(|e| e.path)
        .collect();
    paths.sort();
    Ok(paths)
}

/// Load the store, follow the repository status, let `op` change it, save, and return the
/// lists with their files.
async fn update<F>(state: &AppState, op: F) -> Result<Vec<ChangelistView>>
where
    F: FnOnce(&mut Changelists, &dyn Fn(&str) -> Vec<String>) -> std::result::Result<(), String>
        + Send
        + 'static,
{
    let file = store_file(state)?;
    with_repo_files(state, move |repo, backend| {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut cl = load(&file);
        let before = cl.clone();
        let paths = changed_paths(repo)?;
        cl.sync(&paths);
        let hunks = |p: &str| hunk_ids(repo, backend, p);
        op(&mut cl, &hunks).map_err(ForgeError::Other)?;
        let with_hunks: Vec<String> = cl.hunks.keys().cloned().collect();
        let mut ids = std::collections::HashMap::new();
        for p in with_hunks {
            let h = hunks(&p);
            cl.normalize_file(&p, &h);
            ids.insert(p, h);
        }
        if cl != before {
            save(&file, &cl)?;
        }
        Ok(cl.view(&paths, &|p| ids.get(p).cloned().unwrap_or_default()))
    })
    .await
}

/// The changelists with their files (Commit tool window).
#[tauri::command]
pub async fn git_changelists(state: State<'_, AppState>) -> Result<Vec<ChangelistView>> {
    update(&state, |_, _| Ok(())).await
}

/// New Changelist….
#[tauri::command]
pub async fn git_changelist_create(
    name: String,
    comment: String,
    active: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ChangelistView>> {
    let res = update(&state, move |cl, _| {
        cl.create(&name, &comment, active).map(drop)
    })
    .await;
    changed(&app);
    res
}

/// Edit Changelist….
#[tauri::command]
pub async fn git_changelist_edit(
    id: String,
    name: String,
    comment: String,
    active: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ChangelistView>> {
    let res = update(&state, move |cl, _| cl.edit(&id, &name, &comment, active)).await;
    changed(&app);
    res
}

/// Set Active Changelist.
#[tauri::command]
pub async fn git_changelist_set_active(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ChangelistView>> {
    let res = update(&state, move |cl, _| cl.set_active(&id)).await;
    changed(&app);
    res
}

/// Delete Changelist (its changes move to the active list).
#[tauri::command]
pub async fn git_changelist_delete(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ChangelistView>> {
    let res = update(&state, move |cl, _| cl.delete(&id)).await;
    changed(&app);
    res
}

/// Move to Another Changelist (F6): whole files, or with `hunks` single hunks of `paths[0]`.
/// `target` is a list id, or with `create` the name of a new list.
#[tauri::command]
pub async fn git_changelist_move(
    paths: Vec<String>,
    hunks: Option<Vec<String>>,
    target: String,
    create: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ChangelistView>> {
    let res = update(&state, move |cl, hunks_of| {
        let target = if create.unwrap_or(false) {
            cl.ensure(&target)?
        } else {
            target
        };
        match (hunks, paths.first()) {
            (Some(ids), Some(path)) => cl.move_hunks(path, &ids, &target, &hunks_of(path)),
            _ => cl.move_files(&paths, &target),
        }
    })
    .await;
    changed(&app);
    res
}

/// The change hunks of a file against HEAD; `content` is an unsaved editor buffer.
#[tauri::command]
pub async fn git_file_hunks(
    path: String,
    content: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<ChangeHunk>> {
    with_repo_files(&state, move |repo, backend| {
        let text = match content {
            Some(c) => c,
            None => backend.read_optional(&path)?.unwrap_or_default(),
        };
        repo.file_hunks(&path, &text)
    })
    .await
}

/// Commit whole files and single hunks with the commit panel's options. Sign-off also
/// follows Settings | Version Control | Git.
#[tauri::command]
pub async fn git_commit_files(
    message: String,
    files: Vec<CommitFile>,
    mut options: CommitOptions,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String> {
    options.sign_off |= state
        .read_settings()
        .get_bool("vcs.git.commit.signOff")
        .unwrap_or(false);
    options.signing = crate::commands::git_extras::commit_signing(&state);
    let msg = message.clone();
    let res = with_repo_files(&state, move |repo, backend| {
        let read = |p: &str| backend.read_file(p);
        repo.commit_files(&msg, &files, &options, &read)
    })
    .await;
    if res.is_ok() {
        crate::commands::git_extras::record_commit_message(&state, &message);
    }
    changed(&app);
    res
}

/// Create Patch from local changes, paths relative to the project root.
#[tauri::command]
pub async fn git_create_patch(
    paths: Vec<String>,
    reverse: bool,
    state: State<'_, AppState>,
) -> Result<String> {
    with_repo_files(&state, move |repo, _| repo.create_patch(&paths, reverse)).await
}

/// A patch file: absolute paths are files on this machine (picked in a file dialog), others
/// are project files.
#[tauri::command]
pub async fn git_read_patch(path: String, state: State<'_, AppState>) -> Result<String> {
    if Path::new(&path).is_absolute() {
        return blocking(move || {
            std::fs::read_to_string(&path).map_err(|e| ForgeError::io(Path::new(&path), e))
        })
        .await;
    }
    let backend = state.backend()?;
    blocking(move || backend.read_file(&path)).await
}

/// Save a created patch: absolute paths on this machine, others in the project.
#[tauri::command]
pub async fn git_write_patch(
    path: String,
    patch: String,
    state: State<'_, AppState>,
) -> Result<()> {
    if Path::new(&path).is_absolute() {
        return blocking(move || {
            std::fs::write(&path, patch).map_err(|e| ForgeError::io(Path::new(&path), e))
        })
        .await;
    }
    let backend = state.backend()?;
    blocking(move || backend.write_file(&path, &patch)).await
}

/// Apply Patch preview: the files the patch changes under `base`, before and after.
#[tauri::command]
pub async fn git_patch_preview(
    patch: String,
    base: String,
    state: State<'_, AppState>,
) -> Result<Vec<PatchPreviewFile>> {
    let backend = state.backend()?;
    blocking(move || {
        let read = |p: &str| backend.read_optional(p).ok().flatten();
        Ok(preview_patch(&patch, &base, &read))
    })
    .await
}

/// Apply Patch: apply what fits, merge the rest three-way (conflicts go to the resolver).
#[tauri::command]
pub async fn git_apply_patch(
    patch: String,
    base: String,
    paths: Option<Vec<String>>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyPatchResult> {
    let res = with_repo_files(&state, move |repo, _| {
        repo.apply_patch(&patch, &base, paths.as_deref())
    })
    .await;
    changed(&app);
    res
}
