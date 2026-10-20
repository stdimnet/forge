//! Fuzzy matching, Search Everywhere ranking and file type detection. The logic lives in
//! the pure `forge-fuzzy` and `forge-filetypes` crates, which browser mode runs as
//! WebAssembly.

use forge_filetypes::{DetectRequest, FileTypeInfo};
use forge_fuzzy::everywhere::{self, RankRequest, RankResponse};
use forge_fuzzy::{FilterHit, FilterRequest};
use tauri::State;

use crate::state::AppState;

/// Filter and rank in-memory lists (Recent Files, popups) by fuzzy match.
#[tauri::command]
pub fn fuzzy_filter(request: FilterRequest) -> Vec<FilterHit> {
    forge_fuzzy::filter(&request)
}

/// Merge, score, dedupe and group the candidates of all Search Everywhere providers.
#[tauri::command]
pub fn search_rank(request: RankRequest) -> RankResponse {
    everywhere::rank(&request)
}

/// File type, label, indentation and data language of files (editor and status bar).
#[tauri::command]
pub fn filetypes_detect(request: DetectRequest, state: State<'_, AppState>) -> Vec<FileTypeInfo> {
    let plugins = state.plugins.read().expect("plugins lock poisoned");
    plugins.file_type_registry().resolve(&request)
}
