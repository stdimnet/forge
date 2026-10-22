//! The Refactor menu's Extract / Introduce and Inline refactorings for PHP
//! (`phpforge_lang_php::refactor`): Introduce Variable / Constant / Field / Parameter, Extract
//! Method and Inline. Rust analyses the code and computes the edit; the UI shows the choosers
//! and dialogs and applies the edit as one undoable step. Call sites and usages come from the
//! PHP index (Find Usages) and files are read through the project's backend, so remote
//! projects work too. Positions are LSP positions (0-based line, UTF-16 character).

use std::collections::HashMap;
use std::sync::Arc;

use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_lsp::forge_codeintel::{
    self as codeintel, preview_workspace_edit, FilePreview, Position, Range, RenameConflict,
    TextEdit as LspEdit, UsageOptions, WorkspaceEdit,
};
use phpforge_lang_php::generate::TextEdit;
use phpforge_lang_php::offsets::{byte_to_utf16, utf16_to_byte};
use phpforge_lang_php::refactor::extract_method::{self, ExtractMethodInfo, ExtractMethodOptions};
use phpforge_lang_php::refactor::inline::{self, InlineKind, InlineMode};
use phpforge_lang_php::refactor::introduce::{self, FieldInit, IntroduceKind, IntroduceOptions};
use phpforge_lang_php::refactor::{FileEdits, Plan, Span, UsageSpan};
use serde::Serialize;
use tauri::State;

use crate::commands::workspace::blocking;
use crate::state::AppState;

fn to_byte(text: &str, p: Position) -> usize {
    utf16_to_byte(text, codeintel::utf16_offset(text, p))
}

fn to_pos(text: &str, byte: usize) -> Position {
    codeintel::position_at(text, byte_to_utf16(text, byte))
}

fn to_range(text: &str, s: Span) -> Range {
    Range::new(to_pos(text, s.start), to_pos(text, s.end))
}

fn lsp_edits(text: &str, edits: &[TextEdit]) -> Vec<LspEdit> {
    edits
        .iter()
        .map(|e| LspEdit {
            range: Range::new(to_pos(text, e.start), to_pos(text, e.end)),
            new_text: e.new_text.clone(),
        })
        .collect()
}

fn refuse(message: String) -> ForgeError {
    ForgeError::Other(message)
}

fn require_php(path: &str) -> Result<()> {
    let p = path.to_ascii_lowercase();
    if (p.ends_with(".php") || p.ends_with(".phtml") || p.ends_with(".inc"))
        && !p.ends_with(".blade.php")
    {
        Ok(())
    } else {
        Err(refuse(
            "This refactoring needs a PHP file or a language server that offers it".into(),
        ))
    }
}

/// Project files: editor buffers first, then the backend.
struct Files {
    backend: Arc<dyn Backend>,
    buffers: HashMap<String, String>,
}

impl Files {
    fn get(&mut self, path: &str) -> Option<String> {
        if let Some(t) = self.buffers.get(path) {
            return Some(t.clone());
        }
        let t = self.backend.read_optional(path).ok().flatten()?;
        self.buffers.insert(path.to_string(), t.clone());
        Some(t)
    }
}

/// The usages of the symbol at `byte` in `path` from the PHP index, as byte spans.
fn usages_at(
    backend: &Arc<dyn Backend>,
    files: &mut Files,
    path: &str,
    text: &str,
    byte: usize,
) -> Result<Vec<UsageSpan>> {
    let Some(found) = backend.php().usages(
        path,
        byte_to_utf16(text, byte),
        UsageOptions::default(),
        Some(text.to_string()),
    )?
    else {
        return Ok(vec![]);
    };
    let mut out = Vec::new();
    for u in found.declaration.into_iter().chain(found.usages) {
        let Some(t) = files.get(&u.path) else {
            continue;
        };
        out.push(UsageSpan {
            span: Span::new(to_byte(&t, u.range.start), to_byte(&t, u.range.end)),
            path: u.path,
        });
    }
    Ok(out)
}

/// The edit of a refactoring, its preview, its problems and where the new name is.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefactorAnswer {
    pub edit: WorkspaceEdit,
    pub preview: Vec<FilePreview>,
    pub conflicts: Vec<RenameConflict>,
    /// The new name's occurrences in the current file after the edit (in-place editing).
    pub name_ranges: Vec<Range>,
    /// Usages that were not changed (kept for the Refactoring Preview's wording).
    pub skipped: usize,
}

