//! Git IPC: thin wrappers over `forge_vcs::Repo`. Every command runs git off the main
//! thread (on the project's host: this machine or the remote agent); mutating commands
//! emit `git://changed` so open views refresh.

use forge_core::{ForgeError, Result};
use forge_vcs::{
    Blame, Branches, Commit, CommitDetails, LogQuery, RemoteResult, Repo, RepoInfo, RepoStatus,
};
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;

/// Event emitted after any operation that changes repository state.
pub const CHANGED_EVENT: &str = "git://changed";

async fn with_repo<T, F>(state: &AppState, f: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&Repo) -> Result<T> + Send + 'static,
{
    let backend = state.backend()?;
    tauri::async_runtime::spawn_blocking(move || f(&backend.repo()?))
        .await
        .map_err(|e| ForgeError::Other(format!("git task failed: {e}")))?
}

/// Like [`with_repo`] but notifies the UI afterwards, also on failure (a failed merge or
/// pull may still have changed the working tree).
async fn mutate<T, F>(app: &AppHandle, state: &AppState, f: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&Repo) -> Result<T> + Send + 'static,
{
    let res = with_repo(state, f).await;
    if let Err(e) = app.emit(CHANGED_EVENT, ()) {
        tracing::warn!("could not emit {CHANGED_EVENT}: {e}");
    }
    res
}

/// Repository info, or `null` when the project is not under git.
#[tauri::command]
pub async fn git_info(state: State<'_, AppState>) -> Result<Option<RepoInfo>> {
    let backend = state.backend()?;
    tauri::async_runtime::spawn_blocking(move || match backend.repo() {
        Ok(repo) => repo.info().map(Some),
        Err(_) => Ok(None),
    })
    .await
    .map_err(|e| ForgeError::Other(e.to_string()))?
}

#[tauri::command]
pub async fn git_status(state: State<'_, AppState>) -> Result<RepoStatus> {
    with_repo(&state, |r| r.status()).await
}

/// File content at a revision (`HEAD`, a sha, or `""` for the index).
#[tauri::command]
pub async fn git_show(
    rev: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Option<String>> {
    with_repo(&state, move |r| r.show(&rev, &path)).await
}

/// Unified diff; `staged` = index vs HEAD, otherwise working tree vs index.
#[tauri::command]
pub async fn git_diff(
    paths: Vec<String>,
    staged: bool,
    state: State<'_, AppState>,
) -> Result<String> {
    with_repo(&state, move |r| r.diff_text(&paths, staged)).await
}

/// Unified diff of working tree vs HEAD for the given paths.
#[tauri::command]
pub async fn git_diff_head(paths: Vec<String>, state: State<'_, AppState>) -> Result<String> {
    with_repo(&state, move |r| r.diff_head_text(&paths)).await
}

#[tauri::command]
pub async fn git_blame(
    path: String,
    contents: Option<String>,
    state: State<'_, AppState>,
) -> Result<Blame> {
    with_repo(&state, move |r| r.blame(&path, contents.as_deref())).await
}

#[tauri::command]
pub async fn git_log(query: LogQuery, state: State<'_, AppState>) -> Result<Vec<Commit>> {
    with_repo(&state, move |r| r.log(&query)).await
}

#[tauri::command]
pub async fn git_commit_details(sha: String, state: State<'_, AppState>) -> Result<CommitDetails> {
    with_repo(&state, move |r| r.commit_details(&sha)).await
}

#[tauri::command]
pub async fn git_branches(state: State<'_, AppState>) -> Result<Branches> {
    with_repo(&state, |r| r.branches()).await
}

/// Check out a branch, tag or revision; `remote` checks out a remote branch as a local
/// tracking branch.
#[tauri::command]
pub async fn git_checkout(
    rev: String,
    remote: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| {
        if remote.unwrap_or(false) {
            r.checkout_remote(&rev)
        } else {
            r.checkout(&rev)
        }
    })
    .await
}

#[tauri::command]
pub async fn git_create_branch(
    name: String,
    start: Option<String>,
    checkout: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| {
        r.create_branch(&name, start.as_deref(), checkout)
    })
    .await
}

#[tauri::command]
pub async fn git_delete_branch(
    name: String,
    force: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| {
        r.delete_branch(&name, force.unwrap_or(false))
    })
    .await
}

#[tauri::command]
pub async fn git_merge(rev: String, app: AppHandle, state: State<'_, AppState>) -> Result<String> {
    mutate(&app, &state, move |r| r.merge(&rev)).await
}

#[tauri::command]
pub async fn git_rebase(
    onto: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String> {
    mutate(&app, &state, move |r| r.rebase(&onto)).await
}

#[tauri::command]
pub async fn git_stage(
    paths: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| r.stage(&paths)).await
}

#[tauri::command]
pub async fn git_unstage(
    paths: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| r.unstage(&paths)).await
}

#[tauri::command]
pub async fn git_rollback(
    paths: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| r.rollback(&paths)).await
}

/// Commit and return the new HEAD sha. `paths` selects the files to commit (the checked
/// files of the Commit tool window); `null` commits the index.
#[tauri::command]
pub async fn git_commit(
    message: String,
    amend: bool,
    paths: Option<Vec<String>>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String> {
    mutate(&app, &state, move |r| {
        r.commit(&message, amend, paths.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn git_push(
    force: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RemoteResult> {
    mutate(&app, &state, move |r| r.push(force.unwrap_or(false))).await
}

#[tauri::command]
pub async fn git_pull(
    rebase: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RemoteResult> {
    mutate(&app, &state, move |r| r.pull(rebase.unwrap_or(false))).await
}

#[tauri::command]
pub async fn git_fetch(app: AppHandle, state: State<'_, AppState>) -> Result<RemoteResult> {
    mutate(&app, &state, |r| r.fetch()).await
}
