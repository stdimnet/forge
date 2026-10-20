//! Process output helpers for the UI (`forge_process::output`).

use forge_core::Result;
use forge_process::output::{file_links_utf16, FileLink};
use tauri::State;

use crate::state::AppState;

/// File links in a line of terminal output that resolve into the project; offsets are
/// JavaScript string indices.
#[tauri::command]
pub fn output_file_links(text: String, state: State<'_, AppState>) -> Result<Vec<FileLink>> {
    let root = state.project_root().ok().map(|r| r.display().to_string());
    Ok(file_links_utf16(&text, root.as_deref()))
}
