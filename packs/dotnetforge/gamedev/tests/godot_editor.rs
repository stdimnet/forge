//! The Godot editor's servers as Forge reaches them, against fakes on loopback: the GDScript
//! language server (TCP, `tcp://` server command) and the debug adapter (TCP, the `godot`
//! adapter's `launch` with the project and scene). No Godot needed.

use std::sync::Arc;
use std::time::Duration;

use dotnetforge_gamedev::godot::dap::{launch_arguments, GodotAdapter, GODOT_ADAPTER};
use dotnetforge_gamedev::godot::lsp::server_config;
use forge_dap::{DapManager, LaunchMode, LaunchRequest};
use forge_debug::{DebugEvent, DebugServer, EventSink};
use forge_lsp::lsp_types::Position;
use forge_lsp::LspClient;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

async fn read_frame<R: tokio::io::AsyncBufRead + Unpin>(r: &mut R) -> Option<Value> {
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

async fn write_frame(w: &mut OwnedWriteHalf, v: Value) {
    let body = v.to_string();
    w.write_all(format!("Content-Length: {}\r\n\r\n{body}", body.len()).as_bytes())
        .await
        .unwrap();
}

/// What the Godot editor's GDScript server does for this test: initialize, a workspace
/// notification of its own, hover, shutdown.
async fn fake_gdscript_server(listener: TcpListener) -> Vec<String> {
    let (stream, _) = listener.accept().await.unwrap();
    let (r, mut w) = stream.into_split();
    let mut r = BufReader::new(r);
    let mut methods = Vec::new();
    while let Some(msg) = read_frame(&mut r).await {
        let method = msg["method"].as_str().unwrap_or_default().to_string();
        methods.push(method.clone());
        let id = msg.get("id").cloned();
        match method.as_str() {
            "initialize" => {
                write_frame(
                    &mut w,
                    json!({"jsonrpc": "2.0", "id": id, "result": {
                    "capabilities": {"textDocumentSync": 1, "hoverProvider": true}}}),
                )
                .await;
            }
            "initialized" => {
                write_frame(
                    &mut w,
                    json!({"jsonrpc": "2.0", "method": "gdscript_client/changeWorkspace",
                    "params": {"path": "/home/sam/dodge"}}),
                )
                .await;
            }
            "textDocument/hover" => {
                write_frame(
                    &mut w,
                    json!({"jsonrpc": "2.0", "id": id, "result": {
                    "contents": {"kind": "markdown", "value": "func jump() -> void"}}}),
                )
                .await;
            }
            "shutdown" => {
                write_frame(&mut w, json!({"jsonrpc": "2.0", "id": id, "result": null})).await
            }
            _ => {}
        }
    }
    methods
}

#[tokio::test]
async fn gdscript_language_server_over_tcp() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(fake_gdscript_server(listener));
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("player.gd");
    let client = LspClient::start(server_config("127.0.0.1", port), dir.path())
        .await
        .unwrap();
    client
        .did_open(
            &file,
            "extends CharacterBody2D\n\nfunc jump() -> void:\n\tpass\n",
        )
        .unwrap();
    let hover = client
        .hover(&file, Position::new(2, 6))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        forge_lsp::model::EditorHover::from(hover).contents,
        "func jump() -> void"
    );
    client.shutdown().await;
    let methods = tokio::time::timeout(Duration::from_secs(10), server)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        &methods[..3],
        ["initialize", "initialized", "textDocument/didOpen"]
    );
    assert!(methods.contains(&"textDocument/hover".to_string()));
    assert!(methods.contains(&"shutdown".to_string()));
}

