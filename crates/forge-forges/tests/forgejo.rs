mod common;

use common::{MockServer, Route};
use forge_forges::*;

const R: &str = "/api/v1/repos/acme/acme-shop";

fn repo() -> RepoRef {
    RepoRef {
        kind: HostKind::Forgejo,
        server: "https://git.acme.dev".into(),
        path: "acme/acme-shop".into(),
        remote: "origin".into(),
    }
}

fn pr(number: u64, title: &str, merged: bool, state: &str, user: &str) -> String {
    format!(
        r#"{{"number":{number},"title":"{title}","state":"{state}","merged":{merged},
        "user":{{"login":"{user}","full_name":"","avatar_url":""}},
        "head":{{"ref":"feature/{number}","sha":"sha{number}"}},"base":{{"ref":"main","sha":"base"}},
        "labels":[{{"name":"backend","color":"e11d21"}}],"comments":2,"requested_reviewers":[{{"login":"ben"}}],
        "created_at":"2026-10-01T10:00:00Z","updated_at":"2026-10-0{number}T10:00:00Z","html_url":"https://git.acme.dev/acme/acme-shop/pulls/{number}"}}"#
    )
}

async fn host(routes: Vec<Route>) -> (MockServer, Forgejo) {
    let server = MockServer::start(routes).await;
    let fj = Forgejo::new(
        forge_forges::http::client(),
        &format!("{}/api/v1", server.base),
        Some("fj-token".into()),
    );
    (server, fj)
}

