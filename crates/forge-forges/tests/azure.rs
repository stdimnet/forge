mod common;

use common::{MockServer, Route};
use forge_forges::*;

const ORG: &str = "/acme";
const G: &str = "/acme/Shop/_apis/git/repositories/shop";

fn repo(base: &str) -> RepoRef {
    RepoRef {
        kind: HostKind::Azure,
        server: format!("{base}{ORG}"),
        path: "Shop/shop".into(),
        remote: "origin".into(),
    }
}

fn pr(id: u64, title: &str, status: &str, user: &str) -> String {
    format!(
        r#"{{"pullRequestId":{id},"title":"{title}","status":"{status}","isDraft":{draft},
        "createdBy":{{"displayName":"{user}","uniqueName":"{user}@acme.dev","id":"u-{user}"}},
        "creationDate":"2026-10-0{id}T10:00:00Z","sourceRefName":"refs/heads/feature/{id}","targetRefName":"refs/heads/main",
        "mergeStatus":"succeeded","description":"Adds things","labels":[{{"name":"backend","active":true}}],
        "lastMergeSourceCommit":{{"commitId":"src{id}"}},"lastMergeTargetCommit":{{"commitId":"base{id}"}},
        "reviewers":[{{"displayName":"Ben","uniqueName":"ben@acme.dev","vote":10,"isRequired":true}},
                     {{"displayName":"Cleo","uniqueName":"cleo@acme.dev","vote":-5}}]}}"#,
        draft = id == 2
    )
}

const ME: &str = r#"{"authenticatedUser":{"id":"u-anna","providerDisplayName":"Anna","properties":{"Account":{"$type":"System.String","$value":"anna@acme.dev"}}}}"#;

async fn host(routes: Vec<Route>) -> (MockServer, AzureDevOps) {
    let server = MockServer::start(routes).await;
    let az = AzureDevOps::new(
        forge_forges::http::client(),
        &format!("{}{ORG}", server.base),
        Some("pat-123".into()),
    );
    (server, az)
}

#[tokio::test]
async fn verifies_the_token_with_basic_auth() {
    let (server, az) = host(vec![Route::json("GET", "/acme/_apis/connectionData", ME)]).await;
    let me = az.current_user().await.unwrap();
    assert_eq!(me.username, "anna@acme.dev");
    assert_eq!(me.name.as_deref(), Some("Anna"));
    let req = server.find("GET", "/acme/_apis/connectionData");
    // Basic base64(":pat-123").
    assert_eq!(
        req.headers.get("authorization").map(String::as_str),
        Some("Basic OnBhdC0xMjM=")
    );
    // A rejected token: 203 with the HTML sign-in page.
    let (_s, az) = host(vec![Route {
        content_type: "text/html",
        ..Route::json("GET", "/acme/_apis/connectionData", "<html>Sign in</html>").status(203)
    }])
    .await;
    assert!(matches!(
        az.current_user().await,
        Err(HostError::Unauthorized { .. })
    ));
}

