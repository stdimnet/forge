//! A project on another machine, served by the Forge agent over an [`RpcClient`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use forge_core::search::{FindRequest, FindResult};
use forge_core::{FileEntry, FileMatch, Result};
use forge_lsp::launch::{ServerIo, ServerProcess};
use forge_lsp::{LspServerConfig as LspConfig, ServerLauncher};
use forge_process::{CommandLine, PtyEvent, PtySink, RunEvent, RunSink};
use forge_project::ProjectInfo;
use forge_vcs::{GitExec, GitOutput, GitRequest};
use futures_util::future::BoxFuture;
use phpforge_lang_php::generate::{GenerateKind, GenerateOptions, GenerateTargets, TextEdit};
use phpforge_lang_php::hover::Hover;
use phpforge_lang_php::index::{SearchKind, SymbolHit};
use phpforge_lang_php::lsp::{DetectedServers, LspServerConfig, PhpLspSettings};
use phpforge_lang_php::structure::FileStructure;
use phpforge_lang_php::IndexStats;
use serde::de::DeserializeOwned;

use crate::frameworks::FrameworkCall;
use crate::langservers::{LangServersAnswer, LangServersRequest};
use crate::php::ProgressSink;
use crate::protocol::{Call, Event, Opened, PhpCall, TerminalOpened};
use crate::rpc::{connection_lost, RpcClient};
use crate::{
    Backend, BackendInfo, FsSink, PhpBackend, Process, RemoteHost, Terminal, TerminalSpec, Tunnels,
    WatchGuard,
};

/// Default time to wait for an answer.
const TIMEOUT: Duration = Duration::from_secs(60);
/// Network git operations (push, pull, fetch) may take a while.
const GIT_NETWORK_TIMEOUT: Duration = Duration::from_secs(600);
/// A full PHP index of a large project.
const INDEX_TIMEOUT: Duration = Duration::from_secs(3600);
/// A framework scan or language scan of a large project (artisan may run too).
const SCAN_TIMEOUT: Duration = Duration::from_secs(300);

/// The current connection to the agent; it changes when the session reconnects.
pub trait Connection: Send + Sync {
    fn client(&self) -> Result<RpcClient>;

    /// Port forwarding over the connection (SSH tunnels), if it has any.
    fn tunnels(&self) -> Option<Arc<dyn Tunnels>> {
        None
    }
}

impl Connection for RpcClient {
    fn client(&self) -> Result<RpcClient> {
        if self.is_closed() {
            Err(connection_lost())
        } else {
            Ok(self.clone())
        }
    }
}

#[derive(Default)]
struct Routes {
    fs: Option<FsSink>,
    pty: HashMap<u32, PtySink>,
    run: HashMap<u32, RunSink>,
    progress: Option<ProgressSink>,
}

struct Shared {
    info: BackendInfo,
    conn: Arc<dyn Connection>,
    routes: Mutex<Routes>,
}

impl Shared {
    fn client(&self) -> Result<RpcClient> {
        self.conn.client()
    }

    fn call<T: DeserializeOwned>(&self, call: Call, timeout: Duration) -> Result<T> {
        self.client()?.call_blocking(call, timeout)
    }

    fn on_event(&self, event: Event) {
        match event {
            Event::FsChanged(changes) => {
                let sink = self.routes.lock().expect("routes lock").fs.clone();
                if let Some(sink) = sink {
                    sink(changes);
                }
            }
            Event::Pty { channel, event } => {
                let mut routes = self.routes.lock().expect("routes lock");
                let sink = if matches!(event, PtyEvent::Exited { .. }) {
                    routes.pty.remove(&channel)
                } else {
                    routes.pty.get(&channel).cloned()
                };
                drop(routes);
                if let Some(sink) = sink {
                    sink(event);
                }
            }
            Event::Run { channel, event } => {
                let mut routes = self.routes.lock().expect("routes lock");
                let sink = if matches!(event, RunEvent::Exited { .. }) {
                    routes.run.remove(&channel)
                } else {
                    routes.run.get(&channel).cloned()
                };
                drop(routes);
                if let Some(sink) = sink {
                    sink(event);
                }
            }
            Event::PhpIndexProgress { done, total } => {
                let sink = self.routes.lock().expect("routes lock").progress.clone();
                if let Some(sink) = sink {
                    sink(done, total);
                }
            }
        }
    }
}

/// Serves a project folder on a remote host through the Forge agent.
pub struct RemoteBackend {
    shared: Arc<Shared>,
}

fn attach(shared: &Arc<Shared>, client: &RpcClient) {
    let weak: Weak<Shared> = Arc::downgrade(shared);
    client.set_event_handler(Arc::new(move |event| {
        if let Some(shared) = weak.upgrade() {
            shared.on_event(event);
        }
    }));
}

