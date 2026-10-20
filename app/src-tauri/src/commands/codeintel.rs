//! Code intelligence actions on top of the language servers and the PHP index: Find Usages
//! (Alt+F7) / Show Usages, Rename (Shift+F6), Show Context Actions (Alt+Enter), Go to
//! Implementation / Type Declaration / Super Method, and applying workspace edits to the
//! files that are not open in an editor. Paths are workspace-relative; positions are LSP
//! positions (0-based line, UTF-16 character). File access goes through the project's
//! backend, so everything works for remote projects too.
//!
//! Events: `lsp://apply-edit` `{ label?, edit: WorkspaceEdit }` when a server applies an
//! edit itself (e.g. while running a code action's command).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_lsp::forge_codeintel::{
    self as codeintel, order_actions, preview_workspace_edit, ActionKind, AppliedFile,
    ContextAction, FilePreview, FileStore, GroupRequest, Position, Range, UsageLocation, UsageTree,
    WorkspaceEdit,
};
use forge_lsp::model::{EditorCodeAction, EditorLocation};
use forge_lsp::{lsp_types, Feature, LspManager};
use phpforge_lang_php::navigate::{NavKind, NavTarget};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};

use crate::commands::lsp::{to_ui_path, LspState};
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Why an action found nothing: no server of the file supports it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingFeature {
    pub feature: String,
    /// "Rename requires Intelephense Premium (licence key in Settings | PHP)"
    pub message: String,
    /// The server is Intelephense without a licence.
    pub premium: bool,
}

fn missing_feature(feature: Feature, servers: &[String]) -> MissingFeature {
    let premium = servers
        .iter()
        .any(|s| s.to_ascii_lowercase().contains("intelephense"));
    let message = if premium {
        format!(
            "{} requires Intelephense Premium (licence key in Settings | PHP)",
            feature.title()
        )
    } else {
        format!(
            "{} is not supported by {}",
            feature.title(),
            servers.join(", ")
        )
    };
    MissingFeature {
        feature: feature.title().into(),
        message,
        premium,
    }
}

fn is_php(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    (p.ends_with(".php") || p.ends_with(".phtml") || p.ends_with(".inc"))
        && !p.ends_with(".blade.php")
}

/// Reads and writes project files through the backend (local folder or remote host).
struct BackendStore(Arc<dyn Backend>);

impl FileStore for BackendStore {
    fn read(&mut self, path: &str) -> std::result::Result<Option<String>, String> {
        self.0.read_optional(path).map_err(|e| e.to_string())
    }
    fn write(&mut self, path: &str, text: &str) -> std::result::Result<(), String> {
        self.0.write_file(path, text).map_err(|e| e.to_string())
    }
    fn delete(&mut self, path: &str) -> std::result::Result<(), String> {
        self.0.delete(path).map_err(|e| e.to_string())
    }
}

/// Texts of project files: editor buffers first, then the backend.
struct Texts {
    backend: Arc<dyn Backend>,
    buffers: HashMap<String, String>,
}

impl Texts {
    fn load(&mut self, path: &str) {
        if !self.buffers.contains_key(path) {
            if let Ok(Some(t)) = self.backend.read_optional(path) {
                self.buffers.insert(path.to_string(), t);
            }
        }
    }
    fn get(&self, path: &str) -> Option<&str> {
        self.buffers.get(path).map(String::as_str)
    }
}

struct Ctx {
    manager: Arc<LspManager>,
    backend: Arc<dyn Backend>,
    abs: std::path::PathBuf,
}

impl Ctx {
    fn new(path: &str, app: &AppHandle, lsp: &LspState, ws: &AppState) -> Result<Self> {
        let manager = lsp.manager(app, ws)?;
        let backend = ws.backend()?;
        let abs = backend.resolve(path)?;
        Ok(Ctx {
            manager,
            backend,
            abs,
        })
    }

    fn ui_path(&self, abs: &str) -> String {
        to_ui_path(self.manager.root(), Path::new(abs))
    }

    fn relative_edit(&self, mut edit: WorkspaceEdit) -> WorkspaceEdit {
        let root = self.manager.root().to_path_buf();
        edit.map_paths(|p| to_ui_path(&root, Path::new(p)));
        edit
    }

    fn relative_locations(&self, locs: Vec<EditorLocation>) -> Vec<UsageLocation> {
        locs.into_iter()
            .map(|l| UsageLocation {
                path: self.ui_path(&l.path),
                range: lsp_range(l.range),
                kind: None,
            })
            .collect()
    }
}

