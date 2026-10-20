//! Host-neutral types for merge requests, issues and pipelines. GitHub and Forgejo call
//! merge requests "pull requests"; the UI picks the wording from [`HostKind`].

use forge_vcs::Hunk;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostKind {
    Gitlab,
    Forgejo,
    Github,
}

impl HostKind {
    pub const ALL: [HostKind; 3] = [HostKind::Gitlab, HostKind::Forgejo, HostKind::Github];

    pub fn display_name(self) -> &'static str {
        match self {
            HostKind::Gitlab => "GitLab",
            HostKind::Forgejo => "Forgejo / Gitea",
            HostKind::Github => "GitHub",
        }
    }

    /// The public instance used when an account has no server URL.
    pub fn default_server(self) -> &'static str {
        match self {
            HostKind::Gitlab => "https://gitlab.com",
            HostKind::Forgejo => "https://codeberg.org",
            HostKind::Github => "https://github.com",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            HostKind::Gitlab => "gitlab",
            HostKind::Forgejo => "forgejo",
            HostKind::Github => "github",
        }
    }
}

/// A repository on a code host, detected from a git remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoRef {
    pub kind: HostKind,
    /// `https://host[:port]` of the web UI.
    pub server: String,
    /// `owner/repo`, or `group/subgroup/project` on GitLab.
    pub path: String,
    /// Name of the git remote it was found on.
    pub remote: String,
}

impl RepoRef {
    /// Owner (namespace) and repository name.
    pub fn owner_repo(&self) -> (&str, &str) {
        self.path.rsplit_once('/').unwrap_or(("", &self.path))
    }

    pub fn web_url(&self) -> String {
        format!("{}/{}", self.server, self.path)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub username: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub name: String,
    /// `#rrggbb` when the host provides one.
    pub color: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MrState {
    Open,
    Closed,
    Merged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CiStatus {
    Success,
    Failed,
    Running,
    Pending,
    Canceled,
    Skipped,
    Manual,
}

impl CiStatus {
    /// Map the many spellings hosts use (`failure`, `error`, `in_progress`, `queued`, ...).
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "success" | "passed" | "completed" | "neutral" => Self::Success,
            "failed" | "failure" | "error" | "timed_out" | "action_required" | "startup_failure" => {
                Self::Failed
            }
            "running" | "in_progress" => Self::Running,
            "pending" | "created" | "queued" | "waiting" | "waiting_for_resource" | "preparing"
            | "scheduled" | "requested" | "blocked" => Self::Pending,
            "canceled" | "cancelled" | "stale" => Self::Canceled,
            "skipped" => Self::Skipped,
            "manual" => Self::Manual,
            _ => return None,
        })
    }

    /// Overall status of several jobs/checks: any failure wins, then running, pending, ...
    pub fn combine(all: impl IntoIterator<Item = CiStatus>) -> Option<CiStatus> {
        let all: Vec<_> = all.into_iter().collect();
        let order = [
            Self::Failed,
            Self::Running,
            Self::Pending,
            Self::Manual,
            Self::Canceled,
            Self::Success,
            Self::Skipped,
        ];
        order.into_iter().find(|s| all.contains(s))
    }
}