fn answer(
    path: &str,
    text: &str,
    plan: Plan,
    other: Vec<FileEdits>,
    files: &mut Files,
) -> RefactorAnswer {
    let mut edit = WorkspaceEdit::default();
    // Edits of the current file from the plan and from usages in it.
    let mut own = plan.edits;
    let mut rest = Vec::new();
    for f in plan.other_files.into_iter().chain(other) {
        if f.path == path {
            own.extend(f.edits);
        } else {
            rest.push(f);
        }
    }
    own.sort_by_key(|e| (e.start, e.end));
    // A call site inside a replaced expression is covered by that replacement.
    let mut kept: Vec<TextEdit> = Vec::with_capacity(own.len());
    for e in own {
        if kept.last().is_some_and(|k| e.start < k.end) {
            continue;
        }
        kept.push(e);
    }
    let own = kept;
    let new_text = phpforge_lang_php::generate::apply_edits(text, &own);
    let name_ranges = plan
        .name_spans
        .iter()
        .map(|s| to_range(&new_text, *s))
        .collect();
    if !own.is_empty() {
        edit.add_edits(path.to_string(), lsp_edits(text, &own));
    }
    for f in rest {
        if let Some(t) = files.get(&f.path) {
            edit.add_edits(f.path.clone(), lsp_edits(&t, &f.edits));
        }
    }
    files.buffers.insert(path.to_string(), text.to_string());
    let preview = preview_workspace_edit(&edit, |p| files.get(p));
    RefactorAnswer {
        edit,
        preview,
        conflicts: plan.conflicts,
        name_ranges,
        skipped: 0,
    }
}

// ---- Introduce Variable / Constant / Field / Parameter ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpressionChoice {
    pub range: Range,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntroduceInfoAnswer {
    pub range: Range,
    pub text: String,
    pub occurrences: usize,
    pub names: Vec<String>,
    pub class_name: Option<String>,
    pub function_name: Option<String>,
    pub init_places: Vec<FieldInit>,
    pub is_static: bool,
    pub constant_expression: bool,
    pub global: bool,
    /// Introduce Parameter: the calls that get the new argument.
    pub call_sites: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntroducePrepareAnswer {
    /// Several expressions at the caret: PHPStorm's "Expressions" popup.
    pub expressions: Vec<ExpressionChoice>,
    pub info: Option<IntroduceInfoAnswer>,
}

/// Introduce Variable / Constant / Field / Parameter: the expressions at the caret, or the
/// selected expression with its occurrences and name suggestions. Errors are PHPStorm's
/// "Cannot perform refactoring" hints.
#[tauri::command]
pub async fn refactor_introduce_prepare(
    path: String,
    kind: IntroduceKind,
    start: Position,
    end: Position,
    content: String,
    state: State<'_, AppState>,
) -> Result<IntroducePrepareAnswer> {
    require_php(&path)?;
    let backend = state.backend()?;
    blocking(move || {
        let text = content;
        let (s, e) = (to_byte(&text, start), to_byte(&text, end));
        let prepared = introduce::prepare(&text, s, e, kind).map_err(refuse)?;
        let mut call_sites = 0;
        if let (Some(info), IntroduceKind::Parameter) = (&prepared.info, kind) {
            if let Some(call) =
                introduce::parameter_call(&text, &path, info.span.start, info.span.end, "x")
            {
                let mut files = Files {
                    backend: backend.clone(),
                    buffers: HashMap::from([(path.clone(), text.clone())]),
                };
                call_sites = usages_at(&backend, &mut files, &path, &text, call.declaration.start)
                    .unwrap_or_default()
                    .iter()
                    .filter(|u| !(u.path == path && u.span == call.declaration))
                    .count();
            }
        }
        Ok(IntroducePrepareAnswer {
            expressions: prepared
                .expressions
                .iter()
                .map(|c| ExpressionChoice {
                    range: to_range(&text, c.span),
                    text: c.text.clone(),
                })
                .collect(),
            info: prepared.info.map(|i| IntroduceInfoAnswer {
                range: to_range(&text, i.span),
                text: i.text,
                occurrences: i.occurrences,
                names: i.names,
                class_name: i.class_name,
                function_name: i.function_name,
                init_places: i.init_places,
                is_static: i.is_static,
                constant_expression: i.constant_expression,
                global: i.global,
                call_sites,
            }),
        })
    })
    .await
}

/// Introduce the variable / constant / field / parameter for the expression at
/// `start..end`. Introduce Parameter also passes the expression at every call site.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn refactor_introduce(
    path: String,
    kind: IntroduceKind,
    start: Position,
    end: Position,
    content: String,
    options: IntroduceOptions,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<RefactorAnswer> {
    require_php(&path)?;
    let backend = state.backend()?;
    let style = crate::commands::php::php_code_style(&state);
    blocking(move || {
        let text = content;
        let (s, e) = (to_byte(&text, start), to_byte(&text, end));
        let options = IntroduceOptions { style, ..options };
        let mut plan = introduce::introduce(&text, &path, s, e, kind, &options).map_err(refuse)?;
        let mut files = Files {
            backend: backend.clone(),
            buffers: buffers.unwrap_or_default(),
        };
        files.buffers.insert(path.clone(), text.clone());
        let mut other = Vec::new();
        if kind == IntroduceKind::Parameter && !options.default_value {
            if let Some(call) = introduce::parameter_call(&text, &path, s, e, &options.name) {
                let usages = usages_at(&backend, &mut files, &path, &text, call.declaration.start)?;
                let (edits, conflicts) =
                    introduce::parameter_call_edits(&call, &usages, &mut |p| files.get(p));
                plan.conflicts.extend(conflicts);
                other = edits;
            }
        }
        Ok(answer(&path, &text, plan, other, &mut files))
    })
    .await
}

// ---- Extract Method ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractMethodAnswer {
    #[serde(flatten)]
    pub info: ExtractMethodInfo,
    /// The fragment (whole statements).
    pub range: Range,
}

