//! Git history IPC: Show History of a file or folder, Show History for Selection,
//! Annotate Revision and the changes of a folder since a branch (Compare with Branch).

use forge_core::Result;
use forge_vcs::{Blame, BlockRevision, ChangedFile, FileRevision, HistoryQuery};
use tauri::State;

use crate::commands::git::with_repo;
use crate::state::AppState;

#[tauri::command]
pub async fn git_file_history(
    path: String,
    dir: bool,
    query: HistoryQuery,
    state: State<'_, AppState>,
) -> Result<Vec<FileRevision>> {
    with_repo(&state, move |r| r.file_history(&path, dir, &query)).await
}

/// Lines `start..=end` (1-based) of `path`.
#[tauri::command]
pub async fn git_block_history(
    path: String,
    start: u32,
    end: u32,
    state: State<'_, AppState>,
) -> Result<Vec<BlockRevision>> {
    with_repo(&state, move |r| r.block_history(&path, start, end, 0)).await
}

#[tauri::command]
pub async fn git_blame_revision(
    rev: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Blame> {
    with_repo(&state, move |r| r.blame_at(&rev, &path)).await
}

#[tauri::command]
pub async fn git_changes_since(
    rev: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<ChangedFile>> {
    with_repo(&state, move |r| r.changes_since(&rev, &path)).await
}
