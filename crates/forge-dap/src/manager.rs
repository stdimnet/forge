//! The DAP side of the debugger: the adapter registry and all running DAP sessions.
//! Sessions report through the same [`EventSink`] as Xdebug sessions and share the
//! project's breakpoints (through a [`BreakpointHost`], the PHP debug server in the app).

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use forge_debug::{
    Breakpoint, BreakpointSpec, BreakpointsSnapshot, DebugEvent, DebugServer, EventSink,
    SessionInfo,
};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::adapter::{
    AdapterInfo, AdapterRegistry, DebugAdapterDescriptor, LaunchRequest, ResolvedLaunch,
};
use crate::client::{DapClient, Trace};
use crate::error::{DapError, DapResult};
use crate::protocol::StartDebuggingArgs;
use crate::session::DapSession;
use crate::transport;

/// Where breakpoints live. The app uses the project's store in [`DebugServer`].
pub trait BreakpointHost: Send + Sync {
    fn snapshot(&self) -> BreakpointsSnapshot;
    fn add(&self, spec: BreakpointSpec) -> DapResult<Breakpoint>;
    fn remove(&self, id: u64);
}

impl BreakpointHost for DebugServer {
    fn snapshot(&self) -> BreakpointsSnapshot {
        self.breakpoints()
    }

    fn add(&self, spec: BreakpointSpec) -> DapResult<Breakpoint> {
        Ok(self.add_breakpoint(spec)?)
    }

    fn remove(&self, id: u64) {
        if let Err(e) = self.remove_breakpoint(id) {
            tracing::warn!("cannot remove breakpoint {id}: {e}");
        }
    }
}

/// Timeouts of the DAP layer.
#[derive(Debug, Clone)]
pub struct DapConfig {
    /// Answer time for ordinary requests.
    pub request_timeout: Duration,
    /// Time for the adapter to start, connect and answer `launch`/`attach`.
    pub launch_timeout: Duration,
    /// Log every message at `trace` level.
    pub trace: bool,
}

impl Default for DapConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(10),
            launch_timeout: Duration::from_secs(30),
            trace: false,
        }
    }
}

pub(crate) struct Inner {
    registry: RwLock<AdapterRegistry>,
    sessions: Mutex<HashMap<String, Arc<DapSession>>>,
    sink: EventSink,
    breakpoints: Arc<dyn BreakpointHost>,
    root: RwLock<Option<PathBuf>>,
    config: RwLock<DapConfig>,
    next: AtomicU64,
}

impl Inner {
    pub(crate) fn emit(&self, event: DebugEvent) {
        (self.sink)(event);
    }

    pub(crate) fn root(&self) -> Option<PathBuf> {
        self.root.read().expect("root lock").clone()
    }

    pub(crate) fn breakpoints(&self) -> &Arc<dyn BreakpointHost> {
        &self.breakpoints
    }

    pub(crate) fn remove_session(&self, id: &str) {
        self.sessions.lock().expect("sessions lock").remove(id);
    }

