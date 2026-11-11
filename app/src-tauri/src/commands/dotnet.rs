//! DotNetForge (`packs/dotnetforge/*`): the .NET host and SDKs, the solution and its
//! projects, the Build and NuGet tool windows, ".NET Project" run configurations with
//! launch profiles, the .NET test provider's context, netcoredbg debugging, `dotnet new`
//! templates, Code Cleanup with `dotnet format`, and the Roslyn language server.
//!
//! Reading the solution, project files and launch profiles runs nothing. Every command
//! line that builds, runs, tests, restores, formats or installs is started by the UI
//! through `run_start` (hidden type `dotnet-command`), which refuses it in Safe Mode;
//! `dotnet_debug_request` and `dotnet_format` check trust themselves. The Roslyn server
//! loads MSBuild projects, so it is in `forge_trust::SERVERS_RUNNING_PROJECT_CODE`.
//!
//! Events: `dotnet://workspace` (the reloaded `DotnetWorkspace`), `dotnet://install`
//! (`{ id: "roslyn" | "netcoredbg", event }` while downloading).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use dotnetforge_lang_dotnet::command::{self as cmdline, DotnetCommand, FormatRequest, NewProject};
use dotnetforge_lang_dotnet::format::{
    editorconfig_candidates, is_root_editorconfig, parse_format_report, FormatChange,
};
use dotnetforge_lang_dotnet::nuget::SearchResult;
use dotnetforge_lang_dotnet::roslyn::{RoslynSettings, RoslynSetup, SERVER_ID as ROSLYN_ID};
use dotnetforge_lang_dotnet::templates::{common_templates, parse_template_list, Template};
use dotnetforge_lang_dotnet::{
    BuildReport, DotnetContext, DotnetWorkspace, CONTEXT_KEY, DOTNET_RUN, DOTNET_TEST,
};
use dotnetforge_sdk::detect::{system_runner, DetectEnv};
use dotnetforge_sdk::{DotnetStatus, NuGetClient, PackageUpdate, ToolLocation};
use forge_backend::process_io::capture;
use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_dap::{LaunchMode, LaunchRequest};
use forge_langservers::managed::{
    netcoredbg as managed_dbg, roslyn as managed_roslyn, Event, Http,
};
use forge_lsp::{LspManager, LspNotification, LspServerConfig};
use forge_process::{CommandLine, RunConfiguration};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::library::LibraryState;
use crate::commands::lsp::LspState;
use crate::commands::workspace::blocking;
use crate::state::AppState;

pub const PLUGIN: &str = "forge.dotnet";
pub const INSTALL_EVENT: &str = "dotnet://install";
pub const WORKSPACE_EVENT: &str = "dotnet://workspace";
const BUILD_TIMEOUT: Duration = Duration::from_secs(1800);
const PROBE_TIMEOUT: Duration = Duration::from_secs(60);
const FORMAT_TIMEOUT: Duration = Duration::from_secs(120);

// ---- Settings ------------------------------------------------------------------------------

pub(crate) fn setting(state: &AppState, key: &str) -> String {
    state
        .read_settings()
        .get_as::<String>(key)
        .map(|v| v.trim().to_string())
        .unwrap_or_default()
}

fn flag(state: &AppState, key: &str, default: bool) -> bool {
    state.read_settings().get_bool(key).unwrap_or(default)
}

fn environment(state: &AppState) -> BTreeMap<String, String> {
    state
        .read_settings()
        .get_as::<Vec<String>>("dotnet.environment")
        .unwrap_or_default()
        .iter()
        .filter_map(|e| e.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.to_string()))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

pub(crate) fn is_local(state: &AppState) -> bool {
    state
        .backend()
        .map(|b| b.info().remote.is_none())
        .unwrap_or(true)
}

fn enabled(state: &AppState) -> bool {
    state
        .plugins
        .read()
        .expect("plugins lock poisoned")
        .is_enabled(PLUGIN)
}

pub(crate) fn other(e: impl ToString) -> ForgeError {
    ForgeError::Other(e.to_string())
}

