mod common;

use common::{MockServer, Route};
use forge_forges::*;

const R: &str = "/repos/acme/shop";

fn repo() -> RepoRef {
    RepoRef {
        kind: HostKind::Github,
        server: "https://github.com".into(),
        path: "acme/shop".into(),
        remote: "origin".into(),
    }
}

fn pr(number: u64, title: &str, merged_at: &str, draft: bool) -> String {
    format!(
        r#"{{"number":{number},"title":"{title}","state":"{}","draft":{draft},"merged_at":{merged_at},
        "user":{{"login":"anna","avatar_url":"https://avatars/anna"}},"head":{{"ref":"feat/{number}","sha":"h{number}","repo":{{"full_name":"acme/shop"}}}},
        "base":{{"ref":"main","sha":"b{number}"}},"labels":[{{"name":"bug","color":"d73a4a"}}],"requested_reviewers":[],
        "created_at":"2026-10-01T10:00:00Z","updated_at":"2026-10-02T10:00:00Z","html_url":"https://github.com/acme/shop/pull/{number}"}}"#,
        if merged_at == "null" {
            "open"
        } else {
            "closed"
        }
    )
}

async fn host(routes: Vec<Route>) -> (MockServer, GitHub) {
    let server = MockServer::start(routes).await;
    let gh = GitHub::new(
        forge_forges::http::client(),
        &server.base,
        Some("ghp_x".into()),
    );
    (server, gh)
}

#[tokio::test]
async fn lists_pulls_with_check_runs() {
    let list = format!(
        "[{},{}]",
        pr(1, "Add search", "null", true),
        pr(2, "Old", "\"2026-09-01T00:00:00Z\"", false)
    );
    let (server, gh) = host(vec![
        Route::json("GET", &format!("{R}/pulls"), &list),
        Route::json(
            "GET",
            &format!("{R}/commits/h1/check-runs"),
            r#"{"check_runs":[{"name":"test","status":"completed","conclusion":"success"},{"name":"lint","status":"in_progress","conclusion":null}]}"#,
        ),
    ])
    .await;
    let page = gh
        .merge_requests(
            &repo(),
            &MrFilter {
                state: StateFilter::All,
                ..MrFilter::default()
            },
        )
        .await
        .unwrap();
    let req = server.find("GET", &format!("{R}/pulls"));
    assert_eq!(
        req.headers.get("authorization").map(String::as_str),
        Some("Bearer ghp_x")
    );
    assert_eq!(
        req.headers.get("accept").map(String::as_str),
        Some("application/vnd.github+json")
    );
    assert!(req
        .headers
        .get("user-agent")
        .is_some_and(|u| u.starts_with("Forge-IDE/")));
    assert_eq!(page.items.len(), 2);
    assert!(page.items[0].draft);
    assert_eq!(page.items[0].ci, Some(CiStatus::Running));
    assert_eq!(page.items[1].state, MrState::Merged);
    assert_eq!(page.items[1].ci, None);
}