impl RemoteBackend {
    /// Open `root` on the agent behind `conn`. Blocks until the agent answered.
    pub fn open(conn: Arc<dyn Connection>, host: RemoteHost, root: &str) -> Result<Self> {
        let client = conn.client()?;
        let opened: Opened = client.call_blocking(
            Call::Open {
                root: root.to_string(),
            },
            TIMEOUT,
        )?;
        let shared = Arc::new(Shared {
            info: BackendInfo {
                name: opened.name,
                root: opened.root,
                remote: Some(host),
            },
            conn,
            routes: Mutex::new(Routes::default()),
        });
        attach(&shared, &client);
        Ok(Self { shared })
    }

    /// The connection was lost: running terminals and processes ended with it.
    pub fn connection_lost(&self) {
        let (pty, run) = {
            let mut routes = self.shared.routes.lock().expect("routes lock");
            (
                std::mem::take(&mut routes.pty),
                std::mem::take(&mut routes.run),
            )
        };
        for sink in pty.into_values() {
            sink(PtyEvent::Output {
                data: "\r\n\x1b[2m[Connection to the remote host lost]\x1b[0m\r\n".into(),
            });
            sink(PtyEvent::Exited { code: None });
        }
        for sink in run.into_values() {
            sink(RunEvent::Output {
                stream: forge_process::OutputStream::System,
                text: "Connection to the remote host lost\n".into(),
            });
            sink(RunEvent::Exited {
                code: None,
                signal: None,
            });
        }
    }

    /// After a reconnect: open the project on the new agent and resume watching.
    pub fn reattach(&self) -> Result<()> {
        let client = self.shared.client()?;
        let _: Opened = client.call_blocking(
            Call::Open {
                root: self.shared.info.root.to_string_lossy().into_owned(),
            },
            TIMEOUT,
        )?;
        attach(&self.shared, &client);
        let watching = self.shared.routes.lock().expect("routes lock").fs.is_some();
        if watching {
            client.call_blocking::<()>(Call::Watch, TIMEOUT)?;
        }
        Ok(())
    }

    fn php_call<T: DeserializeOwned>(&self, call: PhpCall) -> Result<T> {
        self.shared.call(Call::Php(call), TIMEOUT)
    }
}

struct RemoteGit {
    shared: Arc<Shared>,
}

impl GitExec for RemoteGit {
    fn exec(&self, req: &GitRequest) -> Result<GitOutput> {
        let timeout = if req.non_interactive {
            GIT_NETWORK_TIMEOUT
        } else {
            TIMEOUT
        };
        self.shared.call(Call::Git(req.clone()), timeout)
    }
}

struct RemoteTerminal {
    shared: Arc<Shared>,
    client: RpcClient,
    channel: u32,
    opened: TerminalOpened,
}

impl Terminal for RemoteTerminal {
    fn shell(&self) -> String {
        self.opened.shell.clone()
    }
    fn cwd(&self) -> PathBuf {
        self.opened.cwd.clone()
    }
    fn write(&self, data: &[u8]) -> Result<()> {
        self.client.send_data(self.channel, data.to_vec())
    }
    fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.client.call_blocking(
            Call::TerminalResize {
                channel: self.channel,
                cols,
                rows,
            },
            TIMEOUT,
        )
    }
    fn kill(&self) -> Result<()> {
        self.client.close_channel(self.channel);
        self.shared
            .routes
            .lock()
            .expect("routes lock")
            .pty
            .remove(&self.channel);
        Ok(())
    }
}

struct RemoteProcess {
    client: RpcClient,
    channel: u32,
}

impl Process for RemoteProcess {
    fn write_stdin(&self, data: &[u8]) -> Result<()> {
        self.client.send_data(self.channel, data.to_vec())
    }
    fn close_stdin(&self) -> Result<()> {
        self.client.call_blocking(
            Call::ProcessCloseStdin {
                channel: self.channel,
            },
            TIMEOUT,
        )
    }
    fn kill(&self, grace: Duration) {
        let _ = self.client.call_blocking::<()>(
            Call::ProcessKill {
                channel: self.channel,
                grace_ms: grace.as_millis() as u64,
            },
            TIMEOUT,
        );
    }
}

struct RemoteLauncher {
    shared: Arc<Shared>,
}

struct RemoteServer {
    client: RpcClient,
    channel: u32,
}

