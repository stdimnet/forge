//! Azure DevOps Services and Azure DevOps Server (REST `api-version=7.1`, Basic
//! authentication with a personal access token). The account's server is the organization
//! (`https://dev.azure.com/acme`) or the collection (`https://tfs.acme.dev/tfs/DefaultCollection`);
//! a repository's path is `project/repository`.
//!
//! Pull requests, their threads, votes and completion map onto merge requests; work items
//! (found with WIQL) are the issues; builds are the pipelines. The REST API has no patches,
//! so changed files are diffed here from the merge base and the source commit.

use async_trait::async_trait;
use futures_util::future::join_all;
use reqwest::Method;
use serde_json::{json, Value};

use crate::error::{HostError, HostResult};
use crate::host::{count_changes, parse_patch, CodeHost};
use crate::http::{enc, Auth, Http};
use crate::json::*;
use crate::model::*;

const API: &str = "api-version=7.1";
const COMMENTS_API: &str = "api-version=7.1-preview.4";
/// Files of a pull request whose contents are fetched for the diff.
const MAX_DIFF_FILES: usize = 300;

pub struct AzureDevOps {
    http: Http,
}

/// An Azure DevOps identity (`uniqueName` is the login, usually an email address).
fn az_user(v: &Value) -> User {
    let username = os(v, "uniqueName")
        .or_else(|| os(v, "displayName"))
        .unwrap_or_default();
    User {
        name: os(v, "displayName").filter(|n| *n != username && !n.is_empty()),
        username,
        avatar_url: None,
    }
}

fn az_user_at(v: &Value, key: &str) -> User {
    v.get(key)
        .filter(|u| u.is_object())
        .map(az_user)
        .unwrap_or_default()
}

fn strip_heads(r: &str) -> String {
    r.strip_prefix("refs/heads/").unwrap_or(r).to_string()
}

fn is_sha(r: &str) -> bool {
    r.len() == 40 && r.chars().all(|c| c.is_ascii_hexdigit())
}

/// The `value` array of a list answer.
fn values(v: &Value) -> &[Value] {
    arr(v, "value")
}

/// A build as a pipeline.
fn pipeline_from_build(b: &Value) -> Option<Pipeline> {
    let status = match s(b, "status").as_str() {
        "inProgress" => CiStatus::Running,
        "notStarted" | "postponed" => CiStatus::Pending,
        "cancelling" => CiStatus::Canceled,
        "completed" => match s(b, "result").as_str() {
            "succeeded" => CiStatus::Success,
            "canceled" => CiStatus::Canceled,
            "failed" | "partiallySucceeded" => CiStatus::Failed,
            _ => CiStatus::Skipped,
        },
        _ => return None,
    };
    let definition = b
        .get("definition")
        .map(|d| s(d, "name"))
        .unwrap_or_default();
    Some(Pipeline {
        id: s(b, "id"),
        name: Some(match os(b, "buildNumber") {
            Some(n) if !definition.is_empty() => format!("{definition} {n}"),
            Some(n) => n,
            None => definition,
        }),
        status,
        git_ref: os(b, "sourceBranch").map(|r| strip_heads(&r)),
        sha: os(b, "sourceVersion"),
        web_url: b
            .get("_links")
            .and_then(|l| l.get("web"))
            .and_then(|w| os(w, "href")),
        created_at: os(b, "queueTime"),
        jobs: Vec::new(),
    })
}

/// Closed work item states of the common processes (Agile, Scrum, CMMI, Basic).
fn closed_state(state: &str) -> bool {
    matches!(
        state,
        "Closed" | "Done" | "Removed" | "Resolved" | "Completed"
    )
}

/// Work item HTML (descriptions, comments) as plain text with line breaks.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::new();
    let mut tag = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let t = tag.trim_start_matches('/').to_ascii_lowercase();
                let name = t
                    .split(|c: char| c.is_whitespace() || c == '/')
                    .next()
                    .unwrap_or("");
                if matches!(name, "br" | "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3")
                    && !out.ends_with('\n')
                {
                    out.push('\n');
                }
                if name == "li" && !tag.starts_with('/') {
                    out.push_str("- ");
                }
            }
            _ if in_tag => tag.push(c),
            _ => out.push(c),
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
        .trim()
        .to_string()
}

