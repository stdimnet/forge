//! End-to-end tests against `tests/support/fake_server.rs`.

use std::path::Path;
use std::time::Duration;

use forge_lsp::lsp_types::Position;
use forge_lsp::{LspClient, LspEvent, LspManager, LspServerConfig, ServerState};
use serde_json::json;
use tokio::sync::broadcast;

fn config() -> LspServerConfig {
    LspServerConfig {
        id: "fake".into(),
        name: Some("Fake LS".into()),
        language_ids: vec!["php".into()],
        file_extensions: vec!["php".into()],
        command: env!("CARGO_BIN_EXE_forge-lsp-fake-server").into(),
        args: vec![],
        initialization_options: json!({"licenceKey": null}),
        settings: json!({"fake": {"answer": 42}}),
    }
}

async fn next_matching<T>(
    rx: &mut broadcast::Receiver<LspEvent>,
    mut f: impl FnMut(LspEvent) -> Option<T>,
) -> T {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let ev = rx.recv().await.expect("event channel open");
            if let Some(v) = f(ev) {
                return v;
            }
        }
    })
    .await
    .expect("expected event did not arrive")
}

#[tokio::test]
async fn client_handshake_requests_and_server_requests() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("User.php");
    let (tx, mut rx) = broadcast::channel(256);
    let client = LspClient::start_with_events(config(), dir.path(), tx)
        .await
        .unwrap();
    assert!(client.capabilities().unwrap().hover_provider.is_some());

    // The server asked for configuration; we answered from `settings`.
    let log = next_matching(&mut rx, |e| match e {
        LspEvent::Log { message, .. } if message.starts_with("config:") => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(log, "config: [42,null]");

    client.did_open(&file, "<?php\n// TODO one\n").unwrap();
    let diags = next_matching(&mut rx, |e| match e {
        LspEvent::Diagnostics { diagnostics, .. } => Some(diagnostics),
        _ => None,
    })
    .await;
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].range.start.line, 1);

    client.did_change(&file, "<?php\n").unwrap();
    let diags = next_matching(&mut rx, |e| match e {
        LspEvent::Diagnostics { diagnostics, .. } => Some(diagnostics),
        _ => None,
    })
    .await;
    assert!(diags.is_empty());

    client.did_save(&file, "<?php\n").unwrap();
    let saved = next_matching(&mut rx, |e| match e {
        LspEvent::Log { message, .. } if message.starts_with("saved") => Some(message),
        _ => None,
    })
    .await;
    assert_eq!(saved, "saved with text: true");

    // Concurrent requests are correlated by id.
    let (hover, completion, symbols) = tokio::join!(
        client.hover(&file, Position::new(3, 7)),
        client.completion(&file, Position::new(0, 0)),
        client.document_symbols(&file),
    );
    let hover = forge_lsp::model::EditorHover::from(hover.unwrap().unwrap());
    assert_eq!(hover.contents, "**hover** 3:7");
    let completion = forge_lsp::model::EditorCompletionList::from(completion.unwrap().unwrap());
    assert_eq!(completion.items[0].kind, "method");
    assert!(symbols.unwrap().is_some());

    let resolved = client
        .completion_resolve(serde_json::from_value(completion.items[0].raw.clone()).unwrap())
        .await
        .unwrap();
    assert!(resolved.documentation.is_some());

    // Server errors surface as errors.
    let err = client
        .request_raw("custom/unknown", json!({}), Duration::from_secs(5))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("unknown method"));

    client.did_close(&file).unwrap();
    client.shutdown().await;
    assert_eq!(client.state(), ServerState::Stopped);
    assert!(client.hover(&file, Position::new(0, 0)).await.is_err());
}

#[tokio::test]
async fn manager_routes_by_extension_and_starts_lazily() {
    let dir = tempfile::tempdir().unwrap();
    let manager = LspManager::new(dir.path());
    manager.register_server(config());
    let mut rx = manager.subscribe();

    assert_eq!(manager.statuses()[0].state, ServerState::NotStarted);
    let txt = dir.path().join("notes.txt");
    assert!(manager
        .hover(&txt, Position::new(0, 0))
        .await
        .unwrap()
        .is_none());
    assert_eq!(manager.statuses()[0].state, ServerState::NotStarted);

    let php = dir.path().join("src/Order.php");
    manager.did_open(&php, "<?php\n").await.unwrap();
    let path = next_matching(&mut rx, |e| match e {
        LspEvent::Diagnostics { uri, .. } => uri.to_file_path().ok(),
        _ => None,
    })
    .await;
    assert_eq!(path, php);
    assert!(matches!(
        manager.statuses()[0].state,
        ServerState::Ready | ServerState::Busy { .. }
    ));

    let defs = manager.definition(&php, Position::new(1, 1)).await.unwrap();
    assert_eq!(Path::new(&defs[0].path), php);
    let refs = manager.references(&php, Position::new(1, 1)).await.unwrap();
    assert_eq!(refs.len(), 2);
    let symbols = manager.document_symbols(&php).await.unwrap();
    assert_eq!(symbols[0].children[0].name, "orders");
    let sig = manager
        .signature_help(&php, Position::new(0, 0))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sig.signatures[0].parameters[1].label, [10, 16]);
    assert_eq!(sig.active_parameter, 1);

    manager.did_change(&php, "  <?php   \n").await.unwrap();
    let edits = manager.format(&php, 4, true).await.unwrap();
    assert_eq!(edits[0].new_text, "<?php");

    manager.shutdown_all().await;
}

#[tokio::test]
async fn missing_server_binary_reports_failure() {
    let dir = tempfile::tempdir().unwrap();
    let manager = LspManager::new(dir.path());
    let mut cfg = config();
    cfg.command = "forge-definitely-not-installed-ls".into();
    manager.register_server(cfg);
    let php = dir.path().join("a.php");
    manager.did_open(&php, "<?php").await.unwrap();
    assert!(manager
        .hover(&php, Position::new(0, 0))
        .await
        .unwrap()
        .is_none());
    assert!(matches!(
        manager.statuses()[0].state,
        ServerState::Failed { .. }
    ));
}
