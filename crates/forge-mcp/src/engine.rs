//! The transport-independent server: sessions, the MCP methods, and the path of every tool
//! call through the policy, the user's approval, the IDE ([`McpHost`]), the `.aiignore`
//! filter and the call log.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::jsonrpc::{self, Incoming, Message, RpcError};
use crate::policy::{CallPolicy, Decision};
use crate::protocol::{self, ServerInfo};
use crate::tools::{self, ToolGroup, ToolKind, ToolSpec};

/// Longest text returned by one call; longer output is cut.
pub const MAX_RESULT_CHARS: usize = 100_000;
/// Longest argument text kept in the call log.
const MAX_LOGGED_ARGS: usize = 2_000;

/// What a tool returned.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolOutput {
    Text(String),
    Json(Value),
}

/// The user's answer to an approval card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Approval {
    Once,
    /// Allow this tool for the rest of the client's session (not for terminal commands).
    Session,
    Deny,
}

/// What the approval card shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub id: u64,
    pub session: String,
    pub client: String,
    pub tool: String,
    pub title: String,
    pub group: ToolGroup,
    pub kind: ToolKind,
    /// "edit app/Models/User.php", "run a terminal command".
    pub summary: String,
    pub arguments: Value,
    /// Whether "Allow for this session" is offered.
    pub session_allowed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub client: ClientInfo,
    pub protocol_version: String,
    /// `http` or `sse`.
    pub transport: String,
    /// Unix milliseconds.
    pub connected_at: u64,
    pub last_seen_at: u64,
    pub calls: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CallStatus {
    Ok,
    /// The tool failed (bad arguments, file not found, …).
    Error,
    /// Refused by the settings, Safe Mode or `.aiignore`.
    Refused,
    /// The user denied the approval.
    Denied,
}

/// One line of the call log.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallLogEntry {
    pub id: u64,
    pub session: String,
    pub client: String,
    pub tool: String,
    /// The arguments as JSON (cut when long).
    pub arguments: String,
    pub status: CallStatus,
    pub message: Option<String>,
    /// Unix milliseconds.
    pub started_at: u64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerEvent {
    SessionsChanged,
    Call(CallLogEntry),
}

/// A listed resource (an open editor, for example).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceInfo {
    /// Project-relative path.
    pub path: String,
    pub name: String,
    pub mime_type: Option<String>,
}

/// The IDE side: runs tools, asks the user and reports what happened.
#[async_trait]
pub trait McpHost: Send + Sync + 'static {
    /// The current settings and project state.
    async fn policy(&self) -> CallPolicy;
    /// Show an approval card and wait for the answer (deny on timeout).
    async fn approve(&self, request: ApprovalRequest) -> Approval;
    /// Run an allowed call. `Err` is a tool error shown to the client.
    async fn call_tool(&self, spec: &ToolSpec, args: Value) -> Result<ToolOutput, String>;
    /// Resources to list (the open editors).
    async fn list_resources(&self) -> Vec<ResourceInfo> {
        Vec::new()
    }
    /// The text of a project file.
    async fn read_resource(&self, path: &str) -> Result<String, String>;
    fn on_event(&self, _event: ServerEvent) {}
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Random hex (session ids, tokens).
pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::getrandom(&mut buf).expect("the OS random number generator is unavailable");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

struct Session {
    info: SessionInfo,
    initialized: bool,
    /// Tools the user allowed for this session.
    allowed: HashSet<String>,
}

/// Responses to send back for one incoming body.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    /// One per request, in order (empty for notifications only).
    pub responses: Vec<Value>,
    /// The session `initialize` created.
    pub created_session: Option<String>,
    /// The body was a batch (answer with an array).
    pub batch: bool,
}

impl Outcome {
    /// The response body, `None` when there is nothing to answer (202 Accepted).
    pub fn body(&self) -> Option<Value> {
        match (self.batch, self.responses.as_slice()) {
            (_, []) => None,
            (false, [one]) => Some(one.clone()),
            _ => Some(Value::Array(self.responses.clone())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// A non-initialize message without a session (HTTP 400).
    MissingSession,
    /// Unknown or ended session (HTTP 404).
    UnknownSession,
}

pub struct Engine {
    host: Arc<dyn McpHost>,
    info: ServerInfo,
    sessions: Mutex<HashMap<String, Session>>,
    next_call: AtomicU64,
}

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max).collect();
    let rest = text.chars().count() - max;
    format!("{kept}\n… (truncated, {rest} more characters)")
}

fn str_arg<'a>(args: &'a Value, name: &str) -> &'a str {
    args.get(name).and_then(Value::as_str).unwrap_or("")
}

