//! Move (F6), Copy (F5), Safe Delete (Alt+Delete) and Change Signature (Ctrl+F6) for PHP
//! (`phpforge_lang_php::refactor::project`). Rust finds the usages through the PHP index and
//! computes every edit; the UI shows the dialogs and applies the edit as one undoable step.
//! Files are read and the index is asked through the project's backend, so remote projects
//! work too. Positions are LSP positions (0-based line, UTF-16 character).

use std::collections::HashMap;
use std::sync::Arc;

use forge_backend::Backend;
use forge_core::{FileKind, ForgeError, Result};
use forge_file_templates::psr4;
use forge_lsp::forge_codeintel::{
    self as codeintel, preview_workspace_edit, DocumentChange, ElementKind, GroupRequest, Position,
    Range, TextEdit as LspEdit, UsageLocation, UsageOptions, WorkspaceEdit,
};
use phpforge_lang_php::index::{SearchKind, SymbolHit};
use phpforge_lang_php::navigate::PhpUsages;
use phpforge_lang_php::offsets::byte_to_utf16;
use phpforge_lang_php::refactor::make_static::{MakeStaticInfo, MakeStaticOptions};
use phpforge_lang_php::refactor::move_class::{self, ClassInfo};
use phpforge_lang_php::refactor::move_members::{self, MembersInfo};
use phpforge_lang_php::refactor::project::{
    self, FileMove, FileOp, MoveTarget, Project, ProjectPlan,
};
use phpforge_lang_php::refactor::signature::{self, ChangeSignatureOptions, SignatureInfo};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::codeintel::UsagesAnswer;
use crate::commands::refactor::{
    lsp_edits, refuse, require_php, to_byte, to_pos, Files, RefactorAnswer,
};
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// The project as the refactorings see it: the backend's PHP index and files (editor
/// buffers first).
struct BackendProject {
    backend: Arc<dyn Backend>,
    files: Files,
}

impl BackendProject {
    fn new(backend: Arc<dyn Backend>, buffers: Option<HashMap<String, String>>) -> Self {
        Self {
            files: Files {
                backend: backend.clone(),
                buffers: buffers.unwrap_or_default(),
            },
            backend,
        }
    }

    /// `content` (the editor's text) or the file.
    fn text(&mut self, path: &str, content: Option<String>) -> Result<String> {
        match content {
            Some(c) => {
                self.files.buffers.insert(path.to_string(), c.clone());
                Ok(c)
            }
            None => self
                .files
                .get(path)
                .ok_or_else(|| refuse(format!("Cannot read {path}"))),
        }
    }
}

impl Project for BackendProject {
    fn usages(
        &mut self,
        path: &str,
        text: &str,
        offset: usize,
        options: &UsageOptions,
    ) -> Option<PhpUsages> {
        self.backend
            .php()
            .usages(
                path,
                byte_to_utf16(text, offset),
                options.clone(),
                Some(text.to_string()),
            )
            .ok()
            .flatten()
    }

    fn read(&mut self, path: &str) -> Option<String> {
        self.files.get(path)
    }

    fn definition(&mut self, path: &str, text: &str, offset: usize) -> Option<(String, usize)> {
        let d = self
            .backend
            .php()
            .definition(path, byte_to_utf16(text, offset), Some(text.to_string()))
            .ok()?;
        if d.on_declaration {
            return None;
        }
        let t = d.targets.first()?;
        let target = self.files.get(&t.path)?;
        Some((
            t.path.clone(),
            to_byte(&target, Position::new(t.line, t.column)),
        ))
    }

    fn search(&mut self, name: &str, kind: SearchKind) -> Vec<SymbolHit> {
        self.backend
            .php()
            .search(name, kind, 50, true)
            .unwrap_or_default()
    }
}

