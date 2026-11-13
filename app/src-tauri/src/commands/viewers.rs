//! Non-text file editors (`forge-viewers`): images for the image editor, read through the
//! project backend so remote projects work, and CSV/TSV for "Edit as Table".

use forge_core::{ForgeError, Result};
use forge_viewers::{CsvEdit, CsvTable, ImageData};
use tauri::State;

use super::workspace::blocking;
use crate::state::AppState;

/// An image file of the project: format, size and its bytes in base64.
#[tauri::command]
pub async fn image_read(path: String, state: State<'_, AppState>) -> Result<ImageData> {
    let backend = state.backend()?;
    blocking(move || {
        let bytes = backend.read_bytes(&path)?;
        forge_viewers::image_data(&path, &bytes).map_err(ForgeError::Other)
    })
    .await
}

/// Parse CSV/TSV text; without `delimiter` it is detected from `path` and the first line.
#[tauri::command]
pub fn csv_parse(text: String, path: Option<String>, delimiter: Option<char>) -> CsvTable {
    let delimiter = delimiter
        .unwrap_or_else(|| forge_viewers::detect_delimiter(path.as_deref().unwrap_or(""), &text));
    forge_viewers::parse_table(&text, delimiter)
}

/// Apply table edits to CSV/TSV text, keeping the quoting of untouched cells.
#[tauri::command]
pub fn csv_edit(text: String, delimiter: char, edits: Vec<CsvEdit>) -> String {
    forge_viewers::apply_edits(&text, delimiter, edits)
}

/// Display order of the rows sorted by a column (header rows stay first).
#[tauri::command]
pub fn csv_sort_order(
    rows: Vec<Vec<String>>,
    column: usize,
    descending: bool,
    header_rows: usize,
) -> Vec<usize> {
    forge_viewers::sort_order(&rows, column, descending, header_rows)
}
