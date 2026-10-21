//! The DBGp listener ("Start Listening for PHP Debug Connections") and the registry of
//! sessions, breakpoints and servers it shares with them.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use tokio::io::BufReader;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::breakpoints::{Breakpoint, BreakpointSpec, BreakpointStore, BreakpointsSnapshot};
use crate::config::DebugConfig;
use crate::connection::Connection;
use crate::error::{DebugError, DebugResult};
use crate::events::{DebugEvent, EventSink, SessionInfo, SessionStatus};
use crate::protocol::{read_packet, InitPacket};
use crate::servers::{load_servers_from, save_servers_to, ServerConfig};
use crate::session::Session;
use crate::xml;
use forge_core::{LocalFiles, SharedFiles};

/// State shared by the listener and every session.
pub(crate) struct Shared {
    config: RwLock<DebugConfig>,
    root: RwLock<Option<PathBuf>>,
    /// The project's files (breakpoints and servers are stored there).
    files: RwLock<Option<SharedFiles>>,
    servers: RwLock<Vec<ServerConfig>>,
    breakpoints: Mutex<BreakpointStore>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    sink: EventSink,
    next_session: AtomicU64,
}

impl Shared {
    pub(crate) fn emit(&self, event: DebugEvent) {
        (self.sink)(event);
    }

    pub(crate) fn config(&self) -> DebugConfig {
        self.config.read().expect("config lock").clone()
    }

    pub(crate) fn root(&self) -> Option<PathBuf> {
        self.root.read().expect("root lock").clone()
    }

    pub(crate) fn servers(&self) -> Vec<ServerConfig> {
        self.servers.read().expect("servers lock").clone()
    }

    pub(crate) fn with_breakpoints<T>(&self, f: impl FnOnce(&mut BreakpointStore) -> T) -> T {
        f(&mut self.breakpoints.lock().expect("breakpoints lock"))
    }

    pub(crate) fn remove_session(&self, id: &str) {
        self.sessions.lock().expect("sessions lock").remove(id);
    }

    fn sessions(&self) -> Vec<Arc<Session>> {
        let mut list: Vec<Arc<Session>> = self
            .sessions
            .lock()
            .expect("sessions lock")
            .values()
            .cloned()
            .collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }
}

struct Listener {
    /// The port debuggers connect to (for a remote project: the forwarded port on its host).
    port: u16,
    task: JoinHandle<()>,
}

/// Owns the TCP listener and all debug sessions.
pub struct DebugServer {
    shared: Arc<Shared>,
    listener: Mutex<Option<Listener>>,
}

impl DebugServer {
    pub fn new(sink: EventSink) -> Self {
        Self {
            shared: Arc::new(Shared {
                config: RwLock::new(DebugConfig::default()),
                root: RwLock::new(None),
                files: RwLock::new(None),
                servers: RwLock::new(Vec::new()),
                breakpoints: Mutex::new(BreakpointStore::memory()),
                sessions: Mutex::new(HashMap::new()),
                sink,
                next_session: AtomicU64::new(1),
            }),
            listener: Mutex::new(None),
        }
    }

    // ---- configuration ------------------------------------------------------------------

    pub fn config(&self) -> DebugConfig {
        self.shared.config()
    }

    pub fn set_config(&self, config: DebugConfig) {
        *self.shared.config.write().expect("config lock") = config;
    }

    /// Switch to a project: loads its breakpoints and servers (`None` closes it).
    pub fn open_project(&self, root: Option<&Path>) {
        self.open_project_files(root.map(|r| (r.to_path_buf(), LocalFiles::shared(r))));
    }

    /// Switch to a project whose `.forge/` files are read and written through `files`
    /// (a project on a remote host: `root` is its path there, which is also what Xdebug
    /// reports, so files map 1:1 without path mappings).
    pub fn open_project_files(&self, project: Option<(PathBuf, SharedFiles)>) {
        let (root, files) = match project {
            Some((r, f)) => (Some(r), Some(f)),
            None => (None, None),
        };
        let store = files
            .clone()
            .map(BreakpointStore::load_files)
            .unwrap_or_default();
        let servers = files.as_deref().map(load_servers_from).unwrap_or_default();
        *self.shared.root.write().expect("root lock") = root;
        *self.shared.files.write().expect("files lock") = files;
        *self.shared.breakpoints.lock().expect("breakpoints lock") = store;
        *self.shared.servers.write().expect("servers lock") = servers;
        self.emit_breakpoints();
    }

    pub fn project_root(&self) -> Option<PathBuf> {
        self.shared.root()
    }

