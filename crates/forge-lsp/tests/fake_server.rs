//! End-to-end tests against `tests/support/fake_server.rs`.

use std::path::Path;
use std::time::Duration;

use forge_lsp::lsp_types::Position;
use forge_lsp::lsp_types::Range;
use forge_lsp::{Feature, LspClient, LspEvent, LspManager, LspServerConfig, ServerState};
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

#[tokio::test]
async fn several_servers_per_file_are_merged() {
    let dir = tempfile::tempdir().unwrap();
    let manager = LspManager::new(dir.path());
    manager.register_server(config());
    let mut lint = config();
    lint.id = "lint".into();
    lint.name = Some("Lint".into());
    lint.args = vec!["secondary".into()];
    manager.register_server(lint);
    let mut rx = manager.subscribe();

    let php = dir.path().join("a.php");
    manager
        .did_open(&php, "<?php\n// TODO\n// FIXME\n")
        .await
        .unwrap();
    assert_eq!(manager.running_clients_for(&php).len(), 2);

    // Diagnostics arrive per server; merged they contain both.
    let mut merged = Vec::new();
    for _ in 0..2 {
        let (server, uri, diags) = next_matching(&mut rx, |e| match e {
            LspEvent::Diagnostics {
                server_id,
                uri,
                diagnostics,
                ..
            } => Some((server_id, uri, diagnostics)),
            _ => None,
        })
        .await;
        merged = manager.merge_diagnostics(&server, &uri, diags);
    }
    let mut sources: Vec<_> = merged.iter().filter_map(|d| d.source.clone()).collect();
    sources.sort();
    assert_eq!(sources, vec!["fake", "lint"]);

    // Hover: first non-empty answer (the secondary server has none).
    let hover = manager.hover(&php, Position::new(1, 2)).await.unwrap();
    assert_eq!(hover.unwrap().contents, "**hover** 1:2");

    // Completion lists are merged and remember their server for resolve.
    let list = manager
        .completion(&php, Position::new(0, 0))
        .await
        .unwrap()
        .unwrap();
    let labels: Vec<_> = list.items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, vec!["hasMany", "$orders", "lint-ignore"]);
    assert_eq!(
        list.items[2].raw[forge_lsp::COMPLETION_SERVER_KEY],
        json!("lint")
    );
    let resolved = manager
        .completion_resolve(&php, list.items[0].raw.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(resolved.documentation.as_deref(), Some("Resolved docs"));

    // Definitions from both servers are deduplicated.
    let defs = manager.definition(&php, Position::new(0, 0)).await.unwrap();
    assert_eq!(defs.len(), 1);

    // Formatting goes to the server that supports it.
    let edits = manager.format(&php, 4, true).await.unwrap();
    assert_eq!(edits[0].new_text, "<?php");

    // Workspace symbols come from every running server.
    let symbols = manager.workspace_symbols("Order").await;
    assert_eq!(symbols.len(), 2);
    assert!(symbols
        .iter()
        .all(|s| s.name == "OrderSymbol" && s.kind == "class"));
    let mut servers: Vec<_> = symbols.iter().map(|s| s.server.as_str()).collect();
    servers.sort();
    assert_eq!(servers, vec!["fake", "lint"]);

    manager.shutdown_all().await;
}

