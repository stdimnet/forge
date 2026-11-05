//! Startup: the splash window, the loading steps and the main window.
//!
//! `setup` opens the `splash` window (unless `appearance.showSplash` is off) and starts
//! [`run_steps`] on its own thread. Each step reports what it is doing over
//! `splash://progress` (`{ step, detail, percent }`) right before doing it, so the molten fill
//! of the wordmark is the real progress: core services, plugins, the project from the command
//! line or the last session (with framework detection), background services, the update check
//! and finally the workbench. The main window is created hidden once loading is done and shown
//! when its UI has rendered (`startup_workbench_ready`); then the splash closes. When Forge
//! starts only for files ("Open With → Forge"), the LightEdit window is the first window
//! instead, and the project window opens later, when a project is opened
//! ([`open_project_window`]).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use forge_update::UpdateOffer;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow, WebviewWindowBuilder};

use crate::commands::updates;
use crate::state::AppState;

pub const SPLASH_LABEL: &str = "splash";
pub const MAIN_LABEL: &str = "main";
/// `{ step, detail, percent }` while Forge starts.
pub const PROGRESS_EVENT: &str = "splash://progress";
/// Setting that turns the splash window off.
pub const SHOW_SPLASH_KEY: &str = "appearance.showSplash";
/// The main window is shown after this long even if its UI never reports ready.
const READY_TIMEOUT: Duration = Duration::from_secs(20);

/// The splash status line and the fill of the wordmark.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub step: String,
    pub detail: String,
    /// 0–100.
    pub percent: u8,
}

/// What the splash window asks for when it has loaded (events sent earlier are gone).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplashSnapshot {
    pub progress: Progress,
    pub version: String,
    /// `appearance.theme`.
    pub theme: String,
    /// The update waiting for an answer, if any.
    pub offer: Option<UpdateOffer>,
}

#[derive(Default)]
pub struct StartupState {
    progress: Mutex<Progress>,
    /// The workbench is shown; the splash is gone.
    finished: AtomicBool,
    /// Labels of the windows that rendered their first screen (`main`, the LightEdit window).
    ready_windows: Mutex<Vec<String>>,
    ready_signal: Condvar,
    /// An update found while no splash window was shown; the workbench offers it.
    offer_for_workbench: Mutex<Option<UpdateOffer>>,
}

impl StartupState {
    pub fn finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }
}

/// Report a step to the splash window.
pub fn progress(app: &AppHandle, step: &str, detail: impl Into<String>, percent: u8) {
    let p = Progress {
        step: step.into(),
        detail: detail.into(),
        percent: percent.min(100),
    };
    tracing::debug!("startup: {} {} ({}%)", p.step, p.detail, p.percent);
    *app.state::<StartupState>()
        .progress
        .lock()
        .expect("progress lock poisoned") = p.clone();
    if app.get_webview_window(SPLASH_LABEL).is_some() {
        let _ = app.emit_to(SPLASH_LABEL, PROGRESS_EVENT, p);
    }
}

pub fn splash_open(app: &AppHandle) -> bool {
    app.get_webview_window(SPLASH_LABEL).is_some() && !app.state::<StartupState>().finished()
}

/// Called from `setup`: open the splash window and start loading.
pub fn begin(app: &AppHandle) {
    let show = app
        .state::<AppState>()
        .read_settings()
        .get_bool(SHOW_SPLASH_KEY)
        .unwrap_or(true);
    // Additional project windows open without a splash, like PHPStorm.
    if show && !crate::commands::project_windows::is_secondary() {
        // The static page of the splash (`splash.html`) paints in the theme of the settings.
        let theme = serde_json::to_string(&splash_theme(app)).unwrap_or_else(|_| "\"dark\"".into());
        let opened = window_from_config(app, SPLASH_LABEL).and_then(|b| {
            b.initialization_script(format!("window.__FORGE_SPLASH_THEME__ = {theme};"))
                .build()
        });
        if let Err(e) = opened {
            tracing::warn!("cannot open the splash window: {e}");
        }
    }
    let app = app.clone();
    std::thread::Builder::new()
        .name("forge-startup".into())
        .spawn(move || run_steps(&app))
        .expect("cannot start the startup thread");
}

/// A window builder from the (not auto-created) window in `tauri.conf.json`.
fn window_from_config<'a>(
    app: &'a AppHandle,
    label: &str,
) -> tauri::Result<WebviewWindowBuilder<'a, tauri::Wry, AppHandle>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == label)
        .cloned()
        .ok_or_else(|| tauri::Error::WindowNotFound)?;
    WebviewWindowBuilder::from_config(app, &config)
}

