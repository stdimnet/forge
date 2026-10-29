//! Optimize Imports (Ctrl+Alt+O) and Auto Import (Settings | Editor | General | Auto
//! Import). PHP is Forge's own (`phpforge_lang_php::imports`): the import style comes from
//! the code style scheme and the Auto Import settings, unresolved class names and their
//! candidates from the backend's PHP index (so remote projects work) plus PHP's own classes
//! from the stubs. JavaScript, TypeScript and Vue use the TypeScript server's source
//! actions (`source.organizeImports`, `source.addMissingImports`,
//! `source.removeUnusedImports`). Offsets are UTF-16, positions LSP positions.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use forge_backend::Backend;
use forge_core::{FileKind, ForgeError, Result};
use forge_lsp::forge_codeintel::imports::{self as shared, OptimizeAnswer};
use forge_lsp::forge_codeintel::{
    self as codeintel, apply_text_edits, DocumentChange, Position, Range, TextEdit as LspEdit,
    WorkspaceEdit,
};
use forge_lsp::LspManager;
use phpforge_lang_php::generate::TextEdit;
use phpforge_lang_php::imports::auto::{self, OnTheFlyAnswer, OnTheFlyRequest, UnresolvedClass};
use phpforge_lang_php::imports::{self, style as import_style_mod, ImportStyle};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::commands::codeintel::{is_php, is_web_script, Ctx};
use crate::commands::git::with_repo;
use crate::commands::library::LibraryState;
use crate::commands::lsp::LspState;
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// The PHP import style of the project: the selected code style scheme's Imports tab and
/// the PHP Auto Import settings. Also kept as the default of the refactorings.
pub(crate) fn import_style(state: &AppState) -> ImportStyle {
    let settings = crate::commands::code_style::code_style_settings(state);
    let style = ImportStyle::from_settings(|k| settings.get(k).cloned());
    import_style_mod::set_default(style);
    style
}

/// Whether a settings change can change the import style.
pub(crate) fn affects_import_style(key: &str) -> bool {
    key.starts_with("php.codeStyle.")
        || key.starts_with("php.autoImport.")
        || key.starts_with("editor.codeStyle.")
}

/// Unresolved class names of a PHP text with candidates from the index and, when the stubs
/// are loaded, PHP's own classes.
pub(crate) async fn unresolved_with_stubs(
    backend: &Arc<dyn Backend>,
    path: &str,
    text: &str,
    app: &AppHandle,
    ws: &AppState,
    lib: &LibraryState,
) -> Vec<UnresolvedClass> {
    let (b, p, t) = (backend.clone(), path.to_string(), text.to_string());
    let list = blocking(move || b.php().unresolved_classes(&p, Some(t)))
        .await
        .unwrap_or_default();
    if list.is_empty() {
        return list;
    }
    match lib.stubs(app, ws, false).await {
        Ok(stubs) => UnresolvedClass::add_global(list, |name| stubs.class(name).is_some()),
        Err(_) => list,
    }
}

fn whole(text: &str) -> Range {
    Range::new(
        Position::new(0, 0),
        codeintel::position_at(text, text.encode_utf16().count()),
    )
}

