mod common;

use common::{MockServer, Route};
use forge_forges::*;

const P: &str = "/api/v4/projects/acme%2Fbackend%2Fapi";

fn repo() -> RepoRef {
    RepoRef {
        kind: HostKind::Gitlab,
        server: "https://gitlab.example.com".into(),
        path: "acme/backend/api".into(),
        remote: "origin".into(),
    }
}

fn mr_json(iid: u64, title: &str, sha: &str) -> String {
    format!(
        r#"{{"iid":{iid},"title":"{title}","state":"opened","draft":false,
        "author":{{"username":"anna","name":"Anna Berg","avatar_url":"https://a/x.png"}},
        "source_branch":"feature/{iid}","target_branch":"main",
        "labels":[{{"name":"backend","color":"1f75cb"}}],"user_notes_count":3,
        "created_at":"2026-10-01T10:00:00Z","updated_at":"2026-10-02T10:00:00Z",
        "web_url":"https://gitlab.example.com/acme/backend/api/-/merge_requests/{iid}","sha":"{sha}"}}"#
    )
}

async fn host(routes: Vec<Route>) -> (MockServer, GitLab) {
    let server = MockServer::start(routes).await;
    let gl = GitLab::new(
        forge_forges::http::client(),
        &format!("{}/api/v4", server.base),
        Some("glpat-secret".into()),
    );
    (server, gl)
}

#[tokio::test]
async fn lists_merge_requests_with_filters_pagination_and_ci() {
    let list = format!("[{},{}]", mr_json(7, "Draft: Add checkout", "aaa"), mr_json(5, "Fix cart", "bbb"));
    let (server, gl) = host(vec![
        Route::json("GET", &format!("{P}/merge_requests"), &list)
            .header("X-Next-Page", "2")
            .header("X-Total", "41"),
        Route::json(
            "GET",
            &format!("{P}/pipelines"),
            r#"[{"id":9,"sha":"aaa","status":"running"},{"id":8,"sha":"bbb","status":"failed"},{"id":7,"sha":"aaa","status":"success"}]"#,
        ),
    ])
    .await;
    let filter = MrFilter {
        author: Some("anna".into()),
        search: Some("cart flow".into()),
        ..MrFilter::default()
    };
    let page = gl.merge_requests(&repo(), &filter).await.unwrap();
    assert_eq!(page.next_page, Some(2));
    assert_eq!(page.total, Some(41));
    assert_eq!(page.items.len(), 2);
    let first = &page.items[0];
    assert_eq!(first.number, 7);
    assert!(first.draft);
    assert_eq!(first.title, "Add checkout");
    assert_eq!(first.ci, Some(CiStatus::Running));
    assert_eq!(first.comments, Some(3));
    assert_eq!(first.labels[0].color.as_deref(), Some("#1f75cb"));
    assert_eq!(page.items[1].ci, Some(CiStatus::Failed));

    let req = server.find("GET", &format!("{P}/merge_requests"));
    assert_eq!(req.headers.get("private-token").map(String::as_str), Some("glpat-secret"));
    assert!(req.query.contains("state=opened"));
    assert!(req.query.contains("author_username=anna"));
    assert!(req.query.contains("search=cart%20flow"));
}

