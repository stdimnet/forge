//! Git Log context menu IPC: which actions a selection allows, Cherry-Pick, Revert Commit,
//! Undo Commit, Edit Commit Message, Squash / Drop Commits, the interactive rebase, Create
//! Patch and Show Repository at Revision. Thin wrappers over `forge_vcs::Repo`; git runs on
//! the project's host.

use forge_core::Result;
use forge_vcs::rebase::parse_protected;
use forge_vcs::{LogActionStates, RebaseEntry, RebasePlan, RewriteResult, UndoneCommit};
use tauri::{AppHandle, State};

use crate::commands::git::{mutate, with_repo};
use crate::state::AppState;

/// Settings | Version Control | Git, "Protected branches".
fn protected_branches(state: &AppState) -> Vec<String> {
    let value = state
        .read_settings()
        .get_as::<String>("vcs.git.protectedBranches")
        .unwrap_or_else(|| "master;main".into());
    parse_protected(&value)
}

/// The Log actions the selected commits (log order) allow, with the reasons of disabled ones.
#[tauri::command]
pub async fn git_log_actions(
    shas: Vec<String>,
    state: State<'_, AppState>,
) -> Result<LogActionStates> {
    let protected = protected_branches(&state);
    with_repo(&state, move |r| r.log_action_states(&shas, &protected)).await
}

/// Cherry-Pick (log order, newest first). "Add the 'cherry-picked from' suffix" applies to
/// commits pushed to protected branches.
#[tauri::command]
pub async fn git_cherry_pick(
    shas: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String> {
    let protected = protected_branches(&state);
    let suffix = state
        .read_settings()
        .get_as::<bool>("vcs.git.cherryPickSuffix")
        .unwrap_or(true);
    mutate(&app, &state, move |r| {
        r.cherry_pick(&shas, suffix, &protected)
    })
    .await
}

#[tauri::command]
pub async fn git_revert_commits(
    shas: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String> {
    mutate(&app, &state, move |r| r.revert_commits(&shas)).await
}

#[tauri::command]
pub async fn git_undo_commit(
    sha: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<UndoneCommit> {
    mutate(&app, &state, move |r| r.undo_commit(&sha)).await
}

/// Undo of Edit Commit Message / Squash / Drop / the interactive rebase.
#[tauri::command]
pub async fn git_undo_rewrite(
    old_head: String,
    new_head: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| r.undo_rewrite(&old_head, &new_head)).await
}

/// Edit Commit Message….
#[tauri::command]
pub async fn git_reword(
    sha: String,
    message: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RewriteResult> {
    mutate(&app, &state, move |r| r.reword(&sha, &message)).await
}

/// Squash Commits….
#[tauri::command]
pub async fn git_squash_commits(
    shas: Vec<String>,
    message: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RewriteResult> {
    mutate(&app, &state, move |r| r.squash_commits(&shas, &message)).await
}

/// Drop Commits.
#[tauri::command]
pub async fn git_drop_commits(
    shas: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RewriteResult> {
    mutate(&app, &state, move |r| r.drop_commits(&shas)).await
}

/// Interactively Rebase from Here…: the dialog's commits and initial actions.
#[tauri::command]
pub async fn git_rebase_plan(sha: String, state: State<'_, AppState>) -> Result<RebasePlan> {
    with_repo(&state, move |r| r.rebase_plan(&sha)).await
}

/// Start Rebasing with the dialog's plan (oldest first).
#[tauri::command]
pub async fn git_interactive_rebase(
    sha: String,
    entries: Vec<RebaseEntry>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RewriteResult> {
    mutate(&app, &state, move |r| r.interactive_rebase(&sha, &entries)).await
}

/// Create Patch…: the unified diff of the selected commits.
#[tauri::command]
pub async fn git_commits_patch(shas: Vec<String>, state: State<'_, AppState>) -> Result<String> {
    with_repo(&state, move |r| r.commits_patch(&shas)).await
}

/// Show Repository at Revision: the workspace files in `rev`.
#[tauri::command]
pub async fn git_tree_files(rev: String, state: State<'_, AppState>) -> Result<Vec<String>> {
    with_repo(&state, move |r| r.tree_files(&rev)).await
}