#[tokio::test]
async fn lists_and_reads_pull_requests() {
    let list = format!(
        r#"{{"count":2,"value":[{},{}]}}"#,
        pr(1, "Add wishlist", "active", "anna"),
        pr(2, "Fix totals", "active", "ben")
    );
    let (server, az) = host(vec![
        Route::json("GET", &format!("{G}/pullrequests"), &list),
        Route::json("GET", &format!("{G}/pullrequests/1"), &pr(1, "Add wishlist", "active", "anna")),
        Route::json("GET", G, r#"{"id":"repo-guid","name":"shop"}"#),
        Route::json(
            "GET",
            "/acme/Shop/_apis/build/builds",
            r#"{"value":[{"id":9,"buildNumber":"20261001.1","status":"inProgress","definition":{"name":"CI"},"sourceBranch":"refs/pull/1/merge","sourceVersion":"src1"}]}"#,
        ),
    ])
    .await;
    let r = repo(&server.base);
    let page = az
        .merge_requests(
            &r,
            &MrFilter {
                author: Some("anna@acme.dev".into()),
                per_page: 2,
                ..MrFilter::default()
            },
        )
        .await
        .unwrap();
    let req = server.find("GET", &format!("{G}/pullrequests"));
    assert!(req.query.contains("searchCriteria.status=active"));
    assert!(req.query.contains("$top=2&$skip=0"));
    assert!(req.query.contains("api-version=7.1"));
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.next_page, Some(2));
    let mr = &page.items[0];
    assert_eq!(
        (
            mr.number,
            mr.source_branch.as_str(),
            mr.target_branch.as_str()
        ),
        (1, "feature/1", "main")
    );
    assert_eq!(
        mr.web_url,
        format!("{}/acme/Shop/_git/shop/pullrequest/1", server.base)
    );
    assert_eq!(mr.labels[0].name, "backend");

    let d = az.merge_request(&r, 1).await.unwrap();
    assert_eq!(d.approvals.required, 1);
    assert_eq!(d.approvals.approved_by[0].username, "ben@acme.dev");
    assert_eq!(d.reviewers[1].state, ReviewState::ChangesRequested);
    assert_eq!(d.mergeable, Some(true));
    assert_eq!(d.base_sha.as_deref(), Some("base1"));
    let p = d.pipeline.unwrap();
    assert_eq!(
        (p.status, p.name.as_deref()),
        (CiStatus::Running, Some("CI 20261001.1"))
    );
    assert!(server
        .find("GET", "/acme/Shop/_apis/build/builds")
        .query
        .contains("repositoryId=repo-guid"));
}

