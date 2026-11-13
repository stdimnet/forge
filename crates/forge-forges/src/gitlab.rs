//! GitLab REST API v4 (gitlab.com and self-managed). Authentication with a personal access
//! token (`PRIVATE-TOKEN`). Projects are addressed by their URL-encoded full path, so nested
//! groups work without looking up numeric ids.

use std::collections::HashMap;

use async_trait::async_trait;
use reqwest::Method;
use serde_json::{json, Value};

use crate::error::{HostError, HostResult};
use crate::host::{count_changes, parse_patch, strip_draft_prefix, CodeHost};
use crate::http::{enc, query, Auth, Http, REPO_PAGES};
use crate::json::*;
use crate::model::*;

pub struct GitLab {
    http: Http,
}

impl GitLab {
    /// `api_base` is e.g. `https://gitlab.com/api/v4`.
    pub fn new(client: reqwest::Client, api_base: &str, token: Option<String>) -> Self {
        let auth = token.map(Auth::PrivateToken).unwrap_or(Auth::None);
        Self {
            http: Http::new(client, api_base, auth, "GitLab", "application/json"),
        }
    }

    fn project(repo: &RepoRef) -> String {
        format!("projects/{}", enc(&repo.path))
    }

    fn mr_path(repo: &RepoRef, number: u64) -> String {
        format!("{}/merge_requests/{number}", Self::project(repo))
    }

    fn merge_request_from(v: &Value, ci: Option<CiStatus>) -> MergeRequest {
        let (prefixed, title) = strip_draft_prefix(&s(v, "title"));
        let state = match s(v, "state").as_str() {
            "merged" => MrState::Merged,
            "closed" | "locked" => MrState::Closed,
            _ => MrState::Open,
        };
        let ci = ci.or_else(|| {
            v.get("head_pipeline")
                .or_else(|| v.get("pipeline"))
                .and_then(|p| CiStatus::parse(&s(p, "status")))
        });
        MergeRequest {
            number: u(v, "iid"),
            title,
            state,
            draft: prefixed || b(v, "draft") || b(v, "work_in_progress"),
            author: user_at(v, "author"),
            source_branch: s(v, "source_branch"),
            target_branch: s(v, "target_branch"),
            labels: labels(v, "labels"),
            comments: ou(v, "user_notes_count").map(|n| n as u32),
            ci,
            created_at: s(v, "created_at"),
            updated_at: s(v, "updated_at"),
            web_url: s(v, "web_url"),
            head_sha: os(v, "sha"),
        }
    }

    fn pipeline_from(v: &Value) -> Option<Pipeline> {
        Some(Pipeline {
            id: os(v, "id")?,
            name: os(v, "name").filter(|n| !n.is_empty()),
            status: CiStatus::parse(&s(v, "status"))?,
            git_ref: os(v, "ref"),
            sha: os(v, "sha"),
            web_url: os(v, "web_url"),
            created_at: os(v, "created_at"),
            jobs: Vec::new(),
        })
    }

    async fn jobs(&self, repo: &RepoRef, pipeline_id: &str) -> HostResult<Vec<Job>> {
        let jobs: Vec<Value> = self
            .http
            .get_all(
                &format!(
                    "{}/pipelines/{pipeline_id}/jobs?per_page=100",
                    Self::project(repo)
                ),
                "pipeline jobs",
                3,
            )
            .await?;
        let mut out: Vec<Job> = jobs
            .iter()
            .filter_map(|j| {
                Some(Job {
                    name: s(j, "name"),
                    stage: os(j, "stage"),
                    status: CiStatus::parse(&s(j, "status"))?,
                    web_url: os(j, "web_url"),
                    duration: j.get("duration").and_then(Value::as_f64),
                })
            })
            .collect();
        // The API lists newest jobs first; show them in pipeline order.
        out.reverse();
        Ok(out)
    }

    fn note_from(n: &Value) -> Note {
        Note {
            id: s(n, "id"),
            author: user_at(n, "author"),
            body: s(n, "body"),
            created_at: s(n, "created_at"),
            system: b(n, "system"),
        }
    }

    fn position_from(p: &Value) -> Option<Position> {
        if !p.is_object() {
            return None;
        }
        let path = os(p, "new_path").or_else(|| os(p, "old_path"))?;
        let old_path = os(p, "old_path").filter(|o| *o != path);
        Some(Position {
            path,
            old_path,
            new_line: ou(p, "new_line").map(|n| n as u32),
            old_line: ou(p, "old_line").map(|n| n as u32),
        })
    }

