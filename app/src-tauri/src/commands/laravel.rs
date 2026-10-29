//! Laravel IPC (`phpforge-fw-laravel`): project analysis, routes, completion, Go to
//! Declaration, gutter markers, the Laravel tool window (overview, logs) and `ide.json`
//! code generations. The analysis runs where the project's files are
//! (`forge_backend::frameworks`: here for local projects, in the agent for remote ones).
//! Artisan commands run in the Command Line Tools Console (`command_line_tools`). Offsets
//! crossing the boundary are UTF-16.

use forge_backend::frameworks::LaravelRoutes;
use forge_backend::{framework_call, FrameworkCall};
use forge_core::Result;
use phpforge_fw_laravel::dashboard::LaravelOverview;
use phpforge_fw_laravel::ide_json::GeneratedFile;
use phpforge_fw_laravel::logs::{LogPage, DEFAULT_MAX_BYTES};
use phpforge_fw_laravel::{Completions, GutterMark, LaravelInfo, Location};
use serde::de::DeserializeOwned;
use tauri::State;

use crate::commands::workspace::blocking;
use crate::state::AppState;

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
    let mut source = setting(&state, "laravel.routes.source", "auto");
    // `php artisan route:list` boots the application: Safe Mode parses the route files.
    if state.safe_mode.is_on() {
        source = "static".into();
    }
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

/// Version, environment, models, commands, log files and `ide.json` of the application.
#[tauri::command]
pub async fn laravel_overview(state: State<'_, AppState>) -> Result<LaravelOverview> {
    call(&state, FrameworkCall::LaravelOverview).await
}

/// Entries of a log in `storage/logs` (empty `path`: the newest), read from its end.
#[tauri::command]
pub async fn laravel_logs(
    path: Option<String>,
    max_bytes: Option<u64>,
    state: State<'_, AppState>,
) -> Result<LogPage> {
    call(
        &state,
        FrameworkCall::LaravelLogs {
            path: path.unwrap_or_default(),
            max_bytes: max_bytes.unwrap_or(DEFAULT_MAX_BYTES),
        },
    )
    .await
}

/// Preview (`write: false`) or create the files of an `ide.json` code generation.
#[tauri::command]
pub async fn laravel_generate(
    id: String,
    input: String,
    write: bool,
    state: State<'_, AppState>,
) -> Result<Vec<GeneratedFile>> {
    call(&state, FrameworkCall::LaravelGenerate { id, input, write }).await
}
