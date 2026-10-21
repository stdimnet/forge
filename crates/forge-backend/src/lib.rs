//! Project backends: where an open project's files, git, processes, language servers and
//! PHP index live.
//!
//! - [`Backend`] is what the app's workspace, file system, git, terminal, run, LSP and PHP
//!   commands talk to. The UI never knows which implementation answers.
//! - [`LocalBackend`] serves a folder on this machine.
//! - [`RemoteBackend`] serves a folder on another machine through the Forge agent
//!   (`crates/forge-remote-agent`), which is itself a [`LocalBackend`] behind the
//!   [`protocol`]. [`rpc::RpcClient`] multiplexes calls, events and byte channels over any
//!   stream (SSH in `crates/forge-remote`, an in-memory pipe in tests).

pub mod frameworks;
pub mod langservers;
pub mod local;
pub mod paths;
pub mod php;
pub mod process_io;
pub mod protocol;
pub mod remote;
pub mod replace;
pub mod rpc;
pub mod tunnels;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_core::search::{FileMatches, FindPaged, FindRequest, FindResult};
use forge_core::{FileEntry, FileMatch, FsChanges, ProjectFiles, Result};
use forge_lsp::ServerLauncher;
use forge_process::{CommandLine, PtySink, RunSink};
use forge_project::ProjectInfo;
use forge_vcs::{GitExec, Repo};
use serde::{Deserialize, Serialize};

pub use forge_search_core::SymbolHit;
pub use frameworks::{framework_call, FrameworkCall};
pub use langservers::{LangServersAnswer, LangServersRequest};
#[cfg(feature = "search")]
pub use local::IndexOptions;
pub use local::LocalBackend;
pub use php::PhpBackend;
pub use remote::RemoteBackend;
pub use tunnels::{PortForward, Tunnels};

/// A Find in Files result a window pages through.
pub enum Found {
    Full(FindResult),
    #[cfg(feature = "search")]
    Lazy(forge_search::FoundText),
}

impl Found {
    /// The totals with the first `page_size` rows.
    pub fn paged(&self, id: u64, page_size: usize) -> FindPaged {
        match self {
            Found::Full(r) => r.paged(id, page_size),
            #[cfg(feature = "search")]
            Found::Lazy(r) => r.paged(id, page_size),
        }
    }

    /// The rows `[offset, offset + count)`.
    pub fn page(&self, offset: usize, count: usize) -> Vec<FileMatches> {
        match self {
            Found::Full(r) => r.page(offset, count),
            #[cfg(feature = "search")]
            Found::Lazy(r) => r.page(offset, count),
        }
    }
}

/// The machine a remote project lives on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteHost {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// Id of the saved SSH configuration, if the connection came from one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_id: Option<String>,
}

impl RemoteHost {
    /// `user@host` (`user@host:2222` for a non-default port), as shown in the UI.
    pub fn label(&self) -> String {
        if self.port == 22 {
            format!("{}@{}", self.user, self.host)
        } else {
            format!("{}@{}:{}", self.user, self.host, self.port)
        }
    }

    /// A project path on this host as the UI shows it: `user@host:/var/www/shop`.
    pub fn display_path(&self, path: &str) -> String {
        format!("{}@{}:{}", self.user, self.host, path)
    }
}

/// What a backend serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendInfo {
    /// Project name (the root folder's name).
    pub name: String,
    /// Absolute project root on the machine that serves it.
    pub root: PathBuf,
    /// Set for projects on another machine.
    pub remote: Option<RemoteHost>,
}

impl BackendInfo {
    /// The root as the UI shows it: a local path, or `user@host:/path`.
    pub fn display_root(&self) -> String {
        let root = self.root.to_string_lossy();
        match &self.remote {
            Some(r) => r.display_path(&root),
            None => root.into_owned(),
        }
    }
}