    async fn user_id(&self, username: &str) -> HostResult<u64> {
        let users: Vec<Value> = self
            .http
            .get(&format!("users?username={}", enc(username)), "user")
            .await?;
        users
            .first()
            .and_then(|u| ou(u, "id"))
            .ok_or_else(|| HostError::NotFound {
                host: "GitLab".into(),
                what: format!("user {username}"),
            })
    }

    async fn latest_pipelines_by_sha(&self, repo: &RepoRef) -> HashMap<String, CiStatus> {
        let res: HostResult<Vec<Value>> = self
            .http
            .get(
                &format!("{}/pipelines?per_page=100", Self::project(repo)),
                "pipelines",
            )
            .await;
        let mut map = HashMap::new();
        for p in res.unwrap_or_default() {
            if let (Some(sha), Some(st)) = (os(&p, "sha"), CiStatus::parse(&s(&p, "status"))) {
                // Newest first: keep the first status per commit.
                map.entry(sha).or_insert(st);
            }
        }
        map
    }
}

#[async_trait]
impl CodeHost for GitLab {
    fn kind(&self) -> HostKind {
        HostKind::Gitlab
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            resolve_threads: true,
            approvals: true,
            draft: true,
            merge_methods: vec![MergeMethod::Merge, MergeMethod::Squash],
            request_name: "Merge Request".into(),
            number_prefix: "!".into(),
        }
    }

    async fn current_user(&self) -> HostResult<User> {
        let v: Value = self.http.get("user", "user").await?;
        Ok(user(&v))
    }

    async fn repositories(&self) -> HostResult<Vec<HostRepository>> {
        let list: Vec<Value> = self
            .http
            .get_all(
                "projects?membership=true&per_page=100&order_by=last_activity_at&sort=desc",
                "projects",
                REPO_PAGES,
            )
            .await?;
        Ok(list
            .iter()
            .map(|v| HostRepository {
                path: s(v, "path_with_namespace"),
                name: s(v, "name"),
                description: os(v, "description").filter(|d| !d.is_empty()),
                private: os(v, "visibility").is_some_and(|vis| vis != "public"),
                fork: v.get("forked_from_project").is_some_and(|f| !f.is_null()),
                clone_url: s(v, "http_url_to_repo"),
                ssh_url: os(v, "ssh_url_to_repo"),
                web_url: s(v, "web_url"),
                updated_at: s(v, "last_activity_at"),
            })
            .collect())
    }

    async fn merge_requests(
        &self,
        repo: &RepoRef,
        filter: &MrFilter,
    ) -> HostResult<Page<MergeRequest>> {
        let state = match filter.state {
            StateFilter::Open => "opened",
            StateFilter::Closed => "closed",
            StateFilter::Merged => "merged",
            StateFilter::All => "all",
        };
        let q = query(&[
            ("state", Some(state.into())),
            ("author_username", filter.author.clone()),
            ("reviewer_username", filter.reviewer.clone()),
            ("assignee_username", filter.assignee.clone()),
            ("labels", filter.label.clone()),
            ("search", filter.search.clone()),
            ("order_by", Some("updated_at".into())),
            ("with_labels_details", Some("true".into())),
            ("page", Some(filter.page().to_string())),
            ("per_page", Some(filter.per_page().to_string())),
        ]);
        let path = format!("{}/merge_requests{q}", Self::project(repo));
        let (page, statuses) = tokio::join!(
            self.http.get_page::<Value>(&path, "project"),
            self.latest_pipelines_by_sha(repo)
        );
        let page = page?;
        let items = page
            .items
            .iter()
            .map(|v| {
                let ci = os(v, "sha").and_then(|sha| statuses.get(&sha).copied());
                Self::merge_request_from(v, ci)
            })
            .collect();
        Ok(Page {
            items,
            next_page: page.next_page,
            total: page.total,
        })
    }

    async fn merge_request(&self, repo: &RepoRef, number: u64) -> HostResult<MergeRequestDetails> {
        let path = Self::mr_path(repo, number);
        let v: Value = self
            .http
            .get(&path, &format!("merge request !{number}"))
            .await?;
        let approvals: Option<Value> = self
            .http
            .get(&format!("{path}/approvals"), "approvals")
            .await
            .ok();
        let mut pipeline = v.get("head_pipeline").and_then(Self::pipeline_from);
        if let Some(p) = pipeline.as_mut() {
            p.jobs = self.jobs(repo, &p.id).await.unwrap_or_default();
        }
        let approved_by: Vec<User> = approvals
            .as_ref()
            .map(|a| {
                arr(a, "approved_by")
                    .iter()
                    .map(|x| user_at(x, "user"))
                    .collect()
            })
            .unwrap_or_default();
        let reviewers = users(&v, "reviewers")
            .into_iter()
            .map(|user| Reviewer {
                state: if approved_by.iter().any(|a| a.username == user.username) {
                    ReviewState::Approved
                } else {
                    ReviewState::Requested
                },
                user,
            })
            .collect();
        let refs = v.get("diff_refs").cloned().unwrap_or(Value::Null);
        let mergeable = match s(&v, "detailed_merge_status").as_str() {
            "" => match s(&v, "merge_status").as_str() {
                "can_be_merged" => Some(true),
                "cannot_be_merged" | "cannot_be_merged_recheck" => Some(false),
                _ => None,
            },
            "mergeable" => Some(true),
            "checking" | "unchecked" | "preparing" => None,
            _ => Some(false),
        };
        let mut summary = Self::merge_request_from(&v, None);
        summary.ci = pipeline.as_ref().map(|p| p.status).or(summary.ci);
        Ok(MergeRequestDetails {
            summary,
            description: s(&v, "description"),
            reviewers,
            assignees: users(&v, "assignees"),
            approvals: Approvals {
                required: approvals
                    .as_ref()
                    .map(|a| u(a, "approvals_required") as u32)
                    .unwrap_or(0),
                approved_by,
            },
            pipeline,
            mergeable,
            conflicts: b(&v, "has_conflicts"),
            base_sha: os(&refs, "base_sha"),
            start_sha: os(&refs, "start_sha"),
            merged_by: v
                .get("merge_user")
                .or_else(|| v.get("merged_by"))
                .filter(|u| u.is_object())
                .map(user),
        })
    }

    async fn merge_request_files(
        &self,
        repo: &RepoRef,
        number: u64,
    ) -> HostResult<Vec<ChangedFile>> {
        let path = Self::mr_path(repo, number);
        let diffs: Vec<Value> = match self
            .http
            .get_all(&format!("{path}/diffs?per_page=50"), "diffs", 20)
            .await
        {
            Ok(d) => d,
            // GitLab < 15.7 has no /diffs endpoint.
            Err(HostError::NotFound { .. }) => {
                let v: Value = self.http.get(&format!("{path}/changes"), "changes").await?;
                arr(&v, "changes").to_vec()
            }
            Err(e) => return Err(e),
        };
        Ok(diffs
            .iter()
            .map(|d| {
                let new_path = s(d, "new_path");
                let old_path = s(d, "old_path");
                let hunks = parse_patch(&new_path, &s(d, "diff"));
                let (additions, deletions) = count_changes(&hunks);
                let status = if b(d, "new_file") {
                    ChangeKind::Added
                } else if b(d, "deleted_file") {
                    ChangeKind::Deleted
                } else if b(d, "renamed_file") {
                    ChangeKind::Renamed
                } else {
                    ChangeKind::Modified
                };
                ChangedFile {
                    old_path: (old_path != new_path && !old_path.is_empty()).then_some(old_path),
                    path: new_path,
                    status,
                    additions,
                    deletions,
                    hunks,
                }
            })
            .collect())
    }

    async fn discussions(&self, repo: &RepoRef, number: u64) -> HostResult<Vec<Discussion>> {
        let list: Vec<Value> = self
            .http
            .get_all(
                &format!("{}/discussions?per_page=100", Self::mr_path(repo, number)),
                "discussions",
                20,
            )
            .await?;
        Ok(list
            .iter()
            .filter_map(|d| {
                let notes_json = arr(d, "notes");
                let first = notes_json.first()?;
                let resolvable: Vec<&Value> =
                    notes_json.iter().filter(|n| b(n, "resolvable")).collect();
                Some(Discussion {
                    id: s(d, "id"),
                    notes: notes_json.iter().map(Self::note_from).collect(),
                    position: first.get("position").and_then(Self::position_from),
                    resolvable: !resolvable.is_empty(),
                    resolved: !resolvable.is_empty() && resolvable.iter().all(|n| b(n, "resolved")),
                })
            })
            .collect())
    }

    async fn create_merge_request(
        &self,
        repo: &RepoRef,
        new: &NewMergeRequest,
    ) -> HostResult<MergeRequest> {
        let mut reviewer_ids = Vec::new();
        for r in &new.reviewers {
            reviewer_ids.push(self.user_id(r).await?);
        }
        let title = if new.draft {
            format!("Draft: {}", new.title)
        } else {
            new.title.clone()
        };
        let body = json!({
            "source_branch": new.source_branch,
            "target_branch": new.target_branch,
            "title": title,
            "description": new.description,
            "reviewer_ids": reviewer_ids,
            "labels": new.labels.join(","),
            "remove_source_branch": new.remove_source_branch,
        });
        let v: Value = self
            .http
            .send_json(
                Method::POST,
                &format!("{}/merge_requests", Self::project(repo)),
                &body,
                "project",
            )
            .await?;
        Ok(Self::merge_request_from(&v, None))
    }

    async fn add_comment(
        &self,
        repo: &RepoRef,
        number: u64,
        body: &str,
        position: Option<&Position>,
    ) -> HostResult<()> {
        let path = Self::mr_path(repo, number);
        let Some(pos) = position else {
            return self
                .http
                .send_unit(
                    Method::POST,
                    &format!("{path}/notes"),
                    Some(&json!({ "body": body })),
                    "merge request",
                )
                .await;
        };
        let mr: Value = self.http.get(&path, "merge request").await?;
        let refs = mr.get("diff_refs").cloned().unwrap_or(Value::Null);
        let mut position = json!({
            "position_type": "text",
            "base_sha": s(&refs, "base_sha"),
            "start_sha": s(&refs, "start_sha"),
            "head_sha": s(&refs, "head_sha"),
            "new_path": pos.path,
            "old_path": pos.old_path.as_deref().unwrap_or(&pos.path),
        });
        if let Some(l) = pos.new_line {
            position["new_line"] = json!(l);
        }
        if let Some(l) = pos.old_line {
            position["old_line"] = json!(l);
        }
        self.http
            .send_unit(
                Method::POST,
                &format!("{path}/discussions"),
                Some(&json!({ "body": body, "position": position })),
                "merge request",
            )
            .await
    }

    async fn reply(
        &self,
        repo: &RepoRef,
        number: u64,
        discussion_id: &str,
        body: &str,
    ) -> HostResult<()> {
        self.http
            .send_unit(
                Method::POST,
                &format!(
                    "{}/discussions/{}/notes",
                    Self::mr_path(repo, number),
                    enc(discussion_id)
                ),
                Some(&json!({ "body": body })),
                "discussion",
            )
            .await
    }

    async fn resolve(
        &self,
        repo: &RepoRef,
        number: u64,
        discussion_id: &str,
        resolved: bool,
    ) -> HostResult<()> {
        self.http
            .send_unit(
                Method::PUT,
                &format!(
                    "{}/discussions/{}?resolved={resolved}",
                    Self::mr_path(repo, number),
                    enc(discussion_id)
                ),
                None,
                "discussion",
            )
            .await
    }

    async fn approve(&self, repo: &RepoRef, number: u64) -> HostResult<()> {
        self.http
            .send_unit(
                Method::POST,
                &format!("{}/approve", Self::mr_path(repo, number)),
                None,
                "merge request",
            )
            .await
    }

    async fn merge(&self, repo: &RepoRef, number: u64, opts: &MergeOptions) -> HostResult<()> {
        if opts.method == MergeMethod::Rebase {
            return Err(HostError::Unsupported {
                host: "GitLab".into(),
                what: "rebase merges from Forge; use the project's merge method".into(),
            });
        }
        let squash = opts.method == MergeMethod::Squash;
        let mut body = json!({
            "squash": squash,
            "should_remove_source_branch": opts.delete_source_branch,
        });
        if let Some(m) = opts
            .commit_message
            .as_deref()
            .filter(|m| !m.trim().is_empty())
        {
            body[if squash {
                "squash_commit_message"
            } else {
                "merge_commit_message"
            }] = json!(m);
        }
        self.http
            .send_unit(
                Method::PUT,
                &format!("{}/merge", Self::mr_path(repo, number)),
                Some(&body),
                "merge request",
            )
            .await
    }

    async fn close(&self, repo: &RepoRef, number: u64) -> HostResult<()> {
        self.http
            .send_unit(
                Method::PUT,
                &Self::mr_path(repo, number),
                Some(&json!({ "state_event": "close" })),
                "merge request",
            )
            .await
    }

    async fn issues(&self, repo: &RepoRef, filter: &IssueFilter) -> HostResult<Page<Issue>> {
        let state = match filter.state {
            IssueStateFilter::Open => "opened",
            IssueStateFilter::Closed => "closed",
            IssueStateFilter::All => "all",
        };
        let q = query(&[
            ("state", Some(state.into())),
            ("author_username", filter.author.clone()),
            ("assignee_username", filter.assignee.clone()),
            ("labels", filter.label.clone()),
            ("search", filter.search.clone()),
            ("order_by", Some("updated_at".into())),
            ("with_labels_details", Some("true".into())),
            ("page", Some(filter.page().to_string())),
            ("per_page", Some(filter.per_page().to_string())),
        ]);
        let page = self
            .http
            .get_page::<Value>(&format!("{}/issues{q}", Self::project(repo)), "project")
            .await?;
        Ok(Page {
            items: page.items.iter().map(issue_from).collect(),
            next_page: page.next_page,
            total: page.total,
        })
    }

    async fn issue(&self, repo: &RepoRef, number: u64) -> HostResult<IssueDetails> {
        let path = format!("{}/issues/{number}", Self::project(repo));
        let v: Value = self.http.get(&path, &format!("issue #{number}")).await?;
        let notes: Vec<Value> = self
            .http
            .get_all(
                &format!("{path}/notes?sort=asc&order_by=created_at&per_page=100"),
                "notes",
                10,
            )
            .await?;
        Ok(IssueDetails {
            summary: issue_from(&v),
            description: s(&v, "description"),
            notes: notes.iter().map(Self::note_from).collect(),
        })
    }

    async fn comment_issue(&self, repo: &RepoRef, number: u64, body: &str) -> HostResult<()> {
        self.http
            .send_unit(
                Method::POST,
                &format!("{}/issues/{number}/notes", Self::project(repo)),
                Some(&json!({ "body": body })),
                "issue",
            )
            .await
    }

    async fn pipelines(&self, repo: &RepoRef, branch: Option<&str>) -> HostResult<Vec<Pipeline>> {
        let q = query(&[
            ("ref", branch.map(str::to_string)),
            ("per_page", Some("20".into())),
        ]);
        let list: Vec<Value> = self
            .http
            .get(
                &format!("{}/pipelines{q}", Self::project(repo)),
                "pipelines",
            )
            .await?;
        Ok(list.iter().filter_map(Self::pipeline_from).collect())
    }

    async fn ci_status(&self, repo: &RepoRef, git_ref: &str) -> HostResult<Option<Pipeline>> {
        let key = if is_sha(git_ref) { "sha" } else { "ref" };
        let list: Vec<Value> = self
            .http
            .get(
                &format!(
                    "{}/pipelines?{key}={}&per_page=1",
                    Self::project(repo),
                    enc(git_ref)
                ),
                "pipelines",
            )
            .await?;
        let Some(mut p) = list.first().and_then(Self::pipeline_from) else {
            return Ok(None);
        };
        p.jobs = self.jobs(repo, &p.id).await.unwrap_or_default();
        Ok(Some(p))
    }

    async fn members(&self, repo: &RepoRef) -> HostResult<Vec<User>> {
        let list: Vec<Value> = self
            .http
            .get_all(
                &format!("{}/members/all?per_page=100", Self::project(repo)),
                "members",
                3,
            )
            .await?;
        Ok(list
            .iter()
            .filter(|m| s(m, "state") != "blocked")
            .map(user)
            .collect())
    }

    async fn file_content(
        &self,
        repo: &RepoRef,
        path: &str,
        git_ref: &str,
    ) -> HostResult<Option<String>> {
        self.http
            .get_text(
                &format!(
                    "{}/repository/files/{}/raw?ref={}",
                    Self::project(repo),
                    enc(path),
                    enc(git_ref)
                ),
                Some("*/*"),
            )
            .await
    }
}

fn issue_from(v: &Value) -> Issue {
    Issue {
        number: u(v, "iid"),
        title: s(v, "title"),
        state: if s(v, "state") == "closed" {
            IssueState::Closed
        } else {
            IssueState::Open
        },
        author: user_at(v, "author"),
        labels: labels(v, "labels"),
        assignees: users(v, "assignees"),
        comments: u(v, "user_notes_count") as u32,
        created_at: s(v, "created_at"),
        updated_at: s(v, "updated_at"),
        web_url: s(v, "web_url"),
    }
}

pub(crate) fn is_sha(s: &str) -> bool {
    s.len() >= 7 && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
