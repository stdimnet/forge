//! An LSP client talking to one language server process over stdio.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use forge_core::{ForgeError, Result};
use lsp_types::notification::{self as notif, Notification};
use lsp_types::request::{self as req, Request};
use lsp_types::{
    CallHierarchyItem, ClientCapabilities, CodeAction, CodeActionContext, CodeActionOrCommand,
    CodeActionParams, CodeActionProviderCapability, CompletionItem, CompletionParams,
    CompletionResponse, Diagnostic, DidChangeConfigurationParams, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    DocumentFormattingParams, DocumentHighlight, DocumentHighlightParams, DocumentSymbolParams,
    DocumentSymbolResponse, ExecuteCommandParams, FormattingOptions, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverParams, ImplementationProviderCapability, InitializeParams,
    InitializeResult, InitializedParams, Location, OneOf, Position, PrepareRenameResponse, Range,
    ReferenceContext, ReferenceParams, RenameParams, ServerCapabilities, SignatureHelp,
    SignatureHelpParams, TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, TextDocumentSyncSaveOptions, TextEdit,
    TypeDefinitionProviderCapability, TypeHierarchyItem, Url, VersionedTextDocumentIdentifier,
    WorkspaceEdit, WorkspaceFolder, WorkspaceSymbolParams, WorkspaceSymbolResponse,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::config::LspServerConfig;
use crate::framing;
use crate::launch::{LocalLauncher, ServerLauncher, ServerProcess};
use crate::log::{explain, LogSource, ServerLog};

/// Default time to wait for a response before giving up.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Lifecycle state of a language server, shown in the status bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ServerState {
    /// Registered, but no matching file has been opened yet.
    NotStarted,
    Starting,
    Ready,
    /// Work-done progress is running (e.g. "Indexing").
    Busy {
        message: String,
        percentage: Option<u32>,
    },
    Stopped,
    Failed {
        error: String,
    },
}

/// Something the server pushed to us.
#[derive(Debug, Clone)]
pub enum LspEvent {
    Diagnostics {
        server_id: String,
        uri: Url,
        version: Option<i32>,
        diagnostics: Vec<Diagnostic>,
    },
    State {
        server_id: String,
        state: ServerState,
    },
    Log {
        server_id: String,
        /// LSP `MessageType`: 1 error, 2 warning, 3 info, 4 log.
        level: i32,
        message: String,
    },
    /// `workspace/applyEdit` from the server (e.g. while running a code action's command):
    /// the UI applies it to open editors and the backend to the other files.
    ApplyEdit {
        server_id: String,
        label: Option<String>,
        edit: WorkspaceEdit,
    },
}

/// Language features a server may or may not announce in its capabilities; many leave out
/// rename, implementation or type definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Feature {
    Hover,
    Definition,
    References,
    Implementation,
    TypeDefinition,
    Rename,
    CodeAction,
    SignatureHelp,
    DocumentHighlight,
    TypeHierarchy,
    CallHierarchy,
}

impl Feature {
    /// PHPStorm's action name, for messages like "Rename requires …".
    pub fn title(self) -> &'static str {
        match self {
            Feature::Hover => "Quick Documentation",
            Feature::Definition => "Go to Declaration",
            Feature::References => "Find Usages",
            Feature::Implementation => "Go to Implementation",
            Feature::TypeDefinition => "Go to Type Declaration",
            Feature::Rename => "Rename",
            Feature::CodeAction => "Show Context Actions",
            Feature::SignatureHelp => "Parameter Info",
            Feature::DocumentHighlight => "Highlight Usages in File",
            Feature::TypeHierarchy => "Type Hierarchy",
            Feature::CallHierarchy => "Call Hierarchy",
        }
    }
}

type Pending = Mutex<HashMap<i64, oneshot::Sender<std::result::Result<Value, RpcError>>>>;

/// A JSON-RPC error object returned by the server.
#[derive(Debug, Clone, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

struct Inner {
    config: LspServerConfig,
    root: PathBuf,
    next_id: AtomicI64,
    outgoing: mpsc::UnboundedSender<Vec<u8>>,
    pending: Pending,
    events: broadcast::Sender<LspEvent>,
    capabilities: OnceLock<ServerCapabilities>,
    /// The capabilities as sent, for ones `lsp_types` does not model (`typeHierarchyProvider`).
    raw_capabilities: OnceLock<Value>,
    versions: Mutex<HashMap<Url, i32>>,
    state: Mutex<ServerState>,
    progress: Mutex<HashMap<String, String>>,
    process: tokio::sync::Mutex<Option<Box<dyn ServerProcess>>>,
    /// `processId` of `initialize` (see [`crate::launch::ServerIo::parent_pid`]).
    parent_pid: Option<u32>,
    /// Settings changed after the start (code style options before Reformat Code), merged
    /// over `config.settings`.
    overrides: Mutex<Value>,
    /// The server process ended (crashed, was killed or quit) without being shut down.
    exited: AtomicBool,
    /// Start, stderr, log messages and exit of the server.
    log: ServerLog,
}

/// Handle to a running language server. Cheap to clone.
#[derive(Clone)]
pub struct LspClient {
    inner: Arc<Inner>,
}

/// Convert a file path into a `file://` URI. Windows paths lose their verbatim `\\?\`
/// prefix and get an upper-case drive letter (`file:///C:/work/a.php`), the spelling
/// [`normalize_uri`] gives the URIs servers send back.
///
/// `/var/www/a.php` works on Windows too: the path of a remote project's file on its Linux
/// host.
pub fn path_to_uri(path: &Path) -> Result<Url> {
    let path = forge_core::paths::simplified(path);
    Url::from_file_path(&path)
        .ok()
        .or_else(|| path.to_str().and_then(posix_path_to_uri))
        .map(|u| normalize_uri(&u))
        .ok_or_else(|| ForgeError::Other(format!("not an absolute path: {}", path.display())))
}

/// Convert a `file://` URI back into a path (any spelling [`normalize_uri`] accepts).
/// `file:///var/www/a.php` gives `/var/www/a.php` on Windows too (a remote project's file).
pub fn uri_to_path(uri: &Url) -> Option<PathBuf> {
    let uri = normalize_uri(uri);
    uri.to_file_path()
        .ok()
        .or_else(|| uri_to_posix_path(&uri).map(PathBuf::from))
        .map(|p| forge_core::paths::simplified(&p))
}

