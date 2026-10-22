//! Language server IPC. Paths are workspace-relative like the `fs_*` commands; positions
//! are LSP positions (0-based line, UTF-16 character).
//!
//! Events:
//! - `lsp://diagnostics` `{ path, diagnostics: EditorDiagnostic[] }`
//! - `lsp://status` `ServerStatus[]`
//! - `lsp://apply-edit` `{ label?, edit: WorkspaceEdit }` (a server applies an edit itself)

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use forge_core::{ForgeError, Result};
use forge_lsp::forge_codeintel::inlay::{self, EditorInlayHint, InlayFilter, RawInlayHint};
use forge_lsp::lsp_types::{
    InlayHint, InlayHintKind, InlayHintLabel, InlayHintTooltip, Position, TextEdit,
};
use forge_lsp::model::{
    EditorCompletionItem, EditorCompletionList, EditorDiagnostic, EditorHover, EditorLocation,
    EditorSignatureHelp, EditorSymbol,
};
use forge_lsp::{LspEvent, LspManager, LspServerConfig, ServerStatus};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;

#[derive(Serialize, Clone)]
struct ApplyEditEvent {
    label: Option<String>,
    edit: forge_lsp::forge_codeintel::WorkspaceEdit,
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
    /// Add (or replace by id) a server config for this and future projects. A changed
    /// config restarts the running server; an identical one is a no-op.
    pub fn register_server(&self, config: LspServerConfig) {
        {
            let mut registered = self.registered.lock().expect("lsp lock");
            if registered.contains(&config) {
                return;
            }
            registered.retain(|c| c.id != config.id);
            registered.push(config.clone());
        }
        if let Some(m) = self.manager.lock().expect("lsp lock").as_ref() {
            m.register_server(config);
        }
    }

    /// Stop every server of the current project; the next request starts them again (after
    /// a remote project reconnected, or when another project with the same root opens).
    pub fn reset(&self) {
        if let Some(old) = self.manager.lock().expect("lsp lock").take() {
            tauri::async_runtime::spawn(async move { old.shutdown_all().await });
        }
    }

    /// Remove a server config and stop its server.
    pub fn unregister_server(&self, id: &str) {
        self.registered
            .lock()
            .expect("lsp lock")
            .retain(|c| c.id != id);
        if let Some(m) = self.manager.lock().expect("lsp lock").as_ref() {
            m.unregister_server(id);
        }
    }

    /// The manager for the open workspace, (re)created when the project changes. Servers
    /// start where the project lives (on the remote host for remote projects).
    pub(crate) fn manager(&self, app: &AppHandle, ws: &AppState) -> Result<Arc<LspManager>> {
        let backend = ws.backend()?;
        let root = backend.info().root.clone();
        let mut current = self.manager.lock().expect("lsp lock");
        if let Some(m) = current.as_ref().filter(|m| m.root() == root) {
            return Ok(m.clone());
        }
        if let Some(old) = current.take() {
            tauri::async_runtime::spawn(async move { old.shutdown_all().await });
        }
        let manager = Arc::new(LspManager::with_launcher(root, backend.lsp_launcher()));
        // Language packs register their servers (PHP: `commands::php::sync_lsp_server`).
        for config in self.registered.lock().expect("lsp lock").iter().cloned() {
            manager.register_server(config);
        }
        // Catalog servers are planned where the project is: a remote host scans and probes
        // in the agent, so it is asked in the background.
        if backend.info().remote.is_none() {
            crate::commands::langservers::register_enabled(&manager, ws);
        } else {
            let (app, manager) = (app.clone(), manager.clone());
            tauri::async_runtime::spawn_blocking(move || {
                use tauri::Manager;
                crate::commands::langservers::register_enabled(&manager, &app.state::<AppState>());
                let _ = tauri::Emitter::emit(&app, "langservers://changed", ());
            });
        }
        forward_events(app.clone(), &manager);
        *current = Some(manager.clone());
        Ok(manager)
    }
}

