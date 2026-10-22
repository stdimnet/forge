//! Structural Search and Replace (Edit | Find | Search Structurally…, Replace
//! Structurally…): templates matched on syntax trees by `forge-ssr`. Searches run through the
//! project's backend, so remote projects are parsed on their host; a replace writes through
//! the backend and can be undone like Replace in Files (`undo_replace`).

use forge_core::Result;
use forge_ssr::library::SavedTemplate;
use forge_ssr::{SsrRequest, SsrResult, TemplateCheck};
use tauri::State;

use crate::commands::workspace::blocking;
use crate::state::AppState;

/// The built-in templates (Existing Templates).
#[tauri::command]
pub fn ssr_templates() -> Vec<SavedTemplate> {
    forge_ssr::library::builtin()
}

/// The variables of a template and why it does not compile, while it is typed.
#[tauri::command]
pub fn ssr_check(request: SsrRequest) -> TemplateCheck {
    forge_ssr::check(&request)
}

/// The matches of a template in the scope, with the replacement of each when the request
/// has a replacement template.
#[tauri::command]
pub async fn ssr_search(request: SsrRequest, state: State<'_, AppState>) -> Result<SsrResult> {
    let backend = state.backend()?;
    blocking(move || backend.structural_search(&request)).await
}

/// Replace All: every match replaced; the changed files are kept for `undo_replace`.
#[tauri::command]
pub async fn ssr_replace(
    request: SsrRequest,
    state: State<'_, AppState>,
) -> Result<forge_backend::replace::ReplaceDone> {
    let backend = state.backend()?;
    let done = blocking(move || forge_backend::ssr::replace_all(&*backend, &request)).await?;
    *state.last_replace.lock().expect("last replace lock") = done.undo.clone();
    Ok(done)
}