#[tokio::test]
async fn merge_request_details_with_approvals_pipeline_and_jobs() {
    let (_server, gl) = host(vec![
        Route::json(
            "GET",
            &format!("{P}/merge_requests/7"),
            r#"{"iid":7,"title":"Add checkout","state":"opened","description":"Summary:\nAdds **checkout**.",
            "author":{"username":"anna"},"source_branch":"feature/checkout","target_branch":"main","labels":["backend"],
            "reviewers":[{"username":"ben","name":"Ben"},{"username":"cleo"}],"assignees":[{"username":"anna"}],
            "head_pipeline":{"id":99,"status":"failed","ref":"feature/checkout","sha":"abc","web_url":"https://ci/99"},
            "diff_refs":{"base_sha":"b1","start_sha":"s1","head_sha":"h1"},"detailed_merge_status":"mergeable","has_conflicts":false,
            "created_at":"2026-10-01T10:00:00Z","updated_at":"2026-10-02T10:00:00Z","web_url":"w","sha":"abc"}"#,
        ),
        Route::json(
            "GET",
            &format!("{P}/merge_requests/7/approvals"),
            r#"{"approvals_required":2,"approved_by":[{"user":{"username":"ben","name":"Ben"}}]}"#,
        ),
        Route::json(
            "GET",
            &format!("{P}/pipelines/99/jobs"),
            r#"[{"name":"phpunit","stage":"test","status":"failed","duration":61.2},{"name":"composer","stage":"build","status":"success"}]"#,
        ),
    ])
    .await;
    let d = gl.merge_request(&repo(), 7).await.unwrap();
    assert_eq!(d.description, "Summary:\nAdds **checkout**.");
    assert_eq!(d.approvals.required, 2);
    assert_eq!(d.approvals.approved_by[0].username, "ben");
    assert_eq!(d.reviewers[0].state, ReviewState::Approved);
    assert_eq!(d.reviewers[1].state, ReviewState::Requested);
    assert_eq!(d.mergeable, Some(true));
    assert_eq!(d.start_sha.as_deref(), Some("s1"));
    let p = d.pipeline.unwrap();
    assert_eq!(p.status, CiStatus::Failed);
    assert_eq!(p.jobs.len(), 2);
    assert_eq!(p.jobs[0].name, "composer", "jobs are listed in pipeline order");
    assert_eq!(d.summary.ci, Some(CiStatus::Failed));
}

#[tokio::test]
async fn files_follow_link_pagination() {
    let (server, gl) = host(vec![
        Route::json(
            "GET",
            &format!("{P}/merge_requests/7/diffs"),
            r#"[{"old_path":"app/Models/User.php","new_path":"app/Models/User.php","diff":"@@ -10,3 +10,4 @@ class User\n     use HasFactory;\n-    protected $guarded = [];\n+    protected $fillable = ['name'];\n+    protected $hidden = ['password'];\n }\n"}]"#,
        )
        .header("Link", &format!("<{{base}}{P}/merge_requests/7/diffs?page=2&per_page=50>; rel=\"next\"")),
        Route::json(
            "GET",
            &format!("{P}/merge_requests/7/diffs"),
            r#"[{"old_path":"routes/old.php","new_path":"routes/web.php","renamed_file":true,"diff":""},{"old_path":"x","new_path":"x","new_file":true,"diff":"@@ -0,0 +1 @@\n+hi\n"}]"#,
        )
        .query("page=2"),
    ])
    .await;
    let files = gl.merge_request_files(&repo(), 7).await.unwrap();
    assert_eq!(server.count("GET", &format!("{P}/merge_requests/7/diffs")), 2);
    assert_eq!(files.len(), 3);
    assert_eq!((files[0].additions, files[0].deletions), (2, 1));
    assert_eq!(files[0].hunks[0].new_start, 10);
    assert_eq!(files[1].status, ChangeKind::Renamed);
    assert_eq!(files[1].old_path.as_deref(), Some("routes/old.php"));
    assert_eq!(files[2].status, ChangeKind::Added);
}

