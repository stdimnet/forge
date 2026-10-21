//! Window chrome (Forge's own title bar controls) and dialog windows.
//!
//! Dialogs such as Settings open as separate, owned windows (`dialog-<id>`) that load the same
//! UI bundle with `?dialog=<id>`. While a dialog is open its parent is disabled and focusing the
//! parent focuses the dialog, like a modal dialog. Closing reports the dialog's result to the
//! parent window (`dialog://closed`), and the dialog's last geometry is remembered in
//! `<config dir>/forge/dialog-windows.json`. Pure logic lives in `forge-window`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use forge_core::{ForgeError, Result};
use forge_window::{
    detect_desktop, dialog_id, dialog_label, place_dialog, ButtonLayout, DialogStates, Os, Rect,
    Size, SystemLayouts, WindowChrome,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, Window, WindowEvent,
};

use crate::state::AppState;

/// Setting with the title bar style (`auto | windows | macos | gnome | kde | linux | native`).
pub const SETTING_KEY: &str = "appearance.windowControls";
/// Sent to a dialog window when the OS asks to close it (Alt+F4, taskbar), so the dialog
/// runs its Cancel path. The dialog then closes itself with `dialog_window_close`.
const CLOSE_REQUESTED_EVENT: &str = "dialog://close-requested";
/// Sent to the parent window when a dialog closed: `{ id, result }`.
const CLOSED_EVENT: &str = "dialog://closed";
/// Sent to the parent window by `dialog_window_message`: `{ id, message }`.
const MESSAGE_EVENT: &str = "dialog://message";
/// A dialog that did not answer a close request within this time is closed anyway.
const CLOSE_GRACE: Duration = Duration::from_secs(3);

struct OpenDialog {
    id: String,
    parent: String,
    result: Option<Value>,
    /// The dialog asked to close (`dialog_window_close`).
    closing: bool,
    close_requested: Option<Instant>,
    geometry: Option<Rect>,
}

/// Open dialog windows by label and the remembered dialog geometry.
#[derive(Default)]
pub struct DialogWindowsState {
    open: Mutex<HashMap<String, OpenDialog>>,
    states: Mutex<Option<DialogStates>>,
}

impl DialogWindowsState {
    fn with_states<T>(&self, f: impl FnOnce(&mut DialogStates) -> T) -> T {
        let mut guard = self.states.lock().expect("dialog states lock poisoned");
        let states = guard.get_or_insert_with(|| {
            states_path()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .map(|text| DialogStates::from_json(&text))
                .unwrap_or_default()
        });
        f(states)
    }
}

fn states_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("forge").join("dialog-windows.json"))
}

fn save_states(states: &DialogStates) {
    let Some(path) = states_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, states.to_json()) {
        tracing::warn!("cannot save dialog window geometry: {e}");
    }
}

// ---- chrome ------------------------------------------------------------------------------

/// Button layouts the desktop is configured with (GNOME `button-layout`, KDE `kwinrc`),
/// read once.
fn system_layouts() -> &'static SystemLayouts {
    static LAYOUTS: OnceLock<SystemLayouts> = OnceLock::new();
    LAYOUTS.get_or_init(|| {
        if Os::current() != Os::Linux {
            return SystemLayouts::default();
        }
        let gnome = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.wm.preferences", "button-layout"])
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| ButtonLayout::parse_gnome(&String::from_utf8_lossy(&o.stdout)));
        let kde = dirs::config_dir()
            .and_then(|d| std::fs::read_to_string(d.join("kwinrc")).ok())
            .and_then(|text| ButtonLayout::from_kwinrc(&text));
        SystemLayouts { gnome, kde }
    })
}

fn chrome_for(setting: &str) -> WindowChrome {
    let os = Os::current();
    let desktop = detect_desktop(os, |k| std::env::var(k).ok());
    forge_window::resolve(setting, os, desktop, system_layouts())
}

fn setting(state: &AppState) -> String {
    state
        .read_settings()
        .get_as::<String>(SETTING_KEY)
        .unwrap_or_else(|| "auto".into())
}

/// The title bar style for this platform and the `appearance.windowControls` setting;
/// `style` overrides the setting (screenshots, previews).
#[tauri::command]
pub fn window_chrome(style: Option<String>, state: State<'_, AppState>) -> WindowChrome {
    chrome_for(&style.unwrap_or_else(|| setting(&state)))
}

/// Give every window OS decorations (`native`) or none (Forge draws the title bar). The
/// splash window never has decorations.
pub fn apply_decorations(app: &AppHandle) {
    let native = chrome_for(&setting(&app.state::<AppState>())).native;
    for window in app.webview_windows().values() {
        if window.label() == crate::startup::SPLASH_LABEL {
            continue;
        }
        if let Err(e) = window.set_decorations(native) {
            tracing::warn!("cannot change window decorations: {e}");
        }
    }
}