/// One merge/pull request in a list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRequest {
    pub number: u64,
    pub title: String,
    pub state: MrState,
    pub draft: bool,
    pub author: User,
    pub source_branch: String,
    pub target_branch: String,
    pub labels: Vec<Label>,
    /// Number of comments, when the list endpoint reports it.
    pub comments: Option<u32>,
    pub ci: Option<CiStatus>,
    pub created_at: String,
    pub updated_at: String,
    pub web_url: String,
    pub head_sha: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewState {
    Requested,
    Approved,
    ChangesRequested,
    Commented,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reviewer {
    pub user: User,
    pub state: ReviewState,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Approvals {
    /// Approvals the host requires before merging (0 = none required / unknown).
    pub required: u32,
    pub approved_by: Vec<User>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRequestDetails {
    #[serde(flatten)]
    pub summary: MergeRequest,
    /// Markdown.
    pub description: String,
    pub reviewers: Vec<Reviewer>,
    pub assignees: Vec<User>,
    pub approvals: Approvals,
    pub pipeline: Option<Pipeline>,
    /// `None` while the host is still computing it.
    pub mergeable: Option<bool>,
    pub conflicts: bool,
    pub base_sha: Option<String>,
    /// GitLab's `start_sha` (needed for inline comments); otherwise the base sha.
    pub start_sha: Option<String>,
    pub merged_by: Option<User>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
}

/// A file changed by a merge request, with its parsed diff.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    pub old_path: Option<String>,
    pub status: ChangeKind,
    pub additions: u32,
    pub deletions: u32,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub author: User,
    /// Markdown.
    pub body: String,
    pub created_at: String,
    /// Generated by the host ("added 2 commits", "approved this merge request").
    pub system: bool,
}

/// Where an inline comment is anchored. `new_line` is in the new file, `old_line` in the
/// old one; context lines have both.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub path: String,
    pub old_path: Option<String>,
    pub new_line: Option<u32>,
    pub old_line: Option<u32>,
}

/// A thread: a general comment with replies, or an inline review thread.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Discussion {
    pub id: String,
    pub notes: Vec<Note>,
    pub position: Option<Position>,
    pub resolvable: bool,
    pub resolved: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewMergeRequest {
    pub source_branch: String,
    pub target_branch: String,
    pub title: String,
    pub description: String,
    pub draft: bool,
    /// Usernames.
    pub reviewers: Vec<String>,
    pub labels: Vec<String>,
    pub remove_source_branch: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeMethod {
    #[default]
    Merge,
    Squash,
    Rebase,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MergeOptions {
    pub method: MergeMethod,
    pub delete_source_branch: bool,
    pub commit_message: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StateFilter {
    #[default]
    Open,
    Closed,
    Merged,
    All,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MrFilter {
    pub state: StateFilter,
    pub author: Option<String>,
    pub reviewer: Option<String>,
    pub assignee: Option<String>,
    pub label: Option<String>,
    pub search: Option<String>,
    /// 1-based; 0 means 1.
    pub page: u32,
    /// 0 means the host default (20).
    pub per_page: u32,
}

impl MrFilter {
    pub fn page(&self) -> u32 {
        self.page.max(1)
    }

    pub fn per_page(&self) -> u32 {
        if self.per_page == 0 {
            20
        } else {
            self.per_page.min(100)
        }
    }

    /// Client-side check for hosts whose list endpoint lacks a filter.
    pub fn matches(&self, mr: &MergeRequest, reviewers: &[String], assignees: &[String]) -> bool {
        let eq = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
        unset_or(&self.author, |a| eq(a, &mr.author.username))
            && unset_or(&self.reviewer, |r| reviewers.iter().any(|x| eq(x, r)))
            && unset_or(&self.assignee, |r| assignees.iter().any(|x| eq(x, r)))
            && unset_or(&self.label, |l| mr.labels.iter().any(|x| eq(&x.name, l)))
            && unset_or(&self.search, |q| {
                let q = q.to_lowercase();
                mr.title.to_lowercase().contains(&q)
                    || mr.number.to_string() == q.trim_start_matches(['#', '!'])
                    || mr.source_branch.to_lowercase().contains(&q)
            })
    }
}

/// True when the filter value is unset or empty, otherwise the result of `f`.
fn unset_or(value: &Option<String>, f: impl FnOnce(&str) -> bool) -> bool {
    match value.as_deref().map(str::trim) {
        None | Some("") => true,
        Some(v) => f(v),
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueStateFilter {
    #[default]
    Open,
    Closed,
    All,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct IssueFilter {
    pub state: IssueStateFilter,
    pub author: Option<String>,
    pub assignee: Option<String>,
    pub label: Option<String>,
    pub search: Option<String>,
    pub page: u32,
    pub per_page: u32,
}

impl IssueFilter {
    pub fn page(&self) -> u32 {
        self.page.max(1)
    }

    pub fn per_page(&self) -> u32 {
        if self.per_page == 0 {
            20
        } else {
            self.per_page.min(100)
        }
    }
}

/// One page of a paginated list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_page: Option<u32>,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueState {
    Open,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub state: IssueState,
    pub author: User,
    pub labels: Vec<Label>,
    pub assignees: Vec<User>,
    pub comments: u32,
    pub created_at: String,
    pub updated_at: String,
    pub web_url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueDetails {
    #[serde(flatten)]
    pub summary: Issue,
    pub description: String,
    pub notes: Vec<Note>,
}

/// A CI pipeline (GitLab), workflow run (GitHub Actions, Forgejo Actions) or the combined
/// commit status of a revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pipeline {
    pub id: String,
    pub name: Option<String>,
    pub status: CiStatus,
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,
    pub sha: Option<String>,
    pub web_url: Option<String>,
    pub created_at: Option<String>,
    pub jobs: Vec<Job>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub name: String,
    pub stage: Option<String>,
    pub status: CiStatus,
    pub web_url: Option<String>,
    /// Seconds.
    pub duration: Option<f64>,
}

/// What a host's API can do, so the UI hides actions that would fail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub resolve_threads: bool,
    pub approvals: bool,
    pub draft: bool,
    pub merge_methods: Vec<MergeMethod>,
    /// "Merge Request" (GitLab) or "Pull Request".
    pub request_name: String,
    /// "!" (GitLab) or "#".
    pub number_prefix: String,
}