pub(crate) fn path_string(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

// ---- SDKs and tools --------------------------------------------------------------------------

/// Detected status per project root, looked up again after a minute.
static STATUS: Mutex<Option<(PathBuf, String, Instant, DotnetStatus)>> = Mutex::new(None);

/// The .NET host and SDKs (local projects; remote ones use `dotnet` on the remote PATH).
pub(crate) fn status(state: &AppState, refresh: bool) -> DotnetStatus {
    let Ok(root) = state.project_root() else {
        return DotnetStatus::default();
    };
    if !is_local(state) {
        return DotnetStatus::default();
    }
    let custom = setting(state, "dotnet.path");
    {
        let cached = STATUS.lock().expect("dotnet status");
        if let Some((r, c, at, s)) = cached.as_ref() {
            if !refresh && *r == root && *c == custom && at.elapsed() < Duration::from_secs(60) {
                return s.clone();
            }
        }
    }
    let global = cached_workspace(state).and_then(|w| w.global_json);
    let s = dotnetforge_sdk::detect(
        &DetectEnv::system(),
        &custom,
        global.as_ref(),
        std::env::consts::OS,
        &system_runner,
    );
    *STATUS.lock().expect("dotnet status") = Some((root, custom, Instant::now(), s.clone()));
    s
}

/// The `dotnet` program of commands.
fn dotnet_program(state: &AppState) -> String {
    if is_local(state) {
        if let Some(h) = status(state, false).host {
            return path_string(&h.path);
        }
    }
    match setting(state, "dotnet.path") {
        p if p.is_empty() => "dotnet".into(),
        p => p,
    }
}

fn netcoredbg(state: &AppState) -> std::result::Result<ToolLocation, String> {
    dotnetforge_sdk::find_netcoredbg(
        &setting(state, "dotnet.debugger.path"),
        managed_dbg::netcoredbg_dir().as_deref(),
        &forge_lsp::find_on_path,
    )
}

fn roslyn(state: &AppState) -> std::result::Result<ToolLocation, String> {
    dotnetforge_sdk::find_roslyn(
        &setting(state, "dotnet.roslyn.path"),
        managed_roslyn::roslyn_dir().as_deref(),
    )
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DotnetStatusPayload {
    #[serde(flatten)]
    pub status: DotnetStatus,
    pub roslyn: Option<ToolLocation>,
    pub roslyn_problem: Option<String>,
    /// The pinned Roslyn server Forge downloads, and its size in bytes.
    pub roslyn_version: String,
    pub roslyn_size: Option<u64>,
    pub netcoredbg: Option<ToolLocation>,
    pub netcoredbg_problem: Option<String>,
    pub netcoredbg_version: String,
    pub remote: bool,
}

/// Settings | .NET and the status bar: the host, SDKs, runtimes, the project's SDK, the
/// Roslyn server and netcoredbg.
#[tauri::command]
pub async fn dotnet_status(refresh: Option<bool>, app: AppHandle) -> Result<DotnetStatusPayload> {
    blocking(move || {
        let state = app.state::<AppState>();
        let s = status(&state, refresh.unwrap_or(false));
        let (r, d) = (roslyn(&state), netcoredbg(&state));
        Ok(DotnetStatusPayload {
            status: s,
            roslyn_problem: r.as_ref().err().cloned(),
            roslyn: r.ok(),
            roslyn_version: managed_roslyn::ROSLYN_VERSION.into(),
            roslyn_size: managed_roslyn::current_roslyn_release().map(|d| d.size),
            netcoredbg_problem: d.as_ref().err().cloned(),
            netcoredbg: d.ok(),
            netcoredbg_version: managed_dbg::NETCOREDBG_VERSION.into(),
            remote: !is_local(&state),
        })
    })
    .await
}

fn emitter(app: &AppHandle, id: &'static str) -> impl FnMut(Event) + Send {
    let app = app.clone();
    move |event: Event| {
        let _ = app.emit(INSTALL_EVENT, json!({ "id": id, "event": event }));
    }
}

/// Download the pinned Roslyn language server (Microsoft's vs-impl feed, SHA-256), then
/// start it. Returns its version.
#[tauri::command]
pub async fn dotnet_roslyn_install(app: AppHandle) -> Result<String> {
    let base = managed_roslyn::roslyn_dir()
        .ok_or_else(|| other("no user data folder for the Roslyn server"))?;
    let d = managed_roslyn::current_roslyn_release()
        .ok_or_else(|| other("The Roslyn language server has no build for this platform"))?;
    let http = Http::new().map_err(ForgeError::Other)?;
    let mut on = emitter(&app, "roslyn");
    let inst = managed_roslyn::install_roslyn(&http, &base, &d, &mut on)
        .await
        .map_err(ForgeError::Other)?;
    let h = app.clone();
    tauri::async_runtime::spawn_blocking(move || sync_lsp_server(&h, None))
        .await
        .map_err(other)?;
    Ok(inst.version)
}

/// Download the pinned netcoredbg (GitHub release, SHA-256). Returns its version.
#[tauri::command]
pub async fn dotnet_netcoredbg_install(app: AppHandle) -> Result<String> {
    let base =
        managed_dbg::netcoredbg_dir().ok_or_else(|| other("no user data folder for netcoredbg"))?;
    let d = managed_dbg::current_netcoredbg_release()
        .ok_or_else(|| other("netcoredbg has no build for this platform"))?;
    let http = Http::new().map_err(ForgeError::Other)?;
    let mut on = emitter(&app, "netcoredbg");
    let inst = managed_dbg::install_netcoredbg(&http, &base, &d, &mut on)
        .await
        .map_err(ForgeError::Other)?;
    Ok(inst.version)
}

// ---- Solution and projects ---------------------------------------------------------------------

static WORKSPACE: Mutex<Option<(PathBuf, DotnetWorkspace)>> = Mutex::new(None);

const PROJECT_EXTENSIONS: &[&str] = &["sln", "slnx", "csproj", "fsproj", "vbproj"];

fn load_workspace(state: &AppState) -> Result<DotnetWorkspace> {
    let backend = state.backend()?;
    let root = backend.info().root.clone();
    let req = forge_core::search::CollectRequest {
        extensions: PROJECT_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
        max_files: 5_000,
        max_file_size: 8 << 20,
        max_total: 128 << 20,
    };
    let collected = backend.collect_files(&req)?;
    let texts: BTreeMap<String, String> = collected.into_iter().map(|f| (f.path, f.text)).collect();
    let files: Vec<String> = texts.keys().cloned().collect();
    let read = |p: &str| -> Option<String> {
        texts
            .get(p)
            .cloned()
            .or_else(|| backend.read_optional(p).ok().flatten())
    };
    let ws = dotnetforge_lang_dotnet::load_workspace(
        &files,
        &read,
        &setting(state, "dotnet.solution"),
        &setting(state, "dotnet.configuration"),
    );
    *WORKSPACE.lock().expect("dotnet workspace") = Some((root, ws.clone()));
    Ok(ws)
}

/// The last loaded workspace of the open project (never reads files).
pub(crate) fn cached_workspace(state: &AppState) -> Option<DotnetWorkspace> {
    let root = state.project_root().ok()?;
    WORKSPACE
        .lock()
        .expect("dotnet workspace")
        .as_ref()
        .filter(|(r, _)| *r == root)
        .map(|(_, w)| w.clone())
}

pub(crate) fn workspace(state: &AppState) -> Result<DotnetWorkspace> {
    match cached_workspace(state) {
        Some(w) => Ok(w),
        None => load_workspace(state),
    }
}

/// The solution, its projects (frameworks, references, packages, launch profiles) and
/// `global.json`, read from the files (`refresh`: read them again).
#[tauri::command]
pub async fn dotnet_workspace(refresh: Option<bool>, app: AppHandle) -> Result<DotnetWorkspace> {
    let a = app.clone();
    let ws = blocking(move || {
        let state = a.state::<AppState>();
        if refresh.unwrap_or(false) {
            load_workspace(&state)
        } else {
            workspace(&state)
        }
    })
    .await?;
    let _ = app.emit(WORKSPACE_EVENT, &ws);
    Ok(ws)
}

// ---- Context, runs and tests -------------------------------------------------------------------

pub(crate) fn base_context(state: &AppState) -> DotnetContext {
    DotnetContext {
        dotnet: dotnet_program(state),
        configuration: setting(state, "dotnet.configuration"),
        verbosity: setting(state, "dotnet.build.verbosity"),
        env: environment(state),
        windows: cfg!(windows) && is_local(state),
        ..DotnetContext::default()
    }
}

/// The context with the workspace (read if needed).
pub(crate) fn context(state: &AppState) -> DotnetContext {
    let ctx = base_context(state);
    match workspace(state) {
        Ok(ws) => ctx.with_workspace(&ws),
        Err(_) => ctx,
    }
}

/// Test classes of the test projects (`Ns.Class` → file), for the results' locations.
fn test_classes(backend: &dyn Backend, ws: &DotnetWorkspace) -> BTreeMap<String, String> {
    let dirs: Vec<String> = ws.test_projects().map(|p| p.dir()).collect();
    if dirs.is_empty() {
        return BTreeMap::new();
    }
    let req = forge_core::search::CollectRequest {
        extensions: vec!["cs".into(), "fs".into()],
        max_files: 20_000,
        max_file_size: 2 << 20,
        max_total: 256 << 20,
    };
    let mut out = BTreeMap::new();
    for f in backend.collect_files(&req).unwrap_or_default() {
        if !dirs
            .iter()
            .any(|d| d.is_empty() || f.path.starts_with(&format!("{d}/")))
        {
            continue;
        }
        let hint = ws.project_of_file(&f.path).and_then(|p| p.test_framework);
        let scan = dotnetforge_lang_dotnet::scan::scan_source(&f.path, &f.text, hint);
        for c in scan.declared {
            out.entry(c).or_insert_with(|| f.path.clone());
        }
        for c in scan.classes {
            out.entry(c.full_name).or_insert_with(|| f.path.clone());
        }
    }
    out
}

/// Hand the .NET context to the test provider of a test run (`run.rs`).
pub(crate) fn extend_test_context(
    state: &AppState,
    type_id: &str,
    ctx: &mut forge_testing::CommandContext,
) {
    if type_id != DOTNET_TEST {
        return;
    }
    let mut dotnet = context(state);
    if let (Ok(backend), Some(ws)) = (state.backend(), cached_workspace(state)) {
        dotnet.classes = test_classes(backend.as_ref(), &ws);
    }
    if let Ok(json) = serde_json::to_string(&dotnet) {
        ctx.settings.insert(CONTEXT_KEY.to_string(), json);
    }
}

/// The tests of a C# / F# file with its project (gutter ▶).
pub(crate) fn discover_file(state: &AppState, path: &str, text: &str) -> forge_testing::TestFile {
    let ctx = cached_workspace(state).map(|ws| base_context(state).with_workspace(&ws));
    dotnetforge_lang_dotnet::discover_file(path, text, ctx.as_ref())
}

pub(crate) fn to_command_line(c: DotnetCommand, root: &Path) -> CommandLine {
    let mut cmd = CommandLine::new(c.program).args(c.args);
    cmd.cwd = if c.cwd.is_empty() {
        root.to_path_buf()
    } else if Path::new(&c.cwd).is_absolute() {
        PathBuf::from(&c.cwd)
    } else {
        root.join(&c.cwd)
    };
    cmd.env = c.env;
    cmd
}

pub(crate) fn to_runtools(cfg: &RunConfiguration) -> forge_runtools::Configuration {
    forge_runtools::Configuration {
        name: cfg.name.clone(),
        type_id: cfg.type_id.clone(),
        working_directory: cfg.working_directory.clone(),
        interpreter_options: cfg.interpreter_options.clone(),
        arguments: cfg.arguments.clone(),
        options: cfg.options.clone(),
        before_launch: Vec::new(),
    }
}

/// The command line of a ".NET Project" configuration (`run.rs`).
pub(crate) fn run_command_line(state: &AppState, cfg: &RunConfiguration) -> Result<CommandLine> {
    let ctx = context(state);
    let built = cmdline::run_command(&to_runtools(cfg), &ctx).map_err(ForgeError::Other)?;
    let mut cmd = to_command_line(built, &state.project_root()?);
    cmd.env.extend(cfg.env.clone());
    Ok(cmd)
}

/// A build action (`build`, `rebuild`, `clean`, `restore`, `test`, `pack`, `publish`,
/// `watch`) of the solution or a project (started by the UI with `run_start`).
#[tauri::command]
pub async fn dotnet_command(
    action: String,
    target: Option<String>,
    app: AppHandle,
) -> Result<CommandLine> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ctx = context(&state);
        let c =
            cmdline::action_command(&action, target.as_deref(), &ctx).map_err(ForgeError::Other)?;
        Ok(to_command_line(c, &state.project_root()?))
    })
    .await
}

