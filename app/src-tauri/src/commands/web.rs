//! WebForge IPC: the npm tool window (`package.json` files, scripts, package managers),
//! run icons in `package.json`, and component navigation and line markers of the React,
//! Vue and Angular plugins. The analysis runs where the project's files are
//! (`forge_backend::web`: in this process for a local project, in the agent on the host for
//! a remote one); script runs go through the backend's process API like every run
//! configuration. Logic lives in `packs/webforge/*`; offsets are UTF-16.

use forge_backend::{web_call, WebCall};
use forge_core::Result;
use serde::de::DeserializeOwned;
use tauri::State;
use webforge_lang_web::{npm, Component, GutterMark, Location, NpmPackage};

use crate::commands::workspace::blocking;
use crate::state::AppState;

async fn call<T: DeserializeOwned + Send + 'static>(state: &AppState, call: WebCall) -> Result<T> {
    let backend = state.backend()?;
    blocking(move || web_call(backend.as_ref(), call)).await
}

/// Every `package.json` of the project with its scripts and package manager. `refresh`
/// reads the project again (Reload Scripts).
#[tauri::command]
pub async fn npm_packages(
    state: State<'_, AppState>,
    refresh: Option<bool>,
) -> Result<Vec<NpmPackage>> {
    let package_manager: String = state
        .read_settings()
        .get_as("web.packageManager")
        .unwrap_or_else(|| "auto".into());
    call(
        &state,
        WebCall::NpmPackages {
            refresh: refresh.unwrap_or(false),
            package_manager,
        },
    )
    .await
}

/// Run icons for the scripts of a `package.json` (its current editor text).
#[tauri::command]
pub fn npm_gutter(path: String, content: String) -> Vec<GutterMark> {
    npm::gutter(&path, &content)
}

/// Components (directives and pipes for Angular) with their props, events and usage counts.
#[tauri::command]
pub async fn web_components(
    state: State<'_, AppState>,
    framework: String,
) -> Result<Vec<Component>> {
    call(&state, WebCall::Components { framework }).await
}

/// Go to Declaration for component tags, props, events, selectors, pipes and
/// `templateUrl`/`styleUrls`; on a component's own name its usages.
#[tauri::command]
pub async fn web_goto(
    state: State<'_, AppState>,
    framework: String,
    path: String,
    content: String,
    offset: usize,
) -> Result<Vec<Location>> {
    call(
        &state,
        WebCall::Goto {
            framework,
            path,
            content,
            offset,
        },
    )
    .await
}

/// Line markers of a framework for a file.
#[tauri::command]
pub async fn web_gutter(
    state: State<'_, AppState>,
    framework: String,
    path: String,
    content: String,
) -> Result<Vec<GutterMark>> {
    call(
        &state,
        WebCall::Gutter {
            framework,
            path,
            content,
        },
    )
    .await
}
