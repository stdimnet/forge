//! Routes files to language servers: one server per registered config, started lazily
//! the first time a matching file is used.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use forge_core::Result;
use lsp_types::{FormattingOptions, Position, TextEdit};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, OnceCell};

use crate::client::{LspClient, LspEvent, ServerState};
use crate::config::LspServerConfig;
use crate::model::{
    definition_locations, document_symbols, locations, EditorCompletionItem, EditorCompletionList,
    EditorHover, EditorLocation, EditorSignatureHelp, EditorSymbol,
};

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
    configs: RwLock<Vec<LspServerConfig>>,
    slots: Mutex<HashMap<String, Slot>>,
    /// Startup errors per server id (cleared when the config is registered again).
    failures: Mutex<HashMap<String, String>>,
    events: broadcast::Sender<LspEvent>,
}

impl LspManager {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        LspManager {
            root: root.into(),
            configs: RwLock::new(Vec::new()),
            slots: Mutex::new(HashMap::new()),
            failures: Mutex::new(HashMap::new()),
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
            configs.retain(|c| c.id != id);
            configs.push(config);
        }
        self.failures.lock().expect("failures lock").remove(&id);
        let old = self.slots.lock().expect("slots lock").remove(&id);
        if let Some(Some(client)) = old.and_then(|s| s.get().cloned()) {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move { client.shutdown().await });
            }
        }
        let _ = self.events.send(LspEvent::State {
            server_id: id,
            state: ServerState::NotStarted,
        });
    }

    /// Remove a server config and shut its server down. Unknown ids are ignored.
    pub fn unregister_server(&self, id: &str) {
        let removed = {
            let mut configs = self.configs.write().expect("configs lock");
            let before = configs.len();
            configs.retain(|c| c.id != id);
            configs.len() != before
        };
        self.failures.lock().expect("failures lock").remove(id);
        let old = self.slots.lock().expect("slots lock").remove(id);
        if let Some(Some(client)) = old.and_then(|s| s.get().cloned()) {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move { client.shutdown().await });
            }
        }
        if removed {
            let _ = self.events.send(LspEvent::State {
                server_id: id.to_string(),
                state: ServerState::NotStarted,
            });
        }
    }

    pub fn configs(&self) -> Vec<LspServerConfig> {
        self.configs.read().expect("configs lock").clone()
    }

    /// The config whose extensions best match `path`.
    pub fn config_for(&self, path: &Path) -> Option<LspServerConfig> {
        self.configs
            .read()
            .expect("configs lock")
            .iter()
            .filter_map(|c| c.match_len(path).map(|n| (n, c)))
            .max_by_key(|(n, _)| *n)
            .map(|(_, c)| c.clone())
    }

    fn slot(&self, id: &str) -> Slot {
        self.slots
            .lock()
            .expect("slots lock")
            .entry(id.to_string())
            .or_default()
            .clone()
    }

    /// The client for `path`, starting its server if needed. `Ok(None)` when no server
    /// handles this file type or the server failed to start.
    pub async fn client_for(&self, path: &Path) -> Option<LspClient> {
        let config = self.config_for(path)?;
        let id = config.id.clone();
        let slot = self.slot(&id);
        let events = self.events.clone();
        let root = self.root.clone();
        slot.get_or_init(|| async move {
            match LspClient::start_with_events(config, root, events).await {
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
    }

    /// The client for `path` only if its server is already running.
    pub fn running_client_for(&self, path: &Path) -> Option<LspClient> {
        let config = self.config_for(path)?;
        let slot = self
            .slots
            .lock()
            .expect("slots lock")
            .get(&config.id)
            .cloned()?;
        slot.get().cloned().flatten()
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

    // ---- Document sync ----

    pub async fn did_open(&self, path: &Path, text: &str) -> Result<()> {
        match self.client_for(path).await {
            Some(client) if client.is_open(path) => client.did_change(path, text),
            Some(client) => client.did_open(path, text),
            None => Ok(()),
        }
    }

    pub async fn did_change(&self, path: &Path, text: &str) -> Result<()> {
        match self.running_client_for(path) {
            Some(client) if client.is_open(path) => client.did_change(path, text),
            _ => self.did_open(path, text).await,
        }
    }

    pub fn did_save(&self, path: &Path, text: &str) -> Result<()> {
        match self.running_client_for(path) {
            Some(client) if client.is_open(path) => client.did_save(path, text),
            _ => Ok(()),
        }
    }

    pub fn did_close(&self, path: &Path) -> Result<()> {
        match self.running_client_for(path) {
            Some(client) if client.is_open(path) => client.did_close(path),
            _ => Ok(()),
        }
    }

    // ---- Features (all return empty results for files without a server) ----

    pub async fn hover(&self, path: &Path, position: Position) -> Result<Option<EditorHover>> {
        let Some(client) = self.client_for(path).await else {
            return Ok(None);
        };
        Ok(client.hover(path, position).await?.map(EditorHover::from))
    }

    pub async fn completion(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Option<EditorCompletionList>> {
        let Some(client) = self.client_for(path).await else {
            return Ok(None);
        };
        Ok(client
            .completion(path, position)
            .await?
            .map(EditorCompletionList::from))
    }

    pub async fn completion_resolve(
        &self,
        path: &Path,
        raw: serde_json::Value,
    ) -> Result<Option<EditorCompletionItem>> {
        let Some(client) = self.client_for(path).await else {
            return Ok(None);
        };
        let item = serde_json::from_value(raw)
            .map_err(|e| forge_core::ForgeError::Other(format!("bad completion item: {e}")))?;
        let resolved = client.completion_resolve(item).await?;
        Ok(Some(EditorCompletionItem::from(&resolved)))
    }

    pub async fn definition(&self, path: &Path, position: Position) -> Result<Vec<EditorLocation>> {
        let Some(client) = self.client_for(path).await else {
            return Ok(vec![]);
        };
        Ok(client
            .definition(path, position)
            .await?
            .map(definition_locations)
            .unwrap_or_default())
    }

    pub async fn references(&self, path: &Path, position: Position) -> Result<Vec<EditorLocation>> {
        let Some(client) = self.client_for(path).await else {
            return Ok(vec![]);
        };
        Ok(client
            .references(path, position, true)
            .await?
            .map(|l| locations(&l))
            .unwrap_or_default())
    }

    pub async fn document_symbols(&self, path: &Path) -> Result<Vec<EditorSymbol>> {
        let Some(client) = self.client_for(path).await else {
            return Ok(vec![]);
        };
        Ok(client
            .document_symbols(path)
            .await?
            .map(document_symbols)
            .unwrap_or_default())
    }

    pub async fn signature_help(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Option<EditorSignatureHelp>> {
        let Some(client) = self.client_for(path).await else {
            return Ok(None);
        };
        Ok(client
            .signature_help(path, position)
            .await?
            .filter(|h| !h.signatures.is_empty())
            .map(EditorSignatureHelp::from))
    }

    pub async fn format(
        &self,
        path: &Path,
        tab_size: u32,
        insert_spaces: bool,
    ) -> Result<Vec<TextEdit>> {
        let Some(client) = self.client_for(path).await else {
            return Ok(vec![]);
        };
        let options = FormattingOptions {
            tab_size,
            insert_spaces,
            trim_trailing_whitespace: Some(true),
            insert_final_newline: Some(true),
            ..Default::default()
        };
        Ok(client.formatting(path, options).await?.unwrap_or_default())
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

    fn cfg(id: &str) -> LspServerConfig {
        LspServerConfig {
            id: id.into(),
            name: None,
            language_ids: vec!["php".into()],
            file_extensions: vec!["php".into()],
            command: "x".into(),
            args: vec![],
            initialization_options: serde_json::Value::Null,
            settings: serde_json::Value::Null,
        }
    }

    #[test]
    fn unregister_removes_config() {
        let m = LspManager::new("/p");
        m.register_server(cfg("php"));
        m.register_server(cfg("other"));
        assert!(m.config_for(Path::new("/p/a.php")).is_some());
        m.unregister_server("php");
        m.unregister_server("missing");
        assert_eq!(m.configs().len(), 1);
        assert_eq!(m.statuses()[0].id, "other");
    }
}
