//! Routes files to language servers: one server per registered config, started lazily
//! the first time a matching file is used.
//!
//! Several servers may serve one file (TypeScript + ESLint + Tailwind CSS): every config
//! sharing the longest extension match is attached, so a more specific config
//! (`blade.php`, `=docker-compose.yml`) still replaces a generic one (`php`, `yml`).
//! Results are merged: diagnostics per server, completion lists concatenated, locations
//! deduplicated, and hover / signature help / symbols from the first server with an answer
//! (registration order is priority order).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use forge_core::{ForgeError, Result};
use futures_util::future::join_all;
use lsp_types::{
    CallHierarchyItem, Diagnostic, FormattingOptions, Position, Range, TextEdit, TypeHierarchyItem,
    Url,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{broadcast, OnceCell};

use crate::client::{Feature, LspClient, LspEvent, ServerState};
use crate::config::LspServerConfig;
use crate::launch::{LocalLauncher, ServerLauncher};
use crate::model::{
    definition_locations, document_symbols, locations, workspace_edit, workspace_symbols,
    EditorCodeAction, EditorCompletionItem, EditorCompletionList, EditorHover, EditorLocation,
    EditorPrepareRename, EditorSignatureHelp, EditorSymbol, EditorWorkspaceSymbol,
};

/// Key added to a completion item's `raw` value so `completionItem/resolve` goes back to
/// the server that produced it.
pub const COMPLETION_SERVER_KEY: &str = "forgeServer";

/// How long a project-wide symbol search waits for each server.
const WORKSPACE_SYMBOL_TIMEOUT: Duration = Duration::from_secs(5);

/// A server that exits on its own is started again on its next use, at most
/// [`MAX_RESTARTS`] times within [`RESTART_WINDOW`] (like VS Code), so a server that
/// crashes on start does not loop.
const MAX_RESTARTS: usize = 5;
const RESTART_WINDOW: Duration = Duration::from_secs(180);

/// Status of one registered server, for the status bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub state: ServerState,
}

type Slot = Arc<OnceCell<Option<LspClient>>>;

pub struct LspManager {
    root: PathBuf,
    launcher: Arc<dyn ServerLauncher>,
    configs: RwLock<Vec<LspServerConfig>>,
    slots: Mutex<HashMap<String, Slot>>,
    /// Startup errors per server id (cleared when the config is registered again).
    failures: Mutex<HashMap<String, String>>,
    /// When each server was restarted after exiting on its own (within [`RESTART_WINDOW`]).
    restarts: Mutex<HashMap<String, Vec<Instant>>>,
    /// Latest diagnostics per document and server, merged for the editor.
    diagnostics: Mutex<HashMap<Url, BTreeMap<String, Vec<Diagnostic>>>>,
    events: broadcast::Sender<LspEvent>,
}

