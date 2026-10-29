//! [`CollabBackend`]: the host's project as a `forge_backend::Backend`, so the guest's
//! workspace, file system, search, git, PHP and framework commands work unchanged, the way
//! SSH projects do through `RemoteBackend` (`crates/forge-backend/src/remote.rs`).
//!
//! Nothing here is trusted by the host: every method is one [`HostCall`], which the host
//! checks against the guest's permissions before it touches anything.
//!
//! Not available to guests (yet): terminals and processes (S5 shares the host's terminals and
//! runs), language servers (the guest asks the host's servers through [`GuestSession::lsp`]),
//! executables and language server management.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_backend::php::ProgressSink;
use forge_backend::protocol::{decode_value, PhpCall};
use forge_backend::{
    Backend, BackendInfo, CollabHost, FrameworkCall, FsSink, InstallRequest, InstallSink,
    LangServersAnswer, LangServersRequest, PhpBackend, Process, Terminal, TerminalSpec, WatchGuard,
    WebCall,
};
use forge_codeintel::{HighlightAnswer, UsageOptions};
use forge_core::search::{FindRequest, FindResult, TodoRequest, TodoResult};
use forge_core::{CollectRequest, FileEntry, FileMatch, ForgeError, Result, SourceFile};
use forge_langservers::install::InstallOutcome;
use forge_langservers::managed::PackageUpdate;
use forge_lsp::launch::ServerIo;
use forge_lsp::{LspServerConfig as LspConfig, ServerLauncher};
use forge_process::{CommandLine, PtySink, RunSink};
use forge_project::ProjectInfo;
use forge_vcs::{GitExec, GitOutput, GitRequest};
use futures_util::future::BoxFuture;
use phpforge_lang_php::complete::PhpCompletion;
use phpforge_lang_php::generate::{GenerateKind, GenerateOptions, GenerateTargets, TextEdit};
use phpforge_lang_php::hierarchy::{HierarchyQuery, PhpHierarchy};
use phpforge_lang_php::hover::Hover;
use phpforge_lang_php::imports::auto::UnresolvedClass;
use phpforge_lang_php::index::{SearchKind, SymbolHit};
use phpforge_lang_php::injections::LanguagePlace;
use phpforge_lang_php::inspections::PhpProblem;
use phpforge_lang_php::intentions::Intention;
use phpforge_lang_php::lsp::{DetectedServers, LspServerConfig, PhpLspSettings};
use phpforge_lang_php::navigate::{NavKind, NavTarget, PhpDefinition, PhpLineMarker, PhpUsages};
use phpforge_lang_php::project::{LibraryEntry, LibraryRoot, PhpProjectSettings};
use phpforge_lang_php::structure::FileStructure;
use phpforge_lang_php::IndexStats;
use serde::de::DeserializeOwned;
use serde_bytes::ByteBuf;

use super::calls::{HostCall, OpenedProject};
use super::git::{self, GitCall};
use crate::guest::GuestSession;
use crate::host::virtual_root;

/// Default time to wait for the host.
const TIMEOUT: Duration = Duration::from_secs(60);
/// Searches over a large project.
const FIND_TIMEOUT: Duration = Duration::from_secs(120);
/// Network git operations (push, pull, fetch) may take a while.
const GIT_NETWORK_TIMEOUT: Duration = Duration::from_secs(600);
/// A full PHP index of a large project.
const INDEX_TIMEOUT: Duration = Duration::from_secs(3600);
/// A framework scan of a large project.
const SCAN_TIMEOUT: Duration = Duration::from_secs(300);

fn unavailable(what: &str) -> ForgeError {
    ForgeError::Other(format!("{what} is not available in Code With Me sessions"))
}

struct Shared {
    info: BackendInfo,
    session: Arc<GuestSession>,
}

impl Shared {
    fn raw(&self, call: HostCall, timeout: Duration) -> Result<Vec<u8>> {
        self.session
            .call_blocking(&call, timeout)
            .map_err(ForgeError::Other)
    }

    fn call<T: DeserializeOwned>(&self, call: HostCall, timeout: Duration) -> Result<T> {
        decode_value(&self.raw(call, timeout)?)
    }
}

/// The host's project, seen from a guest.
pub struct CollabBackend {
    shared: Arc<Shared>,
}

