//! Drupal, Joomla and Shopware IPC: detection and Drupal hook implementations (completion,
//! gutter marks, Go to the hook docs). Logic lives in `phpforge-fw-cms`; the analysis runs
//! where the project's files are (`forge_backend::frameworks`). Drush and the Shopware
//! console run through the Command Line Tool Support (`cli_tools_run`), which refuses
//! them in Safe Mode. Offsets crossing the boundary are UTF-16 (JS strings).

use forge_backend::{framework_call, FrameworkCall};
use forge_core::Result;
use phpforge_fw_cms::api::Query;
use serde_json::Value;
use tauri::State;

use crate::commands::workspace::blocking;
use crate::state::AppState;

async fn query(state: &AppState, query: Query) -> Result<Value> {
    let backend = state.backend()?;
    blocking(move || framework_call(backend.as_ref(), FrameworkCall::Cms { query })).await
}

/// `CmsInfo[]`: the CMS the project uses.
#[tauri::command]
pub async fn cms_detect(state: State<'_, AppState>) -> Result<Value> {
    query(&state, Query::Detect).await
}

#[tauri::command]
pub async fn cms_refresh(state: State<'_, AppState>) -> Result<Value> {
    query(&state, Query::Refresh).await
}

/// Drupal hook implementations after `function`.
#[tauri::command]
pub async fn cms_complete(
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

/// The docs of the hook the function at the caret implements.
#[tauri::command]
pub async fn cms_goto(
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
pub async fn cms_gutter(
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<Value> {
    query(&state, Query::Gutter { path, content }).await
}
