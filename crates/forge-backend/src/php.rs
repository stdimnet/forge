//! The PHP index of a project and the PHP language server lookup, wherever the project lives.
//! Offsets are UTF-16 (JS strings), see `phpforge_lang_php::service`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use forge_core::{ForgeError, FsChanges, Result, Workspace};
use phpforge_lang_php::generate::{GenerateKind, GenerateOptions, GenerateTargets, TextEdit};
use phpforge_lang_php::hover::Hover;
use phpforge_lang_php::index::VendorMode;
use phpforge_lang_php::index::{SearchKind, SymbolHit};
use phpforge_lang_php::intentions::Intention;
use phpforge_lang_php::lsp::{self, DetectedServers, LspServerConfig, PhpLspSettings};
use phpforge_lang_php::navigate::{NavKind, NavTarget, PhpDefinition, PhpLineMarker, PhpUsages};
use phpforge_lang_php::service;
use phpforge_lang_php::structure::FileStructure;
use phpforge_lang_php::{IndexConfig, IndexStats, ProjectIndex};

/// Index progress callback: `(done, total)` files.
pub type ProgressSink = Arc<dyn Fn(usize, usize) + Send + Sync>;

/// PHP services of a project. `content` is the editor buffer; `None` reads the file.
pub trait PhpBackend: Send + Sync {
    /// Index the whole project (incremental through the cache) and save the cache.
    fn index_workspace(&self, progress: ProgressSink) -> Result<IndexStats>;
    fn search(&self, query: &str, kind: SearchKind, limit: usize) -> Result<Vec<SymbolHit>>;
    fn file_structure(
        &self,
        path: &str,
        content: Option<String>,
        inherited: bool,
    ) -> Result<FileStructure>;
    fn hover(&self, path: &str, offset: usize, content: Option<String>) -> Result<Option<Hover>>;
    fn generate_targets(
        &self,
        path: &str,
        offset: usize,
        content: Option<String>,
    ) -> Result<GenerateTargets>;
    fn generate(
        &self,
        path: &str,
        offset: usize,
        kind: GenerateKind,
        options: GenerateOptions,
        content: Option<String>,
    ) -> Result<Vec<TextEdit>>;
    /// Find Usages of the symbol at `offset` from the index (when no language server answers).
    fn usages(
        &self,
        path: &str,
        offset: usize,
        content: Option<String>,
    ) -> Result<Option<PhpUsages>>;
    /// Go to Implementation / Super Method / Type Declaration from the index.
    fn navigate(
        &self,
        path: &str,
        offset: usize,
        kind: NavKind,
        content: Option<String>,
    ) -> Result<Vec<NavTarget>>;
    /// Go to Declaration from the index: the target, "on a declaration" (Show Usages) or
    /// the name to look up in the PHP stubs.
    fn definition(
        &self,
        path: &str,
        offset: usize,
        content: Option<String>,
    ) -> Result<PhpDefinition>;
    /// The implemented / overridden gutter markers of a file.
    fn line_markers(&self, path: &str, content: Option<String>) -> Result<Vec<PhpLineMarker>>;
    /// Built-in intentions for Show Context Actions (Alt+Enter).
    fn intentions(
        &self,
        path: &str,
        offset: usize,
        content: Option<String>,
    ) -> Result<Vec<Intention>>;
    /// The language server to start for the project (Intelephense or phpactor found on the
    /// project's machine), according to `settings`.
    fn lsp_config(&self, settings: PhpLspSettings) -> Result<Option<LspServerConfig>>;
    /// Language servers installed on the project's machine.
    fn detect_servers(&self, settings: PhpLspSettings) -> Result<DetectedServers>;
}