#[tokio::test]
async fn discussions_comments_replies_and_resolving() {
    let (server, gl) = host(vec![
        Route::json(
            "GET",
            &format!("{P}/merge_requests/7/discussions"),
            r#"[{"id":"d1","individual_note":false,"notes":[
                {"id":1,"body":"Why fillable?","author":{"username":"ben"},"created_at":"t1","system":false,"resolvable":true,"resolved":true,
                 "position":{"new_path":"app/Models/User.php","old_path":"app/Models/User.php","new_line":12,"old_line":null}},
                {"id":2,"body":"Mass assignment.","author":{"username":"anna"},"created_at":"t2","system":false,"resolvable":true,"resolved":true}]},
               {"id":"d2","individual_note":true,"notes":[{"id":3,"body":"added 1 commit","author":{"username":"anna"},"created_at":"t3","system":true,"resolvable":false}]}]"#,
        ),
        Route::json("GET", &format!("{P}/merge_requests/7"), r#"{"iid":7,"diff_refs":{"base_sha":"b","start_sha":"s","head_sha":"h"}}"#),
        Route::json("POST", &format!("{P}/merge_requests/7/discussions"), "{}").status(201),
        Route::json("POST", &format!("{P}/merge_requests/7/notes"), "{}").status(201),
        Route::json("POST", &format!("{P}/merge_requests/7/discussions/d1/notes"), "{}").status(201),
        Route::json("PUT", &format!("{P}/merge_requests/7/discussions/d1"), "{}"),
    ])
    .await;
    let ds = gl.discussions(&repo(), 7).await.unwrap();
    assert_eq!(ds.len(), 2);
    assert!(ds[0].resolvable && ds[0].resolved);
    assert_eq!(ds[0].notes.len(), 2);
    let pos = ds[0].position.as_ref().unwrap();
    assert_eq!((pos.path.as_str(), pos.new_line, pos.old_line), ("app/Models/User.php", Some(12), None));
    assert!(ds[1].notes[0].system);

    let at = Position {
        path: "app/Models/User.php".into(),
        old_path: None,
        new_line: Some(14),
        old_line: None,
    };
    gl.add_comment(&repo(), 7, "Nit: order", Some(&at)).await.unwrap();
    let body = server.find("POST", &format!("{P}/merge_requests/7/discussions")).json();
    assert_eq!(body["position"]["start_sha"], "s");
    assert_eq!(body["position"]["new_line"], 14);
    assert_eq!(body["position"]["old_path"], "app/Models/User.php");
    assert!(body["position"].get("old_line").is_none());

    gl.add_comment(&repo(), 7, "LGTM", None).await.unwrap();
    assert_eq!(server.find("POST", &format!("{P}/merge_requests/7/notes")).json()["body"], "LGTM");
    gl.reply(&repo(), 7, "d1", "Thanks").await.unwrap();
    gl.resolve(&repo(), 7, "d1", true).await.unwrap();
    assert_eq!(server.find("PUT", &format!("{P}/merge_requests/7/discussions/d1")).query, "resolved=true");
}