fn run_steps(app: &AppHandle) {
    let state = app.state::<AppState>();

    // 1. Core: the keymap (PHPStorm's keymap plus the user's overrides).
    progress(app, "Loading the core", "settings · keymap", 4);
    app.manage(crate::commands::keymap::KeymapState::default());

    // 2. Plugins: the settings listeners of the built-in plugins and their servers.
    let plugins = state
        .plugins
        .read()
        .expect("plugins lock poisoned")
        .list()
        .iter()
        .filter(|p| p.enabled)
        .count();
    progress(app, "Loading plugins", format!("{plugins} plugins"), 16);
    crate::activate_plugins(app);

    // 3. The project from the command line, a forge:// link or the last session.
    progress(app, "Opening project", "", 34);
    let cwd = std::env::current_dir().unwrap_or_default();
    let secondary = crate::commands::project_windows::is_secondary();
    let light_edit_only = crate::companion::handle_launch_args(
        app,
        crate::companion::startup_launch_args(),
        &cwd,
        true,
    ) && !secondary;
    // Opened for files only: the LightEdit window, without the last project.
    if state.project_info().is_none() && !secondary && !light_edit_only {
        reopen_last_project(app);
    }
    let detected = match state.backend() {
        Ok(backend) => {
            let name = backend.info().name.clone();
            progress(app, "Opening project", format!("{name} · detecting"), 42);
            match backend.detect_project() {
                Ok(info) => {
                    let kind = info
                        .primary
                        .as_ref()
                        .map(|f| f.name.clone())
                        .unwrap_or_else(|| "no framework".into());
                    progress(app, "Opening project", format!("{name} · {kind}"), 50);
                    Some((backend.info().root.clone(), info))
                }
                Err(e) => {
                    tracing::warn!("project detection failed: {e}");
                    None
                }
            }
        }
        Err(_) => {
            progress(app, "Opening project", "Welcome screen", 50);
            None
        }
    };

    // 4. Background services.
    progress(app, "Starting background services", "language servers", 56);
    crate::commands::php::sync_lsp_server(app, detected.as_ref().map(|(r, i)| (r.as_path(), i)));
    crate::commands::java::sync_lsp_server(app, detected.as_ref().map(|(r, i)| (r.as_path(), i)));
    crate::commands::kotlin::sync_lsp_server(app, detected.as_ref().map(|(r, i)| (r.as_path(), i)));
    crate::commands::cpp::sync_lsp_server(app, detected.as_ref().map(|(r, i)| (r.as_path(), i)));
    crate::commands::scala::sync_lsp_server(app, detected.as_ref().map(|(r, i)| (r.as_path(), i)));
    crate::commands::dotnet::sync_lsp_server(app, detected.as_ref().map(|(r, i)| (r.as_path(), i)));
    crate::commands::gamedev::sync_lsp_server(
        app,
        detected.as_ref().map(|(r, i)| (r.as_path(), i)),
    );
    if let Ok(root) = state.project_root() {
        progress(app, "Starting background services", "file watcher", 62);
        crate::commands::workspace::start_watching(app);
        progress(app, "Starting background services", "data sources", 66);
        crate::commands::db::project_opened(app, root);
    }
    progress(
        app,
        "Starting background services",
        "Docker · Kubernetes",
        70,
    );
    crate::commands::services::start(app);
    // The project from the command line or the last session: its lifecycle starts here (the
    // UI adopts it with `workspace_current` once it renders).
    crate::lifecycle::opened(app);

    // 5. Updates (the splash may offer one and wait for the answer). The first project
    // window checks; additional ones do not ask again.
    let offer = if crate::commands::project_windows::is_secondary() {
        None
    } else {
        updates::startup_check(app)
    };
    if let Some(offer) = offer {
        if splash_open(app) {
            updates::offer_on_splash(app, offer);
        } else {
            *app.state::<StartupState>()
                .offer_for_workbench
                .lock()
                .expect("offer lock poisoned") = Some(offer);
        }
    }

    // 6. The workbench.
    progress(app, "Opening workbench", "", 92);
    if light_edit_only {
        open_light_edit(app);
    } else {
        open_workbench(app);
    }
}

/// Forge started for files only: wait until the LightEdit window (opened by
/// `handle_launch_args`) has rendered, then close the splash. No project window opens.
fn open_light_edit(app: &AppHandle) {
    let startup = app.state::<StartupState>();
    wait_until_ready(&startup, &crate::commands::light_edit::label());
    progress(app, "Ready", "", 100);
    startup.finished.store(true, Ordering::SeqCst);
    if let Some(splash) = app.get_webview_window(SPLASH_LABEL) {
        let _ = splash.close();
    }
    // A folder that arrived meanwhile (a second launch) waits for a project window.
    if crate::companion::has_pending(app) {
        open_project_window(app);
    }
}

