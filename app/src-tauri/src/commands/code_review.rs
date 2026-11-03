//! Review mode of a pull/merge request (`forge-code-review`): viewed files, pending review
//! comments submitted as one review, and the AI features "Summarize PR", "AI Review" and
//! "Explain this change".
//!
//! State lives in the cache folder, never in the project. AI calls use the actions model
//! (Settings | Tools | AI Assistant) and pass the project's AI policy first: nothing is sent
//! when no provider is configured, AI is off for the project (`ai.enabled`) or a file is
//! in `.aiignore`. Nothing is posted to the code host without an explicit user action.

use std::sync::Mutex;

use forge_ai::AiFeature;
use forge_code_review::{
    build_review, draft_from_finding, AiRun, Budget, CachedFindings, CachedSummary, Draft, Finding,
    RequestMeta, ReviewRecord, ReviewStore,
};
use forge_core::{ForgeError, Result};
use forge_forges::{ChangedFile, CodeHost, MergeRequestDetails, RepoRef, ReviewEvent};
use serde::Serialize;
use tauri::State;

use crate::commands::ai::AiState;
use crate::commands::ai_editor::policy;
use crate::commands::forges::{current, ForgesState};
use crate::state::AppState;

/// Managed state: the review store and a lock that serializes its read-modify-write cycles.
pub struct CodeReviewState {
    store: ReviewStore,
    lock: Mutex<()>,
}

impl Default for CodeReviewState {
    fn default() -> Self {
        let dir = ReviewStore::default_dir()
            .unwrap_or_else(|| std::env::temp_dir().join("forge-code-review"));
        Self {
            store: ReviewStore::new(dir),
            lock: Mutex::new(()),
        }
    }
}

impl CodeReviewState {
    fn update<T>(
        &self,
        repo: &RepoRef,
        number: u64,
        f: impl FnOnce(&mut ReviewRecord) -> T,
    ) -> Result<T> {
        let _guard = self.lock.lock().expect("review store lock");
        self.store
            .update(repo, number, f)
            .map_err(|e| ForgeError::Other(format!("could not save review state: {e}")))
    }

    fn load(&self, repo: &RepoRef, number: u64) -> ReviewRecord {
        let _guard = self.lock.lock().expect("review store lock");
        self.store.load(repo, number)
    }
}

/// One changed file in review mode.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFile {
    pub path: String,
    /// Digest of the file's diff; pass it back to `review_set_viewed`.
    pub digest: String,
    pub viewed: bool,
    /// Excluded from AI by `.aiignore` (or AI is off for the project).
    pub ai_excluded: bool,
}

/// The provider AI features would use, or why there is none.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiInfo {
    pub provider: Option<String>,
    pub model: Option<String>,
    /// Runs on this machine (Ollama, LM Studio, llama.cpp).
    pub local: bool,
    /// Set when AI cannot be used: no provider, provider disabled, AI off for the project.
    pub problem: Option<String>,
}

/// AI Review findings still waiting for a decision.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingsView {
    pub head_sha: String,
    pub findings: Vec<Finding>,
    /// All findings of the run, including dismissed and drafted ones.
    pub total: usize,
    pub provider: String,
    pub model: Option<String>,
    pub excluded: Vec<String>,
    pub omitted: Vec<String>,
}

impl From<&CachedFindings> for FindingsView {
    fn from(c: &CachedFindings) -> Self {
        Self {
            head_sha: c.head_sha.clone(),
            findings: c.open(),
            total: c.findings.len(),
            provider: c.provider.clone(),
            model: c.model.clone(),
            excluded: c.excluded.clone(),
            omitted: c.omitted.clone(),
        }
    }
}

/// Everything review mode shows for one request.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSession {
    pub head_sha: String,
    pub files: Vec<ReviewFile>,
    /// The changed files with their diffs (same as `forges_mr_files`).
    pub changes: Vec<ChangedFile>,
    pub drafts: Vec<Draft>,
    /// Verdicts the host accepts when submitting a review.
    pub events: Vec<ReviewEvent>,
    /// The cached summary of the current head commit.
    pub summary: Option<CachedSummary>,
    /// The cached AI review of the current head commit.
    pub findings: Option<FindingsView>,
    pub ai: AiInfo,
}

