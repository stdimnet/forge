//! Laravel IPC (`phpforge-fw-laravel`): project analysis, routes, completion, Go to
//! Declaration, gutter markers and artisan. The analysis runs where the project's files are
//! (`forge_backend::frameworks`: here for local projects, in the agent for remote ones);
//! artisan runs through the project backend's process API. Offsets crossing the boundary
//! are UTF-16.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use forge_backend::frameworks::{artisan_command, ArtisanCommands, LaravelRoutes};
use forge_backend::process_io::run_lines;
use forge_backend::{framework_call, Backend, FrameworkCall};
use forge_core::{ForgeError, Result};
use phpforge_fw_laravel::artisan::{split_command_line, ArtisanEvent};
use phpforge_fw_laravel::{Completions, GutterMark, LaravelInfo, Location};
use serde::de::DeserializeOwned;
use tauri::{AppHandle, Emitter, State};

use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Running artisan commands (cancel flags by run id).
#[derive(Default)]
pub struct LaravelState {
    runs: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

fn err(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

/// The project's PHP for framework helpers (a local executable).
fn php(app: &AppState) -> String {
    crate::commands::php_interpreters::local_php(app)
}

fn setting(app: &AppState, key: &str, default: &str) -> String {
    app.read_settings()
        .get_as::<String>(key)
        .unwrap_or_else(|| default.into())
}

/// Ask the project's backend (off the async runtime: remote calls wait for the host).
async fn call<T: DeserializeOwned + Send + 'static>(
    state: &AppState,
    call: FrameworkCall,
) -> Result<T> {
    let backend = state.backend()?;
    blocking(move || framework_call(backend.as_ref(), call)).await
}

/// Whether the open project is a Laravel application, with a summary.
#[tauri::command]
pub async fn laravel_detect(state: State<'_, AppState>) -> Result<LaravelInfo> {
    call(&state, FrameworkCall::LaravelDetect).await
}

/// All routes. With `laravel.routes.source` = `artisan` (or `auto` when artisan can run),
/// `php artisan route:list --json` is used and static parsing is the fallback.
#[tauri::command]
pub async fn laravel_routes(state: State<'_, AppState>) -> Result<LaravelRoutes> {
    let source = setting(&state, "laravel.routes.source", "auto");
    let php = php(&state);
    call(&state, FrameworkCall::LaravelRoutes { source, php }).await
}

/// Completion for `route()`, `view()`, `config()`, `env()`, `__()`, query columns,
/// relations, model properties, scopes, Blade components and custom directives.
#[tauri::command]
pub async fn laravel_complete(
    path: String,
    offset: usize,
    content: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<Completions>> {
    call(
        &state,
        FrameworkCall::LaravelComplete {
            path,
            offset,
            content,
        },
    )
    .await
}

/// Declarations of the Laravel symbol at `offset` (empty when there is none).
#[tauri::command]
pub async fn laravel_goto(
    path: String,
    offset: usize,
    content: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<Location>> {
    call(
        &state,
        FrameworkCall::LaravelGoto {
            path,
            offset,
            content,
        },
    )
    .await
}

/// Gutter icons: routed controller methods, route definitions and `view()` calls.
#[tauri::command]
pub async fn laravel_gutter(
    path: String,
    content: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<GutterMark>> {
    call(&state, FrameworkCall::LaravelGutter { path, content }).await
}

#[tauri::command]
pub async fn laravel_artisan_commands(state: State<'_, AppState>) -> Result<ArtisanCommands> {
    let php = php(&state);
    call(&state, FrameworkCall::LaravelArtisanCommands { php }).await
}

fn run_artisan(
    backend: &dyn Backend,
    php: &str,
    command_line: &str,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(ArtisanEvent),
) -> i32 {
    let cmd = artisan_command(&backend.info().root, php, command_line);
    emit(ArtisanEvent::Started {
        command_line: format!("php artisan {}", split_command_line(command_line).join(" ")),
    });
    let code = run_lines(backend, &cmd, cancel, &mut |text, stderr| {
        emit(ArtisanEvent::Output { text, stderr })
    });
    let code = match code {
        Ok(code) => code.unwrap_or(-1),
        Err(e) => {
            emit(ArtisanEvent::Error {
                message: format!("Cannot run {php}: {e}"),
            });
            -1
        }
    };
    emit(ArtisanEvent::Exit { code });
    code
}

/// Run `php artisan <commandLine>` where the project lives, streaming [`ArtisanEvent`]s on
/// `laravel://artisan/<runId>`. Returns the exit code.
#[tauri::command]
pub async fn laravel_artisan_run(
    command_line: String,
    run_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
    laravel: State<'_, LaravelState>,
) -> Result<i32> {
    let backend = state.backend()?;
    let php = php(&state);
    let cancel = Arc::new(AtomicBool::new(false));
    laravel
        .runs
        .lock()
        .map_err(err)?
        .insert(run_id.clone(), cancel.clone());
    let event = format!("laravel://artisan/{run_id}");
    let code = blocking(move || {
        Ok(run_artisan(
            backend.as_ref(),
            &php,
            &command_line,
            &cancel,
            &mut |e: ArtisanEvent| {
                if let Err(err) = app.emit(&event, e) {
                    tracing::warn!("could not emit {event}: {err}");
                }
            },
        ))
    })
    .await?;
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