/// The plan as a workspace edit with its preview. `source` maps a path of the plan to the
/// file holding its current text (moved files are edited at their new path).
fn plan_answer(
    plan: ProjectPlan,
    files: &mut Files,
    source: &dyn Fn(&str) -> String,
) -> RefactorAnswer {
    let mut edit = WorkspaceEdit::default();
    for op in plan.ops {
        match op {
            FileOp::Edit { path, edits } => {
                if let Some(t) = files.get(&source(&path)) {
                    edit.add_edits(path, lsp_edits(&t, &edits));
                }
            }
            FileOp::Create { path, text } => {
                edit.changes.push(DocumentChange::Create {
                    path: path.clone(),
                    overwrite: false,
                });
                edit.add_edits(
                    path,
                    vec![LspEdit {
                        range: Range::default(),
                        new_text: text,
                    }],
                );
            }
            FileOp::Rename { from, to } => edit.changes.push(DocumentChange::Rename { from, to }),
            FileOp::Delete { path } => edit.changes.push(DocumentChange::Delete { path }),
        }
    }
    let preview = preview_workspace_edit(&edit, |p| files.get(&source(p)));
    RefactorAnswer {
        edit,
        preview,
        conflicts: plan.conflicts,
        name_ranges: vec![],
        skipped: 0,
    }
}

fn same(p: &str) -> String {
    p.to_string()
}

// ---- Move / Copy Class ----

/// The class Move and Copy work on (F6 / F5 in the editor).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassTarget {
    /// `class`.
    pub kind: String,
    /// The file that declares it.
    pub path: String,
    /// Its name's position in that file.
    pub position: Position,
    pub name: String,
    pub fqn: String,
    pub namespace: String,
    /// `class`, `interface`, `trait` or `enum`.
    pub class_kind: String,
    /// Alone in its file (the file moves with it).
    pub sole: bool,
    /// The folder of its file.
    pub directory: String,
}

fn class_target(path: String, class: ClassInfo, text: &str) -> ClassTarget {
    ClassTarget {
        kind: "class".into(),
        directory: path
            .rsplit_once('/')
            .map_or(String::new(), |(d, _)| d.to_string()),
        position: to_pos(text, class.name_span.start),
        name: class.name,
        fqn: class.fqn,
        namespace: class.namespace,
        class_kind: class.kind,
        sole: class.sole,
        path,
    }
}

/// The static members Move Members offers (F6 on a static member).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MembersTarget {
    pub path: String,
    /// The class name's position.
    pub position: Position,
    #[serde(flatten)]
    pub info: MembersInfo,
}

/// What F6 / F5 work on: a class, or static members (`kind`: `class` | `members`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MovePrepareAnswer {
    pub kind: String,
    pub class: Option<ClassTarget>,
    pub members: Option<MembersTarget>,
}

/// Move / Copy (F6 / F5) in the editor: the static member at the caret (Move Members),
/// else the class at the caret (a reference's class too). `copy` asks for a class only.
#[tauri::command]
pub async fn refactor_move_prepare(
    path: String,
    position: Position,
    content: String,
    copy: Option<bool>,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<MovePrepareAnswer> {
    require_php(&path)?;
    let backend = state.backend()?;
    blocking(move || {
        let mut p = BackendProject::new(backend, buffers);
        let text = p.text(&path, Some(content))?;
        let at = to_byte(&text, position);
        if !copy.unwrap_or(false) {
            let members = project::at_declaration(&mut p, &path, &text, at, |t, o| {
                move_members::members_at(t, o)
                    .filter(|m| m.selected.is_some())
                    .ok_or_else(String::new)
            });
            if let Ok((decl_path, decl, info)) = members {
                return Ok(MovePrepareAnswer {
                    kind: "members".into(),
                    class: None,
                    members: Some(MembersTarget {
                        position: to_pos(&decl, class_name_start(&decl, &info)),
                        path: decl_path,
                        info,
                    }),
                });
            }
        }
        match project::move_target(&mut p, &path, &text, at).map_err(refuse)? {
            MoveTarget::Class { path, class } => {
                let decl = p.text(&path, None)?;
                Ok(MovePrepareAnswer {
                    kind: "class".into(),
                    class: Some(class_target(path, class, &decl)),
                    members: None,
                })
            }
        }
    })
    .await
}

/// Where the class of `info` has its name (for later calls at that position).
fn class_name_start(text: &str, info: &MembersInfo) -> usize {
    move_class::class_at(text, info.class_span.start)
        .map_or(info.class_span.start, |c| c.name_span.start)
}

/// Move Class: into `targetNamespace`, its file into `targetDirectory`, updating the
/// namespace, imports and references project-wide.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn refactor_move_class(
    path: String,
    position: Position,
    content: Option<String>,
    target_namespace: String,
    target_directory: String,
    search_in_comments: bool,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<RefactorAnswer> {
    require_php(&path)?;
    let backend = state.backend()?;
    blocking(move || {
        let mut p = BackendProject::new(backend, buffers);
        let text = p.text(&path, content)?;
        let at = to_byte(&text, position);
        let plan = project::move_class(
            &mut p,
            &path,
            &text,
            at,
            &target_namespace,
            &target_directory,
            search_in_comments,
        )
        .map_err(refuse)?;
        Ok(plan_answer(plan, &mut p.files, &same))
    })
    .await
}

/// Copy Class: a copy named `newName` in `targetNamespace`, in `targetDirectory`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn refactor_copy_class(
    path: String,
    position: Position,
    content: Option<String>,
    new_name: String,
    target_namespace: String,
    target_directory: String,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<RefactorAnswer> {
    require_php(&path)?;
    let backend = state.backend()?;
    blocking(move || {
        let mut p = BackendProject::new(backend, buffers);
        let text = p.text(&path, content)?;
        let at = to_byte(&text, position);
        let plan = project::copy_class(
            &mut p,
            &path,
            &text,
            at,
            &new_name,
            &target_namespace,
            &target_directory,
        )
        .map_err(refuse)?;
        Ok(plan_answer(plan, &mut p.files, &same))
    })
    .await
}

// ---- Files and folders (Project view) ----

/// What the Project view moves or copies: a top-level entry and its new path.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PathMove {
    pub from: String,
    pub to: String,
}