/// State of a backend's search index.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchStatus {
    /// The backend keeps a search index at all.
    pub enabled: bool,
    /// The index is being built or refreshed; results may be incomplete.
    pub indexing: bool,
    /// Files the index knows about.
    pub files: usize,
    /// Of those, files whose text is searched.
    #[serde(default)]
    pub content_files: usize,
    /// Binary files, listed by name only.
    #[serde(default)]
    pub binary_files: usize,
    /// Files over the size limit, listed by name only.
    #[serde(default)]
    pub oversized_files: usize,
    /// Unix milliseconds of the last finished refresh; 0 before the first.
    #[serde(default)]
    pub last_refresh_ms: u64,
}

/// How to start a terminal session on the backend.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSpec {
    /// Shell executable; `None` uses the host's default shell.
    #[serde(default)]
    pub shell: Option<String>,
    /// Arguments for `shell`; `None` uses the shell's interactive defaults.
    #[serde(default)]
    pub args: Option<Vec<String>>,
    /// Absolute start directory on the backend's machine; the project root when it does not
    /// exist.
    pub cwd: PathBuf,
    #[serde(default)]
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
}

/// A running terminal session.
pub trait Terminal: Send + Sync {
    fn shell(&self) -> String;
    fn cwd(&self) -> PathBuf;
    fn write(&self, data: &[u8]) -> Result<()>;
    fn resize(&self, cols: u16, rows: u16) -> Result<()>;
    fn kill(&self) -> Result<()>;
}

/// A started process (run configurations, tools).
pub trait Process: Send + Sync {
    fn write_stdin(&self, data: &[u8]) -> Result<()>;
    /// End of input (programs that read their whole stdin, like a CLI reading a prompt).
    fn close_stdin(&self) -> Result<()>;
    /// Stop the process tree: terminate, then kill after `grace`.
    fn kill(&self, grace: std::time::Duration);
}

/// Receives debounced file system changes of the project.
pub type FsSink = Arc<dyn Fn(FsChanges) + Send + Sync>;

/// Keeps a file watcher alive; dropping it stops watching.
pub type WatchGuard = Box<dyn Send + Sync>;

/// Everything the app needs from the place where a project lives. Methods block (remote
/// calls wait for the answer), so call them off the UI thread. Paths are workspace-relative
/// with `/` separators unless stated otherwise.
pub trait Backend: Send + Sync {
    fn info(&self) -> &BackendInfo;

    /// Absolute path on the backend's machine for a workspace-relative path, refusing `..`
    /// escapes and absolute paths.
    fn resolve(&self, rel: &str) -> Result<PathBuf> {
        paths::resolve(&self.info().root, rel)
    }

    /// One directory level: folders first, `.gitignore`d entries flagged (unless tracked).
    fn list_dir(&self, rel: &str) -> Result<Vec<FileEntry>>;
    fn read_file(&self, rel: &str) -> Result<String>;
    /// The file's text, `None` when it does not exist (one round trip for remote projects).
    fn read_optional(&self, rel: &str) -> Result<Option<String>>;
    fn write_file(&self, rel: &str, contents: &str) -> Result<()>;
    /// New empty file (parent folders included); never replaces an existing path.
    fn create_file(&self, rel: &str) -> Result<()>;
    /// New folder (parent folders included); never replaces an existing path.
    fn create_dir(&self, rel: &str) -> Result<()>;
    /// Rename or move a file or folder inside the project.
    fn rename(&self, from: &str, to: &str) -> Result<()>;
    /// Delete a file or a folder with its content.
    fn delete(&self, rel: &str) -> Result<()>;
    /// "Go to File": fuzzy file-name search honoring `.gitignore`.
    /// `exact` matches the query as a substring instead of fuzzy.
    fn find_files(&self, query: &str, limit: usize, exact: bool) -> Result<Vec<FileMatch>>;
    /// Find in Files: text search over the project honoring `.gitignore`.
    fn find_in_files(&self, req: &FindRequest) -> Result<FindResult>;