fn err(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

/// The PHP index of a local folder (created lazily from its cache).
pub struct LocalPhp {
    workspace: Workspace,
    index: Mutex<Option<ProjectIndex>>,
    /// Vendor packages as libraries ("Add packages as libraries" of the Composer settings,
    /// which reach the backend with [`PhpBackend::lsp_config`]).
    vendor: Mutex<VendorMode>,
}

impl LocalPhp {
    pub fn new(workspace: Workspace) -> Self {
        Self {
            workspace,
            index: Mutex::new(None),
            vendor: Mutex::new(VendorMode::default()),
        }
    }

    fn root(&self) -> &Path {
        self.workspace.root()
    }

    fn fresh_index(&self) -> ProjectIndex {
        let config = IndexConfig {
            vendor: *self.vendor.lock().unwrap_or_else(|e| e.into_inner()),
            ..IndexConfig::default()
        };
        let mut idx = ProjectIndex::new(self.root(), config);
        idx.load_cache();
        idx
    }

    fn with_index<T>(&self, f: impl FnOnce(&mut ProjectIndex) -> T) -> Result<T> {
        let mut guard = self.index.lock().map_err(err)?;
        if guard.is_none() {
            *guard = Some(self.fresh_index());
        }
        Ok(f(guard.as_mut().expect("index initialised above")))
    }

    fn source(&self, path: &str, content: Option<String>) -> Result<String> {
        match content {
            Some(c) => Ok(c),
            None => self.workspace.read_file(path),
        }
    }

    /// Keep an existing index current when files change on disk. Only an index that was
    /// already loaded is updated; the next full index picks up everything else.
    pub fn apply_fs_changes(&self, changes: &FsChanges) {
        if changes.changed.is_empty() && changes.removed.is_empty() {
            return;
        }
        let Ok(mut guard) = self.index.lock() else {
            return;
        };
        if let Some(idx) = guard.as_mut() {
            let n = idx.apply_changes(&changes.changed, &changes.removed);
            if n > 0 {
                tracing::debug!("PHP index: {n} files updated from disk");
            }
        }
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

impl PhpBackend for LocalPhp {
    fn index_workspace(&self, progress: ProgressSink) -> Result<IndexStats> {
        // Take the index out so queries are not blocked while indexing.
        let previous = self.index.lock().map_err(err)?.take();
        let mut idx = previous.unwrap_or_else(|| self.fresh_index());
        let stats = idx.index_workspace(|done, total| progress(done, total));
        if let Err(e) = idx.save_cache() {
            tracing::warn!("could not save PHP index cache: {e}");
        }
        *self.index.lock().map_err(err)? = Some(idx);
        Ok(stats)
    }

    fn search(&self, query: &str, kind: SearchKind, limit: usize) -> Result<Vec<SymbolHit>> {
        self.with_index(|idx| idx.search(query, kind, limit))
    }

    fn file_structure(
        &self,
        path: &str,
        content: Option<String>,
        inherited: bool,
    ) -> Result<FileStructure> {
        let text = self.source(path, content)?;
        self.with_index(|idx| service::file_structure(idx, path, &text, inherited))
    }

    fn hover(&self, path: &str, offset: usize, content: Option<String>) -> Result<Option<Hover>> {
        let text = self.source(path, content)?;
        self.with_index(|idx| service::hover(idx, path, &text, offset))
    }

    fn generate_targets(
        &self,
        path: &str,
        offset: usize,
        content: Option<String>,
    ) -> Result<GenerateTargets> {
        let text = self.source(path, content)?;
        self.with_index(|idx| service::generate_targets(idx, path, &text, offset))
    }

    fn generate(
        &self,
        path: &str,
        offset: usize,
        kind: GenerateKind,
        options: GenerateOptions,
        content: Option<String>,
    ) -> Result<Vec<TextEdit>> {
        let text = self.source(path, content)?;
        self.with_index(|idx| service::generate(idx, path, &text, offset, kind, &options))?
            .map_err(err)
    }

    fn usages(
        &self,
        path: &str,
        offset: usize,
        content: Option<String>,
    ) -> Result<Option<PhpUsages>> {
        let text = self.source(path, content)?;
        self.with_index(|idx| service::usages(idx, path, &text, offset))
    }

    fn navigate(
        &self,
        path: &str,
        offset: usize,
        kind: NavKind,
        content: Option<String>,
    ) -> Result<Vec<NavTarget>> {
        let text = self.source(path, content)?;
        self.with_index(|idx| service::navigate(idx, path, &text, offset, kind))
    }

    fn definition(
        &self,
        path: &str,
        offset: usize,
        content: Option<String>,
    ) -> Result<PhpDefinition> {
        let text = self.source(path, content)?;
        self.with_index(|idx| service::definition(idx, path, &text, offset))
    }

    fn line_markers(&self, path: &str, content: Option<String>) -> Result<Vec<PhpLineMarker>> {
        let text = self.source(path, content)?;
        self.with_index(|idx| service::line_markers(idx, path, &text))
    }

    fn intentions(
        &self,
        path: &str,
        offset: usize,
        content: Option<String>,
    ) -> Result<Vec<Intention>> {
        let text = self.source(path, content)?;
        self.with_index(|idx| service::intentions(idx, path, &text, offset))
    }

    fn lsp_config(&self, settings: PhpLspSettings) -> Result<Option<LspServerConfig>> {
        let mode = if settings.vendor_libraries {
            VendorMode::Lazy
        } else {
            VendorMode::Skip
        };
        let changed = {
            let mut vendor = self.vendor.lock().map_err(err)?;
            std::mem::replace(&mut *vendor, mode) != mode
        };
        if changed {
            // Built again with the new mode on the next use.
            *self.index.lock().map_err(err)? = None;
        }
        Ok(lsp::resolve_for_workspace(&settings, self.root()))
    }

    fn detect_servers(&self, settings: PhpLspSettings) -> Result<DetectedServers> {
        let path_env = std::env::var("PATH").ok();
        let dirs = lsp::search_dirs(path_env.as_deref(), home_dir().as_deref());
        Ok(lsp::detect(&settings, &dirs, &lsp::FsProbe))
    }
}