/// Wait until the window `label` rendered its first screen (or the timeout passed).
fn wait_until_ready(startup: &StartupState, label: &str) {
    let ready = startup.ready_windows.lock().expect("ready lock poisoned");
    let (_ready, timeout) = startup
        .ready_signal
        .wait_timeout_while(ready, READY_TIMEOUT, |ready| {
            !ready.iter().any(|l| l == label)
        })
        .expect("ready lock poisoned");
    if timeout.timed_out() {
        tracing::warn!("{label} did not report ready; showing it anyway");
    }
}

/// A window rendered its first screen (the startup waits for the first one).
pub fn window_ready(app: &AppHandle, label: &str) {
    let startup = app.state::<StartupState>();
    let mut ready = startup.ready_windows.lock().expect("ready lock poisoned");
    if !ready.iter().any(|l| l == label) {
        ready.push(label.to_string());
    }
    startup.ready_signal.notify_all();
}

/// Open the project window when Forge runs with the LightEdit window only (a folder was
/// opened from there or from outside). It is shown when its UI has rendered.
pub fn open_project_window(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        if app.get_webview_window(MAIN_LABEL).is_some() {
            return;
        }
        match window_from_config(&app, MAIN_LABEL).and_then(|b| b.visible(false).build()) {
            Ok(main) => {
                crate::commands::window::apply_decorations(&app);
                crate::commands::project_windows::resume();
                std::thread::sleep(READY_TIMEOUT);
                if !main.is_visible().unwrap_or(true) {
                    let _ = main.show();
                }
            }
            Err(e) => tracing::error!("cannot open the project window: {e}"),
        }
    });
}

/// `system.reopenLastProject`: open the most recent local project that still exists.
fn reopen_last_project(app: &AppHandle) {
    let reopen = app
        .state::<AppState>()
        .read_settings()
        .get_bool("system.reopenLastProject")
        .unwrap_or(true);
    if !reopen {
        return;
    }
    let last = crate::recent::load()
        .into_iter()
        .find(|p| p.remote.is_none() && std::path::Path::new(&p.root).is_dir());
    if let Some(project) = last {
        progress(app, "Opening project", project.name.clone(), 38);
        crate::companion::open_workspace_now(app, std::path::Path::new(&project.root));
    }
}

/// Create the main window hidden, wait until its UI has rendered, show it and close the
/// splash.
fn open_workbench(app: &AppHandle) {
    let startup = app.state::<StartupState>();
    let main = match app.get_webview_window(MAIN_LABEL) {
        Some(w) => Some(w),
        None => match window_from_config(app, MAIN_LABEL).and_then(|b| b.visible(false).build()) {
            Ok(w) => Some(w),
            Err(e) => {
                tracing::error!("cannot open the main window: {e}");
                None
            }
        },
    };
    crate::commands::window::apply_decorations(app);
    wait_until_ready(&startup, MAIN_LABEL);
    progress(app, "Ready", "", 100);
    if let Some(main) = main {
        let _ = main.show();
        let _ = main.set_focus();
    }
    startup.finished.store(true, Ordering::SeqCst);
    if let Some(splash) = app.get_webview_window(SPLASH_LABEL) {
        let _ = splash.close();
    }
}

/// The main window rendered its first screen. Returns an update found during startup that
/// the splash could not offer (splash turned off).
#[tauri::command]
pub fn startup_workbench_ready(
    app: AppHandle,
    window: WebviewWindow,
    startup: State<'_, StartupState>,
) -> Option<UpdateOffer> {
    if window.label() != MAIN_LABEL {
        return None;
    }
    window_ready(&app, MAIN_LABEL);
    // A project window opened after the startup (from the LightEdit window) shows itself.
    if startup.finished() && !window.is_visible().unwrap_or(true) {
        let _ = window.show();
        let _ = window.set_focus();
    }
    startup
        .offer_for_workbench
        .lock()
        .expect("offer lock poisoned")
        .take()
}

/// The theme setting, as the splash shows it.
fn splash_theme(app: &AppHandle) -> String {
    app.state::<AppState>()
        .read_settings()
        .get_as::<String>("appearance.theme")
        .unwrap_or_else(|| "dark".into())
}

/// The splash window's current state.
#[tauri::command]
pub fn splash_state(app: AppHandle, startup: State<'_, StartupState>) -> SplashSnapshot {
    let theme = splash_theme(&app);
    SplashSnapshot {
        progress: startup
            .progress
            .lock()
            .expect("progress lock poisoned")
            .clone(),
        version: app.package_info().version.to_string(),
        theme,
        offer: updates::waiting_offer(&app),
    }
}

/// Closing the splash before the workbench is shown quits Forge.
pub fn on_splash_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() != SPLASH_LABEL {
        return;
    }
    if let tauri::WindowEvent::CloseRequested { .. } = event {
        let app = window.app_handle();
        if !app.state::<StartupState>().finished() {
            app.exit(0);
        }
    }
}
