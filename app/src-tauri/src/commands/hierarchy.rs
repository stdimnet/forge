//! Type Hierarchy (Ctrl+H), Method Hierarchy (Ctrl+Shift+H) and Call Hierarchy (Ctrl+Alt+H):
//! the language servers' `typeHierarchy` / `callHierarchy` first, the PHP index (through
//! the project's backend, so remote projects work) for PHP files, merged in
//! `forge_codeintel::hierarchy`. The commands answer the data; the views (Class /
//! Supertypes / Subtypes, scopes, sorting, method marks) are computed from it by
//! `forge_codeintel::hierarchy` too, in the UI through WebAssembly.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_backend::Backend;
use forge_core::Result;
use forge_lsp::forge_codeintel::hierarchy::{
    merge_call_nodes, merge_type_graphs, CallDirection, CallNode, HierarchyItem, TypeGraph,
};
use forge_lsp::forge_codeintel::{self as codeintel, Position};
use forge_lsp::{lsp_types, LspManager};
use phpforge_lang_php::hierarchy::{HierarchyQuery, PhpHierarchy};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::commands::library::LibraryState;
use crate::commands::lsp::{to_ui_path, LspState};
use crate::commands::workspace::blocking;
use crate::state::AppState;

fn is_php(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    (p.ends_with(".php") || p.ends_with(".phtml") || p.ends_with(".inc"))
        && !p.ends_with(".blade.php")
}

/// A Type or Method Hierarchy.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeHierarchyAnswer {
    /// `PaymentGateway`, `Stripe::charge()`.
    pub title: String,
    pub graph: Option<TypeGraph>,
    /// `lsp`, `index` or `none`.
    pub source: String,
    /// Why there is nothing to show.
    pub message: Option<String>,
}

/// The function a Call Hierarchy starts at.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallHierarchyAnswer {
    /// `PaymentService::pay()`.
    pub title: String,
    pub root: Option<HierarchyItem>,
    pub source: String,
    pub message: Option<String>,
}

struct Ctx {
    manager: Arc<LspManager>,
    backend: Arc<dyn Backend>,
    root: PathBuf,
    remote: bool,
}

impl Ctx {
    fn new(app: &AppHandle, lsp: &LspState, ws: &AppState) -> Result<Self> {
        let manager = lsp.manager(app, ws)?;
        Ok(Ctx {
            root: manager.root().to_path_buf(),
            manager,
            backend: ws.backend()?,
            remote: ws.is_remote(),
        })
    }

    /// A server's absolute path as the UI's path: workspace-relative inside the project, a
    /// read-only `library://file/…` path for files on this machine, empty otherwise.
    fn ui_path(&self, path: &str, lib: &LibraryState) -> String {
        let p = Path::new(path);
        if path.is_empty() || !p.is_absolute() {
            path.to_string()
        } else if p.starts_with(&self.root) {
            to_ui_path(&self.root, p)
        } else if !self.remote && p.is_file() {
            lib.grant(p)
        } else {
            String::new()
        }
    }

    fn map_item(&self, item: &mut HierarchyItem, lib: &LibraryState) {
        item.path = self.ui_path(&item.path, lib);
    }

    /// The caret's text: the editor buffer, else the file.
    fn text(&self, path: &str, content: Option<String>) -> String {
        content
            .or_else(|| self.backend.read_optional(path).ok().flatten())
            .unwrap_or_default()
    }
}

fn index_query(
    backend: &Arc<dyn Backend>,
    path: &str,
    text: String,
    line: u32,
    character: u32,
    query: HierarchyQuery,
) -> Result<PhpHierarchy> {
    let offset = codeintel::utf16_offset(&text, Position::new(line, character));
    backend.php().hierarchy(path, offset, query, Some(text))
}

/// Type Hierarchy of the type at the caret: the servers' type hierarchy, merged with the
/// PHP index for PHP files (traits, abstract classes and vendor parents).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn code_type_hierarchy(
    path: String,
    line: u32,
    character: u32,
    content: Option<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
    lib: State<'_, LibraryState>,
) -> Result<TypeHierarchyAnswer> {
    let c = Ctx::new(&app, &lsp, &ws)?;
    let abs = c.backend.resolve(&path)?;
    let pos = lsp_types::Position::new(line, character);
    let mut lsp_graph = c.manager.type_graph(&abs, pos).await.ok().flatten();
    if let Some(g) = &mut lsp_graph {
        for t in &mut g.types {
            c.map_item(&mut t.item, &lib);
        }
    }
    let text = c.text(&path, content);
    let backend = c.backend.clone();
    let index = if is_php(&path) {
        let p = path.clone();
        blocking(move || index_query(&backend, &p, text, line, character, HierarchyQuery::Type))
            .await?
    } else {
        PhpHierarchy::default()
    };
    let (graph, source) = match (index.graph, lsp_graph) {
        (Some(i), Some(l)) => (Some(merge_type_graphs(i, l)), "index"),
        (Some(i), None) => (Some(i), "index"),
        (None, Some(l)) => (Some(l), "lsp"),
        (None, None) => (None, "none"),
    };
    Ok(TypeHierarchyAnswer {
        title: graph
            .as_ref()
            .and_then(|g| g.types.iter().find(|t| t.item.id == g.root))
            .map(|t| t.item.name.clone())
            .unwrap_or_default(),
        message: graph
            .is_none()
            .then(|| "No class, interface, trait or enum at the caret".to_string()),
        graph,
        source: source.into(),
    })
}

