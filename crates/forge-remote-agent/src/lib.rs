//! The Forge agent: a headless [`LocalBackend`] behind the agent
//! [`protocol`](forge_backend::protocol), run on a remote host over SSH (stdin/stdout).
//!
//! The desktop app uploads this binary to `~/.cache/forge-agent/<version>/`, starts
//! `forge-remote-agent serve` and talks to it with [`forge_backend::rpc::RpcClient`]. Every
//! service runs next to the project's files: file system, file search, git, terminals,
//! processes, language servers (Intelephense indexes the remote project itself), language
//! server installs, the PHP index and the framework and WebForge analysis.

mod browse;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use forge_backend::protocol::{
    encode_value, io, Call, Event, Frame, Hello, Opened, PhpCall, TerminalOpened, PROTOCOL_VERSION,
};
use forge_backend::{langservers, Backend, LocalBackend, Process, Terminal, WatchGuard};
use forge_core::{ForgeError, FsChanges, Result};
use forge_lsp::launch::spawn_local;
use forge_process::{PtyEvent, PtySink, RunEvent, RunSink};
use forge_vcs::{GitExec, LocalGit};
use serde::Serialize;
use serde_bytes::ByteBuf;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;

pub use browse::browse;

/// Version of this agent; the client uploads a new binary when it differs.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What `forge-remote-agent --version` prints.
pub fn version_line() -> String {
    format!("forge-remote-agent {VERSION} protocol {PROTOCOL_VERSION}")
}

type Input = mpsc::UnboundedSender<Vec<u8>>;

enum Channel {
    Terminal(Arc<dyn Terminal>, Input),
    Process(Arc<dyn Process>, Input),
    Lsp(Input),
    /// A language server install; notified to cancel it.
    Install(Arc<tokio::sync::Notify>),
}

impl Channel {
    fn input(&self) -> Option<&Input> {
        match self {
            Channel::Terminal(_, i) | Channel::Process(_, i) | Channel::Lsp(i) => Some(i),
            Channel::Install(_) => None,
        }
    }

    /// Stop whatever runs on the channel.
    fn stop(self) {
        match self {
            Channel::Terminal(t, _) => {
                let _ = t.kill();
            }
            Channel::Process(p, _) => p.kill(Duration::from_secs(2)),
            // Dropping the input ends the stdin writer, which stops the server.
            Channel::Lsp(_) => {}
            Channel::Install(cancel) => cancel.notify_one(),
        }
    }
}

struct Agent {
    out: mpsc::UnboundedSender<Frame>,
    backend: RwLock<Option<Arc<LocalBackend>>>,
    watch: Mutex<Option<WatchGuard>>,
    channels: Mutex<HashMap<u32, Channel>>,
    /// The result the last `FindStart` kept, for `FindPage`.
    found: Mutex<Option<(u64, Arc<forge_backend::Found>)>>,
    next_found: std::sync::atomic::AtomicU64,
}

