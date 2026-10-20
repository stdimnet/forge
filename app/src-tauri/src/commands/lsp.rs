//! Language server IPC. Paths are workspace-relative like the `fs_*` commands; positions
//! are LSP positions (0-based line, UTF-16 character).
//!
//! Events:
//! - `lsp://diagnostics` `{ path, diagnostics: EditorDiagnostic[] }`
//! - `lsp://status` `ServerStatus[]`

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use forge_core::{ForgeError, Result};
use forge_lsp::lsp_types::{Position, TextEdit};
use forge_lsp::model::{
    EditorCompletionItem, EditorCompletionList, EditorDiagnostic, EditorHover, EditorLocation,
    EditorSignatureHelp, EditorSymbol,
};
use forge_lsp::{find_on_path, LspEvent, LspManager, LspServerConfig, ServerStatus};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;

/// Register the built-in fallback servers below. Language packs (e.g. PHPForge's
/// `lang-php`) register their own configs through [`LspState::register_server`]; the
/// integrator can switch this off once they do.
pub const REGISTER_DEFAULT_SERVERS: bool = true;

/// Fallback servers used when they are installed on `PATH`.
pub fn default_servers() -> Vec<LspServerConfig> {
    let mut servers = Vec::new();
    if find_on_path("intelephense").is_some() {
        servers.push(LspServerConfig {
            id: "intelephense".into(),
            name: Some("Intelephense".into()),
            language_ids: vec!["php".into()],
            file_extensions: vec!["php".into(), "phtml".into()],
            command: "intelephense".into(),
            args: vec!["--stdio".into()],
            initialization_options: json!({
                "storagePath": std::env::temp_dir().join("forge-intelephense").display().to_string(),
                "clearCache": false
            }),
            settings: json!({
                "intelephense": {
                    "files": { "maxSize": 5_000_000, "exclude": ["**/.git/**", "**/node_modules/**"] },
                    "format": { "enable": true }
                }
            }),
        });
    }
    servers
}

#[derive(Serialize, Clone)]
struct DiagnosticsEvent {
    path: String,
    diagnostics: Vec<EditorDiagnostic>,
}

/// Language servers of the open project. Managed by Tauri alongside [`AppState`].
#[derive(Default)]
pub struct LspState {
    manager: Mutex<Option<Arc<LspManager>>>,
    registered: Mutex<Vec<LspServerConfig>>,
}

impl LspState {
    /// Add (or replace by id) a server config for this and future projects.
    pub fn register_server(&self, config: LspServerConfig) {
        {
            let mut registered = self.registered.lock().expect("lsp lock");
            registered.retain(|c| c.id != config.id);
            registered.push(config.clone());
        }
        if let Some(m) = self.manager.lock().expect("lsp lock").as_ref() {
            m.register_server(config);
        }
    }

    /// The manager for the open workspace, (re)created when the project changes.
    fn manager(&self, app: &AppHandle, ws: &AppState) -> Result<Arc<LspManager>> {
        let root = ws.with_workspace(|w| Ok(w.root().to_path_buf()))?;
        let mut current = self.manager.lock().expect("lsp lock");
        if let Some(m) = current.as_ref().filter(|m| m.root() == root) {
            return Ok(m.clone());
        }
        if let Some(old) = current.take() {
            tauri::async_runtime::spawn(async move { old.shutdown_all().await });
        }
        let manager = Arc::new(LspManager::new(root));
        let defaults = if REGISTER_DEFAULT_SERVERS {
            default_servers()
        } else {
            vec![]
        };
        for config in defaults
            .into_iter()
            .chain(self.registered.lock().expect("lsp lock").iter().cloned())
        {
            manager.register_server(config);
        }
        forward_events(app.clone(), &manager);
        *current = Some(manager.clone());
        Ok(manager)
    }
}

/// Workspace-relative, `/`-separated path for the UI (absolute if outside the project).
fn to_ui_path(root: &Path, abs: &Path) -> String {
    match abs.strip_prefix(root) {
        Ok(rel) => rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => abs.display().to_string(),
    }
}

fn forward_events(app: AppHandle, manager: &Arc<LspManager>) {
    let mut rx = manager.subscribe();
    let weak = Arc::downgrade(manager);
    tauri::async_runtime::spawn(async move {
        loop {
            let event = match rx.recv().await {
                Ok(e) => e,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            };
            let Some(manager) = weak.upgrade() else { break };
            match event {
                LspEvent::Diagnostics {
                    uri, diagnostics, ..
                } => {
                    let Some(abs) = forge_lsp::uri_to_path(&uri) else {
                        continue;
                    };
                    let _ = app.emit(
                        "lsp://diagnostics",
                        DiagnosticsEvent {
                            path: to_ui_path(manager.root(), &abs),
                            diagnostics: diagnostics.iter().map(EditorDiagnostic::from).collect(),
                        },
                    );
                }
                LspEvent::State { .. } => {
                    let _ = app.emit("lsp://status", manager.statuses());
                }
                LspEvent::Log { .. } => {}
            }
        }
    });
}