// ---- dialog windows ----------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogWindowRequest {
    /// Registered dialog id (`settings`, `run-configurations`, …).
    pub id: String,
    pub title: String,
    /// Default size in logical pixels (CSS pixels).
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub min_width: Option<u32>,
    #[serde(default)]
    pub min_height: Option<u32>,
    /// Passed to the dialog as `window.__FORGE_DIALOG__.props`.
    #[serde(default)]
    pub props: Value,
}

#[derive(Debug, Clone, Serialize)]
struct DialogBoot<'a> {
    id: &'a str,
    parent: &'a str,
    props: &'a Value,
}

/// Payloads carry the label of the window they are meant for (`window`), so a window can
/// ignore events for others.
#[derive(Debug, Clone, Serialize)]
struct DialogClosed {
    window: String,
    id: String,
    result: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
struct DialogMessage {
    window: String,
    id: String,
    message: Value,
}

#[derive(Debug, Clone, Serialize)]
struct CloseRequested {
    window: String,
}

fn err(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

fn outer_rect(window: &impl WindowGeometry) -> Option<Rect> {
    let pos = window.outer_position_px()?;
    let size = window.inner_size_px()?;
    Some(Rect::new(pos.x, pos.y, size.width, size.height))
}

/// `Window` and `WebviewWindow` share these getters; this keeps `outer_rect` generic.
trait WindowGeometry {
    fn outer_position_px(&self) -> Option<PhysicalPosition<i32>>;
    fn inner_size_px(&self) -> Option<PhysicalSize<u32>>;
}

impl WindowGeometry for Window {
    fn outer_position_px(&self) -> Option<PhysicalPosition<i32>> {
        self.outer_position().ok()
    }
    fn inner_size_px(&self) -> Option<PhysicalSize<u32>> {
        self.inner_size().ok()
    }
}

impl WindowGeometry for WebviewWindow {
    fn outer_position_px(&self) -> Option<PhysicalPosition<i32>> {
        self.outer_position().ok()
    }
    fn inner_size_px(&self) -> Option<PhysicalSize<u32>> {
        self.inner_size().ok()
    }
}

/// Open (or focus) the dialog window for `request.id`, owned by the calling window.
#[tauri::command]
pub async fn dialog_window_open(
    app: AppHandle,
    window: WebviewWindow,
    request: DialogWindowRequest,
    dialogs: State<'_, DialogWindowsState>,
    state: State<'_, AppState>,
) -> Result<()> {
    let label = dialog_label(&request.id)
        .ok_or_else(|| err(format!("invalid dialog id {:?}", request.id)))?;
    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.unminimize();
        existing.set_focus().map_err(err)?;
        return Ok(());
    }
    let parent = window;
    let chrome = chrome_for(&setting(&state));
    let scale = parent.scale_factor().unwrap_or(1.0);
    let px = |logical: u32| (logical as f64 * scale).round() as u32;
    let default = Size {
        width: px(request.width),
        height: px(request.height),
    };
    let min = Size {
        width: px(request.min_width.unwrap_or(request.width.min(480))),
        height: px(request.min_height.unwrap_or(request.height.min(320))),
    };
    let parent_rect = outer_rect(&parent).unwrap_or(Rect::new(0, 0, default.width, default.height));
    let monitors: Vec<Rect> = parent
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let area = m.work_area();
            Rect::new(
                area.position.x,
                area.position.y,
                area.size.width,
                area.size.height,
            )
        })
        .collect();
    let remembered = dialogs.with_states(|s| s.get(&request.id));
    let rect = place_dialog(parent_rect, default, min, remembered, &monitors);

    let boot = DialogBoot {
        id: &request.id,
        parent: parent.label(),
        props: &request.props,
    };
    let init = format!(
        "window.__FORGE_DIALOG__ = {};",
        serde_json::to_string(&boot).map_err(err)?
    );
    let url = WebviewUrl::App(format!("index.html?dialog={}", request.id).into());
    let dialog = WebviewWindowBuilder::new(&app, &label, url)
        .title(&request.title)
        .decorations(chrome.native)
        .resizable(true)
        .minimizable(false)
        .skip_taskbar(true)
        .visible(false)
        .min_inner_size(min.width as f64 / scale, min.height as f64 / scale)
        .initialization_script(&init)
        .parent(&parent)
        .map_err(err)?
        .build()
        .map_err(err)?;
    dialogs.open.lock().expect("dialogs lock poisoned").insert(
        label.clone(),
        OpenDialog {
            id: request.id.clone(),
            parent: parent.label().to_string(),
            result: None,
            closing: false,
            close_requested: None,
            geometry: Some(rect),
        },
    );
    let _ = dialog.set_size(PhysicalSize::new(rect.width, rect.height));
    let _ = dialog.set_position(PhysicalPosition::new(rect.x, rect.y));
    dialog.show().map_err(err)?;
    let _ = dialog.set_focus();
    // Modal: the parent takes no input until the dialog closes.
    let _ = parent.set_enabled(false);
    Ok(())
}