/// The problems, outputs and summary of a build's output (Build tool window, editor).
#[tauri::command]
pub fn dotnet_build_problems(
    output: String,
    state: State<'_, AppState>,
    lib: State<'_, LibraryState>,
) -> Result<BuildReport> {
    let root = path_string(&state.project_root()?);
    let mut report = dotnetforge_lang_dotnet::parse_build_output(&output, &root);
    let remote = state.is_remote();
    for p in &mut report.problems {
        p.library = lib.outside_file(&p.path, remote).unwrap_or_default();
    }
    Ok(report)
}

// ---- Templates and new projects ------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatesPayload {
    pub templates: Vec<Template>,
    /// Read from `dotnet new list` (else Forge's list of common templates).
    pub from_sdk: bool,
    pub error: Option<String>,
}

/// The installed templates (`dotnet new list`, run outside the project).
#[tauri::command]
pub async fn dotnet_templates(app: AppHandle) -> Result<TemplatesPayload> {
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let ctx = base_context(&state);
        let mut cmd = to_command_line(cmdline::template_list_command(&ctx), &backend.info().root);
        if is_local(&state) {
            cmd.cwd = std::env::temp_dir();
        }
        let fallback = |error: String| TemplatesPayload {
            templates: common_templates(),
            from_sdk: false,
            error: Some(error),
        };
        Ok(match capture(backend.as_ref(), &cmd, "", PROBE_TIMEOUT) {
            Ok(out) if out.code == Some(0) => {
                let t = parse_template_list(&out.stdout);
                if t.is_empty() {
                    fallback("dotnet new list printed no templates".into())
                } else {
                    TemplatesPayload {
                        templates: t,
                        from_sdk: true,
                        error: None,
                    }
                }
            }
            Ok(out) => fallback(
                out.stderr
                    .trim()
                    .lines()
                    .last()
                    .unwrap_or("dotnet new list failed")
                    .to_string(),
            ),
            Err(e) => fallback(e.to_string()),
        })
    })
    .await
}

