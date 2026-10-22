//! Git menu dialogs (Pull, Merge, Rebase, Reset HEAD, New Tag, Manage Remotes, Unstash) and
//! the shelf: thin wrappers over `forge_vcs::Repo`. Git runs on the project's host; the shelf
//! lives in Forge's per-project data directory, never inside the project.

use std::path::PathBuf;

use forge_core::{ForgeError, Result};
use forge_vcs::{
    ChangedFile, MergeOptions, OperationAction, OperationState, PullOptions, RebaseOptions,
    RemoteResult, ResetMode, Shelf, ShelfFileContents, ShelvedChangelist, StashFileContents,
    TagOptions,
};
use tauri::{AppHandle, State};

use crate::commands::git::{mutate, with_repo};
use crate::commands::workbench::project_state_file;
use crate::state::AppState;

/// `<data dir>/forge/projects/<project>/shelf` (PHPStorm keeps it in `.idea/shelf`).
fn shelf(state: &AppState) -> Result<Shelf> {
    let info = state
        .project_info()
        .ok_or_else(|| ForgeError::Other("no project is open".into()))?;
    let dir: PathBuf = project_state_file(&info.name, &info.display_root(), "shelf")
        .ok_or_else(|| ForgeError::Other("no data directory on this system".into()))?;
    Ok(Shelf::new(dir))
}

/// Git ▸ Pull… with the dialog's remote, branches and options.
#[tauri::command]
pub async fn git_pull_with(
    options: PullOptions,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RemoteResult> {
    mutate(&app, &state, move |r| r.pull_with(&options)).await
}

/// Git ▸ Merge… with the dialog's branches and options.
#[tauri::command]
pub async fn git_merge_with(
    options: MergeOptions,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String> {
    mutate(&app, &state, move |r| r.merge_with(&options)).await
}

/// Git ▸ Rebase… with the dialog's onto/from, branch and options.
#[tauri::command]
pub async fn git_rebase_with(
    options: RebaseOptions,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String> {
    mutate(&app, &state, move |r| r.rebase_with(&options)).await
}

/// The rebase, merge, cherry-pick or revert in progress (branch widget, Git menu).
#[tauri::command]
pub async fn git_operation_state(state: State<'_, AppState>) -> Result<OperationState> {
    with_repo(&state, |r| r.operation_state()).await
}

/// Continue / Abort / Skip the operation in progress.
#[tauri::command]
pub async fn git_operation_action(
    action: OperationAction,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String> {
    mutate(&app, &state, move |r| r.operation_action(action)).await
}

/// Git ▸ Reset HEAD….
#[tauri::command]
pub async fn git_reset_head(
    rev: String,
    mode: ResetMode,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| r.reset_head(&rev, mode)).await
}

/// Git ▸ New Tag… with force, commit and message.
#[tauri::command]
pub async fn git_tag_with(
    options: TagOptions,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| r.tag(&options)).await
}

/// The full hash of a commit, or `null` (Validate in the Tag and Reset dialogs).
#[tauri::command]
pub async fn git_resolve_commit(rev: String, state: State<'_, AppState>) -> Result<Option<String>> {
    with_repo(&state, move |r| Ok(r.resolve_commit(&rev))).await
}

/// Manage Remotes ▸ Edit: rename a remote and/or change its URL.
#[tauri::command]
pub async fn git_edit_remote(
    old_name: String,
    name: String,
    url: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| r.edit_remote(&old_name, &name, &url)).await
}

/// Files of a stash (Unstash ▸ View, the Stash tab).
#[tauri::command]
pub async fn git_stash_files(index: usize, state: State<'_, AppState>) -> Result<Vec<ChangedFile>> {
    with_repo(&state, move |r| r.stash_files(index)).await
}

/// A file of a stash before and after the stashed change.
#[tauri::command]
pub async fn git_stash_file(
    index: usize,
    path: String,
    state: State<'_, AppState>,
) -> Result<StashFileContents> {
    with_repo(&state, move |r| r.stash_file(index, &path)).await
}

/// Unstash ▸ Clear.
#[tauri::command]
pub async fn git_stash_clear(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    mutate(&app, &state, |r| r.stash_clear()).await
}

/// Unstash "As new branch".
#[tauri::command]
pub async fn git_stash_branch(
    index: usize,
    branch: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String> {
    mutate(&app, &state, move |r| r.stash_branch(index, &branch)).await
}

// ---- shelf ----------------------------------------------------------------------------------

#[tauri::command]
pub async fn git_shelf_list(state: State<'_, AppState>) -> Result<Vec<ShelvedChangelist>> {
    let shelf = shelf(&state)?;
    with_repo(&state, move |r| Ok(r.shelf_list(&shelf))).await
}

/// Shelve Changes: store the changes of `paths` and roll them back.
#[tauri::command]
pub async fn git_shelve(
    paths: Vec<String>,
    name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ShelvedChangelist> {
    let shelf = shelf(&state)?;
    mutate(&app, &state, move |r| r.shelve(&shelf, &paths, &name)).await
}

/// Unshelve the changelist, or only `paths` of it; `remove` drops the applied files from
/// the shelf.
#[tauri::command]
pub async fn git_unshelve(
    id: String,
    paths: Option<Vec<String>>,
    remove: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String> {
    let shelf = shelf(&state)?;
    mutate(&app, &state, move |r| {
        r.unshelve(&shelf, &id, paths.as_deref(), remove)
    })
    .await
}

#[tauri::command]
pub async fn git_shelf_delete(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    let shelf = shelf(&state)?;
    mutate(&app, &state, move |_| shelf.delete(&id)).await
}

#[tauri::command]
pub async fn git_shelf_rename(
    id: String,
    name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ShelvedChangelist> {
    let shelf = shelf(&state)?;
    mutate(&app, &state, move |r| {
        shelf.rename(&id, &name).map(|cl| {
            r.shelf_list(&shelf)
                .into_iter()
                .find(|c| c.id == cl.id)
                .unwrap_or(cl)
        })
    })
    .await
}

/// A shelved file before and after its change (Show Diff in the Shelf tab).
#[tauri::command]
pub async fn git_shelf_file(
    id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<ShelfFileContents> {
    let shelf = shelf(&state)?;
    with_repo(&state, move |r| r.shelf_file(&shelf, &id, &path)).await
}