/// The edits of a workspace edit for the file `abs` (absolute, as servers answer).
fn edits_for(edit: &WorkspaceEdit, abs: &str) -> Vec<LspEdit> {
    edit.changes
        .iter()
        .filter_map(|c| match c {
            DocumentChange::Edit { path, edits } if path == abs => Some(edits.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// `source.*` action of the TypeScript server for the whole file: its edit (resolved),
/// absolute paths. `None` when no server offers it.
async fn source_action_edit(
    manager: &LspManager,
    abs: &std::path::Path,
    text: &str,
    kind: &str,
) -> Result<Option<WorkspaceEdit>> {
    let actions = manager.source_actions(abs, whole_lsp(text), kind).await;
    let Some(action) = actions.into_iter().next() else {
        return Ok(None);
    };
    let action = if action.edit.is_none() && action.needs_resolve {
        manager
            .resolve_code_action(abs, &action.server, action.raw.clone())
            .await?
    } else {
        action
    };
    if let Some(edit) = action.edit {
        return Ok(Some(edit));
    }
    // Servers that only offer a command apply the edit themselves (`lsp://apply-edit`).
    if let Some(cmd) = action.command {
        manager
            .execute_command(abs, &action.server, &cmd.command, cmd.arguments)
            .await?;
    }
    Ok(Some(WorkspaceEdit::default()))
}

fn whole_lsp(text: &str) -> forge_lsp::lsp_types::Range {
    let r = whole(text);
    forge_lsp::lsp_types::Range::new(
        forge_lsp::lsp_types::Position::new(r.start.line, r.start.character),
        forge_lsp::lsp_types::Position::new(r.end.line, r.end.character),
    )
}

/// Organize the imports of a JavaScript / TypeScript / Vue text with the TypeScript
/// server: the new text (`None`: unchanged or applied by the server itself) and the message.
async fn organize_web(
    manager: &LspManager,
    abs: &std::path::Path,
    text: &str,
) -> Result<Option<(Option<String>, String)>> {
    let Some(edit) = source_action_edit(manager, abs, text, "source.organizeImports").await? else {
        return Ok(None);
    };
    let edits = edits_for(&edit, &abs.to_string_lossy());
    if edits.is_empty() {
        let message = if edit.is_empty() && edit.changes.is_empty() {
            shared::summary(0, false, 0)
        } else {
            "Optimized imports".into()
        };
        return Ok(Some((None, message)));
    }
    let new = apply_text_edits(text, &edits)
        .ok_or_else(|| ForgeError::Other("the language server sent overlapping edits".into()))?;
    let (removed, rearranged) = webforge_lang_web::imports::organize_report(text, &new);
    let changed = (new != text).then_some(new);
    Ok(Some((changed, shared::summary(removed, rearranged, 0))))
}

/// Optimize Imports of the file in the editor (`content`): PHP with the code style, web
/// scripts through the TypeScript server.
#[tauri::command]
pub async fn imports_optimize(
    path: String,
    content: String,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<OptimizeAnswer> {
    if is_php(&path) {
        let style = import_style(&ws);
        return blocking(move || {
            let report = imports::optimize_imports_with(&content, &style);
            Ok(OptimizeAnswer::new(
                &content,
                report.text.as_deref(),
                report.message(),
            ))
        })
        .await;
    }
    if !is_web_script(&path) {
        return Err(ForgeError::Other(
            "Optimize Imports is not available for this file type".into(),
        ));
    }
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    match organize_web(&c.manager, &c.abs, &content).await? {
        Some((new, message)) => Ok(OptimizeAnswer::new(&content, new.as_deref(), message)),
        None => Err(ForgeError::Other(
            "No language server offers Optimize Imports for this file".into(),
        )),
    }
}

/// Optimize Imports of folders and files from the Project view.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderOptimizeAnswer {
    /// Edits of the changed files (workspace-relative), applied as one undoable step.
    pub edit: WorkspaceEdit,
    /// Files checked.
    pub files: usize,
    /// Files changed.
    pub changed: usize,
    pub message: String,
}

/// Files below `path` (directly in it without `recursive`), or `path` itself.
fn collect_files(backend: &Arc<dyn Backend>, path: &str, recursive: bool, out: &mut Vec<String>) {
    let parent = path.rsplit_once('/').map_or("", |(d, _)| d);
    let kind = if path.is_empty() {
        Some(FileKind::Dir)
    } else {
        backend
            .list_dir(parent)
            .ok()
            .and_then(|l| l.into_iter().find(|e| e.path == path).map(|e| e.kind))
    };
    match kind {
        Some(FileKind::File) => out.push(path.to_string()),
        Some(FileKind::Dir) => {
            for e in backend.list_dir(path).unwrap_or_default() {
                match e.kind {
                    FileKind::File => out.push(e.path),
                    FileKind::Dir if recursive => collect_files(backend, &e.path, true, out),
                    FileKind::Dir => {}
                }
            }
        }
        None => {}
    }
}

/// Optimize Imports on folders and files of the Project view (PHPStorm's "Optimize
/// Imports" dialog: "Include subdirectories", "Only VCS changed files"). `buffers` are the
/// texts of open editors, which win over the files.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn imports_optimize_paths(
    paths: Vec<String>,
    include_subdirectories: bool,
    only_vcs_changed: bool,
    buffers: Option<HashMap<String, String>>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<FolderOptimizeAnswer> {
    let backend = ws.backend()?;
    let changed: Option<BTreeSet<String>> = if only_vcs_changed {
        let status = with_repo(&ws, |r| r.status()).await?;
        Some(
            status
                .entries
                .into_iter()
                .filter(|e| {
                    !matches!(e.worktree, forge_vcs::FileState::Deleted)
                        && !matches!(e.index, forge_vcs::FileState::Deleted)
                })
                .map(|e| e.path)
                .collect(),
        )
    } else {
        None
    };
    let b = backend.clone();
    let files = blocking(move || {
        let mut all = Vec::new();
        for p in &paths {
            collect_files(&b, p, include_subdirectories, &mut all);
        }
        Ok(shared::optimize_targets(&all, changed.as_ref()))
    })
    .await?;
    let buffers = buffers.unwrap_or_default();
    let style = import_style(&ws);
    let mut answer = FolderOptimizeAnswer {
        files: files.len(),
        ..Default::default()
    };
    let mut removed = 0usize;
    let mut manager: Option<Arc<LspManager>> = None;
    for path in files {
        let open = buffers.get(&path).cloned();
        let text = match &open {
            Some(t) => t.clone(),
            None => {
                let (b, p) = (backend.clone(), path.clone());
                match blocking(move || b.read_file(&p)).await {
                    Ok(t) => t,
                    Err(_) => continue,
                }
            }
        };
        let new = if is_php(&path) {
            let t = text.clone();
            let report = blocking(move || Ok(imports::optimize_imports_with(&t, &style))).await?;
            removed += report.removed;
            report.text
        } else {
            let m = match &manager {
                Some(m) => m.clone(),
                None => {
                    let m = lsp.manager(&app, &ws)?;
                    manager = Some(m.clone());
                    m
                }
            };
            let abs = backend.resolve(&path)?;
            // Files not open in an editor are opened in the server for the request.
            if open.is_none() {
                m.did_open(&abs, &text).await.ok();
            }
            let result = organize_web(&m, &abs, &text).await;
            if open.is_none() {
                m.did_close(&abs).ok();
            }
            match result {
                Ok(Some((new, _))) => {
                    if let Some(n) = &new {
                        removed += webforge_lang_web::imports::organize_report(&text, n).0;
                    }
                    new
                }
                _ => None,
            }
        };
        if let Some(edit) = new.and_then(|n| shared::minimal_edit(&text, &n)) {
            answer.edit.add_edits(path, vec![edit]);
            answer.changed += 1;
        }
    }
    answer.message = shared::folder_summary(answer.files, answer.changed, removed);
    Ok(answer)
}

/// Auto Import on the fly in a PHP editor, after the user paused typing.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn php_imports_on_the_fly(
    path: String,
    content: String,
    request: OnTheFlyRequest,
    app: AppHandle,
    ws: State<'_, AppState>,
    lib: State<'_, LibraryState>,
) -> Result<OnTheFlyAnswer> {
    if !is_php(&path) || !(request.add || request.optimize) {
        return Ok(OnTheFlyAnswer::default());
    }
    let style = import_style(&ws);
    let backend = ws.backend()?;
    let list = if request.add {
        unresolved_with_stubs(&backend, &path, &content, &app, &ws, &lib).await
    } else {
        vec![]
    };
    blocking(move || Ok(auto::on_the_fly(&content, &list, &request, &style))).await
}

/// The class the import popup ("? App\Models\User Alt+Enter") offers at `offset`.
#[tauri::command]
pub async fn php_import_hint(
    path: String,
    content: String,
    offset: usize,
    app: AppHandle,
    ws: State<'_, AppState>,
    lib: State<'_, LibraryState>,
) -> Result<Option<UnresolvedClass>> {
    if !is_php(&path) {
        return Ok(None);
    }
    let backend = ws.backend()?;
    let list = unresolved_with_stubs(&backend, &path, &content, &app, &ws, &lib).await;
    Ok(auto::import_hint(&content, &list, offset))
}

/// The edits referring to `fqn` at `start..end` (the chosen class of the "Class to Import"
/// chooser, or a completed class name): an import or the fully qualified name, as the
/// import style says. `auto` for completion (the auto-import scopes apply).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn php_import_reference(
    content: String,
    start: usize,
    end: usize,
    fqn: String,
    doc: Option<bool>,
    auto: Option<bool>,
    ws: State<'_, AppState>,
) -> Result<Vec<TextEdit>> {
    let style = import_style(&ws);
    blocking(move || {
        Ok(auto::reference_edits(
            &content,
            start,
            end,
            &fqn,
            doc.unwrap_or(false),
            &style,
            auto.unwrap_or(false),
        ))
    })
    .await
}

/// Auto Import on the fly in JavaScript / TypeScript / Vue: the TypeScript server's
/// "remove unused imports" (`optimize`) or "add missing imports" (`add`) edit, when it
/// stays away from the caret and the lines edited last.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn code_imports_on_the_fly(
    path: String,
    content: String,
    add: bool,
    optimize: bool,
    caret: Position,
    edited: Option<Range>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
) -> Result<Option<WorkspaceEdit>> {
    if !is_web_script(&path) {
        return Ok(None);
    }
    let c = Ctx::new(&path, &app, &lsp, &ws)?;
    let abs = c.abs.to_string_lossy().into_owned();
    let kinds = [
        (optimize, "source.removeUnusedImports"),
        (add, "source.addMissingImports"),
    ];
    for (on, kind) in kinds {
        if !on {
            continue;
        }
        let Some(edit) = source_action_edit_quiet(&c.manager, &c.abs, &content, kind).await else {
            continue;
        };
        let edits = edits_for(&edit, &abs);
        if edits.is_empty() || !shared::on_the_fly_allowed(&edits, caret, edited) {
            continue;
        }
        let mut out = WorkspaceEdit::default();
        out.add_edits(abs.clone(), edits);
        return Ok(Some(c.relative_edit(out)));
    }
    Ok(None)
}

/// Like [`source_action_edit`] but never runs a server command (on the fly only applies
/// edits it can check first).
async fn source_action_edit_quiet(
    manager: &LspManager,
    abs: &std::path::Path,
    text: &str,
    kind: &str,
) -> Option<WorkspaceEdit> {
    let action = manager
        .source_actions(abs, whole_lsp(text), kind)
        .await
        .into_iter()
        .next()?;
    if action.edit.is_some() {
        return action.edit;
    }
    if action.needs_resolve {
        return manager
            .resolve_code_action(abs, &action.server, action.raw)
            .await
            .ok()?
            .edit;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert!(affects_import_style("php.codeStyle.imports.sort"));
        assert!(affects_import_style("php.autoImport.fileScope"));
        assert!(!affects_import_style("editor.fontSize"));
        assert_eq!(whole("a\nbc").end, Position::new(1, 2));
    }
}
