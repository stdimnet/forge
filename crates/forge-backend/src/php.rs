//! The PHP index of a project and the PHP language server lookup, wherever the project lives.
//! Offsets are UTF-16 (JS strings), see `phpforge_lang_php::service`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use forge_codeintel::{HighlightAnswer, UsageOptions};
use forge_core::{ForgeError, FsChanges, Result, Workspace};
use phpforge_lang_php::complete::PhpCompletion;
use phpforge_lang_php::generate::{GenerateKind, GenerateOptions, GenerateTargets, TextEdit};
use phpforge_lang_php::hierarchy::{HierarchyQuery, PhpHierarchy};
use phpforge_lang_php::hover::Hover;
use phpforge_lang_php::imports::auto::{self, UnresolvedClass};
use phpforge_lang_php::index::VendorMode;
use phpforge_lang_php::index::{SearchKind, SymbolHit};
use phpforge_lang_php::injections::LanguagePlace;
use phpforge_lang_php::inspections::PhpProblem;
use phpforge_lang_php::intentions::Intention;
use phpforge_lang_php::lsp::{self, DetectedServers, LspServerConfig, PhpLspSettings};
use phpforge_lang_php::navigate::{NavKind, NavTarget, PhpDefinition, PhpLineMarker, PhpUsages};
use phpforge_lang_php::project::{self, LibraryEntry, LibraryRoot, PhpProjectSettings};
use phpforge_lang_php::service;
use phpforge_lang_php::structure::FileStructure;
use phpforge_lang_php::{IndexConfig, IndexStats, ProjectIndex};

/// Index progress callback: `(done, total)` files.
pub type ProgressSink = Arc<dyn Fn(usize, usize) + Send + Sync>;

/// PHP services of a project. `content` is the editor buffer; `None` reads the file.
pub trait PhpBackend: Send + Sync {
    /// Index the whole project (incremental through the cache) and save the cache.
    fn index_workspace(&self, progress: ProgressSink) -> Result<IndexStats>;
    fn search(
        &self,
        query: &str,
        kind: SearchKind,
        limit: usize,
        exact: bool,
    ) -> Result<Vec<SymbolHit>>;
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
    /// Find Usages of the symbol at `offset` from the index (when no language server answers),
    /// with the options of the Find Usages dialog.
    fn usages(
        &self,
        path: &str,
        offset: usize,
        options: UsageOptions,
        content: Option<String>,
    ) -> Result<Option<PhpUsages>>;
    /// Highlight Usages in File: the usages of the symbol at `offset` in its file (or the
    /// methods implementing / overriding the parents on `implements` / `extends`).
    fn highlights(
        &self,
        path: &str,
        offset: usize,
        content: Option<String>,
    ) -> Result<Option<HighlightAnswer>>;
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
    /// Type, Method or Call Hierarchy from the index (when no language server answers).
    fn hierarchy(
        &self,
        path: &str,
        offset: usize,
        query: HierarchyQuery,
        content: Option<String>,
    ) -> Result<PhpHierarchy>;
    /// Built-in intentions for Show Context Actions (Alt+Enter), including Create from
    /// usage; `options` carries the code style and code templates for generated code.
    fn intentions(
        &self,
        path: &str,
        offset: usize,
        options: GenerateOptions,
        content: Option<String>,
    ) -> Result<Vec<Intention>>;
    /// Class names of a file that resolve to no known class, with the classes of the index
    /// (project and vendor) an import could bring in (Auto Import, "Import class").
    fn unresolved_classes(
        &self,
        path: &str,
        content: Option<String>,
    ) -> Result<Vec<UnresolvedClass>>;
    /// The language server to start for the project (Intelephense or phpactor found on the
    /// project's machine), according to `settings`.
    fn lsp_config(&self, settings: PhpLspSettings) -> Result<Option<LspServerConfig>>;
    /// Language servers installed on the project's machine.
    fn detect_servers(&self, settings: PhpLspSettings) -> Result<DetectedServers>;
    /// Include paths and source roots; a change of the include paths indexes again on the
    /// next [`PhpBackend::index_workspace`].
    fn configure(&self, settings: PhpProjectSettings) -> Result<()>;
    /// Forge's PHP inspections of a file (metadata, JetBrains attributes, PSR-4 namespace).
    fn inspect(&self, path: &str, content: Option<String>) -> Result<Vec<PhpProblem>>;
    /// Completion of expected arguments / return values and array shape keys.
    fn complete(
        &self,
        path: &str,
        offset: usize,
        content: Option<String>,
    ) -> Result<Option<PhpCompletion>>;
    /// Parameters marked `#[Language]` (for the language injections).
    fn language_places(&self) -> Result<Vec<LanguagePlace>>;
    /// The include paths ("External Libraries").
    fn libraries(&self) -> Result<Vec<LibraryRoot>>;
    /// The entries of a folder inside an include path.
    fn library_list(&self, path: &str) -> Result<Vec<LibraryEntry>>;
    /// A file inside an include path.
    fn library_read(&self, path: &str) -> Result<String>;
}