    pub fn servers(&self) -> Vec<ServerConfig> {
        self.shared.servers()
    }

    /// Replace the server list and persist it in the project.
    pub fn save_servers(&self, servers: Vec<ServerConfig>) -> DebugResult<()> {
        let files = self.shared.files.read().expect("files lock").clone();
        if let Some(files) = files {
            save_servers_to(files.as_ref(), &servers)?;
        }
        *self.shared.servers.write().expect("servers lock") = servers;
        Ok(())
    }

    // ---- listening ----------------------------------------------------------------------

    pub fn listening_port(&self) -> Option<u16> {
        self.listener
            .lock()
            .expect("listener lock")
            .as_ref()
            .map(|l| l.port)
    }

    /// Listen on `port` on all interfaces (0 picks a free port). Returns the bound port.
    pub async fn listen(&self, port: u16) -> DebugResult<u16> {
        self.listen_with(port, |bound| async move { Ok((bound, ())) })
            .await
            .map(|(port, ())| port)
    }

    /// Listen on `port` here, then let `forward` make the listener reachable elsewhere (an
    /// SSH tunnel from a remote host's port to the bound one). `forward` returns the port
    /// debuggers connect to, which is reported as the listening port, and a value kept by
    /// the caller (the tunnel). The listener stops when `forward` fails.
    pub async fn listen_with<T, F, Fut>(&self, port: u16, forward: F) -> DebugResult<(u16, T)>
    where
        F: FnOnce(u16) -> Fut,
        Fut: std::future::Future<Output = DebugResult<(u16, T)>>,
    {
        self.stop_listening_quietly();
        let listener = match TcpListener::bind(("0.0.0.0", port)).await {
            Ok(l) => l,
            Err(e) => {
                let message = if e.kind() == std::io::ErrorKind::AddrInUse {
                    format!("Port {port} is busy. Another debugger (or PHP-FPM) may be using it.")
                } else {
                    format!("Cannot listen on port {port}: {e}")
                };
                self.shared.emit(DebugEvent::Listening {
                    listening: false,
                    port,
                    error: Some(message.clone()),
                });
                return Err(DebugError::Other(message));
            }
        };
        let bound = listener.local_addr()?.port();
        let (shown, kept) = match forward(bound).await {
            Ok(v) => v,
            Err(e) => {
                self.shared.emit(DebugEvent::Listening {
                    listening: false,
                    port,
                    error: Some(e.to_string()),
                });
                return Err(e);
            }
        };
        let shared = Arc::clone(&self.shared);
        let task = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, peer)) => {
                        let shared = Arc::clone(&shared);
                        tokio::spawn(async move { handle_connection(shared, stream, peer).await });
                    }
                    Err(e) => {
                        tracing::warn!("debug listener accept failed: {e}");
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                }
            }
        });
        *self.listener.lock().expect("listener lock") = Some(Listener { port: shown, task });
        self.shared.emit(DebugEvent::Listening {
            listening: true,
            port: shown,
            error: None,
        });
        Ok((shown, kept))
    }

    fn stop_listening_quietly(&self) -> Option<u16> {
        let old = self.listener.lock().expect("listener lock").take();
        old.map(|l| {
            l.task.abort();
            l.port
        })
    }

    /// Stop accepting connections; running sessions continue.
    pub fn stop_listening(&self) {
        if let Some(port) = self.stop_listening_quietly() {
            self.shared.emit(DebugEvent::Listening {
                listening: false,
                port,
                error: None,
            });
        }
    }

    // ---- sessions -----------------------------------------------------------------------

    pub fn sessions(&self) -> Vec<SessionInfo> {
        self.shared.sessions().iter().map(|s| s.info()).collect()
    }

    pub fn session(&self, id: &str) -> DebugResult<Arc<Session>> {
        self.shared
            .sessions
            .lock()
            .expect("sessions lock")
            .get(id)
            .cloned()
            .ok_or_else(|| DebugError::NotFound(format!("no debug session {id}")))
    }

    /// "Accept" in the Incoming Connection dialog: remember `server` and start debugging.
    pub async fn accept_connection(&self, id: &str, server: ServerConfig) -> DebugResult<()> {
        let session = self.session(id)?;
        let mut servers = self.servers();
        match servers.iter_mut().find(|s| s.id == server.id) {
            Some(slot) => *slot = server.clone(),
            None => servers.push(server.clone()),
        }
        self.save_servers(servers)?;
        session.accept(&server).await
    }

    /// "Ignore" in the Incoming Connection dialog.
    pub async fn ignore_connection(&self, id: &str) -> DebugResult<()> {
        self.session(id)?.detach().await;
        Ok(())
    }

    // ---- breakpoints --------------------------------------------------------------------

    pub fn breakpoints(&self) -> BreakpointsSnapshot {
        self.shared.with_breakpoints(|s| s.snapshot())
    }

    fn emit_breakpoints(&self) {
        let snapshot = self.breakpoints();
        self.shared.emit(DebugEvent::Breakpoints { snapshot });
    }

    /// Push a breakpoint change to every live session without blocking the caller (a
    /// running script only reads commands when it suspends).
    fn propagate(&self, id: u64, bp: Option<Breakpoint>) {
        let muted = self.shared.with_breakpoints(|s| s.muted());
        for session in self.shared.sessions() {
            let bp = bp.clone();
            tokio::spawn(async move { session.sync_breakpoint(id, bp, muted).await });
        }
    }

    pub fn add_breakpoint(&self, spec: BreakpointSpec) -> DebugResult<Breakpoint> {
        let bp = self.shared.with_breakpoints(|s| s.add(spec))?;
        self.propagate(bp.id, Some(bp.clone()));
        self.emit_breakpoints();
        Ok(bp)
    }

    pub fn update_breakpoint(&self, bp: Breakpoint) -> DebugResult<Breakpoint> {
        self.shared.with_breakpoints(|s| s.update(bp.clone()))?;
        self.propagate(bp.id, Some(bp.clone()));
        self.emit_breakpoints();
        Ok(bp)
    }

    pub fn remove_breakpoint(&self, id: u64) -> DebugResult<()> {
        self.shared.with_breakpoints(|s| s.remove(id))?;
        self.propagate(id, None);
        self.emit_breakpoints();
        Ok(())
    }

    /// Click in the gutter / Ctrl+F8: add a line breakpoint or remove the existing one.
    /// Returns the new breakpoint, or `None` when one was removed.
    pub fn toggle_line_breakpoint(&self, file: &str, line: u32) -> DebugResult<Option<Breakpoint>> {
        let existing = self
            .shared
            .with_breakpoints(|s| s.find_line(file, line).map(|b| b.id));
        match existing {
            Some(id) => {
                self.remove_breakpoint(id)?;
                Ok(None)
            }
            None => self
                .add_breakpoint(BreakpointSpec {
                    file: Some(file.to_string()),
                    line: Some(line),
                    ..Default::default()
                })
                .map(Some),
        }
    }

    /// "Mute Breakpoints": keep them but do not suspend.
    pub fn set_muted(&self, muted: bool) -> DebugResult<()> {
        self.shared.with_breakpoints(|s| s.set_muted(muted))?;
        for session in self.shared.sessions() {
            tokio::spawn(async move { session.sync_all_breakpoints().await });
        }
        self.emit_breakpoints();
        Ok(())
    }
}