/// Method Hierarchy of the method at the caret (PHP index).
#[tauri::command]
pub async fn code_method_hierarchy(
    path: String,
    line: u32,
    character: u32,
    content: Option<String>,
    ws: State<'_, AppState>,
) -> Result<TypeHierarchyAnswer> {
    if !is_php(&path) {
        return Ok(TypeHierarchyAnswer {
            source: "none".into(),
            message: Some("Method Hierarchy is available for PHP files".into()),
            ..Default::default()
        });
    }
    let backend = ws.backend()?;
    blocking(move || {
        let text = content
            .or_else(|| backend.read_optional(&path).ok().flatten())
            .unwrap_or_default();
        let found = index_query(
            &backend,
            &path,
            text,
            line,
            character,
            HierarchyQuery::Method,
        )?;
        Ok(TypeHierarchyAnswer {
            title: found.title.unwrap_or_default(),
            source: if found.graph.is_some() {
                "index"
            } else {
                "none"
            }
            .into(),
            message: found
                .graph
                .is_none()
                .then(|| "No method at the caret".to_string()),
            graph: found.graph,
        })
    })
    .await
}

/// The function a Call Hierarchy starts at: the servers' `prepareCallHierarchy`, else the
/// PHP index.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn code_call_hierarchy(
    path: String,
    line: u32,
    character: u32,
    content: Option<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
    lib: State<'_, LibraryState>,
) -> Result<CallHierarchyAnswer> {
    let c = Ctx::new(&app, &lsp, &ws)?;
    let abs = c.backend.resolve(&path)?;
    let pos = lsp_types::Position::new(line, character);
    let mut root = c.manager.call_root(&abs, pos).await.ok().flatten();
    if let Some(r) = &mut root {
        c.map_item(r, &lib);
    }
    let text = c.text(&path, content);
    let backend = c.backend.clone();
    let index = if is_php(&path) {
        let p = path.clone();
        blocking(move || index_query(&backend, &p, text, line, character, HierarchyQuery::Call))
            .await?
    } else {
        PhpHierarchy::default()
    };
    let (root, title, source) = match (root, index.root) {
        (Some(r), i) => {
            // The index names members `Class::method()`; servers only `method`.
            let title = index
                .title
                .filter(|_| i.is_some())
                .unwrap_or_else(|| match &r.container {
                    Some(c) => format!("{c}.{}", r.name),
                    None => r.name.clone(),
                });
            (Some(r), title, "lsp")
        }
        (None, Some(i)) => (Some(i), index.title.unwrap_or_default(), "index"),
        (None, None) => (None, String::new(), "none"),
    };
    Ok(CallHierarchyAnswer {
        message: root
            .is_none()
            .then(|| "No method or function at the caret".to_string()),
        root,
        title,
        source: source.into(),
    })
}

/// Callers or callees of a Call Hierarchy item (lazily, one level per expansion): the
/// server that handed out the item, merged with the PHP index for PHP files.
#[tauri::command]
pub async fn code_call_hierarchy_calls(
    item: HierarchyItem,
    direction: CallDirection,
    app: AppHandle,
    lsp: State<'_, LspState>,
    ws: State<'_, AppState>,
    lib: State<'_, LibraryState>,
) -> Result<Vec<CallNode>> {
    let c = Ctx::new(&app, &lsp, &ws)?;
    let mut from_lsp = c
        .manager
        .call_nodes(&item, direction)
        .await
        .unwrap_or_default();
    for n in &mut from_lsp {
        c.map_item(&mut n.item, &lib);
        for s in &mut n.calls {
            s.path = c.ui_path(&s.path, &lib);
        }
    }
    let backend = c.backend.clone();
    blocking(move || {
        let mut nodes = from_lsp;
        let project_php = is_php(&item.path) && !item.path.contains("://");
        if project_php {
            let query = match direction {
                CallDirection::Callers => HierarchyQuery::Callers,
                CallDirection::Callees => HierarchyQuery::Callees,
            };
            if let Some(text) = backend.read_optional(&item.path)? {
                let found = index_query(&backend, &item.path, text, item.line, item.column, query)?;
                nodes = merge_call_nodes(nodes, found.calls);
            }
        }
        fill_previews(&backend, &mut nodes);
        Ok(nodes)
    })
    .await
}

/// The source line of every call site that has none yet (servers send ranges only).
fn fill_previews(backend: &Arc<dyn Backend>, nodes: &mut [CallNode]) {
    let mut texts: HashMap<String, Option<String>> = HashMap::new();
    for site in nodes.iter_mut().flat_map(|n| n.calls.iter_mut()) {
        if !site.preview.is_empty() || site.path.is_empty() || site.path.contains("://") {
            continue;
        }
        let text = texts
            .entry(site.path.clone())
            .or_insert_with(|| backend.read_optional(&site.path).ok().flatten());
        if let Some(line) = text
            .as_deref()
            .and_then(|t| t.lines().nth(site.range.start.line as usize))
        {
            site.preview = line.trim().to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn php_files_are_recognized() {
        assert!(is_php("app/Models/User.php"));
        assert!(!is_php("resources/views/a.blade.php"));
        assert!(!is_php("src/main.ts"));
    }
}