impl CollabBackend {
    /// Open the host's project of `session`. Blocks until the host answered; call it off the
    /// async runtime.
    pub fn open(session: Arc<GuestSession>) -> Result<Self> {
        let (host_name, label) = session.host();
        let opened: OpenedProject = decode_value(
            &session
                .call_blocking(&HostCall::Open, TIMEOUT)
                .map_err(ForgeError::Other)?,
        )?;
        Ok(Self {
            shared: Arc::new(Shared {
                info: BackendInfo {
                    root: virtual_root(&opened.name),
                    name: opened.name,
                    remote: None,
                    collab: Some(CollabHost {
                        name: host_name,
                        label,
                    }),
                },
                session,
            }),
        })
    }

    /// The session this project belongs to.
    pub fn session(&self) -> &Arc<GuestSession> {
        &self.shared.session
    }
}

struct CollabGit {
    shared: Arc<Shared>,
}

impl GitExec for CollabGit {
    fn exec(&self, req: &GitRequest) -> Result<GitOutput> {
        let (call, timeout) = match git::parse(req).map_err(ForgeError::Other)? {
            GitCall::Query(q) => (HostCall::Git(q), TIMEOUT),
            GitCall::Action(a) => {
                let timeout = if a.non_interactive() {
                    GIT_NETWORK_TIMEOUT
                } else {
                    TIMEOUT
                };
                (HostCall::GitAction(a), timeout)
            }
        };
        self.shared.call(call, timeout)
    }
}

struct NoLauncher;

impl ServerLauncher for NoLauncher {
    fn launch<'a>(
        &'a self,
        _config: &'a LspConfig,
        _root: &'a std::path::Path,
    ) -> BoxFuture<'a, Result<ServerIo>> {
        Box::pin(async {
            Err(ForgeError::Other(
                "Language servers run on the host in Code With Me sessions".into(),
            ))
        })
    }
}

struct StopWatching {
    session: Arc<GuestSession>,
}

impl Drop for StopWatching {
    fn drop(&mut self) {
        self.session.set_fs_sink(None);
    }
}

impl Backend for CollabBackend {
    fn info(&self) -> &BackendInfo {
        &self.shared.info
    }

    fn list_dir(&self, rel: &str) -> Result<Vec<FileEntry>> {
        self.shared
            .call(HostCall::ListDir { path: rel.into() }, TIMEOUT)
    }

    fn read_file(&self, rel: &str) -> Result<String> {
        self.shared
            .call(HostCall::ReadFile { path: rel.into() }, TIMEOUT)
    }

    fn read_optional(&self, rel: &str) -> Result<Option<String>> {
        self.shared
            .call(HostCall::ReadOptional { path: rel.into() }, TIMEOUT)
    }

    fn read_bytes(&self, rel: &str) -> Result<Vec<u8>> {
        let bytes: ByteBuf = self
            .shared
            .call(HostCall::ReadBytes { path: rel.into() }, TIMEOUT)?;
        Ok(bytes.into_vec())
    }

    fn write_bytes(&self, rel: &str, bytes: &[u8]) -> Result<()> {
        self.shared.call(
            HostCall::WriteBytes {
                path: rel.into(),
                bytes: ByteBuf::from(bytes.to_vec()),
            },
            TIMEOUT,
        )
    }

    fn write_file(&self, rel: &str, contents: &str) -> Result<()> {
        self.shared.call(
            HostCall::WriteFile {
                path: rel.into(),
                contents: contents.into(),
            },
            TIMEOUT,
        )
    }

    fn create_file(&self, rel: &str) -> Result<()> {
        self.shared
            .call(HostCall::CreateFile { path: rel.into() }, TIMEOUT)
    }

    fn create_dir(&self, rel: &str) -> Result<()> {
        self.shared
            .call(HostCall::CreateDir { path: rel.into() }, TIMEOUT)
    }

    fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.shared.call(
            HostCall::Rename {
                from: from.into(),
                to: to.into(),
            },
            TIMEOUT,
        )
    }

    fn delete(&self, rel: &str) -> Result<()> {
        self.shared
            .call(HostCall::Delete { path: rel.into() }, TIMEOUT)
    }

    fn find_files(&self, query: &str, limit: usize, exact: bool) -> Result<Vec<FileMatch>> {
        self.shared.call(
            HostCall::FindFiles {
                query: query.into(),
                limit,
                exact,
            },
            FIND_TIMEOUT,
        )
    }

    fn find_in_files(&self, req: &FindRequest) -> Result<FindResult> {
        self.shared
            .call(HostCall::FindInFiles(req.clone()), FIND_TIMEOUT)
    }

    fn find_todos(&self, _req: &TodoRequest) -> Result<TodoResult> {
        Err(unavailable("The TODO tool window"))
    }

    fn collect_files(&self, _req: &CollectRequest) -> Result<Vec<SourceFile>> {
        Err(unavailable("Indexing files"))
    }

    fn list_files(&self, _limit: usize) -> Result<Vec<String>> {
        Err(unavailable("Listing every project file"))
    }

    fn structural_search(&self, _req: &forge_ssr::SsrRequest) -> Result<forge_ssr::SsrResult> {
        Err(unavailable("Structural Search"))
    }

    fn compare_dirs(&self, _left: &str, _right: &str) -> Result<forge_diff::DirCompare> {
        Err(unavailable("Compare Directories"))
    }

    fn git(&self) -> Arc<dyn GitExec> {
        Arc::new(CollabGit {
            shared: self.shared.clone(),
        })
    }

    fn detect_project(&self) -> Result<ProjectInfo> {
        self.shared.call(HostCall::Detect, TIMEOUT)
    }

    fn watch(&self, sink: FsSink) -> Result<WatchGuard> {
        self.shared.session.set_fs_sink(Some(sink));
        self.shared.call::<()>(HostCall::Watch, TIMEOUT)?;
        Ok(Box::new(StopWatching {
            session: self.shared.session.clone(),
        }))
    }

    fn open_terminal(&self, _spec: TerminalSpec, _sink: PtySink) -> Result<Arc<dyn Terminal>> {
        Err(unavailable("Terminals"))
    }

    fn start_process(&self, _cmd: &CommandLine, _sink: RunSink) -> Result<Arc<dyn Process>> {
        Err(unavailable("Running programs"))
    }

    fn lsp_launcher(&self) -> Arc<dyn ServerLauncher> {
        Arc::new(NoLauncher)
    }

    fn php(&self) -> &dyn PhpBackend {
        self
    }

    fn framework(&self, call: FrameworkCall) -> Result<Vec<u8>> {
        self.shared.raw(HostCall::Framework(call), SCAN_TIMEOUT)
    }

    fn langservers(&self, _req: LangServersRequest) -> Result<LangServersAnswer> {
        Err(unavailable("Managing language servers"))
    }

    fn install_langserver(
        &self,
        _req: InstallRequest,
        _cancel: Arc<tokio::sync::Notify>,
        _sink: InstallSink,
    ) -> Result<InstallOutcome> {
        Err(unavailable("Installing language servers"))
    }

    fn langserver_updates(
        &self,
        _registry: Option<String>,
        _node_mirror: Option<String>,
        _force: bool,
    ) -> Result<Option<Vec<PackageUpdate>>> {
        Err(unavailable("Checking language server updates"))
    }

    fn web(&self, _call: WebCall) -> Result<Vec<u8>> {
        Err(unavailable("WebForge analysis"))
    }

    fn find_executable(&self, _names: &[String], _custom: Option<&str>) -> Result<Option<PathBuf>> {
        Ok(None)
    }
}

impl CollabBackend {
    fn php_call<T: DeserializeOwned>(&self, call: PhpCall) -> Result<T> {
        self.shared.call(HostCall::Php(call), TIMEOUT)
    }
}

impl PhpBackend for CollabBackend {
    fn index_workspace(&self, _progress: ProgressSink) -> Result<IndexStats> {
        self.shared
            .call(HostCall::Php(PhpCall::Index), INDEX_TIMEOUT)
    }

    fn search(
        &self,
        query: &str,
        kind: SearchKind,
        limit: usize,
        exact: bool,
    ) -> Result<Vec<SymbolHit>> {
        self.php_call(PhpCall::Search {
            query: query.into(),
            kind,
            limit,
            exact,
        })
    }

    fn file_structure(
        &self,
        path: &str,
        content: Option<String>,
        inherited: bool,
    ) -> Result<FileStructure> {
        self.php_call(PhpCall::Structure {
            path: path.into(),
            content,
            inherited,
        })
    }

    fn hover(&self, path: &str, offset: usize, content: Option<String>) -> Result<Option<Hover>> {
        self.php_call(PhpCall::Hover {
            path: path.into(),
            offset,
            content,
        })
    }