fn lsp_range(r: lsp_types::Range) -> Range {
    Range::new(
        Position::new(r.start.line, r.start.character),
        Position::new(r.end.line, r.end.character),
    )
}

/// The caret's text: the editor buffer, else the file.
fn current_text(backend: &Arc<dyn Backend>, path: &str, content: &Option<String>) -> String {
    content
        .clone()
        .or_else(|| backend.read_optional(path).ok().flatten())
        .unwrap_or_default()
}

// ---- Find Usages ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsagesAnswer {
    /// `User`, `User::orders()`, or the word at the caret.
    pub title: String,
    /// Where the answer came from: `lsp`, `index` or `none`.
    pub source: String,
    pub declaration: Option<UsageLocation>,
    pub by_type: UsageTree,
    pub by_file: UsageTree,
}

/// Find Usages / Show Usages: references from the language servers, from the PHP index
/// when they have none, grouped by usage type and by file. `buffers` are unsaved editor
/// contents by path.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn code_find_usages(
    path: String,
    line: u32,
    character: u32,
    content: Option<String>,
    buffers: Option<HashMap<String, String>>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<UsagesAnswer> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let pos = lsp_types::Position::new(line, character);
    let refs = c.manager.references(&c.abs, pos).await.unwrap_or_default();
    let mut declaration = None;
    let (mut usages, mut source) = (c.relative_locations(refs), "lsp");
    if !usages.is_empty() {
        declaration = c
            .manager
            .definition(&c.abs, pos)
            .await
            .ok()
            .and_then(|d| c.relative_locations(d).into_iter().next());
    }
    let backend = c.backend.clone();
    blocking(move || {
        let text = current_text(&backend, &path, &content);
        let mut title = codeintel::word_range(&text, Position::new(line, character))
            .map(|(_, w)| w)
            .unwrap_or_default();
        if usages.is_empty() && is_php(&path) {
            let offset = codeintel::utf16_offset(&text, Position::new(line, character));
            if let Some(found) = backend.php().usages(&path, offset, Some(text.clone()))? {
                title = found.title;
                declaration = found.declaration;
                usages = found.usages;
                source = "index";
            }
        }
        if usages.is_empty() {
            source = "none";
        }
        let mut buffers = buffers.unwrap_or_default();
        buffers.insert(path.clone(), text);
        let mut texts = Texts { backend, buffers };
        for u in &usages {
            texts.load(&u.path);
        }
        let mut req = GroupRequest {
            usages,
            declaration: declaration.clone(),
            group_by_type: true,
        };
        let by_type = codeintel::group_usages(&req, |p| texts.get(p));
        req.group_by_type = false;
        let by_file = codeintel::group_usages(&req, |p| texts.get(p));
        Ok(UsagesAnswer {
            title,
            source: source.into(),
            declaration,
            by_type,
            by_file,
        })
    })
    .await
}

// ---- Navigate: Implementation, Type Declaration, Super Method ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigateAnswer {
    pub targets: Vec<NavTarget>,
    /// Set when the language server lacks the feature (and the index found nothing).
    pub missing: Option<MissingFeature>,
}

/// Go to Implementation (`implementation`), Type Declaration (`typeDeclaration`) and Super
/// Method (`superMethod`, PHP index only): the language servers first, then the PHP index.
#[tauri::command]
pub async fn code_navigate(
    path: String,
    line: u32,
    character: u32,
    kind: NavKind,
    content: Option<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<NavigateAnswer> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let pos = lsp_types::Position::new(line, character);
    let feature = match kind {
        NavKind::Implementation => Some(Feature::Implementation),
        NavKind::TypeDeclaration => Some(Feature::TypeDefinition),
        NavKind::SuperMethod => None,
    };
    let mut targets: Vec<NavTarget> = Vec::new();
    let mut missing = None;
    if let Some(feature) = feature {
        let locs = match feature {
            Feature::Implementation => c.manager.implementation(&c.abs, pos).await,
            _ => c.manager.type_definition(&c.abs, pos).await,
        }
        .unwrap_or_default();
        targets = c
            .relative_locations(locs)
            .into_iter()
            .map(|l| NavTarget {
                path: l.path,
                line: l.range.start.line,
                column: l.range.start.character,
                name: String::new(),
                container: None,
            })
            .collect();
        missing = c
            .manager
            .missing_feature(&c.abs, feature)
            .await
            .map(|servers| missing_feature(feature, &servers));
    }
    let backend = c.backend.clone();
    blocking(move || {
        if targets.is_empty() && is_php(&path) {
            let text = current_text(&backend, &path, &content);
            let offset = codeintel::utf16_offset(&text, Position::new(line, character));
            targets = backend.php().navigate(&path, offset, kind, Some(text))?;
        }
        Ok(NavigateAnswer {
            missing: if targets.is_empty() { missing } else { None },
            targets,
        })
    })
    .await
}