#[tokio::test]
async fn rename_implementation_and_code_actions() {
    let dir = tempfile::tempdir().unwrap();
    let manager = LspManager::new(dir.path());
    manager.register_server(config());
    let mut lint = config();
    lint.id = "lint".into();
    lint.name = Some("Lint".into());
    lint.args = vec!["secondary".into()];
    lint.file_extensions = vec!["php".into(), "lint".into()];
    manager.register_server(lint);
    let mut rx = manager.subscribe();

    let php = dir.path().join("User.php");
    manager
        .did_open(&php, "class User\n// TODO\n")
        .await
        .unwrap();
    let (server, uri, diags) = next_matching(&mut rx, |e| match e {
        LspEvent::Diagnostics {
            server_id,
            uri,
            diagnostics,
            ..
        } if server_id == "fake" => Some((server_id, uri, diagnostics)),
        _ => None,
    })
    .await;
    manager.merge_diagnostics(&server, &uri, diags);

    // A feature one of the servers supports is not missing; a file served only by the
    // secondary server lacks it (the UI then explains why, e.g. Intelephense Premium).
    assert_eq!(manager.missing_feature(&php, Feature::Rename).await, None);
    let lint_file = dir.path().join("rules.lint");
    assert_eq!(
        manager.missing_feature(&lint_file, Feature::Rename).await,
        Some(vec!["Lint".to_string()])
    );
    assert!(manager
        .rename(&lint_file, Position::new(0, 0), "x")
        .await
        .is_err());

    let prepared = manager
        .prepare_rename(&php, Position::new(0, 7))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(prepared.placeholder.as_deref(), Some("User"));
    let edit = manager
        .rename(&php, Position::new(0, 7), "Customer")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(edit.paths(), vec![php.display().to_string()]);
    let mut store = forge_lsp::forge_codeintel::MemoryStore::default();
    store
        .0
        .insert(php.display().to_string(), "class User\n// TODO\n".into());
    forge_lsp::forge_codeintel::apply_workspace_edit(&edit, &mut store).unwrap();
    assert_eq!(
        store.0[&php.display().to_string()],
        "class Customer\n// TODO\n"
    );

    let impls = manager
        .implementation(&php, Position::new(0, 7))
        .await
        .unwrap();
    assert_eq!(impls.len(), 1);
    assert_eq!(impls[0].range.start.line, 3);

    // Code actions get the diagnostics under the range as context.
    let line1 = Range::new(Position::new(1, 0), Position::new(1, 7));
    let actions = manager.code_actions(&php, line1).await.unwrap();
    assert_eq!(actions[0].title, "Fix 1 problem(s)");
    assert!(actions[0].needs_resolve);
    let resolved = manager
        .resolve_code_action(&php, &actions[0].server, actions[0].raw.clone())
        .await
        .unwrap();
    assert_eq!(resolved.edit.unwrap().edit_count(), 1);
    let elsewhere = Range::new(Position::new(0, 0), Position::new(0, 1));
    assert_eq!(
        manager.code_actions(&php, elsewhere).await.unwrap()[0].title,
        "Fix 0 problem(s)"
    );

    // A command's edit comes back as `workspace/applyEdit`.
    let cmd = actions[1].command.clone().unwrap();
    manager
        .execute_command(&php, "fake", &cmd.command, cmd.arguments)
        .await
        .unwrap();
    let label = next_matching(&mut rx, |e| match e {
        LspEvent::ApplyEdit { label, edit, .. } => {
            assert!(edit.changes.is_some());
            label
        }
        _ => None,
    })
    .await;
    assert_eq!(label, "fake");

    manager.shutdown_all().await;
}

/// Starts the fake server locally but, like the launcher of a remote project, has no process
/// on the server's machine for it to watch.
struct RemoteLikeLauncher;

impl forge_lsp::ServerLauncher for RemoteLikeLauncher {
    fn launch<'a>(
        &'a self,
        config: &'a LspServerConfig,
        root: &'a Path,
    ) -> futures_util::future::BoxFuture<'a, forge_core::Result<forge_lsp::ServerIo>> {
        Box::pin(async move {
            let mut io = forge_lsp::LocalLauncher.launch(config, root).await?;
            io.parent_pid = None;
            Ok(io)
        })
    }
}

#[tokio::test]
async fn sends_the_parent_process_only_for_servers_on_this_machine() {
    let dir = tempfile::tempdir().unwrap();
    let process_id = |launcher: Option<&'static dyn forge_lsp::ServerLauncher>| {
        let root = dir.path().to_path_buf();
        async move {
            let (tx, mut rx) = broadcast::channel(256);
            let client = match launcher {
                Some(l) => LspClient::start_with_launcher(config(), &root, tx, l).await,
                None => LspClient::start_with_events(config(), &root, tx).await,
            }
            .unwrap();
            let log = next_matching(&mut rx, |e| match e {
                LspEvent::Log { message, .. } if message.starts_with("processId:") => Some(message),
                _ => None,
            })
            .await;
            client.shutdown().await;
            log
        }
    };
    assert_eq!(
        process_id(None).await,
        format!("processId: {}", std::process::id())
    );
    // Intelephense exits when the given process does not exist on its machine.
    assert_eq!(
        process_id(Some(&RemoteLikeLauncher)).await,
        "processId: null"
    );
}
