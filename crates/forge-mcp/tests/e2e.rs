//! End to end: a real server on 127.0.0.1 with a fake IDE, an in-process client over
//! Streamable HTTP, the legacy HTTP+SSE transport, and the stdio proxy.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_ai_editor::AiIgnore;
use forge_mcp::engine::{
    Approval, ApprovalRequest, CallLogEntry, CallStatus, Engine, McpHost, ResourceInfo,
    ServerEvent, ToolOutput,
};
use forge_mcp::http::{self, Response};
use forge_mcp::jsonrpc::{notification, request};
use forge_mcp::protocol::ServerInfo;
use forge_mcp::server::{McpServer, ENDPOINT, SESSION_HEADER};
use forge_mcp::stdio::{run_proxy, Target};
use forge_mcp::{CallPolicy, ToolGroup, ToolSpec};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

const TOKEN: &str = "0123456789abcdef0123456789abcdef";

/// A fake IDE: a two-file project, approvals answered by the test through a channel.
struct FakeIde {
    policy: Mutex<CallPolicy>,
    approvals: Mutex<Option<mpsc::UnboundedSender<ApprovalRequest>>>,
    answer: Mutex<Approval>,
    edits: Mutex<Vec<Value>>,
    log: Mutex<Vec<CallLogEntry>>,
}

impl FakeIde {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            policy: Mutex::new(CallPolicy {
                ignore: AiIgnore::parse(".env\n"),
                ..CallPolicy::default()
            }),
            approvals: Mutex::new(None),
            answer: Mutex::new(Approval::Once),
            edits: Mutex::new(Vec::new()),
            log: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl McpHost for FakeIde {
    async fn policy(&self) -> CallPolicy {
        self.policy.lock().unwrap().clone()
    }

    async fn approve(&self, request: ApprovalRequest) -> Approval {
        if let Some(tx) = self.approvals.lock().unwrap().as_ref() {
            let _ = tx.send(request);
        }
        *self.answer.lock().unwrap()
    }

    async fn call_tool(&self, spec: &ToolSpec, args: Value) -> Result<ToolOutput, String> {
        match spec.name {
            "read_file" => match args["path"].as_str() {
                Some("src/App.php") => Ok(ToolOutput::Text("<?php\necho 'hi';\n".into())),
                Some(p) => Err(format!("{p}: file not found")),
                None => Err("path is missing".into()),
            },
            "find_files" => Ok(ToolOutput::Json(json!({
                "files": [{ "path": "src/App.php" }, { "path": ".env" }]
            }))),
            "replace_text_in_file" => {
                self.edits.lock().unwrap().push(args);
                Ok(ToolOutput::Text("Replaced 1 occurrence".into()))
            }
            "execute_terminal_command" => {
                Ok(ToolOutput::Json(json!({ "exitCode": 0, "output": "ok\n" })))
            }
            other => Ok(ToolOutput::Text(format!("ran {other}"))),
        }
    }

    async fn list_resources(&self) -> Vec<ResourceInfo> {
        vec![ResourceInfo {
            path: "src/App.php".into(),
            name: "App.php".into(),
            mime_type: Some("text/x-php".into()),
        }]
    }

    async fn read_resource(&self, path: &str) -> Result<String, String> {
        match path {
            "src/App.php" => Ok("<?php\necho 'hi';\n".into()),
            p => Err(format!("{p}: file not found")),
        }
    }

    fn on_event(&self, event: ServerEvent) {
        if let ServerEvent::Call(e) = event {
            self.log.lock().unwrap().push(e);
        }
    }
}

async fn start(ide: Arc<FakeIde>) -> McpServer {
    let engine = Arc::new(Engine::new(ide, ServerInfo::forge("0.1.0")));
    McpServer::start(0, TOKEN.into(), engine).await.unwrap()
}

/// A minimal Streamable HTTP client.
struct Client {
    port: u16,
    session: Option<String>,
    next_id: u64,
}

impl Client {
    fn new(port: u16) -> Self {
        Self {
            port,
            session: None,
            next_id: 1,
        }
    }