#[tokio::test]
async fn diffs_changed_files() {
    let (server, az) = host(vec![
        Route::json(
            "GET",
            &format!("{G}/pullrequests/1/iterations"),
            r#"{"value":[{"id":1},{"id":2,"sourceRefCommit":{"commitId":"head"},"commonRefCommit":{"commitId":"base"}}]}"#,
        ),
        Route::json(
            "GET",
            &format!("{G}/pullrequests/1/iterations/2/changes"),
            r#"{"changeEntries":[{"changeType":"edit","item":{"path":"/src/Cart.cs"}},{"changeType":"add","item":{"path":"/src/Wish.cs"}},{"changeType":"edit","item":{"path":"/src","isFolder":true}}]}"#,
        ),
        Route::json("GET", &format!("{G}/items"), r#"{"content":"class Cart {\n  int a;\n}\n"}"#).query("version=base"),
        Route::json("GET", &format!("{G}/items"), r#"{"content":"class Cart {\n  int b;\n}\n"}"#).query("version=head"),
    ])
    .await;
    let files = az
        .merge_request_files(&repo(&server.base), 1)
        .await
        .unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(
        (
            files[0].path.as_str(),
            files[0].additions,
            files[0].deletions
        ),
        ("src/Cart.cs", 1, 1)
    );
    assert_eq!(files[1].status, ChangeKind::Added);
    assert_eq!(files[1].additions, 3);
    let item = server.find("GET", &format!("{G}/items"));
    assert!(item.query.contains("path=%2Fsrc%2F"));
    assert!(
        item.query.contains("versionDescriptor.versionType=branch")
            || item.query.contains("includeContent=true")
    );
}

#[tokio::test]
async fn threads_comments_votes_and_completion() {
    let threads = r#"{"value":[
        {"id":5,"status":"active","comments":[{"id":1,"content":"Looks good","commentType":"text","publishedDate":"2026-10-02T10:00:00Z","author":{"displayName":"Ben","uniqueName":"ben@acme.dev"}}]},
        {"id":6,"status":"fixed","threadContext":{"filePath":"/src/Cart.cs","rightFileStart":{"line":12,"offset":1}},
         "comments":[{"id":1,"content":"Rename?","commentType":"text","publishedDate":"2026-10-01T10:00:00Z","author":{"uniqueName":"cleo@acme.dev"}},
                     {"id":2,"content":"Done","parentCommentId":1,"commentType":"text","publishedDate":"2026-10-01T11:00:00Z","author":{"uniqueName":"anna@acme.dev"}}]},
        {"id":7,"comments":[{"id":1,"content":"Ben voted 10","commentType":"system","publishedDate":"2026-10-03T10:00:00Z","author":{"uniqueName":"ben@acme.dev"}}]},
        {"id":8,"isDeleted":true,"comments":[{"id":1,"content":"x","commentType":"text"}]}
    ]}"#;
    let (server, az) = host(vec![
        Route::json("GET", &format!("{G}/pullrequests/1/threads"), threads),
        Route::json("POST", &format!("{G}/pullrequests/1/threads"), "{}"),
        Route::json(
            "POST",
            &format!("{G}/pullrequests/1/threads/6/comments"),
            "{}",
        ),
        Route::json("PATCH", &format!("{G}/pullrequests/1/threads/6"), "{}"),
        Route::json("GET", "/acme/_apis/connectionData", ME),
        Route::json("PUT", &format!("{G}/pullrequests/1/reviewers/u-anna"), "{}"),
        Route::json(
            "GET",
            &format!("{G}/pullrequests/1"),
            &pr(1, "Add wishlist", "active", "anna"),
        ),
        Route::json("PATCH", &format!("{G}/pullrequests/1"), "{}"),
    ])
    .await;
    let r = repo(&server.base);
    let d = az.discussions(&r, 1).await.unwrap();
    assert_eq!(d.len(), 3);
    assert_eq!(d[0].id, "6");
    assert!(d[0].resolved);
    assert_eq!(d[0].position.as_ref().unwrap().path, "src/Cart.cs");
    assert_eq!(d[0].position.as_ref().unwrap().new_line, Some(12));
    assert_eq!(d[0].notes.len(), 2);
    assert!(!d[2].resolvable && d[2].notes[0].system);

    let pos = Position {
        path: "src/Cart.cs".into(),
        new_line: Some(3),
        ..Position::default()
    };
    az.add_comment(&r, 1, "Why?", Some(&pos)).await.unwrap();
    let body = server
        .find("POST", &format!("{G}/pullrequests/1/threads"))
        .json();
    assert_eq!(body["comments"][0]["content"], "Why?");
    assert_eq!(body["threadContext"]["filePath"], "/src/Cart.cs");
    assert_eq!(body["threadContext"]["rightFileStart"]["line"], 3);

    az.reply(&r, 1, "6", "Thanks").await.unwrap();
    assert_eq!(
        server
            .find("POST", &format!("{G}/pullrequests/1/threads/6/comments"))
            .json()["parentCommentId"],
        1
    );
    az.resolve(&r, 1, "6", false).await.unwrap();
    assert_eq!(
        server
            .find("PATCH", &format!("{G}/pullrequests/1/threads/6"))
            .json()["status"],
        "active"
    );
    az.approve(&r, 1).await.unwrap();
    assert_eq!(
        server
            .find("PUT", &format!("{G}/pullrequests/1/reviewers/u-anna"))
            .json()["vote"],
        10
    );

    az.merge(
        &r,
        1,
        &MergeOptions {
            method: MergeMethod::Squash,
            delete_source_branch: true,
            commit_message: None,
        },
    )
    .await
    .unwrap();
    let merge = server.find("PATCH", &format!("{G}/pullrequests/1")).json();
    assert_eq!(merge["status"], "completed");
    assert_eq!(merge["lastMergeSourceCommit"]["commitId"], "src1");
    assert_eq!(merge["completionOptions"]["mergeStrategy"], "squash");
    assert_eq!(merge["completionOptions"]["deleteSourceBranch"], true);
    az.close(&r, 1).await.unwrap();
    assert_eq!(
        server.find("PATCH", &format!("{G}/pullrequests/1")).json()["status"],
        "abandoned"
    );
}