struct Ctx {
    manager: Arc<LspManager>,
    abs: PathBuf,
}

impl Ctx {
    fn new(path: &str, app: &AppHandle, lsp: &LspState, ws: &AppState) -> Result<Self> {
        let manager = lsp.manager(app, ws)?;
        let abs = ws.with_workspace(|w| w.resolve(path))?;
        Ok(Ctx { manager, abs })
    }

    fn relativize(&self, mut locs: Vec<EditorLocation>) -> Vec<EditorLocation> {
        for l in &mut locs {
            l.path = to_ui_path(self.manager.root(), Path::new(&l.path));
        }
        locs
    }
}

#[tauri::command]
pub async fn lsp_did_open(
    path: String,
    text: String,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<()> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    c.manager.did_open(&c.abs, &text).await
}

#[tauri::command]
pub async fn lsp_did_change(
    path: String,
    text: String,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<()> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    c.manager.did_change(&c.abs, &text).await
}

#[tauri::command]
pub async fn lsp_did_save(
    path: String,
    text: String,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<()> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    c.manager.did_save(&c.abs, &text)
}

#[tauri::command]
pub async fn lsp_did_close(
    path: String,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<()> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    c.manager.did_close(&c.abs)
}

#[tauri::command]
pub async fn lsp_hover(
    path: String,
    line: u32,
    character: u32,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Option<EditorHover>> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    c.manager
        .hover(&c.abs, Position::new(line, character))
        .await
}

#[tauri::command]
pub async fn lsp_completion(
    path: String,
    line: u32,
    character: u32,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Option<EditorCompletionList>> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    c.manager
        .completion(&c.abs, Position::new(line, character))
        .await
}

#[tauri::command]
pub async fn lsp_completion_resolve(
    path: String,
    item: Value,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Option<EditorCompletionItem>> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    c.manager.completion_resolve(&c.abs, item).await
}

#[tauri::command]
pub async fn lsp_definition(
    path: String,
    line: u32,
    character: u32,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Vec<EditorLocation>> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let locs = c
        .manager
        .definition(&c.abs, Position::new(line, character))
        .await?;
    Ok(c.relativize(locs))
}

#[tauri::command]
pub async fn lsp_references(
    path: String,
    line: u32,
    character: u32,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Vec<EditorLocation>> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let locs = c
        .manager
        .references(&c.abs, Position::new(line, character))
        .await?;
    Ok(c.relativize(locs))
}

#[tauri::command]
pub async fn lsp_document_symbols(
    path: String,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Vec<EditorSymbol>> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    c.manager.document_symbols(&c.abs).await
}

#[tauri::command]
pub async fn lsp_signature_help(
    path: String,
    line: u32,
    character: u32,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Option<EditorSignatureHelp>> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    c.manager
        .signature_help(&c.abs, Position::new(line, character))
        .await
}

#[tauri::command]
pub async fn lsp_format(
    path: String,
    tab_size: Option<u32>,
    insert_spaces: Option<bool>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Vec<TextEdit>> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    c.manager
        .format(&c.abs, tab_size.unwrap_or(4), insert_spaces.unwrap_or(true))
        .await
}

/// Status of every registered server (for the status bar).
#[tauri::command]
pub fn lsp_status(
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Vec<ServerStatus>> {
    match lsp.manager(&app, &ws) {
        Ok(m) => Ok(m.statuses()),
        // No project open yet: nothing is running.
        Err(ForgeError::Other(_)) => Ok(vec![]),
        Err(e) => Err(e),
    }
}

/// Register a server config at runtime (used by language packs and plugins).
#[tauri::command]
pub fn lsp_register_server(config: LspServerConfig, lsp: State<'_, LspState>) -> Result<()> {
    if config.command.trim().is_empty() || config.file_extensions.is_empty() {
        return Err(ForgeError::Other(
            "a language server needs a command and file extensions".into(),
        ));
    }
    lsp.register_server(config);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_paths_are_relative_with_forward_slashes() {
        let root = Path::new("/p/shop");
        assert_eq!(
            to_ui_path(root, Path::new("/p/shop/app/Models/User.php")),
            "app/Models/User.php"
        );
        assert_eq!(to_ui_path(root, Path::new("/stubs/a.php")), "/stubs/a.php");
    }

    #[test]
    fn registered_servers_replace_by_id() {
        let state = LspState::default();
        let cfg = |cmd: &str| LspServerConfig {
            id: "x".into(),
            name: None,
            language_ids: vec![],
            file_extensions: vec!["x".into()],
            command: cmd.into(),
            args: vec![],
            initialization_options: Value::Null,
            settings: Value::Null,
        };
        state.register_server(cfg("a"));
        state.register_server(cfg("b"));
        let registered = state.registered.lock().unwrap();
        assert_eq!(registered.len(), 1);
        assert_eq!(registered[0].command, "b");
    }
}