/// `dotnet new` (and `dotnet sln add`) for "Add Project from Template…"; the UI runs them
/// one after the other with `run_start`.
#[tauri::command]
pub async fn dotnet_new_project(request: NewProject, app: AppHandle) -> Result<Vec<CommandLine>> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ctx = context(&state);
        let root = state.project_root()?;
        Ok(cmdline::new_project_commands(&request, &ctx)
            .map_err(ForgeError::Other)?
            .into_iter()
            .map(|c| to_command_line(c, &root))
            .collect())
    })
    .await
}

// ---- NuGet -------------------------------------------------------------------------------------

static NUGET: Mutex<Option<(String, NuGetClient)>> = Mutex::new(None);

fn nuget(state: &AppState) -> Result<NuGetClient> {
    let source = match setting(state, "dotnet.nuget.source") {
        s if s.is_empty() => dotnetforge_lang_dotnet::nuget::NUGET_ORG.to_string(),
        s => s,
    };
    let mut guard = NUGET.lock().expect("nuget client");
    if let Some((s, c)) = guard.as_ref() {
        if *s == source {
            return Ok(c.clone());
        }
    }
    let c = NuGetClient::new(&source).map_err(ForgeError::Other)?;
    *guard = Some((source, c.clone()));
    Ok(c)
}