fn kind_of(backend: &Arc<dyn Backend>, path: &str) -> Option<FileKind> {
    let parent = path.rsplit_once('/').map_or("", |(d, _)| d);
    backend
        .list_dir(parent)
        .ok()?
        .into_iter()
        .find(|e| e.path == path)
        .map(|e| e.kind)
}

/// Every file below `path` (or `path` itself).
fn files_below(backend: &Arc<dyn Backend>, path: &str, out: &mut Vec<String>) {
    match kind_of(backend, path) {
        Some(FileKind::Dir) => {
            for e in backend.list_dir(path).unwrap_or_default() {
                match e.kind {
                    FileKind::Dir => files_below(backend, &e.path, out),
                    FileKind::File => out.push(e.path),
                }
            }
        }
        Some(FileKind::File) => out.push(path.to_string()),
        None => {}
    }
}

/// The edit of a Project view move.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveFilesAnswer {
    /// Text edits for the paths after the move (the UI moves the files first).
    pub answer: RefactorAnswer,
    /// Every file that moves.
    pub files: Vec<FileMove>,
}

/// Moving files and folders (F6, drag and drop): PHP files following PSR-4 get the
/// namespace of their new folder and references to their classes are updated
/// (`searchForReferences`). Nothing is moved here.
#[tauri::command]
pub async fn refactor_move_files(
    moves: Vec<PathMove>,
    search_for_references: bool,
    search_in_comments: bool,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<MoveFilesAnswer> {
    let roots = crate::commands::templates::psr4_roots(&state).await?;
    let backend = state.backend()?;
    blocking(move || {
        let mut files = Vec::new();
        for m in &moves {
            if m.to == m.from || m.to.starts_with(&format!("{}/", m.from)) {
                return Err(refuse(format!("Cannot move {} into itself", m.from)));
            }
            let mut below = Vec::new();
            files_below(&backend, &m.from, &mut below);
            for f in below {
                let to = format!("{}{}", m.to, &f[m.from.len()..]);
                files.push(FileMove { from: f, to });
            }
        }
        let mut p = BackendProject::new(backend, buffers);
        let plan = if search_for_references {
            let ns = |d: &str| psr4::namespace_for_dir(&roots, d);
            project::move_files(&mut p, &files, &ns, search_in_comments).map_err(refuse)?
        } else {
            ProjectPlan::default()
        };
        let back: HashMap<String, String> = files
            .iter()
            .map(|f| (f.to.clone(), f.from.clone()))
            .collect();
        let source = |p: &str| back.get(p).cloned().unwrap_or_else(|| p.to_string());
        Ok(MoveFilesAnswer {
            answer: plan_answer(plan, &mut p.files, &source),
            files,
        })
    })
    .await
}

// ---- Safe Delete ----

/// Safe Delete: the element, the usages that are not safe to delete and the edit that
/// deletes it ("Delete Anyway" applies the same edit).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeDeleteAnswer {
    /// `class`, `method`, `parameter`, ...; `file` / `directory` in the Project view.
    pub kind: String,
    /// `User`, `orders()`, `$limit`.
    pub name: String,
    /// The Usages Detected dialog's text.
    pub message: String,
    pub count: usize,
    pub in_comments: usize,
    /// The usages for "View Usages" (Find tool window).
    pub usages: UsagesAnswer,
    pub answer: RefactorAnswer,
    /// Folders and files the workspace edit cannot delete (the UI deletes them).
    pub paths: Vec<String>,
}

