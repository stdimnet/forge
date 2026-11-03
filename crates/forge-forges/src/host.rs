//! The [`CodeHost`] trait implemented by GitLab, Forgejo/Gitea and GitHub.

use async_trait::async_trait;
use forge_vcs::Hunk;

use crate::error::HostResult;
use crate::model::*;

/// Merge/pull requests, reviews, issues and CI of one repository on a code host. Numbers
/// are the per-project ones users see (GitLab `iid`, GitHub/Forgejo `number`).
#[async_trait]
pub trait CodeHost: Send + Sync {
    fn kind(&self) -> HostKind;
    fn capabilities(&self) -> Capabilities;

    /// The token owner (used to verify a token).
    async fn current_user(&self) -> HostResult<User>;

    /// Repositories the account owns or is a member of, recently active first (Get from
    /// VCS). At most a few hundred.
    async fn repositories(&self) -> HostResult<Vec<HostRepository>>;

    async fn merge_requests(
        &self,
        repo: &RepoRef,
        filter: &MrFilter,
    ) -> HostResult<Page<MergeRequest>>;
    async fn merge_request(&self, repo: &RepoRef, number: u64) -> HostResult<MergeRequestDetails>;
    async fn merge_request_files(
        &self,
        repo: &RepoRef,
        number: u64,
    ) -> HostResult<Vec<ChangedFile>>;
    /// General comments and inline review threads, oldest first.
    async fn discussions(&self, repo: &RepoRef, number: u64) -> HostResult<Vec<Discussion>>;
    async fn create_merge_request(
        &self,
        repo: &RepoRef,
        new: &NewMergeRequest,
    ) -> HostResult<MergeRequest>;
    /// Start a thread: a general comment, or an inline one when `position` is given.
    async fn add_comment(
        &self,
        repo: &RepoRef,
        number: u64,
        body: &str,
        position: Option<&Position>,
    ) -> HostResult<()>;
    async fn reply(
        &self,
        repo: &RepoRef,
        number: u64,
        discussion_id: &str,
        body: &str,
    ) -> HostResult<()>;
    async fn resolve(
        &self,
        repo: &RepoRef,
        number: u64,
        discussion_id: &str,
        resolved: bool,
    ) -> HostResult<()>;
    async fn approve(&self, repo: &RepoRef, number: u64) -> HostResult<()>;
    async fn merge(&self, repo: &RepoRef, number: u64, opts: &MergeOptions) -> HostResult<()>;
    async fn close(&self, repo: &RepoRef, number: u64) -> HostResult<()>;

    async fn issues(&self, repo: &RepoRef, filter: &IssueFilter) -> HostResult<Page<Issue>>;
    async fn issue(&self, repo: &RepoRef, number: u64) -> HostResult<IssueDetails>;
    async fn comment_issue(&self, repo: &RepoRef, number: u64, body: &str) -> HostResult<()>;

    /// Recent pipelines / workflow runs, newest first, optionally for one branch.
    async fn pipelines(&self, repo: &RepoRef, branch: Option<&str>) -> HostResult<Vec<Pipeline>>;
    /// Latest CI status of a branch or commit, with its jobs/checks.
    async fn ci_status(&self, repo: &RepoRef, git_ref: &str) -> HostResult<Option<Pipeline>>;

    /// Review verdicts [`CodeHost::submit_review`] accepts.
    fn review_events(&self) -> Vec<ReviewEvent> {
        let mut events = vec![ReviewEvent::Comment];
        if self.capabilities().approvals {
            events.push(ReviewEvent::Approve);
        }
        events
    }

    /// Submit pending comments as one review. Hosts without a review API post the comments
    /// one by one, then the body, then approve.
    async fn submit_review(
        &self,
        repo: &RepoRef,
        number: u64,
        review: &NewReview,
    ) -> HostResult<()> {
        if !self.review_events().contains(&review.event) {
            return Err(crate::error::HostError::Unsupported {
                host: self.kind().display_name().into(),
                what: "this review verdict".into(),
            });
        }
        for c in &review.comments {
            let body = match (c.start_line, c.position.new_line.or(c.position.old_line)) {
                (Some(s), Some(e)) if s < e => format!("*Lines {s}–{e}:*\n\n{}", c.body),
                _ => c.body.clone(),
            };
            self.add_comment(repo, number, &body, Some(&c.position))
                .await?;
        }
        if !review.body.trim().is_empty() {
            self.add_comment(repo, number, &review.body, None).await?;
        }
        if review.event == ReviewEvent::Approve {
            self.approve(repo, number).await?;
        }
        Ok(())
    }

    /// Users who can be asked for a review.
    async fn members(&self, repo: &RepoRef) -> HostResult<Vec<User>>;
    /// A file at a revision (`None` if it does not exist there).
    async fn file_content(
        &self,
        repo: &RepoRef,
        path: &str,
        git_ref: &str,
    ) -> HostResult<Option<String>>;
}

/// Parse a per-file patch that starts at the first `@@` (GitLab `diff`, GitHub `patch`).
pub fn parse_patch(path: &str, patch: &str) -> Vec<Hunk> {
    let text = format!("diff --git a/{path} b/{path}\n{patch}");
    forge_vcs::parse_diff(&text)
        .into_iter()
        .next()
        .map(|f| f.hunks)
        .unwrap_or_default()
}

/// Count added and removed lines of parsed hunks.
pub fn count_changes(hunks: &[Hunk]) -> (u32, u32) {
    let mut add = 0;
    let mut del = 0;
    for l in hunks.iter().flat_map(|h| &h.lines) {
        match l.kind {
            forge_vcs::LineKind::Added => add += 1,
            forge_vcs::LineKind::Removed => del += 1,
            forge_vcs::LineKind::Context => {}
        }
    }
    (add, del)
}

/// Turn "Draft: Title" / "WIP: Title" / "[WIP] Title" into `(true, "Title")`.
pub fn strip_draft_prefix(title: &str) -> (bool, String) {
    let t = title.trim_start();
    for p in ["Draft:", "[Draft]", "(Draft)", "WIP:", "[WIP]"] {
        if t.len() >= p.len() && t[..p.len()].eq_ignore_ascii_case(p) {
            return (true, t[p.len()..].trim_start().to_string());
        }
    }
    (false, title.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_patches() {
        let hunks = parse_patch(
            "a.php",
            "@@ -1,2 +1,3 @@ class A\n <?php\n-old\n+new\n+more\n",
        );
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].new_start, 1);
        assert_eq!(count_changes(&hunks), (2, 1));
    }

    #[test]
    fn draft_prefixes() {
        assert_eq!(strip_draft_prefix("Draft: Add x"), (true, "Add x".into()));
        assert_eq!(strip_draft_prefix("WIP: Add x"), (true, "Add x".into()));
        assert_eq!(strip_draft_prefix("Add x"), (false, "Add x".into()));
    }
}