/// Extract Method (Ctrl+Alt+M): check the selection and describe the dialog (parameters,
/// returned variables, static, duplicates).
#[tauri::command]
pub async fn refactor_extract_method_prepare(
    path: String,
    start: Position,
    end: Position,
    content: String,
) -> Result<ExtractMethodAnswer> {
    require_php(&path)?;
    blocking(move || {
        let text = content;
        let info = extract_method::prepare(&text, to_byte(&text, start), to_byte(&text, end))
            .map_err(refuse)?;
        Ok(ExtractMethodAnswer {
            range: to_range(&text, info.span),
            info,
        })
    })
    .await
}

/// Extract the method with the dialog's choices.
#[tauri::command]
pub async fn refactor_extract_method(
    path: String,
    start: Position,
    end: Position,
    content: String,
    options: ExtractMethodOptions,
    state: State<'_, AppState>,
) -> Result<RefactorAnswer> {
    require_php(&path)?;
    let backend = state.backend()?;
    let style = crate::commands::php::php_code_style(&state);
    blocking(move || {
        let text = content;
        let options = ExtractMethodOptions { style, ..options };
        let plan = extract_method::extract_method(
            &text,
            &path,
            to_byte(&text, start),
            to_byte(&text, end),
            &options,
        )
        .map_err(refuse)?;
        let mut files = Files {
            backend,
            buffers: HashMap::new(),
        };
        Ok(answer(&path, &text, plan, vec![], &mut files))
    })
    .await
}

// ---- Inline ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineInfoAnswer {
    pub kind: InlineKind,
    /// "Inline Variable", "Inline Method", ...
    pub title: String,
    /// `$total`, `total()`, `MAX`.
    pub name: String,
    /// The caret is on a reference / call: "this reference only" is offered.
    pub on_reference: bool,
    /// References (variables, constants) or invocations (methods, functions).
    pub occurrences: usize,
    /// Declared in a library (vendor) file: only "this invocation" can be inlined.
    pub read_only: bool,
}

/// The declaration (path, text, name span) of the method / function / constant at `byte`.
fn declaration_of(
    backend: &Arc<dyn Backend>,
    files: &mut Files,
    path: &str,
    text: &str,
    byte: usize,
) -> Result<(String, String, Span, Vec<UsageSpan>)> {
    let found = backend
        .php()
        .usages(
            path,
            byte_to_utf16(text, byte),
            UsageOptions::default(),
            Some(text.to_string()),
        )?
        .ok_or_else(|| refuse("Cannot perform refactoring. Cannot find the declaration".into()))?;
    let decl = found
        .declaration
        .ok_or_else(|| refuse("Cannot perform refactoring. Cannot find the declaration".into()))?;
    let decl_text = files
        .get(&decl.path)
        .ok_or_else(|| refuse(format!("Cannot read {}", decl.path)))?;
    let span = Span::new(
        to_byte(&decl_text, decl.range.start),
        to_byte(&decl_text, decl.range.end),
    );
    let mut usages = Vec::new();
    for u in found.usages {
        let Some(t) = files.get(&u.path) else {
            continue;
        };
        usages.push(UsageSpan {
            span: Span::new(to_byte(&t, u.range.start), to_byte(&t, u.range.end)),
            path: u.path,
        });
    }
    Ok((decl.path, decl_text, span, usages))
}