impl ServerProcess for RemoteServer {
    fn stop(&mut self, _grace: Duration) -> BoxFuture<'_, ()> {
        // The agent waits for the server to exit and kills it after its own grace period.
        self.client.close_channel(self.channel);
        Box::pin(async {})
    }

    fn kill(&mut self) -> BoxFuture<'_, ()> {
        self.client.close_channel(self.channel);
        Box::pin(async {})
    }
}

impl ServerLauncher for RemoteLauncher {
    fn launch<'a>(
        &'a self,
        config: &'a LspConfig,
        _root: &'a Path,
    ) -> BoxFuture<'a, Result<ServerIo>> {
        Box::pin(async move {
            let client = self.shared.client()?;
            let (channel, rx) = client.open_channel();
            client
                .call::<()>(
                    Call::LspStart {
                        channel,
                        command: config.command.clone(),
                        args: config.args.clone(),
                    },
                    TIMEOUT,
                )
                .await?;
            let (reader, writer) = client.channel_stream(channel, rx);
            Ok(ServerIo {
                stdin: Box::pin(writer),
                stdout: Box::pin(reader),
                stderr: None,
                process: Box::new(RemoteServer { client, channel }),
            })
        })
    }
}

struct StopWatching {
    shared: Weak<Shared>,
}

impl Drop for StopWatching {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            shared.routes.lock().expect("routes lock").fs = None;
        }
    }
}

impl Backend for RemoteBackend {
    fn info(&self) -> &BackendInfo {
        &self.shared.info
    }

    fn list_dir(&self, rel: &str) -> Result<Vec<FileEntry>> {
        self.shared.call(
            Call::ListDir {
                path: rel.to_string(),
            },
            TIMEOUT,
        )
    }

    fn read_file(&self, rel: &str) -> Result<String> {
        self.shared.call(
            Call::ReadFile {
                path: rel.to_string(),
            },
            TIMEOUT,
        )
    }

    fn read_optional(&self, rel: &str) -> Result<Option<String>> {
        self.shared.call(
            Call::ReadOptional {
                path: rel.to_string(),
            },
            TIMEOUT,
        )
    }

    fn write_file(&self, rel: &str, contents: &str) -> Result<()> {
        self.shared.call(
            Call::WriteFile {
                path: rel.to_string(),
                contents: contents.to_string(),
            },
            TIMEOUT,
        )
    }

    fn create_file(&self, rel: &str) -> Result<()> {
        self.shared.call(
            Call::CreateFile {
                path: rel.to_string(),
            },
            TIMEOUT,
        )
    }

    fn create_dir(&self, rel: &str) -> Result<()> {
        self.shared.call(
            Call::CreateDir {
                path: rel.to_string(),
            },
            TIMEOUT,
        )
    }

    fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.shared.call(
            Call::Rename {
                from: from.to_string(),
                to: to.to_string(),
            },
            TIMEOUT,
        )
    }

    fn delete(&self, rel: &str) -> Result<()> {
        self.shared.call(
            Call::Delete {
                path: rel.to_string(),
            },
            TIMEOUT,
        )
    }

    fn find_files(&self, query: &str, limit: usize, exact: bool) -> Result<Vec<FileMatch>> {
        self.shared.call(
            Call::FindFiles {
                query: query.to_string(),
                limit,
                exact,
            },
            TIMEOUT,
        )
    }

    fn find_in_files(&self, req: &FindRequest) -> Result<FindResult> {
        self.shared
            .call(Call::FindInFiles(req.clone()), GIT_NETWORK_TIMEOUT)
    }

    fn find_in_files_found(
        &self,
        req: &FindRequest,
        _cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<crate::Found> {
        let totals: forge_core::search::FindPaged = self.shared.call(
            Call::FindStart {
                req: req.clone(),
                page_size: 0,
            },
            GIT_NETWORK_TIMEOUT,
        )?;
        let (shared, id) = (self.shared.clone(), totals.id);
        let fetch = move |offset: usize, count: usize| {
            shared
                .call::<Vec<forge_core::search::FileMatches>>(
                    Call::FindPage { id, offset, count },
                    GIT_NETWORK_TIMEOUT,
                )
                .unwrap_or_default()
        };
        Ok(crate::Found::Remote {
            totals,
            fetch: Arc::new(fetch),
        })
    }

    fn find_symbols(
        &self,
        query: &str,
        types_only: bool,
        limit: usize,
        skip_extensions: &[String],
        exact: bool,
    ) -> Result<Vec<forge_search_core::SymbolHit>> {
        self.shared.call(
            Call::FindSymbols {
                query: query.to_string(),
                types_only,
                limit,
                skip_extensions: skip_extensions.to_vec(),
                exact,
            },
            TIMEOUT,
        )
    }

    fn search_status(&self) -> Result<crate::SearchStatus> {
        self.shared.call(Call::SearchStatus, TIMEOUT)
    }

    fn git(&self) -> Arc<dyn GitExec> {
        Arc::new(RemoteGit {
            shared: self.shared.clone(),
        })
    }

    fn detect_project(&self) -> Result<ProjectInfo> {
        self.shared.call(Call::Detect, TIMEOUT)
    }

    fn watch(&self, sink: FsSink) -> Result<WatchGuard> {
        self.shared.routes.lock().expect("routes lock").fs = Some(sink);
        self.shared.call::<()>(Call::Watch, TIMEOUT)?;
        Ok(Box::new(StopWatching {
            shared: Arc::downgrade(&self.shared),
        }))
    }

    fn open_terminal(&self, spec: TerminalSpec, sink: PtySink) -> Result<Arc<dyn Terminal>> {
        let client = self.shared.client()?;
        let channel = client.new_channel();
        self.shared
            .routes
            .lock()
            .expect("routes lock")
            .pty
            .insert(channel, sink);
        let opened: Result<TerminalOpened> =
            client.call_blocking(Call::TerminalOpen { channel, spec }, TIMEOUT);
        match opened {
            Ok(opened) => Ok(Arc::new(RemoteTerminal {
                shared: self.shared.clone(),
                client,
                channel,
                opened,
            })),
            Err(e) => {
                self.shared
                    .routes
                    .lock()
                    .expect("routes lock")
                    .pty
                    .remove(&channel);
                Err(e)
            }
        }
    }

    fn start_process(&self, cmd: &CommandLine, sink: RunSink) -> Result<Arc<dyn Process>> {
        let client = self.shared.client()?;
        let channel = client.new_channel();
        self.shared
            .routes
            .lock()
            .expect("routes lock")
            .run
            .insert(channel, sink);
        let started: Result<()> = client.call_blocking(
            Call::ProcessStart {
                channel,
                cmd: cmd.clone(),
            },
            TIMEOUT,
        );
        match started {
            Ok(()) => Ok(Arc::new(RemoteProcess { client, channel })),
            Err(e) => {
                self.shared
                    .routes
                    .lock()
                    .expect("routes lock")
                    .run
                    .remove(&channel);
                Err(e)
            }
        }
    }

    fn lsp_launcher(&self) -> Arc<dyn ServerLauncher> {
        Arc::new(RemoteLauncher {
            shared: self.shared.clone(),
        })
    }

    fn php(&self) -> &dyn PhpBackend {
        self
    }

    fn framework(&self, call: FrameworkCall) -> Result<Vec<u8>> {
        self.shared
            .client()?
            .call_raw_blocking(Call::Framework(call), SCAN_TIMEOUT)
    }

    fn langservers(&self, req: LangServersRequest) -> Result<LangServersAnswer> {
        self.shared.call(Call::LangServers(req), SCAN_TIMEOUT)
    }

    fn find_executable(&self, names: &[String], custom: Option<&str>) -> Result<Option<PathBuf>> {
        self.shared.call(
            Call::FindExecutable {
                names: names.to_vec(),
                custom: custom.map(str::to_string),
            },
            TIMEOUT,
        )
    }

    fn tunnels(&self) -> Option<Arc<dyn Tunnels>> {
        self.shared.conn.tunnels()
    }
}

impl PhpBackend for RemoteBackend {
    fn index_workspace(&self, progress: ProgressSink) -> Result<IndexStats> {
        self.shared.routes.lock().expect("routes lock").progress = Some(progress);
        let result = self.shared.call(Call::Php(PhpCall::Index), INDEX_TIMEOUT);
        self.shared.routes.lock().expect("routes lock").progress = None;
        result
    }

    fn search(
        &self,
        query: &str,
        kind: SearchKind,
        limit: usize,
        exact: bool,
    ) -> Result<Vec<SymbolHit>> {
        self.php_call(PhpCall::Search {
            query: query.to_string(),
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
            path: path.to_string(),
            content,
            inherited,
        })
    }

    fn hover(&self, path: &str, offset: usize, content: Option<String>) -> Result<Option<Hover>> {
        self.php_call(PhpCall::Hover {
            path: path.to_string(),
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
            path: path.to_string(),
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
            path: path.to_string(),
            offset,
            kind,
            options,
            content,
        })
    }

    fn lsp_config(&self, settings: PhpLspSettings) -> Result<Option<LspServerConfig>> {
        self.php_call(PhpCall::LspConfig { settings })
    }

    fn detect_servers(&self, settings: PhpLspSettings) -> Result<DetectedServers> {
        self.php_call(PhpCall::DetectServers { settings })
    }
}