fn prerelease(state: &AppState, asked: Option<bool>) -> bool {
    asked.unwrap_or_else(|| flag(state, "dotnet.nuget.prerelease", false))
}

/// Search the NuGet feed.
#[tauri::command]
pub async fn nuget_search(
    query: String,
    prerelease: Option<bool>,
    skip: Option<u32>,
    take: Option<u32>,
    state: State<'_, AppState>,
) -> Result<SearchResult> {
    let client = nuget(&state)?;
    let pre = self::prerelease(&state, prerelease);
    client
        .search(&query, skip.unwrap_or(0), take.unwrap_or(25), pre)
        .await
        .map_err(ForgeError::Other)
}

/// The versions of a package (newest last).
#[tauri::command]
pub async fn nuget_versions(id: String, state: State<'_, AppState>) -> Result<Vec<String>> {
    let client = nuget(&state)?;
    client.versions(&id).await.map_err(ForgeError::Other)
}

/// Newer versions of the solution's package references.
#[tauri::command]
pub async fn nuget_updates(prerelease: Option<bool>, app: AppHandle) -> Result<Vec<PackageUpdate>> {
    let a = app.clone();
    let ws = blocking(move || workspace(&a.state::<AppState>())).await?;
    let state = app.state::<AppState>();
    let pre = self::prerelease(&state, prerelease);
    let mut installed: BTreeSet<(String, String)> = BTreeSet::new();
    for p in &ws.projects {
        for r in &p.packages {
            if !r.version.is_empty() {
                installed.insert((r.id.clone(), r.version.clone()));
            }
        }
    }
    let client = nuget(&state)?;
    let list: Vec<(String, String)> = installed.into_iter().collect();
    Ok(client.updates(&list, pre).await)
}

/// `dotnet add|remove <project> package <id> [--version v]` (started by the UI with
/// `run_start`, so Safe Mode refuses it: restores run MSBuild).
#[tauri::command]
pub async fn nuget_command(
    action: String,
    project: String,
    package: String,
    version: Option<String>,
    prerelease: Option<bool>,
    app: AppHandle,
) -> Result<CommandLine> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ctx = base_context(&state);
        let c = cmdline::package_command(
            &action,
            &project,
            &package,
            version.as_deref(),
            prerelease.unwrap_or(false),
            &ctx,
        )
        .map_err(ForgeError::Other)?;
        Ok(to_command_line(c, &state.project_root()?))
    })
    .await
}

// ---- dotnet format -------------------------------------------------------------------------------

static FORMAT_SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn temp_dir(kind: &str) -> PathBuf {
    let n = FORMAT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("forge-dotnet-{kind}-{}-{n}", std::process::id()))
}