    async fn post(&mut self, body: &Value) -> Response {
        let auth = format!("Bearer {TOKEN}");
        let mut headers = vec![
            ("Authorization", auth.as_str()),
            ("Content-Type", "application/json"),
            ("Accept", "application/json, text/event-stream"),
        ];
        let session = self.session.clone();
        if let Some(s) = &session {
            headers.push((SESSION_HEADER, s.as_str()));
        }
        let resp = http::send(
            self.port,
            "POST",
            ENDPOINT,
            &headers,
            body.to_string().as_bytes(),
        )
        .await
        .unwrap();
        if let Some(s) = resp.header(SESSION_HEADER) {
            self.session = Some(s.to_string());
        }
        resp
    }

    async fn rpc(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let resp = self.post(&request(id, method, params)).await;
        assert_eq!(resp.status, 200, "{}", String::from_utf8_lossy(&resp.body));
        let v: Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(v["id"], id);
        v
    }

    async fn connect(&mut self) -> Value {
        let init = self
            .rpc(
                "initialize",
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "e2e-client", "version": "1.0" }
                }),
            )
            .await;
        let resp = self
            .post(&notification("notifications/initialized", json!({})))
            .await;
        assert_eq!(resp.status, 202);
        init
    }

    async fn call(&mut self, name: &str, args: Value) -> Value {
        self.rpc("tools/call", json!({ "name": name, "arguments": args }))
            .await["result"]
            .clone()
    }
}

#[tokio::test]
async fn http_lifecycle_tools_and_resources() {
    let ide = FakeIde::new();
    let server = start(ide.clone()).await;
    let mut c = Client::new(server.port);
    let init = c.connect().await;
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "forge");
    assert!(c.session.is_some());
    assert_eq!(server.engine().sessions()[0].client.name, "e2e-client");

    let tools = c.rpc("tools/list", json!({})).await;
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"read_file") && names.contains(&"git_diff"));
    assert!(
        !names.contains(&"execute_terminal_command"),
        "terminal is off by default"
    );

    let r = c.call("read_file", json!({ "path": "src/App.php" })).await;
    assert_eq!(r["isError"], false);
    assert_eq!(r["content"][0]["text"], "<?php\necho 'hi';\n");
    let r = c.call("read_file", json!({ "path": ".env" })).await;
    assert_eq!(r["isError"], true, ".aiignore'd files are refused");
    let r = c.call("find_files", json!({ "query": "a" })).await;
    assert_eq!(r["structuredContent"]["files"].as_array().unwrap().len(), 1);

    let res = c.rpc("resources/list", json!({})).await;
    assert_eq!(
        res["result"]["resources"][0]["uri"],
        "forge://project/src/App.php"
    );
    let res = c
        .rpc(
            "resources/read",
            json!({ "uri": "forge://project/src/App.php" }),
        )
        .await;
    assert_eq!(res["result"]["contents"][0]["text"], "<?php\necho 'hi';\n");

    // Ending the session: later requests get 404.
    let auth = format!("Bearer {TOKEN}");
    let sid = c.session.clone().unwrap();
    let del = http::send(
        server.port,
        "DELETE",
        ENDPOINT,
        &[
            ("Authorization", auth.as_str()),
            (SESSION_HEADER, sid.as_str()),
        ],
        b"",
    )
    .await
    .unwrap();
    assert_eq!(del.status, 200);
    let resp = c.post(&request(99, "ping", json!({}))).await;
    assert_eq!(resp.status, 404);
    assert_eq!(ide.log.lock().unwrap().len(), 3);
    server.stop();
}

