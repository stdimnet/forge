//! Several project windows (Window | Next / Previous Project Window, Open Project in a New
//! Window).
//!
//! Every project window is a Forge process of its own, so each window holds its own project
//! state in Rust (`AppState`, language servers, terminals, watchers). Opening a project in a
//! new window starts another instance with the project folder and `FORGE_NEW_WINDOW=1`
//! (which skips the single-instance hand-over, the splash and the update check).
//!
//! The instances find each other through `<cache dir>/forge/project-windows.json` and a
//! control port per instance on `127.0.0.1`, which answers `info` (the open project) and
//! `focus` (come to the front) requests carrying the instance's secret token. The registry
//! model and the protocol live in `forge_window::projects`.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use forge_core::{ForgeError, Result};
use forge_window::{
    control_line, parse_control_line, ControlRequest, ProjectWindowEntry, ProjectWindowInfo,
    ProjectWindows,
};
use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// Set for instances started by [`project_window_open`].
pub const NEW_WINDOW_ENV: &str = "FORGE_NEW_WINDOW";
const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);
const IO_TIMEOUT: Duration = Duration::from_secs(1);
/// Longest request line the control port reads.
const MAX_LINE: u64 = 256;

static SECONDARY: AtomicBool = AtomicBool::new(false);
static ME: OnceLock<ProjectWindowEntry> = OnceLock::new();
/// Serializes this instance's reads and writes of the registry file.
static REGISTRY_LOCK: Mutex<()> = Mutex::new(());

/// Read (and clear) [`NEW_WINDOW_ENV`] before anything else runs, so processes started from
/// this instance (terminals, run configurations) do not inherit it.
pub fn init_from_env() {
    if std::env::var_os(NEW_WINDOW_ENV).is_some() {
        SECONDARY.store(true, Ordering::SeqCst);
        std::env::remove_var(NEW_WINDOW_ENV);
    }
}

/// This instance was opened as an additional project window.
pub fn is_secondary() -> bool {
    SECONDARY.load(Ordering::SeqCst)
}

fn registry_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("forge").join("project-windows.json"))
}

fn load_registry() -> ProjectWindows {
    registry_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|text| ProjectWindows::from_json(&text))
        .unwrap_or_default()
}

fn save_registry(registry: &ProjectWindows) {
    let Some(path) = registry_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension(format!("json.{}", std::process::id()));
    let written =
        std::fs::write(&tmp, registry.to_json()).and_then(|_| std::fs::rename(&tmp, &path));
    if let Err(e) = written {
        tracing::warn!("cannot save the project window registry: {e}");
    }
}

fn update_registry(f: impl FnOnce(&mut ProjectWindows)) {
    let _guard = REGISTRY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut registry = load_registry();
    f(&mut registry);
    save_registry(&registry);
}

fn random_token() -> String {
    let a = RandomState::new().build_hasher().finish();
    let mut h = RandomState::new().build_hasher();
    h.write_u128(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
    );
    format!("{a:016x}{:016x}", h.finish())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

/// What this window reports about itself.
fn own_info(app: &AppHandle, id: &str) -> ProjectWindowInfo {
    let info = app.state::<AppState>().project_info();
    ProjectWindowInfo {
        id: id.to_string(),
        name: info.as_ref().map(|i| i.name.clone()),
        root: info.as_ref().map(|i| i.display_root()),
        current: false,
    }
}

fn focus_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(crate::startup::MAIN_LABEL) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn serve(app: &AppHandle, stream: TcpStream, token: &str, id: &str) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let mut line = String::new();
    let Ok(reader) = stream.try_clone() else {
        return;
    };
    if BufReader::new(reader.take(MAX_LINE))
        .read_line(&mut line)
        .is_err()
    {
        return;
    }
    let answer = match parse_control_line(&line, token) {
        Some(ControlRequest::Info) => serde_json::to_string(&own_info(app, id)).unwrap_or_default(),
        Some(ControlRequest::Focus) => {
            focus_main(app);
            "ok".into()
        }
        None => return,
    };
    let mut stream = stream;
    let _ = stream.write_all(format!("{answer}\n").as_bytes());
}

/// Start this instance's control port and register it (called once from `setup`).
pub fn start(app: &AppHandle) {
    let listener = match TcpListener::bind(("127.0.0.1", 0)) {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("project windows: no control port: {e}");
            return;
        }
    };
    let Ok(addr) = listener.local_addr() else {
        return;
    };
    let entry = ProjectWindowEntry {
        id: format!("{}-{}", std::process::id(), now_ms()),
        pid: std::process::id(),
        port: addr.port(),
        token: random_token(),
        opened: now_ms(),
    };
    if ME.set(entry.clone()).is_err() {
        return;
    }
    update_registry(|r| r.register(entry.clone()));
    let app = app.clone();
    std::thread::Builder::new()
        .name("forge-project-windows".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                serve(&app, stream, &entry.token, &entry.id);
            }
        })
        .map_err(|e| tracing::warn!("project windows: {e}"))
        .ok();
}

/// Remove this instance from the registry (its main window closed).
pub fn stop() {
    if let Some(me) = ME.get() {
        update_registry(|r| {
            r.unregister(&me.id);
        });
    }
}

