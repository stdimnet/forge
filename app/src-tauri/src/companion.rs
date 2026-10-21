//! Browser Companion and other ways to open files from outside: the local HTTP server for
//! the browser extension, `forge://` links and `forge file:12` launches. The logic lives
//! in `forge-open`; this module wires it to the window, the settings and the UI. For a
//! remote project, browser paths are mapped to the project on the host
//! (`forge_open::resolve_remote`) and checked and searched through the project's backend.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use forge_backend::Backend;
use forge_core::{FileKind, FileMatch};
use forge_open::cli::{parse_launch_args, LaunchTarget};
use forge_open::pairing::{ClientInfo, Pairing};
use forge_open::server::{
    CompanionServer, HostError, HostInfo, OpenHost, OpenOutcome, OpenStatus, ProjectInfo,
    DEFAULT_PORT,
};
use forge_open::symbols::SymbolSource;
use forge_open::{
    open_event, resolve_remote, symbols, KnownProject, LinkAction, OpenEvent, OpenRequest,
    OpenSource, ProjectContext, RemoteProject, ResolveError, SymbolRequest, Target,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

pub const OPEN_EVENT: &str = "companion://open";
pub const OPEN_PROJECT_EVENT: &str = "companion://open-project";
pub const STATUS_EVENT: &str = "companion://status";

pub const KEY_ENABLED: &str = "browserCompanion.enabled";
pub const KEY_PORT: &str = "browserCompanion.port";
pub const KEY_FOCUS: &str = "browserCompanion.focusWindow";

/// Server state shown on the settings page.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct CompanionState {
    pub pairing: Arc<Mutex<Pairing>>,
    server: Mutex<Option<CompanionServer>>,
    pub status: Mutex<ServerStatus>,
    /// Requests that arrived before the UI was ready (e.g. `forge file:12` at startup).
    pending: Mutex<Vec<PendingEvent>>,
    ui_ready: AtomicBool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PendingEvent {
    Open(OpenEvent),
    #[serde(rename_all = "camelCase")]
    OpenProject {
        root: String,
    },
}

impl CompanionState {
    pub fn load() -> Self {
        let pairing = Pairing::default_path()
            .map(Pairing::load)
            .unwrap_or_else(Pairing::in_memory);
        Self {
            pairing: Arc::new(Mutex::new(pairing)),
            ..Self::default()
        }
    }

    pub fn pairing(&self) -> std::sync::MutexGuard<'_, Pairing> {
        self.pairing.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Mark the UI as listening and hand over what arrived before.
    pub fn take_pending(&self) -> Vec<PendingEvent> {
        self.ui_ready.store(true, Ordering::SeqCst);
        std::mem::take(&mut *self.pending.lock().expect("pending lock poisoned"))
    }
}

fn known_projects(app: &AppHandle) -> ProjectContext {
    let open = app
        .state::<AppState>()
        .local_workspace()
        .map(|ws| KnownProject::new(ws.name(), ws.root()));
    let recent = crate::recent::load()
        .into_iter()
        .filter(|p| p.remote.is_none())
        .map(|p| KnownProject::new(p.name, p.root))
        .collect();
    ProjectContext { open, recent }
}

fn focus_main_window(app: &AppHandle) {
    let focus = app
        .state::<AppState>()
        .read_settings()
        .get_bool(KEY_FOCUS)
        .unwrap_or(true);
    if !focus {
        return;
    }
    if let Some(w) = app.get_webview_window(crate::startup::MAIN_LABEL) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Send an event to the UI, or queue it until the UI is ready.
fn dispatch(app: &AppHandle, event: PendingEvent) {
    let state = app.state::<CompanionState>();
    if !state.ui_ready.load(Ordering::SeqCst) {
        state
            .pending
            .lock()
            .expect("pending lock poisoned")
            .push(event);
        return;
    }
    let sent = match &event {
        PendingEvent::Open(e) => app.emit(OPEN_EVENT, e),
        PendingEvent::OpenProject { root } => app.emit(OPEN_PROJECT_EVENT, root),
    };
    if let Err(e) = sent {
        tracing::warn!("cannot forward open request to the UI: {e}");
    }
    focus_main_window(app);
}

fn resolve_error(e: ResolveError) -> HostError {
    let status = match e {
        ResolveError::NotFound(_) | ResolveError::NotAFile(_) => 404,
        ResolveError::NoProject(_) => 409,
        ResolveError::UnknownProject(_) => 400,
    };
    HostError::new(status, e.to_string())
}

/// The open project when it is on a remote host, with its backend.
fn remote_project(app: &AppHandle) -> Option<(RemoteProject, Arc<dyn Backend>)> {
    let state = app.state::<AppState>();
    let info = state.project_info().filter(|i| i.remote.is_some())?;
    let backend = state.backend().ok()?;
    Some((
        RemoteProject {
            display_root: info.display_root(),
            root: info.root.to_string_lossy().into_owned(),
        },
        backend,
    ))
}

/// Whether `rel` is a file of the remote project (one directory listing on the host).
fn remote_file_exists(backend: &dyn Backend, rel: &str) -> bool {
    let parent = rel.rsplit_once('/').map_or("", |(d, _)| d);
    backend.list_dir(parent).is_ok_and(|entries| {
        entries
            .iter()
            .any(|e| e.path == rel && e.kind == FileKind::File)
    })
}

/// An open request for the remote project: `None` when the file is not part of it.
fn remote_open_event(
    project: &RemoteProject,
    backend: &dyn Backend,
    request: &OpenRequest,
    source: OpenSource,
    client: Option<String>,
) -> Option<OpenEvent> {
    let (file, target) =
        resolve_remote(request, project, &|rel| remote_file_exists(backend, rel)).ok()?;
    let (mut line, mut column) = (request.line, request.column);
    if line.is_none() {
        if let Some(symbol) = &request.symbol {
            if let Some((l, c)) = backend
                .read_file(target.path())
                .ok()
                .and_then(|content| symbols::find_declaration(&content, symbol))
            {
                line = Some(l);
                column = Some(c);
            }
        }
    }
    Some(OpenEvent {
        source,
        client,
        file,
        line,
        column,
        target,
    })
}

/// Resolve and dispatch one open request.
pub fn open_request(
    app: &AppHandle,
    request: &OpenRequest,
    source: OpenSource,
    client: Option<String>,
) -> Result<OpenEvent, ResolveError> {
    let remote = remote_project(app).and_then(|(project, backend)| {
        remote_open_event(&project, backend.as_ref(), request, source, client.clone())
    });
    let event = match remote {
        Some(event) => event,
        // Not part of the remote project: maybe a local project's file.
        None => open_event(request, &known_projects(app), source, client)?,
    };
    dispatch(app, PendingEvent::Open(event.clone()));
    Ok(event)
}

/// The files of the open project as the symbol search sees them.
struct BackendSymbols(Arc<dyn Backend>);

impl SymbolSource for BackendSymbols {
    fn find_files(&self, query: &str, limit: usize) -> forge_core::Result<Vec<FileMatch>> {
        self.0.find_files(query, limit, false)
    }

    fn read_small(&self, path: &str, max_bytes: u64) -> Option<String> {
        self.0
            .read_file(path)
            .ok()
            .filter(|t| t.len() as u64 <= max_bytes)
    }
}

/// Open the best declaration of `request.name` in the open project (searched on the host
/// for a remote project).
pub fn open_symbol(
    app: &AppHandle,
    request: &SymbolRequest,
    source: OpenSource,
    client: Option<String>,
) -> Result<OpenEvent, HostError> {
    let state = app.state::<AppState>();
    let no_project = || HostError::new(409, "no project is open in Forge");
    let info = state.project_info().ok_or_else(no_project)?;
    let backend = state.backend().map_err(|_| no_project())?;
    let hit = symbols::resolve_symbol(&BackendSymbols(backend), request)
        .map_err(|e| HostError::new(500, e.to_string()))?
        .into_iter()
        .next();
    let root = info.display_root();
    let hit = hit
        .ok_or_else(|| HostError::new(404, format!("no declaration of {} found", request.name)))?;
    let file = match &info.remote {
        Some(_) => format!("{root}/{}", hit.path),
        None => Path::new(&root).join(&hit.path).display().to_string(),
    };
    let event = OpenEvent {
        source,
        client,
        file,
        line: hit.line,
        column: hit.column,
        target: Target::Open {
            root,
            path: hit.path,
        },
    };
    dispatch(app, PendingEvent::Open(event.clone()));
    Ok(event)
}

/// Handle a `forge://` link (from the OS, so it is not trusted).
pub fn open_link(app: &AppHandle, link: &LinkAction) {
    let result = match link {
        LinkAction::Open(r) => open_request(app, r, OpenSource::Link, None)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        LinkAction::OpenSymbol(r) => open_symbol(app, r, OpenSource::Link, None)
            .map(|_| ())
            .map_err(|e| e.message),
    };
    if let Err(e) = result {
        tracing::warn!("cannot open link: {e}");
    }
}

/// Handle launcher arguments: at startup (`startup = true`) a project or file argument
/// opens its project directly; later (second launch) everything goes through the UI.
pub fn handle_launch_args<I, S>(app: &AppHandle, args: I, cwd: &Path, startup: bool)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    for target in parse_launch_args(args, cwd) {
        match target {
            LaunchTarget::Project(dir) => {
                if startup {
                    open_workspace_now(app, &dir);
                } else {
                    dispatch(
                        app,
                        PendingEvent::OpenProject {
                            root: dir.display().to_string(),
                        },
                    );
                }
            }
            LaunchTarget::File(request) => {
                if startup && app.state::<AppState>().project_info().is_none() {
                    // Open the file's project first, so the file opens in it right away.
                    let ctx = known_projects(app);
                    if let Ok((_, target)) = forge_open::resolve_file(&request, &ctx) {
                        open_workspace_now(app, Path::new(target.root()));
                    }
                }
                if let Err(e) = open_request(app, &request, OpenSource::Cli, None) {
                    tracing::warn!("cannot open {}: {e}", request.file);
                }
            }
            LaunchTarget::Link(link) => open_link(app, &link),
        }
    }
}

pub(crate) fn open_workspace_now(app: &AppHandle, dir: &Path) {
    match forge_core::Workspace::open(dir) {
        Ok(ws) => {
            crate::recent::remember(crate::recent::RecentProject::local(
                ws.name(),
                ws.root().display().to_string(),
            ));
            app.state::<AppState>().set_workspace(ws);
        }
        Err(e) => tracing::warn!("could not open {}: {e}", dir.display()),
    }
}

/// The IDE side of the HTTP API.
struct AppHost {
    app: AppHandle,
}

impl OpenHost for AppHost {
    fn info(&self) -> HostInfo {
        let projects = self
            .app
            .state::<AppState>()
            .project_info()
            .map(|info| {
                vec![ProjectInfo {
                    root: info.display_root(),
                    name: info.name,
                }]
            })
            .unwrap_or_default();
        HostInfo {
            name: forge_core::APP_NAME.into(),
            version: self.app.package_info().version.to_string(),
            projects,
        }
    }

    fn open(&self, request: OpenRequest, client: &ClientInfo) -> Result<OpenOutcome, HostError> {
        let event = open_request(
            &self.app,
            &request,
            OpenSource::Browser,
            Some(client.name.clone()),
        )
        .map_err(resolve_error)?;
        Ok(outcome(&event))
    }

    fn open_symbol(
        &self,
        request: SymbolRequest,
        client: &ClientInfo,
    ) -> Result<OpenOutcome, HostError> {
        let event = open_symbol(
            &self.app,
            &request,
            OpenSource::Browser,
            Some(client.name.clone()),
        )?;
        Ok(outcome(&event))
    }
}

fn outcome(event: &OpenEvent) -> OpenOutcome {
    OpenOutcome {
        status: if event.opens_directly() {
            OpenStatus::Opened
        } else {
            OpenStatus::NeedsConfirmation
        },
        file: event.file.clone(),
        line: event.line,
    }
}

/// (Re)start or stop the server according to the settings. Safe to call from a settings
/// listener: the settings are read on a background task, after the writer released them.
pub fn apply_server_settings(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let (enabled, port) = {
            let app_state = app.state::<AppState>();
            let settings = app_state.read_settings();
            let enabled = settings.get_bool(KEY_ENABLED).unwrap_or(true);
            let port = settings
                .get_i64(KEY_PORT)
                .and_then(|p| u16::try_from(p).ok())
                .filter(|p| *p >= 1024)
                .unwrap_or(DEFAULT_PORT);
            (enabled, port)
        };
        let state = app.state::<CompanionState>();
        if let Some(server) = state.server.lock().expect("server lock poisoned").take() {
            server.stop();
        }
        let status = if enabled {
            let host = Arc::new(AppHost { app: app.clone() });
            match CompanionServer::start(port, host, state.pairing.clone()).await {
                Ok(server) => {
                    let port = server.port;
                    *state.server.lock().expect("server lock poisoned") = Some(server);
                    tracing::info!("browser companion listening on 127.0.0.1:{port}");
                    ServerStatus {
                        running: true,
                        port: Some(port),
                        error: None,
                    }
                }
                Err(e) => ServerStatus {
                    running: false,
                    port: None,
                    error: Some(format!("cannot listen on port {port}: {e}")),
                },
            }
        } else {
            ServerStatus::default()
        };
        *state.status.lock().expect("status lock poisoned") = status.clone();
        let _ = app.emit(STATUS_EVENT, status);
    });
}
