//! A guest's language server features, answered by the host's real `LspManager` talking to a
//! fake language server (an in-process stand-in for Intelephense that speaks the real
//! protocol over pipes): diagnostics, hover, completion and Go to Declaration.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_backend::{Backend, LocalBackend};
use forge_collab::excludes::Excludes;
use forge_collab::guest::{GuestEvent, GuestSession};
use forge_collab::guest_backend::lsp_host::{LspServices, ManagerSource};
use forge_collab::guest_backend::LspQuery;
use forge_collab::host::{DevAutoAccept, HostConfig, HostSession, DEV_PREVIEW_NOTICE};
use forge_collab::permissions::Permissions;
use forge_collab::transport::memory::MemoryNetwork;
use forge_lsp::launch::{ServerIo, ServerLauncher, ServerProcess};
use forge_lsp::model::EditorDiagnostic;
use forge_lsp::{LspEvent, LspManager, LspServerConfig};
use futures_util::future::BoxFuture;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

async fn read_msg<R: AsyncBufReadExt + Unpin>(r: &mut R) -> Option<Value> {
    let mut len = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).await.ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.strip_prefix("Content-Length:") {
            len = v.trim().parse().ok();
        }
    }
    let mut buf = vec![0; len?];
    r.read_exact(&mut buf).await.ok()?;
    serde_json::from_slice(&buf).ok()
}

async fn write_msg<W: AsyncWriteExt + Unpin>(w: &mut W, msg: Value) {
    let body = msg.to_string();
    let _ = w
        .write_all(format!("Content-Length: {}\r\n\r\n{body}", body.len()).as_bytes())
        .await;
    let _ = w.flush().await;
}

/// The fake server: warns about `TODO`, canned hover, completion and definition answers.
async fn fake_server(stdin: tokio::io::DuplexStream, mut stdout: tokio::io::DuplexStream) {
    let mut input = BufReader::new(stdin);
    let mut root_uri = String::new();
    while let Some(msg) = read_msg(&mut input).await {
        let method = msg["method"].as_str().unwrap_or_default().to_string();
        let id = msg.get("id").cloned();
        let params = &msg["params"];
        let uri = params["textDocument"]["uri"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let range = |l: u32, a: u32, b: u32| json!({"start": {"line": l, "character": a}, "end": {"line": l, "character": b}});
        let result = match method.as_str() {
            "initialize" => {
                root_uri = params["rootUri"].as_str().unwrap_or_default().to_string();
                json!({"capabilities": {
                    "textDocumentSync": {"openClose": true, "change": 1},
                    "hoverProvider": true,
                    "completionProvider": {"resolveProvider": true},
                    "definitionProvider": true,
                    "workspaceSymbolProvider": true
                }})
            }
            // 600 bundled stubs outside the project (more than the host's limit of 500),
            // then a library class in `vendor/` and one class of the project.
            "workspace/symbol" => {
                let mut hits: Vec<Value> = (0..600)
                    .map(|i| json!({"name": format!("UserStub{i}"), "kind": 5,
                        "location": {"uri": "file:///elsewhere/stubs/Core.php", "range": range(i, 0, 5)}}))
                    .collect();
                hits.push(json!({"name": "UserProvider", "kind": 5,
                    "location": {"uri": format!("{}/vendor/acme/auth/UserProvider.php", root_uri.trim_end_matches('/')), "range": range(0, 6, 18)}}));
                hits.push(json!({"name": "User", "kind": 5,
                    "location": {"uri": format!("{}/app/Models/User.php", root_uri.trim_end_matches('/')), "range": range(2, 6, 10)}}));
                json!(hits)
            }
            "textDocument/didOpen" | "textDocument/didChange" => {
                let text = params["textDocument"]["text"]
                    .as_str()
                    .or_else(|| params["contentChanges"][0]["text"].as_str())
                    .unwrap_or_default();
                let diagnostics: Vec<Value> = text
                    .lines()
                    .enumerate()
                    .filter_map(|(i, l)| {
                        l.find("TODO").map(|c| {
                            json!({"range": range(i as u32, c as u32, c as u32 + 4), "severity": 2,
                                   "source": "intelephense", "message": "Unfinished work"})
                        })
                    })
                    .collect();
                write_msg(
                    &mut stdout,
                    json!({"jsonrpc": "2.0", "method": "textDocument/publishDiagnostics",
                           "params": {"uri": uri, "diagnostics": diagnostics}}),
                )
                .await;
                continue;
            }
            "textDocument/hover" => json!({"contents": {"kind": "markdown",
                "value": format!("**User** at {}:{}", params["position"]["line"], params["position"]["character"])}}),
            "textDocument/completion" => json!({"isIncomplete": false, "items": [
                {"label": "firstName", "kind": 5, "detail": "string"},
                {"label": "orders", "kind": 2, "detail": "HasMany"}
            ]}),
            "textDocument/definition" => json!([
                {"uri": uri, "range": range(0, 0, 5)},
                {"uri": "file:///elsewhere/stubs/Core.php", "range": range(0, 0, 5)}
            ]),
            "shutdown" => Value::Null,
            "exit" => break,
            _ => {
                if let Some(id) = id {
                    write_msg(
                        &mut stdout,
                        json!({"jsonrpc": "2.0", "id": id, "result": null}),
                    )
                    .await;
                }
                continue;
            }
        };
        if let Some(id) = id {
            write_msg(
                &mut stdout,
                json!({"jsonrpc": "2.0", "id": id, "result": result}),
            )
            .await;
        }
    }
}

struct Fake;
struct FakeProcess;

impl ServerProcess for FakeProcess {
    fn stop(&mut self, _grace: Duration) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
    fn kill(&mut self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
}

impl ServerLauncher for Fake {
    fn launch<'a>(
        &'a self,
        _config: &'a LspServerConfig,
        _root: &'a Path,
    ) -> BoxFuture<'a, forge_core::Result<ServerIo>> {
        Box::pin(async {
            let (client_out, server_in) = tokio::io::duplex(1 << 20);
            let (server_out, client_in) = tokio::io::duplex(1 << 20);
            tokio::spawn(fake_server(server_in, server_out));
            Ok(ServerIo {
                stdin: Box::pin(client_out),
                stdout: Box::pin(client_in),
                stderr: None,
                process: Box::new(FakeProcess),
                parent_pid: None,
            })
        })
    }
}