    fn generate_targets(
        &self,
        path: &str,
        offset: usize,
        content: Option<String>,
    ) -> Result<GenerateTargets> {
        self.php_call(PhpCall::GenerateTargets {
            path: path.into(),
            offset,
            content,
        })
    }

    fn generate(
        &self,
        path: &str,
        offset: usize,
        kind: GenerateKind,
        options: GenerateOptions,
        content: Option<String>,
    ) -> Result<Vec<TextEdit>> {
        self.php_call(PhpCall::Generate {
            path: path.into(),
            offset,
            kind,
            options,
            content,
        })
    }

    fn usages(
        &self,
        path: &str,
        offset: usize,
        options: UsageOptions,
        content: Option<String>,
    ) -> Result<Option<PhpUsages>> {
        self.php_call(PhpCall::Usages {
            path: path.into(),
            offset,
            options,
            content,
        })
    }

    fn highlights(
        &self,
        path: &str,
        offset: usize,
        content: Option<String>,
    ) -> Result<Option<HighlightAnswer>> {
        self.php_call(PhpCall::Highlights {
            path: path.into(),
            offset,
            content,
        })
    }

    fn navigate(
        &self,
        path: &str,
        offset: usize,
        kind: NavKind,
        content: Option<String>,
    ) -> Result<Vec<NavTarget>> {
        self.php_call(PhpCall::Navigate {
            path: path.into(),
            offset,
            kind,
            content,
        })
    }

    fn definition(
        &self,
        path: &str,
        offset: usize,
        content: Option<String>,
    ) -> Result<PhpDefinition> {
        self.php_call(PhpCall::Definition {
            path: path.into(),
            offset,
            content,
        })
    }

    fn line_markers(&self, path: &str, content: Option<String>) -> Result<Vec<PhpLineMarker>> {
        self.php_call(PhpCall::LineMarkers {
            path: path.into(),
            content,
        })
    }

    fn hierarchy(
        &self,
        path: &str,
        offset: usize,
        query: HierarchyQuery,
        content: Option<String>,
    ) -> Result<PhpHierarchy> {
        self.php_call(PhpCall::Hierarchy {
            path: path.into(),
            offset,
            query,
            content,
        })
    }

    fn intentions(
        &self,
        path: &str,
        offset: usize,
        options: GenerateOptions,
        content: Option<String>,
    ) -> Result<Vec<Intention>> {
        self.php_call(PhpCall::Intentions {
            path: path.into(),
            offset,
            options,
            content,
        })
    }

    /// The host runs the language server; a guest has none to configure.
    fn lsp_config(&self, _settings: PhpLspSettings) -> Result<Option<LspServerConfig>> {
        Ok(None)
    }

    fn detect_servers(&self, _settings: PhpLspSettings) -> Result<DetectedServers> {
        Ok(DetectedServers::default())
    }

    fn unresolved_classes(
        &self,
        path: &str,
        content: Option<String>,
    ) -> Result<Vec<UnresolvedClass>> {
        self.php_call(PhpCall::UnresolvedClasses {
            path: path.into(),
            content,
        })
    }

    /// The host's include paths and source roots apply; a guest's settings do not.
    fn configure(&self, _settings: PhpProjectSettings) -> Result<()> {
        Ok(())
    }

    fn inspect(&self, path: &str, content: Option<String>) -> Result<Vec<PhpProblem>> {
        self.php_call(PhpCall::Inspect {
            path: path.into(),
            content,
        })
    }

    fn complete(
        &self,
        path: &str,
        offset: usize,
        content: Option<String>,
    ) -> Result<Option<PhpCompletion>> {
        self.php_call(PhpCall::Complete {
            path: path.into(),
            offset,
            content,
        })
    }

    fn language_places(&self) -> Result<Vec<LanguagePlace>> {
        self.php_call(PhpCall::LanguagePlaces)
    }

    /// External libraries live outside the project and are not shared.
    fn libraries(&self) -> Result<Vec<LibraryRoot>> {
        Ok(Vec::new())
    }

    fn library_list(&self, _path: &str) -> Result<Vec<LibraryEntry>> {
        Err(unavailable("External Libraries"))
    }

    fn library_read(&self, _path: &str) -> Result<String> {
        Err(unavailable("External Libraries"))
    }
}