    /// [`Backend::find_in_files`] for a result that is paged: a local backend with an index keeps
    /// match ranges instead of building every row.
    /// A search that sees `cancel` set stops early; its result is then not meaningful.
    fn find_in_files_found(
        &self,
        req: &FindRequest,
        _cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<Found> {
        self.find_in_files(req).map(Found::Full)
    }

    /// Classes (`types_only`) or every declared symbol matching `query`, best first. Files with
    /// one of `skip_extensions` are left out (languages a language server answers for).
    /// Backends without a search index return nothing.
    fn find_symbols(
        &self,
        _query: &str,
        _types_only: bool,
        _limit: usize,
        _skip_extensions: &[String],
        _exact: bool,
    ) -> Result<Vec<SymbolHit>> {
        Ok(Vec::new())
    }

    /// Whether the search index is ready or still building.
    fn search_status(&self) -> Result<SearchStatus> {
        Ok(SearchStatus::default())
    }

    /// Where git runs for this project.
    fn git(&self) -> Arc<dyn GitExec>;

    /// The git repository containing the project.
    fn repo(&self) -> Result<Repo> {
        Repo::open_with(&self.info().root, self.git())
    }

    /// Languages, frameworks and tools of the project.
    fn detect_project(&self) -> Result<ProjectInfo>;

    /// Watch the project; `sink` gets every debounced batch (the PHP index is already
    /// updated when it runs).
    fn watch(&self, sink: FsSink) -> Result<WatchGuard>;

    fn open_terminal(&self, spec: TerminalSpec, sink: PtySink) -> Result<Arc<dyn Terminal>>;

    /// Start `cmd` (its `cwd` is a path on the backend's machine).
    fn start_process(&self, cmd: &CommandLine, sink: RunSink) -> Result<Arc<dyn Process>>;

    /// Starts language servers next to the project's files.
    fn lsp_launcher(&self) -> Arc<dyn ServerLauncher>;

    /// The project's PHP index and PHP language server lookup.
    fn php(&self) -> &dyn PhpBackend;

    /// Laravel, Symfony and Doctrine analysis, run next to the files. The reply is the
    /// MessagePack encoding of the call's reply type; use [`framework_call`].
    fn framework(&self, call: FrameworkCall) -> Result<Vec<u8>>;

    /// Plan the catalog language servers of the project on the machine that has its files
    /// and the installed servers.
    fn langservers(&self, req: LangServersRequest) -> Result<LangServersAnswer>;

    /// Full path of the first of `names` found on the backend's machine (`PATH` plus the
    /// usual package manager directories). An existing `custom` path wins.
    fn find_executable(&self, names: &[String], custom: Option<&str>) -> Result<Option<PathBuf>>;

    /// SSH tunnels to the project's host; `None` for local projects.
    fn tunnels(&self) -> Option<Arc<dyn Tunnels>> {
        None
    }
}

/// [`ProjectFiles`] of the project a backend serves (settings, data sources, breakpoints of
/// a remote project live on its host).
pub struct BackendFiles(pub Arc<dyn Backend>);

impl ProjectFiles for BackendFiles {
    fn read(&self, rel: &str) -> Result<Option<String>> {
        self.0.read_optional(rel)
    }

    fn write(&self, rel: &str, text: &str) -> Result<()> {
        self.0.write_file(rel, text)
    }

    fn describe(&self, rel: &str) -> String {
        let info = self.0.info();
        let path = info.root.join(rel).to_string_lossy().into_owned();
        match &info.remote {
            Some(r) => r.display_path(&path),
            None => path,
        }
    }
}

/// Name of a project folder (last path component).
pub fn folder_name(root: &Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_paths_are_shown_with_user_and_host() {
        let host = RemoteHost {
            host: "shop.example.com".into(),
            port: 22,
            user: "deploy".into(),
            config_id: None,
        };
        let info = BackendInfo {
            name: "shop".into(),
            root: PathBuf::from("/var/www/shop"),
            remote: Some(host.clone()),
        };
        assert_eq!(info.display_root(), "deploy@shop.example.com:/var/www/shop");
        assert_eq!(host.label(), "deploy@shop.example.com");
        let other = RemoteHost { port: 2222, ..host };
        assert_eq!(other.label(), "deploy@shop.example.com:2222");
    }
}
