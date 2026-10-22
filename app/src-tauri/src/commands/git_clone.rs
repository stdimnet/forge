//! Get from VCS (Git ▸ Clone…, the Welcome screen's "Get from VCS"): the account
//! repositories to choose from, the Test button (`git ls-remote`) and the clone itself with
//! progress and Cancel. Git runs on this machine, or on the remote host of an SSH connection
//! (`connectionId`). HTTPS tokens of code host accounts reach git for that one invocation.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_backend::vcs::{self, GitRunner};
use forge_backend::{Backend, LocalBackend};
use forge_core::{ForgeError, Result};
use forge_forges::HostRepository;
use forge_vcs::clone::{repo_name_from_url, CloneProgress, HttpsCredential, RemoteRef};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::commands::forges::ForgesState;
use crate::commands::git::apply_git_path;
use crate::commands::remote::{clone_target, RemoteState};
use crate::commands::workspace::blocking;
use crate::state::AppState;

pub const PROGRESS_EVENT: &str = "git://clone-progress";

/// The running clone's cancel flag.
#[derive(Default)]
pub struct CloneState {
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

/// Where git runs and the home directory there.
struct Target {
    backend: Arc<dyn Backend>,
    runner: GitRunner,
    home: String,
}

fn local_home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(std::env::temp_dir)
}

async fn target(
    connection_id: Option<String>,
    state: &AppState,
    remote: &RemoteState,
) -> Result<Target> {
    match connection_id.filter(|c| !c.is_empty()) {
        Some(id) => {
            let (backend, home) = clone_target(remote, state, &id).await?;
            Ok(Target {
                backend,
                runner: GitRunner {
                    program: "git".into(),
                    cwd: PathBuf::from("/"),
                    ssh_batch: false,
                },
                home,
            })
        }
        None => {
            apply_git_path(state);
            let home = local_home();
            let backend: Arc<dyn Backend> = Arc::new(LocalBackend::open(&home)?);
            Ok(Target {
                backend,
                runner: GitRunner {
                    program: forge_vcs::git_binary(),
                    cwd: home.clone(),
                    ssh_batch: std::env::var_os("GIT_SSH_COMMAND").is_none(),
                },
                home: home.to_string_lossy().into_owned(),
            })
        }
    }
}

/// `~` and `~/…` on the machine git runs on.
fn expand_home(path: &str, home: &str) -> String {
    let path = path.trim();
    if path == "~" {
        return home.to_string();
    }
    match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        Some(rest) => format!("{}/{rest}", home.trim_end_matches(['/', '\\'])),
        None => path.to_string(),
    }
}

fn credential(
    forges: &ForgesState,
    url: &str,
    account_id: Option<&str>,
) -> Option<HttpsCredential> {
    forges.service().credential_for_url(url, account_id)
}

/// The repositories of a code host account (owned, member, collaborator).
#[tauri::command]
pub async fn forges_repositories(
    account_id: String,
    forges: State<'_, ForgesState>,
) -> Result<Vec<HostRepository>> {
    forges
        .service()
        .repositories(&account_id)
        .await
        .map_err(|e| ForgeError::Other(e.to_string()))
}

/// The directory name for a URL (`git@host:acme/shop.git` → `shop`).
#[tauri::command]
pub fn git_clone_directory_name(url: String) -> Option<String> {
    repo_name_from_url(&url)
}

/// The home directory where git would run (this machine or the connection's host).
#[tauri::command]
pub async fn git_clone_home(
    connection_id: Option<String>,
    state: State<'_, AppState>,
    remote: State<'_, RemoteState>,
) -> Result<String> {
    match connection_id.filter(|c| !c.is_empty()) {
        Some(id) => Ok(clone_target(&remote, &state, &id).await?.1),
        None => Ok(local_home().to_string_lossy().into_owned()),
    }
}

/// The Test button: `git ls-remote` with the same credentials as the clone.
#[tauri::command]
pub async fn git_clone_test(
    url: String,
    account_id: Option<String>,
    connection_id: Option<String>,
    state: State<'_, AppState>,
    remote: State<'_, RemoteState>,
    forges: State<'_, ForgesState>,
) -> Result<Vec<RemoteRef>> {
    let t = target(connection_id, &state, &remote).await?;
    let cred = credential(&forges, &url, account_id.as_deref());
    blocking(move || {
        vcs::ls_remote(
            t.backend.as_ref(),
            &t.runner,
            &url,
            cred.as_ref(),
            Duration::from_secs(60),
        )
    })
    .await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneProgressEvent {
    #[serde(flatten)]
    pub progress: CloneProgress,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneResult {
    /// Absolute path of the new working copy on the machine it was cloned on.
    pub path: String,
}

/// Clone `url` into `directory` (`~` is the home directory there). Progress arrives as
/// `git://clone-progress`; `git_clone_cancel` stops it.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn git_clone(
    url: String,
    directory: String,
    account_id: Option<String>,
    connection_id: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
    remote: State<'_, RemoteState>,
    forges: State<'_, ForgesState>,
    clones: State<'_, CloneState>,
) -> Result<CloneResult> {
    let t = target(connection_id, &state, &remote).await?;
    let cred = credential(&forges, &url, account_id.as_deref());
    let path = expand_home(&directory, &t.home);
    let cancel = Arc::new(AtomicBool::new(false));
    *clones.cancel.lock().expect("clone lock") = Some(cancel.clone());
    let flag = cancel.clone();
    let dir = path.clone();
    let res = blocking(move || {
        vcs::clone(
            t.backend.as_ref(),
            &t.runner,
            &url,
            &dir,
            cred.as_ref(),
            &flag,
            &mut |progress| {
                let _ = app.emit(PROGRESS_EVENT, CloneProgressEvent { progress });
            },
        )
    })
    .await;
    *clones.cancel.lock().expect("clone lock") = None;
    res.map(|()| CloneResult { path })
}

/// Cancel the running clone.
#[tauri::command]
pub fn git_clone_cancel(clones: State<'_, CloneState>) {
    if let Some(c) = clones.cancel.lock().expect("clone lock").as_ref() {
        c.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_home_on_the_target_machine() {
        assert_eq!(
            expand_home("~/Projects/shop", "/home/m"),
            "/home/m/Projects/shop"
        );
        assert_eq!(expand_home("~", "/home/m/"), "/home/m/");
        assert_eq!(expand_home("/srv/shop", "/home/m"), "/srv/shop");
    }
}