/// `file:///var/www/a%20b.php` for `/var/www/a b.php`, on any platform (`Url::from_file_path`
/// refuses such paths on Windows, where they are not absolute).
pub fn posix_path_to_uri(path: &str) -> Option<Url> {
    if !path.starts_with('/') || path.starts_with("//") {
        return None;
    }
    let mut url = Url::parse("file:///").ok()?;
    url.set_path(path);
    Some(url)
}

/// `/var/www/a b.php` for `file:///var/www/a%20b.php`, on any platform; `None` for URIs with
/// a host or a drive letter.
pub fn uri_to_posix_path(uri: &Url) -> Option<String> {
    if uri.scheme() != "file" || uri.host_str().is_some_and(|h| !h.is_empty()) {
        return None;
    }
    let path = percent_decode(uri.path())?;
    let b = path.as_bytes();
    if b.len() >= 3 && b[2] == b':' && b[1].is_ascii_alphabetic() {
        return None;
    }
    Some(path)
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok()
}

/// One spelling for every `file://` URI of a Windows file, so URIs from servers compare
/// equal to Forge's own and map back to the project's paths. Servers write the drive in
/// many ways: php-lsp sends `file:///c%3A/work/a.php`, others `file:///c:/…`, and paths
/// with the verbatim prefix become `file:////?/C:/…`. All of them become
/// `file:///C:/work/a.php` (`file://server/share/…` for UNC paths). Other URIs are
/// returned unchanged.
pub fn normalize_uri(uri: &Url) -> Url {
    match normalize_file_uri(uri.as_str()) {
        Some(fixed) => Url::parse(&fixed).unwrap_or_else(|_| uri.clone()),
        None => uri.clone(),
    }
}

/// The string form of [`normalize_uri`]: `None` when `uri` is already normal (or not a
/// `file://` URI). Works on any platform.
pub fn normalize_file_uri(uri: &str) -> Option<String> {
    let rest = uri
        .strip_prefix("file://")
        .or_else(|| uri.strip_prefix("FILE://"))?;
    // `file:` URIs with a host (`file://server/share`) are UNC paths already.
    let mut path = rest.strip_prefix('/')?.to_string();
    let mut host = String::new();
    // A verbatim prefix kept in the URI: `file:////?/C:/…`, `file:////%3F/C%3A/…` (php-lsp's
    // spelling of `\\?\C:\…`), `file:////?/UNC/server/share/…`.
    for verbatim in ["/?/", "/%3F/", "/%3f/"] {
        if let Some(after) = path.strip_prefix(verbatim) {
            path = after.to_string();
            if let Some(unc) = path
                .strip_prefix("UNC/")
                .or_else(|| path.strip_prefix("unc/"))
            {
                let (server, tail) = unc.split_once('/').unwrap_or((unc, ""));
                host = server.to_string();
                path = tail.to_string();
            }
            break;
        }
    }
    // The drive: `c:`, `C%3A`, `c%3a` → `C:`.
    let b = path.as_bytes();
    if host.is_empty() && !b.is_empty() && b[0].is_ascii_alphabetic() {
        let after = if b.get(1) == Some(&b':') {
            Some(2)
        } else if path.len() >= 4 && path[1..4].eq_ignore_ascii_case("%3A") {
            Some(4)
        } else {
            None
        };
        if let Some(after) = after {
            let tail = &path[after..];
            if tail.is_empty() || tail.starts_with('/') {
                path = format!(
                    "{}:{}",
                    (b[0] as char).to_ascii_uppercase(),
                    if tail.is_empty() { "/" } else { tail }
                );
            }
        }
    }
    let fixed = if host.is_empty() {
        format!("file:///{path}")
    } else {
        format!("file://{host}/{path}")
    };
    (fixed != uri).then_some(fixed)
}

