//! Forgejo and Gitea (`/api/v1`, `Authorization: token ...`). Both share the API; Codeberg
//! runs Forgejo. Pull request review comments are grouped into threads by file and line,
//! since the API has no thread objects.

use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use futures_util::future::join_all;
use reqwest::Method;
use serde_json::{json, Value};

use crate::error::{HostError, HostResult};
use crate::host::{count_changes, strip_draft_prefix, CodeHost};
use crate::http::{enc, enc_path, query, Auth, Http, REPO_PAGES};
use crate::json::*;
use crate::model::*;

pub struct Forgejo {
    http: Http,
}

impl Forgejo {
    /// `api_base` is e.g. `https://codeberg.org/api/v1`.
    pub fn new(client: reqwest::Client, api_base: &str, token: Option<String>) -> Self {
        let auth = token.map(Auth::Token).unwrap_or(Auth::None);
        Self {
            http: Http::new(client, api_base, auth, "Forgejo", "application/json"),
        }
    }

    fn repo_path(repo: &RepoRef) -> String {
        let (owner, name) = repo.owner_repo();
        format!("repos/{}/{}", enc(owner), enc(name))
    }

    fn pr_path(repo: &RepoRef, number: u64) -> String {
        format!("{}/pulls/{number}", Self::repo_path(repo))
    }

    fn pull_from(v: &Value, ci: Option<CiStatus>) -> MergeRequest {
        let (prefixed, title) = strip_draft_prefix(&s(v, "title"));
        let head = v.get("head").cloned().unwrap_or(Value::Null);
        let base = v.get("base").cloned().unwrap_or(Value::Null);
        MergeRequest {
            number: u(v, "number"),
            title,
            state: if b(v, "merged") {
                MrState::Merged
            } else if s(v, "state") == "closed" {
                MrState::Closed
            } else {
                MrState::Open
            },
            draft: prefixed || b(v, "draft"),
            author: user_at(v, "user"),
            source_branch: s(&head, "ref"),
            target_branch: s(&base, "ref"),
            labels: labels(v, "labels"),
            comments: ou(v, "comments").map(|n| n as u32),
            ci,
            created_at: s(v, "created_at"),
            updated_at: s(v, "updated_at"),
            web_url: s(v, "html_url"),
            head_sha: os(&head, "sha"),
        }
    }

    fn note_from(c: &Value) -> Note {
        Note {
            id: s(c, "id"),
            author: user_at(c, "user"),
            body: s(c, "body"),
            created_at: os(c, "created_at")
                .or_else(|| os(c, "submitted_at"))
                .unwrap_or_default(),
            system: false,
        }
    }

    async fn reviews(&self, repo: &RepoRef, number: u64) -> HostResult<Vec<Value>> {
        self.http
            .get_all(
                &format!("{}/reviews?limit=50", Self::pr_path(repo, number)),
                "reviews",
                10,
            )
            .await
    }

    async fn head_sha(&self, repo: &RepoRef, number: u64) -> HostResult<String> {
        let pr: Value = self
            .http
            .get(&Self::pr_path(repo, number), "pull request")
            .await?;
        Ok(pr.get("head").map(|h| s(h, "sha")).unwrap_or_default())
    }

    /// Post an inline review comment (Forgejo threads are all comments on one line).
    async fn inline_comment(
        &self,
        repo: &RepoRef,
        number: u64,
        path: &str,
        new_line: Option<u32>,
        old_line: Option<u32>,
        body: &str,
    ) -> HostResult<()> {
        let commit = self.head_sha(repo, number).await?;
        let mut comment = json!({ "path": path, "body": body });
        match (new_line, old_line) {
            (Some(n), _) => comment["new_position"] = json!(n),
            (None, Some(o)) => comment["old_position"] = json!(o),
            (None, None) => {}
        }
        self.http
            .send_unit(
                Method::POST,
                &format!("{}/reviews", Self::pr_path(repo, number)),
                Some(&json!({
                    "event": "COMMENT",
                    "body": "",
                    "commit_id": commit,
                    "comments": [comment],
                })),
                "pull request",
            )
            .await
    }

    async fn combined_status(&self, repo: &RepoRef, git_ref: &str) -> HostResult<Option<Value>> {
        self.http
            .get_optional(
                &format!("{}/commits/{}/status", Self::repo_path(repo), enc(git_ref)),
                "commit status",
            )
            .await
    }

