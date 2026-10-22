//! GitHub REST API (github.com and GitHub Enterprise Server, `Authorization: Bearer`).
//! Review threads are rebuilt from `in_reply_to_id`; their resolved state and resolving
//! use the GraphQL API, which REST does not cover.

use async_trait::async_trait;
use futures_util::future::join_all;
use reqwest::Method;
use serde_json::{json, Value};

use crate::error::{HostError, HostResult};
use crate::host::{count_changes, parse_patch, CodeHost};
use crate::http::{enc, enc_path, query, Auth, Http, REPO_PAGES};
use crate::json::*;
use crate::model::*;

pub struct GitHub {
    http: Http,
    graphql: String,
}

impl GitHub {
    /// `api_base` is `https://api.github.com` or `https://ghe.example.com/api/v3`.
    pub fn new(client: reqwest::Client, api_base: &str, token: Option<String>) -> Self {
        let auth = token.map(Auth::Bearer).unwrap_or(Auth::None);
        let base = api_base.trim_end_matches('/');
        let graphql = match base.strip_suffix("/api/v3") {
            Some(server) => format!("{server}/api/graphql"),
            None => format!("{base}/graphql"),
        };
        Self {
            http: Http::new(client, base, auth, "GitHub", "application/vnd.github+json"),
            graphql,
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
        let head = v.get("head").cloned().unwrap_or(Value::Null);
        let base = v.get("base").cloned().unwrap_or(Value::Null);
        let merged = b(v, "merged") || v.get("merged_at").is_some_and(|m| !m.is_null());
        let comments = match (ou(v, "comments"), ou(v, "review_comments")) {
            (None, None) => None,
            (a, b) => Some((a.unwrap_or(0) + b.unwrap_or(0)) as u32),
        };
        MergeRequest {
            number: u(v, "number"),
            title: s(v, "title"),
            state: if merged {
                MrState::Merged
            } else if s(v, "state") == "closed" {
                MrState::Closed
            } else {
                MrState::Open
            },
            draft: b(v, "draft"),
            author: user_at(v, "user"),
            source_branch: s(&head, "ref"),
            target_branch: s(&base, "ref"),
            labels: labels(v, "labels"),
            comments,
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

    fn run_status(v: &Value) -> Option<CiStatus> {
        match s(v, "status").as_str() {
            "completed" => CiStatus::parse(&s(v, "conclusion")),
            other => CiStatus::parse(other),
        }
    }

    async fn check_runs(&self, repo: &RepoRef, git_ref: &str) -> HostResult<Vec<Job>> {
        let v: Value = self
            .http
            .get(
                &format!(
                    "{}/commits/{}/check-runs?per_page=100",
                    Self::repo_path(repo),
                    enc(git_ref)
                ),
                "check runs",
            )
            .await?;
        Ok(arr(&v, "check_runs")
            .iter()
            .filter_map(|r| {
                Some(Job {
                    name: s(r, "name"),
                    stage: r
                        .get("app")
                        .and_then(|a| os(a, "name"))
                        .filter(|n| n != "GitHub Actions"),
                    status: Self::run_status(r)?,
                    web_url: os(r, "html_url"),
                    duration: None,
                })
            })
            .collect())
    }

    async fn graphql(&self, query: &str, variables: Value) -> HostResult<Value> {
        let v: Value = self
            .http
            .send_json(
                Method::POST,
                &self.graphql,
                &json!({ "query": query, "variables": variables }),
                "GraphQL API",
            )
            .await?;
        if let Some(err) = arr(&v, "errors").first() {
            return Err(HostError::Api {
                host: "GitHub".into(),
                message: s(err, "message"),
            });
        }
        Ok(v.get("data").cloned().unwrap_or(Value::Null))
    }

    /// Review threads: root comment database id -> (thread node id, resolved).
    async fn review_threads(
        &self,
        repo: &RepoRef,
        number: u64,
    ) -> HostResult<Vec<(u64, String, bool)>> {
        let (owner, name) = repo.owner_repo();
        let data = self
            .graphql(
                "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){reviewThreads(first:100){nodes{id isResolved comments(first:1){nodes{databaseId}}}}}}}",
                json!({ "owner": owner, "name": name, "number": number }),
            )
            .await?;
        let nodes = data
            .pointer("/repository/pullRequest/reviewThreads/nodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(nodes
            .iter()
            .filter_map(|n| {
                let root = n.pointer("/comments/nodes/0/databaseId")?.as_u64()?;
                Some((root, s(n, "id"), b(n, "isResolved")))
            })
            .collect())
    }
}

#[async_trait]
impl CodeHost for GitHub {
    fn kind(&self) -> HostKind {
        HostKind::Github
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            resolve_threads: true,
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
            .get_all(
                "user/repos?per_page=100&sort=updated&affiliation=owner,collaborator,organization_member",
                "repositories",
                REPO_PAGES,
            )
            .await?;
        Ok(list
            .iter()
            .map(|v| HostRepository {
                path: s(v, "full_name"),
                name: s(v, "name"),
                description: os(v, "description").filter(|d| !d.is_empty()),
                private: b(v, "private"),
                fork: b(v, "fork"),
                clone_url: s(v, "clone_url"),
                ssh_url: os(v, "ssh_url"),
                web_url: s(v, "html_url"),
                updated_at: os(v, "pushed_at").unwrap_or_else(|| s(v, "updated_at")),
            })
            .collect())
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
            "{}/pulls?state={state}&sort=updated&direction=desc&page={}&per_page={}",
            Self::repo_path(repo),
            filter.page(),
            filter.per_page()
        );
        let page = self.http.get_page::<Value>(&path, "repository").await?;
        let selected: Vec<&Value> = page
            .items
            .iter()
            .filter(|v| {
                let merged = v.get("merged_at").is_some_and(|m| !m.is_null());
                match filter.state {
                    StateFilter::Merged => merged,
                    StateFilter::Closed => !merged,
                    _ => true,
                }
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
            let jobs = self.check_runs(repo, &sha).await.ok()?;
            CiStatus::combine(jobs.iter().map(|j| j.status))
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
        let path = Self::pr_path(repo, number);
        let v: Value = self
            .http
            .get(&path, &format!("pull request #{number}"))
            .await?;
        let reviews: Vec<Value> = self
            .http
            .get_all(&format!("{path}/reviews?per_page=100"), "reviews", 5)
            .await
            .unwrap_or_default();
        let author = user_at(&v, "user").username;
        let mut reviewers: Vec<Reviewer> = Vec::new();
        for r in &reviews {
            let state = match s(r, "state").as_str() {
                "APPROVED" => ReviewState::Approved,
                "CHANGES_REQUESTED" => ReviewState::ChangesRequested,
                "COMMENTED" => ReviewState::Commented,
                "DISMISSED" => ReviewState::Requested,
                _ => continue,
            };
            let user = user_at(r, "user");
            if user.username == author {
                continue;
            }
            match reviewers
                .iter_mut()
                .find(|x| x.user.username == user.username)
            {
                Some(existing) => {
                    if state != ReviewState::Commented || existing.state == ReviewState::Commented {
                        existing.state = state;
                    }
                }
                None => reviewers.push(Reviewer { user, state }),
            }
        }
        for u in users(&v, "requested_reviewers") {
            match reviewers.iter_mut().find(|x| x.user.username == u.username) {
                // Re-requested after a review.
                Some(existing) => existing.state = ReviewState::Requested,
                None => reviewers.push(Reviewer {
                    user: u,
                    state: ReviewState::Requested,
                }),
            }
        }
        let head_sha = v.get("head").map(|h| s(h, "sha")).unwrap_or_default();
        let pipeline = if head_sha.is_empty() {
            None
        } else {
            self.ci_status(repo, &head_sha).await.unwrap_or(None)
        };
        let base_sha = v.get("base").and_then(|b| os(b, "sha"));
        let mergeable = v.get("mergeable").and_then(Value::as_bool);
        Ok(MergeRequestDetails {
            summary: Self::pull_from(&v, pipeline.as_ref().map(|p| p.status)),
            description: s(&v, "body"),
            approvals: Approvals {
                required: 0,
                approved_by: reviewers
                    .iter()
                    .filter(|r| r.state == ReviewState::Approved)
                    .map(|r| r.user.clone())
                    .collect(),
            },
            reviewers,
            assignees: users(&v, "assignees"),
            pipeline,
            mergeable,
            conflicts: s(&v, "mergeable_state") == "dirty",
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
        let files: Vec<Value> = self
            .http
            .get_all(
                &format!("{}/files?per_page=100", Self::pr_path(repo, number)),
                "files",
                30,
            )
            .await?;
        Ok(files
            .iter()
            .map(|f| {
                let path = s(f, "filename");
                let hunks = parse_patch(&path, &s(f, "patch"));
                let (add, del) = count_changes(&hunks);
                ChangedFile {
                    old_path: os(f, "previous_filename"),
                    status: match s(f, "status").as_str() {
                        "added" => ChangeKind::Added,
                        "removed" => ChangeKind::Deleted,
                        "renamed" => ChangeKind::Renamed,
                        _ => ChangeKind::Modified,
                    },
                    additions: ou(f, "additions").map(|n| n as u32).unwrap_or(add),
                    deletions: ou(f, "deletions").map(|n| n as u32).unwrap_or(del),
                    path,
                    hunks,
                }
            })
            .collect())
    }

    async fn discussions(&self, repo: &RepoRef, number: u64) -> HostResult<Vec<Discussion>> {
        let issue_comments: Vec<Value> = self
            .http
            .get_all(
                &format!(
                    "{}/issues/{number}/comments?per_page=100",
                    Self::repo_path(repo)
                ),
                "comments",
                10,
            )
            .await?;
        let review_comments: Vec<Value> = self
            .http
            .get_all(
                &format!("{}/comments?per_page=100", Self::pr_path(repo, number)),
                "review comments",
                10,
            )
            .await?;
        let reviews: Vec<Value> = self
            .http
            .get_all(
                &format!("{}/reviews?per_page=100", Self::pr_path(repo, number)),
                "reviews",
                5,
            )
            .await
            .unwrap_or_default();
        let threads = self.review_threads(repo, number).await.unwrap_or_default();

        let mut out: Vec<Discussion> = issue_comments
            .iter()
            .map(|c| Discussion {
                id: format!("c{}", s(c, "id")),
                notes: vec![Self::note_from(c)],
                position: None,
                resolvable: false,
                resolved: false,
            })
            .collect();
        for r in &reviews {
            let body = s(r, "body");
            let verb = match s(r, "state").as_str() {
                "APPROVED" => "approved these changes",
                "CHANGES_REQUESTED" => "requested changes",
                "COMMENTED" if !body.trim().is_empty() => "reviewed",
                _ => continue,
            };
            let mut note = Self::note_from(r);
            note.system = body.trim().is_empty();
            note.body = if note.system { verb.into() } else { body };
            out.push(Discussion {
                id: format!("r{}", s(r, "id")),
                notes: vec![note],
                position: None,
                resolvable: false,
                resolved: false,
            });
        }
        for root in review_comments
            .iter()
            .filter(|c| ou(c, "in_reply_to_id").is_none())
        {
            let id = u(root, "id");
            let line = ou(root, "line").or_else(|| ou(root, "original_line"));
            let left = s(root, "side") == "LEFT";
            let mut notes = vec![Self::note_from(root)];
            notes.extend(
                review_comments
                    .iter()
                    .filter(|c| ou(c, "in_reply_to_id") == Some(id))
                    .map(Self::note_from),
            );
            let resolved = threads
                .iter()
                .find(|(r, _, _)| *r == id)
                .is_some_and(|(_, _, res)| *res);
            out.push(Discussion {
                id: id.to_string(),
                notes,
                position: Some(Position {
                    path: s(root, "path"),
                    old_path: None,
                    new_line: if left { None } else { line.map(|l| l as u32) },
                    old_line: if left { line.map(|l| l as u32) } else { None },
                }),
                resolvable: true,
                resolved,
            });
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
        let v: Value = self
            .http
            .send_json(
                Method::POST,
                &format!("{}/pulls", Self::repo_path(repo)),
                &json!({
                    "title": new.title,
                    "head": new.source_branch,
                    "base": new.target_branch,
                    "body": new.description,
                    "draft": new.draft,
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
        if !new.labels.is_empty() {
            self.http
                .send_unit(
                    Method::POST,
                    &format!("{}/issues/{number}/labels", Self::repo_path(repo)),
                    Some(&json!({ "labels": new.labels })),
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
        let Some(p) = position else {
            return self.comment_issue(repo, number, body).await;
        };
        let pr: Value = self
            .http
            .get(&Self::pr_path(repo, number), "pull request")
            .await?;
        let (line, side) = match (p.new_line, p.old_line) {
            (Some(n), _) => (n, "RIGHT"),
            (None, Some(o)) => (o, "LEFT"),
            (None, None) => {
                return Err(HostError::Api {
                    host: "GitHub".into(),
                    message: "an inline comment needs a line".into(),
                })
            }
        };
        self.http
            .send_unit(
                Method::POST,
                &format!("{}/comments", Self::pr_path(repo, number)),
                Some(&json!({
                    "body": body,
                    "commit_id": pr.get("head").map(|h| s(h, "sha")).unwrap_or_default(),
                    "path": p.path,
                    "line": line,
                    "side": side,
                })),
                "pull request",
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
        if discussion_id.starts_with(['c', 'r']) {
            return self.comment_issue(repo, number, body).await;
        }
        self.http
            .send_unit(
                Method::POST,
                &format!(
                    "{}/comments/{}/replies",
                    Self::pr_path(repo, number),
                    enc(discussion_id)
                ),
                Some(&json!({ "body": body })),
                "review comment",
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
        let root: u64 = discussion_id.parse().map_err(|_| HostError::Unsupported {
            host: "GitHub".into(),
            what: "resolving general comments".into(),
        })?;
        let thread = self
            .review_threads(repo, number)
            .await?
            .into_iter()
            .find(|(r, _, _)| *r == root)
            .ok_or_else(|| HostError::NotFound {
                host: "GitHub".into(),
                what: "review thread".into(),
            })?;
        let mutation = if resolved {
            "mutation($id:ID!){resolveReviewThread(input:{threadId:$id}){thread{isResolved}}}"
        } else {
            "mutation($id:ID!){unresolveReviewThread(input:{threadId:$id}){thread{isResolved}}}"
        };
        self.graphql(mutation, json!({ "id": thread.1 }))
            .await
            .map(drop)
    }

    async fn approve(&self, repo: &RepoRef, number: u64) -> HostResult<()> {
        self.http
            .send_unit(
                Method::POST,
                &format!("{}/reviews", Self::pr_path(repo, number)),
                Some(&json!({ "event": "APPROVE" })),
                "pull request",
            )
            .await
    }

    async fn merge(&self, repo: &RepoRef, number: u64, opts: &MergeOptions) -> HostResult<()> {
        let mut body = json!({
            "merge_method": match opts.method {
                MergeMethod::Merge => "merge",
                MergeMethod::Squash => "squash",
                MergeMethod::Rebase => "rebase",
            },
        });
        if let Some(m) = opts
            .commit_message
            .as_deref()
            .filter(|m| !m.trim().is_empty())
        {
            let (title, rest) = m.split_once('\n').unwrap_or((m, ""));
            body["commit_title"] = json!(title.trim());
            body["commit_message"] = json!(rest.trim());
        }
        self.http
            .send_unit(
                Method::PUT,
                &format!("{}/merge", Self::pr_path(repo, number)),
                Some(&body),
                "pull request",
            )
            .await?;
        if opts.delete_source_branch {
            let pr: Value = self
                .http
                .get(&Self::pr_path(repo, number), "pull request")
                .await?;
            let head = pr.get("head").cloned().unwrap_or(Value::Null);
            let same_repo = head
                .pointer("/repo/full_name")
                .and_then(Value::as_str)
                .is_none_or(|n| n.eq_ignore_ascii_case(&repo.path));
            if same_repo {
                self.http
                    .send_unit(
                        Method::DELETE,
                        &format!(
                            "{}/git/refs/heads/{}",
                            Self::repo_path(repo),
                            enc_path(&s(&head, "ref"))
                        ),
                        None,
                        "branch",
                    )
                    .await?;
            }
        }
        Ok(())
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
        if let Some(text) = filter.search.as_deref().filter(|t| !t.trim().is_empty()) {
            let mut q = format!("repo:{} is:issue {text}", repo.path);
            if filter.state != IssueStateFilter::All {
                q.push_str(&format!(" state:{state}"));
            }
            for (k, v) in [
                ("author", &filter.author),
                ("assignee", &filter.assignee),
                ("label", &filter.label),
            ] {
                if let Some(v) = v.as_deref().filter(|v| !v.is_empty()) {
                    q.push_str(&format!(" {k}:\"{v}\""));
                }
            }
            let path = format!(
                "search/issues?q={}&sort=updated&page={}&per_page={}",
                enc(&q),
                filter.page(),
                filter.per_page()
            );
            let v: Value = self.http.get(&path, "issues").await?;
            let items: Vec<Issue> = arr(&v, "items").iter().map(issue_from).collect();
            let total = ou(&v, "total_count");
            let seen = u64::from(filter.page() * filter.per_page());
            let next_page = total.filter(|t| *t > seen).map(|_| filter.page() + 1);
            return Ok(Page {
                items,
                next_page,
                total,
            });
        }
        let q = query(&[
            ("state", Some(state.into())),
            ("creator", filter.author.clone()),
            ("assignee", filter.assignee.clone()),
            ("labels", filter.label.clone()),
            ("sort", Some("updated".into())),
            ("page", Some(filter.page().to_string())),
            ("per_page", Some(filter.per_page().to_string())),
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
                .filter(|v| v.get("pull_request").is_none())
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
            .get_all(&format!("{path}/comments?per_page=100"), "comments", 10)
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
        let q = query(&[
            ("branch", branch.map(str::to_string)),
            ("per_page", Some("20".into())),
        ]);
        let v: Value = self
            .http
            .get(
                &format!("{}/actions/runs{q}", Self::repo_path(repo)),
                "workflow runs",
            )
            .await?;
        Ok(arr(&v, "workflow_runs")
            .iter()
            .filter_map(|r| {
                Some(Pipeline {
                    id: s(r, "id"),
                    name: os(r, "display_title").or_else(|| os(r, "name")),
                    status: Self::run_status(r)?,
                    git_ref: os(r, "head_branch"),
                    sha: os(r, "head_sha"),
                    web_url: os(r, "html_url"),
                    created_at: os(r, "created_at"),
                    jobs: Vec::new(),
                })
            })
            .collect())
    }

    async fn ci_status(&self, repo: &RepoRef, git_ref: &str) -> HostResult<Option<Pipeline>> {
        let mut jobs = self.check_runs(repo, git_ref).await?;
        let combined: Option<Value> = self
            .http
            .get_optional(
                &format!("{}/commits/{}/status", Self::repo_path(repo), enc(git_ref)),
                "commit status",
            )
            .await
            .unwrap_or(None);
        if let Some(c) = &combined {
            jobs.extend(arr(c, "statuses").iter().filter_map(|st| {
                Some(Job {
                    name: s(st, "context"),
                    stage: os(st, "description").filter(|d| !d.is_empty()),
                    status: CiStatus::parse(&s(st, "state"))?,
                    web_url: os(st, "target_url"),
                    duration: None,
                })
            }));
        }
        let Some(status) = CiStatus::combine(jobs.iter().map(|j| j.status)) else {
            return Ok(None);
        };
        Ok(Some(Pipeline {
            id: combined
                .as_ref()
                .and_then(|c| os(c, "sha"))
                .unwrap_or_else(|| git_ref.to_string()),
            name: None,
            status,
            git_ref: Some(git_ref.to_string()),
            sha: combined.as_ref().and_then(|c| os(c, "sha")),
            web_url: jobs.iter().find_map(|j| j.web_url.clone()),
            created_at: None,
            jobs,
        }))
    }

    async fn members(&self, repo: &RepoRef) -> HostResult<Vec<User>> {
        let list: Vec<Value> = self
            .http
            .get_all(
                &format!("{}/collaborators?per_page=100", Self::repo_path(repo)),
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
                    "{}/contents/{}?ref={}",
                    Self::repo_path(repo),
                    enc_path(path),
                    enc(git_ref)
                ),
                Some("application/vnd.github.raw"),
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