/// Workspace-relative, `/`-separated path for the UI (absolute if outside the project).
pub(crate) fn to_ui_path(root: &Path, abs: &Path) -> String {
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
                    server_id,
                    uri,
                    diagnostics,
                    ..
                } => {
                    let Some(abs) = forge_lsp::uri_to_path(&uri) else {
                        continue;
                    };
                    // Several servers may report on one file; send all of them.
                    let merged = manager.merge_diagnostics(&server_id, &uri, diagnostics);
                    let _ = app.emit(
                        "lsp://diagnostics",
                        DiagnosticsEvent {
                            path: to_ui_path(manager.root(), &abs),
                            diagnostics: merged.iter().map(EditorDiagnostic::from).collect(),
                        },
                    );
                }
                LspEvent::State { .. } => {
                    let _ = app.emit("lsp://status", manager.statuses());
                }
                LspEvent::Log { .. } => {}
                LspEvent::ApplyEdit { label, edit, .. } => {
                    let mut edit = forge_lsp::model::workspace_edit(&edit);
                    edit.map_paths(|p| to_ui_path(manager.root(), Path::new(p)));
                    let _ = app.emit("lsp://apply-edit", ApplyEditEvent { label, edit });
                }
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
        let abs = ws.backend()?.resolve(path)?;
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

/// Reformat Code through the file's server. `language_id` (the file type) selects the code
/// style options the server gets in its settings (`typescript.format`, `Lua.format`, …).
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn lsp_format(
    path: String,
    tab_size: Option<u32>,
    insert_spaces: Option<bool>,
    language_id: Option<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Vec<TextEdit>> {
    let patch = language_id
        .as_deref()
        .and_then(|id| forge_codestyle::model().language_for_file_type(id))
        .map(|l| {
            forge_codestyle::lsp::settings_patch(
                &l.id,
                &crate::commands::code_style::code_style_values(&ws),
            )
        })
        .filter(|p| !p.is_null());
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    c.manager
        .format(
            &c.abs,
            tab_size.unwrap_or(4),
            insert_spaces.unwrap_or(true),
            patch.as_ref(),
        )
        .await
}

/// Inlay hints of lines `start_line..=end_line` (Settings | Editor | Inlay Hints): asked
/// from the file's servers, grouped and filtered by the settings in
/// `forge_codeintel::inlay`. `text` is the editor's document, `language_id` its file type.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn lsp_inlay_hints(
    path: String,
    start_line: u32,
    end_line: u32,
    text: String,
    language_id: Option<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Vec<EditorInlayHint>> {
    let filter = {
        let settings = ws.read_settings();
        InlayFilter::from_settings(language_id.as_deref(), |k| settings.get_bool(k))
    };
    if !filter.language_enabled || !(filter.parameter_names || filter.types || filter.return_types)
    {
        return Ok(Vec::new());
    }
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let range = forge_lsp::lsp_types::Range::new(
        Position::new(start_line, 0),
        Position::new(end_line.saturating_add(1), 0),
    );
    let raw = c
        .manager
        .inlay_hints(&c.abs, range)
        .await
        .into_iter()
        .map(raw_inlay_hint)
        .collect();
    Ok(inlay::editor_hints(raw, &text, &filter))
}

fn raw_inlay_hint(h: InlayHint) -> RawInlayHint {
    RawInlayHint {
        line: h.position.line,
        character: h.position.character,
        label: match h.label {
            InlayHintLabel::String(s) => s,
            InlayHintLabel::LabelParts(parts) => parts.into_iter().map(|p| p.value).collect(),
        },
        kind: h.kind.map(|k| if k == InlayHintKind::TYPE { 1 } else { 2 }),
        padding_left: h.padding_left.unwrap_or(false),
        padding_right: h.padding_right.unwrap_or(false),
        tooltip: h.tooltip.map(|t| match t {
            InlayHintTooltip::String(s) => s,
            InlayHintTooltip::MarkupContent(m) => m.value,
        }),
    }
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
        state.register_server(cfg("b"));
        {
            let registered = state.registered.lock().unwrap();
            assert_eq!(registered.len(), 1);
            assert_eq!(registered[0].command, "b");
        }
        state.unregister_server("x");
        assert!(state.registered.lock().unwrap().is_empty());
    }
}
