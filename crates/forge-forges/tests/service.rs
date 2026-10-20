mod common;

use std::sync::Arc;

use common::{MockServer, Route};
use forge_forges::*;

#[tokio::test]
async fn login_stores_token_in_keychain_and_detects_self_hosted_remotes() {
    let server = MockServer::start(vec![Route::json(
        "GET",
        "/api/v1/user",
        r#"{"login":"marlon","full_name":"Marlon"}"#,
    )])
    .await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("forges.toml");
    let secrets = Arc::new(MemoryStore::default());
    let svc = ForgeService::new(Some(path.clone()), secrets.clone())
        .unwrap()
        .with_env(|_| None);

    let info = svc
        .login(HostKind::Forgejo, &server.base, " tok-123 ", None)
        .await
        .unwrap();
    assert_eq!(info.account.username.as_deref(), Some("marlon"));
    assert_eq!(info.token_source, TokenSource::Keychain);
    assert_eq!(
        secrets.get(&info.account.id).unwrap().as_deref(),
        Some("tok-123")
    );
    assert_eq!(
        server.find("GET", "/api/v1/user").headers["authorization"],
        "token tok-123"
    );
    let file = std::fs::read_to_string(&path).unwrap();
    assert!(file.contains("marlon") && !file.contains("tok-123"));

    // Logging in again for the same server replaces the account.
    svc.login(HostKind::Forgejo, &server.base, "tok-456", None)
        .await
        .unwrap();
    assert_eq!(svc.accounts().len(), 1);
    assert_eq!(
        svc.verify(&info.account.id).await.unwrap().username,
        "marlon"
    );

    // The account makes its server known for remote detection (here: an SSH remote).
    let host = server
        .base
        .trim_start_matches("http://")
        .split(':')
        .next()
        .unwrap()
        .to_string();
    let refs = svc.detect(&[("origin".into(), format!("git@{host}:acme/acme-shop.git"))]);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].kind, HostKind::Forgejo);
    assert_eq!(refs[0].path, "acme/acme-shop");
    assert!(svc.account_for(&refs[0]).is_some());

    // Reloading from disk keeps the account.
    let again = ForgeService::new(Some(path), secrets.clone()).unwrap();
    assert_eq!(again.accounts()[0].account.id, info.account.id);

    svc.logout(&info.account.id).unwrap();
    assert!(svc.accounts().is_empty());
    assert_eq!(secrets.get(&info.account.id).unwrap(), None);
}

#[tokio::test]
async fn rejected_tokens_are_not_stored() {
    let server = MockServer::start(vec![Route::json(
        "GET",
        "/api/v4/user",
        r#"{"message":"401 Unauthorized"}"#,
    )
    .status(401)])
    .await;
    let secrets = Arc::new(MemoryStore::default());
    let svc = ForgeService::new(None, secrets).unwrap();
    let err = svc
        .login(HostKind::Gitlab, &server.base, "bad", None)
        .await
        .unwrap_err();
    assert!(matches!(err, HostError::Unauthorized { .. }));
    assert!(svc.accounts().is_empty());
}
