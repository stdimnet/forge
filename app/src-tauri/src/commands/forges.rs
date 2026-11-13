//! Code host IPC (GitLab, Forgejo/Gitea, GitHub): accounts for Settings | Version Control,
//! and merge requests, issues and pipelines of the project's repository. The repository
//! is detected from `git remote -v` on every call, so switching projects needs no reset.

use std::sync::Arc;

use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_forges::{
    review_ref, AccountInfo, Capabilities, ChangedFile, CodeHost, Discussion, ForgeService,
    HostKind, Issue, IssueDetails, IssueFilter, KeyringStore, MergeOptions, MergeRequest,
    MergeRequestDetails, MrFilter, NewMergeRequest, Page, Pipeline, Position, RepoRef, User,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;

/// Managed state: accounts (config file + keychain) and HTTP clients.
pub struct ForgesState {
    service: ForgeService,
}

impl ForgesState {
    pub fn load() -> Self {
        let path = forge_forges::accounts::AccountsFile::default_path();
        let service = ForgeService::new(path.clone(), Arc::new(KeyringStore)).unwrap_or_else(|e| {
            tracing::warn!("{e}; code host accounts will not be saved");
            ForgeService::new(None, Arc::new(KeyringStore)).expect("in-memory accounts")
        });
        Self { service }
    }

    pub(crate) fn service(&self) -> &ForgeService {
        &self.service
    }
}

/// What the UI needs to know about the project's code host.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoContext {
    pub repo: RepoRef,
    /// Every code-host remote (origin first).
    pub repos: Vec<RepoRef>,
    pub account: Option<AccountInfo>,
    pub capabilities: Capabilities,
}

/// The project's backend: version control runs where the project lives (on the host for a
/// remote project), API calls stay on this machine with the user's tokens.
fn backend(state: &AppState) -> Result<Arc<dyn Backend>> {
    state.backend()
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| ForgeError::Other(format!("task failed: {e}")))?
}

async fn remotes(state: &AppState) -> Result<Vec<(String, String)>> {
    let backend = backend(state)?;
    blocking(move || {
        let Ok(repo) = backend.repo() else {
            return Ok(Vec::new());
        };
        Ok(forge_forges::remote::parse_remote_v(
            &repo.remote_verbose()?,
        ))
    })
    .await
}

pub(crate) async fn current(
    app: &AppState,
    forges: &ForgesState,
) -> Result<(RepoRef, Arc<dyn CodeHost>)> {
    let repo = forges
        .service
        .detect(&remotes(app).await?)
        .into_iter()
        .next()
        .ok_or(forge_forges::HostError::NoRemote)?;
    let host = forges.service.host_for(&repo);
    Ok((repo, host))
}

#[tauri::command]
pub fn forges_accounts(forges: State<'_, ForgesState>) -> Vec<AccountInfo> {
    forges.service.accounts()
}

/// Verify a token and store the account; the token goes to the OS keychain.
#[tauri::command]
pub async fn forges_login(
    kind: HostKind,
    server: String,
    token: String,
    api_url: Option<String>,
    forges: State<'_, ForgesState>,
) -> Result<AccountInfo> {
    Ok(forges.service.login(kind, &server, &token, api_url).await?)
}

#[tauri::command]
pub fn forges_logout(id: String, forges: State<'_, ForgesState>) -> Result<()> {
    Ok(forges.service.logout(&id)?)
}

#[tauri::command]
pub async fn forges_verify(id: String, forges: State<'_, ForgesState>) -> Result<User> {
    Ok(forges.service.verify(&id).await?)
}

/// The project's code host repository, or `null` when no remote points to one.
#[tauri::command]
pub async fn forges_repo(
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<Option<RepoContext>> {
    let repos = forges.service.detect(&remotes(&app).await?);
    let Some(repo) = repos.first().cloned() else {
        return Ok(None);
    };
    let account = forges.service.account_for(&repo).map(|a| {
        forges
            .service
            .accounts()
            .into_iter()
            .find(|i| i.account.id == a.id)
            .expect("account listed")
    });
    let capabilities = forges.service.host_for(&repo).capabilities();
    Ok(Some(RepoContext {
        repo,
        repos,
        account,
        capabilities,
    }))
}

#[tauri::command]
pub async fn forges_merge_requests(
    filter: MrFilter,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<Page<MergeRequest>> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host.merge_requests(&repo, &filter).await?)
}