/// Result of acting on a finding.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingUpdate {
    pub drafts: Vec<Draft>,
    pub findings: Option<FindingsView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainResult {
    /// Markdown.
    pub text: String,
    pub provider: String,
    pub model: Option<String>,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn meta(host: &dyn CodeHost, d: &MergeRequestDetails) -> RequestMeta {
    RequestMeta {
        request_name: host.capabilities().request_name.to_lowercase(),
        title: d.summary.title.clone(),
        description: d.description.clone(),
        source_branch: d.summary.source_branch.clone(),
        target_branch: d.summary.target_branch.clone(),
    }
}

fn head_of(d: &MergeRequestDetails) -> String {
    d.summary.head_sha.clone().unwrap_or_default()
}

fn ai_info(ai: &AiState, project_enabled: bool) -> AiInfo {
    match ai.service.selection(None, None, Some(AiFeature::Chat)) {
        Ok((cfg, model)) => AiInfo {
            provider: Some(cfg.name.clone()),
            model: model.or_else(|| cfg.default_model.clone()),
            local: cfg.kind.is_local(),
            problem: (!project_enabled)
                .then(|| "AI features are turned off for this project".to_string()),
        },
        Err(e) => AiInfo {
            problem: Some(e.to_string()),
            ..AiInfo::default()
        },
    }
}

/// Review mode of a request: files with their viewed state, drafts, cached AI results.
#[tauri::command]
pub async fn review_session(
    number: u64,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
    ai: State<'_, AiState>,
    review: State<'_, CodeReviewState>,
) -> Result<ReviewSession> {
    let (repo, host) = current(&app, &forges).await?;
    let (details, changes) = tokio::try_join!(
        host.merge_request(&repo, number),
        host.merge_request_files(&repo, number)
    )?;
    let head = head_of(&details);
    let policy = policy(&app).await;
    let record = review.load(&repo, number);
    let files = changes
        .iter()
        .map(|f| ReviewFile {
            path: f.path.clone(),
            digest: forge_code_review::digest(f),
            viewed: record.is_viewed(f),
            ai_excluded: !forge_code_review::context::allowed(&policy, f),
        })
        .collect();
    Ok(ReviewSession {
        files,
        changes,
        drafts: record.drafts.clone(),
        events: host.review_events(),
        summary: record.summary_for(&head).cloned(),
        findings: record.findings_for(&head).map(FindingsView::from),
        ai: ai_info(&ai, policy.project_enabled),
        head_sha: head,
    })
}

/// Mark a file as viewed (with the digest `review_session` returned) or unviewed.
#[tauri::command]
pub async fn review_set_viewed(
    number: u64,
    path: String,
    digest: String,
    viewed: bool,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
    review: State<'_, CodeReviewState>,
) -> Result<()> {
    let (repo, _) = current(&app, &forges).await?;
    review.update(&repo, number, |r| r.set_viewed(&path, &digest, viewed))
}

/// Add (empty id) or change a pending review comment; returns all drafts.
#[tauri::command]
pub async fn review_save_draft(
    number: u64,
    draft: Draft,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
    review: State<'_, CodeReviewState>,
) -> Result<Vec<Draft>> {
    if draft.body.trim().is_empty() {
        return Err(ForgeError::Other("A review comment needs some text".into()));
    }
    let (repo, _) = current(&app, &forges).await?;
    review.update(&repo, number, |r| {
        r.save_draft(draft);
        r.drafts.clone()
    })
}

#[tauri::command]
pub async fn review_delete_draft(
    number: u64,
    id: String,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
    review: State<'_, CodeReviewState>,
) -> Result<Vec<Draft>> {
    let (repo, _) = current(&app, &forges).await?;
    review.update(&repo, number, |r| {
        r.delete_draft(&id);
        r.drafts.clone()
    })
}

/// Submit all pending comments as one review with the verdict `event`.
#[tauri::command]
pub async fn review_submit(
    number: u64,
    event: ReviewEvent,
    body: String,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
    review: State<'_, CodeReviewState>,
) -> Result<()> {
    let (repo, host) = current(&app, &forges).await?;
    let files = host.merge_request_files(&repo, number).await?;
    let drafts = review.load(&repo, number).drafts;
    let new = build_review(&drafts, event, &body, &files).map_err(ForgeError::Other)?;
    host.submit_review(&repo, number, &new).await?;
    let sent: Vec<String> = drafts.iter().map(|d| d.id.clone()).collect();
    review.update(&repo, number, |r| {
        r.drafts.retain(|d| !sent.contains(&d.id))
    })
}

/// The provider and model for review features plus the project policy, or an error that
/// says why nothing can be sent.
async fn ai_run_parts(
    app: &AppState,
    ai: &AiState,
) -> Result<(
    Box<dyn forge_ai::Provider>,
    forge_ai::ProviderConfig,
    Option<String>,
    forge_ai_editor::Policy,
)> {
    let policy = policy(app).await;
    if !policy.project_enabled {
        return Err(ForgeError::Other(
            "AI features are turned off for this project".into(),
        ));
    }
    let (cfg, model) = ai.service.selection(None, None, Some(AiFeature::Chat))?;
    Ok((ai.service.build(&cfg), cfg, model, policy))
}

fn review_error(e: forge_code_review::ReviewError) -> ForgeError {
    match e {
        forge_code_review::ReviewError::Ai(a) => a.into(),
        other => ForgeError::Other(other.to_string()),
    }
}

/// "Summarize PR": overview, risks, per-file summary and test plan; cached per head SHA.
#[tauri::command]
pub async fn review_summarize(
    number: u64,
    refresh: bool,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
    ai: State<'_, AiState>,
    review: State<'_, CodeReviewState>,
) -> Result<CachedSummary> {
    let (repo, host) = current(&app, &forges).await?;
    let details = host.merge_request(&repo, number).await?;
    let head = head_of(&details);
    if !refresh {
        if let Some(s) = review.load(&repo, number).summary_for(&head) {
            return Ok(s.clone());
        }
    }
    let (provider, cfg, model, policy) = ai_run_parts(&app, &ai).await?;
    let files = host.merge_request_files(&repo, number).await?;
    let run = AiRun {
        provider: provider.as_ref(),
        model: model.clone(),
        policy: &policy,
        budget: Budget::default(),
    };
    let out = forge_code_review::summarize(&run, &meta(host.as_ref(), &details), &files)
        .await
        .map_err(review_error)?;
    let cached = CachedSummary {
        head_sha: head,
        summary: out.result,
        provider: cfg.name.clone(),
        model: model.or(cfg.default_model),
        excluded: out.excluded,
        omitted: out.omitted,
        created_at: now(),
    };
    let c = cached.clone();
    review.update(&repo, number, move |r| r.summary = Some(c))?;
    Ok(cached)
}

/// "AI Review": inline findings; cached per head SHA. Nothing is posted.
#[tauri::command]
pub async fn review_ai(
    number: u64,
    refresh: bool,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
    ai: State<'_, AiState>,
    review: State<'_, CodeReviewState>,
) -> Result<FindingsView> {
    let (repo, host) = current(&app, &forges).await?;
    let details = host.merge_request(&repo, number).await?;
    let head = head_of(&details);
    if !refresh {
        if let Some(c) = review.load(&repo, number).findings_for(&head) {
            return Ok(c.into());
        }
    }
    let (provider, cfg, model, policy) = ai_run_parts(&app, &ai).await?;
    let files = host.merge_request_files(&repo, number).await?;
    let run = AiRun {
        provider: provider.as_ref(),
        model: model.clone(),
        policy: &policy,
        budget: Budget::default(),
    };
    let out = forge_code_review::review(&run, &meta(host.as_ref(), &details), &files)
        .await
        .map_err(review_error)?;
    let cached = CachedFindings {
        head_sha: head,
        findings: out.result,
        handled: Default::default(),
        provider: cfg.name.clone(),
        model: model.or(cfg.default_model),
        excluded: out.excluded,
        omitted: out.omitted,
        created_at: now(),
    };
    let view = FindingsView::from(&cached);
    review.update(&repo, number, move |r| r.findings = Some(cached))?;
    Ok(view)
}

/// Turn a finding into a pending review comment (`action = "draft"`) or dismiss it.
#[tauri::command]
pub async fn review_finding(
    number: u64,
    id: String,
    action: String,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
    review: State<'_, CodeReviewState>,
) -> Result<FindingUpdate> {
    let (repo, _) = current(&app, &forges).await?;
    review.update(&repo, number, |r| {
        let finding = r
            .findings
            .as_ref()
            .and_then(|c| c.findings.iter().find(|f| f.id == id).cloned())
            .ok_or_else(|| ForgeError::Other("This finding is no longer available".into()))?;
        match action.as_str() {
            "draft" => {
                r.save_draft(draft_from_finding(&finding, repo.kind));
            }
            "dismiss" => {}
            other => return Err(ForgeError::Other(format!("unknown action {other}"))),
        }
        r.handle_finding(&id);
        Ok(FindingUpdate {
            drafts: r.drafts.clone(),
            findings: r.findings.as_ref().map(FindingsView::from),
        })
    })?
}

/// "Explain this change" for hunk `hunk` (0-based) of `path`.
#[tauri::command]
pub async fn review_explain(
    number: u64,
    path: String,
    hunk: usize,
    app: State<'_, AppState>,
    forges: State<'_, ForgesState>,
    ai: State<'_, AiState>,
) -> Result<ExplainResult> {
    let (repo, host) = current(&app, &forges).await?;
    let (provider, cfg, model, policy) = ai_run_parts(&app, &ai).await?;
    let (details, files) = tokio::try_join!(
        host.merge_request(&repo, number),
        host.merge_request_files(&repo, number)
    )?;
    let run = AiRun {
        provider: provider.as_ref(),
        model: model.clone(),
        policy: &policy,
        budget: Budget::default(),
    };
    let e = forge_code_review::explain(&run, &meta(host.as_ref(), &details), &files, &path, hunk)
        .await
        .map_err(review_error)?;
    Ok(ExplainResult {
        text: e.text,
        provider: cfg.name,
        model: model.or(cfg.default_model),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_match_the_ipc_contract() {
        let d: Draft = serde_json::from_value(serde_json::json!({
            "id": "", "body": "Nit", "position": { "path": "a.php", "newLine": 3 }, "startLine": 1
        }))
        .unwrap();
        assert_eq!(d.position.unwrap().new_line, Some(3));
        let e: ReviewEvent = serde_json::from_value(serde_json::json!("requestChanges")).unwrap();
        assert_eq!(e, ReviewEvent::RequestChanges);
        let info = serde_json::to_value(AiInfo::default()).unwrap();
        assert_eq!(info["problem"], serde_json::Value::Null);
    }
}