/// Use Forge's managed Intelephense (`node …/intelephense.js`) of this machine unless a path
/// is set. On a remote agent "this machine" is the project's host, where Forge installs it.
pub fn with_managed_intelephense(mut settings: PhpLspSettings) -> PhpLspSettings {
    if settings.intelephense_path.is_none() {
        if let Some((node, script)) =
            forge_langservers::Detector::system().managed_launch("intelephense")
        {
            settings.intelephense_path = Some(script);
            settings.intelephense_node = Some(node);
        }
    }
    settings
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
    /// Include paths and source roots ([`PhpBackend::configure`]).
    project: Mutex<PhpProjectSettings>,
}

impl LocalPhp {
    pub fn new(workspace: Workspace) -> Self {
        Self {
            workspace,
            index: Mutex::new(None),
            vendor: Mutex::new(VendorMode::default()),
            project: Mutex::new(PhpProjectSettings::default()),
        }
    }

    fn project_settings(&self) -> PhpProjectSettings {
        self.project
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn include_dirs(&self) -> Vec<PathBuf> {
        self.project_settings()
            .include_dirs(self.root(), home_dir().as_deref())
    }

    /// An absolute path inside an include path.
    fn library_path(&self, path: &str) -> Result<PathBuf> {
        let p = PathBuf::from(path);
        if p.is_absolute() && project::inside_library(&self.include_dirs(), &p) {
            Ok(p)
        } else {
            Err(ForgeError::Other(format!(
                "not inside an include path: {path}"
            )))
        }
    }

    fn root(&self) -> &Path {
        self.workspace.root()
    }

    fn fresh_index(&self) -> ProjectIndex {
        let settings = self.project_settings();
        let config = IndexConfig {
            vendor: *self.vendor.lock().unwrap_or_else(|e| e.into_inner()),
            include_paths: settings.include_dirs(self.root(), home_dir().as_deref()),
            ..IndexConfig::default()
        };
        let mut idx = ProjectIndex::new(self.root(), config);
        idx.set_structure(self.workspace.excludes().config());
        idx.load_cache();
        idx.set_source_roots(settings.effective_roots(self.root()));
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
        // composer.json may have changed since.
        idx.set_source_roots(self.project_settings().effective_roots(self.root()));
        // Excluded folders may have changed since (Settings | Directories).
        idx.set_structure(self.workspace.excludes().config());
        let stats = idx.index_workspace(|done, total| progress(done, total));
        if let Err(e) = idx.save_cache() {
            tracing::warn!("could not save PHP index cache: {e}");
        }
        *self.index.lock().map_err(err)? = Some(idx);
        Ok(stats)
    }

    fn search(
        &self,
        query: &str,
        kind: SearchKind,
        limit: usize,
        exact: bool,
    ) -> Result<Vec<SymbolHit>> {
        self.with_index(|idx| idx.search_with(query, kind, limit, exact))
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
        options: UsageOptions,
        content: Option<String>,
    ) -> Result<Option<PhpUsages>> {
        let text = self.source(path, content)?;
        self.with_index(|idx| service::usages(idx, path, &text, offset, &options))
    }

    fn highlights(
        &self,
        path: &str,
        offset: usize,
        content: Option<String>,
    ) -> Result<Option<HighlightAnswer>> {
        let text = self.source(path, content)?;
        self.with_index(|idx| service::highlights(idx, path, &text, offset))
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

    fn hierarchy(
        &self,
        path: &str,
        offset: usize,
        query: HierarchyQuery,
        content: Option<String>,
    ) -> Result<PhpHierarchy> {
        let text = self.source(path, content)?;
        self.with_index(|idx| service::hierarchy(idx, path, &text, offset, query))
    }

    fn intentions(
        &self,
        path: &str,
        offset: usize,
        options: GenerateOptions,
        content: Option<String>,
    ) -> Result<Vec<Intention>> {
        let text = self.source(path, content)?;
        self.with_index(|idx| service::intentions(idx, path, &text, offset, &options))
    }

    fn unresolved_classes(
        &self,
        path: &str,
        content: Option<String>,
    ) -> Result<Vec<UnresolvedClass>> {
        let text = self.source(path, content)?;
        self.with_index(|idx| auto::unresolved_classes(idx, path, &text))
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
        Ok(lsp::resolve_for_workspace(
            &with_managed_intelephense(settings),
            self.root(),
        ))
    }

    fn detect_servers(&self, settings: PhpLspSettings) -> Result<DetectedServers> {
        let path_env = std::env::var("PATH").ok();
        let dirs = lsp::search_dirs(path_env.as_deref(), home_dir().as_deref());
        Ok(lsp::detect(
            &with_managed_intelephense(settings),
            &dirs,
            &lsp::FsProbe,
        ))
    }

    fn configure(&self, settings: PhpProjectSettings) -> Result<()> {
        let previous = std::mem::replace(&mut *self.project.lock().map_err(err)?, settings.clone());
        if previous.include_paths != settings.include_paths {
            // Built again with the new include paths on the next use.
            *self.index.lock().map_err(err)? = None;
        } else if let Some(idx) = self.index.lock().map_err(err)?.as_mut() {
            idx.set_source_roots(settings.effective_roots(self.root()));
        }
        Ok(())
    }

    fn inspect(&self, path: &str, content: Option<String>) -> Result<Vec<PhpProblem>> {
        let text = self.source(path, content)?;
        self.with_index(|idx| service::inspect(idx, path, &text))
    }

    fn complete(
        &self,
        path: &str,
        offset: usize,
        content: Option<String>,
    ) -> Result<Option<PhpCompletion>> {
        let text = self.source(path, content)?;
        self.with_index(|idx| service::complete(idx, path, &text, offset))
    }

    fn language_places(&self) -> Result<Vec<LanguagePlace>> {
        self.with_index(|idx| service::language_places(idx))
    }

    fn libraries(&self) -> Result<Vec<LibraryRoot>> {
        Ok(project::library_roots(&self.include_dirs()))
    }

    fn library_list(&self, path: &str) -> Result<Vec<LibraryEntry>> {
        let p = self.library_path(path)?;
        project::list_library(&self.include_dirs(), &p)
            .ok_or_else(|| ForgeError::Other(format!("cannot list {path}")))
    }

    fn library_read(&self, path: &str) -> Result<String> {
        let p = self.library_path(path)?;
        project::read_library(&self.include_dirs(), &p)
            .ok_or_else(|| ForgeError::Other(format!("cannot read {path}")))
    }
}
