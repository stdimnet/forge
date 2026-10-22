//! Git IPC: thin wrappers over `forge_vcs::Repo`. Every command runs git off the main
//! thread (on the project's host: this machine or the remote agent); mutating commands
//! emit `git://changed` so open views refresh.

use forge_core::{FileKind, ForgeError, Result};
use forge_vcs::{
    Blame, Branches, Commit, CommitDetails, ConflictSet, ConflictSide, LogQuery, MergeFile, Remote,
    RemoteResult, Repo, RepoInfo, RepoStatus, StashEntry,
};
use tauri::{AppHandle, Emitter, State};

use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Event emitted after any operation that changes repository state.
pub const CHANGED_EVENT: &str = "git://changed";

/// Settings | Version Control | Git, "Path to Git executable" (`vcs.git.path`), applied
/// before every git command so a changed path takes effect at once.
pub(crate) fn apply_git_path(state: &AppState) {
    let path = state.read_settings().get_as::<String>("vcs.git.path");
    forge_vcs::set_git_program(path.as_deref());
}

pub(crate) async fn with_repo<T, F>(state: &AppState, f: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&Repo) -> Result<T> + Send + 'static,
{
    apply_git_path(state);
    let backend = state.backend()?;
    tauri::async_runtime::spawn_blocking(move || f(&backend.repo()?))
        .await
        .map_err(|e| ForgeError::Other(format!("git task failed: {e}")))?
}

/// Like [`with_repo`] but notifies the UI afterwards, also on failure (a failed merge or
/// pull may still have changed the working tree).
pub(crate) async fn mutate<T, F>(app: &AppHandle, state: &AppState, f: F) -> Result<T>
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
    apply_git_path(&state);
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
    // Settings | Version Control | Git, "Sign-off commits".
    let sign_off = state
        .read_settings()
        .get_bool("vcs.git.commit.signOff")
        .unwrap_or(false);
    let opts = forge_vcs::CommitOptions {
        amend,
        sign_off,
        signing: crate::commands::git_extras::commit_signing(&state),
        ..forge_vcs::CommitOptions::default()
    };
    let msg = message.clone();
    let res = mutate(&app, &state, move |r| {
        r.commit_opts(&msg, paths.as_deref(), &opts)
    })
    .await;
    if res.is_ok() {
        crate::commands::git_extras::record_commit_message(&state, &message);
    }
    res
}

/// "Warn if CRLF line separators are about to be committed": the files among `paths`
/// (workspace-relative) whose text uses CRLF separators. Empty when git converts line
/// separators itself (`core.autocrlf` is `true` or `input`).
#[tauri::command]
pub async fn git_crlf_files(paths: Vec<String>, state: State<'_, AppState>) -> Result<Vec<String>> {
    apply_git_path(&state);
    let backend = state.backend()?;
    tauri::async_runtime::spawn_blocking(move || {
        let repo = backend.repo()?;
        let autocrlf = repo.config_value("core.autocrlf").unwrap_or_default();
        if matches!(autocrlf.to_ascii_lowercase().as_str(), "true" | "input") {
            return Ok(Vec::new());
        }
        let files: Vec<(String, Vec<u8>)> = paths
            .into_iter()
            .filter_map(|p| backend.read_bytes(&p).ok().map(|bytes| (p, bytes)))
            .collect();
        Ok(forge_text::commit::files_with_crlf(
            files.iter().map(|(p, b)| (p.as_str(), b.as_slice())),
        ))
    })
    .await
    .map_err(|e| ForgeError::Other(format!("git task failed: {e}")))?
}