struct Source(Arc<LspManager>);

impl ManagerSource for Source {
    fn manager(&self) -> Result<Arc<LspManager>, String> {
        Ok(self.0.clone())
    }
}

/// What a guest received: the file and its diagnostics.
type Diagnostics = Arc<Mutex<Vec<(String, Vec<EditorDiagnostic>)>>>;

struct Setup {
    _dir: tempfile::TempDir,
    host: Arc<HostSession>,
    guest: Arc<GuestSession>,
    diagnostics: Diagnostics,
}

async fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let root: PathBuf = dir.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join("app/Models")).unwrap();
    std::fs::write(
        root.join("app/Models/User.php"),
        "<?php\n// TODO rename\nclass User {}\n",
    )
    .unwrap();
    std::fs::write(root.join(".env"), "APP_KEY=secret\n").unwrap();
    let backend: Arc<dyn Backend> = Arc::new(LocalBackend::open(&root).unwrap());
    let manager = Arc::new(LspManager::with_launcher(root.clone(), Arc::new(Fake)));
    manager.register_server(LspServerConfig {
        id: "intelephense".into(),
        name: Some("Intelephense".into()),
        language_ids: vec!["php".into()],
        file_extensions: vec!["php".into()],
        command: "intelephense".into(),
        args: vec![],
        initialization_options: Value::Null,
        settings: Value::Null,
        ..Default::default()
    });
    let net = MemoryNetwork::new();
    let host = HostSession::start(
        Arc::new(net.endpoint([1; 32])),
        backend,
        HostConfig {
            host_name: "Robert".into(),
            machine: None,
            permissions: Permissions::READ_ONLY,
            excludes: Excludes::defaults(),
            protected: Excludes::protected_defaults(),
            max_file_size: 1 << 20,
            notice: Some(DEV_PREVIEW_NOTICE.into()),
        },
        Arc::new(DevAutoAccept),
        Arc::new(|_| {}),
    );
    host.set_services(Arc::new(LspServices(Arc::new(Source(manager.clone())))));
    // What the application does with the servers' diagnostics.
    let mut events = manager.subscribe();
    let (forward_host, forward_manager, forward_root) =
        (host.clone(), manager.clone(), root.clone());
    tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            if let LspEvent::Diagnostics {
                server_id,
                uri,
                diagnostics,
                ..
            } = event
            {
                let Some(abs) = forge_lsp::uri_to_path(&uri) else {
                    continue;
                };
                let merged = forward_manager.merge_diagnostics(&server_id, &uri, diagnostics);
                let list: Vec<EditorDiagnostic> =
                    merged.iter().map(EditorDiagnostic::from).collect();
                let rel = abs
                    .strip_prefix(&forward_root)
                    .unwrap_or(&abs)
                    .to_string_lossy()
                    .into_owned();
                forward_host.publish_diagnostics(&rel, serde_json::to_vec(&list).unwrap());
            }
        }
    });
    let guest = GuestSession::join(
        Arc::new(net.endpoint([2; 32])),
        &host.link(),
        "Alex",
        Arc::new(|_: GuestEvent| {}),
    )
    .await
    .unwrap();
    let diagnostics: Diagnostics = Arc::default();
    let sink = diagnostics.clone();
    guest.set_diagnostics_sink(Some(Arc::new(move |path, bytes| {
        let list: Vec<EditorDiagnostic> = serde_json::from_slice(&bytes).unwrap();
        sink.lock().unwrap().push((path, list));
    })));
    Setup {
        _dir: dir,
        host,
        guest,
        diagnostics,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_guest_gets_diagnostics_hover_completion_and_definitions_from_the_hosts_server() {
    let s = setup().await;
    let path = "app/Models/User.php";
    // Opening the file: the host's server reads it and reports; the guest receives that.
    s.guest
        .lsp(LspQuery::Sync { path: path.into() })
        .await
        .unwrap();
    for _ in 0..300 {
        if !s.diagnostics.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    {
        let got = s.diagnostics.lock().unwrap();
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!(got[0].0, path);
        assert_eq!(got[0].1.len(), 1);
        assert_eq!(got[0].1[0].message, "Unfinished work");
        assert_eq!(got[0].1[0].range.start.line, 1);
    }

    let hover = s
        .guest
        .lsp(LspQuery::Hover {
            path: path.into(),
            line: 2,
            character: 7,
        })
        .await
        .unwrap();
    let hover: Value = serde_json::from_slice(&hover).unwrap();
    assert!(hover.to_string().contains("**User** at 2:7"), "{hover}");

    let completion = s
        .guest
        .lsp(LspQuery::Completion {
            path: path.into(),
            line: 2,
            character: 0,
        })
        .await
        .unwrap();
    let completion: Value = serde_json::from_slice(&completion).unwrap();
    let labels: Vec<&str> = completion["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap())
        .collect();
    assert_eq!(labels, ["firstName", "orders"]);

    // Go to Declaration: the project file stays, the stub outside the project is dropped.
    let definition = s
        .guest
        .lsp(LspQuery::Definition {
            path: path.into(),
            line: 2,
            character: 7,
        })
        .await
        .unwrap();
    let definition: Value = serde_json::from_slice(&definition).unwrap();
    let paths: Vec<&str> = definition
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, [path]);

    // Secret files never reach the servers, and a guest cannot ask for a file outside.
    assert!(s
        .guest
        .lsp(LspQuery::Hover {
            path: ".env".into(),
            line: 0,
            character: 0
        })
        .await
        .is_err());
    assert!(s
        .guest
        .lsp(LspQuery::Hover {
            path: "../x.php".into(),
            line: 0,
            character: 0
        })
        .await
        .is_err());
    drop(s.host);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_servers_see_the_shared_documents_unsaved_text() {
    let s = setup().await;
    let path = "app/Models/User.php";
    let doc = s
        .host
        .open_doc(
            path,
            Some("<?php\n// clean\n// TODO one\n// TODO two\n".into()),
        )
        .await
        .unwrap();
    assert_eq!(doc.path, path);
    s.guest
        .lsp(LspQuery::Sync { path: path.into() })
        .await
        .unwrap();
    for _ in 0..300 {
        if !s.diagnostics.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let got = s.diagnostics.lock().unwrap();
    let lines: Vec<u32> = got[0].1.iter().map(|d| d.range.start.line).collect();
    assert_eq!(lines, [2, 3], "the buffer, not the file on disk");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_guests_symbol_search_leaves_out_stubs_before_the_limit() {
    let s = setup().await;
    s.guest
        .lsp(LspQuery::Sync {
            path: "app/Models/User.php".into(),
        })
        .await
        .unwrap();
    let search = |include_non_project| {
        let guest = s.guest.clone();
        async move {
            let bytes = guest
                .lsp(LspQuery::WorkspaceSymbols {
                    query: "User".into(),
                    include_non_project,
                })
                .await
                .unwrap();
            serde_json::from_slice::<Vec<Value>>(&bytes).unwrap()
        }
    };
    // The project's class is found although 600 stubs come first.
    let mut project = Vec::new();
    for _ in 0..300 {
        project = search(false).await;
        if !project.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let names: Vec<&str> = project.iter().filter_map(|h| h["name"].as_str()).collect();
    assert_eq!(names, ["User"], "{project:?}");
    assert_eq!(project[0]["path"], "app/Models/User.php");
    // With non-project items `vendor/` comes too; files outside the project never do.
    let all = search(true).await;
    let names: Vec<&str> = all.iter().filter_map(|h| h["name"].as_str()).collect();
    assert_eq!(names, ["UserProvider", "User"], "{all:?}");
}
