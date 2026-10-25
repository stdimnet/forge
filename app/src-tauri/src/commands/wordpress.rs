//! WordPress IPC: detection (site, plugin, theme; WP-CLI and stubs), the hook index, hook
//! completion and navigation, gutter marks, Find Usages of a hook and the Yoda condition
//! hints. Logic lives in `phpforge-fw-wordpress`; the analysis runs where the project's
//! files are (`forge_backend::frameworks`). WP-CLI commands run through the Command Line
//! Tool Support (`cli_tools_run`), which refuses them in Safe Mode.
//! Offsets crossing the boundary are UTF-16 (JS strings).

use forge_backend::{framework_call, FrameworkCall};
use forge_core::Result;
use phpforge_fw_wordpress::api::Query;
use serde_json::Value;
use tauri::State;

use crate::commands::workspace::blocking;
use crate::state::AppState;

async fn query(state: &AppState, query: Query) -> Result<Value> {
    let backend = state.backend()?;
    blocking(move || framework_call(backend.as_ref(), FrameworkCall::WordPress { query })).await
}

/// `WordPressInfo`, or `null` when the project does not use WordPress.
#[tauri::command]
pub async fn wordpress_detect(state: State<'_, AppState>) -> Result<Value> {
    query(&state, Query::Detect).await
}

/// Scan the project again.
#[tauri::command]
pub async fn wordpress_refresh(state: State<'_, AppState>) -> Result<Value> {
    query(&state, Query::Refresh).await
}

/// Every hook with its registration and invocation counts.
#[tauri::command]
pub async fn wordpress_hooks(state: State<'_, AppState>) -> Result<Value> {
    query(&state, Query::Hooks).await
}

/// Hook names inside `add_action('…')` and friends.
#[tauri::command]
pub async fn wordpress_complete(
    path: String,
    content: String,
    offset: usize,
    state: State<'_, AppState>,
) -> Result<Value> {
    query(
        &state,
        Query::Complete {
            path,
            content,
            offset,
        },
    )
    .await
}

/// Registrations ↔ invocations of the hook at the caret, a callback's function, or a
/// WordPress function in the stubs.
#[tauri::command]
pub async fn wordpress_goto(
    path: String,
    content: String,
    offset: usize,
    state: State<'_, AppState>,
) -> Result<Value> {
    query(
        &state,
        Query::Goto {
            path,
            content,
            offset,
        },
    )
    .await
}

#[tauri::command]
pub async fn wordpress_gutter(
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<Value> {
    query(&state, Query::Gutter { path, content }).await
}

/// Find Usages of the hook at the caret, in the shape of `code_find_usages`.
#[tauri::command]
pub async fn wordpress_usages(
    path: String,
    content: String,
    offset: usize,
    state: State<'_, AppState>,
) -> Result<Value> {
    query(
        &state,
        Query::Usages {
            path,
            content,
            offset,
        },
    )
    .await
}

/// "Use Yoda condition" hints with their quick-fixes.
#[tauri::command]
pub async fn wordpress_inspect(
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<Value> {
    query(&state, Query::Inspect { path, content }).await
}
