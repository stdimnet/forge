//! Extract Interface, Extract Superclass, Pull Members Up, Push Members Down and Refactor
//! This for PHP (`phpforge_lang_php::refactor::{hierarchy, available}`). Rust finds the
//! hierarchy and the usages through the PHP index and computes every edit, the new file
//! included (rendered from the user's PHP Class / Interface file templates); the UI shows
//! the dialogs and applies the edit as one undoable step. Files go through the project's
//! backend, so remote projects work too.

use std::collections::HashMap;

use forge_core::Result;
use forge_file_templates::{self as templates, CustomTemplate, NewFileRequest, TemplateContext};
use forge_lsp::forge_codeintel::Position;
use phpforge_lang_php::refactor::available;
use phpforge_lang_php::refactor::hierarchy::{
    self, ClassMembers, ExtractInterfaceOptions, ExtractSuperclassOptions, HierarchyClass,
    NewTypeFile, PullUpOptions, PushDownOptions,
};
use serde::Serialize;
use tauri::State;

use crate::commands::refactor::{refuse, require_php, to_byte, to_pos, RefactorAnswer};
use crate::commands::refactor_move::{plan_answer, same, BackendProject};
use crate::commands::templates::render_context;
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// What the dialogs show: the class, its members and the classes of its hierarchy.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyAnswer {
    /// The declaring file and the class name's position there (for the refactoring call).
    pub path: String,
    pub position: Position,
    /// The folder of the class's file.
    pub directory: String,
    #[serde(flatten)]
    pub info: ClassMembers,
    /// Pull Members Up: the superclasses and interfaces; Push Members Down: the direct
    /// subclasses.
    pub targets: Vec<HierarchyClass>,
}

/// Which dialog asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HierarchyRefactoring {
    ExtractInterface,
    ExtractSuperclass,
    PullUp,
    PushDown,
}