impl AzureDevOps {
    /// `api_base` is the organization or collection URL (`https://dev.azure.com/acme`).
    pub fn new(client: reqwest::Client, api_base: &str, token: Option<String>) -> Self {
        let auth = token
            .map(|t| Auth::Basic(String::new(), t))
            .unwrap_or(Auth::None);
        Self {
            http: Http::new(client, api_base, auth, "Azure DevOps", "application/json"),
        }
    }

    fn project(repo: &RepoRef) -> String {
        enc(repo.owner_repo().0)
    }

    fn git(repo: &RepoRef) -> String {
        let (project, name) = repo.owner_repo();
        format!("{}/_apis/git/repositories/{}", enc(project), enc(name))
    }

    fn pr(repo: &RepoRef, number: u64) -> String {
        format!("{}/pullrequests/{number}", Self::git(repo))
    }

    fn web(&self, repo: &RepoRef, number: u64) -> String {
        format!("{}/pullrequest/{number}", repo.web_url())
    }

    fn pull_from(&self, repo: &RepoRef, v: &Value) -> MergeRequest {
        let number = u(v, "pullRequestId");
        MergeRequest {
            number,
            title: s(v, "title"),
            state: match s(v, "status").as_str() {
                "completed" => MrState::Merged,
                "abandoned" => MrState::Closed,
                _ => MrState::Open,
            },
            draft: b(v, "isDraft"),
            author: az_user_at(v, "createdBy"),
            source_branch: strip_heads(&s(v, "sourceRefName")),
            target_branch: strip_heads(&s(v, "targetRefName")),
            labels: arr(v, "labels")
                .iter()
                .filter(|l| l.get("active").and_then(Value::as_bool).unwrap_or(true))
                .filter_map(|l| {
                    Some(Label {
                        name: os(l, "name")?,
                        color: None,
                    })
                })
                .collect(),
            comments: None,
            ci: None,
            created_at: s(v, "creationDate"),
            updated_at: os(v, "closedDate").unwrap_or_else(|| s(v, "creationDate")),
            web_url: self.web(repo, number),
            head_sha: v
                .get("lastMergeSourceCommit")
                .and_then(|c| os(c, "commitId")),
        }
    }

    async fn me(&self) -> HostResult<Value> {
        let v: Value = self.http.get("_apis/connectionData", "user").await?;
        Ok(v.get("authenticatedUser").cloned().unwrap_or(Value::Null))
    }

    async fn pr_value(&self, repo: &RepoRef, number: u64) -> HostResult<Value> {
        self.http
            .get(
                &format!("{}?{API}", Self::pr(repo, number)),
                &format!("pull request !{number}"),
            )
            .await
    }

    async fn repo_id(&self, repo: &RepoRef) -> HostResult<String> {
        let v: Value = self
            .http
            .get(&format!("{}?{API}", Self::git(repo)), "repository")
            .await?;
        Ok(s(&v, "id"))
    }

    /// Team members with their identity ids (reviewers are added by id).
    async fn identities(&self, repo: &RepoRef) -> HostResult<Vec<(String, User)>> {
        let project = Self::project(repo);
        let teams: Value = self
            .http
            .get(
                &format!("_apis/projects/{project}/teams?$top=20&{API}"),
                "teams",
            )
            .await?;
        let mut out: Vec<(String, User)> = Vec::new();
        let lists = join_all(values(&teams).iter().take(5).map(|t| {
            let path = format!(
                "_apis/projects/{project}/teams/{}/members?{API}",
                s(t, "id")
            );
            async move { self.http.get::<Value>(&path, "team members").await.ok() }
        }))
        .await;
        for m in lists.iter().flatten().flat_map(values) {
            let identity = m.get("identity").unwrap_or(m);
            let id = s(identity, "id");
            if !id.is_empty() && !out.iter().any(|(x, _)| *x == id) {
                out.push((id, az_user(identity)));
            }
        }
        Ok(out)
    }

