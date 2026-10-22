//! Run Anything, launch plans (compound configurations and "Before launch" tasks) and
//! External Tools. The logic is the pure `forge-runtools` crate (also in browser mode via
//! `forge-wasm`); these commands collect what it needs through the project backend, so
//! remote projects work the same, and keep the Run Anything history in Forge's state dir.

use forge_backend::{web_call, Backend, WebCall};
use forge_core::project_state;
use forge_core::{ForgeError, Result};
use forge_runtools::anything::{self, HistoryEntry, NpmScripts, SuggestRequest, SuggestResponse};
use forge_runtools::external::{self, ExternalTool, ToolCommand};
use forge_runtools::launch::{self, LaunchPlan, PlanContext};
use forge_runtools::macros::{self, MacroContext, MacroInfo};
use forge_runtools::Configuration;
use tauri::State;
use webforge_lang_web::NpmPackage;

use crate::commands::php_interpreters as interpreters;
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// PHP files offered for `php …`.
const PHP_FILE_LIMIT: usize = 30;
const HISTORY_DIR: &str = "run-anything";

fn package_manager_setting(state: &AppState) -> String {
    state
        .read_settings()
        .get_as("web.packageManager")
        .unwrap_or_else(|| "auto".into())
}

fn npm_packages(backend: &dyn Backend, package_manager: String) -> Vec<NpmPackage> {
    web_call(
        backend,
        WebCall::NpmPackages {
            refresh: false,
            package_manager,
        },
    )
    .unwrap_or_default()
}

/// Suggestions of the Run Anything popup. The UI sends the query, history, configurations
/// and plugin commands; npm and Composer scripts and PHP files come from the project.
#[tauri::command]
pub async fn run_anything_suggest(
    mut request: SuggestRequest,
    state: State<'_, AppState>,
) -> Result<SuggestResponse> {
    let backend = state.backend()?;
    let package_manager = package_manager_setting(&state);
    blocking(move || {
        if !request.query.trim_start().starts_with('?') {
            request.npm = npm_packages(backend.as_ref(), package_manager)
                .into_iter()
                .filter(|p| !p.scripts.is_empty())
                .map(|p| NpmScripts {
                    dir: p.dir,
                    package_manager: p.package_manager.manager.id().to_string(),
                    scripts: p.scripts.into_iter().map(|s| s.name).collect(),
                })
                .collect();
            request.composer_scripts = backend
                .read_optional("composer.json")
                .ok()
                .flatten()
                .map(|t| anything::composer_scripts(&t))
                .unwrap_or_default();
            if let Some(rest) = request.query.trim_start().strip_prefix("php") {
                let rest = rest.trim();
                request.php_files = backend
                    .find_files(rest, PHP_FILE_LIMIT * 3, false)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|m| m.path)
                    .filter(|p| p.ends_with(".php") && !p.ends_with(".blade.php"))
                    .take(PHP_FILE_LIMIT)
                    .collect();
            }
        }
        Ok(anything::suggest(&request))
    })
    .await
}

/// The temporary configuration that runs a typed command (`php x.php`, `npm run dev`, any
/// shell command); `cwd` is relative to the project.
#[tauri::command]
pub fn run_anything_resolve(command: String, cwd: Option<String>) -> Result<Configuration> {
    anything::resolve(&command, cwd.as_deref().unwrap_or("")).map_err(ForgeError::Other)
}

fn history_dir() -> Result<std::path::PathBuf> {
    project_state::default_list_dir(HISTORY_DIR)
        .ok_or_else(|| ForgeError::Other("no state directory".into()))
}

fn display_root(state: &AppState) -> Result<String> {
    state
        .project_info()
        .map(|i| i.display_root())
        .ok_or_else(|| ForgeError::Other("no project is open".into()))
}

/// What ran from Run Anything in this project, newest first.
#[tauri::command]
pub async fn run_anything_history(state: State<'_, AppState>) -> Result<Vec<HistoryEntry>> {
    let root = display_root(&state)?;
    blocking(move || Ok(project_state::load_list(&history_dir()?, &root))).await
}

/// Remember a run; returns the new history.
#[tauri::command]
pub async fn run_anything_history_add(
    entry: HistoryEntry,
    state: State<'_, AppState>,
) -> Result<Vec<HistoryEntry>> {
    let root = display_root(&state)?;
    blocking(move || {
        let dir = history_dir()?;
        let history = anything::push_history(&project_state::load_list(&dir, &root), entry);
        project_state::save_list(&dir, &root, &history)?;
        Ok(history)
    })
    .await
}

/// What starting `target` runs: a compound's members and every configuration's ordered
/// before-launch tasks. `configurations` are the saved and temporary ones the UI knows.
#[tauri::command]
pub async fn run_launch_plan(
    configurations: Vec<Configuration>,
    target: String,
    state: State<'_, AppState>,
) -> Result<LaunchPlan> {
    let backend = state.backend()?;
    let setting = package_manager_setting(&state);
    blocking(move || {
        let needs_npm = configurations
            .iter()
            .any(|c| c.before_launch.iter().any(|t| t.kind == "npm"));
        let package_manager = if needs_npm && matches!(setting.as_str(), "" | "auto") {
            npm_packages(backend.as_ref(), setting)
                .into_iter()
                .find(|p| p.dir.is_empty())
                .map(|p| p.package_manager.manager.id().to_string())
                .unwrap_or_default()
        } else {
            setting
        };
        launch::plan(&configurations, &target, &PlanContext { package_manager })
            .map_err(ForgeError::Other)
    })
    .await
}

/// The context with the project filled in: its directory on the host it runs on, the
/// project's PHP interpreter and, for local projects, the OS and home directory.
fn context(state: &AppState, ctx: MacroContext) -> Result<MacroContext> {
    let backend = state.backend()?;
    let info = backend.info();
    let root = info.root.to_string_lossy().into_owned();
    let mut ctx = ctx.with_project(&root, &info.name);
    if ctx.php_executable.is_none() {
        ctx.php_executable = Some(interpreters::local_php(state));
    }
    if info.remote.is_none() {
        ctx.os_name
            .get_or_insert_with(|| std::env::consts::OS.to_string());
        if ctx.user_home.is_none() {
            ctx.user_home = dirs::home_dir().map(|h| h.to_string_lossy().into_owned());
        }
    }
    Ok(ctx)
}

/// The macros of "Insert Macro…" with previews for the current editor and selection.
#[tauri::command]
pub fn external_tool_macros(
    context: MacroContext,
    state: State<'_, AppState>,
) -> Result<Vec<MacroInfo>> {
    Ok(macros::list(&self::context(&state, context)?))
}

/// The command line of an External Tool run (macros expanded, `$Prompt$` answers in
/// `context.prompts`).
#[tauri::command]
pub fn external_tool_command(
    tool: ExternalTool,
    context: MacroContext,
    state: State<'_, AppState>,
) -> Result<ToolCommand> {
    external::command(&tool, &self::context(&state, context)?).map_err(ForgeError::Other)
}

/// Problems of a tool in the Edit Tool dialog (name, program, output filters).
#[tauri::command]
pub fn external_tool_validate(tool: ExternalTool, others: Vec<ExternalTool>) -> Vec<String> {
    external::validate(&tool, &others)
}