/// Close the calling dialog window and report `result` to its parent (`null` = cancelled).
#[tauri::command]
pub fn dialog_window_close(
    window: Window,
    result: Option<Value>,
    dialogs: State<'_, DialogWindowsState>,
) -> Result<()> {
    {
        let mut open = dialogs.open.lock().expect("dialogs lock poisoned");
        let Some(entry) = open.get_mut(window.label()) else {
            return Err(err("not a dialog window"));
        };
        entry.result = result;
        entry.closing = true;
    }
    window.close().map_err(err)
}

/// Send `message` from the calling dialog window to its parent (e.g. after Apply).
#[tauri::command]
pub fn dialog_window_message(
    app: AppHandle,
    window: Window,
    message: Value,
    dialogs: State<'_, DialogWindowsState>,
) -> Result<()> {
    let (id, parent) = {
        let open = dialogs.open.lock().expect("dialogs lock poisoned");
        let entry = open
            .get(window.label())
            .ok_or_else(|| err("not a dialog window"))?;
        (entry.id.clone(), entry.parent.clone())
    };
    let payload = DialogMessage {
        window: parent.clone(),
        id,
        message,
    };
    app.emit_to(parent.as_str(), MESSAGE_EVENT, payload)
        .map_err(err)
}

/// Window events of all windows (registered in `lib.rs`).
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    let label = window.label().to_string();
    let app = window.app_handle();
    let dialogs = app.state::<DialogWindowsState>();
    // Focusing a window that owns an open dialog brings the dialog to the front.
    if let WindowEvent::Focused(true) = event {
        let child = {
            let open = dialogs.open.lock().expect("dialogs lock poisoned");
            open.iter()
                .find(|(_, d)| d.parent == label)
                .map(|(l, _)| l.clone())
        };
        if let Some(dialog) = child.and_then(|l| app.get_webview_window(&l)) {
            let _ = dialog.set_focus();
        }
    }
    if dialog_id(&label).is_none() {
        return;
    }
    match event {
        WindowEvent::CloseRequested { api, .. } => {
            let mut open = dialogs.open.lock().expect("dialogs lock poisoned");
            let Some(entry) = open.get_mut(&label) else {
                return;
            };
            let overdue = entry
                .close_requested
                .is_some_and(|t| t.elapsed() > CLOSE_GRACE);
            if entry.closing || overdue {
                return;
            }
            // Let the dialog run its Cancel path (it may refuse while busy).
            api.prevent_close();
            entry.close_requested.get_or_insert_with(Instant::now);
            drop(open);
            let _ = app.emit_to(
                label.as_str(),
                CLOSE_REQUESTED_EVENT,
                CloseRequested {
                    window: label.clone(),
                },
            );
        }
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
            let minimized = window.is_minimized().unwrap_or(false);
            let maximized = window.is_maximized().unwrap_or(false);
            if minimized || maximized {
                return;
            }
            if let Some(rect) = outer_rect(window) {
                if let Some(entry) = dialogs
                    .open
                    .lock()
                    .expect("dialogs lock poisoned")
                    .get_mut(&label)
                {
                    entry.geometry = Some(rect);
                }
            }
        }
        WindowEvent::Destroyed => {
            let Some(entry) = dialogs
                .open
                .lock()
                .expect("dialogs lock poisoned")
                .remove(&label)
            else {
                return;
            };
            if let Some(rect) = entry.geometry {
                if let Some(json) =
                    dialogs.with_states(|s| s.remember(&entry.id, rect).then(|| s.clone()))
                {
                    save_states(&json);
                }
            }
            if let Some(parent) = app.get_webview_window(&entry.parent) {
                let _ = parent.set_enabled(true);
                let _ = parent.set_focus();
            }
            let closed = DialogClosed {
                window: entry.parent.clone(),
                id: entry.id,
                result: entry.result,
            };
            if let Err(e) = app.emit_to(entry.parent.as_str(), CLOSED_EVENT, closed) {
                tracing::warn!("cannot report a closed dialog: {e}");
            }
        }
        _ => {}
    }
}
