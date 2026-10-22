//! Worktrees (Git tool window › Worktrees), Commit Message History (`Ctrl+M`) and commit
//! signing (Settings | Version Control | Git › "Sign commits with GPG/SSH key"). Git runs
//! through the project's backend, so all of it works for remote projects; the message
//! history lives in Forge's per-project data directory and git config is only read.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_backend::process_io::capture;
use forge_backend::{Backend, LocalBackend};
use forge_core::{ForgeError, Result};
use forge_process::CommandLine;
use forge_vcs::message_history::{MessageHistory, DEFAULT_LIMIT};
use forge_vcs::signing::{
    parse_gpg_secret_keys, parse_ssh_key_listing, parse_ssh_public_key, SSH_KEY_LISTING_SCRIPT,
};
use forge_vcs::{GitSigningConfig, GpgKey, NewWorktree, Signing, SigningKey, SshKey, Worktree};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::commands::git::{mutate, with_repo};
use crate::commands::workbench::project_state_file;
use crate::commands::workspace::blocking;
use crate::state::AppState;

// ---- worktrees ------------------------------------------------------------------------

#[tauri::command]
pub async fn git_worktrees(state: State<'_, AppState>) -> Result<Vec<Worktree>> {
    with_repo(&state, |r| r.worktrees()).await
}

/// "New Worktree…". Returns git's report.
#[tauri::command]
pub async fn git_worktree_add(
    options: NewWorktree,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String> {
    mutate(&app, &state, move |r| r.add_worktree(&options)).await
}

#[tauri::command]
pub async fn git_worktree_remove(
    path: String,
    force: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| r.remove_worktree(&path, force)).await
}

#[tauri::command]
pub async fn git_worktree_prune(app: AppHandle, state: State<'_, AppState>) -> Result<String> {
    mutate(&app, &state, |r| r.prune_worktrees()).await
}

#[tauri::command]
pub async fn git_worktree_lock(
    path: String,
    reason: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| {
        r.lock_worktree(&path, reason.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn git_worktree_unlock(
    path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<()> {
    mutate(&app, &state, move |r| r.unlock_worktree(&path)).await
}

// ---- commit message history -----------------------------------------------------------

/// Serializes read-modify-write cycles of the history file.
static HISTORY_LOCK: Mutex<()> = Mutex::new(());

/// `<data dir>/forge/projects/<project>/commit-messages.json`.
fn history_file(state: &AppState) -> Option<PathBuf> {
    let info = state.project_info()?;
    project_state_file(&info.name, &info.display_root(), "commit-messages.json")
}

fn history_limit(state: &AppState) -> usize {
    state
        .read_settings()
        .get_i64("vcs.commit.messageHistorySize")
        .map(|n| n.clamp(1, 500) as usize)
        .unwrap_or(DEFAULT_LIMIT)
}

fn load_history(file: &PathBuf) -> MessageHistory {
    std::fs::read_to_string(file)
        .map(|t| MessageHistory::from_json(&t))
        .unwrap_or_default()
}

/// Remember a committed message (called after every successful commit from Forge).
pub(crate) fn record_commit_message(state: &AppState, message: &str) {
    let Some(file) = history_file(state) else {
        return;
    };
    let limit = history_limit(state);
    let _guard = HISTORY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut history = load_history(&file);
    history.record(message, limit);
    let saved = file
        .parent()
        .map(std::fs::create_dir_all)
        .transpose()
        .and_then(|_| std::fs::write(&file, history.to_json()));
    if let Err(e) = saved {
        tracing::warn!("cannot save the commit message history: {e}");
    }
}

/// Commit Message History: this project's messages from Forge, then the user's own recent
/// commits, newest first.
#[tauri::command]
pub async fn git_message_history(state: State<'_, AppState>) -> Result<Vec<String>> {
    let limit = history_limit(&state);
    let history = history_file(&state)
        .map(|f| load_history(&f))
        .unwrap_or_default();
    let own = with_repo(&state, move |r| r.own_commit_messages(limit))
        .await
        .unwrap_or_default();
    Ok(history.merged(&own, limit))
}

// ---- signing --------------------------------------------------------------------------

/// The signing options of the next commit, from the settings.
pub(crate) fn commit_signing(state: &AppState) -> Signing {
    let settings = state.read_settings();
    Signing {
        sign: settings.get_bool("vcs.git.commit.gpgSign").unwrap_or(false),
        key: settings
            .get_str("vcs.git.commit.signingKey")
            .and_then(SigningKey::from_setting),
    }
}

/// What the Configure GPG Key dialog shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SigningInfo {
    /// The setup in git config (read-only).
    pub config: GitSigningConfig,
    pub gpg_keys: Vec<GpgKey>,
    /// Why no GPG keys could be listed (gpg missing, …).
    pub gpg_error: Option<String>,
    pub ssh_keys: Vec<SshKey>,
    /// Keys are listed on the remote host.
    pub remote: bool,
}

fn list_gpg_keys(backend: &dyn Backend) -> std::result::Result<Vec<GpgKey>, String> {
    let cmd = CommandLine::new("gpg").args(["--list-secret-keys", "--with-colons"]);
    let out = capture(backend, &cmd, "", Duration::from_secs(15)).map_err(|e| {
        format!("GPG is not available ({e}). Install GnuPG to sign commits with GPG keys.")
    })?;
    match out.code {
        Some(0) => Ok(parse_gpg_secret_keys(&out.stdout)),
        _ => Err(out
            .stderr
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("gpg failed")
            .to_string()),
    }
}

fn local_ssh_keys() -> Vec<SshKey> {
    let Some(dir) = dirs::home_dir().map(|h| h.join(".ssh")) else {
        return Vec::new();
    };
    let mut keys: Vec<SshKey> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "pub"))
        .filter_map(|p| {
            let text = std::fs::read_to_string(&p).ok()?;
            parse_ssh_public_key(&p.to_string_lossy(), &text)
        })
        .collect();
    keys.sort_by(|a, b| a.path.cmp(&b.path));
    keys
}

fn remote_ssh_keys(backend: &dyn Backend) -> Vec<SshKey> {
    let cmd = CommandLine::new("sh").args(["-c", SSH_KEY_LISTING_SCRIPT]);
    capture(backend, &cmd, "", Duration::from_secs(15))
        .map(|out| parse_ssh_key_listing(&out.stdout))
        .unwrap_or_default()
}

/// Keys to sign with (GPG secret keys, SSH public keys) and the git config setup, on the
/// machine the project's git runs on.
#[tauri::command]
pub async fn git_signing_info(state: State<'_, AppState>) -> Result<SigningInfo> {
    let backend: Arc<dyn Backend> = match state.backend() {
        Ok(b) => b,
        Err(_) => {
            let home =
                dirs::home_dir().ok_or_else(|| ForgeError::Other("no home directory".into()))?;
            Arc::new(LocalBackend::open(home)?)
        }
    };
    let config = with_repo(&state, |r| Ok(r.signing_config()))
        .await
        .unwrap_or_default();
    blocking(move || {
        let remote = backend.info().remote.is_some();
        let (gpg_keys, gpg_error) = match list_gpg_keys(backend.as_ref()) {
            Ok(keys) => (keys, None),
            Err(e) => (Vec::new(), Some(e)),
        };
        let ssh_keys = if remote {
            remote_ssh_keys(backend.as_ref())
        } else {
            local_ssh_keys()
        };
        Ok(SigningInfo {
            config,
            gpg_keys,
            gpg_error,
            ssh_keys,
            remote,
        })
    })
    .await
}
