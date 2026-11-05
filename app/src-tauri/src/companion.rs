//! Browser Companion and other ways to open files from outside: the local HTTP server for
//! the browser extension, `forge://` links, `forge file:12` launches, "Open With → Forge" and
//! "Open Folder in Forge" from the file manager, and the macOS open-file event. The logic
//! lives in `forge-open` (`forge_open::launch` sorts a launch into projects, project files and
//! LightEdit files); this module wires it to the windows, the settings and the UI. For a
//! remote project, browser paths are mapped to the project on the host
//! (`forge_open::resolve_remote`) and checked and searched through the project's backend.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use forge_backend::Backend;
use forge_collab::link::JoinRoute;
use forge_core::{FileKind, FileMatch};
use forge_open::cli::parse_launch_args;
use forge_open::launch::{link_folder, opened_urls_to_args, plan_launch, wants_light_edit};
use forge_open::live_edit::LiveEditHub;
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
/// A Code With Me session link to show in the Join Session dialog.
pub const JOIN_SESSION_EVENT: &str = "companion://join-session";
pub const STATUS_EVENT: &str = "companion://status";

pub const KEY_ENABLED: &str = "browserCompanion.enabled";
pub const KEY_PORT: &str = "browserCompanion.port";
pub const KEY_FOCUS: &str = "browserCompanion.focusWindow";
pub const KEY_LIVE_EDIT: &str = "liveEdit.enabled";

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
    /// Live Edit events for the paired browsers.
    pub live_edit: Arc<LiveEditHub>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PendingEvent {
    Open(OpenEvent),
    /// A folder to open as a project (the UI asks "This Window / New Window" when a project
    /// is open). `confirm`: it comes from a `forge://` link, so the UI asks first.
    #[serde(rename_all = "camelCase")]
    OpenProject {
        root: String,
        confirm: bool,
    },
    /// A Code With Me session link (canonical, checked) for the Join Session dialog.
    #[serde(rename_all = "camelCase")]
    JoinSession {
        link: String,
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

/// `OPEN_PROJECT_EVENT` payload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenProjectPayload<'a> {
    root: &'a str,
    confirm: bool,
}

/// Send an event to the project window, or queue it until its UI is ready. When Forge runs
/// with the LightEdit window only, the project window opens first and gets it then.
fn dispatch(app: &AppHandle, event: PendingEvent) {
    let state = app.state::<CompanionState>();
    let no_project_window = app.get_webview_window(crate::startup::MAIN_LABEL).is_none()
        && app.state::<crate::startup::StartupState>().finished();
    if no_project_window {
        state.ui_ready.store(false, Ordering::SeqCst);
    }
    if !state.ui_ready.load(Ordering::SeqCst) {
        state
            .pending
            .lock()
            .expect("pending lock poisoned")
            .push(event);
        if no_project_window {
            crate::startup::open_project_window(app);
        }
        return;
    }
    let sent = match &event {
        PendingEvent::Open(e) => app.emit_to(crate::startup::MAIN_LABEL, OPEN_EVENT, e),
        PendingEvent::OpenProject { root, confirm } => app.emit_to(
            crate::startup::MAIN_LABEL,
            OPEN_PROJECT_EVENT,
            OpenProjectPayload {
                root,
                confirm: *confirm,
            },
        ),
        PendingEvent::JoinSession { link } => {
            app.emit_to(crate::startup::MAIN_LABEL, JOIN_SESSION_EVENT, link)
        }
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

/// Handle a `forge://` link (from the OS, so it is not trusted). A link to a folder opens it
/// as a project once the user confirms.
pub fn open_link(app: &AppHandle, link: &LinkAction) {
    if let Some(dir) = link_folder(link) {
        open_project_later(app, &dir, true);
        return;
    }
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

/// Handle a `forge://cwm/join?…` link (from the OS, so it is not trusted): checked here, then
/// shown in the Join Session dialog of this window, or of a new window when this one is in a
/// session already. Nothing connects before the user presses Join.
pub fn open_join_link(app: &AppHandle, url: &str) -> Result<(), String> {
    let busy = app.state::<crate::commands::collab::CollabState>().busy();
    let (link, route) = forge_collab::link::route_join_url(url, busy).map_err(|e| e.to_string())?;
    match route {
        JoinRoute::ThisWindow => dispatch(app, PendingEvent::JoinSession { link }),
        JoinRoute::NewWindow => {
            crate::commands::project_windows::spawn_join_window(&link).map_err(|e| e.to_string())?
        }
    }
    Ok(())
}

/// The session link a new window was started with (`project_windows::spawn_join_window`):
/// its Join Session dialog opens once the Welcome screen is ready.
pub fn queue_startup_join(app: &AppHandle) {
    if let Some(link) = crate::commands::project_windows::take_join_link() {
        if let Err(e) = open_join_link(app, &link) {
            tracing::warn!("ignoring session link: {e}");
        }
    }
}

/// Open a folder as a project through the project window's UI ("This Window / New
/// Window" when a project is open, "Trust and Open Project?" in the window it opens in).
/// `confirm`: the request comes from a link, so the UI asks first.
pub fn open_project_later(app: &AppHandle, dir: &Path, confirm: bool) {
    dispatch(
        app,
        PendingEvent::OpenProject {
            root: dir.display().to_string(),
            confirm,
        },
    );
}

/// Requests wait for a project window.
pub fn has_pending(app: &AppHandle) -> bool {
    !app.state::<CompanionState>()
        .pending
        .lock()
        .expect("pending lock poisoned")
        .is_empty()
}

/// The project window closed while the LightEdit window keeps Forge running: close its
/// project, and queue requests until a project window is open again.
pub fn project_window_closed(app: &AppHandle) {
    app.state::<CompanionState>()
        .ui_ready
        .store(false, Ordering::SeqCst);
    // The project's lifecycle ends with its window; the LightEdit window has none of its own.
    crate::lifecycle::closing(app);
    app.state::<AppState>().close_project();
}

/// The project a launch opened in this window at startup, until the UI asks whether to
/// trust it (`trust_launch_prompt`): the question belongs to the window the project opens in.
static LAUNCH_TRUST: Mutex<Option<String>> = Mutex::new(None);

/// The project root [`handle_launch_args`] opened at startup, once.
pub fn take_launch_trust() -> Option<String> {
    LAUNCH_TRUST
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
}

/// Paths of macOS open-file events that arrived before the startup read the launch
/// arguments (`None` once it has).
static EARLY_OPENED: Mutex<Option<Vec<String>>> = Mutex::new(Some(Vec::new()));

/// The launch arguments at startup: the command line plus the files and folders of macOS
/// open-file events that arrived so far. Later events are handled as they come.
pub fn startup_launch_args() -> Vec<String> {
    let early = EARLY_OPENED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .unwrap_or_default();
    std::env::args().skip(1).chain(early).collect()
}

/// macOS open-file event (`RunEvent::Opened`: Finder "Open With", files and folders dropped
/// on the Dock icon), also while Forge runs: routed like launcher arguments. Its `forge://`
/// URLs go to the deep-link plugin.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn handle_opened_urls<I, S>(app: &AppHandle, urls: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args = opened_urls_to_args(urls);
    if args.is_empty() {
        return;
    }
    {
        let mut early = EARLY_OPENED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(queue) = early.as_mut() {
            queue.extend(args);
            return;
        }
    }
    handle_launch_args(app, args, Path::new("/"), false);
}

/// Handle launcher arguments. Folders open as projects: at startup (`startup = true`) the
/// first one opens in this window right away, later ones (second launch, "Open Folder in
/// Forge") go through the UI. Files of the open project (or of the folder opened at startup)
/// open in the project window; other files open in the LightEdit window. Returns whether the
/// launch only opened LightEdit files (at startup Forge then shows the LightEdit window only).
pub fn handle_launch_args<I, S>(app: &AppHandle, args: I, cwd: &Path, startup: bool) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    // Session links are Code With Me's; `forge_open` handles the other forge:// links.
    let (joins, args): (Vec<String>, Vec<String>) = args
        .into_iter()
        .map(|a| a.as_ref().to_string())
        .partition(|a| forge_collab::link::is_join_url(a));
    for url in &joins {
        if let Err(e) = open_join_link(app, url) {
            tracing::warn!("ignoring session link: {e}");
        }
    }
    let roots: Vec<PathBuf> = app
        .state::<AppState>()
        .local_workspace()
        .map(|ws| ws.root().to_path_buf())
        .into_iter()
        .collect();
    let force_light_edit = wants_light_edit(&args);
    let plan = plan_launch(
        parse_launch_args(&args, cwd),
        &roots,
        force_light_edit,
        startup,
    );
    let light_edit_only = plan.light_edit_only() && joins.is_empty();
    for (i, dir) in plan.projects.iter().enumerate() {
        if startup && i == 0 && roots.is_empty() {
            open_workspace_now(app, dir);
            if let Some(info) = app.state::<AppState>().project_info() {
                *LAUNCH_TRUST.lock().unwrap_or_else(|e| e.into_inner()) = Some(info.display_root());
            }
        } else {
            open_project_later(app, dir, false);
        }
    }
    for dir in &plan.link_projects {
        open_project_later(app, dir, true);
    }
    for request in &plan.project_files {
        if let Err(e) = open_request(app, request, OpenSource::Cli, None) {
            tracing::warn!("cannot open {}: {e}", request.file);
        }
    }
    crate::commands::light_edit::open_files(app, &plan.light_edit);
    for link in &plan.links {
        open_link(app, link);
    }
    light_edit_only
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

    fn live_edit(&self) -> Option<Arc<LiveEditHub>> {
        let on = self
            .app
            .state::<AppState>()
            .read_settings()
            .get_bool(KEY_LIVE_EDIT)
            .unwrap_or(true);
        on.then(|| self.app.state::<CompanionState>().live_edit.clone())
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