/// The Godot editor's debug adapter: it checks the project, "runs" the scene, reports the
/// breakpoint it was given as hit, and ends when Forge disconnects.
async fn fake_godot_dap(listener: TcpListener, seen: mpsc::UnboundedSender<Value>) {
    let (stream, _) = listener.accept().await.unwrap();
    let (r, mut w) = stream.into_split();
    let mut r = BufReader::new(r);
    let mut seq = 1;
    let mut reply = |req: &Value, body: Value, success: bool| {
        seq += 1;
        json!({"seq": seq, "type": "response", "request_seq": req["seq"], "command": req["command"],
            "success": success, "body": body})
    };
    while let Some(req) = read_frame(&mut r).await {
        let _ = seen.send(req.clone());
        let command = req["command"].as_str().unwrap_or_default().to_string();
        match command.as_str() {
            "initialize" => {
                let resp = reply(
                    &req,
                    json!({"supportsConfigurationDoneRequest": true}),
                    true,
                );
                write_frame(&mut w, resp).await;
                write_frame(
                    &mut w,
                    json!({"seq": 100, "type": "event", "event": "initialized"}),
                )
                .await;
            }
            "launch" => {
                let ok = req["arguments"]["project"] == "/home/sam/dodge";
                let resp = reply(&req, json!({}), ok);
                write_frame(&mut w, resp).await;
            }
            "setBreakpoints" => {
                let n = req["arguments"]["breakpoints"]
                    .as_array()
                    .map_or(0, Vec::len);
                let bps: Vec<Value> = (0..n)
                    .map(|i| json!({"id": i + 1, "verified": true}))
                    .collect();
                let resp = reply(&req, json!({"breakpoints": bps}), true);
                write_frame(&mut w, resp).await;
            }
            "threads" => {
                let resp = reply(&req, json!({"threads": [{"id": 1, "name": "Main"}]}), true);
                write_frame(&mut w, resp).await;
            }
            "disconnect" => {
                let resp = reply(&req, json!({}), true);
                write_frame(&mut w, resp).await;
                break;
            }
            _ => {
                let resp = reply(&req, json!({}), true);
                write_frame(&mut w, resp).await;
            }
        }
    }
}

#[tokio::test]
async fn gdscript_debugging_through_the_editor_adapter() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, mut seen) = mpsc::unbounded_channel();
    let fake = tokio::spawn(fake_godot_dap(listener, tx));

    let (events_tx, _events) = mpsc::unbounded_channel::<DebugEvent>();
    let sink: EventSink = Arc::new(move |e: DebugEvent| {
        let _ = events_tx.send(e);
    });
    let root = tempfile::tempdir().unwrap();
    let server = Arc::new(DebugServer::new(Arc::clone(&sink)));
    server.open_project(Some(root.path()));
    let manager = DapManager::new(sink, server);
    manager.register_adapter(Arc::new(GodotAdapter));
    assert!(manager.adapters().iter().any(|a| a.r#type == GODOT_ADAPTER));

    let mut request = LaunchRequest::launch(GODOT_ADAPTER, "");
    request.program = None;
    request.name = Some("Dodge the Creeps".into());
    request.mode = LaunchMode::AttachServer {
        host: "127.0.0.1".into(),
        port,
    };
    request.configuration = launch_arguments("/home/sam/dodge", Some("res://main.tscn"), &[]);
    let info = manager.start(request).await.unwrap();
    assert!(info.id.starts_with("dap-"));

    let mut commands = Vec::new();
    let launch = loop {
        let msg = tokio::time::timeout(Duration::from_secs(10), seen.recv())
            .await
            .unwrap()
            .unwrap();
        commands.push(msg["command"].as_str().unwrap().to_string());
        if msg["command"] == "launch" {
            break msg;
        }
    };
    assert_eq!(commands[0], "initialize");
    assert_eq!(launch["arguments"]["type"], "godot");
    assert_eq!(launch["arguments"]["scene"], "res://main.tscn");
    assert_eq!(launch["arguments"]["name"], "Dodge the Creeps");
    manager.session(&info.id).unwrap().stop().await;
    tokio::time::timeout(Duration::from_secs(10), fake)
        .await
        .unwrap()
        .unwrap();
}