fn other(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

impl LspClient {
    /// Spawn the server, run the `initialize` handshake and return a ready client.
    pub async fn start(config: LspServerConfig, root: impl AsRef<Path>) -> Result<Self> {
        Self::start_with_events(config, root, broadcast::channel(256).0).await
    }

    /// Like [`start`](Self::start) but publishes events into an existing channel (used by
    /// the manager so one subscription sees every server).
    pub async fn start_with_events(
        config: LspServerConfig,
        root: impl AsRef<Path>,
        events: broadcast::Sender<LspEvent>,
    ) -> Result<Self> {
        Self::start_with_launcher(config, root, events, &LocalLauncher).await
    }

    /// Like [`start_with_logged`](Self::start_with_logged), with a log of its own.
    pub async fn start_with_launcher(
        config: LspServerConfig,
        root: impl AsRef<Path>,
        events: broadcast::Sender<LspEvent>,
        launcher: &dyn ServerLauncher,
    ) -> Result<Self> {
        Self::start_with_logged(config, root, events, launcher, ServerLog::new()).await
    }

    /// Like [`start_with_events`](Self::start_with_events) but lets `launcher` start the
    /// server process (e.g. on a remote development host), and writes the start, the
    /// server's stderr and log messages and its exit to `log`. A failed start explains
    /// itself: the error names the launch error, or the exit status and the last stderr
    /// lines of a server that quit during `initialize`.
    pub async fn start_with_logged(
        config: LspServerConfig,
        root: impl AsRef<Path>,
        events: broadcast::Sender<LspEvent>,
        launcher: &dyn ServerLauncher,
        log: ServerLog,
    ) -> Result<Self> {
        let root = forge_core::paths::simplified(root.as_ref());
        log.push(
            LogSource::Forge,
            &format!(
                "Starting {} in {}",
                std::iter::once(config.command.as_str())
                    .chain(config.args.iter().map(String::as_str))
                    .collect::<Vec<_>>()
                    .join(" "),
                root.display()
            ),
        );
        let io = match launcher.launch(&config, &root).await {
            Ok(io) => io,
            Err(e) => {
                log.push(LogSource::Forge, &format!("Failed to start: {e}"));
                return Err(e);
            }
        };
        let stdin = io.stdin;
        let stdout = io.stdout;
        let stderr = io.stderr;
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();

        let client = LspClient {
            inner: Arc::new(Inner {
                config,
                root,
                next_id: AtomicI64::new(1),
                outgoing: tx,
                pending: Mutex::new(HashMap::new()),
                events,
                capabilities: OnceLock::new(),
                raw_capabilities: OnceLock::new(),
                versions: Mutex::new(HashMap::new()),
                state: Mutex::new(ServerState::Starting),
                progress: Mutex::new(HashMap::new()),
                process: tokio::sync::Mutex::new(Some(io.process)),
                parent_pid: io.parent_pid,
                overrides: Mutex::new(Value::Null),
                exited: AtomicBool::new(false),
                log: log.clone(),
            }),
        };
        client.set_state(ServerState::Starting);

        // Writer: serializes all outgoing frames onto stdin.
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(frame) = rx.recv().await {
                if stdin.write_all(&frame).await.is_err() || stdin.flush().await.is_err() {
                    break;
                }
            }
        });

        // Server stderr goes to the server log (and the tracing log).
        let (stderr_done_tx, stderr_done) = oneshot::channel::<()>();
        if let Some(stderr) = stderr {
            let id = client.id().to_string();
            let log = log.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!(target: "forge_lsp::stderr", server = %id, "{line}");
                    log.push(LogSource::Stderr, &line);
                }
                let _ = stderr_done_tx.send(());
            });
        } else {
            drop(stderr_done_tx);
        }

        // Reader: dispatches responses, notifications and server requests.
        let weak = Arc::downgrade(&client.inner);
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                let msg = framing::read_message(&mut reader).await;
                let Some(inner) = weak.upgrade() else { break };
                let client = LspClient { inner };
                match msg {
                    Ok(Some(msg)) => client.dispatch(msg),
                    Ok(None) => {
                        client.on_exit(None);
                        break;
                    }
                    Err(e) => {
                        client.on_exit(Some(e.to_string()));
                        break;
                    }
                }
            }
        });

        if let Err(e) = client.initialize().await {
            let e = client.explain_failure(e, stderr_done).await;
            client.set_state(ServerState::Failed {
                error: e.to_string(),
            });
            client.kill().await;
            return Err(e);
        }
        Ok(client)
    }

    /// `error` of a failed `initialize`, with the exit status and the last stderr lines when
    /// the server quit (a crash, a missing library, a bad argument).
    async fn explain_failure(
        &self,
        error: ForgeError,
        stderr_done: oneshot::Receiver<()>,
    ) -> ForgeError {
        // Give a dying server a moment to exit and its stderr to drain.
        let _ = tokio::time::timeout(Duration::from_millis(1500), stderr_done).await;
        let mut status = None;
        for _ in 0..10 {
            status = match self.inner.process.lock().await.as_mut() {
                Some(p) => p.exit_status(),
                None => None,
            };
            if status.is_some() || !self.has_exited() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let tail = self.inner.log.stderr_tail(5);
        let error = if status.is_none() && tail.is_empty() {
            error
        } else {
            ForgeError::Other(explain(&error.to_string(), status.as_deref(), &tail))
        };
        self.inner
            .log
            .push(LogSource::Forge, &format!("Failed to start: {error}"));
        error
    }

    /// The server's log (start, stderr, log messages, exit).
    pub fn log(&self) -> &ServerLog {
        &self.inner.log
    }

    pub fn id(&self) -> &str {
        &self.inner.config.id
    }

    pub fn config(&self) -> &LspServerConfig {
        &self.inner.config
    }

    /// Whether the server process ended on its own (crashed, killed or quit) instead of
    /// being shut down by Forge.
    pub fn has_exited(&self) -> bool {
        self.inner.exited.load(Ordering::SeqCst)
    }

    pub fn state(&self) -> ServerState {
        self.inner.state.lock().expect("state lock").clone()
    }

    pub fn capabilities(&self) -> Option<&ServerCapabilities> {
        self.inner.capabilities.get()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LspEvent> {
        self.inner.events.subscribe()
    }

    fn set_state(&self, state: ServerState) {
        *self.inner.state.lock().expect("state lock") = state.clone();
        let _ = self.inner.events.send(LspEvent::State {
            server_id: self.id().to_string(),
            state,
        });
    }

    fn send(&self, msg: Value) -> Result<()> {
        self.inner
            .outgoing
            .send(framing::encode(&msg))
            .map_err(|_| other(format!("language server {} is not running", self.id())))
    }

    /// Send a raw request and wait for its result.
    pub async fn request_raw(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.inner
            .pending
            .lock()
            .expect("pending lock")
            .insert(id, tx);
        if let Err(e) =
            self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
        {
            self.inner.pending.lock().expect("pending lock").remove(&id);
            return Err(e);
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(e))) => Err(other(format!(
                "{method} failed: {} ({})",
                e.message, e.code
            ))),
            Ok(Err(_)) => Err(other(format!("language server {} exited", self.id()))),
            Err(_) => {
                self.inner.pending.lock().expect("pending lock").remove(&id);
                let _ = self.notify_raw("$/cancelRequest", json!({"id": id}));
                Err(other(format!("{method} timed out")))
            }
        }
    }

    /// Send a typed request.
    pub async fn request<R: Request>(&self, params: R::Params) -> Result<R::Result>
    where
        R::Params: Serialize,
        R::Result: DeserializeOwned,
    {
        let params = serde_json::to_value(params).map_err(other)?;
        let value = self.request_raw(R::METHOD, params, REQUEST_TIMEOUT).await?;
        serde_json::from_value(value).map_err(|e| other(format!("bad {} response: {e}", R::METHOD)))
    }

    /// The server's settings: its configuration with later changes merged in.
    pub fn settings(&self) -> Value {
        let mut settings = self.inner.config.settings.clone();
        let overrides = self.inner.overrides.lock().expect("overrides lock").clone();
        if !overrides.is_null() {
            if settings.is_null() {
                settings = json!({});
            }
            crate::config::merge_settings(&mut settings, &overrides);
        }
        settings
    }

    /// Merge `patch` into the server's settings and push them with
    /// `workspace/didChangeConfiguration` (later `workspace/configuration` requests see
    /// them too). Returns whether anything changed; unchanged settings are not sent again.
    pub fn update_settings(&self, patch: &Value) -> Result<bool> {
        if !patch.is_object() {
            return Ok(false);
        }
        {
            let mut overrides = self.inner.overrides.lock().expect("overrides lock");
            let mut next = if overrides.is_null() {
                json!({})
            } else {
                overrides.clone()
            };
            crate::config::merge_settings(&mut next, patch);
            if *overrides == next {
                return Ok(false);
            }
            *overrides = next;
        }
        self.notify::<notif::DidChangeConfiguration>(DidChangeConfigurationParams {
            settings: self.settings(),
        })?;
        Ok(true)
    }

    pub fn notify_raw(&self, method: &str, params: Value) -> Result<()> {
        self.send(json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    pub fn notify<N: Notification>(&self, params: N::Params) -> Result<()>
    where
        N::Params: Serialize,
    {
        self.notify_raw(N::METHOD, serde_json::to_value(params).map_err(other)?)
    }

    fn dispatch(&self, msg: Value) {
        let method = msg.get("method").and_then(Value::as_str).map(str::to_owned);
        let id = msg.get("id").cloned();
        match (method, id) {
            (Some(method), Some(id)) => {
                let result =
                    self.answer_server_request(&method, msg.get("params").unwrap_or(&Value::Null));
                let reply = match result {
                    Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                    Err((code, message)) => {
                        json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
                    }
                };
                let _ = self.send(reply);
            }
            (Some(method), None) => {
                self.on_notification(&method, msg.get("params").cloned().unwrap_or(Value::Null))
            }
            (None, Some(id)) => {
                let Some(id) = id.as_i64() else { return };
                let Some(tx) = self.inner.pending.lock().expect("pending lock").remove(&id) else {
                    return;
                };
                let outcome = match msg.get("error") {
                    Some(err) => Err(serde_json::from_value(err.clone()).unwrap_or(RpcError {
                        code: -32603,
                        message: err.to_string(),
                    })),
                    None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
                };
                let _ = tx.send(outcome);
            }
            (None, None) => tracing::warn!("ignoring malformed message from {}", self.id()),
        }
    }

    /// Answer requests the server sends to the client.
    fn answer_server_request(
        &self,
        method: &str,
        params: &Value,
    ) -> std::result::Result<Value, (i64, String)> {
        match method {
            req::WorkspaceConfiguration::METHOD => {
                let items = params
                    .get("items")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let settings = self.settings();
                Ok(Value::Array(
                    items
                        .iter()
                        .map(|item| {
                            crate::config::setting_of(
                                &settings,
                                item.get("section").and_then(Value::as_str),
                            )
                        })
                        .collect(),
                ))
            }
            req::WorkDoneProgressCreate::METHOD
            | req::RegisterCapability::METHOD
            | req::UnregisterCapability::METHOD
            | req::ShowMessageRequest::METHOD
            | req::WorkspaceDiagnosticRefresh::METHOD
            | req::SemanticTokensRefresh::METHOD
            | req::InlayHintRefreshRequest::METHOD
            | req::CodeLensRefresh::METHOD => Ok(Value::Null),
            req::WorkspaceFoldersRequest::METHOD => {
                Ok(serde_json::to_value(self.workspace_folders()).unwrap_or(Value::Null))
            }
            req::ApplyWorkspaceEdit::METHOD => {
                match serde_json::from_value::<lsp_types::ApplyWorkspaceEditParams>(params.clone())
                {
                    Ok(p) => {
                        let sent = self.inner.events.send(LspEvent::ApplyEdit {
                            server_id: self.id().to_string(),
                            label: p.label,
                            edit: p.edit,
                        });
                        Ok(match sent {
                            Ok(_) => json!({"applied": true}),
                            Err(_) => {
                                json!({"applied": false, "failureReason": "no editor is listening"})
                            }
                        })
                    }
                    Err(e) => Err((-32602, format!("bad workspace/applyEdit params: {e}"))),
                }
            }
            _ => Err((-32601, format!("method not supported: {method}"))),
        }
    }

    fn on_notification(&self, method: &str, params: Value) {
        match method {
            notif::PublishDiagnostics::METHOD => {
                if let Ok(p) = serde_json::from_value::<lsp_types::PublishDiagnosticsParams>(params)
                {
                    let _ = self.inner.events.send(LspEvent::Diagnostics {
                        server_id: self.id().to_string(),
                        // The URI as Forge spells it, so it matches the open document.
                        uri: normalize_uri(&p.uri),
                        version: p.version,
                        diagnostics: p.diagnostics,
                    });
                }
            }
            notif::Progress::METHOD => self.on_progress(&params),
            "language/status" => self.on_language_status(&params),
            notif::LogMessage::METHOD | notif::ShowMessage::METHOD => {
                let level = params.get("type").and_then(Value::as_i64).unwrap_or(4) as i32;
                let message = params
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                tracing::debug!(server = %self.id(), level, "{message}");
                self.inner.log.push(LogSource::Server, &message);
                let _ = self.inner.events.send(LspEvent::Log {
                    server_id: self.id().to_string(),
                    level,
                    message,
                });
            }
            _ => tracing::trace!(server = %self.id(), "unhandled notification {method}"),
        }
    }

    fn on_progress(&self, params: &Value) {
        let token = match params.get("token") {
            Some(Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => return,
        };
        let value = params.get("value").unwrap_or(&Value::Null);
        let kind = value
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let percentage = value
            .get("percentage")
            .and_then(Value::as_u64)
            .map(|p| p as u32);
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
        let next = {
            let mut progress = self.inner.progress.lock().expect("progress lock");
            match kind {
                "begin" => {
                    let title = text("title").unwrap_or_default();
                    let message = match text("message") {
                        Some(m) if !m.is_empty() => format!("{title}: {m}"),
                        _ => title.clone(),
                    };
                    progress.insert(token, title);
                    Some(ServerState::Busy {
                        message,
                        percentage,
                    })
                }
                "report" => {
                    let title = progress.get(&token).cloned().unwrap_or_default();
                    let message = match text("message") {
                        Some(m) if !m.is_empty() && !title.is_empty() => format!("{title}: {m}"),
                        Some(m) if !m.is_empty() => m,
                        _ => title,
                    };
                    Some(ServerState::Busy {
                        message,
                        percentage,
                    })
                }
                "end" => {
                    progress.remove(&token);
                    progress.is_empty().then_some(ServerState::Ready)
                }
                _ => None,
            }
        };
        if let Some(state) = next {
            self.set_state(state);
        }
    }

    /// Eclipse JDT LS reports its startup and the Maven / Gradle import as `language/status`
    /// (`{ type: "Starting", message: "57% Starting Java Language Server - Importing Maven
    /// project(s)" }`, then `Started` / `ServiceReady`): shown like work-done progress.
    fn on_language_status(&self, params: &Value) {
        let kind = params
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let message = params
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match kind {
            "Starting" => {
                let (percentage, message) = split_percentage(message);
                self.set_state(ServerState::Busy {
                    message: message.to_string(),
                    percentage,
                });
            }
            "Started" | "ServiceReady" => {
                let idle = self
                    .inner
                    .progress
                    .lock()
                    .expect("progress lock")
                    .is_empty();
                if idle && matches!(self.state(), ServerState::Busy { .. }) {
                    self.set_state(ServerState::Ready);
                }
            }
            _ => {}
        }
    }

    fn on_exit(&self, error: Option<String>) {
        let pending: Vec<_> = self
            .inner
            .pending
            .lock()
            .expect("pending lock")
            .drain()
            .collect();
        drop(pending); // dropping the senders fails every waiting request
        let previous = self.state();
        if previous != ServerState::Stopped {
            self.inner.exited.store(true, Ordering::SeqCst);
            let how = match &error {
                Some(e) => format!("The server exited: {e}"),
                None => "The server exited.".to_string(),
            };
            self.inner.log.push(LogSource::Forge, &how);
        }
        let state = match (error, previous) {
            (_, ServerState::Stopped) => ServerState::Stopped,
            (Some(error), _) => ServerState::Failed { error },
            (None, ServerState::Starting) => ServerState::Failed {
                error: "server exited during startup".into(),
            },
            (None, _) => ServerState::Stopped,
        };
        self.set_state(state);
    }

    fn workspace_folders(&self) -> Vec<WorkspaceFolder> {
        let name = self
            .inner
            .root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        path_to_uri(&self.inner.root)
            .map(|uri| vec![WorkspaceFolder { uri, name }])
            .unwrap_or_default()
    }

    #[allow(deprecated)] // root_uri / root_path are still read by many servers
    async fn initialize(&self) -> Result<()> {
        let mut capabilities = client_capabilities();
        if self.inner.config.pull_diagnostics {
            capabilities["textDocument"]["diagnostic"] =
                json!({ "dynamicRegistration": false, "relatedDocumentSupport": false });
        }
        let capabilities: ClientCapabilities =
            serde_json::from_value(capabilities).map_err(other)?;
        let root_uri = path_to_uri(&self.inner.root).ok();
        let init_options = match &self.inner.config.initialization_options {
            Value::Null => None,
            v => Some(v.clone()),
        };
        let params = InitializeParams {
            process_id: self.inner.parent_pid,
            root_path: Some(self.inner.root.display().to_string()),
            root_uri,
            initialization_options: init_options,
            capabilities,
            workspace_folders: Some(self.workspace_folders()),
            client_info: Some(lsp_types::ClientInfo {
                name: forge_core::APP_NAME.into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            locale: Some("en".into()),
            ..Default::default()
        };
        let params = serde_json::to_value(params).map_err(other)?;
        let result = self
            .request_raw(req::Initialize::METHOD, params, Duration::from_secs(60))
            .await?;
        if let Some(raw) = result.get("capabilities") {
            let _ = self.inner.raw_capabilities.set(raw.clone());
        }
        let result: InitializeResult = serde_json::from_value(result).map_err(other)?;
        let _ = self.inner.capabilities.set(result.capabilities);
        self.notify::<notif::Initialized>(InitializedParams {})?;
        if !self.inner.config.settings.is_null() {
            self.notify::<notif::DidChangeConfiguration>(DidChangeConfigurationParams {
                settings: self.inner.config.settings.clone(),
            })?;
        }
        for n in &self.inner.config.after_initialized {
            self.notify_raw(&n.method, n.params.clone())?;
        }
        // Progress may already have moved us to Busy; only promote from Starting.
        let starting = matches!(self.state(), ServerState::Starting);
        if starting {
            self.set_state(ServerState::Ready);
        }
        Ok(())
    }

    /// Politely stop the server (`shutdown` + `exit`), killing it if it does not comply.
    pub async fn shutdown(&self) {
        let _ = self
            .request_raw(req::Shutdown::METHOD, Value::Null, Duration::from_secs(3))
            .await;
        let _ = self.notify_raw(notif::Exit::METHOD, Value::Null);
        *self.inner.state.lock().expect("state lock") = ServerState::Stopped;
        let mut process = self.inner.process.lock().await;
        if let Some(mut p) = process.take() {
            p.stop(Duration::from_secs(2)).await;
        }
        self.set_state(ServerState::Stopped);
    }

    async fn kill(&self) {
        if let Some(mut p) = self.inner.process.lock().await.take() {
            p.kill().await;
        }
    }

    // ---- Document synchronization (full sync) ----

    pub fn did_open(&self, path: &Path, text: &str) -> Result<()> {
        let uri = path_to_uri(path)?;
        self.inner
            .versions
            .lock()
            .expect("versions lock")
            .insert(uri.clone(), 1);
        self.notify::<notif::DidOpenTextDocument>(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: self.inner.config.language_id_for(path),
                version: 1,
                text: text.to_string(),
            },
        })?;
        self.pull_diagnostics(uri, 1);
        Ok(())
    }

    pub fn is_open(&self, path: &Path) -> bool {
        path_to_uri(path)
            .map(|uri| {
                self.inner
                    .versions
                    .lock()
                    .expect("versions lock")
                    .contains_key(&uri)
            })
            .unwrap_or(false)
    }

    pub fn did_change(&self, path: &Path, text: &str) -> Result<()> {
        let uri = path_to_uri(path)?;
        let version = {
            let mut versions = self.inner.versions.lock().expect("versions lock");
            let v = versions.entry(uri.clone()).or_insert(1);
            *v += 1;
            *v
        };
        self.notify::<notif::DidChangeTextDocument>(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.to_string(),
            }],
        })?;
        self.pull_diagnostics(uri, version);
        Ok(())
    }

    /// Whether the server answers `textDocument/diagnostic` (LSP 3.17 pull diagnostics).
    pub fn supports_pull_diagnostics(&self) -> bool {
        self.inner
            .raw_capabilities
            .get()
            .and_then(|c| c.get("diagnosticProvider"))
            .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
    }

    /// Ask a pull server for the diagnostics of `uri` at `version` (shortly after the
    /// change, skipped when a newer one came in) and publish them like pushed ones.
    fn pull_diagnostics(&self, uri: Url, version: i32) {
        if !self.inner.config.pull_diagnostics || !self.supports_pull_diagnostics() {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let client = self.clone();
        handle.spawn(async move {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let current = || {
                client
                    .inner
                    .versions
                    .lock()
                    .expect("versions lock")
                    .get(&uri)
                    .copied()
            };
            if current() != Some(version) {
                return;
            }
            let params = json!({ "textDocument": { "uri": uri } });
            match client
                .request_raw("textDocument/diagnostic", params, REQUEST_TIMEOUT)
                .await
            {
                Ok(report) if report.get("kind").and_then(Value::as_str) == Some("full") => {
                    if current() != Some(version) {
                        return;
                    }
                    let diagnostics: Vec<Diagnostic> = report
                        .get("items")
                        .cloned()
                        .and_then(|i| serde_json::from_value(i).ok())
                        .unwrap_or_default();
                    let _ = client.inner.events.send(LspEvent::Diagnostics {
                        server_id: client.id().to_string(),
                        uri,
                        version: Some(version),
                        diagnostics,
                    });
                }
                Ok(_) => {}
                Err(e) => tracing::debug!(server = %client.id(), "textDocument/diagnostic: {e}"),
            }
        });
    }

    pub fn did_save(&self, path: &Path, text: &str) -> Result<()> {
        let include_text = match self
            .capabilities()
            .and_then(|c| c.text_document_sync.as_ref())
        {
            Some(lsp_types::TextDocumentSyncCapability::Options(o)) => match &o.save {
                Some(TextDocumentSyncSaveOptions::SaveOptions(s)) => {
                    s.include_text.unwrap_or(false)
                }
                _ => false,
            },
            _ => false,
        };
        self.notify::<notif::DidSaveTextDocument>(DidSaveTextDocumentParams {
            text_document: TextDocumentIdentifier {
                uri: path_to_uri(path)?,
            },
            text: include_text.then(|| text.to_string()),
        })
    }

    pub fn did_close(&self, path: &Path) -> Result<()> {
        let uri = path_to_uri(path)?;
        self.inner
            .versions
            .lock()
            .expect("versions lock")
            .remove(&uri);
        self.notify::<notif::DidCloseTextDocument>(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri },
        })
    }

    // ---- Language features ----

    fn position_params(path: &Path, position: Position) -> Result<TextDocumentPositionParams> {
        Ok(TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: path_to_uri(path)?,
            },
            position,
        })
    }

    pub async fn hover(&self, path: &Path, position: Position) -> Result<Option<Hover>> {
        self.request::<req::HoverRequest>(HoverParams {
            text_document_position_params: Self::position_params(path, position)?,
            work_done_progress_params: Default::default(),
        })
        .await
    }

    pub async fn completion(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Option<CompletionResponse>> {
        self.request::<req::Completion>(CompletionParams {
            text_document_position: Self::position_params(path, position)?,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
            context: None,
        })
        .await
    }

    pub async fn completion_resolve(&self, item: CompletionItem) -> Result<CompletionItem> {
        self.request::<req::ResolveCompletionItem>(item).await
    }

    pub async fn definition(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Option<GotoDefinitionResponse>> {
        self.request::<req::GotoDefinition>(GotoDefinitionParams {
            text_document_position_params: Self::position_params(path, position)?,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
    }

    pub async fn references(
        &self,
        path: &Path,
        position: Position,
        include_declaration: bool,
    ) -> Result<Option<Vec<Location>>> {
        self.request::<req::References>(ReferenceParams {
            text_document_position: Self::position_params(path, position)?,
            context: ReferenceContext {
                include_declaration,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
    }

    /// Usages of the symbol at `position` in its document (`textDocument/documentHighlight`).
    pub async fn document_highlight(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        self.request::<req::DocumentHighlightRequest>(DocumentHighlightParams {
            text_document_position_params: Self::position_params(path, position)?,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
    }

    pub async fn document_symbols(&self, path: &Path) -> Result<Option<DocumentSymbolResponse>> {
        self.request::<req::DocumentSymbolRequest>(DocumentSymbolParams {
            text_document: TextDocumentIdentifier {
                uri: path_to_uri(path)?,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
    }

    pub async fn signature_help(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Option<SignatureHelp>> {
        self.request::<req::SignatureHelpRequest>(SignatureHelpParams {
            text_document_position_params: Self::position_params(path, position)?,
            work_done_progress_params: Default::default(),
            context: None,
        })
        .await
    }

    pub async fn formatting(
        &self,
        path: &Path,
        options: FormattingOptions,
    ) -> Result<Option<Vec<TextEdit>>> {
        self.request::<req::Formatting>(DocumentFormattingParams {
            text_document: TextDocumentIdentifier {
                uri: path_to_uri(path)?,
            },
            options,
            work_done_progress_params: Default::default(),
        })
        .await
    }

    /// Project-wide symbol search (`workspace/symbol`).
    pub async fn workspace_symbols(&self, query: &str) -> Result<Option<WorkspaceSymbolResponse>> {
        self.request::<req::WorkspaceSymbolRequest>(WorkspaceSymbolParams {
            query: query.to_string(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
    }

    /// Whether the server announced `workspaceSymbolProvider`.
    pub fn supports_workspace_symbols(&self) -> bool {
        match self
            .capabilities()
            .and_then(|c| c.workspace_symbol_provider.as_ref())
        {
            Some(lsp_types::OneOf::Left(enabled)) => *enabled,
            Some(lsp_types::OneOf::Right(_)) => true,
            None => false,
        }
    }

    /// Whether the server announced `documentFormattingProvider`.
    pub fn supports_formatting(&self) -> bool {
        match self
            .capabilities()
            .and_then(|c| c.document_formatting_provider.as_ref())
        {
            Some(lsp_types::OneOf::Left(enabled)) => *enabled,
            Some(lsp_types::OneOf::Right(_)) => true,
            None => false,
        }
    }

    /// Whether the server announced `feature` in its capabilities.
    pub fn supports(&self, feature: Feature) -> bool {
        let Some(c) = self.capabilities() else {
            return false;
        };
        fn one_of<T>(o: Option<&OneOf<bool, T>>) -> bool {
            match o {
                Some(OneOf::Left(enabled)) => *enabled,
                Some(OneOf::Right(_)) => true,
                None => false,
            }
        }
        match feature {
            Feature::Hover => !matches!(
                c.hover_provider,
                None | Some(lsp_types::HoverProviderCapability::Simple(false))
            ),
            Feature::Definition => one_of(c.definition_provider.as_ref()),
            Feature::References => one_of(c.references_provider.as_ref()),
            Feature::Rename => one_of(c.rename_provider.as_ref()),
            Feature::Implementation => !matches!(
                c.implementation_provider,
                None | Some(ImplementationProviderCapability::Simple(false))
            ),
            Feature::TypeDefinition => !matches!(
                c.type_definition_provider,
                None | Some(TypeDefinitionProviderCapability::Simple(false))
            ),
            Feature::CodeAction => !matches!(
                c.code_action_provider,
                None | Some(CodeActionProviderCapability::Simple(false))
            ),
            Feature::SignatureHelp => c.signature_help_provider.is_some(),
            Feature::DocumentHighlight => one_of(c.document_highlight_provider.as_ref()),
            Feature::CallHierarchy => !matches!(
                c.call_hierarchy_provider,
                None | Some(lsp_types::CallHierarchyServerCapability::Simple(false))
            ),
            Feature::TypeHierarchy => match self
                .inner
                .raw_capabilities
                .get()
                .and_then(|r| r.get("typeHierarchyProvider"))
            {
                Some(Value::Bool(b)) => *b,
                Some(Value::Object(_)) => true,
                _ => false,
            },
        }
    }

    /// Whether `textDocument/prepareRename` is supported (`renameProvider.prepareProvider`).
    pub fn supports_prepare_rename(&self) -> bool {
        matches!(
            self.capabilities().and_then(|c| c.rename_provider.as_ref()),
            Some(OneOf::Right(lsp_types::RenameOptions {
                prepare_provider: Some(true),
                ..
            }))
        )
    }

    /// Whether code actions can be resolved lazily (`codeActionProvider.resolveProvider`).
    pub fn supports_code_action_resolve(&self) -> bool {
        matches!(
            self.capabilities()
                .and_then(|c| c.code_action_provider.as_ref()),
            Some(CodeActionProviderCapability::Options(
                lsp_types::CodeActionOptions {
                    resolve_provider: Some(true),
                    ..
                }
            ))
        )
    }

    pub async fn implementation(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Option<GotoDefinitionResponse>> {
        self.request::<req::GotoImplementation>(GotoDefinitionParams {
            text_document_position_params: Self::position_params(path, position)?,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
    }

    pub async fn type_definition(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Option<GotoDefinitionResponse>> {
        self.request::<req::GotoTypeDefinition>(GotoDefinitionParams {
            text_document_position_params: Self::position_params(path, position)?,
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
    }

    /// `textDocument/prepareTypeHierarchy`.
    pub async fn prepare_type_hierarchy(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        self.request::<req::TypeHierarchyPrepare>(lsp_types::TypeHierarchyPrepareParams {
            text_document_position_params: Self::position_params(path, position)?,
            work_done_progress_params: Default::default(),
        })
        .await
    }

    /// `typeHierarchy/supertypes` (`subtypes: false`) or `typeHierarchy/subtypes`.
    pub async fn type_hierarchy(
        &self,
        item: TypeHierarchyItem,
        subtypes: bool,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        if subtypes {
            self.request::<req::TypeHierarchySubtypes>(lsp_types::TypeHierarchySubtypesParams {
                item,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .await
        } else {
            self.request::<req::TypeHierarchySupertypes>(lsp_types::TypeHierarchySupertypesParams {
                item,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            })
            .await
        }
    }

    /// `textDocument/prepareCallHierarchy`.
    pub async fn prepare_call_hierarchy(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Option<Vec<CallHierarchyItem>>> {
        self.request::<req::CallHierarchyPrepare>(lsp_types::CallHierarchyPrepareParams {
            text_document_position_params: Self::position_params(path, position)?,
            work_done_progress_params: Default::default(),
        })
        .await
    }

    /// `callHierarchy/incomingCalls`.
    pub async fn incoming_calls(
        &self,
        item: CallHierarchyItem,
    ) -> Result<Option<Vec<lsp_types::CallHierarchyIncomingCall>>> {
        self.request::<req::CallHierarchyIncomingCalls>(
            lsp_types::CallHierarchyIncomingCallsParams {
                item,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        )
        .await
    }

    /// `callHierarchy/outgoingCalls`.
    pub async fn outgoing_calls(
        &self,
        item: CallHierarchyItem,
    ) -> Result<Option<Vec<lsp_types::CallHierarchyOutgoingCall>>> {
        self.request::<req::CallHierarchyOutgoingCalls>(
            lsp_types::CallHierarchyOutgoingCallsParams {
                item,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        )
        .await
    }

    pub async fn prepare_rename(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Option<PrepareRenameResponse>> {
        self.request::<req::PrepareRenameRequest>(Self::position_params(path, position)?)
            .await
    }

    pub async fn rename(
        &self,
        path: &Path,
        position: Position,
        new_name: &str,
    ) -> Result<Option<WorkspaceEdit>> {
        self.request::<req::Rename>(RenameParams {
            text_document_position: Self::position_params(path, position)?,
            new_name: new_name.to_string(),
            work_done_progress_params: Default::default(),
        })
        .await
    }

    /// `textDocument/inlayHint` for `range`.
    pub async fn inlay_hints(
        &self,
        path: &Path,
        range: Range,
    ) -> Result<Option<Vec<lsp_types::InlayHint>>> {
        self.request::<req::InlayHintRequest>(lsp_types::InlayHintParams {
            text_document: TextDocumentIdentifier {
                uri: path_to_uri(path)?,
            },
            range,
            work_done_progress_params: Default::default(),
        })
        .await
    }

    /// Whether the server announced `inlayHintProvider`.
    pub fn supports_inlay_hints(&self) -> bool {
        match self
            .capabilities()
            .and_then(|c| c.inlay_hint_provider.as_ref())
        {
            Some(OneOf::Left(enabled)) => *enabled,
            Some(OneOf::Right(_)) => true,
            None => false,
        }
    }

    pub async fn code_action_resolve(&self, action: CodeAction) -> Result<CodeAction> {
        self.request::<req::CodeActionResolveRequest>(action).await
    }

    pub async fn execute_command(
        &self,
        command: &str,
        arguments: Vec<Value>,
    ) -> Result<Option<Value>> {
        self.request::<req::ExecuteCommand>(ExecuteCommandParams {
            command: command.to_string(),
            arguments,
            work_done_progress_params: Default::default(),
        })
        .await
    }

    pub async fn code_action(
        &self,
        path: &Path,
        range: Range,
        diagnostics: Vec<Diagnostic>,
        only: Option<Vec<lsp_types::CodeActionKind>>,
    ) -> Result<Option<Vec<CodeActionOrCommand>>> {
        self.request::<req::CodeActionRequest>(CodeActionParams {
            text_document: TextDocumentIdentifier {
                uri: path_to_uri(path)?,
            },
            range,
            context: CodeActionContext {
                diagnostics,
                only,
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
    }
}

/// `"57% Starting Java Language Server"` → `(Some(57), "Starting Java Language Server")`.
pub(crate) fn split_percentage(message: &str) -> (Option<u32>, &str) {
    let trimmed = message.trim();
    if let Some((num, rest)) = trimmed.split_once('%') {
        if let Ok(p) = num.trim().parse::<u32>() {
            return (Some(p.min(100)), rest.trim());
        }
    }
    (None, trimmed)
}

/// What Forge supports, announced in `initialize`.
fn client_capabilities() -> Value {
    json!({
        "workspace": {
            "configuration": true,
            "workspaceFolders": true,
            "didChangeConfiguration": { "dynamicRegistration": false },
            "applyEdit": true,
            "workspaceEdit": {
                "documentChanges": true,
                "resourceOperations": ["create", "rename", "delete"],
                "failureHandling": "abort"
            },
            "executeCommand": { "dynamicRegistration": false },
            "symbol": {
                "symbolKind": { "valueSet": [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26] }
            }
        },
        "window": { "workDoneProgress": true },
        "general": { "positionEncodings": ["utf-16"] },
        "textDocument": {
            "synchronization": { "didSave": true, "willSave": false, "dynamicRegistration": false },
            "hover": { "contentFormat": ["markdown", "plaintext"] },
            "completion": {
                "completionItem": {
                    "snippetSupport": false,
                    "documentationFormat": ["markdown", "plaintext"],
                    "deprecatedSupport": true,
                    "tagSupport": { "valueSet": [1] },
                    "labelDetailsSupport": true,
                    "resolveSupport": { "properties": ["documentation", "detail", "additionalTextEdits"] }
                },
                "contextSupport": false
            },
            "signatureHelp": {
                "signatureInformation": {
                    "documentationFormat": ["markdown", "plaintext"],
                    "parameterInformation": { "labelOffsetSupport": true },
                    "activeParameterSupport": true
                }
            },
            "definition": { "linkSupport": true },
            "implementation": { "linkSupport": true },
            "typeDefinition": { "linkSupport": true },
            "references": {},
            "documentHighlight": {},
            "typeHierarchy": { "dynamicRegistration": false },
            "callHierarchy": { "dynamicRegistration": false },
            "rename": { "prepareSupport": true },
            "documentSymbol": { "hierarchicalDocumentSymbolSupport": true },
            "formatting": {},
            "inlayHint": { "dynamicRegistration": false },
            "codeAction": {
                "codeActionLiteralSupport": {
                    "codeActionKind": { "valueSet": ["", "quickfix", "refactor", "refactor.extract", "refactor.inline", "refactor.rewrite", "source", "source.organizeImports"] }
                },
                "isPreferredSupport": true,
                "disabledSupport": true,
                "dataSupport": true,
                "resolveSupport": { "properties": ["edit"] }
            },
            "publishDiagnostics": {
                "relatedInformation": false,
                "versionSupport": true,
                "tagSupport": { "valueSet": [1, 2] }
            }
        }
    })
}

#[cfg(test)]
mod uri_tests {
    use super::*;

    fn norm(uri: &str) -> String {
        normalize_file_uri(uri).unwrap_or_else(|| uri.to_string())
    }

    #[test]
    fn windows_drive_uris_get_one_spelling() {
        // php-lsp: encoded colon, lower-case drive.
        assert_eq!(
            norm("file:///c%3A/work/shop/src/User.php"),
            "file:///C:/work/shop/src/User.php"
        );
        assert_eq!(norm("file:///c%3a/work"), "file:///C:/work");
        assert_eq!(norm("file:///d:/a%20b/x.php"), "file:///D:/a%20b/x.php");
        assert_eq!(norm("file:///c%3A"), "file:///C:/");
        // Verbatim paths that slipped into a URI.
        assert_eq!(norm("file:////?/C:/work/a.php"), "file:///C:/work/a.php");
        assert_eq!(
            norm("file:////%3F/C%3A/work/a.php"),
            "file:///C:/work/a.php"
        );
        assert_eq!(
            norm("file:////?/UNC/server/share/a.php"),
            "file://server/share/a.php"
        );
        // Already normal, or nothing to do with drives.
        assert_eq!(normalize_file_uri("file:///C:/work/a.php"), None);
        assert_eq!(normalize_file_uri("file:///home/me/a.php"), None);
        assert_eq!(normalize_file_uri("file:///c/work"), None);
        assert_eq!(normalize_file_uri("file:///cd:/x"), None);
        assert_eq!(normalize_file_uri("file://server/share/a.php"), None);
        assert_eq!(normalize_file_uri("untitled:Untitled-1"), None);
        assert_eq!(normalize_file_uri("jdt://contents/x"), None);
    }

    #[test]
    fn server_and_forge_uris_compare_equal() {
        let ours = Url::parse("file:///C:/work/shop/a.php").unwrap();
        let php_lsp = Url::parse("file:///c%3A/work/shop/a.php").unwrap();
        assert_eq!(normalize_uri(&php_lsp), ours);
        assert_eq!(normalize_uri(&ours), ours);
    }

    #[test]
    fn posix_paths_work_as_uris_on_every_platform() {
        let uri = posix_path_to_uri("/var/www/my shop/a.php").unwrap();
        assert_eq!(uri.as_str(), "file:///var/www/my%20shop/a.php");
        assert_eq!(
            uri_to_posix_path(&uri).as_deref(),
            Some("/var/www/my shop/a.php")
        );
        assert_eq!(posix_path_to_uri("relative/a.php"), None);
        assert_eq!(
            uri_to_posix_path(&Url::parse("file:///C:/work/a.php").unwrap()),
            None
        );
        assert_eq!(
            uri_to_posix_path(&Url::parse("file://server/share/a.php").unwrap()),
            None
        );
    }

    #[test]
    fn paths_round_trip_through_uris() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("my project").join("a.php");
        let uri = path_to_uri(&file).unwrap();
        assert_eq!(
            uri_to_path(&uri).unwrap(),
            forge_core::paths::simplified(&file)
        );
        // The verbatim spelling Windows' canonicalize returns maps to the same URI.
        let canonical = dir.path().canonicalize().unwrap().join("a.php");
        assert_eq!(
            path_to_uri(&canonical).unwrap(),
            path_to_uri(&forge_core::paths::simplified(&canonical)).unwrap()
        );
        #[cfg(windows)]
        {
            let uri = path_to_uri(Path::new(r"\\?\C:\work\a.php")).unwrap();
            assert_eq!(uri.as_str(), "file:///C:/work/a.php");
            let back = uri_to_path(&Url::parse("file:///c%3A/work/a.php").unwrap()).unwrap();
            assert_eq!(back, PathBuf::from(r"C:\work\a.php"));
            assert_eq!(
                path_to_uri(Path::new("/var/www/a.php")).unwrap().as_str(),
                "file:///var/www/a.php"
            );
            assert_eq!(
                uri_to_path(&Url::parse("file:///var/www/a.php").unwrap()).unwrap(),
                PathBuf::from("/var/www/a.php")
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::split_percentage;

    #[test]
    fn language_status_percentages() {
        assert_eq!(
            split_percentage("57% Starting Java Language Server - Importing Maven project(s)"),
            (
                Some(57),
                "Starting Java Language Server - Importing Maven project(s)"
            )
        );
        assert_eq!(split_percentage("Init..."), (None, "Init..."));
        assert_eq!(split_percentage("100 % done"), (Some(100), "done"));
    }
}