    /// A file at a commit or branch through the items API.
    /// A new active thread; with a position it is anchored to a line (or the range from
    /// `start_line`, on the same side).
    async fn post_thread(
        &self,
        repo: &RepoRef,
        number: u64,
        body: &str,
        position: Option<&Position>,
        start_line: Option<u32>,
    ) -> HostResult<()> {
        let mut thread = json!({
            "comments": [{ "parentCommentId": 0, "content": body, "commentType": 1 }],
            "status": 1,
        });
        if let Some(p) = position {
            let mut ctx = json!({ "filePath": format!("/{}", p.path.trim_start_matches('/')) });
            let side = match (p.new_line, p.old_line) {
                (Some(n), _) => Some(("right", n)),
                (None, Some(o)) => Some(("left", o)),
                (None, None) => None,
            };
            if let Some((side, end)) = side {
                let start = start_line.filter(|s| *s < end).unwrap_or(end);
                ctx[format!("{side}FileStart")] = json!({ "line": start, "offset": 1 });
                ctx[format!("{side}FileEnd")] = json!({ "line": end, "offset": 1 });
            }
            thread["threadContext"] = ctx;
        }
        self.http
            .send_unit(
                Method::POST,
                &format!("{}/threads?{API}", Self::pr(repo, number)),
                Some(&thread),
                "pull request",
            )
            .await
    }

    /// Sets the signed-in user's vote (10 approved, -5 waiting for author).
    async fn vote(&self, repo: &RepoRef, number: u64, vote: i32) -> HostResult<()> {
        let me = self.me().await?;
        let id = s(&me, "id");
        if id.is_empty() {
            return Err(HostError::Unauthorized {
                host: "Azure DevOps".into(),
            });
        }
        self.http
            .send_unit(
                Method::PUT,
                &format!("{}/reviewers/{}?{API}", Self::pr(repo, number), enc(&id)),
                Some(&json!({ "vote": vote })),
                "pull request",
            )
            .await
    }

    async fn item(&self, repo: &RepoRef, path: &str, git_ref: &str) -> HostResult<Option<String>> {
        let kind = if is_sha(git_ref) { "commit" } else { "branch" };
        let url = format!(
            "{}/items?path={}&versionDescriptor.version={}&versionDescriptor.versionType={kind}&includeContent=true&{API}",
            Self::git(repo),
            enc(&format!("/{}", path.trim_start_matches('/'))),
            enc(&strip_heads(git_ref)),
        );
        let v: Option<Value> = self.http.get_optional(&url, "file").await?;
        Ok(v.and_then(|v| os(&v, "content")))
    }

    async fn builds(&self, repo: &RepoRef, filter: &str) -> HostResult<Vec<Pipeline>> {
        let id = self.repo_id(repo).await?;
        let v: Value = self
            .http
            .get(
                &format!(
                    "{}/_apis/build/builds?repositoryId={}&repositoryType=TfsGit&queryOrder=queueTimeDescending&$top=30{filter}&{API}",
                    Self::project(repo),
                    enc(&id)
                ),
                "builds",
            )
            .await?;
        Ok(values(&v).iter().filter_map(pipeline_from_build).collect())
    }

    async fn work_items(&self, repo: &RepoRef, ids: &[u64]) -> HostResult<Vec<Value>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let list: Vec<String> = ids.iter().map(u64::to_string).collect();
        let v: Value = self
            .http
            .get(
                &format!(
                    "{}/_apis/wit/workitems?ids={}&$expand=links&{API}",
                    Self::project(repo),
                    list.join(",")
                ),
                "work items",
            )
            .await?;
        Ok(values(&v).to_vec())
    }

    fn issue_from(&self, repo: &RepoRef, v: &Value) -> Issue {
        let f = v.get("fields").cloned().unwrap_or(Value::Null);
        let number = u(v, "id");
        Issue {
            number,
            title: s(&f, "System.Title"),
            state: if closed_state(&s(&f, "System.State")) {
                IssueState::Closed
            } else {
                IssueState::Open
            },
            author: az_user_at(&f, "System.CreatedBy"),
            labels: s(&f, "System.Tags")
                .split(';')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(|t| Label {
                    name: t.to_string(),
                    color: None,
                })
                .chain(os(&f, "System.WorkItemType").map(|t| Label {
                    name: t,
                    color: None,
                }))
                .collect(),
            assignees: f
                .get("System.AssignedTo")
                .filter(|a| a.is_object())
                .map(|a| vec![az_user(a)])
                .unwrap_or_default(),
            comments: u(&f, "System.CommentCount") as u32,
            created_at: s(&f, "System.CreatedDate"),
            updated_at: s(&f, "System.ChangedDate"),
            web_url: v
                .get("_links")
                .and_then(|l| l.get("html"))
                .and_then(|h| os(h, "href"))
                .unwrap_or_else(|| {
                    format!(
                        "{}/{}/_workitems/edit/{number}",
                        repo.server.trim_end_matches('/'),
                        repo.owner_repo().0
                    )
                }),
        }
    }
}