#[tokio::test]
async fn creates_draft_with_reviewers_and_merges() {
    let (server, gl) = host(vec![
        Route::json("GET", "/api/v4/users", r#"[{"id":42,"username":"ben"}]"#),
        Route::json("POST", &format!("{P}/merge_requests"), &mr_json(8, "Draft: Payment retries", "c")).status(201),
        Route::json("PUT", &format!("{P}/merge_requests/8/merge"), "{}"),
        Route::json("POST", &format!("{P}/merge_requests/8/approve"), "{}").status(201),
        Route::json("PUT", &format!("{P}/merge_requests/8"), "{}"),
    ])
    .await;
    let mr = gl
        .create_merge_request(
            &repo(),
            &NewMergeRequest {
                source_branch: "feature/retries".into(),
                target_branch: "main".into(),
                title: "Payment retries".into(),
                description: "Retries failed payments.".into(),
                draft: true,
                reviewers: vec!["ben".into()],
                labels: vec!["backend".into(), "payments".into()],
                remove_source_branch: true,
            },
        )
        .await
        .unwrap();
    assert!(mr.draft);
    assert_eq!(server.find("GET", "/api/v4/users").query, "username=ben");
    let body = server.find("POST", &format!("{P}/merge_requests")).json();
    assert_eq!(body["title"], "Draft: Payment retries");
    assert_eq!(body["reviewer_ids"][0], 42);
    assert_eq!(body["labels"], "backend,payments");

    gl.approve(&repo(), 8).await.unwrap();
    gl.merge(
        &repo(),
        8,
        &MergeOptions {
            method: MergeMethod::Squash,
            delete_source_branch: true,
            commit_message: Some("Payment retries".into()),
        },
    )
    .await
    .unwrap();
    let body = server.find("PUT", &format!("{P}/merge_requests/8/merge")).json();
    assert_eq!(body["squash"], true);
    assert_eq!(body["should_remove_source_branch"], true);
    assert_eq!(body["squash_commit_message"], "Payment retries");
    gl.close(&repo(), 8).await.unwrap();
    assert_eq!(server.find("PUT", &format!("{P}/merge_requests/8")).json()["state_event"], "close");
}

#[tokio::test]
async fn issues_pipelines_members_and_files() {
    let (server, gl) = host(vec![
        Route::json(
            "GET",
            &format!("{P}/issues"),
            r#"[{"iid":12,"title":"Login fails","state":"opened","author":{"username":"dora"},"labels":["bug"],"assignees":[],"user_notes_count":2,"created_at":"a","updated_at":"b","web_url":"w"}]"#,
        ),
        Route::json("GET", &format!("{P}/issues/12"), r#"{"iid":12,"title":"Login fails","state":"opened","description":"Steps","author":{"username":"dora"}}"#),
        Route::json("GET", &format!("{P}/issues/12/notes"), r#"[{"id":1,"body":"Confirmed","author":{"username":"anna"},"created_at":"c"}]"#),
        Route::json("POST", &format!("{P}/issues/12/notes"), "{}").status(201),
        Route::json("GET", &format!("{P}/pipelines"), r#"[{"id":5,"status":"success","ref":"main","sha":"abc"}]"#),
        Route::json("GET", &format!("{P}/pipelines/5/jobs"), r#"[{"name":"lint","stage":"test","status":"success"}]"#),
        Route::json("GET", &format!("{P}/members/all"), r#"[{"username":"ben","state":"active"},{"username":"old","state":"blocked"}]"#),
        Route::text("GET", &format!("{P}/repository/files/app%2FModels%2FUser.php/raw"), "<?php\n"),
    ])
    .await;
    let issues = gl.issues(&repo(), &IssueFilter::default()).await.unwrap();
    assert_eq!(issues.items[0].number, 12);
    assert_eq!(issues.items[0].labels[0].name, "bug");
    let issue = gl.issue(&repo(), 12).await.unwrap();
    assert_eq!(issue.notes[0].body, "Confirmed");
    gl.comment_issue(&repo(), 12, "On it").await.unwrap();

    let ci = gl.ci_status(&repo(), "main").await.unwrap().unwrap();
    assert_eq!(ci.status, CiStatus::Success);
    assert_eq!(ci.jobs[0].name, "lint");
    assert!(server.find("GET", &format!("{P}/pipelines")).query.contains("ref=main"));
    gl.ci_status(&repo(), "0123456789abcdef").await.unwrap();
    assert!(server.find("GET", &format!("{P}/pipelines")).query.contains("sha=0123456789abcdef"));

    let members = gl.members(&repo()).await.unwrap();
    assert_eq!(members.len(), 1);
    let text = gl.file_content(&repo(), "app/Models/User.php", "abc").await.unwrap();
    assert_eq!(text.as_deref(), Some("<?php\n"));
    assert_eq!(gl.file_content(&repo(), "missing.php", "abc").await.unwrap(), None);
}

#[tokio::test]
async fn errors_are_friendly() {
    let (_server, gl) = host(vec![
        Route::json("GET", "/api/v4/user", r#"{"message":"401 Unauthorized"}"#).status(401),
        Route::json("GET", &format!("{P}/merge_requests"), r#"{"message":"Too many requests"}"#)
            .status(429)
            .header("RateLimit-Reset", "4102444800"),
        Route::json("GET", &format!("{P}/merge_requests/1"), r#"{"message":"404 Not found"}"#).status(404),
        Route::json("POST", &format!("{P}/merge_requests"), r#"{"message":["Another open merge request already exists for this source branch: !3"]}"#).status(409),
    ])
    .await;
    let e = gl.current_user().await.unwrap_err().to_string();
    assert!(e.contains("token was rejected"), "{e}");
    let e = gl.merge_requests(&repo(), &MrFilter::default()).await.unwrap_err();
    assert!(matches!(e, HostError::RateLimited { reset: Some(_), .. }), "{e}");
    assert!(e.to_string().contains("rate limit"));
    let e = gl.merge_request(&repo(), 1).await.unwrap_err().to_string();
    assert_eq!(e, "GitLab: merge request !1 not found");
    let e = gl
        .create_merge_request(&repo(), &NewMergeRequest::default())
        .await
        .unwrap_err()
        .to_string();
    assert!(e.contains("Another open merge request already exists"), "{e}");
}
