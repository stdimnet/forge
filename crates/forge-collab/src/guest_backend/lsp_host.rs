//! Answering guests' language server requests with the host's [`LspManager`]: the
//! [`HostServices`] the application plugs into a `HostSession`.
//!
//! The manager is whatever servers the host's window runs for the project (Intelephense and
//! friends, started on the host's machine); a guest never starts or configures one. Before
//! each request the servers get the file's current text (the shared document), so guests and
//! the host see the same completions and diagnostics.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_lsp::lsp_types::Position;
use forge_lsp::model::EditorLocation;
use forge_lsp::LspManager;
use futures_util::future::BoxFuture;
use serde::Serialize;

use super::calls::LspQuery;
use crate::host::{HostServices, LspRequest};

/// Where the application's language servers for the open project are.
pub trait ManagerSource: Send + Sync + 'static {
    /// The servers of the open project (created on first use).
    fn manager(&self) -> Result<Arc<LspManager>, String>;
}

/// [`HostServices`] over the host's language servers.
pub struct LspServices<S: ManagerSource>(pub Arc<S>);

impl<S: ManagerSource> HostServices for LspServices<S> {
    fn lsp(&self, req: LspRequest) -> BoxFuture<'static, Result<Vec<u8>, String>> {
        let source = self.0.clone();
        Box::pin(async move {
            let manager = source.manager()?;
            answer(&manager, req).await
        })
    }
}

fn json<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|e| e.to_string())
}

/// The file a query is about.
fn query_path(q: &LspQuery) -> Option<&str> {
    match q {
        LspQuery::Close { .. } | LspQuery::WorkspaceSymbols { .. } => None,
        LspQuery::Sync { path }
        | LspQuery::Hover { path, .. }
        | LspQuery::Completion { path, .. }
        | LspQuery::CompletionResolve { path, .. }
        | LspQuery::Definition { path, .. }
        | LspQuery::References { path, .. }
        | LspQuery::DocumentSymbols { path }
        | LspQuery::SignatureHelp { path, .. }
        | LspQuery::Format { path, .. } => Some(path),
    }
}

/// Workspace-relative, `/`-separated form of `abs` (unchanged when outside the project, which
/// the host drops from the answer).
fn relative(root: &Path, abs: &Path) -> String {
    match abs.strip_prefix(root) {
        Ok(rel) => rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => abs.display().to_string(),
    }
}

/// Run one query on `manager` and encode the reply as JSON (see [`LspQuery`]).
pub async fn answer(manager: &LspManager, req: LspRequest) -> Result<Vec<u8>, String> {
    use LspQuery as Q;
    let root: PathBuf = manager.root().to_path_buf();
    let abs = |path: &str| -> Result<PathBuf, String> {
        forge_backend::paths::resolve(&root, path).map_err(|e| e.to_string())
    };
    let err = |e: forge_core::ForgeError| e.to_string();
    let relativize = |mut locs: Vec<EditorLocation>| {
        for l in &mut locs {
            l.path = relative(&root, Path::new(&l.path));
        }
        locs
    };
    if let (Some(text), Some(path)) = (&req.text, query_path(&req.query)) {
        manager.did_open(&abs(path)?, text).await.map_err(err)?;
    }
    let at = Position::new;
    match req.query {
        Q::Sync { .. } | Q::Close { .. } => Ok(b"null".to_vec()),
        Q::Hover {
            path,
            line,
            character,
        } => json(
            &manager
                .hover(&abs(&path)?, at(line, character))
                .await
                .map_err(err)?,
        ),
        Q::Completion {
            path,
            line,
            character,
        } => json(
            &manager
                .completion(&abs(&path)?, at(line, character))
                .await
                .map_err(err)?,
        ),
        Q::CompletionResolve { path, item } => {
            let item: serde_json::Value = serde_json::from_str(&item).map_err(|e| e.to_string())?;
            json(
                &manager
                    .completion_resolve(&abs(&path)?, item)
                    .await
                    .map_err(err)?,
            )
        }
        Q::Definition {
            path,
            line,
            character,
        } => json(&relativize(
            manager
                .definition(&abs(&path)?, at(line, character))
                .await
                .map_err(err)?,
        )),
        Q::References {
            path,
            line,
            character,
        } => json(&relativize(
            manager
                .references(&abs(&path)?, at(line, character))
                .await
                .map_err(err)?,
        )),
        Q::DocumentSymbols { path } => {
            json(&manager.document_symbols(&abs(&path)?).await.map_err(err)?)
        }
        Q::SignatureHelp {
            path,
            line,
            character,
        } => json(
            &manager
                .signature_help(&abs(&path)?, at(line, character))
                .await
                .map_err(err)?,
        ),
        Q::Format {
            path,
            tab_size,
            insert_spaces,
        } => json(
            &manager
                .format(&abs(&path)?, tab_size, insert_spaces, None)
                .await
                .map_err(err)?,
        ),
        Q::WorkspaceSymbols { query } => {
            let mut hits = manager.workspace_symbols(&query).await;
            for h in &mut hits {
                h.path = relative(&root, Path::new(&h.path));
            }
            hits.truncate(500);
            json(&hits)
        }
    }
}