/// Send a request to another instance; its answer line, or `None` when it does not answer.
fn request(entry: &ProjectWindowEntry, req: ControlRequest) -> Option<String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], entry.port));
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).ok()?;
    stream.set_read_timeout(Some(IO_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(IO_TIMEOUT)).ok()?;
    stream
        .write_all(control_line(&entry.token, req).as_bytes())
        .ok()?;
    let mut line = String::new();
    BufReader::new(stream.take(64 * 1024))
        .read_line(&mut line)
        .ok()?;
    let line = line.trim().to_string();
    (!line.is_empty()).then_some(line)
}

/// The running project windows in opening order (instances that do not answer are pruned).
fn live_windows(app: &AppHandle) -> Vec<ProjectWindowInfo> {
    let me = ME.get().map(|m| m.id.clone()).unwrap_or_default();
    let registry = {
        let _guard = REGISTRY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        load_registry()
    };
    let mut infos = Vec::new();
    let mut dead = Vec::new();
    for entry in &registry.windows {
        if entry.id == me {
            infos.push(ProjectWindowInfo {
                current: true,
                ..own_info(app, &me)
            });
            continue;
        }
        match request(entry, ControlRequest::Info)
            .and_then(|line| serde_json::from_str::<ProjectWindowInfo>(&line).ok())
            .filter(|info| info.id == entry.id)
        {
            Some(info) => infos.push(ProjectWindowInfo {
                current: false,
                ..info
            }),
            None => dead.push(entry.id.clone()),
        }
    }
    if !dead.is_empty() {
        update_registry(|r| {
            r.retain_alive(|w| !dead.contains(&w.id));
        });
    }
    infos
}

fn focus_other(id: &str) -> Result<()> {
    let registry = load_registry();
    let entry = registry
        .get(id)
        .ok_or_else(|| ForgeError::Other("that project window is closed".into()))?;
    request(entry, ControlRequest::Focus)
        .map(|_| ())
        .ok_or_else(|| ForgeError::Other("that project window does not answer".into()))
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| ForgeError::Other(e.to_string()))?
}

/// Window | the open project windows (this one marked `current`).
#[tauri::command]
pub async fn project_windows_list(app: AppHandle) -> Result<Vec<ProjectWindowInfo>> {
    blocking(move || Ok(live_windows(&app))).await
}

/// Bring another project window to the front.
#[tauri::command]
pub async fn project_window_focus(id: String) -> Result<()> {
    blocking(move || focus_other(&id)).await
}

/// Next Project Window (`delta = 1`) / Previous Project Window (`-1`). Returns the window
/// that came to the front, or `None` when this is the only one.
#[tauri::command]
pub async fn project_window_cycle(app: AppHandle, delta: i32) -> Result<Option<ProjectWindowInfo>> {
    blocking(move || {
        let windows = live_windows(&app);
        let me = ME.get().map(|m| m.id.clone()).unwrap_or_default();
        let registry = ProjectWindows {
            windows: windows
                .iter()
                .map(|w| ProjectWindowEntry {
                    id: w.id.clone(),
                    pid: 0,
                    port: 1,
                    token: String::new(),
                    opened: 0,
                })
                .collect(),
        };
        let Some(next) = registry.cycle(&me, delta.signum()) else {
            return Ok(None);
        };
        focus_other(&next.id)?;
        Ok(windows.into_iter().find(|w| w.id == next.id))
    })
    .await
}

/// The program to start for a new project window (the AppImage itself, not its mount).
fn own_program() -> Result<PathBuf> {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return Ok(PathBuf::from(appimage));
    }
    std::env::current_exe().map_err(|e| ForgeError::Other(format!("cannot find Forge itself: {e}")))
}

/// Open a local project in a new window: a window that already shows it comes to the front,
/// else a new Forge instance opens it.
#[tauri::command]
pub async fn project_window_open(app: AppHandle, path: String) -> Result<()> {
    blocking(move || {
        let dir = PathBuf::from(&path);
        if !dir.is_dir() {
            return Err(ForgeError::Other(format!("not a folder: {path}")));
        }
        let canonical = dir.canonicalize().unwrap_or(dir.clone());
        let same = |root: &str| {
            let root = PathBuf::from(root);
            root.canonicalize().unwrap_or(root) == canonical
        };
        if let Some(open) = live_windows(&app)
            .into_iter()
            .find(|w| w.root.as_deref().is_some_and(same))
        {
            if open.current {
                focus_main(&app);
                return Ok(());
            }
            return focus_other(&open.id);
        }
        spawn_window(Some(&canonical))
    })
    .await
}

/// File | New Window: another Forge instance on the Welcome screen (no project, the last
/// one is not reopened), e.g. to join a Code With Me session next to the open project.
#[tauri::command]
pub async fn project_window_new() -> Result<()> {
    blocking(|| spawn_window(None)).await
}

/// Start another instance as an additional project window, with `project` or without one.
fn spawn_window(project: Option<&std::path::Path>) -> Result<()> {
    let mut command = std::process::Command::new(own_program()?);
    if let Some(project) = project {
        command.arg(project);
    }
    let mut child = command
        .env(NEW_WINDOW_ENV, "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| ForgeError::Other(format!("cannot open a new window: {e}")))?;
    // Reap the instance when it exits (it outlives this one if this window closes first).
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