fn is_library(path: &str) -> bool {
    path.starts_with("vendor/") || path.contains("://")
}

/// Inline (Ctrl+Alt+N): what is at the caret and how often it is used, for the dialog.
#[tauri::command]
pub async fn refactor_inline_prepare(
    path: String,
    position: Position,
    content: String,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<InlineInfoAnswer> {
    require_php(&path)?;
    let backend = state.backend()?;
    blocking(move || {
        let text = content;
        let at = to_byte(&text, position);
        let target = inline::target_at(&text, at).map_err(refuse)?;
        let mut info = InlineInfoAnswer {
            kind: target.kind,
            title: target.kind.title().into(),
            name: target.name.clone(),
            on_reference: target.on_reference,
            occurrences: target.occurrences,
            read_only: false,
        };
        if target.kind == InlineKind::Variable {
            return Ok(info);
        }
        let mut files = Files {
            backend: backend.clone(),
            buffers: buffers.unwrap_or_default(),
        };
        files.buffers.insert(path.clone(), text.clone());
        let (decl_path, decl_text, span, usages) =
            declaration_of(&backend, &mut files, &path, &text, target.span.start)?;
        match target.kind {
            InlineKind::Constant => {
                inline::constant_value(&decl_text, &decl_path, span).map_err(refuse)?;
            }
            _ => {
                inline::callable_body(&decl_text, &decl_path, span).map_err(refuse)?;
            }
        }
        info.read_only = is_library(&decl_path);
        info.occurrences = usages.len();
        Ok(info)
    })
    .await
}

/// Inline what is at the caret: every reference (`all` removes the declaration, `allKeep`
/// keeps it) or only `this` one.
#[tauri::command]
pub async fn refactor_inline(
    path: String,
    position: Position,
    content: String,
    mode: InlineMode,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<RefactorAnswer> {
    require_php(&path)?;
    let backend = state.backend()?;
    blocking(move || {
        let text = content;
        let at = to_byte(&text, position);
        let target = inline::target_at(&text, at).map_err(refuse)?;
        let mut files = Files {
            backend: backend.clone(),
            buffers: buffers.unwrap_or_default(),
        };
        files.buffers.insert(path.clone(), text.clone());
        if target.kind == InlineKind::Variable {
            let plan = inline::inline_variable(&text, &path, at, mode).map_err(refuse)?;
            return Ok(answer(&path, &text, plan, vec![], &mut files));
        }
        let (decl_path, decl_text, span, usages) =
            declaration_of(&backend, &mut files, &path, &text, target.span.start)?;
        let only = (mode == InlineMode::This && target.on_reference).then(|| UsageSpan {
            path: path.clone(),
            span: target.span,
        });
        let remove = mode == InlineMode::All && !is_library(&decl_path);
        let (edits, conflicts) = match target.kind {
            InlineKind::Constant => {
                let c = inline::constant_value(&decl_text, &decl_path, span).map_err(refuse)?;
                inline::inline_constant(&c, &usages, only.as_ref(), remove, &mut |p| files.get(p))
            }
            _ => {
                let body = inline::callable_body(&decl_text, &decl_path, span).map_err(refuse)?;
                inline::inline_callable(&body, &usages, only.as_ref(), remove, &mut |p| {
                    files.get(p)
                })
            }
        };
        if edits.is_empty() && !conflicts.is_empty() {
            return Err(refuse(format!(
                "Cannot perform refactoring. {}",
                conflicts[0].message
            )));
        }
        let plan = Plan {
            conflicts,
            ..Default::default()
        };
        Ok(answer(&path, &text, plan, edits, &mut files))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_between_lsp_positions_and_bytes() {
        let text = "<?php\n$a = '€';\n$b = 1;";
        let p = Position::new(2, 1);
        let b = to_byte(text, p);
        assert_eq!(&text[b..b + 1], "b");
        assert_eq!(to_pos(text, b), p);
        assert!(require_php("a.blade.php").is_err());
        assert!(require_php("src/A.php").is_ok());
    }
}