/// The class for Extract Interface / Superclass, Pull Members Up or Push Members Down at
/// the caret, with its members (the member at the caret selected).
#[tauri::command]
pub async fn refactor_hierarchy_prepare(
    path: String,
    position: Position,
    content: String,
    refactoring: HierarchyRefactoring,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<HierarchyAnswer> {
    require_php(&path)?;
    let backend = state.backend()?;
    blocking(move || {
        let mut p = BackendProject::new(backend, buffers);
        let text = p.text(&path, Some(content))?;
        let at = to_byte(&text, position);
        let (decl_path, info) =
            hierarchy::hierarchy_target(&mut p, &path, &text, at).map_err(refuse)?;
        let decl = p.text(&decl_path, None)?;
        let targets = match refactoring {
            HierarchyRefactoring::PullUp => {
                let t = hierarchy::supertypes(&mut p, &info);
                if t.is_empty() {
                    return Err(refuse(format!(
                        "Cannot perform refactoring. {} {} has no superclass or interface in the project",
                        capitalized(&info.kind),
                        info.name
                    )));
                }
                t
            }
            HierarchyRefactoring::PushDown => {
                hierarchy::direct_subclasses(&mut p, &decl_path, &decl, &info)
            }
            HierarchyRefactoring::ExtractInterface if !matches!(info.kind.as_str(), "class" | "enum") => {
                return Err(refuse(
                    "Cannot perform refactoring. Extract Interface is available for classes and enums".into(),
                ))
            }
            HierarchyRefactoring::ExtractSuperclass if info.kind != "class" => {
                return Err(refuse(
                    "Cannot perform refactoring. Extract Superclass is available for classes".into(),
                ))
            }
            _ => vec![],
        };
        Ok(HierarchyAnswer {
            position: to_pos(&decl, info.name_span.start),
            directory: decl_path
                .rsplit_once('/')
                .map_or(String::new(), |(d, _)| d.to_string()),
            path: decl_path,
            info,
            targets,
        })
    })
    .await
}

fn capitalized(s: &str) -> String {
    let mut cs = s.chars();
    cs.next()
        .map(|f| f.to_uppercase().chain(cs).collect())
        .unwrap_or_default()
}

/// The new file from the PHP Class / Interface template.
fn render_type(
    custom: &[CustomTemplate],
    context: &TemplateContext,
    t: &NewTypeFile,
) -> std::result::Result<(String, String), String> {
    let request = NewFileRequest {
        template: if t.kind == "interface" {
            templates::builtin::names::PHP_INTERFACE
        } else {
            templates::builtin::names::PHP_CLASS
        }
        .into(),
        dir: t.directory.clone(),
        name: t.name.clone(),
        namespace: Some(t.namespace.clone()),
        extends: t
            .extends
            .as_ref()
            .map(|e| format!("\\{}", e.trim_start_matches('\\'))),
        context: context.clone(),
        ..NewFileRequest::default()
    };
    templates::new_file(&request, custom)
        .map(|f| (f.path, f.text))
        .map_err(|e| format!("Cannot perform refactoring. {}", e.message))
}

/// Extract Interface from the class whose name is at `position`.
#[tauri::command]
pub async fn refactor_extract_interface(
    path: String,
    position: Position,
    content: Option<String>,
    options: ExtractInterfaceOptions,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<RefactorAnswer> {
    require_php(&path)?;
    let (custom, context) = render_context(&state)?;
    let backend = state.backend()?;
    blocking(move || {
        let mut p = BackendProject::new(backend, buffers);
        let text = p.text(&path, content)?;
        let at = to_byte(&text, position);
        let skeleton = |t: &NewTypeFile| render_type(&custom, &context, t);
        let plan = hierarchy::extract_interface(&mut p, &path, &text, at, &options, &skeleton)
            .map_err(refuse)?;
        Ok(plan_answer(plan, &mut p.files, &same))
    })
    .await
}

/// Extract Superclass from the class whose name is at `position`.
#[tauri::command]
pub async fn refactor_extract_superclass(
    path: String,
    position: Position,
    content: Option<String>,
    options: ExtractSuperclassOptions,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<RefactorAnswer> {
    require_php(&path)?;
    let (custom, context) = render_context(&state)?;
    let backend = state.backend()?;
    blocking(move || {
        let mut p = BackendProject::new(backend, buffers);
        let text = p.text(&path, content)?;
        let at = to_byte(&text, position);
        let skeleton = |t: &NewTypeFile| render_type(&custom, &context, t);
        let plan = hierarchy::extract_superclass(&mut p, &path, &text, at, &options, &skeleton)
            .map_err(refuse)?;
        Ok(plan_answer(plan, &mut p.files, &same))
    })
    .await
}

/// Pull Members Up from the class whose name is at `position`.
#[tauri::command]
pub async fn refactor_pull_up(
    path: String,
    position: Position,
    content: Option<String>,
    options: PullUpOptions,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<RefactorAnswer> {
    require_php(&path)?;
    let backend = state.backend()?;
    blocking(move || {
        let mut p = BackendProject::new(backend, buffers);
        let text = p.text(&path, content)?;
        let at = to_byte(&text, position);
        let plan = hierarchy::pull_up(&mut p, &path, &text, at, &options).map_err(refuse)?;
        Ok(plan_answer(plan, &mut p.files, &same))
    })
    .await
}

/// Push Members Down from the class whose name is at `position`.
#[tauri::command]
pub async fn refactor_push_down(
    path: String,
    position: Position,
    content: Option<String>,
    options: PushDownOptions,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<RefactorAnswer> {
    require_php(&path)?;
    let backend = state.backend()?;
    blocking(move || {
        let mut p = BackendProject::new(backend, buffers);
        let text = p.text(&path, content)?;
        let at = to_byte(&text, position);
        let plan = hierarchy::push_down(&mut p, &path, &text, at, &options).map_err(refuse)?;
        Ok(plan_answer(plan, &mut p.files, &same))
    })
    .await
}

/// Refactor This in a PHP file: the refactorings (action ids) that apply to the selection
/// `start..end` (a caret when equal).
pub(crate) fn php_refactorings(content: &str, start: Position, end: Position) -> Vec<String> {
    let (s, e) = (to_byte(content, start), to_byte(content, end));
    available::refactorings_at(content, s.min(e), s.max(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_new_types_from_the_file_templates() {
        let t = NewTypeFile {
            kind: "class".into(),
            name: "Customer".into(),
            namespace: "App\\Domain".into(),
            directory: "app/Domain".into(),
            extends: Some("App\\Models\\Model".into()),
        };
        let (path, text) = render_type(&[], &TemplateContext::default(), &t).unwrap();
        assert_eq!(path, "app/Domain/Customer.php");
        assert!(text.contains("namespace App\\Domain;"), "{text}");
        assert!(text.contains("use App\\Models\\Model;"), "{text}");
        assert!(text.contains("class Customer extends Model"), "{text}");
        let content = "<?php\nclass A {\n    public function f() {}\n}\n";
        let found = php_refactorings(content, Position::new(1, 7), Position::new(1, 7));
        assert!(found.contains(&"ExtractInterface".to_string()));
    }
}
