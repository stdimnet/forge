//! Laravel IPC (`phpforge-fw-laravel`): project analysis, routes, completion, Go to
//! Declaration, gutter markers and artisan. Offsets crossing the boundary are UTF-16.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_core::{ForgeError, Result};
use phpforge_fw_laravel::artisan::{self, ArtisanCommand, ArtisanEvent};
use phpforge_fw_laravel::{Completions, GutterMark, Laravel, LaravelInfo, Location, Route};
use phpforge_lang_php::offsets::{byte_to_utf16, utf16_to_byte};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;

/// How often the project files are checked for changes at most.
const RECHECK: Duration = Duration::from_secs(2);

struct Cached {
    laravel: Laravel,
    checked: Instant,
}

/// The analysed project (rebuilt when its files change) and running artisan commands.
#[derive(Default)]
pub struct LaravelState {
    cache: Mutex<Option<Cached>>,
    runs: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

fn err(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

fn root(app: &AppState) -> Result<PathBuf> {
    app.with_workspace(|ws| Ok(ws.root().to_path_buf()))
}

fn php(app: &AppState) -> String {
    app.read_settings()
        .get_as::<String>("php.interpreter")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "php".into())
}

fn setting(app: &AppState, key: &str, default: &str) -> String {
    app.read_settings()
        .get_as::<String>(key)
        .unwrap_or_else(|| default.into())
}

/// Run `f` with the up-to-date analysis of the open project.
fn with_laravel<T>(
    app: &AppState,
    state: &LaravelState,
    f: impl FnOnce(&mut Laravel) -> T,
) -> Result<T> {
    let root = root(app)?;
    let mut guard = state.cache.lock().map_err(err)?;
    let stale = match guard.as_mut() {
        Some(c) if c.laravel.root == root => {
            if c.checked.elapsed() > RECHECK {
                c.checked = Instant::now();
                c.laravel.fingerprint != phpforge_fw_laravel::fingerprint(&root)
            } else {
                false
            }
        }
        _ => true,
    };
    if stale {
        *guard = Some(Cached {
            laravel: Laravel::load(&root),
            checked: Instant::now(),
        });
    }
    Ok(f(&mut guard.as_mut().expect("loaded above").laravel))
}

/// Current buffer content, or the file on disk.
fn source(app: &AppState, path: &str, content: Option<String>) -> Result<String> {
    match content {
        Some(c) => Ok(c),
        None => app.with_workspace(|ws| ws.read_file(path)),
    }
}

/// Whether the open project is a Laravel application, with a summary.
#[tauri::command]
pub async fn laravel_detect(
    state: State<'_, AppState>,
    laravel: State<'_, LaravelState>,
) -> Result<LaravelInfo> {
    let root = root(&state)?;
    if !phpforge_fw_laravel::is_laravel(&root) {
        return Ok(LaravelInfo {
            detected: false,
            version: None,
            routes: 0,
            views: 0,
            components: 0,
            models: 0,
            config_keys: 0,
            translations: 0,
            directives: Vec::new(),
        });
    }
    with_laravel(&state, &laravel, |l| l.info())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutesResult {
    routes: Vec<Route>,
    /// `static` or `artisan`.
    source: String,
}

/// All routes. With `laravel.routes.source` = `artisan` (or `auto` when artisan can run),
/// `php artisan route:list --json` is used and static parsing is the fallback.
#[tauri::command]
pub async fn laravel_routes(
    state: State<'_, AppState>,
    laravel: State<'_, LaravelState>,
) -> Result<RoutesResult> {
    let root = root(&state)?;
    let mode = setting(&state, "laravel.routes.source", "auto");
    let php = php(&state);
    let json = if mode == "artisan" || (mode == "auto" && artisan::is_runnable(&root)) {
        let r = root.clone();
        tauri::async_runtime::spawn_blocking(move || {
            artisan::capture(&r, &php, &["route:list", "--json"], Duration::from_secs(20))
        })
        .await
        .map_err(err)?
    } else {
        None
    };
    with_laravel(&state, &laravel, |l| {
        if let Some(json) = json {
            l.use_route_list_json(&json);
        }
        RoutesResult {
            routes: l.routes.clone(),
            source: l.routes_source.clone(),
        }
    })
}

/// Completion for `route()`, `view()`, `config()`, `env()`, `__()`, query columns,
/// relations, model properties, scopes, Blade components and custom directives.
#[tauri::command]
pub fn laravel_complete(
    path: String,
    offset: usize,
    content: Option<String>,
    state: State<'_, AppState>,
    laravel: State<'_, LaravelState>,
) -> Result<Option<Completions>> {
    let text = source(&state, &path, content)?;
    let byte = utf16_to_byte(&text, offset);
    let result = with_laravel(&state, &laravel, |l| l.complete(&path, &text, byte))?;
    Ok(result.map(|mut c| {
        c.from = byte_to_utf16(&text, c.from);
        c.to = byte_to_utf16(&text, c.to);
        c
    }))
}

/// Declarations of the Laravel symbol at `offset` (empty when there is none).
#[tauri::command]
pub fn laravel_goto(
    path: String,
    offset: usize,
    content: Option<String>,
    state: State<'_, AppState>,
    laravel: State<'_, LaravelState>,
) -> Result<Vec<Location>> {
    let text = source(&state, &path, content)?;
    let byte = utf16_to_byte(&text, offset);
    with_laravel(&state, &laravel, |l| l.goto(&path, &text, byte))
}

/// Gutter icons: routed controller methods, route definitions and `view()` calls.
#[tauri::command]
pub fn laravel_gutter(
    path: String,
    content: Option<String>,
    state: State<'_, AppState>,
    laravel: State<'_, LaravelState>,
) -> Result<Vec<GutterMark>> {
    let text = source(&state, &path, content)?;
    with_laravel(&state, &laravel, |l| l.gutter(&path, &text))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtisanCommands {
    commands: Vec<ArtisanCommand>,
    /// `false` when artisan could not run and the built-in list is shown.
    from_project: bool,
}

#[tauri::command]
pub async fn laravel_artisan_commands(state: State<'_, AppState>) -> Result<ArtisanCommands> {
    let root = root(&state)?;
    let php = php(&state);
    let (commands, from_project) =
        tauri::async_runtime::spawn_blocking(move || artisan::list_commands(&root, &php))
            .await
            .map_err(err)?;
    Ok(ArtisanCommands {
        commands,
        from_project,
    })
}

/// Run `php artisan <commandLine>`, streaming [`ArtisanEvent`]s on
/// `laravel://artisan/<runId>`. Returns the exit code.
#[tauri::command]
pub async fn laravel_artisan_run(
    command_line: String,
    run_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
    laravel: State<'_, LaravelState>,
) -> Result<i32> {
    let root = root(&state)?;
    let php = php(&state);
    let cancel = Arc::new(AtomicBool::new(false));
    laravel
        .runs
        .lock()
        .map_err(err)?
        .insert(run_id.clone(), cancel.clone());
    let event = format!("laravel://artisan/{run_id}");
    let code = tauri::async_runtime::spawn_blocking(move || {
        artisan::run(
            &root,
            &php,
            &command_line,
            &cancel,
            &mut |e: ArtisanEvent| {
                if let Err(err) = app.emit(&event, e) {
                    tracing::warn!("could not emit {event}: {err}");
                }
            },
        )
    })
    .await
    .map_err(err)?;
    laravel.runs.lock().map_err(err)?.remove(&run_id);
    Ok(code)
}

#[tauri::command]
pub fn laravel_artisan_cancel(run_id: String, laravel: State<'_, LaravelState>) -> Result<()> {
    if let Some(flag) = laravel.runs.lock().map_err(err)?.get(&run_id) {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}