// ---- Rename ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareRenameAnswer {
    /// What to rename: the server's range, else the word at the caret.
    pub range: Option<Range>,
    pub placeholder: Option<String>,
    /// Nothing can be renamed here.
    pub invalid: bool,
    pub missing: Option<MissingFeature>,
}

#[tauri::command]
pub async fn code_prepare_rename(
    path: String,
    line: u32,
    character: u32,
    content: Option<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<PrepareRenameAnswer> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let text = current_text(&c.backend, &path, &content);
    let word = codeintel::word_range(&text, Position::new(line, character));
    if let Some(servers) = c.manager.missing_feature(&c.abs, Feature::Rename).await {
        return Ok(PrepareRenameAnswer {
            range: word.as_ref().map(|w| w.0),
            placeholder: word.map(|w| w.1),
            invalid: false,
            missing: Some(missing_feature(Feature::Rename, &servers)),
        });
    }
    if c.manager.clients_for(&c.abs).await.is_empty() {
        return Err(ForgeError::Other(
            "Rename needs a language server for this file".into(),
        ));
    }
    let prepared = c
        .manager
        .prepare_rename(&c.abs, lsp_types::Position::new(line, character))
        .await?;
    Ok(match prepared {
        None => PrepareRenameAnswer {
            range: None,
            placeholder: None,
            invalid: true,
            missing: None,
        },
        Some(p) => {
            let range = p.range.map(lsp_range).or(word.as_ref().map(|w| w.0));
            let placeholder = p.placeholder.or_else(|| {
                range.and_then(|r| {
                    let line = text.split('\n').nth(r.start.line as usize)?;
                    let s = codeintel::utf16_offset(line, Position::new(0, r.start.character));
                    let e = codeintel::utf16_offset(line, Position::new(0, r.end.character));
                    let units: Vec<u16> = line.encode_utf16().collect();
                    String::from_utf16(units.get(s..e)?).ok()
                })
            });
            PrepareRenameAnswer {
                invalid: range.is_none(),
                range,
                placeholder,
                missing: None,
            }
        }
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameAnswer {
    pub edit: WorkspaceEdit,
    /// Affected lines per file, for the Refactoring Preview.
    pub preview: Vec<FilePreview>,
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn code_rename(
    path: String,
    line: u32,
    character: u32,
    new_name: String,
    buffers: Option<HashMap<String, String>>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<RenameAnswer> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let edit = c
        .manager
        .rename(&c.abs, lsp_types::Position::new(line, character), &new_name)
        .await?
        .ok_or_else(|| ForgeError::Other("Nothing to rename here".into()))?;
    let edit = c.relative_edit(edit);
    let backend = c.backend.clone();
    blocking(move || {
        let mut texts = Texts {
            backend,
            buffers: buffers.unwrap_or_default(),
        };
        let preview = preview_workspace_edit(&edit, |p| {
            texts.load(p);
            texts.get(p).map(str::to_string)
        });
        Ok(RenameAnswer { edit, preview })
    })
    .await
}

// ---- Workspace edits ----

/// Apply `edit` to the files through the backend, except the text edits of `skip` (files
/// open in an editor, which the UI edits itself so they stay undoable there). All or
/// nothing; the answer is what `code_revert_edit` needs to undo it.
#[tauri::command]
pub async fn code_apply_edit(
    edit: WorkspaceEdit,
    skip: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<Vec<AppliedFile>> {
    let backend = state.backend()?;
    blocking(move || {
        let edit = edit.without_text_edits_of(&skip.unwrap_or_default());
        codeintel::apply_workspace_edit(&edit, &mut BackendStore(backend))
            .map_err(|e| ForgeError::Other(e.to_string()))
    })
    .await
}

/// Undo [`code_apply_edit`]; refused when a file changed since.
#[tauri::command]
pub async fn code_revert_edit(files: Vec<AppliedFile>, state: State<'_, AppState>) -> Result<()> {
    let backend = state.backend()?;
    blocking(move || {
        codeintel::revert_applied(&files, &mut BackendStore(backend))
            .map_err(|e| ForgeError::Other(e.to_string()))
    })
    .await
}

// ---- Show Context Actions ----

fn from_lsp_action(a: EditorCodeAction, c: &Ctx) -> ContextAction {
    ContextAction {
        kind: ActionKind::of_lsp(a.kind.as_deref()),
        title: a.title,
        source: "lsp".into(),
        is_preferred: a.is_preferred,
        disabled: a.disabled,
        edit: a.edit.map(|e| c.relative_edit(e)),
        command: a.command.and_then(|cmd| serde_json::to_value(cmd).ok()),
        needs_resolve: a.needs_resolve,
        server: Some(a.server),
        raw: Some(a.raw),
        action: None,
    }
}

/// The Alt+Enter popup: the language servers' quick fixes and refactorings for the caret
/// (or selection) and, in PHP files, Forge's own intentions, in PHPStorm's order.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn code_context_actions(
    path: String,
    line: u32,
    character: u32,
    end_line: Option<u32>,
    end_character: Option<u32>,
    content: Option<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Vec<ContextAction>> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let start = lsp_types::Position::new(line, character);
    let end =
        lsp_types::Position::new(end_line.unwrap_or(line), end_character.unwrap_or(character));
    let lsp_actions = c
        .manager
        .code_actions(&c.abs, lsp_types::Range::new(start, end))
        .await
        .unwrap_or_default();
    let mut actions: Vec<ContextAction> = lsp_actions
        .into_iter()
        .map(|a| from_lsp_action(a, &c))
        .collect();
    let backend = c.backend.clone();
    blocking(move || {
        if is_php(&path) {
            let text = current_text(&backend, &path, &content);
            let offset = codeintel::utf16_offset(&text, Position::new(line, character));
            for i in backend
                .php()
                .intentions(&path, offset, Some(text.clone()))?
            {
                let mut edit = WorkspaceEdit::default();
                if !i.edits.is_empty() {
                    edit.add_edits(
                        path.clone(),
                        i.edits
                            .iter()
                            .map(|e| codeintel::TextEdit {
                                range: Range::new(
                                    codeintel::position_at(&text, e.start),
                                    codeintel::position_at(&text, e.end),
                                ),
                                new_text: e.new_text.clone(),
                            })
                            .collect(),
                    );
                }
                actions.push(ContextAction {
                    title: i.title,
                    kind: match i.kind {
                        phpforge_lang_php::intentions::IntentionKind::QuickFix => {
                            ActionKind::QuickFix
                        }
                        phpforge_lang_php::intentions::IntentionKind::Intention => {
                            ActionKind::Intention
                        }
                    },
                    source: "php".into(),
                    is_preferred: false,
                    disabled: None,
                    edit: (!edit.is_empty()).then_some(edit),
                    command: None,
                    needs_resolve: false,
                    server: None,
                    raw: None,
                    action: i.action,
                });
            }
        }
        Ok(order_actions(actions))
    })
    .await
}

/// Fill in a code action's edit (`codeAction/resolve`).
#[tauri::command]
pub async fn code_resolve_action(
    path: String,
    server: String,
    raw: Value,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<ContextAction> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let resolved = c.manager.resolve_code_action(&c.abs, &server, raw).await?;
    Ok(from_lsp_action(resolved, &c))
}

/// Run a code action's server command; edits it makes arrive as `lsp://apply-edit`.
#[tauri::command]
pub async fn code_execute_command(
    path: String,
    server: String,
    command: String,
    arguments: Option<Vec<Value>>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Option<Value>> {
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    c.manager
        .execute_command(&c.abs, &server, &command, arguments.unwrap_or_default())
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_features_mention_intelephense_premium() {
        let m = missing_feature(Feature::Rename, &["Intelephense".into()]);
        assert!(m.premium);
        assert_eq!(
            m.message,
            "Rename requires Intelephense Premium (licence key in Settings | PHP)"
        );
        let m = missing_feature(Feature::Implementation, &["phpactor".into()]);
        assert!(!m.premium);
        assert_eq!(
            m.message,
            "Go to Implementation is not supported by phpactor"
        );
    }

    #[test]
    fn php_files_are_recognized() {
        assert!(is_php("app/Models/User.php"));
        assert!(!is_php("resources/views/a.blade.php"));
        assert!(!is_php("src/app.ts"));
    }
}
