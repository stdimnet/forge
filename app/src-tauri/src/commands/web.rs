//! WebForge IPC: the npm tool window (`package.json` files, scripts, package managers),
//! run icons in `package.json`, and component navigation and line markers of the React,
//! Vue and Angular plugins. Logic lives in `packs/webforge/*`; offsets are UTF-16.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_core::{ForgeError, Result};
use tauri::State;
use webforge_lang_web::{
    npm, Component, FrameworkIndex, GutterMark, Location, NpmPackage, Sources,
};

use crate::state::AppState;

/// How long loaded project files are reused before the project is read again.
const SOURCES_TTL: Duration = Duration::from_secs(5);
/// Script, template and manifest files read per project.
const FILE_LIMIT: usize = 8000;

/// Project files shared by the npm and framework commands.
#[derive(Default)]
pub struct WebState {
    sources: Mutex<Option<(PathBuf, Instant, Arc<Sources>)>>,
}

fn err(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

async fn sources(state: &AppState, web: &WebState, force: bool) -> Result<Arc<Sources>> {
    let root = state.with_workspace(|ws| Ok(ws.root().to_path_buf()))?;
    if !force {
        let guard = web.sources.lock().map_err(err)?;
        if let Some((r, at, s)) = guard.as_ref() {
            if *r == root && at.elapsed() < SOURCES_TTL {
                return Ok(s.clone());
            }
        }
    }
    let scan_root = root.clone();
    let loaded = tauri::async_runtime::spawn_blocking(move || {
        webforge_lang_web::fs::load(&scan_root, FILE_LIMIT)
    })
    .await
    .map_err(err)?;
    let loaded = Arc::new(loaded);
    *web.sources.lock().map_err(err)? = Some((root, Instant::now(), loaded.clone()));
    Ok(loaded)
}

/// Run `f` with the index of `framework` (`react`, `vue`, `angular`).
fn with_index<T>(
    sources: &Sources,
    framework: &str,
    f: impl FnOnce(&dyn FrameworkIndex) -> T,
) -> Result<T> {
    match framework {
        "react" => Ok(f(&webforge_fw_react::ReactIndex::new(sources))),
        "vue" => Ok(f(&webforge_fw_vue::VueIndex::new(sources))),
        "angular" => Ok(f(&webforge_fw_angular::AngularIndex::new(sources))),
        other => Err(ForgeError::Other(format!("unknown framework {other}"))),
    }
}

/// Every `package.json` of the project with its scripts and package manager. `refresh`
/// reads the project again (Reload Scripts).
#[tauri::command]
pub async fn npm_packages(
    state: State<'_, AppState>,
    web: State<'_, WebState>,
    refresh: Option<bool>,
) -> Result<Vec<NpmPackage>> {
    let sources = sources(&state, &web, refresh.unwrap_or(false)).await?;
    let setting: String = state
        .read_settings()
        .get_as("web.packageManager")
        .unwrap_or_else(|| "auto".into());
    Ok(npm::packages(&sources, &setting))
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
    web: State<'_, WebState>,
    framework: String,
) -> Result<Vec<Component>> {
    let sources = sources(&state, &web, false).await?;
    with_index(&sources, &framework, |i| i.components())
}

/// Go to Declaration for component tags, props, events, selectors, pipes and
/// `templateUrl`/`styleUrls`; on a component's own name its usages.
#[tauri::command]
pub async fn web_goto(
    state: State<'_, AppState>,
    web: State<'_, WebState>,
    framework: String,
    path: String,
    content: String,
    offset: usize,
) -> Result<Vec<Location>> {
    let sources = sources(&state, &web, false).await?;
    with_index(&sources, &framework, |i| i.goto(&path, &content, offset))
}

/// Line markers of a framework for a file.
#[tauri::command]
pub async fn web_gutter(
    state: State<'_, AppState>,
    web: State<'_, WebState>,
    framework: String,
    path: String,
    content: String,
) -> Result<Vec<GutterMark>> {
    let sources = sources(&state, &web, false).await?;
    with_index(&sources, &framework, |i| i.gutter(&path, &content))
}