fn element_kind(kind: &str) -> ElementKind {
    match kind {
        "class" => ElementKind::Class,
        "interface" => ElementKind::Interface,
        "trait" => ElementKind::Trait,
        "enum" => ElementKind::Enum,
        "method" => ElementKind::Method,
        "property" => ElementKind::Property,
        "constant" => ElementKind::Constant,
        "enum case" => ElementKind::EnumCase,
        "function" => ElementKind::Function,
        "parameter" => ElementKind::Variable,
        _ => ElementKind::Symbol,
    }
}

fn usages_answer(
    title: String,
    kind: &str,
    usages: Vec<UsageLocation>,
    files: &mut Files,
) -> UsagesAnswer {
    let mut texts: HashMap<String, String> = HashMap::new();
    for u in &usages {
        if !texts.contains_key(&u.path) {
            if let Some(t) = files.get(&u.path) {
                texts.insert(u.path.clone(), t);
            }
        }
    }
    let mut req = GroupRequest {
        usages,
        declaration: None,
        group_by_type: true,
    };
    let by_type = codeintel::group_usages(&req, |p| texts.get(p).map(String::as_str));
    req.group_by_type = false;
    let by_file = codeintel::group_usages(&req, |p| texts.get(p).map(String::as_str));
    UsagesAnswer {
        title,
        source: "index".into(),
        declaration: None,
        by_type,
        by_file,
        element: element_kind(kind),
        scope: "Project Files".into(),
    }
}