#[tokio::test]
async fn threads_resolve_through_graphql() {
    let (server, gh) = host(vec![
        Route::json("GET", &format!("{R}/issues/1/comments"), r#"[{"id":5,"body":"Nice","user":{"login":"ben"},"created_at":"2026-10-01T09:00:00Z"}]"#),
        Route::json(
            "GET",
            &format!("{R}/pulls/1/comments"),
            r#"[{"id":100,"body":"Rename?","user":{"login":"ben"},"path":"src/a.php","line":7,"side":"RIGHT","created_at":"2026-10-01T10:00:00Z"},
               {"id":101,"in_reply_to_id":100,"body":"Done","user":{"login":"anna"},"path":"src/a.php","line":7,"side":"RIGHT","created_at":"2026-10-01T11:00:00Z"},
               {"id":102,"body":"Removed?","user":{"login":"ben"},"path":"src/b.php","line":3,"side":"LEFT","created_at":"2026-10-01T12:00:00Z"}]"#,
        ),
        Route::json("GET", &format!("{R}/pulls/1/reviews"), r#"[{"id":9,"user":{"login":"ben"},"state":"APPROVED","body":"","submitted_at":"2026-10-01T13:00:00Z"}]"#),
        Route::json(
            "POST",
            "/graphql",
            r#"{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[{"id":"PRRT_1","isResolved":true,"comments":{"nodes":[{"databaseId":100}]}},{"id":"PRRT_2","isResolved":false,"comments":{"nodes":[{"databaseId":102}]}}]}}}}}"#,
        ),
        Route::json("POST", &format!("{R}/pulls/1/comments/102/replies"), "{}").status(201),
        Route::json("GET", &format!("{R}/pulls/1"), &pr(1, "Add search", "null", false)),
        Route::json("POST", &format!("{R}/pulls/1/comments"), "{}").status(201),
    ])
    .await;
    let ds = gh.discussions(&repo(), 1).await.unwrap();
    assert_eq!(ds.len(), 4);
    let t1 = ds.iter().find(|d| d.id == "100").unwrap();
    assert_eq!(t1.notes.len(), 2);
    assert!(t1.resolved);
    let t2 = ds.iter().find(|d| d.id == "102").unwrap();
    assert_eq!(t2.position.as_ref().unwrap().old_line, Some(3));
    assert!(!t2.resolved);
    assert!(ds
        .iter()
        .any(|d| d.notes[0].system && d.notes[0].body == "approved these changes"));

    gh.reply(&repo(), 1, "102", "Yes").await.unwrap();
    gh.resolve(&repo(), 1, "102", true).await.unwrap();
    let body = server.find("POST", "/graphql").json();
    assert!(body["query"]
        .as_str()
        .unwrap()
        .contains("resolveReviewThread"));
    assert_eq!(body["variables"]["id"], "PRRT_2");

    let at = Position {
        path: "src/b.php".into(),
        old_path: None,
        new_line: None,
        old_line: Some(3),
    };
    gh.add_comment(&repo(), 1, "Hm", Some(&at)).await.unwrap();
    let body = server.find("POST", &format!("{R}/pulls/1/comments")).json();
    assert_eq!(
        (
            body["side"].as_str(),
            body["line"].as_u64(),
            body["commit_id"].as_str()
        ),
        (Some("LEFT"), Some(3), Some("h1"))
    );
}

#[tokio::test]
async fn details_files_and_merge() {
    let (server, gh) = host(vec![
        Route::json("GET", &format!("{R}/pulls/1"), &pr(1, "Add search", "null", false).replace("\"draft\"", "\"body\":\"Body\",\"mergeable\":false,\"mergeable_state\":\"dirty\",\"comments\":1,\"review_comments\":2,\"draft\"")),
        Route::json("GET", &format!("{R}/pulls/1/reviews"), r#"[{"user":{"login":"ben"},"state":"CHANGES_REQUESTED"},{"user":{"login":"ben"},"state":"COMMENTED"}]"#),
        Route::json("GET", &format!("{R}/commits/h1/check-runs"), r#"{"check_runs":[{"name":"build","status":"completed","conclusion":"failure","html_url":"https://ci/b"}]}"#),
        Route::json("GET", &format!("{R}/commits/h1/status"), r#"{"sha":"h1","statuses":[{"context":"deploy/preview","state":"success","target_url":"https://p"}]}"#),
        Route::json("GET", &format!("{R}/pulls/1/files"), r#"[{"filename":"src/a.php","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-a\n+b"},{"filename":"img.png","status":"added","additions":0,"deletions":0}]"#),
        Route::json("PUT", &format!("{R}/pulls/1/merge"), r#"{"merged":true}"#),
        Route::json("DELETE", &format!("{R}/git/refs/heads/feat/1"), "").status(204),
    ])
    .await;
    let d = gh.merge_request(&repo(), 1).await.unwrap();
    assert_eq!(d.summary.comments, Some(3));
    assert_eq!(d.reviewers[0].state, ReviewState::ChangesRequested);
    assert!(d.conflicts);
    let p = d.pipeline.unwrap();
    assert_eq!(p.status, CiStatus::Failed);
    assert_eq!(p.jobs.len(), 2);

    let files = gh.merge_request_files(&repo(), 1).await.unwrap();
    assert_eq!(files[0].hunks.len(), 1);
    assert!(files[1].hunks.is_empty());

    gh.merge(
        &repo(),
        1,
        &MergeOptions {
            method: MergeMethod::Squash,
            delete_source_branch: true,
            commit_message: Some("Add search (#1)\n\nDetails".into()),
        },
    )
    .await
    .unwrap();
    let body = server.find("PUT", &format!("{R}/pulls/1/merge")).json();
    assert_eq!(body["merge_method"], "squash");
    assert_eq!(body["commit_title"], "Add search (#1)");
    assert_eq!(body["commit_message"], "Details");
    assert_eq!(
        server.count("DELETE", &format!("{R}/git/refs/heads/feat/1")),
        1
    );
}

#[tokio::test]
async fn issues_search_and_rate_limits() {
    let (server, gh) = host(vec![
        Route::json("GET", &format!("{R}/issues"), r#"[{"number":3,"title":"Bug","state":"open","user":{"login":"x"},"comments":1},{"number":1,"title":"PR","pull_request":{}}]"#),
        Route::json("GET", "/search/issues", r#"{"total_count":45,"items":[{"number":3,"title":"Bug","state":"open","user":{"login":"x"}}]}"#),
        Route::json("GET", "/user", r#"{"message":"API rate limit exceeded for user ID 1."}"#)
            .status(403)
            .header("X-RateLimit-Remaining", "0")
            .header("X-RateLimit-Reset", "4102444800"),
    ])
    .await;
    let page = gh.issues(&repo(), &IssueFilter::default()).await.unwrap();
    assert_eq!(page.items.len(), 1);
    let page = gh
        .issues(
            &repo(),
            &IssueFilter {
                search: Some("crash".into()),
                label: Some("bug".into()),
                ..IssueFilter::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.next_page, Some(2));
    let q = server.find("GET", "/search/issues").query;
    assert!(q.contains("repo%3Aacme%2Fshop%20is%3Aissue%20crash"), "{q}");
    assert!(q.contains("label%3A%22bug%22"), "{q}");

    let e = gh.current_user().await.unwrap_err();
    assert!(matches!(e, HostError::RateLimited { .. }), "{e}");
    assert!(e
        .to_string()
        .starts_with("GitHub: API rate limit reached, try again in"));
}

#[tokio::test]
async fn lists_the_accounts_repositories_across_pages() {
    let page1 = r#"[{"full_name":"acme/shop","name":"shop","description":"Online shop","private":true,"fork":false,
        "clone_url":"https://github.com/acme/shop.git","ssh_url":"git@github.com:acme/shop.git",
        "html_url":"https://github.com/acme/shop","pushed_at":"2026-10-20T10:00:00Z","updated_at":"2026-10-01T10:00:00Z"}]"#;
    let page2 = r#"[{"full_name":"marlon/dotfiles","name":"dotfiles","description":null,"private":false,"fork":true,
        "clone_url":"https://github.com/marlon/dotfiles.git","ssh_url":"git@github.com:marlon/dotfiles.git",
        "html_url":"https://github.com/marlon/dotfiles","updated_at":"2026-09-01T10:00:00Z"}]"#;
    let (server, gh) = host(vec![
        Route::json("GET", "/user/repos", page1).header(
            "Link",
            "<{base}/user/repos?per_page=100&page=2>; rel=\"next\"",
        ),
        Route::json("GET", "/user/repos", page2).query("page=2"),
    ])
    .await;
    let repos = gh.repositories().await.unwrap();
    assert_eq!(repos.len(), 2);
    assert_eq!(repos[0].path, "acme/shop");
    assert!(repos[0].private && !repos[0].fork);
    assert_eq!(
        repos[0].ssh_url.as_deref(),
        Some("git@github.com:acme/shop.git")
    );
    assert_eq!(repos[0].updated_at, "2026-10-20T10:00:00Z");
    assert_eq!(repos[1].description, None);
    assert!(repos[1].fork);
    let first = server.find("GET", "/user/repos");
    assert!(first.query.contains("page=2") || first.query.contains("affiliation=owner"));
    assert_eq!(server.count("GET", "/user/repos"), 2);
}

fn review() -> NewReview {
    NewReview {
        event: ReviewEvent::RequestChanges,
        body: "A few things".into(),
        comments: vec![
            ReviewComment {
                body: "Range".into(),
                position: Position {
                    path: "app/User.php".into(),
                    new_line: Some(14),
                    ..Position::default()
                },
                start_line: Some(12),
            },
            ReviewComment {
                body: "Removed line".into(),
                position: Position {
                    path: "app/User.php".into(),
                    old_line: Some(3),
                    ..Position::default()
                },
                start_line: None,
            },
        ],
    }
}

#[tokio::test]
async fn submits_a_review_with_all_comments_at_once() {
    let (server, gh) = host(vec![
        Route::json("GET", &format!("{R}/pulls/1"), &pr(1, "x", "null", false)),
        Route::json("POST", &format!("{R}/pulls/1/reviews"), "{}"),
    ])
    .await;
    assert!(gh.review_events().contains(&ReviewEvent::RequestChanges));
    gh.submit_review(&repo(), 1, &review()).await.unwrap();
    assert_eq!(server.count("POST", &format!("{R}/pulls/1/reviews")), 1);
    let body = server.find("POST", &format!("{R}/pulls/1/reviews")).json();
    assert_eq!(body["event"], "REQUEST_CHANGES");
    assert_eq!(body["commit_id"], "h1");
    assert_eq!(body["body"], "A few things");
    let c = &body["comments"];
    assert_eq!(
        (c[0]["line"].as_u64(), c[0]["start_line"].as_u64()),
        (Some(14), Some(12))
    );
    assert_eq!(
        (c[0]["side"].as_str(), c[1]["side"].as_str()),
        (Some("RIGHT"), Some("LEFT"))
    );
    assert!(c[1].get("start_line").is_none());
}