#[tokio::test]
async fn lists_pulls_with_client_side_filters_and_statuses() {
    let list = format!(
        "[{},{},{}]",
        pr(1, "WIP: Add wishlist", false, "open", "anna"),
        pr(2, "Fix cart totals", false, "open", "ben"),
        pr(3, "Old", true, "closed", "anna")
    );
    let (server, fj) = host(vec![
        Route::json("GET", &format!("{R}/pulls"), &list)
            .header(
                "Link",
                &format!("<{{base}}{R}/pulls?page=2&limit=20>; rel=\"next\""),
            )
            .header("X-Total-Count", "23"),
        Route::json(
            "GET",
            &format!("{R}/commits/sha1/status"),
            r#"{"state":"pending","statuses":[{"context":"ci/woodpecker","status":"pending"}]}"#,
        ),
        Route::json(
            "GET",
            &format!("{R}/commits/sha3/status"),
            r#"{"state":"success","statuses":[{"context":"ci","status":"success"}]}"#,
        ),
    ])
    .await;
    let page = fj
        .merge_requests(
            &repo(),
            &MrFilter {
                state: StateFilter::All,
                author: Some("anna".into()),
                ..MrFilter::default()
            },
        )
        .await
        .unwrap();
    let req = server.find("GET", &format!("{R}/pulls"));
    assert_eq!(
        req.headers.get("authorization").map(String::as_str),
        Some("token fj-token")
    );
    assert!(req.query.contains("state=all"));
    assert_eq!(page.next_page, Some(2));
    assert_eq!(page.total, Some(23));
    assert_eq!(page.items.len(), 2);
    assert!(page.items[0].draft);
    assert_eq!(page.items[0].title, "Add wishlist");
    assert_eq!(page.items[0].ci, Some(CiStatus::Pending));
    assert_eq!(page.items[0].labels[0].color.as_deref(), Some("#e11d21"));
    assert_eq!(page.items[1].state, MrState::Merged);
    assert_eq!(page.items[1].ci, Some(CiStatus::Success));

    let merged = fj
        .merge_requests(
            &repo(),
            &MrFilter {
                state: StateFilter::Merged,
                ..MrFilter::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(merged.items.len(), 1);
    assert_eq!(merged.items[0].number, 3);
}

#[tokio::test]
async fn details_reviews_and_files_from_the_diff() {
    let diff = "diff --git a/app/Models/User.php b/app/Models/User.php\nindex 1..2 100644\n--- a/app/Models/User.php\n+++ b/app/Models/User.php\n@@ -1,3 +1,4 @@\n <?php\n+use Foo;\n namespace App;\n-old\n+new\ndiff --git a/new.txt b/new.txt\nnew file mode 100644\n--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1 @@\n+hello\n";
    let (_server, fj) = host(vec![
        Route::json("GET", &format!("{R}/pulls/4"), &pr(4, "Add wishlist", false, "open", "anna").replace("\"comments\":2", "\"comments\":2,\"body\":\"Adds a wishlist.\",\"mergeable\":true")),
        Route::json(
            "GET",
            &format!("{R}/pulls/4/reviews"),
            r#"[{"id":1,"user":{"login":"ben"},"state":"REQUEST_CHANGES","body":"","comments_count":1,"submitted_at":"t1"},
               {"id":2,"user":{"login":"ben"},"state":"APPROVED","body":"","comments_count":0,"submitted_at":"t2"},
               {"id":3,"user":{"login":"cleo"},"state":"COMMENT","body":"Looks fine","comments_count":0,"submitted_at":"t3"}]"#,
        ),
        Route::json("GET", &format!("{R}/commits/sha4/status"), r#"{"state":"failure","sha":"sha4","statuses":[{"context":"lint","status":"success","target_url":"https://ci/1"},{"context":"phpunit","status":"failure","target_url":"https://ci/2"}]}"#),
        Route::text("GET", &format!("{R}/pulls/4.diff"), diff),
    ])
    .await;
    let d = fj.merge_request(&repo(), 4).await.unwrap();
    assert_eq!(d.description, "Adds a wishlist.");
    let ben = d
        .reviewers
        .iter()
        .find(|r| r.user.username == "ben")
        .unwrap();
    assert_eq!(ben.state, ReviewState::Approved);
    let cleo = d
        .reviewers
        .iter()
        .find(|r| r.user.username == "cleo")
        .unwrap();
    assert_eq!(cleo.state, ReviewState::Commented);
    assert_eq!(d.approvals.approved_by.len(), 1);
    let p = d.pipeline.unwrap();
    assert_eq!(p.status, CiStatus::Failed);
    assert_eq!(p.jobs.len(), 2);
    assert_eq!(d.mergeable, Some(true));

    let files = fj.merge_request_files(&repo(), 4).await.unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!((files[0].additions, files[0].deletions), (2, 1));
    assert_eq!(files[1].status, ChangeKind::Added);
}

#[tokio::test]
async fn inline_threads_comments_and_replies() {
    let (server, fj) = host(vec![
        Route::json("GET", &format!("{R}/issues/4/comments"), r#"[{"id":10,"body":"Please add tests","user":{"login":"ben"},"created_at":"2026-10-01T11:00:00Z"}]"#),
        Route::json("GET", &format!("{R}/pulls/4/reviews"), r#"[{"id":1,"user":{"login":"ben"},"state":"COMMENT","body":"","comments_count":2,"submitted_at":"2026-10-01T12:00:00Z"},{"id":2,"user":{"login":"anna"},"state":"COMMENT","body":"","comments_count":1,"submitted_at":"2026-10-01T13:00:00Z"}]"#),
        Route::json(
            "GET",
            &format!("{R}/pulls/4/reviews/1/comments"),
            r#"[{"id":21,"body":"Hash this?","user":{"login":"ben"},"path":"app/Models/User.php","position":24,"original_position":0,"created_at":"2026-10-01T12:00:00Z"},
               {"id":22,"body":"Old line","user":{"login":"ben"},"path":"app/Models/User.php","position":0,"original_position":5,"created_at":"2026-10-01T12:01:00Z","resolver":{"login":"anna"}}]"#,
        ),
        Route::json("GET", &format!("{R}/pulls/4/reviews/2/comments"), r#"[{"id":23,"body":"Done","user":{"login":"anna"},"path":"app/Models/User.php","position":24,"created_at":"2026-10-01T13:00:00Z"}]"#),
        Route::json("GET", &format!("{R}/pulls/4"), &pr(4, "x", false, "open", "anna")),
        Route::json("POST", &format!("{R}/pulls/4/reviews"), "{}"),
        Route::json("POST", &format!("{R}/issues/4/comments"), "{}").status(201),
    ])
    .await;
    let ds = fj.discussions(&repo(), 4).await.unwrap();
    assert_eq!(ds.len(), 3, "{ds:#?}");
    assert_eq!(ds[0].id, "c10");
    let thread = ds
        .iter()
        .find(|d| d.id == "n24:app/Models/User.php")
        .unwrap();
    assert_eq!(thread.notes.len(), 2);
    assert_eq!(thread.notes[1].body, "Done");
    assert!(!thread.resolved);
    let old = ds
        .iter()
        .find(|d| d.id == "o5:app/Models/User.php")
        .unwrap();
    assert!(old.resolved);
    assert_eq!(old.position.as_ref().unwrap().old_line, Some(5));

    fj.reply(&repo(), 4, "n24:app/Models/User.php", "Thanks")
        .await
        .unwrap();
    let body = server.find("POST", &format!("{R}/pulls/4/reviews")).json();
    assert_eq!(body["event"], "COMMENT");
    assert_eq!(body["commit_id"], "sha4");
    assert_eq!(body["comments"][0]["path"], "app/Models/User.php");
    assert_eq!(body["comments"][0]["new_position"], 24);

    fj.reply(&repo(), 4, "c10", "Added").await.unwrap();
    assert_eq!(
        server
            .find("POST", &format!("{R}/issues/4/comments"))
            .json()["body"],
        "Added"
    );
    let err = fj.resolve(&repo(), 4, "n24:x", true).await.unwrap_err();
    assert!(matches!(err, HostError::Unsupported { .. }));

    fj.approve(&repo(), 4).await.unwrap();
    assert_eq!(
        server.find("POST", &format!("{R}/pulls/4/reviews")).json()["event"],
        "APPROVED"
    );
}

#[tokio::test]
async fn create_merge_close_and_issues() {
    let (server, fj) = host(vec![
        Route::json("GET", &format!("{R}/labels"), r#"[{"id":3,"name":"backend"},{"id":4,"name":"ui"}]"#),
        Route::json("POST", &format!("{R}/pulls"), &pr(9, "WIP: Search", false, "open", "anna")).status(201),
        Route::json("POST", &format!("{R}/pulls/9/requested_reviewers"), "{}").status(201),
        Route::json("POST", &format!("{R}/pulls/9/merge"), ""),
        Route::json("PATCH", &format!("{R}/pulls/9"), "{}").status(201),
        Route::json(
            "GET",
            &format!("{R}/issues"),
            r#"[{"number":31,"title":"Checkout 500","state":"open","user":{"login":"dora"},"labels":[],"assignees":null,"comments":4,"created_at":"a","updated_at":"b","html_url":"h"},
               {"number":9,"title":"PR","state":"open","user":{"login":"anna"},"pull_request":{"merged":false}}]"#,
        ),
        Route::json("GET", &format!("{R}/actions/tasks"), r#"{"workflow_runs":[{"id":77,"display_title":"Add search","head_branch":"feature/search","head_sha":"x","status":"running","url":"https://git.acme.dev/acme/acme-shop/actions/runs/7"},{"id":76,"head_branch":"main","status":"success"}]}"#),
        Route::text("GET", &format!("{R}/raw/app/Models/User.php"), "<?php // main\n"),
    ])
    .await;
    let mr = fj
        .create_merge_request(
            &repo(),
            &NewMergeRequest {
                source_branch: "feature/search".into(),
                target_branch: "main".into(),
                title: "Search".into(),
                draft: true,
                reviewers: vec!["ben".into()],
                labels: vec!["UI".into()],
                ..NewMergeRequest::default()
            },
        )
        .await
        .unwrap();
    assert!(mr.draft);
    let body = server.find("POST", &format!("{R}/pulls")).json();
    assert_eq!(body["title"], "WIP: Search");
    assert_eq!(body["labels"][0], 4);
    assert_eq!(
        server
            .find("POST", &format!("{R}/pulls/9/requested_reviewers"))
            .json()["reviewers"][0],
        "ben"
    );

    fj.merge(
        &repo(),
        9,
        &MergeOptions {
            method: MergeMethod::Rebase,
            delete_source_branch: true,
            commit_message: None,
        },
    )
    .await
    .unwrap();
    let body = server.find("POST", &format!("{R}/pulls/9/merge")).json();
    assert_eq!(body["Do"], "rebase");
    assert_eq!(body["delete_branch_after_merge"], true);
    fj.close(&repo(), 9).await.unwrap();
    assert_eq!(
        server.find("PATCH", &format!("{R}/pulls/9")).json()["state"],
        "closed"
    );

    let issues = fj
        .issues(
            &repo(),
            &IssueFilter {
                search: Some("checkout".into()),
                ..IssueFilter::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(issues.items.len(), 1);
    assert_eq!(issues.items[0].comments, 4);
    let q = server.find("GET", &format!("{R}/issues")).query;
    assert!(q.contains("type=issues") && q.contains("q=checkout"), "{q}");

    let runs = fj.pipelines(&repo(), Some("feature/search")).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, CiStatus::Running);
    assert_eq!(runs[0].name.as_deref(), Some("Add search"));
    assert_eq!(
        fj.file_content(&repo(), "app/Models/User.php", "main")
            .await
            .unwrap()
            .as_deref(),
        Some("<?php // main\n")
    );
}

#[tokio::test]
async fn forbidden_and_unreachable_servers() {
    let (_server, fj) = host(vec![Route::json(
        "GET",
        &format!("{R}/pulls/1"),
        r#"{"message":"user does not have permission"}"#,
    )
    .status(403)])
    .await;
    let e = fj.merge_request(&repo(), 1).await.unwrap_err().to_string();
    assert_eq!(e, "Forgejo: access denied (user does not have permission)");

    let dead = format!("127.0.0.1:{}", forge_ports::free_port().unwrap());
    let fj = Forgejo::new(
        forge_forges::http::client(),
        &format!("http://{dead}/api/v1"),
        None,
    );
    let e = fj.current_user().await.unwrap_err().to_string();
    assert!(e.starts_with("Forgejo: cannot connect to"), "{e}");
}

#[tokio::test]
async fn lists_the_users_repositories_newest_first() {
    let (_server, fj) = host(vec![Route::json(
        "GET",
        "/api/v1/user/repos",
        r#"[{"full_name":"marlon/old","name":"old","private":false,"fork":false,"clone_url":"https://codeberg.org/marlon/old.git",
             "ssh_url":"","html_url":"https://codeberg.org/marlon/old","updated_at":"2025-01-01T00:00:00Z"},
            {"full_name":"marlon/forge","name":"forge","description":"IDE","private":true,"fork":false,
             "clone_url":"https://codeberg.org/marlon/forge.git","ssh_url":"git@codeberg.org:marlon/forge.git",
             "html_url":"https://codeberg.org/marlon/forge","updated_at":"2026-10-01T00:00:00Z"}]"#,
    )])
    .await;
    let repos = fj.repositories().await.unwrap();
    assert_eq!(repos[0].path, "marlon/forge");
    assert_eq!(repos[1].ssh_url, None);
    assert!(repos[0].private);
}

#[tokio::test]
async fn submits_a_review_with_all_comments_at_once() {
    let (server, fj) = host(vec![
        Route::json(
            "GET",
            &format!("{R}/pulls/4"),
            &pr(4, "x", false, "open", "anna"),
        ),
        Route::json("POST", &format!("{R}/pulls/4/reviews"), "{}"),
    ])
    .await;
    let review = NewReview {
        event: ReviewEvent::Approve,
        body: "LGTM".into(),
        comments: vec![ReviewComment {
            body: "Nit".into(),
            position: Position {
                path: "a.php".into(),
                new_line: Some(7),
                ..Position::default()
            },
            start_line: None,
        }],
    };
    fj.submit_review(&repo(), 4, &review).await.unwrap();
    assert_eq!(server.count("POST", &format!("{R}/pulls/4/reviews")), 1);
    let body = server.find("POST", &format!("{R}/pulls/4/reviews")).json();
    assert_eq!(body["event"], "APPROVED");
    assert_eq!(body["commit_id"], "sha4");
    assert_eq!(body["comments"][0]["new_position"], 7);
}