impl LspManager {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::with_launcher(root, Arc::new(LocalLauncher))
    }

    /// A manager whose servers are started by `launcher` (e.g. on a remote host, where
    /// `root` is a path on that host).
    pub fn with_launcher(root: impl Into<PathBuf>, launcher: Arc<dyn ServerLauncher>) -> Self {
        LspManager {
            root: root.into(),
            launcher,
            configs: RwLock::new(Vec::new()),
            slots: Mutex::new(HashMap::new()),
            failures: Mutex::new(HashMap::new()),
            restarts: Mutex::new(HashMap::new()),
            diagnostics: Mutex::new(HashMap::new()),
            events: broadcast::channel(1024).0,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Events of every server managed here (diagnostics, state changes, logs).
    pub fn subscribe(&self) -> broadcast::Receiver<LspEvent> {
        self.events.subscribe()
    }

    /// Add or replace a server config. A running server with the same id is shut down
    /// and restarted on next use.
    pub fn register_server(&self, config: LspServerConfig) {
        let id = config.id.clone();
        {
            let mut configs = self.configs.write().expect("configs lock");
            match configs.iter_mut().find(|c| c.id == id) {
                // Keep the position: it is the server's priority.
                Some(existing) => *existing = config,
                None => configs.push(config),
            }
        }
        self.stop_slot(&id);
        let _ = self.events.send(LspEvent::State {
            server_id: id,
            state: ServerState::NotStarted,
        });
    }

    /// Remove a server config and stop its server. Returns whether it was registered.
    pub fn unregister_server(&self, id: &str) -> bool {
        let removed = {
            let mut configs = self.configs.write().expect("configs lock");
            let before = configs.len();
            configs.retain(|c| c.id != id);
            configs.len() != before
        };
        self.stop_slot(id);
        if removed {
            let _ = self.events.send(LspEvent::State {
                server_id: id.to_string(),
                state: ServerState::Stopped,
            });
        }
        removed
    }

    fn stop_slot(&self, id: &str) {
        self.failures.lock().expect("failures lock").remove(id);
        self.restarts.lock().expect("restarts lock").remove(id);
        for per_server in self
            .diagnostics
            .lock()
            .expect("diagnostics lock")
            .values_mut()
        {
            per_server.remove(id);
        }
        let old = self.slots.lock().expect("slots lock").remove(id);
        if let Some(Some(client)) = old.and_then(|s| s.get().cloned()) {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move { client.shutdown().await });
            }
        }
    }

    pub fn configs(&self) -> Vec<LspServerConfig> {
        self.configs.read().expect("configs lock").clone()
    }

    /// Every config serving `path`: those sharing the longest extension match, in
    /// registration (priority) order.
    pub fn configs_for(&self, path: &Path) -> Vec<LspServerConfig> {
        let configs = self.configs.read().expect("configs lock");
        let matches: Vec<(usize, &LspServerConfig)> = configs
            .iter()
            .filter_map(|c| c.match_len(path).map(|n| (n, c)))
            .collect();
        let best = matches.iter().map(|(n, _)| *n).max();
        matches
            .into_iter()
            .filter(|(n, _)| Some(*n) == best)
            .map(|(_, c)| c.clone())
            .collect()
    }

    /// The primary config for `path` (first of [`configs_for`](Self::configs_for)).
    pub fn config_for(&self, path: &Path) -> Option<LspServerConfig> {
        self.configs_for(path).into_iter().next()
    }

    /// The slot to start `id` in. A server that exited on its own gets a fresh slot, so it
    /// starts again (within the restart limit); the documents are opened in it again with
    /// their next change.
    fn slot(&self, id: &str) -> Slot {
        let current = self
            .slots
            .lock()
            .expect("slots lock")
            .entry(id.to_string())
            .or_default()
            .clone();
        let exited = matches!(current.get(), Some(Some(client)) if client.has_exited());
        if !exited || !self.may_restart(id) {
            return current;
        }
        tracing::warn!("language server {id} exited; starting it again");
        let fresh = Slot::default();
        let mut slots = self.slots.lock().expect("slots lock");
        match slots.get(id) {
            // Another caller restarted it meanwhile.
            Some(slot) if !Arc::ptr_eq(slot, &current) => slot.clone(),
            _ => {
                slots.insert(id.to_string(), fresh.clone());
                fresh
            }
        }
    }

    /// Count a restart of `id`; false (and a failure for the status bar) once it exited
    /// [`MAX_RESTARTS`] times within [`RESTART_WINDOW`].
    fn may_restart(&self, id: &str) -> bool {
        let now = Instant::now();
        let allowed = {
            let mut restarts = self.restarts.lock().expect("restarts lock");
            let times = restarts.entry(id.to_string()).or_default();
            times.retain(|t| now.duration_since(*t) < RESTART_WINDOW);
            let allowed = times.len() < MAX_RESTARTS;
            if allowed {
                times.push(now);
            }
            allowed
        };
        if !allowed {
            self.failures.lock().expect("failures lock").insert(
                id.to_string(),
                format!(
                    "exited {MAX_RESTARTS} times in {} minutes, not started again",
                    RESTART_WINDOW.as_secs() / 60
                ),
            );
        }
        allowed
    }

    async fn start(&self, config: LspServerConfig) -> Option<LspClient> {
        let id = config.id.clone();
        let slot = self.slot(&id);
        let events = self.events.clone();
        let root = self.root.clone();
        let launcher = self.launcher.clone();
        slot.get_or_init(|| async move {
            match LspClient::start_with_launcher(config, root, events, launcher.as_ref()).await {
                Ok(client) => Some(client),
                Err(e) => {
                    tracing::warn!("language server {id} failed to start: {e}");
                    self.failures
                        .lock()
                        .expect("failures lock")
                        .insert(id, e.to_string());
                    None
                }
            }
        })
        .await
        .clone()
        // Exited and not started again (restart limit).
        .filter(|c| !c.has_exited())
    }

    /// The primary client for `path`, starting its server if needed. `None` when no
    /// server handles this file type or the server failed to start.
    pub async fn client_for(&self, path: &Path) -> Option<LspClient> {
        self.clients_for(path).await.into_iter().next()
    }

    /// Every client for `path`, starting their servers (concurrently) if needed. Servers
    /// that fail to start are left out.
    pub async fn clients_for(&self, path: &Path) -> Vec<LspClient> {
        let configs = self.configs_for(path);
        join_all(configs.into_iter().map(|c| self.start(c)))
            .await
            .into_iter()
            .flatten()
            .collect()
    }

    fn running(&self, id: &str) -> Option<LspClient> {
        let slot = self.slots.lock().expect("slots lock").get(id).cloned()?;
        slot.get().cloned().flatten().filter(|c| !c.has_exited())
    }

    /// The primary client for `path` only if its server is already running.
    pub fn running_client_for(&self, path: &Path) -> Option<LspClient> {
        self.running_clients_for(path).into_iter().next()
    }

    /// Clients for `path` whose servers are already running.
    pub fn running_clients_for(&self, path: &Path) -> Vec<LspClient> {
        self.configs_for(path)
            .iter()
            .filter_map(|c| self.running(&c.id))
            .collect()
    }

    /// Every running client, in priority order.
    pub fn running_clients(&self) -> Vec<LspClient> {
        self.configs()
            .iter()
            .filter_map(|c| self.running(&c.id))
            .collect()
    }

    pub fn statuses(&self) -> Vec<ServerStatus> {
        let failures = self.failures.lock().expect("failures lock").clone();
        self.configs()
            .into_iter()
            .map(|c| {
                let state = match failures.get(&c.id) {
                    Some(error) => ServerState::Failed {
                        error: error.clone(),
                    },
                    None => {
                        let slot = self.slots.lock().expect("slots lock").get(&c.id).cloned();
                        match slot {
                            Some(slot) => match slot.get() {
                                Some(Some(client)) => client.state(),
                                Some(None) => ServerState::Failed {
                                    error: "failed to start".into(),
                                },
                                None => ServerState::Starting,
                            },
                            None => ServerState::NotStarted,
                        }
                    }
                };
                ServerStatus {
                    name: c.display_name().to_string(),
                    id: c.id,
                    state,
                }
            })
            .collect()
    }

    // ---- Diagnostics ----

    /// Record what `server_id` published for `uri` and return the merged diagnostics of
    /// every server for that document (each server replaces only its own). Diagnostics
    /// without a `source` get the server's name, so the Problems view can tell them apart.
    pub fn merge_diagnostics(
        &self,
        server_id: &str,
        uri: &Url,
        mut diagnostics: Vec<Diagnostic>,
    ) -> Vec<Diagnostic> {
        let name = self
            .configs
            .read()
            .expect("configs lock")
            .iter()
            .find(|c| c.id == server_id)
            .map(|c| c.display_name().to_string());
        if let Some(name) = name {
            for d in diagnostics.iter_mut().filter(|d| d.source.is_none()) {
                d.source = Some(name.clone());
            }
        }
        let mut all = self.diagnostics.lock().expect("diagnostics lock");
        let per_server = all.entry(uri.clone()).or_default();
        if diagnostics.is_empty() {
            per_server.remove(server_id);
        } else {
            per_server.insert(server_id.to_string(), diagnostics);
        }
        // Report in priority order rather than by server id.
        let order: Vec<String> = self
            .configs
            .read()
            .expect("configs lock")
            .iter()
            .map(|c| c.id.clone())
            .collect();
        let mut merged: Vec<(usize, &Vec<Diagnostic>)> = per_server
            .iter()
            .map(|(id, d)| (order.iter().position(|o| o == id).unwrap_or(usize::MAX), d))
            .collect();
        merged.sort_by_key(|(i, _)| *i);
        let out = merged.into_iter().flat_map(|(_, d)| d.clone()).collect();
        if per_server.is_empty() {
            all.remove(uri);
        }
        out
    }

    // ---- Document sync (every server attached to the file) ----

    pub async fn did_open(&self, path: &Path, text: &str) -> Result<()> {
        let mut result = Ok(());
        for client in self.clients_for(path).await {
            let r = if client.is_open(path) {
                client.did_change(path, text)
            } else {
                client.did_open(path, text)
            };
            if let Err(e) = r {
                result = Err(e);
            }
        }
        result
    }

    pub async fn did_change(&self, path: &Path, text: &str) -> Result<()> {
        let running = self.running_clients_for(path);
        let all_open = !running.is_empty()
            && running.len() == self.configs_for(path).len()
            && running.iter().all(|c| c.is_open(path));
        if !all_open {
            // Opens the document in servers that have not seen it yet.
            return self.did_open(path, text).await;
        }
        let mut result = Ok(());
        for client in running {
            if let Err(e) = client.did_change(path, text) {
                result = Err(e);
            }
        }
        result
    }

    pub fn did_save(&self, path: &Path, text: &str) -> Result<()> {
        let mut result = Ok(());
        for client in self.running_clients_for(path) {
            if client.is_open(path) {
                if let Err(e) = client.did_save(path, text) {
                    result = Err(e);
                }
            }
        }
        result
    }

    pub fn did_close(&self, path: &Path) -> Result<()> {
        let mut result = Ok(());
        for client in self.running_clients_for(path) {
            if client.is_open(path) {
                if let Err(e) = client.did_close(path) {
                    result = Err(e);
                }
            }
        }
        result
    }

    // ---- Features (all return empty results for files without a server) ----

    /// Ask every server of `path` concurrently. Fails only when every server failed.
    async fn ask_all<T, F, Fut>(&self, path: &Path, ask: F) -> Result<Vec<(LspClient, T)>>
    where
        F: Fn(LspClient) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let clients = self.clients_for(path).await;
        let results = join_all(clients.iter().cloned().map(&ask)).await;
        let mut ok = Vec::new();
        let mut first_err = None;
        for (client, r) in clients.into_iter().zip(results) {
            match r {
                Ok(v) => ok.push((client, v)),
                Err(e) => {
                    tracing::debug!("{} request failed: {e}", client.id());
                    first_err.get_or_insert(e);
                }
            }
        }
        match first_err {
            Some(e) if ok.is_empty() => Err(e),
            _ => Ok(ok),
        }
    }

    pub async fn hover(&self, path: &Path, position: Position) -> Result<Option<EditorHover>> {
        let answers = self
            .ask_all(path, |c| async move { c.hover(path, position).await })
            .await?;
        Ok(answers
            .into_iter()
            .filter_map(|(_, h)| h.map(EditorHover::from))
            .find(|h| !h.contents.trim().is_empty()))
    }

    pub async fn completion(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Option<EditorCompletionList>> {
        let answers = self
            .ask_all(path, |c| async move { c.completion(path, position).await })
            .await?;
        let several = answers.len() > 1;
        let mut merged: Option<EditorCompletionList> = None;
        for (client, list) in answers {
            let Some(list) = list.map(EditorCompletionList::from) else {
                continue;
            };
            let mut items = list.items;
            for item in &mut items {
                if let Value::Object(raw) = &mut item.raw {
                    raw.insert(COMPLETION_SERVER_KEY.into(), client.id().into());
                }
            }
            match &mut merged {
                Some(m) => {
                    m.is_incomplete |= list.is_incomplete;
                    m.items.extend(items);
                }
                None => {
                    merged = Some(EditorCompletionList {
                        is_incomplete: list.is_incomplete,
                        items,
                    })
                }
            }
        }
        if several {
            if let Some(m) = &mut merged {
                // Same label from two servers (e.g. TypeScript and Tailwind): keep the first.
                let mut seen = std::collections::HashSet::new();
                m.items
                    .retain(|i| seen.insert((i.label.clone(), i.kind.clone())));
            }
        }
        Ok(merged)
    }

    pub async fn completion_resolve(
        &self,
        path: &Path,
        mut raw: Value,
    ) -> Result<Option<EditorCompletionItem>> {
        let server = raw
            .as_object_mut()
            .and_then(|o| o.remove(COMPLETION_SERVER_KEY))
            .and_then(|v| v.as_str().map(str::to_owned));
        let clients = self.clients_for(path).await;
        let client = match &server {
            Some(id) => clients.into_iter().find(|c| c.id() == id),
            None => clients.into_iter().next(),
        };
        let Some(client) = client else {
            return Ok(None);
        };
        let item = serde_json::from_value(raw)
            .map_err(|e| ForgeError::Other(format!("bad completion item: {e}")))?;
        let resolved = client.completion_resolve(item).await?;
        let mut out = EditorCompletionItem::from(&resolved);
        if let Value::Object(raw) = &mut out.raw {
            raw.insert(COMPLETION_SERVER_KEY.into(), client.id().into());
        }
        Ok(Some(out))
    }

    fn dedup_locations(locs: impl IntoIterator<Item = EditorLocation>) -> Vec<EditorLocation> {
        let mut out: Vec<EditorLocation> = Vec::new();
        for l in locs {
            if !out.contains(&l) {
                out.push(l);
            }
        }
        out
    }

    pub async fn definition(&self, path: &Path, position: Position) -> Result<Vec<EditorLocation>> {
        let answers = self
            .ask_all(path, |c| async move { c.definition(path, position).await })
            .await?;
        Ok(Self::dedup_locations(answers.into_iter().flat_map(
            |(_, r)| r.map(definition_locations).unwrap_or_default(),
        )))
    }

    pub async fn references(&self, path: &Path, position: Position) -> Result<Vec<EditorLocation>> {
        let answers = self
            .ask_all(
                path,
                |c| async move { c.references(path, position, true).await },
            )
            .await?;
        Ok(Self::dedup_locations(answers.into_iter().flat_map(
            |(_, r)| r.map(|l| locations(&l)).unwrap_or_default(),
        )))
    }

    /// `documentHighlight` of the servers that announce it: the first non-empty answer as
    /// `(range, kind)` with LSP's kind numbers (1 text, 2 read, 3 write).
    pub async fn document_highlights(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Vec<(lsp_types::Range, Option<u8>)>> {
        let clients = self.supporting(path, Feature::DocumentHighlight).await;
        let answers = join_all(
            clients
                .into_iter()
                .map(|c| async move { c.document_highlight(path, position).await }),
        )
        .await;
        Ok(answers
            .into_iter()
            .filter_map(|r| r.ok().flatten())
            .find(|h| !h.is_empty())
            .map(|h| {
                h.into_iter()
                    .map(|d| {
                        let kind = d.kind.map(|k| {
                            if k == lsp_types::DocumentHighlightKind::WRITE {
                                3
                            } else if k == lsp_types::DocumentHighlightKind::READ {
                                2
                            } else {
                                1
                            }
                        });
                        (d.range, kind)
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn document_symbols(&self, path: &Path) -> Result<Vec<EditorSymbol>> {
        let answers = self
            .ask_all(path, |c| async move { c.document_symbols(path).await })
            .await?;
        Ok(answers
            .into_iter()
            .filter_map(|(_, r)| r.map(document_symbols))
            .find(|s| !s.is_empty())
            .unwrap_or_default())
    }

    pub async fn signature_help(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Option<EditorSignatureHelp>> {
        let answers = self
            .ask_all(
                path,
                |c| async move { c.signature_help(path, position).await },
            )
            .await?;
        Ok(answers
            .into_iter()
            .filter_map(|(_, h)| h)
            .find(|h| !h.signatures.is_empty())
            .map(EditorSignatureHelp::from))
    }

    /// Format with the first server that announced formatting support (falling back to
    /// the primary server).
    pub async fn format(
        &self,
        path: &Path,
        tab_size: u32,
        insert_spaces: bool,
        settings: Option<&Value>,
    ) -> Result<Vec<TextEdit>> {
        let clients = self.clients_for(path).await;
        let Some(client) = clients
            .iter()
            .find(|c| c.supports_formatting())
            .or_else(|| clients.first())
        else {
            return Ok(vec![]);
        };
        // Code style options the server reads from its settings.
        if let Some(settings) = settings {
            client.update_settings(settings)?;
        }
        let options = FormattingOptions {
            tab_size,
            insert_spaces,
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            ..Default::default()
        };
        Ok(client.formatting(path, options).await?.unwrap_or_default())
    }

    // ---- Navigation, rename and code actions (servers that announce the feature) ----

    /// Names of the servers attached to `path` when none of them announces `feature`, so
    /// the UI can say why nothing happens (a server without rename). `None` when a
    /// server supports it or no server serves the file.
    pub async fn missing_feature(&self, path: &Path, feature: Feature) -> Option<Vec<String>> {
        let clients = self.clients_for(path).await;
        if clients.is_empty() || clients.iter().any(|c| c.supports(feature)) {
            return None;
        }
        Some(
            clients
                .iter()
                .map(|c| c.config().display_name().to_string())
                .collect(),
        )
    }

    async fn supporting(&self, path: &Path, feature: Feature) -> Vec<LspClient> {
        self.clients_for(path)
            .await
            .into_iter()
            .filter(|c| c.supports(feature))
            .collect()
    }

    async fn locations_from<F, Fut>(
        &self,
        path: &Path,
        feature: Feature,
        ask: F,
    ) -> Result<Vec<EditorLocation>>
    where
        F: Fn(LspClient) -> Fut,
        Fut: std::future::Future<Output = Result<Option<lsp_types::GotoDefinitionResponse>>>,
    {
        let clients = self.supporting(path, feature).await;
        let results = join_all(clients.into_iter().map(ask)).await;
        let mut ok = Vec::new();
        let mut first_err = None;
        for r in results {
            match r {
                Ok(r) => ok.extend(r.map(definition_locations).unwrap_or_default()),
                Err(e) => {
                    first_err.get_or_insert(e);
                }
            }
        }
        match first_err {
            Some(e) if ok.is_empty() => Err(e),
            _ => Ok(Self::dedup_locations(ok)),
        }
    }

    /// Go to Implementation (`textDocument/implementation`).
    pub async fn implementation(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Vec<EditorLocation>> {
        self.locations_from(path, Feature::Implementation, |c| async move {
            c.implementation(path, position).await
        })
        .await
    }

    /// Go to Type Declaration (`textDocument/typeDefinition`).
    pub async fn type_definition(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Vec<EditorLocation>> {
        self.locations_from(path, Feature::TypeDefinition, |c| async move {
            c.type_definition(path, position).await
        })
        .await
    }

    /// `prepareTypeHierarchy` of the servers of `path` that support it: their items with the
    /// server id (to ask that server for supertypes and subtypes).
    pub async fn prepare_type_hierarchy(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Vec<(String, TypeHierarchyItem)>> {
        let clients = self.supporting(path, Feature::TypeHierarchy).await;
        let results = join_all(
            clients
                .iter()
                .map(|c| c.prepare_type_hierarchy(path, position)),
        )
        .await;
        Self::with_servers(clients, results)
    }

    /// Supertypes (or subtypes) of an item `server` handed out.
    pub async fn type_hierarchy(
        &self,
        server: &str,
        item: TypeHierarchyItem,
        subtypes: bool,
    ) -> Result<Vec<TypeHierarchyItem>> {
        let client = self.running_server(server)?;
        Ok(client
            .type_hierarchy(item, subtypes)
            .await?
            .unwrap_or_default())
    }

    /// `prepareCallHierarchy` of the servers of `path` that support it, with the server id.
    pub async fn prepare_call_hierarchy(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Vec<(String, CallHierarchyItem)>> {
        let clients = self.supporting(path, Feature::CallHierarchy).await;
        let results = join_all(
            clients
                .iter()
                .map(|c| c.prepare_call_hierarchy(path, position)),
        )
        .await;
        Self::with_servers(clients, results)
    }

    /// Callers of an item `server` handed out: each caller with the ranges it calls from.
    pub async fn incoming_calls(
        &self,
        server: &str,
        item: CallHierarchyItem,
    ) -> Result<Vec<(CallHierarchyItem, Vec<Range>)>> {
        let client = self.running_server(server)?;
        Ok(client
            .incoming_calls(item)
            .await?
            .unwrap_or_default()
            .into_iter()
            .map(|c| (c.from, c.from_ranges))
            .collect())
    }

    /// Callees of an item `server` handed out: each callee with the ranges (in the item's
    /// file) it is called at.
    pub async fn outgoing_calls(
        &self,
        server: &str,
        item: CallHierarchyItem,
    ) -> Result<Vec<(CallHierarchyItem, Vec<Range>)>> {
        let client = self.running_server(server)?;
        Ok(client
            .outgoing_calls(item)
            .await?
            .unwrap_or_default()
            .into_iter()
            .map(|c| (c.to, c.from_ranges))
            .collect())
    }

    fn running_server(&self, id: &str) -> Result<LspClient> {
        self.running(id)
            .ok_or_else(|| ForgeError::Other(format!("language server {id} is not running")))
    }

    /// Items of several servers, tagged with the server id; fails only when all failed.
    fn with_servers<T>(
        clients: Vec<LspClient>,
        results: Vec<Result<Option<Vec<T>>>>,
    ) -> Result<Vec<(String, T)>> {
        let mut ok = Vec::new();
        let mut first_err = None;
        for (client, r) in clients.into_iter().zip(results) {
            match r {
                Ok(items) => ok.extend(
                    items
                        .unwrap_or_default()
                        .into_iter()
                        .map(|i| (client.id().to_string(), i)),
                ),
                Err(e) => {
                    first_err.get_or_insert(e);
                }
            }
        }
        match first_err {
            Some(e) if ok.is_empty() => Err(e),
            _ => Ok(ok),
        }
    }

    /// What can be renamed at `position`: `Ok(None)` when nothing can (the server said so);
    /// an answer without range when the server has no `prepareRename` (rename the word).
    pub async fn prepare_rename(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Option<EditorPrepareRename>> {
        let Some(client) = self
            .supporting(path, Feature::Rename)
            .await
            .into_iter()
            .next()
        else {
            return Err(ForgeError::Other(
                "no language server can rename here".into(),
            ));
        };
        if !client.supports_prepare_rename() {
            return Ok(Some(EditorPrepareRename {
                range: None,
                placeholder: None,
            }));
        }
        Ok(client
            .prepare_rename(path, position)
            .await?
            .map(EditorPrepareRename::from))
    }

    /// The edit renaming the symbol at `position` (absolute paths).
    pub async fn rename(
        &self,
        path: &Path,
        position: Position,
        new_name: &str,
    ) -> Result<Option<forge_codeintel::WorkspaceEdit>> {
        let Some(client) = self
            .supporting(path, Feature::Rename)
            .await
            .into_iter()
            .next()
        else {
            return Err(ForgeError::Other(
                "no language server can rename here".into(),
            ));
        };
        Ok(client
            .rename(path, position, new_name)
            .await?
            .map(|e| workspace_edit(&e)))
    }

    /// Diagnostics `server_id` reported for `uri` that touch `range` (context for code actions).
    fn diagnostics_in(&self, server_id: &str, uri: &Url, range: Range) -> Vec<Diagnostic> {
        self.diagnostics
            .lock()
            .expect("diagnostics lock")
            .get(uri)
            .and_then(|per_server| per_server.get(server_id))
            .map(|all| {
                all.iter()
                    .filter(|d| d.range.start <= range.end && range.start <= d.range.end)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Quick fixes and refactorings of every server for `range`, with the diagnostics each
    /// server reported there as context. Servers keep their priority order.
    pub async fn code_actions(&self, path: &Path, range: Range) -> Result<Vec<EditorCodeAction>> {
        let uri = crate::client::path_to_uri(path)?;
        let clients = self.supporting(path, Feature::CodeAction).await;
        let results = join_all(clients.iter().map(|c| {
            let diagnostics = self.diagnostics_in(c.id(), &uri, range);
            async move { c.code_action(path, range, diagnostics, None).await }
        }))
        .await;
        let mut out = Vec::new();
        let mut first_err = None;
        for (client, r) in clients.iter().zip(results) {
            match r {
                Ok(actions) => out.extend(actions.unwrap_or_default().iter().map(|a| {
                    EditorCodeAction::from_lsp(
                        a,
                        client.id(),
                        client.supports_code_action_resolve(),
                    )
                })),
                Err(e) => {
                    tracing::debug!("{} code actions failed: {e}", client.id());
                    first_err.get_or_insert(e);
                }
            }
        }
        match first_err {
            Some(e) if out.is_empty() && !clients.is_empty() => Err(e),
            _ => Ok(out),
        }
    }

    /// Source actions of one kind for `range` (the whole document): `source.organizeImports`
    /// for Optimize Imports, `source.addMissingImports` / `source.removeUnusedImports` for
    /// Auto Import on the fly. Servers without code actions or that fail answer nothing.
    pub async fn source_actions(
        &self,
        path: &Path,
        range: Range,
        kind: &str,
    ) -> Vec<EditorCodeAction> {
        let clients = self.supporting(path, Feature::CodeAction).await;
        let only = vec![lsp_types::CodeActionKind::from(kind.to_string())];
        let results = join_all(
            clients
                .iter()
                .map(|c| c.code_action(path, range, vec![], Some(only.clone()))),
        )
        .await;
        let mut out = Vec::new();
        for (client, r) in clients.iter().zip(results) {
            match r {
                Ok(actions) => out.extend(
                    actions
                        .unwrap_or_default()
                        .iter()
                        .map(|a| {
                            EditorCodeAction::from_lsp(
                                a,
                                client.id(),
                                client.supports_code_action_resolve(),
                            )
                        })
                        // Servers may answer more than asked for.
                        .filter(|a| {
                            a.kind
                                .as_deref()
                                .is_some_and(|k| k == kind || k.starts_with(&format!("{kind}.")))
                        }),
                ),
                Err(e) => tracing::debug!("{} {kind} failed: {e}", client.id()),
            }
        }
        out
    }

    /// Inlay hints of every server of `path` that announces them, for `range`. Failing
    /// servers are skipped (hints are decoration, never an error).
    pub async fn inlay_hints(&self, path: &Path, range: Range) -> Vec<lsp_types::InlayHint> {
        let clients: Vec<LspClient> = self
            .clients_for(path)
            .await
            .into_iter()
            .filter(|c| c.supports_inlay_hints())
            .collect();
        let results = join_all(clients.iter().map(|c| c.inlay_hints(path, range))).await;
        let mut out = Vec::new();
        for (client, r) in clients.iter().zip(results) {
            match r {
                Ok(hints) => out.extend(hints.unwrap_or_default()),
                Err(e) => tracing::debug!("{} inlay hints failed: {e}", client.id()),
            }
        }
        out
    }

    async fn server_client(&self, path: &Path, server: &str) -> Result<LspClient> {
        self.clients_for(path)
            .await
            .into_iter()
            .find(|c| c.id() == server)
            .ok_or_else(|| ForgeError::Other(format!("language server {server} is not running")))
    }

    /// Fill in the edit of a lazily resolved code action.
    pub async fn resolve_code_action(
        &self,
        path: &Path,
        server: &str,
        raw: Value,
    ) -> Result<EditorCodeAction> {
        let client = self.server_client(path, server).await?;
        let action = serde_json::from_value(raw)
            .map_err(|e| ForgeError::Other(format!("bad code action: {e}")))?;
        let resolved = client.code_action_resolve(action).await?;
        Ok(EditorCodeAction::from_action(&resolved, server, false))
    }

    /// Run a code action's command on its server. Edits it makes arrive as
    /// [`LspEvent::ApplyEdit`].
    pub async fn execute_command(
        &self,
        path: &Path,
        server: &str,
        command: &str,
        arguments: Vec<Value>,
    ) -> Result<Option<Value>> {
        let client = self.server_client(path, server).await?;
        client.execute_command(command, arguments).await
    }

    /// Project-wide symbol search over every running server that supports it. Slow or
    /// failing servers are skipped. Results keep server priority order.
    pub async fn workspace_symbols(&self, query: &str) -> Vec<EditorWorkspaceSymbol> {
        let clients: Vec<LspClient> = self
            .running_clients()
            .into_iter()
            .filter(|c| c.supports_workspace_symbols())
            .collect();
        let results = join_all(clients.iter().map(|c| async move {
            match tokio::time::timeout(WORKSPACE_SYMBOL_TIMEOUT, c.workspace_symbols(query)).await {
                Ok(Ok(Some(r))) => workspace_symbols(r, c.id()),
                Ok(Ok(None)) => vec![],
                Ok(Err(e)) => {
                    tracing::debug!("workspace/symbol on {} failed: {e}", c.id());
                    vec![]
                }
                Err(_) => vec![],
            }
        }))
        .await;
        results.into_iter().flatten().collect()
    }

    /// Stop every running server.
    pub async fn shutdown_all(&self) {
        let slots: Vec<Slot> = self
            .slots
            .lock()
            .expect("slots lock")
            .drain()
            .map(|(_, s)| s)
            .collect();
        for slot in slots {
            if let Some(Some(client)) = slot.get() {
                client.shutdown().await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg(id: &str, exts: &[&str]) -> LspServerConfig {
        LspServerConfig {
            id: id.into(),
            name: Some(id.to_uppercase()),
            language_ids: vec![],
            file_extensions: exts.iter().map(|e| e.to_string()).collect(),
            command: id.into(),
            args: vec![],
            initialization_options: Value::Null,
            settings: Value::Null,
            ..Default::default()
        }
    }

    #[test]
    fn routes_to_every_server_with_the_longest_match() {
        let m = LspManager::new("/p");
        m.register_server(cfg("ts", &["ts", "tsx"]));
        m.register_server(cfg("eslint", &["ts", "js"]));
        m.register_server(cfg("yaml", &["yml"]));
        m.register_server(cfg("compose", &["=docker-compose.yml"]));
        m.register_server(cfg("php", &["php"]));
        m.register_server(cfg("blade", &["blade.php"]));
        let ids = |p: &str| -> Vec<String> {
            m.configs_for(Path::new(p))
                .into_iter()
                .map(|c| c.id)
                .collect()
        };
        assert_eq!(ids("/p/a.ts"), vec!["ts", "eslint"]);
        assert_eq!(ids("/p/a.js"), vec!["eslint"]);
        assert_eq!(ids("/p/docker-compose.yml"), vec!["compose"]);
        assert_eq!(ids("/p/ci.yml"), vec!["yaml"]);
        assert_eq!(ids("/p/v.blade.php"), vec!["blade"]);
        assert!(ids("/p/readme.txt").is_empty());

        // Re-registering keeps the priority position; unregistering removes it.
        m.register_server(cfg("ts", &["ts"]));
        assert_eq!(ids("/p/a.ts"), vec!["ts", "eslint"]);
        assert!(m.unregister_server("ts"));
        assert!(!m.unregister_server("ts"));
        assert_eq!(ids("/p/a.ts"), vec!["eslint"]);
    }

    fn diag(line: u32, source: Option<&str>) -> Diagnostic {
        serde_json::from_value(json!({
            "range": {"start": {"line": line, "character": 0}, "end": {"line": line, "character": 1}},
            "message": "m", "source": source
        }))
        .unwrap()
    }

    #[test]
    fn diagnostics_are_merged_per_server() {
        let m = LspManager::new("/p");
        m.register_server(cfg("ts", &["ts"]));
        m.register_server(cfg("eslint", &["ts"]));
        let uri = Url::parse("file:///p/a.ts").unwrap();

        let merged = m.merge_diagnostics("eslint", &uri, vec![diag(3, Some("eslint"))]);
        assert_eq!(merged.len(), 1);
        let merged = m.merge_diagnostics("ts", &uri, vec![diag(1, None), diag(2, Some("ts"))]);
        assert_eq!(merged.len(), 3);
        // Priority order: ts first; missing sources get the server name.
        assert_eq!(merged[0].source.as_deref(), Some("TS"));
        assert_eq!(merged[2].source.as_deref(), Some("eslint"));

        // A server clearing its diagnostics leaves the others alone.
        let merged = m.merge_diagnostics("ts", &uri, vec![]);
        assert_eq!(merged.len(), 1);
        let merged = m.merge_diagnostics("eslint", &uri, vec![]);
        assert!(merged.is_empty());
        assert!(m.diagnostics.lock().unwrap().is_empty());
    }
}