/// Safe Delete (Alt+Delete) of the element at the caret (or the one a reference names).
#[tauri::command]
pub async fn refactor_safe_delete_prepare(
    path: String,
    position: Position,
    content: String,
    search_in_comments: bool,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<SafeDeleteAnswer> {
    require_php(&path)?;
    let backend = state.backend()?;
    blocking(move || {
        let mut p = BackendProject::new(backend, buffers);
        let text = p.text(&path, Some(content))?;
        let at = to_byte(&text, position);
        let r =
            project::safe_delete(&mut p, &path, &text, at, search_in_comments).map_err(refuse)?;
        let count = r.usages.len();
        let usages = usages_answer(
            format!("{} {}", r.target.kind, r.target.name),
            &r.target.kind,
            r.usages,
            &mut p.files,
        );
        Ok(SafeDeleteAnswer {
            kind: r.target.kind,
            name: r.target.name,
            message: r.message,
            count,
            in_comments: r.in_comments,
            usages,
            answer: plan_answer(r.plan, &mut p.files, &same),
            paths: vec![],
        })
    })
    .await
}

/// Safe Delete in the Project view: the usages of the classes the files and folders
/// declare, and the edit deleting the text files (undoable); folders and other files are
/// in `paths`.
#[tauri::command]
pub async fn refactor_safe_delete_files(
    paths: Vec<String>,
    search_in_comments: bool,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<SafeDeleteAnswer> {
    let backend = state.backend()?;
    blocking(move || {
        let mut all = Vec::new();
        let mut dirs = Vec::new();
        for path in &paths {
            if kind_of(&backend, path) == Some(FileKind::Dir) {
                dirs.push(path.clone());
            }
            files_below(&backend, path, &mut all);
        }
        let mut p = BackendProject::new(backend, buffers);
        let usages = project::safe_delete_files(&mut p, &all, search_in_comments);
        let in_comments = usages
            .iter()
            .filter(|u| {
                matches!(
                    u.kind,
                    Some(codeintel::UsageKind::Comment | codeintel::UsageKind::StringLiteral)
                )
            })
            .count();
        let count = usages.len();
        let (kind, name) = match (paths.len(), dirs.len()) {
            (1, 1) => ("directory".to_string(), short(&paths[0])),
            (1, _) => ("file".to_string(), short(&paths[0])),
            (n, _) => ("files".to_string(), format!("{n} items")),
        };
        let mut plan = ProjectPlan::default();
        let mut rest = Vec::new();
        for path in &paths {
            if dirs.contains(path) || p.files.get(path).is_none() {
                rest.push(path.clone());
            } else {
                plan.ops.push(FileOp::Delete { path: path.clone() });
            }
        }
        let what = if count == 1 { "usage" } else { "usages" };
        let message = format!(
            "{kind} {name} has {count} {what} that {} not safe to delete.\nOf those {in_comments} {} in strings, comments, or non-code files.",
            if count == 1 { "is" } else { "are" },
            if in_comments == 1 { "usage is" } else { "usages are" },
        );
        let usages = usages_answer(format!("{kind} {name}"), "class", usages, &mut p.files);
        Ok(SafeDeleteAnswer {
            kind,
            name,
            message,
            count,
            in_comments,
            usages,
            answer: plan_answer(plan, &mut p.files, &same),
            paths: rest,
        })
    })
    .await
}

fn short(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

// ---- Change Signature ----

/// The Change Signature dialog's data.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureAnswer {
    /// The declaration's file and name position (a call at the caret leads there).
    pub path: String,
    pub position: Position,
    #[serde(flatten)]
    pub info: SignatureInfo,
    /// The current signature as code.
    pub signature: String,
}

/// Change Signature (Ctrl+F6): the method or function at the caret (or the one a call at
/// the caret calls).
#[tauri::command]
pub async fn refactor_change_signature_prepare(
    path: String,
    position: Position,
    content: String,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<SignatureAnswer> {
    require_php(&path)?;
    let backend = state.backend()?;
    blocking(move || {
        let mut p = BackendProject::new(backend, buffers);
        let text = p.text(&path, Some(content))?;
        let at = to_byte(&text, position);
        let (decl_path, info) = project::signature_at(&mut p, &path, &text, at).map_err(refuse)?;
        let decl = p.text(&decl_path, None)?;
        Ok(SignatureAnswer {
            position: to_pos(&decl, info.name_span.start),
            signature: signature::render_signature(&info, &signature::unchanged(&info)),
            path: decl_path,
            info,
        })
    })
    .await
}

/// Change the signature of the method or function whose name is at `position` of `path`.
#[tauri::command]
pub async fn refactor_change_signature(
    path: String,
    position: Position,
    content: Option<String>,
    options: ChangeSignatureOptions,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<RefactorAnswer> {
    require_php(&path)?;
    let backend = state.backend()?;
    blocking(move || {
        let mut p = BackendProject::new(backend, buffers);
        let text = p.text(&path, content)?;
        let at = to_byte(&text, position);
        let plan = project::change_signature(&mut p, &path, &text, at, &options).map_err(refuse)?;
        Ok(plan_answer(plan, &mut p.files, &same))
    })
    .await
}

// ---- Make Static ----

/// The Make Static dialog's data.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MakeStaticAnswer {
    pub path: String,
    pub position: Position,
    #[serde(flatten)]
    pub info: MakeStaticInfo,
}

/// Make Static: the method at the caret (or the one a call at the caret calls).
#[tauri::command]
pub async fn refactor_make_static_prepare(
    path: String,
    position: Position,
    content: String,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<MakeStaticAnswer> {
    require_php(&path)?;
    let backend = state.backend()?;
    blocking(move || {
        let mut p = BackendProject::new(backend, buffers);
        let text = p.text(&path, Some(content))?;
        let at = to_byte(&text, position);
        let (decl_path, info) =
            project::make_static_target(&mut p, &path, &text, at).map_err(refuse)?;
        let decl = p.text(&decl_path, None)?;
        Ok(MakeStaticAnswer {
            position: to_pos(&decl, info.name_span.start),
            path: decl_path,
            info,
        })
    })
    .await
}

/// Make the method whose name is at `position` static, updating its calls.
#[tauri::command]
pub async fn refactor_make_static(
    path: String,
    position: Position,
    content: Option<String>,
    options: MakeStaticOptions,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<RefactorAnswer> {
    require_php(&path)?;
    let backend = state.backend()?;
    blocking(move || {
        let mut p = BackendProject::new(backend, buffers);
        let text = p.text(&path, content)?;
        let at = to_byte(&text, position);
        let plan = project::make_static(&mut p, &path, &text, at, &options).map_err(refuse)?;
        Ok(plan_answer(plan, &mut p.files, &same))
    })
    .await
}

// ---- Move Members ----

/// Move the static `members` of the class whose name is at `position` to `target` (a
/// fully qualified class name).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn refactor_move_members(
    path: String,
    position: Position,
    content: Option<String>,
    members: Vec<String>,
    target: String,
    buffers: Option<HashMap<String, String>>,
    state: State<'_, AppState>,
) -> Result<RefactorAnswer> {
    require_php(&path)?;
    let backend = state.backend()?;
    blocking(move || {
        let mut p = BackendProject::new(backend, buffers);
        let text = p.text(&path, content)?;
        let at = to_byte(&text, position);
        let plan =
            project::move_members(&mut p, &path, &text, at, &members, &target).map_err(refuse)?;
        Ok(plan_answer(plan, &mut p.files, &same))
    })
    .await
}

/// Copy a file or folder (Project view Copy, F5) through the backend. Never replaces.
pub(crate) fn copy_path(backend: &Arc<dyn Backend>, from: &str, to: &str) -> Result<()> {
    if to == from || to.starts_with(&format!("{from}/")) {
        return Err(ForgeError::Other(format!("cannot copy {from} into itself")));
    }
    if kind_of(backend, to).is_some() {
        return Err(ForgeError::Other(format!("{to} already exists")));
    }
    match kind_of(backend, from) {
        Some(FileKind::Dir) => {
            backend.create_dir(to)?;
            for e in backend.list_dir(from)? {
                let target = format!("{to}{}", &e.path[from.len()..]);
                copy_path(backend, &e.path, &target)?;
            }
            Ok(())
        }
        Some(FileKind::File) => backend.write_bytes(to, &backend.read_bytes(from)?),
        None => Err(ForgeError::Other(format!("{from} does not exist"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_backend::LocalBackend;

    #[test]
    fn copies_files_and_folders() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("app/Models")).unwrap();
        std::fs::write(dir.path().join("app/Models/User.php"), "<?php\n").unwrap();
        std::fs::write(dir.path().join("app/logo.png"), [0u8, 159, 146, 150]).unwrap();
        let backend: Arc<dyn Backend> = Arc::new(LocalBackend::open(dir.path()).unwrap());
        copy_path(&backend, "app", "copy").unwrap();
        assert_eq!(
            std::fs::read(dir.path().join("copy/logo.png")).unwrap(),
            [0u8, 159, 146, 150]
        );
        assert!(dir.path().join("copy/Models/User.php").exists());
        assert!(copy_path(&backend, "app", "copy").is_err());
        assert!(copy_path(&backend, "app", "app/inner").is_err());
        let mut files = Vec::new();
        files_below(&backend, "app", &mut files);
        files.sort();
        assert_eq!(files, ["app/Models/User.php", "app/logo.png"]);
    }

    #[test]
    fn plans_become_workspace_edits() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.php"), "<?php\nclass A {}\n").unwrap();
        let backend: Arc<dyn Backend> = Arc::new(LocalBackend::open(dir.path()).unwrap());
        let mut files = Files {
            backend,
            buffers: HashMap::new(),
        };
        let plan = ProjectPlan {
            ops: vec![
                FileOp::Edit {
                    path: "a.php".into(),
                    edits: vec![phpforge_lang_php::generate::TextEdit {
                        start: 12,
                        end: 13,
                        new_text: "B".into(),
                    }],
                },
                FileOp::Rename {
                    from: "a.php".into(),
                    to: "b.php".into(),
                },
                FileOp::Create {
                    path: "c.php".into(),
                    text: "<?php\n".into(),
                },
            ],
            conflicts: vec![],
        };
        let a = plan_answer(plan, &mut files, &same);
        assert_eq!(a.edit.changes.len(), 4);
        assert_eq!(a.preview[0].rows[0].after, "class B {}");
        assert_eq!(a.preview[1].operation, "rename");
        assert_eq!(a.preview[2].operation, "create");
    }
}
