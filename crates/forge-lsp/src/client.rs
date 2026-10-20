//! An LSP client talking to one language server process over stdio.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use forge_core::{ForgeError, Result};
use lsp_types::notification::{self as notif, Notification};
use lsp_types::request::{self as req, Request};
use lsp_types::{
    ClientCapabilities, CodeActionContext, CodeActionOrCommand, CodeActionParams, CompletionItem,
    CompletionParams, CompletionResponse, Diagnostic, DidChangeConfigurationParams,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, DocumentFormattingParams, DocumentSymbolParams,
    DocumentSymbolResponse, FormattingOptions, GotoDefinitionParams, GotoDefinitionResponse, Hover,
    HoverParams, InitializeParams, InitializeResult, InitializedParams, Location, Position, Range,
    ReferenceContext, ReferenceParams, ServerCapabilities, SignatureHelp, SignatureHelpParams,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, TextDocumentSyncSaveOptions, TextEdit, Url,
    VersionedTextDocumentIdentifier, WorkspaceFolder, WorkspaceSymbolParams,
    WorkspaceSymbolResponse,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::config::{find_on_path, LspServerConfig};
use crate::framing;

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
    versions: Mutex<HashMap<Url, i32>>,
    state: Mutex<ServerState>,
    progress: Mutex<HashMap<String, String>>,
    child: tokio::sync::Mutex<Option<Child>>,
}

/// Handle to a running language server. Cheap to clone.
#[derive(Clone)]
pub struct LspClient {
    inner: Arc<Inner>,
}

/// Convert a file path into a `file://` URI.
pub fn path_to_uri(path: &Path) -> Result<Url> {
    Url::from_file_path(path)
        .map_err(|_| ForgeError::Other(format!("not an absolute path: {}", path.display())))
}

/// Convert a `file://` URI back into a path.
pub fn uri_to_path(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok()
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
        let root = root.as_ref().to_path_buf();
        let program =
            find_on_path(&config.command).unwrap_or_else(|| config.command.clone().into());
        let mut child = Command::new(&program)
            .args(&config.args)
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| other(format!("could not start {}: {e}", config.command)))?;

        let stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
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
                versions: Mutex::new(HashMap::new()),
                state: Mutex::new(ServerState::Starting),
                progress: Mutex::new(HashMap::new()),
                child: tokio::sync::Mutex::new(Some(child)),
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

        // Server stderr goes to our log.
        let id = client.id().to_string();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!(target: "forge_lsp::stderr", server = %id, "{line}");
            }
        });

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
            client.set_state(ServerState::Failed {
                error: e.to_string(),
            });
            client.kill().await;
            return Err(e);
        }
        Ok(client)
    }

    pub fn id(&self) -> &str {
        &self.inner.config.id
    }

    pub fn config(&self) -> &LspServerConfig {
        &self.inner.config
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
                Ok(Value::Array(
                    items
                        .iter()
                        .map(|item| {
                            self.inner
                                .config
                                .setting(item.get("section").and_then(Value::as_str))
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
                Ok(json!({"applied": false, "failureReason": "not supported yet"}))
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
                        uri: p.uri,
                        version: p.version,
                        diagnostics: p.diagnostics,
                    });
                }
            }
            notif::Progress::METHOD => self.on_progress(&params),
            notif::LogMessage::METHOD | notif::ShowMessage::METHOD => {
                let level = params.get("type").and_then(Value::as_i64).unwrap_or(4) as i32;
                let message = params
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                tracing::debug!(server = %self.id(), level, "{message}");
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

    fn on_exit(&self, error: Option<String>) {
        let pending: Vec<_> = self
            .inner
            .pending
            .lock()
            .expect("pending lock")
            .drain()
            .collect();
        drop(pending); // dropping the senders fails every waiting request
        let state = match (error, self.state()) {
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
        let capabilities: ClientCapabilities =
            serde_json::from_value(client_capabilities()).map_err(other)?;
        let root_uri = path_to_uri(&self.inner.root).ok();
        let init_options = match &self.inner.config.initialization_options {
            Value::Null => None,
            v => Some(v.clone()),
        };
        let params = InitializeParams {
            process_id: Some(std::process::id()),
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
        let result: InitializeResult = serde_json::from_value(result).map_err(other)?;
        let _ = self.inner.capabilities.set(result.capabilities);
        self.notify::<notif::Initialized>(InitializedParams {})?;
        if !self.inner.config.settings.is_null() {
            self.notify::<notif::DidChangeConfiguration>(DidChangeConfigurationParams {
                settings: self.inner.config.settings.clone(),
            })?;
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
        let mut child = self.inner.child.lock().await;
        if let Some(mut c) = child.take() {
            if tokio::time::timeout(Duration::from_secs(2), c.wait())
                .await
                .is_err()
            {
                let _ = c.kill().await;
            }
        }
        self.set_state(ServerState::Stopped);
    }

    async fn kill(&self) {
        if let Some(mut c) = self.inner.child.lock().await.take() {
            let _ = c.kill().await;
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
                uri,
                language_id: self.inner.config.language_id_for(path),
                version: 1,
                text: text.to_string(),
            },
        })
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
            text_document: VersionedTextDocumentIdentifier { uri, version },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.to_string(),
            }],
        })
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

    pub async fn code_action(
        &self,
        path: &Path,
        range: Range,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<Option<Vec<CodeActionOrCommand>>> {
        self.request::<req::CodeActionRequest>(CodeActionParams {
            text_document: TextDocumentIdentifier {
                uri: path_to_uri(path)?,
            },
            range,
            context: CodeActionContext {
                diagnostics,
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
    }
}

/// What Forge supports, announced in `initialize`.
fn client_capabilities() -> Value {
    json!({
        "workspace": {
            "configuration": true,
            "workspaceFolders": true,
            "didChangeConfiguration": { "dynamicRegistration": false },
            "applyEdit": false,
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
            "references": {},
            "documentSymbol": { "hierarchicalDocumentSymbolSupport": true },
            "formatting": {},
            "codeAction": {
                "codeActionLiteralSupport": {
                    "codeActionKind": { "valueSet": ["", "quickfix", "refactor", "source", "source.organizeImports"] }
                }
            },
            "publishDiagnostics": {
                "relatedInformation": false,
                "versionSupport": true,
                "tagSupport": { "valueSet": [1, 2] }
            }
        }
    })
}