impl Drop for DebugServer {
    fn drop(&mut self) {
        self.stop_listening_quietly();
    }
}

async fn handle_connection(shared: Arc<Shared>, stream: TcpStream, peer: SocketAddr) {
    let _ = stream.set_nodelay(true);
    let (read, write) = stream.into_split();
    let mut reader = BufReader::new(read);
    let init = match tokio::time::timeout(Duration::from_secs(10), read_packet(&mut reader)).await {
        Ok(Ok(Some(text))) => match xml::parse(&text).and_then(|el| InitPacket::parse(&el)) {
            Ok(init) => init,
            Err(e) => {
                tracing::warn!("rejecting debug connection from {peer}: {e}");
                return;
            }
        },
        _ => return,
    };
    let n = shared.next_session.fetch_add(1, Ordering::SeqCst);
    let id = format!("xdebug-{n}");
    let timeout = Duration::from_millis(shared.config().command_timeout_ms.max(100));
    let conn = Connection::new(Box::new(write), timeout);
    let session = Arc::new(Session::new(
        id.clone(),
        init,
        peer.to_string(),
        conn,
        Arc::clone(&shared),
    ));
    shared
        .sessions
        .lock()
        .expect("sessions lock")
        .insert(id, Arc::clone(&session));
    let reading = Arc::clone(&session);
    tokio::spawn(async move {
        reading
            .connection()
            .read_loop(reader, |el| reading.on_message(el))
            .await;
        reading.on_closed();
    });
    session.begin().await;
    if session.status() == SessionStatus::Initializing {
        tracing::debug!("session {} ended during setup", session.id);
    }
}