#[async_trait]
impl CodeHost for AzureDevOps {
    fn kind(&self) -> HostKind {
        HostKind::Azure
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            resolve_threads: true,
            approvals: true,
            draft: true,
            merge_methods: vec![MergeMethod::Merge, MergeMethod::Squash, MergeMethod::Rebase],
            request_name: "Pull Request".into(),
            number_prefix: "!".into(),
        }
    }

    async fn current_user(&self) -> HostResult<User> {
        let me = self.me().await?;
        let mut user = User {
            username: me
                .get("properties")
                .and_then(|p| p.get("Account"))
                .and_then(|a| os(a, "$value"))
                .or_else(|| os(&me, "providerDisplayName"))
                .unwrap_or_default(),
            name: os(&me, "providerDisplayName"),
            avatar_url: None,
        };
        if user.name.as_deref() == Some(user.username.as_str()) {
            user.name = None;
        }
        if user.username.is_empty() {
            return Err(HostError::Unauthorized {
                host: "Azure DevOps".into(),
            });
        }
        Ok(user)
    }

    async fn repositories(&self) -> HostResult<Vec<HostRepository>> {
        let v: Value = self
            .http
            .get(&format!("_apis/git/repositories?{API}"), "repositories")
            .await?;
        let mut repos: Vec<HostRepository> = values(&v)
            .iter()
            .filter(|r| !b(r, "isDisabled"))
            .map(|r| {
                let project = r.get("project").cloned().unwrap_or(Value::Null);
                HostRepository {
                    path: format!("{}/{}", s(&project, "name"), s(r, "name")),
                    name: s(r, "name"),
                    description: os(&project, "description").filter(|d| !d.is_empty()),
                    private: s(&project, "visibility") != "public",
                    fork: b(r, "isFork"),
                    clone_url: s(r, "remoteUrl"),
                    ssh_url: os(r, "sshUrl").filter(|u| !u.is_empty()),
                    web_url: s(r, "webUrl"),
                    updated_at: s(&project, "lastUpdateTime"),
                }
            })
            .collect();
        repos.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.path.cmp(&b.path))
        });
        Ok(repos)
    }

    async fn merge_requests(
        &self,
        repo: &RepoRef,
        filter: &MrFilter,
    ) -> HostResult<Page<MergeRequest>> {
        let status = match filter.state {
            StateFilter::Open => "active",
            StateFilter::Merged => "completed",
            StateFilter::Closed => "abandoned",
            StateFilter::All => "all",
        };
        let per = filter.per_page();
        let skip = (filter.page() - 1) * per;
        let v: Value = self
            .http
            .get(
                &format!(
                    "{}/pullrequests?searchCriteria.status={status}&$top={per}&$skip={skip}&{API}",
                    Self::git(repo)
                ),
                "repository",
            )
            .await?;
        let all = values(&v);
        let items: Vec<MergeRequest> = all
            .iter()
            .filter(|p| {
                let mr = self.pull_from(repo, p);
                let reviewers: Vec<String> = arr(p, "reviewers")
                    .iter()
                    .map(|r| az_user(r).username)
                    .collect();
                filter.matches(&mr, &reviewers, &[])
            })
            .map(|p| self.pull_from(repo, p))
            .collect();
        Ok(Page {
            items,
            next_page: (all.len() as u32 >= per).then(|| filter.page() + 1),
            total: None,
        })
    }

    async fn merge_request(&self, repo: &RepoRef, number: u64) -> HostResult<MergeRequestDetails> {
        let v = self.pr_value(repo, number).await?;
        let reviewers: Vec<Reviewer> = arr(&v, "reviewers")
            .iter()
            .filter(|r| !b(r, "isContainer"))
            .map(|r| Reviewer {
                user: az_user(r),
                state: match r.get("vote").and_then(Value::as_i64).unwrap_or(0) {
                    v if v > 0 => ReviewState::Approved,
                    v if v < 0 => ReviewState::ChangesRequested,
                    _ => ReviewState::Requested,
                },
            })
            .collect();
        let required = arr(&v, "reviewers")
            .iter()
            .filter(|r| b(r, "isRequired"))
            .count() as u32;
        let approved_by = reviewers
            .iter()
            .filter(|r| r.state == ReviewState::Approved)
            .map(|r| r.user.clone())
            .collect();
        let summary = self.pull_from(repo, &v);
        let pipeline = match summary.head_sha.as_deref() {
            Some(sha) => self.ci_status(repo, sha).await.unwrap_or(None),
            None => None,
        };
        let merge_status = s(&v, "mergeStatus");
        let base_sha = v
            .get("lastMergeTargetCommit")
            .and_then(|c| os(c, "commitId"));
        let mut summary = summary;
        summary.ci = pipeline.as_ref().map(|p| p.status);
        Ok(MergeRequestDetails {
            description: s(&v, "description"),
            reviewers,
            assignees: Vec::new(),
            approvals: Approvals {
                required,
                approved_by,
            },
            pipeline,
            mergeable: match merge_status.as_str() {
                "succeeded" => Some(true),
                "conflicts" | "rejectedByPolicy" | "failure" => Some(false),
                _ => None,
            },
            conflicts: merge_status == "conflicts",
            start_sha: base_sha.clone(),
            base_sha,
            merged_by: (summary.state == MrState::Merged)
                .then(|| az_user_at(&v, "closedBy"))
                .filter(|u| !u.username.is_empty()),
            summary,
        })
    }

    async fn merge_request_files(
        &self,
        repo: &RepoRef,
        number: u64,
    ) -> HostResult<Vec<ChangedFile>> {
        let iterations: Value = self
            .http
            .get(
                &format!("{}/iterations?{API}", Self::pr(repo, number)),
                "iterations",
            )
            .await?;
        let Some(last) = values(&iterations).last().cloned() else {
            return Ok(Vec::new());
        };
        let head = last
            .get("sourceRefCommit")
            .map(|c| s(c, "commitId"))
            .unwrap_or_default();
        let base = last
            .get("commonRefCommit")
            .or_else(|| last.get("targetRefCommit"))
            .map(|c| s(c, "commitId"))
            .unwrap_or_default();
        let changes: Value = self
            .http
            .get(
                &format!(
                    "{}/iterations/{}/changes?$top=2000&{API}",
                    Self::pr(repo, number),
                    u(&last, "id")
                ),
                "changes",
            )
            .await?;
        let entries: Vec<(String, Option<String>, ChangeKind)> = arr(&changes, "changeEntries")
            .iter()
            .filter(|c| {
                c.get("item")
                    .is_some_and(|i| !b(i, "isFolder") && s(i, "gitObjectType") != "tree")
            })
            .filter_map(|c| {
                let path = c
                    .get("item")
                    .map(|i| s(i, "path"))?
                    .trim_start_matches('/')
                    .to_string();
                let kind = s(c, "changeType");
                let status = if kind.contains("add") {
                    ChangeKind::Added
                } else if kind.contains("delete") {
                    ChangeKind::Deleted
                } else if kind.contains("rename") {
                    ChangeKind::Renamed
                } else {
                    ChangeKind::Modified
                };
                let old = os(c, "originalPath")
                    .or_else(|| os(c, "sourceServerItem"))
                    .map(|p| p.trim_start_matches('/').to_string())
                    .filter(|p| *p != path);
                Some((path, old, status))
            })
            .collect();
        let files = join_all(
            entries
                .into_iter()
                .enumerate()
                .map(|(i, (path, old, status))| {
                    let (head, base) = (head.clone(), base.clone());
                    async move {
                        let (old_text, new_text) = if i < MAX_DIFF_FILES {
                            let old_path = old.clone().unwrap_or_else(|| path.clone());
                            let old_text = if status == ChangeKind::Added || base.is_empty() {
                                String::new()
                            } else {
                                self.item(repo, &old_path, &base)
                                    .await
                                    .ok()
                                    .flatten()
                                    .unwrap_or_default()
                            };
                            let new_text = if status == ChangeKind::Deleted || head.is_empty() {
                                String::new()
                            } else {
                                self.item(repo, &path, &head)
                                    .await
                                    .ok()
                                    .flatten()
                                    .unwrap_or_default()
                            };
                            (old_text, new_text)
                        } else {
                            (String::new(), String::new())
                        };
                        let patch = similar::TextDiff::from_lines(&old_text, &new_text)
                            .unified_diff()
                            .context_radius(3)
                            .to_string();
                        let hunks = parse_patch(&path, &patch);
                        let (additions, deletions) = count_changes(&hunks);
                        ChangedFile {
                            path,
                            old_path: old,
                            status,
                            additions,
                            deletions,
                            hunks,
                        }
                    }
                }),
        )
        .await;
        Ok(files)
    }

    async fn discussions(&self, repo: &RepoRef, number: u64) -> HostResult<Vec<Discussion>> {
        let v: Value = self
            .http
            .get(
                &format!("{}/threads?{API}", Self::pr(repo, number)),
                "threads",
            )
            .await?;
        let mut out: Vec<Discussion> = values(&v)
            .iter()
            .filter(|t| !b(t, "isDeleted"))
            .filter_map(|t| {
                let notes: Vec<Note> = arr(t, "comments")
                    .iter()
                    .filter(|c| !b(c, "isDeleted"))
                    .map(|c| Note {
                        id: s(c, "id"),
                        author: az_user_at(c, "author"),
                        body: s(c, "content"),
                        created_at: s(c, "publishedDate"),
                        system: s(c, "commentType") == "system",
                    })
                    .collect();
                if notes.is_empty() {
                    return None;
                }
                let system = notes.iter().all(|n| n.system);
                let position = t
                    .get("threadContext")
                    .filter(|c| c.is_object())
                    .and_then(|c| {
                        let path = os(c, "filePath")?.trim_start_matches('/').to_string();
                        let line = |k: &str| c.get(k).and_then(|x| ou(x, "line")).map(|l| l as u32);
                        Some(Position {
                            path,
                            old_path: None,
                            new_line: line("rightFileStart"),
                            old_line: if line("rightFileStart").is_none() {
                                line("leftFileStart")
                            } else {
                                None
                            },
                        })
                    });
                let status = s(t, "status");
                Some(Discussion {
                    id: s(t, "id"),
                    notes,
                    position,
                    resolvable: !system,
                    resolved: matches!(
                        status.as_str(),
                        "fixed" | "closed" | "wontFix" | "byDesign"
                    ),
                })
            })
            .collect();
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
        let reviewers: Vec<Value> = if new.reviewers.is_empty() {
            Vec::new()
        } else {
            let ids = self.identities(repo).await.unwrap_or_default();
            new.reviewers
                .iter()
                .filter_map(|r| {
                    ids.iter()
                        .find(|(_, u)| {
                            u.username.eq_ignore_ascii_case(r)
                                || u.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(r))
                        })
                        .map(|(id, _)| json!({ "id": id }))
                })
                .collect()
        };
        let body = json!({
            "sourceRefName": format!("refs/heads/{}", new.source_branch),
            "targetRefName": format!("refs/heads/{}", new.target_branch),
            "title": new.title,
            "description": new.description,
            "isDraft": new.draft,
            "reviewers": reviewers,
            "labels": new.labels.iter().map(|l| json!({ "name": l })).collect::<Vec<_>>(),
            "completionOptions": { "deleteSourceBranch": new.remove_source_branch },
        });
        let v: Value = self
            .http
            .send_json(
                Method::POST,
                &format!("{}/pullrequests?{API}", Self::git(repo)),
                &body,
                "repository",
            )
            .await?;
        Ok(self.pull_from(repo, &v))
    }

    async fn add_comment(
        &self,
        repo: &RepoRef,
        number: u64,
        body: &str,
        position: Option<&Position>,
    ) -> HostResult<()> {
        self.post_thread(repo, number, body, position, None).await
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
                    "{}/threads/{}/comments?{API}",
                    Self::pr(repo, number),
                    enc(discussion_id)
                ),
                Some(&json!({ "parentCommentId": 1, "content": body, "commentType": 1 })),
                "thread",
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
                Method::PATCH,
                &format!(
                    "{}/threads/{}?{API}",
                    Self::pr(repo, number),
                    enc(discussion_id)
                ),
                Some(&json!({ "status": if resolved { "fixed" } else { "active" } })),
                "thread",
            )
            .await
    }

    async fn approve(&self, repo: &RepoRef, number: u64) -> HostResult<()> {
        self.vote(repo, number, 10).await
    }

    fn review_events(&self) -> Vec<ReviewEvent> {
        vec![
            ReviewEvent::Comment,
            ReviewEvent::Approve,
            ReviewEvent::RequestChanges,
        ]
    }

    /// Azure DevOps has no batched review: every comment becomes a thread (multi-line ones
    /// with a line range), the body a general thread, and the verdict a vote ("Approved" or
    /// "Waiting for author").
    async fn submit_review(
        &self,
        repo: &RepoRef,
        number: u64,
        review: &NewReview,
    ) -> HostResult<()> {
        for c in &review.comments {
            self.post_thread(repo, number, &c.body, Some(&c.position), c.start_line)
                .await?;
        }
        if !review.body.trim().is_empty() {
            self.post_thread(repo, number, &review.body, None, None)
                .await?;
        }
        match review.event {
            ReviewEvent::Comment => Ok(()),
            ReviewEvent::Approve => self.vote(repo, number, 10).await,
            ReviewEvent::RequestChanges => self.vote(repo, number, -5).await,
        }
    }

    async fn merge(&self, repo: &RepoRef, number: u64, opts: &MergeOptions) -> HostResult<()> {
        let v = self.pr_value(repo, number).await?;
        let commit = v
            .get("lastMergeSourceCommit")
            .cloned()
            .unwrap_or(Value::Null);
        let mut completion = json!({
            "mergeStrategy": match opts.method {
                MergeMethod::Merge => "noFastForward",
                MergeMethod::Squash => "squash",
                MergeMethod::Rebase => "rebase",
            },
            "deleteSourceBranch": opts.delete_source_branch,
        });
        if let Some(m) = opts
            .commit_message
            .as_deref()
            .filter(|m| !m.trim().is_empty())
        {
            completion["mergeCommitMessage"] = json!(m);
        }
        self.http
            .send_unit(
                Method::PATCH,
                &format!("{}?{API}", Self::pr(repo, number)),
                Some(&json!({
                    "status": "completed",
                    "lastMergeSourceCommit": { "commitId": s(&commit, "commitId") },
                    "completionOptions": completion,
                })),
                "pull request",
            )
            .await
    }

    async fn close(&self, repo: &RepoRef, number: u64) -> HostResult<()> {
        self.http
            .send_unit(
                Method::PATCH,
                &format!("{}?{API}", Self::pr(repo, number)),
                Some(&json!({ "status": "abandoned" })),
                "pull request",
            )
            .await
    }

    async fn issues(&self, repo: &RepoRef, filter: &IssueFilter) -> HostResult<Page<Issue>> {
        let mut wiql =
            String::from("SELECT [System.Id] FROM WorkItems WHERE [System.TeamProject] = @project");
        match filter.state {
            IssueStateFilter::Open => wiql.push_str(
                " AND [System.State] NOT IN ('Closed', 'Done', 'Removed', 'Resolved', 'Completed')",
            ),
            IssueStateFilter::Closed => wiql.push_str(
                " AND [System.State] IN ('Closed', 'Done', 'Removed', 'Resolved', 'Completed')",
            ),
            IssueStateFilter::All => {}
        }
        let quote = |s: &str| s.replace('\'', "''");
        if let Some(a) = filter.assignee.as_deref().filter(|a| !a.trim().is_empty()) {
            wiql.push_str(&format!(" AND [System.AssignedTo] = '{}'", quote(a.trim())));
        }
        if let Some(a) = filter.author.as_deref().filter(|a| !a.trim().is_empty()) {
            wiql.push_str(&format!(" AND [System.CreatedBy] = '{}'", quote(a.trim())));
        }
        if let Some(l) = filter.label.as_deref().filter(|l| !l.trim().is_empty()) {
            wiql.push_str(&format!(
                " AND [System.Tags] CONTAINS '{}'",
                quote(l.trim())
            ));
        }
        if let Some(q) = filter.search.as_deref().filter(|q| !q.trim().is_empty()) {
            wiql.push_str(&format!(
                " AND [System.Title] CONTAINS '{}'",
                quote(q.trim())
            ));
        }
        wiql.push_str(" ORDER BY [System.ChangedDate] DESC");
        let per = filter.per_page() as usize;
        let skip = (filter.page() as usize - 1) * per;
        let found: Value = self
            .http
            .send_json(
                Method::POST,
                &format!(
                    "{}/_apis/wit/wiql?$top={}&{API}",
                    Self::project(repo),
                    skip + per + 1
                ),
                &json!({ "query": wiql }),
                "work items",
            )
            .await?;
        let ids: Vec<u64> = arr(&found, "workItems")
            .iter()
            .map(|w| u(w, "id"))
            .collect();
        let page: Vec<u64> = ids.iter().skip(skip).take(per).copied().collect();
        let items = self.work_items(repo, &page).await?;
        let mut issues: Vec<Issue> = items.iter().map(|v| self.issue_from(repo, v)).collect();
        // Keep the WIQL order.
        issues.sort_by_key(|i| {
            page.iter()
                .position(|p| *p == i.number)
                .unwrap_or(usize::MAX)
        });
        Ok(Page {
            items: issues,
            next_page: (ids.len() > skip + per).then(|| filter.page() + 1),
            total: None,
        })
    }

    async fn issue(&self, repo: &RepoRef, number: u64) -> HostResult<IssueDetails> {
        let v: Value = self
            .http
            .get(
                &format!(
                    "{}/_apis/wit/workitems/{number}?$expand=links&{API}",
                    Self::project(repo)
                ),
                &format!("work item {number}"),
            )
            .await?;
        let comments: Value = self
            .http
            .get(
                &format!(
                    "{}/_apis/wit/workItems/{number}/comments?{COMMENTS_API}",
                    Self::project(repo)
                ),
                "comments",
            )
            .await
            .unwrap_or(Value::Null);
        let f = v.get("fields").cloned().unwrap_or(Value::Null);
        let mut notes: Vec<Note> = arr(&comments, "comments")
            .iter()
            .map(|c| Note {
                id: s(c, "id"),
                author: az_user_at(c, "createdBy"),
                body: html_to_text(&s(c, "text")),
                created_at: s(c, "createdDate"),
                system: false,
            })
            .collect();
        notes.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        let description = os(&f, "System.Description")
            .or_else(|| os(&f, "Microsoft.VSTS.TCM.ReproSteps"))
            .map(|d| html_to_text(&d))
            .unwrap_or_default();
        Ok(IssueDetails {
            summary: self.issue_from(repo, &v),
            description,
            notes,
        })
    }

    async fn comment_issue(&self, repo: &RepoRef, number: u64, body: &str) -> HostResult<()> {
        self.http
            .send_unit(
                Method::POST,
                &format!(
                    "{}/_apis/wit/workItems/{number}/comments?{COMMENTS_API}",
                    Self::project(repo)
                ),
                Some(&json!({ "text": body })),
                "work item",
            )
            .await
    }

    async fn pipelines(&self, repo: &RepoRef, branch: Option<&str>) -> HostResult<Vec<Pipeline>> {
        let filter = branch
            .map(|b| format!("&branchName={}", enc(&format!("refs/heads/{b}"))))
            .unwrap_or_default();
        self.builds(repo, &filter).await
    }

    async fn ci_status(&self, repo: &RepoRef, git_ref: &str) -> HostResult<Option<Pipeline>> {
        if is_sha(git_ref) {
            let builds = self.builds(repo, "").await?;
            return Ok(builds
                .into_iter()
                .find(|b| b.sha.as_deref() == Some(git_ref)));
        }
        let filter = format!(
            "&branchName={}",
            enc(&format!("refs/heads/{}", strip_heads(git_ref)))
        );
        Ok(self.builds(repo, &filter).await?.into_iter().next())
    }

    async fn members(&self, repo: &RepoRef) -> HostResult<Vec<User>> {
        Ok(self
            .identities(repo)
            .await?
            .into_iter()
            .map(|(_, u)| u)
            .collect())
    }

    async fn file_content(
        &self,
        repo: &RepoRef,
        path: &str,
        git_ref: &str,
    ) -> HostResult<Option<String>> {
        self.item(repo, path, git_ref).await
    }
}

/// Decode `%20` and friends in an Azure DevOps remote path (project names with spaces).
pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_builds() {
        let b = json!({"id": 7, "buildNumber": "20241012.3", "status": "completed", "result": "partiallySucceeded",
            "definition": {"name": "CI"}, "sourceBranch": "refs/heads/main", "sourceVersion": "abc",
            "_links": {"web": {"href": "https://dev.azure.com/acme/Shop/_build/results?buildId=7"}}});
        let p = pipeline_from_build(&b).unwrap();
        assert_eq!(p.status, CiStatus::Failed);
        assert_eq!(p.name.as_deref(), Some("CI 20241012.3"));
        assert_eq!(p.git_ref.as_deref(), Some("main"));
        assert!(pipeline_from_build(&json!({"status": "weird"})).is_none());
    }

    #[test]
    fn html_and_paths() {
        assert_eq!(
            html_to_text("<div>Steps:</div><ul><li>Open &amp; save</li></ul>"),
            "Steps:\n- Open & save"
        );
        assert_eq!(percent_decode("My%20Project"), "My Project");
        assert!(closed_state("Done"));
        assert!(!closed_state("Active"));
    }
}
