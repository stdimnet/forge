//! Fuzzy matching, Search Everywhere ranking and file type detection. The logic lives in
//! the pure `forge-fuzzy` and `forge-filetypes` crates, which browser mode runs as
//! WebAssembly.

use forge_backend::{SearchStatus, SymbolHit};
use forge_core::Result;
use forge_filetypes::{DetectRequest, FileTypeInfo};
use forge_fuzzy::everywhere::{self, RankRequest, RankResponse};
use forge_fuzzy::{FilterHit, FilterRequest};
use forge_lsp::{LspManager, ServerState};
use tauri::{AppHandle, State};

use crate::commands::lsp::LspState;
use crate::commands::workspace::blocking;
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

/// Extensions of the languages a running language server answers symbol queries for.
fn served_extensions(manager: &LspManager) -> Vec<String> {
    let running: Vec<String> = manager
        .statuses()
        .into_iter()
        .filter(|s| matches!(s.state, ServerState::Ready | ServerState::Busy { .. }))
        .map(|s| s.id)
        .collect();
    manager
        .configs()
        .into_iter()
        .filter(|c| running.contains(&c.id))
        .flat_map(|c| c.file_extensions)
        .map(|e| {
            e.split(':')
                .next()
                .unwrap_or_default()
                .trim_start_matches('.')
                .to_ascii_lowercase()
        })
        .collect()
}

/// Classes (`types_only`) or all symbols from the project's search index, for the languages
/// no running language server covers. `skip_extensions` adds languages whose pack has an index
/// of its own (PHP, Lua).
#[tauri::command]
pub async fn search_find_symbols(
    query: String,
    types_only: bool,
    limit: usize,
    skip_extensions: Vec<String>,
    app: AppHandle,
    lsp: State<'_, LspState>,
    state: State<'_, AppState>,
) -> Result<Vec<SymbolHit>> {
    let backend = state.backend()?;
    let mut served = lsp
        .manager(&app, &state)
        .map(|m| served_extensions(&m))
        .unwrap_or_default();
    served.extend(skip_extensions);
    blocking(move || backend.find_symbols(&query, types_only, limit, &served)).await
}

/// Whether the project's search index is still being built.
#[tauri::command]
pub async fn search_status(state: State<'_, AppState>) -> Result<SearchStatus> {
    let backend = state.backend()?;
    blocking(move || backend.search_status()).await
}