    async fn start_resolved(
        self: &Arc<Self>,
        info: AdapterInfo,
        request: LaunchRequest,
        resolved: ResolvedLaunch,
        parent: Option<String>,
    ) -> DapResult<Arc<DapSession>> {
        let config = self.config.read().expect("config lock").clone();
        let conn = transport::open(&resolved.transport, config.launch_timeout).await?;
        let n = self.next.fetch_add(1, Ordering::SeqCst);
        let id = format!("dap-{n}");
        let trace: Option<Trace> = config.trace.then(|| {
            let id = id.clone();
            let trace: Trace =
                Arc::new(move |dir: &str, msg: &Value| tracing::trace!("[{id}] {dir} {msg}"));
            trace
        });
        let (client, mut incoming) =
            DapClient::spawn(conn.reader, conn.writer, config.request_timeout, trace);
        let session = Arc::new(DapSession::new(
            id.clone(),
            info,
            request,
            resolved,
            client,
            conn.child,
            conn.peer,
            parent,
            Arc::downgrade(self),
        ));
        self.sessions
            .lock()
            .expect("sessions lock")
            .insert(id, Arc::clone(&session));
        if let Some(stderr) = conn.stderr {
            let s = Arc::clone(&session);
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    s.note_stderr(line);
                }
            });
        }
        let pump = Arc::clone(&session);
        tokio::spawn(async move {
            while let Some(message) = incoming.recv().await {
                pump.on_incoming(message).await;
            }
            pump.end().await;
        });
        if let Err(e) = session.start(config.launch_timeout).await {
            session.end().await;
            return Err(e);
        }
        Ok(session)
    }

    /// `startDebugging`: a child session with the parent's adapter and transport. Boxed:
    /// the child's start can itself handle reverse requests.
    pub(crate) fn start_child(
        self: &Arc<Self>,
        parent: &Arc<DapSession>,
        args: StartDebuggingArgs,
    ) -> Pin<Box<dyn Future<Output = DapResult<Arc<DapSession>>> + Send>> {
        let mut request = parent.launch_request().clone();
        if let Some(name) = args.configuration.get("name").and_then(Value::as_str) {
            request.name = Some(name.to_string());
        }
        let resolved = ResolvedLaunch {
            transport: parent.resolved().transport.clone(),
            request: if args.request.is_empty() {
                "launch".into()
            } else {
                args.request
            },
            arguments: args.configuration,
        };
        let me = Arc::clone(self);
        let info = parent.adapter().clone();
        let parent_id = parent.id.clone();
        Box::pin(async move {
            me.start_resolved(info, request, resolved, Some(parent_id))
                .await
        })
    }
}

/// Make project-relative paths of a request absolute (adapters run outside the project);
/// the working directory defaults to the project root.
pub fn absolutize(mut request: LaunchRequest, root: Option<&std::path::Path>) -> LaunchRequest {
    let Some(root) = root else {
        return request;
    };
    let abs = |p: &str| -> String {
        let path = std::path::Path::new(p);
        if path.is_absolute() || p.is_empty() {
            p.to_string()
        } else {
            root.join(path).to_string_lossy().into_owned()
        }
    };
    let program_is_path = |p: &str| p.contains('/') || p.contains('\\');
    if let Some(p) = request.program.as_deref().filter(|p| program_is_path(p)) {
        request.program = Some(abs(p));
    }
    request.cwd = Some(match request.cwd.as_deref().filter(|c| !c.is_empty()) {
        Some(c) => abs(c),
        None => root.to_string_lossy().into_owned(),
    });
    if let crate::adapter::LaunchMode::CoreDump { core } = &mut request.mode {
        *core = abs(core);
    }
    request
}

/// Starts and tracks DAP sessions.
pub struct DapManager {
    inner: Arc<Inner>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn relative_paths_are_resolved_against_the_project() {
        let mut r = LaunchRequest::launch("lldb", "build/demo");
        r.mode = crate::adapter::LaunchMode::Launch;
        let r = absolutize(r, Some(Path::new("/w")));
        assert_eq!(r.program.as_deref(), Some("/w/build/demo"));
        assert_eq!(r.cwd.as_deref(), Some("/w"));
        let mut bare = LaunchRequest::launch("lldb", "python3");
        bare.cwd = Some("src".into());
        let bare = absolutize(bare, Some(Path::new("/w")));
        assert_eq!(
            bare.program.as_deref(),
            Some("python3"),
            "commands stay on the PATH"
        );
        assert_eq!(bare.cwd.as_deref(), Some("/w/src"));
    }
}

impl DapManager {
    pub fn new(sink: EventSink, breakpoints: Arc<dyn BreakpointHost>) -> Self {
        Self {
            inner: Arc::new(Inner {
                registry: RwLock::new(AdapterRegistry::new()),
                sessions: Mutex::new(HashMap::new()),
                sink,
                breakpoints,
                root: RwLock::new(None),
                config: RwLock::new(DapConfig::default()),
                next: AtomicU64::new(1),
            }),
        }
    }