fn other(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

fn reply<T: Serialize>(value: T) -> Result<Vec<u8>> {
    encode_value(&value)
}

impl Agent {
    fn send(&self, frame: Frame) {
        let _ = self.out.send(frame);
    }

    fn event(&self, event: Event) {
        self.send(Frame::Event(event));
    }

    fn backend(&self) -> Result<Arc<LocalBackend>> {
        self.backend
            .read()
            .expect("backend lock")
            .clone()
            .ok_or_else(|| ForgeError::Other("no project is open on the remote host".into()))
    }

    fn add_channel(&self, id: u32, channel: Channel) {
        let old = self
            .channels
            .lock()
            .expect("channels lock")
            .insert(id, channel);
        if let Some(old) = old {
            old.stop();
        }
    }

    /// Feed a channel's input to `write` in order, off the async runtime.
    fn input_pump(write: impl Fn(&[u8]) -> Result<()> + Send + Sync + 'static) -> Input {
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let write = Arc::new(write);
        tokio::spawn(async move {
            while let Some(data) = rx.recv().await {
                let w = write.clone();
                let ok = tokio::task::spawn_blocking(move || w(&data))
                    .await
                    .map(|r| r.is_ok())
                    .unwrap_or(false);
                if !ok {
                    break;
                }
            }
        });
        tx
    }

    /// Answer one call (blocking: runs on a blocking thread).
    fn handle(self: &Arc<Self>, call: Call) -> Result<Vec<u8>> {
        match call {
            Call::Hello { protocol, .. } => {
                if protocol != PROTOCOL_VERSION {
                    return Err(ForgeError::Other(format!(
                        "the agent speaks protocol {PROTOCOL_VERSION}, the IDE {protocol}"
                    )));
                }
                reply(Hello {
                    version: VERSION.into(),
                    protocol: PROTOCOL_VERSION,
                    os: std::env::consts::OS.into(),
                    arch: std::env::consts::ARCH.into(),
                    home: dirs::home_dir()
                        .map(|h| h.display().to_string())
                        .unwrap_or_default(),
                    hostname: hostname(),
                })
            }
            Call::Ping => reply(()),
            Call::Browse { path } => reply(browse(&path)?),
            Call::Open { root } => {
                let backend = LocalBackend::open(browse::expand_home(&root))?;
                #[cfg(feature = "search")]
                let backend = backend.with_search_index(forge_backend::IndexOptions::default());
                let backend = Arc::new(backend);
                let opened = Opened {
                    root: backend.info().root.clone(),
                    name: backend.info().name.clone(),
                };
                *self.watch.lock().expect("watch lock") = None;
                *self.backend.write().expect("backend lock") = Some(backend);
                reply(opened)
            }
            Call::ListDir { path } => reply(self.backend()?.list_dir(&path)?),
            Call::ReadFile { path } => reply(self.backend()?.read_file(&path)?),
            Call::ReadOptional { path } => reply(self.backend()?.read_optional(&path)?),
            Call::WriteFile { path, contents } => {
                reply(self.backend()?.write_file(&path, &contents)?)
            }
            Call::ReadBytes { path } => reply(serde_bytes::ByteBuf::from(
                self.backend()?.read_bytes(&path)?,
            )),
            Call::WriteBytes { path, bytes } => reply(self.backend()?.write_bytes(&path, &bytes)?),
            Call::CreateFile { path } => reply(self.backend()?.create_file(&path)?),
            Call::CreateDir { path } => reply(self.backend()?.create_dir(&path)?),
            Call::Rename { from, to } => reply(self.backend()?.rename(&from, &to)?),
            Call::Delete { path } => reply(self.backend()?.delete(&path)?),
            Call::FindFiles {
                query,
                limit,
                exact,
            } => reply(self.backend()?.find_files(&query, limit, exact)?),
            Call::FindInFiles(req) => reply(self.backend()?.find_in_files(&req)?),
            Call::FindStart { req, page_size } => {
                let found = self
                    .backend()?
                    .find_in_files_found(&req, &std::sync::atomic::AtomicBool::new(false))?;
                let id = self
                    .next_found
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let first = found.paged(id, page_size);
                *self.found.lock().expect("found lock") = Some((id, Arc::new(found)));
                reply(first)
            }
            Call::FindPage { id, offset, count } => {
                match &*self.found.lock().expect("found lock") {
                    Some((current, found)) if *current == id => reply(found.page(offset, count)),
                    _ => Err(other("The search result was replaced by a newer search")),
                }
            }
            Call::FindSymbols {
                query,
                types_only,
                limit,
                skip_extensions,
                exact,
            } => reply(self.backend()?.find_symbols(
                &query,
                types_only,
                limit,
                &skip_extensions,
                exact,
            )?),
            Call::SearchStatus => reply(self.backend()?.search_status()?),
            Call::FindTodos(req) => reply(self.backend()?.find_todos(&req)?),
            Call::CollectFiles(req) => reply(self.backend()?.collect_files(&req)?),
            Call::Git(req) => reply(LocalGit.exec(&req)?),
            Call::Detect => reply(self.backend()?.detect_project()?),
            Call::Watch => {
                let agent = Arc::downgrade(self);
                let guard = self.backend()?.watch(Arc::new(move |changes: FsChanges| {
                    if let Some(a) = agent.upgrade() {
                        a.event(Event::FsChanged(changes));
                    }
                }))?;
                *self.watch.lock().expect("watch lock") = Some(guard);
                reply(())
            }
            Call::TerminalOpen { channel, spec } => {
                let agent = Arc::downgrade(self);
                let sink: PtySink = Arc::new(move |event: PtyEvent| {
                    if let Some(a) = agent.upgrade() {
                        if matches!(event, PtyEvent::Exited { .. }) {
                            a.channels.lock().expect("channels lock").remove(&channel);
                        }
                        a.event(Event::Pty { channel, event });
                    }
                });
                let term = self.backend()?.open_terminal(spec, sink)?;
                let opened = TerminalOpened {
                    shell: term.shell(),
                    cwd: term.cwd(),
                };
                let writer = term.clone();
                let input = Self::input_pump(move |d| writer.write(d));
                self.add_channel(channel, Channel::Terminal(term, input));
                reply(opened)
            }
            Call::TerminalResize {
                channel,
                cols,
                rows,
            } => {
                let term = match self.channels.lock().expect("channels lock").get(&channel) {
                    Some(Channel::Terminal(t, _)) => t.clone(),
                    _ => return Err(other(format!("no terminal on channel {channel}"))),
                };
                reply(term.resize(cols, rows)?)
            }
            Call::ProcessStart { channel, cmd } => {
                let agent = Arc::downgrade(self);
                let sink: RunSink = Arc::new(move |event: RunEvent| {
                    if let Some(a) = agent.upgrade() {
                        if matches!(event, RunEvent::Exited { .. }) {
                            a.channels.lock().expect("channels lock").remove(&channel);
                        }
                        a.event(Event::Run { channel, event });
                    }
                });
                let process = self.backend()?.start_process(&cmd, sink)?;
                let writer = process.clone();
                // An empty chunk is the end of input (queued after the data before it).
                let input = Self::input_pump(move |d| {
                    if d.is_empty() {
                        writer.close_stdin()
                    } else {
                        writer.write_stdin(d)
                    }
                });
                self.add_channel(channel, Channel::Process(process, input));
                reply(())
            }
            Call::ProcessKill { channel, grace_ms } => {
                let process = match self.channels.lock().expect("channels lock").get(&channel) {
                    Some(Channel::Process(p, _)) => Some(p.clone()),
                    _ => None,
                };
                if let Some(p) = process {
                    p.kill(Duration::from_millis(grace_ms));
                }
                reply(())
            }
            Call::ProcessCloseStdin { channel } => {
                if let Some(Channel::Process(_, input)) =
                    self.channels.lock().expect("channels lock").get(&channel)
                {
                    let _ = input.send(Vec::new());
                }
                reply(())
            }
            Call::FindExecutable { names, custom } => {
                // Works before a project is open, too (the home directory is the root).
                let root = match self.backend() {
                    Ok(b) => b.info().root.clone(),
                    Err(_) => dirs::home_dir().unwrap_or_default(),
                };
                reply(forge_backend::local::find_executable(
                    &root,
                    &names,
                    custom.as_deref(),
                ))
            }
            // Already encoded with the call's reply type.
            Call::Framework(call) => self.backend()?.framework(call),
            Call::LangServers(req) => reply(self.backend()?.langservers(req)?),
            Call::LspStart {
                channel,
                command,
                args,
            } => {
                let root = self.backend()?.info().root.clone();
                let io = spawn_local(&command, &args, &root)?;
                let input = self.pump_server(channel, io);
                self.add_channel(channel, Channel::Lsp(input));
                reply(())
            }
            Call::Php(call) => self.php(call),
            // Already encoded with the call's reply type.
            Call::Web(call) => self.backend()?.web(call),
            // Works before a project is open, too: installs go to the host's Forge folders.
            Call::LangServerInstall { channel, request } => {
                let cancel = Arc::new(tokio::sync::Notify::new());
                self.add_channel(channel, Channel::Install(cancel.clone()));
                let agent = Arc::downgrade(self);
                let mut on = move |event| {
                    if let Some(a) = agent.upgrade() {
                        a.event(Event::Install { channel, event });
                    }
                };
                let outcome =
                    langservers::block_on(langservers::install(&request, cancel, &mut on));
                self.channels
                    .lock()
                    .expect("channels lock")
                    .remove(&channel);
                reply(outcome?.map_err(ForgeError::Other)?)
            }
            Call::LangServerUpdates {
                registry,
                node_mirror,
                force,
            } => reply(
                langservers::block_on(langservers::check_updates(
                    registry.as_deref(),
                    node_mirror.as_deref(),
                    force,
                ))?
                .map_err(ForgeError::Other)?,
            ),
        }
    }

    /// Wire a language server's stdio to `channel`; returns the stdin feed.
    fn pump_server(self: &Arc<Self>, channel: u32, io: forge_lsp::ServerIo) -> Input {
        let forge_lsp::ServerIo {
            mut stdin,
            mut stdout,
            stderr,
            mut process,
        } = io;
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
        tokio::spawn(async move {
            while let Some(data) = rx.recv().await {
                if stdin.write_all(&data).await.is_err() || stdin.flush().await.is_err() {
                    break;
                }
            }
            // The IDE closed the channel (or the server stopped reading): stop the server.
            drop(stdin);
            process.stop(Duration::from_secs(2)).await;
        });
        let agent = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let n = match stdout.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                let Some(a) = agent.upgrade() else { break };
                a.send(Frame::Data {
                    channel,
                    data: ByteBuf::from(buf[..n].to_vec()),
                });
            }
            if let Some(a) = agent.upgrade() {
                a.channels.lock().expect("channels lock").remove(&channel);
                a.send(Frame::Close { channel });
            }
        });
        if let Some(mut stderr) = stderr {
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                while let Ok(n) = stderr.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                    tracing::debug!("language server: {}", String::from_utf8_lossy(&buf[..n]));
                }
            });
        }
        tx
    }

    fn php(self: &Arc<Self>, call: PhpCall) -> Result<Vec<u8>> {
        let backend = self.backend()?;
        let php = backend.php();
        match call {
            PhpCall::Index => {
                let agent = Arc::downgrade(self);
                let last = Mutex::new(None::<usize>);
                let stats = php.index_workspace(Arc::new(move |done, total| {
                    // One event per percent is plenty for a progress bar.
                    let pct = done * 100 / total.max(1);
                    let mut last = last.lock().expect("progress lock");
                    if *last == Some(pct) && done != total {
                        return;
                    }
                    *last = Some(pct);
                    if let Some(a) = agent.upgrade() {
                        a.event(Event::PhpIndexProgress { done, total });
                    }
                }))?;
                reply(stats)
            }
            PhpCall::Search {
                query,
                kind,
                limit,
                exact,
            } => reply(php.search(&query, kind, limit, exact)?),
            PhpCall::Structure {
                path,
                content,
                inherited,
            } => reply(php.file_structure(&path, content, inherited)?),
            PhpCall::Hover {
                path,
                offset,
                content,
            } => reply(php.hover(&path, offset, content)?),
            PhpCall::GenerateTargets {
                path,
                offset,
                content,
            } => reply(php.generate_targets(&path, offset, content)?),
            PhpCall::Generate {
                path,
                offset,
                kind,
                options,
                content,
            } => reply(php.generate(&path, offset, kind, options, content)?),
            PhpCall::Usages {
                path,
                offset,
                content,
            } => reply(php.usages(&path, offset, content)?),
            PhpCall::Navigate {
                path,
                offset,
                kind,
                content,
            } => reply(php.navigate(&path, offset, kind, content)?),
            PhpCall::Definition {
                path,
                offset,
                content,
            } => reply(php.definition(&path, offset, content)?),
            PhpCall::LineMarkers { path, content } => reply(php.line_markers(&path, content)?),
            PhpCall::Intentions {
                path,
                offset,
                content,
            } => reply(php.intentions(&path, offset, content)?),
            PhpCall::LspConfig { settings } => reply(php.lsp_config(settings)?),
            PhpCall::DetectServers { settings } => reply(php.detect_servers(settings)?),
        }
    }

    fn on_data(&self, channel: u32, data: Vec<u8>) {
        if data.is_empty() {
            return;
        }
        if let Some(c) = self.channels.lock().expect("channels lock").get(&channel) {
            if let Some(input) = c.input() {
                let _ = input.send(data);
            }
        }
    }

    fn on_close(&self, channel: u32) {
        let removed = self
            .channels
            .lock()
            .expect("channels lock")
            .remove(&channel);
        if let Some(c) = removed {
            c.stop();
        }
    }

    fn shutdown(&self) {
        *self.watch.lock().expect("watch lock") = None;
        let channels: Vec<Channel> = self
            .channels
            .lock()
            .expect("channels lock")
            .drain()
            .map(|(_, c)| c)
            .collect();
        for c in channels {
            c.stop();
        }
    }
}

fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .or_else(|_| std::fs::read_to_string("/etc/hostname"))
        .map(|s| s.trim().to_string())
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_default()
}

/// Serve the protocol on `reader` / `writer` until the client closes the stream. Everything
/// started for the client (terminals, processes, language servers, the watcher) stops then.
pub async fn serve<R, W>(reader: R, writer: W) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (out, mut out_rx) = mpsc::unbounded_channel::<Frame>();
    let agent = Arc::new(Agent {
        out,
        backend: RwLock::new(None),
        watch: Mutex::new(None),
        found: Mutex::new(None),
        next_found: std::sync::atomic::AtomicU64::new(1),
        channels: Mutex::new(HashMap::new()),
    });

    let writer_task = tokio::spawn(async move {
        let mut writer = writer;
        while let Some(frame) = out_rx.recv().await {
            if let Err(e) = io::write_frame(&mut writer, &frame).await {
                tracing::warn!("cannot write to the IDE: {e}");
                break;
            }
        }
        let _ = writer.shutdown().await;
    });

    let mut reader = reader;
    let result = loop {
        let frame = match io::read_frame(&mut reader).await {
            Ok(Some(f)) => f,
            Ok(None) => break Ok(()),
            Err(e) => break Err(e),
        };
        match frame {
            Frame::Request { id, call } => {
                let agent = agent.clone();
                tokio::spawn(async move {
                    let a = agent.clone();
                    let result = tokio::task::spawn_blocking(move || a.handle(call))
                        .await
                        .unwrap_or_else(|e| Err(other(format!("agent task failed: {e}"))));
                    agent.send(Frame::Response {
                        id,
                        result: result.map(ByteBuf::from).map_err(|e| e.to_string()),
                    });
                });
            }
            Frame::Data { channel, data } => agent.on_data(channel, data.into_vec()),
            Frame::Close { channel } => agent.on_close(channel),
            Frame::Response { .. } | Frame::Event(_) => {
                tracing::warn!("unexpected frame from the IDE");
            }
        }
    };
    agent.shutdown();
    // Let in-flight answers go out, then stop writing.
    drop(agent);
    let _ = tokio::time::timeout(Duration::from_secs(2), writer_task).await;
    result
}