/// Reformat a C# / F# editor text with `dotnet format whitespace` (`dotnet.formatter =
/// dotnetFormat`): the text and the project's `.editorconfig` files along its path are
/// copied into a temporary folder, so the project is not touched and the same rules
/// apply. `None` when formatting is left to the language server or off.
#[tauri::command]
pub async fn dotnet_format(
    path: String,
    content: String,
    app: AppHandle,
) -> Result<Option<String>> {
    blocking(move || {
        let state = app.state::<AppState>();
        if setting(&state, "dotnet.formatter") != "dotnetFormat" {
            return Ok(None);
        }
        if !is_local(&state) {
            return Err(other("dotnet format runs for local projects only"));
        }
        let backend = state.backend()?;
        let program = dotnet_program(&state);
        if state.safe_mode.is_on()
            && forge_trust::runs_project_program(&backend.info().root, &program, &[])
        {
            return Err(other(forge_trust::refusal("Running the project's dotnet")));
        }
        let rel = path.trim_start_matches('/').replace('\\', "/");
        let dir = temp_dir("format");
        let _ = std::fs::remove_dir_all(&dir);
        let write = |p: &Path, text: &str| -> Result<()> {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).map_err(|e| ForgeError::io(parent, e))?;
            }
            std::fs::write(p, text).map_err(|e| ForgeError::io(p, e))
        };
        // The .editorconfig files from the file's folder upwards to the first `root = true`.
        let mut configs: Vec<(String, String)> = Vec::new();
        for c in editorconfig_candidates(&rel).into_iter().rev() {
            if let Ok(Some(t)) = backend.read_optional(&c) {
                let root = is_root_editorconfig(&t);
                configs.push((c, t));
                if root {
                    break;
                }
            }
        }
        let result = (|| -> Result<Option<String>> {
            for (c, t) in &configs {
                write(&dir.join(c), t)?;
            }
            let file = dir.join(&rel);
            write(&file, &content)?;
            let ctx = base_context(&state);
            let c = cmdline::format_command(
                &FormatRequest {
                    scope: "whitespace".into(),
                    target: path_string(&dir),
                    include: vec![rel.clone()],
                    folder: true,
                    ..FormatRequest::default()
                },
                &ctx,
            )
            .map_err(ForgeError::Other)?;
            let cmd = to_command_line(c, &dir);
            let out = capture(backend.as_ref(), &cmd, "", FORMAT_TIMEOUT)?;
            if out.code != Some(0) {
                let err = out.stderr.trim();
                return Err(other(if err.is_empty() {
                    out.stdout.trim()
                } else {
                    err
                }));
            }
            Ok(Some(
                std::fs::read_to_string(&file).map_err(|e| ForgeError::io(&file, e))?,
            ))
        })();
        let _ = std::fs::remove_dir_all(&dir);
        result
    })
    .await
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupCommand {
    pub command: CommandLine,
    /// Where `format-report.json` goes (`dotnet_format_report` reads it).
    pub report_dir: String,
}

/// Code Cleanup: `dotnet format` on files or folders (all of the solution without
/// `paths`) with the rules of `.editorconfig` (started by the UI with `run_start`).
#[tauri::command]
pub async fn dotnet_cleanup_command(
    paths: Option<Vec<String>>,
    app: AppHandle,
) -> Result<CleanupCommand> {
    blocking(move || {
        let state = app.state::<AppState>();
        let ctx = context(&state);
        let report = if is_local(&state) {
            temp_dir("cleanup")
        } else {
            PathBuf::new()
        };
        let scope = match setting(&state, "dotnet.cleanup.scope") {
            s if s.is_empty() => "all".to_string(),
            s => s,
        };
        let req = FormatRequest {
            severity: if scope == "whitespace" {
                String::new()
            } else {
                setting(&state, "dotnet.cleanup.severity")
            },
            scope,
            include: paths.unwrap_or_default(),
            report: if report.as_os_str().is_empty() {
                String::new()
            } else {
                path_string(&report)
            },
            ..FormatRequest::default()
        };
        let c = cmdline::format_command(&req, &ctx).map_err(ForgeError::Other)?;
        Ok(CleanupCommand {
            command: to_command_line(c, &state.project_root()?),
            report_dir: path_string(&report),
        })
    })
    .await
}

/// The fixes of a Code Cleanup run (its `format-report.json`); the folder is removed.
#[tauri::command]
pub fn dotnet_format_report(
    report_dir: String,
    state: State<'_, AppState>,
) -> Result<Vec<FormatChange>> {
    let dir = PathBuf::from(&report_dir);
    if report_dir.is_empty() || !dir.starts_with(std::env::temp_dir()) {
        return Ok(Vec::new());
    }
    let file = dir.join("format-report.json");
    let text = std::fs::read_to_string(&file).unwrap_or_else(|_| "[]".into());
    let _ = std::fs::remove_dir_all(&dir);
    parse_format_report(&text, &path_string(&state.project_root()?)).map_err(ForgeError::Other)
}