    pub fn set_config(&self, config: DapConfig) {
        *self.inner.config.write().expect("config lock") = config;
    }

    /// The project root: breakpoint files are relative to it, adapters get absolute paths.
    pub fn set_project_root(&self, root: Option<PathBuf>) {
        *self.inner.root.write().expect("root lock") = root;
    }

    pub fn register_adapter(&self, adapter: Arc<dyn DebugAdapterDescriptor>) {
        self.inner
            .registry
            .write()
            .expect("registry lock")
            .register(adapter);
    }

    /// Replace the registry (e.g. after plugins were enabled or disabled).
    pub fn set_registry(&self, registry: AdapterRegistry) {
        *self.inner.registry.write().expect("registry lock") = registry;
    }

    pub fn adapters(&self) -> Vec<AdapterInfo> {
        self.inner.registry.read().expect("registry lock").list()
    }

    /// Start a session: launch a program, attach to a process or DAP server, or open a
    /// core dump. Returns once the adapter is configured (the program runs or is paused).
    pub async fn start(&self, request: LaunchRequest) -> DapResult<SessionInfo> {
        let request = absolutize(request, self.inner.root().as_deref());
        let descriptor = self
            .inner
            .registry
            .read()
            .expect("registry lock")
            .get(&request.adapter)?;
        let resolved = descriptor.resolve(&request)?;
        let session = self
            .inner
            .start_resolved(descriptor.info(), request, resolved, None)
            .await?;
        Ok(session.info())
    }

    /// Restart a session: in place when the adapter supports it, else a new session with
    /// the same request (its info is returned).
    pub async fn restart(&self, id: &str) -> DapResult<SessionInfo> {
        let session = self.session(id)?;
        if session.restart_in_place().await? {
            return Ok(session.info());
        }
        let request = session.launch_request().clone();
        session.stop().await;
        self.start(request).await
    }

    pub fn session(&self, id: &str) -> DapResult<Arc<DapSession>> {
        self.inner
            .sessions
            .lock()
            .expect("sessions lock")
            .get(id)
            .cloned()
            .ok_or_else(|| DapError::NotFound(format!("no debug session {id}")))
    }

    /// Whether `id` names a DAP session (Xdebug sessions are handled elsewhere).
    pub fn has_session(&self, id: &str) -> bool {
        self.inner
            .sessions
            .lock()
            .expect("sessions lock")
            .contains_key(id)
    }

    pub fn sessions(&self) -> Vec<SessionInfo> {
        let mut list: Vec<Arc<DapSession>> = self
            .inner
            .sessions
            .lock()
            .expect("sessions lock")
            .values()
            .cloned()
            .collect();
        list.sort_by_key(|s| s.id.trim_start_matches("dap-").parse::<u64>().unwrap_or(0));
        list.iter().map(|s| s.info()).collect()
    }

    /// The project's breakpoints changed: resend them to every configured session.
    pub fn breakpoints_changed(&self) {
        let sessions: Vec<Arc<DapSession>> = self
            .inner
            .sessions
            .lock()
            .expect("sessions lock")
            .values()
            .cloned()
            .collect();
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            tracing::warn!(
                "breakpoints changed outside the async runtime; DAP sessions not updated"
            );
            return;
        };
        for s in sessions.into_iter().filter(|s| s.is_configured()) {
            runtime.spawn(async move { s.sync_breakpoints().await });
        }
    }

    /// Stop every session (project closed, app exit).
    pub async fn stop_all(&self) {
        let sessions: Vec<Arc<DapSession>> = self
            .inner
            .sessions
            .lock()
            .expect("sessions lock")
            .values()
            .cloned()
            .collect();
        for s in sessions {
            s.stop().await;
        }
    }
}