#[tauri::command]
pub async fn git_push(
    force: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RemoteResult> {
    mutate(&app, &state, move |r| r.push(force.unwrap_or(false))).await
}

/// "Update Project": pull with `rebase`, or by default with the update method of
/// Settings | Version Control | Git (`vcs.git.updateMethod`: merge, rebase or the branch's
/// own configuration).
#[tauri::command]
pub async fn git_pull(
    rebase: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RemoteResult> {
    let rebase = match rebase {
        Some(r) => Some(r),
        None => update_method_rebase(
            state
                .read_settings()
                .get_as::<String>("vcs.git.updateMethod")
                .as_deref(),
        ),
    };
    mutate(&app, &state, move |r| r.pull(rebase)).await
}

/// `--rebase` (`Some(true)`), `--no-rebase` (`Some(false)`) or the branch default (`None`).
fn update_method_rebase(method: Option<&str>) -> Option<bool> {
    match method.unwrap_or("merge") {
        "rebase" => Some(true),
        "branchDefault" => None,
        _ => Some(false),
    }
}

#[tauri::command]
pub async fn git_fetch(app: AppHandle, state: State<'_, AppState>) -> Result<RemoteResult> {
    mutate(&app, &state, |r| r.fetch()).await
}

/// Append files or directories to the repository's `.gitignore`; returns the patterns.
/// Remote projects edit the `.gitignore` at the project root through the backend.
#[tauri::command]
pub async fn git_add_to_gitignore(
    paths: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<String>> {
    if !state.is_remote() {
        return mutate(&app, &state, move |r| r.add_to_gitignore(&paths)).await;
    }
    let backend = state.backend()?;
    let res = blocking(move || {
        let is_dir = |p: &str| {
            let (parent, name) = p.rsplit_once('/').unwrap_or(("", p));
            backend.list_dir(parent).is_ok_and(|entries| {
                entries
                    .iter()
                    .any(|e| e.name == name && e.kind == FileKind::Dir)
            })
        };
        let patterns = forge_vcs::ignore_patterns_for(&paths, is_dir)?;
        let existing = backend.read_optional(".gitignore")?.unwrap_or_default();
        if let Some(text) = forge_vcs::append_ignore_patterns(&existing, &patterns) {
            backend.write_file(".gitignore", &text)?;
        }
        Ok(patterns)
    })
    .await;
    if let Err(e) = app.emit(CHANGED_EVENT, ()) {
        tracing::warn!("could not emit {CHANGED_EVENT}: {e}");
    }
    res
}

#[tauri::command]
pub async fn git_create_tag(
    name: String,
    rev: Option<String>,
    message: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| {
        r.create_tag(&name, rev.as_deref(), message.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn git_stash(
    message: String,
    keep_index: bool,
    include_untracked: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String> {
    mutate(&app, &state, move |r| {
        r.stash(&message, keep_index, include_untracked)
    })
    .await
}

#[tauri::command]
pub async fn git_stash_list(state: State<'_, AppState>) -> Result<Vec<StashEntry>> {
    with_repo(&state, |r| r.stash_list()).await
}

#[tauri::command]
pub async fn git_unstash(
    index: usize,
    pop: bool,
    reinstate_index: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String> {
    mutate(&app, &state, move |r| {
        r.unstash(index, pop, reinstate_index)
    })
    .await
}

#[tauri::command]
pub async fn git_drop_stash(
    index: usize,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| r.drop_stash(index)).await
}

#[tauri::command]
pub async fn git_remotes(state: State<'_, AppState>) -> Result<Vec<Remote>> {
    with_repo(&state, |r| r.remote_list()).await
}

#[tauri::command]
pub async fn git_add_remote(
    name: String,
    url: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| r.add_remote(&name, &url)).await
}

#[tauri::command]
pub async fn git_remove_remote(
    name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| r.remove_remote(&name)).await
}

// ---- merge conflicts --------------------------------------------------------------------

/// The unmerged files and the labels of both sides (Conflicts dialog).
#[tauri::command]
pub async fn git_conflicts(state: State<'_, AppState>) -> Result<ConflictSet> {
    with_repo(&state, |r| r.conflicts()).await
}

/// Base, yours and theirs of a conflicted file plus its three-way merge (merge window).
#[tauri::command]
pub async fn git_merge_file(path: String, state: State<'_, AppState>) -> Result<MergeFile> {
    with_repo(&state, move |r| r.merge_file(&path)).await
}

/// Accept Yours / Accept Theirs (also "Keep File" / "Accept Deletion" and binary files).
#[tauri::command]
pub async fn git_accept_conflict_side(
    paths: Vec<String>,
    side: ConflictSide,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| r.accept_conflict_side(&paths, side)).await
}

/// Apply in the merge window: write the merged `text` (with `\r\n` again when the file
/// used it) through the backend and mark the file resolved (`git add`).
#[tauri::command]
pub async fn git_resolve_merge(
    path: String,
    text: String,
    crlf: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    let backend = state.backend()?;
    let content = forge_vcs::merge::restore_eol(&text, crlf);
    let file = path.clone();
    blocking(move || backend.write_file(&file, &content)).await?;
    mutate(&app, &state, move |r| r.stage(&[path])).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_the_update_method_to_a_pull_mode() {
        assert_eq!(update_method_rebase(None), Some(false));
        assert_eq!(update_method_rebase(Some("merge")), Some(false));
        assert_eq!(update_method_rebase(Some("rebase")), Some(true));
        assert_eq!(update_method_rebase(Some("branchDefault")), None);
    }
}