// ---- Debugging ---------------------------------------------------------------------------------

pub(crate) fn adapter_command(state: &AppState) -> Result<String> {
    netcoredbg(state)
        .map(|t| path_string(&t.path))
        .map_err(|e| {
            other(format!(
                "{e}. Download it in Settings | Languages & Frameworks | .NET or set its path."
            ))
        })
}

/// Build a ".NET Project" configuration and return the launch request of its assembly
/// under netcoredbg (`dotnet <assembly>`), with the launch profile's arguments, working
/// folder and environment.
#[tauri::command]
pub async fn dotnet_debug_request(
    configuration: RunConfiguration,
    app: AppHandle,
) -> Result<LaunchRequest> {
    blocking(move || {
        let state = app.state::<AppState>();
        state.require_trusted("Building the program to debug")?;
        if configuration.type_id != DOTNET_RUN {
            return Err(other(format!(
                "'{}' cannot be debugged as a .NET project",
                configuration.name
            )));
        }
        let adapter = adapter_command(&state)?;
        let ws = load_workspace(&state)?;
        let ctx = base_context(&state).with_workspace(&ws);
        let cfg = to_runtools(&configuration);
        let backend = state.backend()?;
        let root = backend.info().root.clone();
        let build = cmdline::debug_build_command(&cfg, &ctx).map_err(ForgeError::Other)?;
        let project_path = build.args.get(1).cloned().unwrap_or_default();
        let project = ws
            .project(&project_path)
            .cloned()
            .ok_or_else(|| other(format!("{project_path} is not a project of the solution")))?;
        let out = capture(
            backend.as_ref(),
            &to_command_line(build, &root),
            "",
            BUILD_TIMEOUT,
        )?;
        let report = dotnetforge_lang_dotnet::parse_build_output(
            &format!("{}\n{}", out.stdout, out.stderr),
            &path_string(&root),
        );
        if out.code != Some(0) || report.succeeded == Some(false) {
            let first = report
                .problems
                .iter()
                .find(|p| p.severity == dotnetforge_lang_dotnet::Severity::Error)
                .map(|p| format!("{} {}: {}", p.path, p.code, p.message))
                .unwrap_or_else(|| "see the build output".into());
            return Err(other(format!("The build failed: {first}")));
        }
        let dll = dotnetforge_lang_dotnet::build::output_of(&report, &project.name)
            .map(|o| o.path.clone())
            .unwrap_or_else(|| project.output_dll(&ctx.configuration(), cfg.option("framework")));
        let dll = if Path::new(&dll).is_absolute() {
            dll
        } else {
            path_string(&root.join(&dll))
        };
        let profile_name = cfg.option("profile").trim().to_string();
        let profile = if profile_name == "none" {
            None
        } else if profile_name.is_empty() {
            project.launch_profiles.iter().find(|p| p.is_runnable())
        } else {
            project
                .launch_profiles
                .iter()
                .find(|p| p.name == profile_name)
        };
        let mut args = vec![dll];
        if let Some(p) = profile {
            args.extend(forge_process::split_args(&p.command_line_args));
        }
        args.extend(forge_process::split_args(&configuration.arguments));
        let project_dir = root.join(project.dir());
        let cwd = match (
            configuration.working_directory.trim(),
            profile.map(|p| p.working_directory.as_str()),
        ) {
            (w, _) if !w.is_empty() => {
                if Path::new(w).is_absolute() {
                    PathBuf::from(w)
                } else {
                    root.join(w)
                }
            }
            (_, Some(w)) if !w.is_empty() => project_dir.join(w.replace("$(ProjectDir)", "")),
            _ => project_dir,
        };
        let mut env = ctx.env.clone();
        if let Some(p) = profile {
            env.extend(p.environment());
        }
        env.extend(configuration.env.clone());
        let mut r = LaunchRequest::launch("coreclr", &ctx.dotnet());
        r.name = Some(configuration.name.clone()).filter(|n| !n.is_empty());
        r.args = args;
        r.cwd = Some(path_string(&cwd));
        r.env = env;
        r.configuration = json!({ "justMyCode": flag(&state, "dotnet.debugger.justMyCode", true) });
        r.adapter_command = Some(adapter);
        Ok(r)
    })
    .await
}

/// Attach netcoredbg to a test host that waits for a debugger (`VSTEST_HOST_DEBUG=1`).
#[tauri::command]
pub async fn dotnet_attach_request(
    pid: u32,
    name: Option<String>,
    app: AppHandle,
) -> Result<LaunchRequest> {
    blocking(move || {
        let state = app.state::<AppState>();
        state.require_trusted("Debugging tests")?;
        let adapter = adapter_command(&state)?;
        let mut r = LaunchRequest::launch("coreclr", "");
        r.mode = LaunchMode::AttachProcess { pid };
        r.program = None;
        r.name = name;
        r.configuration = json!({ "justMyCode": flag(&state, "dotnet.debugger.justMyCode", true) });
        r.adapter_command = Some(adapter);
        Ok(r)
    })
    .await
}