#[tokio::test]
async fn http_rejects_bad_tokens_hosts_and_origins() {
    let server = start(FakeIde::new()).await;
    let port = server.port;
    let body = request(1, "initialize", json!({})).to_string();
    let send = |headers: Vec<(&'static str, String)>| {
        let body = body.clone();
        async move {
            let h: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
            http::send(port, "POST", ENDPOINT, &h, body.as_bytes())
                .await
                .unwrap()
                .status
        }
    };
    let ok_auth = format!("Bearer {TOKEN}");
    assert_eq!(send(vec![]).await, 401);
    assert_eq!(
        send(vec![("Authorization", "Bearer wrong".into())]).await,
        401
    );
    assert_eq!(
        send(vec![
            ("Authorization", ok_auth.clone()),
            ("Origin", "https://evil.example".into())
        ])
        .await,
        403
    );
    // A raw request with a foreign Host header (DNS rebinding).
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let raw = format!(
        "POST /mcp HTTP/1.1\r\nHost: evil.example:{port}\r\nAuthorization: Bearer {TOKEN}\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    s.write_all(raw.as_bytes()).await.unwrap();
    let mut answer = String::new();
    s.read_to_string(&mut answer).await.unwrap();
    assert!(answer.starts_with("HTTP/1.1 403"), "{answer}");
    // Requests without a session after initialize.
    assert_eq!(
        http::send(
            port,
            "POST",
            ENDPOINT,
            &[("Authorization", ok_auth.as_str())],
            request(2, "tools/list", json!({})).to_string().as_bytes()
        )
        .await
        .unwrap()
        .status,
        400
    );
    server.stop();
}

#[tokio::test]
async fn approvals_over_http() {
    let ide = FakeIde::new();
    let (tx, mut rx) = mpsc::unbounded_channel();
    *ide.approvals.lock().unwrap() = Some(tx);
    let server = start(ide.clone()).await;
    let mut c = Client::new(server.port);
    c.connect().await;
    let edit = json!({ "path": "src/App.php", "oldText": "hi", "newText": "hello" });

    // Denied: nothing is changed.
    *ide.answer.lock().unwrap() = Approval::Deny;
    let r = c.call("replace_text_in_file", edit.clone()).await;
    assert_eq!(r["isError"], true);
    assert!(ide.edits.lock().unwrap().is_empty());
    let card = rx.recv().await.unwrap();
    assert_eq!(card.tool, "replace_text_in_file");
    assert_eq!(card.summary, "edit src/App.php");
    assert_eq!(card.client, "e2e-client");

    // Allowed once.
    *ide.answer.lock().unwrap() = Approval::Once;
    let r = c.call("replace_text_in_file", edit.clone()).await;
    assert_eq!(r["isError"], false);
    assert_eq!(ide.edits.lock().unwrap().len(), 1);
    rx.recv().await.unwrap();

    // Terminal commands: off until the group is enabled, then asked on every call even
    // after "allow for this session".
    let r = c
        .call("execute_terminal_command", json!({ "command": "ls" }))
        .await;
    assert!(r["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("turned off"));
    ide.policy
        .lock()
        .unwrap()
        .enabled_groups
        .insert(ToolGroup::Terminal);
    *ide.answer.lock().unwrap() = Approval::Session;
    for _ in 0..2 {
        let r = c
            .call("execute_terminal_command", json!({ "command": "ls" }))
            .await;
        assert_eq!(r["isError"], false);
        let card = rx.recv().await.unwrap();
        assert!(!card.session_allowed);
    }

    // Safe Mode: runs are refused before anyone is asked.
    ide.policy.lock().unwrap().safe_mode = true;
    let r = c
        .call("run_configuration", json!({ "name": "Tests" }))
        .await;
    assert!(r["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Safe Mode"));
    assert!(rx.try_recv().is_err());

    // The AI switch refuses everything.
    ide.policy.lock().unwrap().ai_enabled = false;
    let r = c.call("read_file", json!({ "path": "src/App.php" })).await;
    assert_eq!(r["isError"], true);

    let log = ide.log.lock().unwrap();
    let statuses: Vec<CallStatus> = log.iter().map(|e| e.status).collect();
    assert_eq!(
        statuses,
        vec![
            CallStatus::Denied,
            CallStatus::Ok,
            CallStatus::Refused,
            CallStatus::Ok,
            CallStatus::Ok,
            CallStatus::Refused,
            CallStatus::Refused,
        ]
    );
    server.stop();
}

async fn next_data<R: tokio::io::AsyncBufRead + Unpin>(events: &mut R) -> String {
    loop {
        let mut line = String::new();
        events.read_line(&mut line).await.unwrap();
        if let Some(d) = line.strip_prefix("data: ") {
            return d.trim().to_string();
        }
    }
}

async fn send_line<W: tokio::io::AsyncWrite + Unpin>(w: &mut W, v: Value) {
    w.write_all(format!("{v}\n").as_bytes()).await.unwrap();
}

#[tokio::test]
async fn legacy_sse_transport() {
    let server = start(FakeIde::new()).await;
    let port = server.port;
    let mut sse = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    sse.write_all(
        format!("GET /sse HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {TOKEN}\r\nAccept: text/event-stream\r\n\r\n").as_bytes(),
    )
    .await
    .unwrap();
    let mut events = BufReader::new(sse);
    let endpoint = tokio::time::timeout(Duration::from_secs(5), next_data(&mut events))
        .await
        .unwrap();
    assert!(endpoint.starts_with("/messages?sessionId="));
    let auth = format!("Bearer {TOKEN}");
    let post = |body: Value| {
        let endpoint = endpoint.clone();
        let auth = auth.clone();
        async move {
            http::send(
                port,
                "POST",
                &endpoint,
                &[
                    ("Authorization", auth.as_str()),
                    ("Content-Type", "application/json"),
                ],
                body.to_string().as_bytes(),
            )
            .await
            .unwrap()
            .status
        }
    };
    assert_eq!(
        post(request(
            1,
            "initialize",
            json!({ "protocolVersion": "2024-11-05" })
        ))
        .await,
        202
    );
    let init: Value = serde_json::from_str(&next_data(&mut events).await).unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(post(request(2, "tools/list", json!({}))).await, 202);
    let list: Value = serde_json::from_str(&next_data(&mut events).await).unwrap();
    assert!(list["result"]["tools"].as_array().unwrap().len() > 10);
    server.stop();
}

#[tokio::test]
async fn stdio_proxy_forwards_to_the_running_server() {
    let ide = FakeIde::new();
    let server = start(ide.clone()).await;
    let port = server.port;
    let (mut client_in, proxy_in) = tokio::io::duplex(64 * 1024);
    let (proxy_out, client_out) = tokio::io::duplex(64 * 1024);
    let proxy = tokio::spawn(async move {
        run_proxy(BufReader::new(proxy_in), proxy_out, move || {
            Ok(Target {
                port,
                token: TOKEN.into(),
            })
        })
        .await
    });
    let mut lines = BufReader::new(client_out).lines();
    send_line(&mut client_in, request(
        1,
        "initialize",
        json!({ "protocolVersion": "2025-03-26", "clientInfo": { "name": "Claude Desktop", "version": "1" } }),
    ))
    .await;
    let init: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
    send_line(
        &mut client_in,
        notification("notifications/initialized", json!({})),
    )
    .await;
    send_line(
        &mut client_in,
        request(
            2,
            "tools/call",
            json!({ "name": "read_file", "arguments": { "path": "src/App.php" } }),
        ),
    )
    .await;
    let r: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(r["id"], 2);
    assert_eq!(r["result"]["content"][0]["text"], "<?php\necho 'hi';\n");
    assert_eq!(server.engine().sessions()[0].client.name, "Claude Desktop");
    // Closing stdin ends the proxy and the session.
    drop(client_in);
    proxy.await.unwrap().unwrap();
    assert!(server.engine().sessions().is_empty());
    server.stop();
}

#[tokio::test]
async fn stdio_proxy_reports_a_stopped_server() {
    let server = start(FakeIde::new()).await;
    let port = server.port;
    server.stop();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let input = format!("{}\n", request(1, "initialize", json!({})));
    let mut out = Vec::new();
    run_proxy(BufReader::new(input.as_bytes()), &mut out, move || {
        Ok(Target {
            port,
            token: TOKEN.into(),
        })
    })
    .await
    .unwrap();
    let v: Value = serde_json::from_slice(out.trim_ascii_end()).unwrap();
    assert_eq!(v["id"], 1);
    assert!(v["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Cannot reach Forge"));
}