#[tauri::command]
pub async fn forges_merge_request(
    number: u64,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<MergeRequestDetails> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host.merge_request(&repo, number).await?)
}

#[tauri::command]
pub async fn forges_mr_files(
    number: u64,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<Vec<ChangedFile>> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host.merge_request_files(&repo, number).await?)
}

#[tauri::command]
pub async fn forges_discussions(
    number: u64,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<Vec<Discussion>> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host.discussions(&repo, number).await?)
}

#[tauri::command]
pub async fn forges_create_mr(
    request: NewMergeRequest,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<MergeRequest> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host.create_merge_request(&repo, &request).await?)
}

#[tauri::command]
pub async fn forges_comment(
    number: u64,
    body: String,
    position: Option<Position>,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<()> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host
        .add_comment(&repo, number, &body, position.as_ref())
        .await?)
}

#[tauri::command]
pub async fn forges_reply(
    number: u64,
    discussion_id: String,
    body: String,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<()> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host.reply(&repo, number, &discussion_id, &body).await?)
}

#[tauri::command]
pub async fn forges_resolve(
    number: u64,
    discussion_id: String,
    resolved: bool,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<()> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host
        .resolve(&repo, number, &discussion_id, resolved)
        .await?)
}

#[tauri::command]
pub async fn forges_approve(
    number: u64,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<()> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host.approve(&repo, number).await?)
}

#[tauri::command]
pub async fn forges_merge(
    number: u64,
    options: MergeOptions,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<()> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host.merge(&repo, number, &options).await?)
}

#[tauri::command]
pub async fn forges_close(
    number: u64,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<()> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host.close(&repo, number).await?)
}

#[tauri::command]
pub async fn forges_issues(
    filter: IssueFilter,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<Page<Issue>> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host.issues(&repo, &filter).await?)
}

#[tauri::command]
pub async fn forges_issue(
    number: u64,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<IssueDetails> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host.issue(&repo, number).await?)
}

#[tauri::command]
pub async fn forges_issue_comment(
    number: u64,
    body: String,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<()> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host.comment_issue(&repo, number, &body).await?)
}

#[tauri::command]
pub async fn forges_pipelines(
    branch: Option<String>,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<Vec<Pipeline>> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host.pipelines(&repo, branch.as_deref()).await?)
}

/// Latest CI status of a branch or commit (status bar widget, MR details).
#[tauri::command]
pub async fn forges_ci_status(
    git_ref: String,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<Option<Pipeline>> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host.ci_status(&repo, &git_ref).await?)
}

#[tauri::command]
pub async fn forges_members(
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<Vec<User>> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host.members(&repo).await?)
}

/// A file at a revision on the host (left/right side of a merge request diff).
#[tauri::command]
pub async fn forges_file(
    path: String,
    git_ref: String,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<Option<String>> {
    let (repo, host) = current(&app, &forges).await?;
    Ok(host.file_content(&repo, &path, &git_ref).await?)
}

/// Check out a merge request's source branch: the remote branch when it lives in the
/// repository, otherwise the host's review ref (forks) as a local `mr-<n>`/`pr-<n>` branch.
/// Returns the checked-out branch name.
#[tauri::command]
pub async fn forges_checkout(
    number: u64,
    source_branch: String,
    app_handle: AppHandle,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
) -> Result<String> {
    let (repo_ref, _) = current(&app, &forges).await?;
    let backend = backend(&app)?;
    let res = blocking(move || {
        let repo = backend.repo()?;
        let remote = repo_ref.remote.clone();
        let tracking = format!("refs/remotes/{remote}/{source_branch}");
        let branch_spec = format!("+refs/heads/{source_branch}:{tracking}");
        if repo.fetch_refspec(&remote, &branch_spec).is_ok() {
            if repo.has_branch(&source_branch) {
                repo.checkout(&source_branch)?;
            } else {
                repo.checkout_remote(&format!("{remote}/{source_branch}"))?;
            }
            return Ok(source_branch);
        }
        let local = match repo_ref.kind {
            HostKind::Gitlab => format!("mr-{number}"),
            _ => format!("pr-{number}"),
        };
        let spec = format!("+{}:refs/heads/{local}", review_ref(repo_ref.kind, number));
        repo.fetch_refspec(&remote, &spec)?;
        repo.checkout(&local)?;
        Ok(local)
    })
    .await;
    if let Err(e) = app_handle.emit(super::git::CHANGED_EVENT, ()) {
        tracing::warn!("could not emit git change: {e}");
    }
    res
}