#[tokio::test]
async fn creates_pull_requests_with_reviewers() {
    let (server, az) = host(vec![
        Route::json("GET", "/acme/_apis/projects/Shop/teams", r#"{"value":[{"id":"team-1","name":"Shop Team"}]}"#),
        Route::json(
            "GET",
            "/acme/_apis/projects/Shop/teams/team-1/members",
            r#"{"value":[{"identity":{"id":"id-ben","displayName":"Ben","uniqueName":"ben@acme.dev"}}]}"#,
        ),
        Route::json("POST", &format!("{G}/pullrequests"), &pr(3, "New", "active", "anna")),
    ])
    .await;
    let r = repo(&server.base);
    let mr = az
        .create_merge_request(
            &r,
            &NewMergeRequest {
                source_branch: "feature/3".into(),
                target_branch: "main".into(),
                title: "New".into(),
                draft: true,
                reviewers: vec!["ben@acme.dev".into()],
                ..NewMergeRequest::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(mr.number, 3);
    let body = server.find("POST", &format!("{G}/pullrequests")).json();
    assert_eq!(body["sourceRefName"], "refs/heads/feature/3");
    assert_eq!(body["isDraft"], true);
    assert_eq!(body["reviewers"][0]["id"], "id-ben");
    assert_eq!(az.members(&r).await.unwrap()[0].username, "ben@acme.dev");
}

#[tokio::test]
async fn work_items_are_issues() {
    let (server, az) = host(vec![
        Route::json("POST", "/acme/Shop/_apis/wit/wiql", r#"{"workItems":[{"id":42},{"id":7}]}"#),
        Route::json(
            "GET",
            "/acme/Shop/_apis/wit/workitems",
            r#"{"value":[
              {"id":7,"fields":{"System.Title":"Old bug","System.State":"Active","System.WorkItemType":"Bug","System.CreatedBy":{"uniqueName":"ben@acme.dev"},"System.ChangedDate":"2026-10-01"}},
              {"id":42,"fields":{"System.Title":"Wishlist","System.State":"New","System.WorkItemType":"User Story","System.Tags":"web; ux","System.CreatedBy":{"uniqueName":"anna@acme.dev","displayName":"Anna"},"System.AssignedTo":{"uniqueName":"ben@acme.dev"},"System.CommentCount":1},
               "_links":{"html":{"href":"https://dev.azure.com/acme/Shop/_workitems/edit/42"}}}]}"#,
        ),
        Route::json(
            "GET",
            "/acme/Shop/_apis/wit/workitems/42",
            r#"{"id":42,"fields":{"System.Title":"Wishlist","System.State":"Done","System.Description":"<div>Save books <b>for later</b></div>"}}"#,
        ),
        Route::json(
            "GET",
            "/acme/Shop/_apis/wit/workItems/42/comments",
            r#"{"comments":[{"id":1,"text":"<p>On it</p>","createdDate":"2026-10-02","createdBy":{"uniqueName":"ben@acme.dev"}}]}"#,
        ),
        Route::json("POST", "/acme/Shop/_apis/wit/workItems/42/comments", "{}"),
    ])
    .await;
    let r = repo(&server.base);
    let page = az
        .issues(
            &r,
            &IssueFilter {
                label: Some("web".into()),
                ..IssueFilter::default()
            },
        )
        .await
        .unwrap();
    let wiql = server.find("POST", "/acme/Shop/_apis/wit/wiql").json();
    let q = wiql["query"].as_str().unwrap();
    assert!(q.contains("[System.State] NOT IN ('Closed'"), "{q}");
    assert!(q.contains("[System.Tags] CONTAINS 'web'"));
    assert!(server
        .find("GET", "/acme/Shop/_apis/wit/workitems")
        .query
        .contains("ids=42,7"));
    assert_eq!(
        page.items.iter().map(|i| i.number).collect::<Vec<_>>(),
        [42, 7]
    );
    let first = &page.items[0];
    assert_eq!(
        first
            .labels
            .iter()
            .map(|l| l.name.as_str())
            .collect::<Vec<_>>(),
        ["web", "ux", "User Story"]
    );
    assert_eq!(first.assignees[0].username, "ben@acme.dev");
    assert_eq!(
        first.web_url,
        "https://dev.azure.com/acme/Shop/_workitems/edit/42"
    );

    let d = az.issue(&r, 42).await.unwrap();
    assert_eq!(d.summary.state, IssueState::Closed);
    assert_eq!(d.description, "Save books for later");
    assert_eq!(d.notes[0].body, "On it");
    az.comment_issue(&r, 42, "Shipped").await.unwrap();
    assert_eq!(
        server
            .find("POST", "/acme/Shop/_apis/wit/workItems/42/comments")
            .json()["text"],
        "Shipped"
    );
}

#[tokio::test]
async fn service_logs_in_and_detects_remotes() {
    let server =
        MockServer::start(vec![Route::json("GET", "/acme/_apis/connectionData", ME)]).await;
    let svc = ForgeService::new(None, std::sync::Arc::new(MemoryStore::default()))
        .unwrap()
        .with_env(|_| None);
    let acc = svc
        .login(
            HostKind::Azure,
            &format!("{}/acme", server.base),
            "pat-123",
            None,
        )
        .await
        .unwrap();
    assert_eq!(acc.account.username.as_deref(), Some("anna@acme.dev"));
    assert_eq!(acc.account.api_base(), format!("{}/acme", server.base));
    let refs = svc.detect(&[(
        "origin".into(),
        format!("{}/acme/Shop/_git/shop", server.base),
    )]);
    assert_eq!(refs[0].kind, HostKind::Azure);
    assert_eq!(refs[0].server, format!("{}/acme", server.base));
    assert_eq!(refs[0].path, "Shop/shop");
    assert_eq!(review_ref(HostKind::Azure, 3), "refs/pull/3/merge");
}

#[tokio::test]
async fn submits_a_review_as_threads_and_a_vote() {
    let (server, az) = host(vec![
        Route::json("POST", &format!("{G}/pullrequests/1/threads"), "{}"),
        Route::json("GET", "/acme/_apis/connectionData", ME),
        Route::json("PUT", &format!("{G}/pullrequests/1/reviewers/u-anna"), "{}"),
    ])
    .await;
    let r = repo(&server.base);
    assert!(az.review_events().contains(&ReviewEvent::RequestChanges));
    let review = NewReview {
        event: ReviewEvent::RequestChanges,
        body: "Needs tests".into(),
        comments: vec![
            ReviewComment {
                body: "Extract this".into(),
                position: Position {
                    path: "src/Cart.cs".into(),
                    new_line: Some(9),
                    ..Position::default()
                },
                start_line: Some(5),
            },
            ReviewComment {
                body: "Why removed?".into(),
                position: Position {
                    path: "src/Old.cs".into(),
                    old_line: Some(4),
                    ..Position::default()
                },
                start_line: None,
            },
        ],
    };
    az.submit_review(&r, 1, &review).await.unwrap();
    let threads: Vec<serde_json::Value> = server
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|q| q.method == "POST" && q.path == format!("{G}/pullrequests/1/threads"))
        .map(|q| q.json())
        .collect();
    assert_eq!(threads.len(), 3);
    let ctx = &threads[0]["threadContext"];
    assert_eq!(ctx["filePath"], "/src/Cart.cs");
    assert_eq!(ctx["rightFileStart"]["line"], 5);
    assert_eq!(ctx["rightFileEnd"]["line"], 9);
    assert_eq!(threads[0]["comments"][0]["content"], "Extract this");
    assert_eq!(threads[1]["threadContext"]["leftFileStart"]["line"], 4);
    assert_eq!(threads[1]["threadContext"]["leftFileEnd"]["line"], 4);
    assert_eq!(threads[2]["comments"][0]["content"], "Needs tests");
    assert!(threads[2].get("threadContext").is_none());
    assert_eq!(
        server
            .find("PUT", &format!("{G}/pullrequests/1/reviewers/u-anna"))
            .json()["vote"],
        -5
    );

    // A plain comment review sets no vote.
    let comment_only = NewReview {
        body: "FYI".into(),
        ..NewReview::default()
    };
    az.submit_review(&r, 1, &comment_only).await.unwrap();
    assert_eq!(
        server.count("PUT", &format!("{G}/pullrequests/1/reviewers/u-anna")),
        1
    );
}