/// The approval card's one-line summary.
pub fn approval_summary(spec: &ToolSpec, args: &Value) -> String {
    match spec.name {
        "replace_text_in_file" => format!("edit {}", str_arg(args, "path")),
        "write_file" => format!("write {}", str_arg(args, "path")),
        "reformat_file" => format!("reformat {}", str_arg(args, "path")),
        "run_configuration" => format!("run '{}'", str_arg(args, "name")),
        // The card shows the command and the query in full.
        "execute_terminal_command" => "run a terminal command".into(),
        "run_sql_query" => format!("query {}", str_arg(args, "dataSource")),
        _ => spec.title.to_lowercase(),
    }
}

fn tool_result(text: String, structured: Option<Value>, is_error: bool) -> Value {
    let mut r = json!({ "content": [{ "type": "text", "text": text }], "isError": is_error });
    if let Some(s) = structured.filter(Value::is_object) {
        r["structuredContent"] = s;
    }
    r
}

impl Engine {
    pub fn new(host: Arc<dyn McpHost>, info: ServerInfo) -> Self {
        Self {
            host,
            info,
            sessions: Mutex::new(HashMap::new()),
            next_call: AtomicU64::new(1),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Session>> {
        self.sessions.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn sessions(&self) -> Vec<SessionInfo> {
        let mut list: Vec<SessionInfo> = self
            .lock()
            .values()
            .filter(|s| s.initialized)
            .map(|s| s.info.clone())
            .collect();
        list.sort_by_key(|s| s.connected_at);
        list
    }

    pub fn has_session(&self, id: &str) -> bool {
        self.lock().contains_key(id)
    }

    pub fn end_session(&self, id: &str) -> bool {
        let removed = self.lock().remove(id).is_some();
        if removed {
            self.host.on_event(ServerEvent::SessionsChanged);
        }
        removed
    }

    /// End every session (the server stops).
    pub fn end_all(&self) {
        self.lock().clear();
        self.host.on_event(ServerEvent::SessionsChanged);
    }

    /// Handle one body. `session` is the client's session id (none before `initialize`);
    /// `fixed_id` names the session `initialize` creates (the SSE transport's id).
    pub async fn handle(
        &self,
        session: Option<&str>,
        fixed_id: Option<&str>,
        transport: &str,
        incoming: Incoming,
    ) -> Result<Outcome, EngineError> {
        let batch = matches!(incoming, Incoming::Batch(_));
        let messages = match incoming {
            Incoming::Single(m) => vec![m],
            Incoming::Batch(b) => b,
        };
        let initializes = messages
            .iter()
            .any(|m| m.method() == Some("initialize") && m.is_request());
        let mut session_id = session.map(str::to_string);
        if !initializes {
            match &session_id {
                None => return Err(EngineError::MissingSession),
                Some(id) if !self.has_session(id) => return Err(EngineError::UnknownSession),
                Some(id) => {
                    if let Some(s) = self.lock().get_mut(id) {
                        s.info.last_seen_at = now_ms();
                    }
                }
            }
        }
        let mut outcome = Outcome {
            batch,
            ..Outcome::default()
        };
        for message in messages {
            match message {
                Message::Request { id, method, params } if method == "initialize" => {
                    let new_id = fixed_id
                        .map(str::to_string)
                        .unwrap_or_else(|| random_hex(16));
                    let response = self.initialize(&new_id, transport, &params);
                    outcome
                        .responses
                        .push(jsonrpc::result_response(id, response));
                    outcome.created_session = Some(new_id.clone());
                    session_id = Some(new_id);
                }
                Message::Request { id, method, params } => {
                    let sid = session_id.clone().unwrap_or_default();
                    let response = match self.request(&sid, &method, params).await {
                        Ok(result) => jsonrpc::result_response(id, result),
                        Err(e) => jsonrpc::error_response(id, &e),
                    };
                    outcome.responses.push(response);
                }
                Message::Notification { method, .. } => {
                    if method == "notifications/initialized" {
                        if let Some(sid) = &session_id {
                            if let Some(s) = self.lock().get_mut(sid) {
                                s.initialized = true;
                            }
                            self.host.on_event(ServerEvent::SessionsChanged);
                        }
                    }
                }
                Message::Response { .. } => {}
            }
        }
        Ok(outcome)
    }

    fn initialize(&self, id: &str, transport: &str, params: &Value) -> Value {
        let version = protocol::negotiate_version(params["protocolVersion"].as_str());
        let client = ClientInfo {
            name: params["clientInfo"]["title"]
                .as_str()
                .or(params["clientInfo"]["name"].as_str())
                .unwrap_or("MCP client")
                .to_string(),
            version: params["clientInfo"]["version"]
                .as_str()
                .unwrap_or("")
                .to_string(),
        };
        let now = now_ms();
        self.lock().insert(
            id.to_string(),
            Session {
                info: SessionInfo {
                    id: id.to_string(),
                    client,
                    protocol_version: version.to_string(),
                    transport: transport.to_string(),
                    connected_at: now,
                    last_seen_at: now,
                    calls: 0,
                },
                // Shown as connected once the client confirms with `initialized`.
                initialized: false,
                allowed: HashSet::new(),
            },
        );
        protocol::initialize_result(version, &self.info)
    }

    async fn request(&self, session: &str, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "ping" => Ok(json!({})),
            "tools/list" => {
                let policy = self.host.policy().await;
                let tools: Vec<Value> = tools::catalog()
                    .iter()
                    .filter(|t| policy.lists(t))
                    .map(ToolSpec::to_mcp)
                    .collect();
                Ok(json!({ "tools": tools }))
            }
            "tools/call" => {
                let name = params["name"]
                    .as_str()
                    .ok_or_else(|| RpcError::invalid_params("name is missing"))?;
                let spec = tools::find(name)
                    .ok_or_else(|| RpcError::invalid_params(format!("Unknown tool: {name}")))?;
                let args = match params.get("arguments") {
                    None | Some(Value::Null) => json!({}),
                    Some(a) => a.clone(),
                };
                Ok(self.call(session, &spec, args).await)
            }
            "resources/list" => {
                let policy = self.host.policy().await;
                if !policy.ai_enabled
                    || !policy.lists(&tools::find("read_file").expect("read_file"))
                {
                    return Ok(json!({ "resources": [] }));
                }
                let resources: Vec<Value> = self
                    .host
                    .list_resources()
                    .await
                    .into_iter()
                    .filter(|r| !policy.is_ignored(&r.path))
                    .map(|r| {
                        json!({
                            "uri": protocol::resource_uri(&r.path),
                            "name": r.name,
                            "title": r.path,
                            "mimeType": r.mime_type.unwrap_or_else(|| "text/plain".into()),
                        })
                    })
                    .collect();
                Ok(json!({ "resources": resources }))
            }
            "resources/templates/list" => Ok(protocol::resource_templates()),
            "resources/read" => {
                let uri = params["uri"]
                    .as_str()
                    .ok_or_else(|| RpcError::invalid_params("uri is missing"))?;
                let path = protocol::resource_path(uri).ok_or_else(|| {
                    RpcError::new(
                        jsonrpc::RESOURCE_NOT_FOUND,
                        format!("Resource not found: {uri}"),
                    )
                })?;
                let spec = tools::find("read_file").expect("read_file");
                let policy = self.host.policy().await;
                if let Decision::Deny { reason } = policy.decide(&spec, &json!({ "path": path })) {
                    return Err(RpcError::new(jsonrpc::INVALID_PARAMS, reason));
                }
                let text = self
                    .host
                    .read_resource(path)
                    .await
                    .map_err(|e| RpcError::new(jsonrpc::RESOURCE_NOT_FOUND, e))?;
                Ok(json!({
                    "contents": [{ "uri": uri, "mimeType": "text/plain", "text": cut(&text, MAX_RESULT_CHARS) }]
                }))
            }
            "prompts/list" => Ok(protocol::prompts_list()),
            "prompts/get" => {
                let name = params["name"].as_str().unwrap_or("");
                protocol::prompt_get(name, &params["arguments"])
                    .ok_or_else(|| RpcError::invalid_params(format!("Unknown prompt: {name}")))
            }
            "logging/setLevel" => Ok(json!({})),
            "completion/complete" => Ok(json!({ "completion": { "values": [] } })),
            other => Err(RpcError::method_not_found(other)),
        }
    }

    fn client_of(&self, session: &str) -> String {
        self.lock()
            .get(session)
            .map(|s| s.info.client.name.clone())
            .unwrap_or_else(|| "MCP client".into())
    }

    /// One `tools/call`: always answers with a tool result (`isError` for failures).
    pub async fn call(&self, session: &str, spec: &ToolSpec, args: Value) -> Value {
        let started_at = now_ms();
        let clock = Instant::now();
        let call_id = self.next_call.fetch_add(1, Ordering::Relaxed);
        let client = self.client_of(session);
        if let Some(s) = self.lock().get_mut(session) {
            s.info.calls += 1;
        }
        let (status, message, result) = self.run_call(call_id, session, &client, spec, &args).await;
        self.host.on_event(ServerEvent::Call(CallLogEntry {
            id: call_id,
            session: session.to_string(),
            client,
            tool: spec.name.to_string(),
            arguments: cut(&args.to_string(), MAX_LOGGED_ARGS),
            status,
            message,
            started_at,
            duration_ms: clock.elapsed().as_millis() as u64,
        }));
        result
    }

    async fn run_call(
        &self,
        call_id: u64,
        session: &str,
        client: &str,
        spec: &ToolSpec,
        args: &Value,
    ) -> (CallStatus, Option<String>, Value) {
        let fail = |status, msg: String| (status, Some(msg.clone()), tool_result(msg, None, true));
        if let Err(e) = spec.check_arguments(args) {
            return fail(CallStatus::Error, format!("Invalid arguments: {e}"));
        }
        let policy = self.host.policy().await;
        match policy.decide(spec, args) {
            Decision::Deny { reason } => return fail(CallStatus::Refused, reason),
            Decision::Ask => {
                let pre_allowed = !spec.always_asks()
                    && self
                        .lock()
                        .get(session)
                        .is_some_and(|s| s.allowed.contains(spec.name));
                if !pre_allowed {
                    let answer = self
                        .host
                        .approve(ApprovalRequest {
                            id: call_id,
                            session: session.to_string(),
                            client: client.to_string(),
                            tool: spec.name.to_string(),
                            title: spec.title.to_string(),
                            group: spec.group,
                            kind: spec.kind,
                            summary: approval_summary(spec, args),
                            arguments: args.clone(),
                            session_allowed: !spec.always_asks(),
                        })
                        .await;
                    match answer {
                        Approval::Deny => {
                            return fail(
                                CallStatus::Denied,
                                format!("The user denied {} in Forge.", spec.name),
                            )
                        }
                        Approval::Session if !spec.always_asks() => {
                            if let Some(s) = self.lock().get_mut(session) {
                                s.allowed.insert(spec.name.to_string());
                            }
                        }
                        _ => {}
                    }
                }
            }
            Decision::Allow => {}
        }
        match self.host.call_tool(spec, args.clone()).await {
            Ok(ToolOutput::Text(text)) => (
                CallStatus::Ok,
                None,
                tool_result(cut(&text, MAX_RESULT_CHARS), None, false),
            ),
            Ok(ToolOutput::Json(mut value)) => {
                let removed = policy.filter_result(&mut value);
                if removed > 0 && value.is_object() {
                    value["excludedByAiIgnore"] = json!(removed);
                }
                let text = serde_json::to_string_pretty(&value).unwrap_or_default();
                if text.chars().count() > MAX_RESULT_CHARS {
                    (
                        CallStatus::Ok,
                        None,
                        tool_result(cut(&text, MAX_RESULT_CHARS), None, false),
                    )
                } else {
                    (CallStatus::Ok, None, tool_result(text, Some(value), false))
                }
            }
            Err(e) => fail(CallStatus::Error, e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonrpc::{parse, request};
    use forge_ai_editor::AiIgnore;

    #[derive(Default)]
    struct Fake {
        approvals: Mutex<Vec<ApprovalRequest>>,
        answer: Mutex<Option<Approval>>,
        calls: Mutex<Vec<String>>,
        log: Mutex<Vec<CallLogEntry>>,
        safe_mode: bool,
    }

    #[async_trait]
    impl McpHost for Fake {
        async fn policy(&self) -> CallPolicy {
            CallPolicy {
                safe_mode: self.safe_mode,
                ignore: AiIgnore::parse(".env\n"),
                ..CallPolicy::default()
            }
        }
        async fn approve(&self, request: ApprovalRequest) -> Approval {
            self.approvals.lock().unwrap().push(request);
            self.answer.lock().unwrap().unwrap_or(Approval::Deny)
        }
        async fn call_tool(&self, spec: &ToolSpec, args: Value) -> Result<ToolOutput, String> {
            self.calls.lock().unwrap().push(spec.name.to_string());
            match spec.name {
                "find_files" => Ok(ToolOutput::Json(json!({
                    "files": [{ "path": "src/a.php" }, { "path": ".env" }]
                }))),
                "read_file" if args["path"] == "missing" => Err("missing: not found".into()),
                _ => Ok(ToolOutput::Text(format!("ran {}", spec.name))),
            }
        }
        async fn read_resource(&self, path: &str) -> Result<String, String> {
            Ok(format!("text of {path}"))
        }
        fn on_event(&self, event: ServerEvent) {
            if let ServerEvent::Call(e) = event {
                self.log.lock().unwrap().push(e);
            }
        }
    }

    async fn send(engine: &Engine, session: Option<&str>, msg: Value) -> Outcome {
        engine
            .handle(
                session,
                None,
                "http",
                parse(msg.to_string().as_bytes()).unwrap(),
            )
            .await
            .unwrap()
    }

    async fn connect(engine: &Engine) -> String {
        let out = send(
            engine,
            None,
            request(
                1,
                "initialize",
                json!({
                    "protocolVersion": "2025-03-26",
                    "clientInfo": { "name": "claude-code", "version": "2.0.0" },
                    "capabilities": {}
                }),
            ),
        )
        .await;
        assert_eq!(out.responses[0]["result"]["protocolVersion"], "2025-03-26");
        let sid = out.created_session.unwrap();
        send(
            engine,
            Some(&sid),
            jsonrpc::notification("notifications/initialized", json!({})),
        )
        .await;
        sid
    }

    async fn call(engine: &Engine, sid: &str, name: &str, args: Value) -> Value {
        let out = send(
            engine,
            Some(sid),
            request(9, "tools/call", json!({ "name": name, "arguments": args })),
        )
        .await;
        out.responses[0]["result"].clone()
    }

    fn setup(fake: Fake) -> (Engine, Arc<Fake>) {
        let fake = Arc::new(fake);
        (Engine::new(fake.clone(), ServerInfo::forge("0.1.0")), fake)
    }

    #[tokio::test]
    async fn sessions_are_required_after_initialize() {
        let (engine, _) = setup(Fake::default());
        let ping = parse(request(1, "ping", json!({})).to_string().as_bytes()).unwrap();
        assert_eq!(
            engine.handle(None, None, "http", ping.clone()).await,
            Err(EngineError::MissingSession)
        );
        assert_eq!(
            engine.handle(Some("nope"), None, "http", ping).await,
            Err(EngineError::UnknownSession)
        );
        let sid = connect(&engine).await;
        let sessions = engine.sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].client.name, "claude-code");
        assert!(engine.end_session(&sid));
        assert!(engine.sessions().is_empty());
    }

    #[tokio::test]
    async fn lists_tools_of_enabled_groups() {
        let (engine, _) = setup(Fake::default());
        let sid = connect(&engine).await;
        let out = send(&engine, Some(&sid), request(2, "tools/list", json!({}))).await;
        let names: Vec<&str> = out.responses[0]["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"replace_text_in_file"));
        // Terminal commands are off by default.
        assert!(!names.contains(&"execute_terminal_command"));
        let unknown = send(&engine, Some(&sid), request(3, "nope/nope", json!({}))).await;
        assert_eq!(
            unknown.responses[0]["error"]["code"],
            jsonrpc::METHOD_NOT_FOUND
        );
    }

    #[tokio::test]
    async fn read_calls_run_and_are_logged() {
        let (engine, fake) = setup(Fake::default());
        let sid = connect(&engine).await;
        let r = call(&engine, &sid, "read_file", json!({ "path": "src/a.php" })).await;
        assert_eq!(r["isError"], false);
        assert_eq!(r["content"][0]["text"], "ran read_file");
        let r = call(&engine, &sid, "read_file", json!({ "path": "missing" })).await;
        assert_eq!(r["isError"], true);
        assert_eq!(r["content"][0]["text"], "missing: not found");
        let r = call(&engine, &sid, "read_file", json!({})).await;
        assert_eq!(r["isError"], true);
        let log = fake.log.lock().unwrap();
        assert_eq!(log.len(), 3);
        assert_eq!(log[0].status, CallStatus::Ok);
        assert_eq!(log[0].client, "claude-code");
        assert_eq!(log[1].status, CallStatus::Error);
        assert!(fake.approvals.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn results_are_filtered_by_aiignore() {
        let (engine, _) = setup(Fake::default());
        let sid = connect(&engine).await;
        let r = call(&engine, &sid, "find_files", json!({ "query": "a" })).await;
        assert_eq!(
            r["structuredContent"]["files"],
            json!([{ "path": "src/a.php" }])
        );
        assert_eq!(r["structuredContent"]["excludedByAiIgnore"], 1);
        let r = call(&engine, &sid, "read_file", json!({ "path": ".env" })).await;
        assert_eq!(r["isError"], true);
        assert!(r["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains(".aiignore"));
    }

    #[tokio::test]
    async fn write_calls_wait_for_approval() {
        let (engine, fake) = setup(Fake::default());
        let sid = connect(&engine).await;
        let args = json!({ "path": "a.php", "oldText": "a", "newText": "b" });
        // Denied: the tool does not run.
        let r = call(&engine, &sid, "replace_text_in_file", args.clone()).await;
        assert_eq!(r["isError"], true);
        assert!(fake.calls.lock().unwrap().is_empty());
        {
            let approvals = fake.approvals.lock().unwrap();
            assert_eq!(approvals[0].summary, "edit a.php");
            assert_eq!(approvals[0].client, "claude-code");
            assert!(approvals[0].session_allowed);
        }
        // Allowed for the session: asked once, then remembered.
        *fake.answer.lock().unwrap() = Some(Approval::Session);
        call(&engine, &sid, "replace_text_in_file", args.clone()).await;
        let r = call(&engine, &sid, "replace_text_in_file", args).await;
        assert_eq!(r["isError"], false);
        assert_eq!(fake.approvals.lock().unwrap().len(), 2);
        assert_eq!(fake.calls.lock().unwrap().len(), 2);
        let log = fake.log.lock().unwrap();
        assert_eq!(log[0].status, CallStatus::Denied);
    }

    #[tokio::test]
    async fn safe_mode_refuses_runs_before_asking() {
        let (engine, fake) = setup(Fake {
            safe_mode: true,
            answer: Mutex::new(Some(Approval::Once)),
            ..Fake::default()
        });
        let sid = connect(&engine).await;
        let r = call(
            &engine,
            &sid,
            "run_configuration",
            json!({ "name": "Tests" }),
        )
        .await;
        assert_eq!(r["isError"], true);
        assert!(r["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Safe Mode"));
        assert!(fake.approvals.lock().unwrap().is_empty());
        assert_eq!(fake.log.lock().unwrap()[0].status, CallStatus::Refused);
    }

    #[tokio::test]
    async fn resources_and_prompts() {
        let (engine, _) = setup(Fake::default());
        let sid = connect(&engine).await;
        let out = send(
            &engine,
            Some(&sid),
            request(
                4,
                "resources/read",
                json!({ "uri": "forge://project/src/a.php" }),
            ),
        )
        .await;
        assert_eq!(
            out.responses[0]["result"]["contents"][0]["text"],
            "text of src/a.php"
        );
        let out = send(
            &engine,
            Some(&sid),
            request(
                5,
                "resources/read",
                json!({ "uri": "forge://project/.env" }),
            ),
        )
        .await;
        assert!(out.responses[0]["error"].is_object());
        let out = send(
            &engine,
            Some(&sid),
            request(6, "prompts/get", json!({ "name": "fix_problems" })),
        )
        .await;
        assert!(out.responses[0]["result"]["messages"].is_array());
    }

    #[tokio::test]
    async fn batches_answer_with_arrays() {
        let (engine, _) = setup(Fake::default());
        let sid = connect(&engine).await;
        let body = json!([
            request(1, "ping", json!({})),
            jsonrpc::notification("x", json!({})),
            request(2, "ping", json!({}))
        ]);
        let out = send(&engine, Some(&sid), body).await;
        let body = out.body().unwrap();
        assert_eq!(body.as_array().unwrap().len(), 2);
        let only_notes = send(&engine, Some(&sid), jsonrpc::notification("x", json!({}))).await;
        assert_eq!(only_notes.body(), None);
    }

    #[test]
    fn cuts_long_text() {
        assert_eq!(cut("abc", 5), "abc");
        assert_eq!(cut("abcdef", 3), "abc\n… (truncated, 3 more characters)");
    }
}
