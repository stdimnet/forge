//! Scopes and file colors (`forge-scopes`): the facts that need the project's files. The
//! scope language, the preview tree and the colors themselves are pure Rust that the UI
//! runs through WebAssembly; searches carry a `ScopeFilter` to the backend.

use forge_core::Result;
use tauri::State;

use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Most files the Scopes page's tree shows.
const MAX_FILES: usize = 50_000;

/// Every project file (`.gitignore` respected) for the Scopes page's preview, listed where
/// the project lives.
#[tauri::command]
pub async fn scopes_files(limit: Option<usize>, state: State<'_, AppState>) -> Result<Vec<String>> {
    let backend = state.backend()?;
    let limit = limit.unwrap_or(MAX_FILES).min(MAX_FILES);
    blocking(move || backend.list_files(limit)).await
}

/// Test folders of the project (`autoload-dev` of `composer.json`), for `test:` / `src:`
/// patterns and the Project Test Files scope; conventional test folders need no entry.
#[tauri::command]
pub async fn scopes_test_roots(state: State<'_, AppState>) -> Result<Vec<String>> {
    let backend = state.backend()?;
    blocking(move || {
        Ok(backend
            .read_optional("composer.json")?
            .map(|text| forge_scopes::composer_test_roots(&text))
            .unwrap_or_default())
    })
    .await
}