    fn pipeline_from_status(v: &Value, git_ref: &str) -> Option<Pipeline> {
        let statuses = arr(v, "statuses");
        if statuses.is_empty() {
            return None;
        }
        let jobs: Vec<Job> = statuses
            .iter()
            .filter_map(|st| {
                let state = os(st, "status").or_else(|| os(st, "state"))?;
                Some(Job {
                    name: s(st, "context"),
                    stage: os(st, "description").filter(|d| !d.is_empty()),
                    status: CiStatus::parse(&state)?,
                    web_url: os(st, "target_url").filter(|u| !u.is_empty()),
                    duration: None,
                })
            })
            .collect();
        let status = CiStatus::parse(&s(v, "state"))
            .or_else(|| CiStatus::combine(jobs.iter().map(|j| j.status)))?;
        Some(Pipeline {
            id: os(v, "sha").unwrap_or_else(|| git_ref.to_string()),
            name: None,
            status,
            git_ref: Some(git_ref.to_string()),
            sha: os(v, "sha"),
            web_url: jobs.iter().find_map(|j| j.web_url.clone()),
            created_at: statuses.first().and_then(|st| os(st, "created_at")),
            jobs,
        })
    }

    async fn label_ids(&self, repo: &RepoRef, names: &[String]) -> HostResult<Vec<u64>> {
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let all: Vec<Value> = self
            .http
            .get_all(
                &format!("{}/labels?limit=50", Self::repo_path(repo)),
                "labels",
                5,
            )
            .await?;
        Ok(names
            .iter()
            .filter_map(|n| {
                all.iter()
                    .find(|l| s(l, "name").eq_ignore_ascii_case(n))
                    .and_then(|l| ou(l, "id"))
            })
            .collect())
    }
}

/// Thread id of an inline comment: `n<line>:<path>` (new side) or `o<line>:<path>`.
fn thread_key(c: &Value) -> Option<(String, Position)> {
    let path = os(c, "path").filter(|p| !p.is_empty())?;
    let new = u(c, "position") as u32;
    let old = u(c, "original_position") as u32;
    let (id, pos) = if new > 0 {
        (
            format!("n{new}:{path}"),
            Position {
                path: path.clone(),
                old_path: None,
                new_line: Some(new),
                old_line: None,
            },
        )
    } else {
        (
            format!("o{old}:{path}"),
            Position {
                path: path.clone(),
                old_path: None,
                new_line: None,
                old_line: Some(old),
            },
        )
    };
    Some((id, pos))
}

fn parse_thread_id(id: &str) -> Option<(bool, u32, &str)> {
    let (side_line, path) = id.split_once(':')?;
    let new = side_line.starts_with('n');
    if !new && !side_line.starts_with('o') {
        return None;
    }
    Some((new, side_line[1..].parse().ok()?, path))
}

