//! The LightEdit window: files opened from outside a project ("Open With → Forge", `forge -e
//! file`, a file outside the open project) open as tabs of one lightweight editor window,
//! without a project, tool windows or indexing. The tab model is
//! `forge_window::light_edit`; the window is a non-modal dialog window without a parent
//! (`dialog-light-edit`, `index.html?dialog=light-edit`), so it stays open when the project
//! window closes and opens on its own when Forge starts for a file.
//!
//! Only files that are tabs of the window can be read and written here. Each file keeps the
//! encoding, byte order mark and line separator it was read with.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use forge_core::{ForgeError, Result};
use forge_open::OpenRequest;
use forge_text::file::{read_text, write_text};
use forge_text::{Encoding, LineSeparator};
use forge_window::{LightEditSnapshot, LightEditTabs, LIGHT_EDIT_ID};
use tauri::{AppHandle, Emitter, Manager, State};

use super::workspace::blocking;

/// Sent to the LightEdit window when files were added from outside (a later launch).
pub const CHANGED_EVENT: &str = "light-edit://changed";

/// How a file was read, so saving keeps it.
#[derive(Debug, Clone, Copy)]
struct Format {
    eol: LineSeparator,
    encoding: Encoding,
    bom: bool,
}

pub struct LightEditState {
    tabs: Mutex<LightEditTabs>,
    formats: Mutex<HashMap<String, Format>>,
}

impl Default for LightEditState {
    fn default() -> Self {
        Self {
            tabs: Mutex::new(LightEditTabs::new(cfg!(any(windows, target_os = "macos")))),
            formats: Mutex::default(),
        }
    }
}

impl LightEditState {
    fn tabs(&self) -> std::sync::MutexGuard<'_, LightEditTabs> {
        self.tabs.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn path_of(&self, id: &str) -> Result<PathBuf> {
        self.tabs()
            .path_of(id)
            .map(PathBuf::from)
            .ok_or_else(|| ForgeError::Other(format!("not a LightEdit file: {id}")))
    }
}

/// Label of the LightEdit window.
pub fn label() -> String {
    forge_window::dialog_label(LIGHT_EDIT_ID).unwrap_or_default()
}

pub fn is_open(app: &AppHandle) -> bool {
    app.get_webview_window(&label()).is_some()
}

/// Open files in the LightEdit window: new tabs, or the tab of a file that is open already.
/// Opens the window when it is not open (on a thread: building a window from the main
/// thread's event handlers deadlocks on Windows).
pub fn open_files(app: &AppHandle, requests: &[OpenRequest]) {
    if requests.is_empty() {
        return;
    }
    let snapshot = {
        let state = app.state::<LightEditState>();
        let mut tabs = state.tabs();
        for r in requests {
            tabs.open(&r.file, r.line, r.column);
        }
        tabs.snapshot()
    };
    let label = label();
    if let Some(window) = app.get_webview_window(&label) {
        if let Err(e) = app.emit_to(label.as_str(), CHANGED_EVENT, &snapshot) {
            tracing::warn!("cannot update the LightEdit window: {e}");
        }
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = super::window::open_standalone(
            &app,
            LIGHT_EDIT_ID,
            "LightEdit – Forge",
            (1000, 720),
            (480, 320),
        ) {
            tracing::error!("cannot open the LightEdit window: {e}");
        }
    });
}

/// The LightEdit window closed: its tabs are gone.
pub fn closed(app: &AppHandle) {
    let state = app.state::<LightEditState>();
    state.tabs().clear();
    state
        .formats
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();
}

/// The window's tabs (when it opens, and after a reload).
#[tauri::command]
pub fn light_edit_state(state: State<'_, LightEditState>) -> LightEditSnapshot {
    state.tabs().snapshot()
}

/// File | Open File… of the LightEdit window: add files picked in the system dialog.
#[tauri::command]
pub fn light_edit_open(paths: Vec<String>, state: State<'_, LightEditState>) -> LightEditSnapshot {
    let mut tabs = state.tabs();
    for path in paths {
        let path = PathBuf::from(path);
        if !path.is_absolute() || !path.is_file() {
            tracing::warn!("LightEdit: not a file: {}", path.display());
            continue;
        }
        let path = path.canonicalize().unwrap_or(path);
        tabs.open(&path.display().to_string(), None, None);
    }
    tabs.snapshot()
}

/// A tab was closed in the window.
#[tauri::command]
pub fn light_edit_close(id: String, state: State<'_, LightEditState>) -> LightEditSnapshot {
    let mut tabs = state.tabs();
    tabs.close(&id);
    state
        .formats
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id);
    tabs.snapshot()
}

/// The user switched tabs.
#[tauri::command]
pub fn light_edit_activate(id: String, state: State<'_, LightEditState>) -> LightEditSnapshot {
    let mut tabs = state.tabs();
    tabs.activate(&id);
    tabs.snapshot()
}

/// The text of a tab's file (`\n` separators), its encoding detected.
#[tauri::command]
pub async fn light_edit_read(id: String, app: AppHandle) -> Result<String> {
    let path = app.state::<LightEditState>().path_of(&id)?;
    let file = blocking(move || {
        let bytes = std::fs::read(&path)
            .map_err(|e| ForgeError::Other(format!("cannot read {}: {e}", path.display())))?;
        Ok(read_text(&bytes, None))
    })
    .await?;
    app.state::<LightEditState>()
        .formats
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(
            id,
            Format {
                eol: file.eol,
                encoding: file.encoding,
                bom: file.bom,
            },
        );
    Ok(file.text)
}

/// Save a tab's file in the encoding and with the line separator it was read with.
#[tauri::command]
pub async fn light_edit_write(id: String, text: String, app: AppHandle) -> Result<()> {
    let state = app.state::<LightEditState>();
    let path = state.path_of(&id)?;
    let format = state
        .formats
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .copied();
    let text = forge_text::eol::normalize(&text);
    let (eol, encoding, bom) = format.map_or(
        (LineSeparator::default(), Encoding::default(), false),
        |f| (f.eol, f.encoding, f.bom),
    );
    let bytes =
        write_text(&text, eol, encoding, bom).map_err(|e| ForgeError::Other(e.to_string()))?;
    blocking(move || {
        std::fs::write(&path, bytes)
            .map_err(|e| ForgeError::Other(format!("cannot save {}: {e}", path.display())))
    })
    .await
}

/// "Open Project…" in the LightEdit window: open a folder as a project in the project window
/// (asking "This Window / New Window" there when a project is open). Opens the project window
/// first when Forge runs with the LightEdit window only.
#[tauri::command]
pub fn light_edit_open_project(path: String, app: AppHandle) -> Result<()> {
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(ForgeError::Other(format!("not a folder: {path}")));
    }
    crate::companion::open_project_later(&app, &dir.canonicalize().unwrap_or(dir), false);
    Ok(())
}