// ---- The Roslyn language server -----------------------------------------------------------------

fn roslyn_config(
    state: &AppState,
    root: &Path,
    ws: &DotnetWorkspace,
    dll: &Path,
) -> LspServerConfig {
    let solution = ws.solution.as_deref().map(|s| path_string(&root.join(s)));
    let projects = if solution.is_some() {
        Vec::new()
    } else {
        ws.projects
            .iter()
            .map(|p| path_string(&root.join(&p.path)))
            .collect()
    };
    let setup = RoslynSetup {
        dotnet: dotnet_program(state),
        server_dll: path_string(dll),
        log_dir: dirs::cache_dir()
            .map(|d| path_string(&d.join("forge").join("roslyn").join("logs")))
            .unwrap_or_default(),
        solution,
        projects,
        settings: RoslynSettings {
            analysis_scope: setting(state, "dotnet.roslyn.analysisScope"),
            inlay_hints: flag(state, "dotnet.roslyn.inlayHints", true),
            references_code_lens: false,
        },
    };
    let mut env = environment(state);
    if let Some(h) = status(state, false).host {
        env.insert("DOTNET_ROOT".into(), path_string(&h.root));
    }
    LspServerConfig {
        id: ROSLYN_ID.into(),
        name: Some("Roslyn".into()),
        language_ids: vec!["csharp".into()],
        file_extensions: vec!["cs".into(), "csx".into()],
        command: setup.command(),
        args: setup.args(),
        initialization_options: serde_json::Value::Null,
        settings: setup.settings(),
        env,
        pull_diagnostics: true,
        after_initialized: setup
            .open_notification()
            .map(|(method, params)| vec![LspNotification { method, params }])
            .unwrap_or_default(),
    }
}

/// Register (or remove) the Roslyn server for the open project: a local .NET project with
/// the plugin enabled and the server installed.
pub fn sync_lsp_server(app: &AppHandle, detected: Option<(&Path, &forge_project::ProjectInfo)>) {
    let state = app.state::<AppState>();
    let lsp = app.state::<LspState>();
    let Ok(backend) = state.backend() else {
        lsp.unregister_server(ROSLYN_ID);
        return;
    };
    let root = backend.info().root.clone();
    let fresh;
    let info = match detected {
        Some((r, _)) if r != root => return,
        Some((_, info)) => info,
        None => {
            fresh = match backend.detect_project() {
                Ok(i) => i,
                Err(e) => {
                    tracing::warn!("project detection failed: {e}");
                    return;
                }
            };
            &fresh
        }
    };
    if !enabled(&state) || !info.uses("csharp") || backend.info().remote.is_some() {
        lsp.unregister_server(ROSLYN_ID);
        return;
    }
    let dll = match roslyn(&state) {
        Ok(t) => t.path,
        Err(why) => {
            tracing::info!("Roslyn not started: {why}");
            lsp.unregister_server(ROSLYN_ID);
            return;
        }
    };
    let ws = match load_workspace(&state) {
        Ok(ws) => ws,
        Err(e) => {
            tracing::warn!("Roslyn not started: {e}");
            return;
        }
    };
    let config = roslyn_config(&state, &root, &ws, &dll);
    lsp.unregister_server("csharp-ls");
    lsp.register_server(config);
}

/// After the catalog servers are planned: Roslyn replaces csharp-ls (both serve `.cs`).
pub(crate) fn configure_servers(manager: &LspManager, state: &AppState) {
    if !enabled(state) {
        return;
    }
    if manager.configs().iter().any(|c| c.id == ROSLYN_ID) {
        manager.unregister_server("csharp-ls");
    }
}

/// Whether a changed setting reconfigures the Roslyn server.
pub fn affects_server(key: &str) -> bool {
    key.starts_with("dotnet.roslyn.")
        || matches!(
            key,
            "dotnet.path" | "dotnet.solution" | "dotnet.environment" | "dotnet.configuration"
        )
}

pub fn spawn_sync_lsp_server(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || sync_lsp_server(&app, None));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_that_restart_roslyn() {
        assert!(affects_server("dotnet.roslyn.analysisScope"));
        assert!(affects_server("dotnet.solution"));
        assert!(!affects_server("dotnet.formatter"));
        assert!(temp_dir("x").starts_with(std::env::temp_dir()));
    }
}