#[async_trait]
impl CodeHost for Forgejo {
    fn kind(&self) -> HostKind {
        HostKind::Forgejo
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            resolve_threads: false,
            approvals: true,
            draft: true,
            merge_methods: vec![MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase],
            request_name: "Pull Request".into(),
            number_prefix: "#".into(),
        }
    }

    async fn current_user(&self) -> HostResult<User> {
        let v: Value = self.http.get("user", "user").await?;
        Ok(user(&v))
    }

    async fn repositories(&self) -> HostResult<Vec<HostRepository>> {
        let list: Vec<Value> = self
            .http
            .get_all("user/repos?limit=50", "repositories", REPO_PAGES)
            .await?;
        let mut repos: Vec<HostRepository> = list
            .iter()
            .map(|v| HostRepository {
                path: s(v, "full_name"),
                name: s(v, "name"),
                description: os(v, "description").filter(|d| !d.is_empty()),
                private: b(v, "private"),
                fork: b(v, "fork"),
                clone_url: s(v, "clone_url"),
                ssh_url: os(v, "ssh_url").filter(|u| !u.is_empty()),
                web_url: s(v, "html_url"),
                updated_at: s(v, "updated_at"),
            })
            .collect();
        repos.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(repos)
    }

    async fn merge_requests(
        &self,
        repo: &RepoRef,
        filter: &MrFilter,
    ) -> HostResult<Page<MergeRequest>> {
        let state = match filter.state {
            StateFilter::Open => "open",
            StateFilter::Closed | StateFilter::Merged => "closed",
            StateFilter::All => "all",
        };
        let path = format!(
            "{}/pulls?state={state}&sort=recentupdate&page={}&limit={}",
            Self::repo_path(repo),
            filter.page(),
            filter.per_page()
        );
        let page = self.http.get_page::<Value>(&path, "repository").await?;
        let selected: Vec<&Value> = page
            .items
            .iter()
            .filter(|v| match filter.state {
                StateFilter::Merged => b(v, "merged"),
                StateFilter::Closed => !b(v, "merged"),
                _ => true,
            })
            .filter(|v| {
                let mr = Self::pull_from(v, None);
                let reviewers: Vec<String> = users(v, "requested_reviewers")
                    .into_iter()
                    .map(|u| u.username)
                    .collect();
                let assignees: Vec<String> = users(v, "assignees")
                    .into_iter()
                    .map(|u| u.username)
                    .collect();
                filter.matches(&mr, &reviewers, &assignees)
            })
            .collect();
        let statuses = join_all(selected.iter().map(|v| async move {
            let sha = v.get("head").map(|h| s(h, "sha")).unwrap_or_default();
            if sha.is_empty() {
                return None;
            }
            match self.combined_status(repo, &sha).await {
                Ok(Some(st)) if !arr(&st, "statuses").is_empty() => {
                    CiStatus::parse(&s(&st, "state"))
                }
                _ => None,
            }
        }))
        .await;
        Ok(Page {
            items: selected
                .iter()
                .zip(statuses)
                .map(|(v, ci)| Self::pull_from(v, ci))
                .collect(),
            next_page: page.next_page,
            total: page.total,
        })
    }

    async fn merge_request(&self, repo: &RepoRef, number: u64) -> HostResult<MergeRequestDetails> {
        let v: Value = self
            .http
            .get(
                &Self::pr_path(repo, number),
                &format!("pull request #{number}"),
            )
            .await?;
        let reviews = self.reviews(repo, number).await.unwrap_or_default();
        // Latest decisive review per user.
        let mut latest: BTreeMap<String, (User, ReviewState)> = BTreeMap::new();
        for r in &reviews {
            let u = user_at(r, "user");
            let state = match s(r, "state").as_str() {
                "APPROVED" => ReviewState::Approved,
                "REQUEST_CHANGES" => ReviewState::ChangesRequested,
                "REQUEST_REVIEW" => ReviewState::Requested,
                "COMMENT" => ReviewState::Commented,
                _ => continue,
            };
            if b(r, "dismissed") || b(r, "stale") && state == ReviewState::Approved {
                continue;
            }
            let keep_old = state == ReviewState::Commented
                && latest
                    .get(&u.username)
                    .is_some_and(|(_, s)| *s != ReviewState::Commented);
            if !keep_old {
                latest.insert(u.username.clone(), (u, state));
            }
        }
        for u in users(&v, "requested_reviewers") {
            latest
                .entry(u.username.clone())
                .or_insert((u, ReviewState::Requested));
        }
        let author = user_at(&v, "user").username;
        let reviewers: Vec<Reviewer> = latest
            .into_values()
            .filter(|(u, _)| u.username != author)
            .map(|(user, state)| Reviewer { user, state })
            .collect();
        let approved_by = reviewers
            .iter()
            .filter(|r| r.state == ReviewState::Approved)
            .map(|r| r.user.clone())
            .collect();
        let head_sha = v.get("head").map(|h| s(h, "sha")).unwrap_or_default();
        let pipeline = if head_sha.is_empty() {
            None
        } else {
            self.ci_status(repo, &head_sha).await.unwrap_or(None)
        };
        let mergeable = v.get("mergeable").and_then(Value::as_bool);
        let mut summary = Self::pull_from(&v, pipeline.as_ref().map(|p| p.status));
        if summary.comments.is_none() {
            summary.comments = Some(0);
        }
        let base_sha = os(&v, "merge_base").or_else(|| v.get("base").and_then(|b| os(b, "sha")));
        Ok(MergeRequestDetails {
            summary,
            description: s(&v, "body"),
            reviewers,
            assignees: users(&v, "assignees"),
            approvals: Approvals {
                required: 0,
                approved_by,
            },
            pipeline,
            mergeable,
            conflicts: mergeable == Some(false) && !b(&v, "merged"),
            start_sha: base_sha.clone(),
            base_sha,
            merged_by: v.get("merged_by").filter(|u| u.is_object()).map(user),
        })
    }

    async fn merge_request_files(
        &self,
        repo: &RepoRef,
        number: u64,
    ) -> HostResult<Vec<ChangedFile>> {
        let diff = self
            .http
            .get_text(
                &format!("{}.diff", Self::pr_path(repo, number)),
                Some("text/plain"),
            )
            .await?
            .ok_or_else(|| HostError::NotFound {
                host: "Forgejo".into(),
                what: format!("pull request #{number}"),
            })?;
        Ok(forge_vcs::parse_diff(&diff)
            .into_iter()
            .map(|f| {
                let (additions, deletions) = count_changes(&f.hunks);
                let status = if f.new_file {
                    ChangeKind::Added
                } else if f.deleted_file {
                    ChangeKind::Deleted
                } else if f.old_path.is_some() {
                    ChangeKind::Renamed
                } else {
                    ChangeKind::Modified
                };
                ChangedFile {
                    path: f.path,
                    old_path: f.old_path,
                    status,
                    additions,
                    deletions,
                    hunks: f.hunks,
                }
            })
            .collect())
    }

    async fn discussions(&self, repo: &RepoRef, number: u64) -> HostResult<Vec<Discussion>> {
        let comments: Vec<Value> = self
            .http
            .get_all(
                &format!(
                    "{}/issues/{number}/comments?limit=50",
                    Self::repo_path(repo)
                ),
                "comments",
                10,
            )
            .await?;
        let mut out: Vec<Discussion> = comments
            .iter()
            .map(|c| Discussion {
                id: format!("c{}", s(c, "id")),
                notes: vec![Self::note_from(c)],
                position: None,
                resolvable: false,
                resolved: false,
            })
            .collect();

        let reviews = self.reviews(repo, number).await?;
        let pr = Self::pr_path(repo, number);
        let review_comments = join_all(reviews.iter().filter(|r| u(r, "comments_count") > 0).map(
            |r| {
                let path = format!("{pr}/reviews/{}/comments", s(r, "id"));
                async move {
                    self.http
                        .get::<Vec<Value>>(&path, "review comments")
                        .await
                        .unwrap_or_default()
                }
            },
        ))
        .await;

        // Review summaries ("approved", "requested changes") as timeline entries.
        for r in &reviews {
            let state = s(r, "state");
            let verb = match state.as_str() {
                "APPROVED" => "approved these changes",
                "REQUEST_CHANGES" => "requested changes",
                "COMMENT" if !s(r, "body").trim().is_empty() => "reviewed",
                _ => continue,
            };
            let mut note = Self::note_from(r);
            let body = s(r, "body");
            note.system = body.trim().is_empty();
            note.body = if note.system { verb.to_string() } else { body };
            note.id = format!("r{}", s(r, "id"));
            out.push(Discussion {
                id: note.id.clone(),
                notes: vec![note],
                position: None,
                resolvable: false,
                resolved: false,
            });
        }

        let mut threads: HashMap<String, Discussion> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        for c in review_comments.iter().flatten() {
            let Some((id, position)) = thread_key(c) else {
                continue;
            };
            let resolved = c.get("resolver").is_some_and(Value::is_object);
            let entry = threads.entry(id.clone()).or_insert_with(|| {
                order.push(id.clone());
                Discussion {
                    id: id.clone(),
                    notes: Vec::new(),
                    position: Some(position),
                    resolvable: true,
                    resolved: false,
                }
            });
            entry.resolved |= resolved;
            entry.notes.push(Self::note_from(c));
        }
        for id in order {
            if let Some(mut d) = threads.remove(&id) {
                d.notes.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                out.push(d);
            }
        }
        out.sort_by(|a, b| {
            let ka = a.notes.first().map(|n| n.created_at.as_str()).unwrap_or("");
            let kb = b.notes.first().map(|n| n.created_at.as_str()).unwrap_or("");
            ka.cmp(kb)
        });
        Ok(out)
    }

    async fn create_merge_request(
        &self,
        repo: &RepoRef,
        new: &NewMergeRequest,
    ) -> HostResult<MergeRequest> {
        let labels = self.label_ids(repo, &new.labels).await?;
        let title = if new.draft {
            format!("WIP: {}", new.title)
        } else {
            new.title.clone()
        };
        let v: Value = self
            .http
            .send_json(
                Method::POST,
                &format!("{}/pulls", Self::repo_path(repo)),
                &json!({
                    "head": new.source_branch,
                    "base": new.target_branch,
                    "title": title,
                    "body": new.description,
                    "labels": labels,
                }),
                "repository",
            )
            .await?;
        let number = u(&v, "number");
        if !new.reviewers.is_empty() {
            self.http
                .send_unit(
                    Method::POST,
                    &format!("{}/requested_reviewers", Self::pr_path(repo, number)),
                    Some(&json!({ "reviewers": new.reviewers })),
                    "pull request",
                )
                .await?;
        }
        Ok(Self::pull_from(&v, None))
    }

    async fn add_comment(
        &self,
        repo: &RepoRef,
        number: u64,
        body: &str,
        position: Option<&Position>,
    ) -> HostResult<()> {
        match position {
            Some(p) => {
                self.inline_comment(repo, number, &p.path, p.new_line, p.old_line, body)
                    .await
            }
            None => self.comment_issue(repo, number, body).await,
        }
    }

    async fn reply(
        &self,
        repo: &RepoRef,
        number: u64,
        discussion_id: &str,
        body: &str,
    ) -> HostResult<()> {
        match parse_thread_id(discussion_id) {
            Some((true, line, path)) => {
                self.inline_comment(repo, number, path, Some(line), None, body)
                    .await
            }
            Some((false, line, path)) => {
                self.inline_comment(repo, number, path, None, Some(line), body)
                    .await
            }
            None => self.comment_issue(repo, number, body).await,
        }
    }

    async fn resolve(&self, _: &RepoRef, _: u64, _: &str, _: bool) -> HostResult<()> {
        Err(HostError::Unsupported {
            host: "Forgejo".into(),
            what: "resolving conversations through its API".into(),
        })
    }

    async fn approve(&self, repo: &RepoRef, number: u64) -> HostResult<()> {
        let commit = self.head_sha(repo, number).await?;
        self.http
            .send_unit(
                Method::POST,
                &format!("{}/reviews", Self::pr_path(repo, number)),
                Some(&json!({ "event": "APPROVED", "body": "", "commit_id": commit })),
                "pull request",
            )
            .await
    }

    async fn merge(&self, repo: &RepoRef, number: u64, opts: &MergeOptions) -> HostResult<()> {
        let mut body = json!({
            "Do": match opts.method {
                MergeMethod::Merge => "merge",
                MergeMethod::Squash => "squash",
                MergeMethod::Rebase => "rebase",
            },
            "delete_branch_after_merge": opts.delete_source_branch,
        });
        if let Some(m) = opts
            .commit_message
            .as_deref()
            .filter(|m| !m.trim().is_empty())
        {
            let (title, rest) = m.split_once('\n').unwrap_or((m, ""));
            body["MergeTitleField"] = json!(title.trim());
            body["MergeMessageField"] = json!(rest.trim());
        }
        self.http
            .send_unit(
                Method::POST,
                &format!("{}/merge", Self::pr_path(repo, number)),
                Some(&body),
                "pull request",
            )
            .await
    }

    async fn close(&self, repo: &RepoRef, number: u64) -> HostResult<()> {
        self.http
            .send_unit(
                Method::PATCH,
                &Self::pr_path(repo, number),
                Some(&json!({ "state": "closed" })),
                "pull request",
            )
            .await
    }

    async fn issues(&self, repo: &RepoRef, filter: &IssueFilter) -> HostResult<Page<Issue>> {
        let state = match filter.state {
            IssueStateFilter::Open => "open",
            IssueStateFilter::Closed => "closed",
            IssueStateFilter::All => "all",
        };
        let q = query(&[
            ("type", Some("issues".into())),
            ("state", Some(state.into())),
            ("q", filter.search.clone()),
            ("labels", filter.label.clone()),
            ("created_by", filter.author.clone()),
            ("assigned_by", filter.assignee.clone()),
            ("page", Some(filter.page().to_string())),
            ("limit", Some(filter.per_page().to_string())),
        ]);
        let page = self
            .http
            .get_page::<Value>(
                &format!("{}/issues{q}", Self::repo_path(repo)),
                "repository",
            )
            .await?;
        Ok(Page {
            items: page
                .items
                .iter()
                .filter(|v| v.get("pull_request").is_none_or(Value::is_null))
                .map(issue_from)
                .collect(),
            next_page: page.next_page,
            total: page.total,
        })
    }

    async fn issue(&self, repo: &RepoRef, number: u64) -> HostResult<IssueDetails> {
        let path = format!("{}/issues/{number}", Self::repo_path(repo));
        let v: Value = self.http.get(&path, &format!("issue #{number}")).await?;
        let comments: Vec<Value> = self
            .http
            .get_all(&format!("{path}/comments?limit=50"), "comments", 10)
            .await?;
        Ok(IssueDetails {
            summary: issue_from(&v),
            description: s(&v, "body"),
            notes: comments.iter().map(Self::note_from).collect(),
        })
    }

    async fn comment_issue(&self, repo: &RepoRef, number: u64, body: &str) -> HostResult<()> {
        self.http
            .send_unit(
                Method::POST,
                &format!("{}/issues/{number}/comments", Self::repo_path(repo)),
                Some(&json!({ "body": body })),
                "issue",
            )
            .await
    }

    async fn pipelines(&self, repo: &RepoRef, branch: Option<&str>) -> HostResult<Vec<Pipeline>> {
        let res: HostResult<Option<Value>> = self
            .http
            .get_optional(
                &format!("{}/actions/tasks?limit=30", Self::repo_path(repo)),
                "workflow runs",
            )
            .await;
        match res {
            Ok(Some(v)) => Ok(arr(&v, "workflow_runs")
                .iter()
                .filter(|r| branch.is_none_or(|b| s(r, "head_branch") == b))
                .filter_map(|r| {
                    Some(Pipeline {
                        id: s(r, "id"),
                        name: os(r, "display_title").or_else(|| os(r, "name")),
                        status: CiStatus::parse(&s(r, "status"))?,
                        git_ref: os(r, "head_branch"),
                        sha: os(r, "head_sha"),
                        web_url: os(r, "url"),
                        created_at: os(r, "created_at"),
                        jobs: Vec::new(),
                    })
                })
                .collect()),
            // Older servers without the Actions API: fall back to commit statuses.
            Ok(None) | Err(HostError::Forbidden { .. }) => match branch {
                Some(b) => Ok(self.ci_status(repo, b).await?.into_iter().collect()),
                None => Ok(Vec::new()),
            },
            Err(e) => Err(e),
        }
    }

    async fn ci_status(&self, repo: &RepoRef, git_ref: &str) -> HostResult<Option<Pipeline>> {
        Ok(self
            .combined_status(repo, git_ref)
            .await?
            .and_then(|v| Self::pipeline_from_status(&v, git_ref)))
    }

    async fn members(&self, repo: &RepoRef) -> HostResult<Vec<User>> {
        let list: Vec<Value> = self
            .http
            .get_all(
                &format!("{}/collaborators?limit=50", Self::repo_path(repo)),
                "collaborators",
                3,
            )
            .await?;
        Ok(list.iter().map(user).collect())
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
                    "{}/raw/{}?ref={}",
                    Self::repo_path(repo),
                    enc_path(path),
                    enc(git_ref)
                ),
                Some("*/*"),
            )
            .await
    }
}

fn issue_from(v: &Value) -> Issue {
    Issue {
        number: u(v, "number"),
        title: s(v, "title"),
        state: if s(v, "state") == "closed" {
            IssueState::Closed
        } else {
            IssueState::Open
        },
        author: user_at(v, "user"),
        labels: labels(v, "labels"),
        assignees: users(v, "assignees"),
        comments: u(v, "comments") as u32,
        created_at: s(v, "created_at"),
        updated_at: s(v, "updated_at"),
        web_url: s(v, "html_url"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_ids_round_trip() {
        let c = json!({ "path": "app/Models/User.php", "position": 42, "original_position": 40 });
        let (id, pos) = thread_key(&c).unwrap();
        assert_eq!(id, "n42:app/Models/User.php");
        assert_eq!(pos.new_line, Some(42));
        assert_eq!(
            parse_thread_id(&id),
            Some((true, 42, "app/Models/User.php"))
        );
        let c = json!({ "path": "a:b.txt", "position": 0, "original_position": 7 });
        let (id, _) = thread_key(&c).unwrap();
        assert_eq!(parse_thread_id(&id), Some((false, 7, "a:b.txt")));
        assert_eq!(parse_thread_id("c123"), None);
    }
}
