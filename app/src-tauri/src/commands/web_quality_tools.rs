//! Web code quality tools (Settings | Languages & Frameworks | JavaScript | Code Quality
//! Tools): ESLint, Prettier, Stylelint and Biome.
//!
//! ESLint runs through its CLI (`--format json --stdin`), like Stylelint and Biome: one
//! path for all linters, with the reports parsed in Rust (`webforge-quality-tools`). The
//! optional ESLint language server of Languages & Frameworks | Language Servers is separate.
//!
//! The tools are the project's own packages (`node_modules/<package>`), started with the
//! Node.js interpreter of Languages & Frameworks | Node.js where the project lives
//! (`Backend::start_process`, so remote projects run them on their host). The editor text
//! goes through stdin, so unsaved text is checked, fixed and formatted without writing the
//! file; Actions on Save fix the saved file. Running them runs project code (configuration
//! files, plugins), so every command here is refused in Safe Mode.

use std::path::PathBuf;
use std::time::Duration;

use forge_backend::process_io::{capture, Captured};
use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_process::CommandLine;
use forge_save_actions::Tool;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use webforge_quality_tools::command::{self, absolute};
use webforge_quality_tools::detect::{nearest_config, nearest_package};
use webforge_quality_tools::{
    exit_ok, locate, locate_all, parse_report, Invocation, Launcher, Located, Mode, PathContext,
    Problem, ProjectFs, Skip, ToolConfig, WebTool, WebToolsSettings,
};

use crate::commands::workspace::blocking;
use crate::state::AppState;

const SAFE_MODE: &str = "Running web code quality tools";
/// Inspect Code on many files may take much longer than one file.
const INSPECT_MIN_TIMEOUT: Duration = Duration::from_secs(300);

/// The project as [`ProjectFs`]: project-relative paths through the backend; absolute
/// paths (a manual package outside the project) only for local projects.
struct BackendFs<'a> {
    backend: &'a dyn Backend,
}

fn is_absolute(p: &str) -> bool {
    p.starts_with('/') || p.get(1..3).is_some_and(|s| s == ":/" || s == ":\\")
}

impl BackendFs<'_> {
    fn local(&self) -> bool {
        self.backend.info().remote.is_none()
    }
}

impl ProjectFs for BackendFs<'_> {
    fn read(&self, path: &str) -> Option<String> {
        if is_absolute(path) {
            return self
                .local()
                .then(|| std::fs::read_to_string(path).ok())
                .flatten();
        }
        self.backend.read_optional(path).ok().flatten()
    }

    fn list(&self, dir: &str) -> Vec<String> {
        if is_absolute(dir) {
            if !self.local() {
                return Vec::new();
            }
            return std::fs::read_dir(dir)
                .map(|it| {
                    it.filter_map(|e| e.ok())
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
        }
        self.backend
            .list_dir(dir)
            .map(|es| es.into_iter().map(|e| e.name).collect())
            .unwrap_or_default()
    }
}

fn settings_of(state: &AppState) -> WebToolsSettings {
    let settings = state.read_settings();
    WebToolsSettings::from_settings(&|k| settings.get(k).cloned())
}

fn root_of(backend: &dyn Backend) -> String {
    backend.info().root.to_string_lossy().replace('\\', "/")
}

/// The Node.js interpreter: the configured one, else the machine's (local projects) or
/// `node` on the host's PATH (remote projects).
fn node_of(settings: &WebToolsSettings, backend: &dyn Backend) -> String {
    if !settings.node.is_empty() {
        return settings.node.clone();
    }
    let found = if backend.info().remote.is_none() {
        forge_langservers::Detector::system().node()
    } else {
        backend
            .find_executable(&["node".into()], None)
            .ok()
            .flatten()
    };
    found
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "node".into())
}

/// Everything a run needs besides the tool.
struct Env {
    settings: WebToolsSettings,
    node: String,
    root: String,
}

impl Env {
    fn new(state: &AppState, backend: &dyn Backend) -> Self {
        let settings = settings_of(state);
        Self {
            node: node_of(&settings, backend),
            root: root_of(backend),
            settings,
        }
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(self.settings.timeout_secs)
    }

    fn command_line(&self, inv: &Invocation) -> CommandLine {
        let mut cmd = if inv.node {
            CommandLine::new(self.node.clone())
                .args(std::iter::once(inv.program.clone()).chain(inv.args.iter().cloned()))
        } else {
            CommandLine::new(inv.program.clone()).args(inv.args.iter().cloned())
        };
        cmd.cwd = PathBuf::from(&inv.cwd);
        for (k, v) in &inv.env {
            cmd.env.insert(k.clone(), v.clone());
        }
        cmd
    }

    fn run(&self, backend: &dyn Backend, inv: &Invocation, timeout: Duration) -> Result<Captured> {
        capture(backend, &self.command_line(inv), &inv.stdin, timeout)
    }
}

/// The first lines of a failed run's output.
fn summary(out: &Captured) -> String {
    let text = if out.stderr.trim().is_empty() {
        &out.stdout
    } else {
        &out.stderr
    };
    text.trim().lines().take(12).collect::<Vec<_>>().join("\n")
}

fn failure(tool: WebTool, out: &Captured, timeout: Duration) -> String {
    match out.code {
        None => format!(
            "{} did not finish within {} s",
            tool.label(),
            timeout.as_secs()
        ),
        Some(code) => match summary(out) {
            s if s.is_empty() => format!("{} exited with code {code}", tool.label()),
            s => s,
        },
    }
}

/// How one tool's run went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutcomeStatus {
    Ok,
    /// Configured for the file but not installed: skipped.
    Missing,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutcome {
    pub tool: WebTool,
    pub label: String,
    pub status: OutcomeStatus,
    pub message: String,
}

impl ToolOutcome {
    fn new(tool: WebTool, status: OutcomeStatus, message: impl Into<String>) -> Self {
        Self {
            tool,
            label: tool.label().to_string(),
            status,
            message: message.into(),
        }
    }
}

fn missing(tool: WebTool) -> ToolOutcome {
    ToolOutcome::new(
        tool,
        OutcomeStatus::Missing,
        format!(
            "{} is configured but not installed (node_modules/{}). Install the project's dependencies or set its package in Settings | Languages & Frameworks | JavaScript | Code Quality Tools.",
            tool.label(),
            tool.package()
        ),
    )
}

/// Problems of a check: they replace the earlier problems of `tools` in `files`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    /// The checked project files (empty for the whole project).
    pub files: Vec<String>,
    /// The tools whose problems the result replaces.
    pub tools: Vec<WebTool>,
    pub problems: Vec<Problem>,
    pub outcomes: Vec<ToolOutcome>,
}

const LINTERS: [WebTool; 3] = [WebTool::Eslint, WebTool::Stylelint, WebTool::Biome];

/// Run one linter and parse its report.
fn run_lint(
    backend: &dyn Backend,
    env: &Env,
    l: &Located,
    inv: &Invocation,
    stdin_file: Option<&str>,
    timeout: Duration,
) -> (Vec<Problem>, ToolOutcome) {
    let tool = l.tool;
    let fail = |m: String| (Vec::new(), ToolOutcome::new(tool, OutcomeStatus::Failed, m));
    let out = match env.run(backend, inv, timeout) {
        Ok(out) => out,
        Err(e) => return fail(format!("Cannot run {}: {e}", tool.label())),
    };
    if !out.code.is_some_and(|c| exit_ok(tool, c)) {
        return fail(failure(tool, &out, timeout));
    }
    let mut ctx = PathContext::new(&env.root, &inv.cwd);
    if let Some(f) = stdin_file {
        ctx = ctx.with_stdin(f);
    }
    let text_of = |f: &str| -> Option<String> {
        if Some(f) == stdin_file {
            return Some(inv.stdin.clone());
        }
        backend.read_optional(f).ok().flatten()
    };
    match parse_report(tool, &out.stdout, &out.stderr, &ctx, &text_of) {
        Ok(report) => (
            report.problems,
            ToolOutcome::new(tool, OutcomeStatus::Ok, report.errors.join("\n")),
        ),
        Err(_) => fail(failure(tool, &out, timeout)),
    }
}

/// Check one project file with the linters that apply to it (ESLint, Stylelint, Biome),
/// on `text` (the editor text) or the saved file.
fn check_file(
    backend: &dyn Backend,
    env: &Env,
    path: &str,
    text: Option<&str>,
) -> Result<CheckResult> {
    let text = match text {
        Some(t) => t.to_string(),
        None => backend.read_file(path)?,
    };
    let fs = BackendFs { backend };
    let mut jobs: Vec<Located> = Vec::new();
    let mut outcomes = Vec::new();
    for (tool, r) in locate_all(&fs, &env.settings, path) {
        if !tool.lints() {
            continue;
        }
        match r {
            Ok(l) => jobs.push(l),
            Err(Skip::NoPackage) => outcomes.push(missing(tool)),
            Err(_) => {}
        }
    }
    let timeout = env.timeout();
    let results: Vec<(Vec<Problem>, ToolOutcome)> = std::thread::scope(|s| {
        let handles: Vec<_> = jobs
            .iter()
            .map(|l| {
                let cfg = env.settings.config(l.tool);
                let inv = command::lint(l, &cfg, &env.root, path, &text);
                s.spawn(move || run_lint(backend, env, l, &inv, Some(path), timeout))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("web quality tool thread"))
            .collect()
    });
    let mut problems = Vec::new();
    for (p, o) in results {
        problems.extend(p.into_iter().filter(|p| p.file == path));
        outcomes.push(o);
    }
    Ok(CheckResult {
        files: vec![path.to_string()],
        tools: LINTERS.to_vec(),
        problems,
        outcomes,
    })
}

/// A file name each linter checks, to find how it runs in a folder.
fn probe(tool: WebTool) -> &'static str {
    match tool {
        WebTool::Stylelint => "index.css",
        _ => "index.js",
    }
}

/// Code | Inspect Code with the web quality tools: the linters' problems in `paths`
/// (project files or folders) or the whole project.
fn inspect(backend: &dyn Backend, env: &Env, paths: Option<&[String]>) -> Result<CheckResult> {
    let fs = BackendFs { backend };
    let base = paths
        .and_then(|p| p.first())
        .map(|p| {
            if backend.list_dir(p).is_ok() {
                p.trim_end_matches('/').to_string()
            } else {
                p.rsplit_once('/')
                    .map(|(d, _)| d.to_string())
                    .unwrap_or_default()
            }
        })
        .unwrap_or_default();
    let mut jobs: Vec<Located> = Vec::new();
    let mut outcomes = Vec::new();
    for tool in LINTERS {
        let file = if base.is_empty() {
            probe(tool).to_string()
        } else {
            format!("{base}/{}", probe(tool))
        };
        let found = locate_all(&fs, &env.settings, &file)
            .into_iter()
            .find(|(t, _)| *t == tool)
            .map(|(_, r)| r);
        match found {
            Some(Ok(l)) => jobs.push(l),
            Some(Err(Skip::NoPackage)) => outcomes.push(missing(tool)),
            _ => {}
        }
    }
    let targets: Vec<String> = paths
        .unwrap_or_default()
        .iter()
        .map(|p| absolute(&env.root, p))
        .collect();
    let timeout = env.timeout().max(INSPECT_MIN_TIMEOUT);
    let results: Vec<(Vec<Problem>, ToolOutcome)> = std::thread::scope(|s| {
        let handles: Vec<_> = jobs
            .iter()
            .map(|l| {
                let cfg = env.settings.config(l.tool);
                let inv = command::lint_paths(l, &cfg, &env.root, &targets);
                s.spawn(move || run_lint(backend, env, l, &inv, None, timeout))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("web quality tool thread"))
            .collect()
    });
    let mut problems = Vec::new();
    for (p, o) in results {
        problems.extend(p);
        outcomes.push(o);
    }
    Ok(CheckResult {
        files: paths.map(<[String]>::to_vec).unwrap_or_default(),
        tools: jobs.iter().map(|l| l.tool).collect(),
        problems,
        outcomes,
    })
}

/// A fixer's or formatter's result for the editor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixResult {
    pub tool: Option<WebTool>,
    /// The fixed text (the input when nothing changes).
    pub text: String,
    pub changed: bool,
}

/// How `tool` runs for `path`, or why it does not.
fn located_for(backend: &dyn Backend, env: &Env, tool: WebTool, path: &str) -> Result<Located> {
    let fs = BackendFs { backend };
    let cfg = env.settings.config(tool);
    locate(&fs, &cfg, path).map_err(|skip| {
        ForgeError::Other(match skip {
            Skip::Disabled => format!("{} is disabled in Settings | Languages & Frameworks | JavaScript | Code Quality Tools", tool.label()),
            Skip::NotMatched => format!("{} does not run for {path} (Run for files)", tool.label()),
            Skip::NoConfig => format!("No {} configuration file was found for {path}", tool.label()),
            Skip::NoPackage => missing(tool).message,
            Skip::BiomeFirst => format!("Biome handles {path}"),
        })
    })
}

/// The text a fixer or formatter printed: ESLint's JSON `output`, else stdout.
fn fixed_text(
    l: &Located,
    env: &Env,
    inv: &Invocation,
    out: &Captured,
    path: &str,
    timeout: Duration,
) -> Result<Option<String>> {
    let tool = l.tool;
    if !out.code.is_some_and(|c| exit_ok(tool, c)) {
        return Err(ForgeError::Other(failure(tool, out, timeout)));
    }
    match tool {
        WebTool::Eslint => {
            let ctx = PathContext::new(&env.root, &inv.cwd).with_stdin(path);
            let report = parse_report(tool, &out.stdout, &out.stderr, &ctx, &|_| None)
                .map_err(|_| ForgeError::Other(failure(tool, out, timeout)))?;
            Ok(report.outputs.into_iter().next().map(|o| o.text))
        }
        _ => {
            if out.stdout.is_empty() {
                return Ok(None);
            }
            // An old Stylelint prints its report instead of the fixed text.
            if tool == WebTool::Stylelint
                && parse_report(tool, &out.stdout, "", &PathContext::default(), &|_| None).is_ok()
            {
                return Err(ForgeError::Other(
                    "This Stylelint version does not print fixed text from stdin; fix on save instead".into(),
                ));
            }
            Ok(Some(out.stdout.clone()))
        }
    }
}

fn run_fix(
    backend: &dyn Backend,
    env: &Env,
    l: &Located,
    inv: Invocation,
    path: &str,
    text: &str,
) -> Result<FixResult> {
    let timeout = env.timeout();
    let out = env.run(backend, &inv, timeout)?;
    let fixed = fixed_text(l, env, &inv, &out, path, timeout)?.unwrap_or_else(|| text.to_string());
    Ok(FixResult {
        tool: Some(l.tool),
        changed: fixed != text,
        text: fixed,
    })
}

/// "Fix ESLint problems", "Fix Stylelint problems", "Fix Biome problems" and Reformat with
/// Prettier: the fixed editor text of `path`.
fn fix(
    backend: &dyn Backend,
    env: &Env,
    tool: WebTool,
    path: &str,
    text: &str,
) -> Result<FixResult> {
    let l = located_for(backend, env, tool, path)?;
    let cfg = env.settings.config(tool);
    let inv = command::fix(&l, &cfg, &env.root, path, text);
    run_fix(backend, env, &l, inv, path, text)
}

/// Reformat Code: the text of the formatter turned on for `path` (Biome before Prettier),
/// `None` when neither is (the usual formatter runs then).
fn format(backend: &dyn Backend, env: &Env, path: &str, text: &str) -> Result<Option<FixResult>> {
    let fs = BackendFs { backend };
    let Some(l) = webforge_quality_tools::formatter(&fs, &env.settings, path) else {
        return Ok(None);
    };
    let cfg = env.settings.config(l.tool);
    let inv = command::format(&l, &cfg, &env.root, path, text);
    run_fix(backend, env, &l, inv, path, text).map(Some)
}

/// One tool on the Code Quality Tools page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub tool: WebTool,
    pub label: String,
    pub package: String,
    pub config_files: Vec<String>,
    /// The package found in the project (`node_modules/eslint`), or the manual one.
    pub path: Option<String>,
    pub version: Option<String>,
    /// The configuration file found in the project root (or package.json).
    pub config_file: Option<String>,
    pub config: ToolConfig,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebToolsInfo {
    pub tools: Vec<ToolInfo>,
    /// The Node.js interpreter the tools run with.
    pub node: String,
}

fn tool_info(backend: Option<&dyn Backend>, cfg: ToolConfig) -> ToolInfo {
    let tool = cfg.tool;
    let (path, version, config_file) = match backend {
        Some(b) => {
            let fs = BackendFs { backend: b };
            let (path, version) = match cfg.custom_package() {
                Some(p) => (Some(p.to_string()), None),
                None => match nearest_package(&fs, tool, "") {
                    Some((pkg, pj)) => (
                        Some(pkg),
                        serde_json::from_str::<serde_json::Value>(&pj)
                            .ok()
                            .and_then(|v| v["version"].as_str().map(str::to_string)),
                    ),
                    None => (None, None),
                },
            };
            (path, version, nearest_config(&fs, tool, probe(tool)))
        }
        None => (None, None, None),
    };
    ToolInfo {
        tool,
        label: tool.label().to_string(),
        package: tool.package().to_string(),
        config_files: tool.config_files().iter().map(|s| s.to_string()).collect(),
        path,
        version,
        config_file,
        config: cfg,
    }
}

/// The Code Quality Tools page: every tool's configuration, the package and configuration
/// file found in the project, and the Node.js interpreter.
#[tauri::command]
pub async fn web_quality_tools_info(state: State<'_, AppState>) -> Result<WebToolsInfo> {
    let settings = settings_of(&state);
    let backend = state.backend().ok();
    blocking(move || {
        let node = match backend.as_deref() {
            Some(b) => node_of(&settings, b),
            None => settings.node.clone(),
        };
        Ok(WebToolsInfo {
            tools: settings
                .tools
                .into_iter()
                .map(|c| tool_info(backend.as_deref(), c))
                .collect(),
            node,
        })
    })
    .await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Validation {
    pub ok: bool,
    /// The tool's version, or why it does not run.
    pub message: String,
    pub command_line: String,
}

fn validate(backend: &dyn Backend, env: &Env, cfg: &ToolConfig) -> Result<Validation> {
    let fs = BackendFs { backend };
    let tool = cfg.tool;
    // The configuration as on the page, for a file in the root, without needing a
    // configuration file.
    let manual = ToolConfig {
        mode: Mode::Manual,
        files: "*".into(),
        config: String::new(),
        ..cfg.clone()
    };
    let located = match cfg.custom_package() {
        Some(_) => locate(&fs, &manual, probe(tool)).ok(),
        None => nearest_package(&fs, tool, "").map(|(pkg, pj)| Located {
            tool,
            launcher: Launcher::Node {
                script: format!(
                    "{pkg}/{}",
                    webforge_quality_tools::detect::bin_entry(tool, &pj)
                ),
            },
            package: Some(pkg),
            version: None,
            config: None,
            explicit_config: false,
            working_dir: String::new(),
            flat_config: None,
        }),
    };
    let Some(l) = located else {
        return Ok(Validation {
            ok: false,
            message: missing(tool).message,
            command_line: String::new(),
        });
    };
    let cmd = env.command_line(&command::version(&l, &env.root));
    let command_line = std::iter::once(cmd.program.clone())
        .chain(cmd.args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ");
    let timeout = env.timeout();
    let out = capture(backend, &cmd, "", timeout)?;
    let version = (out.code == Some(0))
        .then(|| command::parse_version(&out.stdout))
        .flatten();
    Ok(match version {
        Some(v) => Validation {
            ok: true,
            message: v,
            command_line,
        },
        None => Validation {
            ok: false,
            message: failure(tool, &out, timeout),
            command_line,
        },
    })
}

/// "Validate": run the tool's `--version` with the configuration shown in the settings
/// dialog (not applied yet).
#[tauri::command]
pub async fn web_quality_tools_validate(
    config: ToolConfig,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Validation> {
    state.require_trusted(SAFE_MODE)?;
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let env = Env::new(&state, backend.as_ref());
        validate(backend.as_ref(), &env, &config)
    })
    .await
}

/// Check one file (on the fly with the editor `text`, or the saved file).
#[tauri::command]
pub async fn web_quality_tools_check(
    path: String,
    text: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CheckResult> {
    state.require_trusted(SAFE_MODE)?;
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let env = Env::new(&state, backend.as_ref());
        check_file(backend.as_ref(), &env, &path, text.as_deref())
    })
    .await
}

/// Code | Inspect Code with the web quality tools (`paths`: files or folders; none: the
/// whole project).
#[tauri::command]
pub async fn web_quality_tools_inspect(
    paths: Option<Vec<String>>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CheckResult> {
    state.require_trusted(SAFE_MODE)?;
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let env = Env::new(&state, backend.as_ref());
        inspect(backend.as_ref(), &env, paths.as_deref())
    })
    .await
}

/// The fixed editor text of `path` (`tool --fix` on stdin).
#[tauri::command]
pub async fn web_quality_tools_fix(
    path: String,
    tool: WebTool,
    text: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<FixResult> {
    state.require_trusted(SAFE_MODE)?;
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let env = Env::new(&state, backend.as_ref());
        fix(backend.as_ref(), &env, tool, &path, &text)
    })
    .await
}

/// Reformat Code with Prettier or Biome when one is turned on for `path`; `None`: the
/// usual formatter. In Safe Mode the usual formatter runs.
#[tauri::command]
pub async fn web_quality_tools_format(
    path: String,
    text: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<FixResult>> {
    if state.require_trusted(SAFE_MODE).is_err() {
        return Ok(None);
    }
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let env = Env::new(&state, backend.as_ref());
        format(backend.as_ref(), &env, &path, &text)
    })
    .await
}

// ---- Actions on Save ----------------------------------------------------------------------

/// The web tool behind a tool of Actions on Save.
pub(crate) fn web_tool(tool: Tool) -> Option<WebTool> {
    match tool {
        Tool::Eslint => Some(WebTool::Eslint),
        Tool::Prettier => Some(WebTool::Prettier),
        Tool::Stylelint => Some(WebTool::Stylelint),
        Tool::Biome => Some(WebTool::Biome),
        _ => None,
    }
}

/// How the web tools of one save run.
pub(crate) struct WebSave {
    env: Env,
    located: Vec<(Tool, Located)>,
    /// The tools of the save that still run: the PHP ones and the web tools that apply to
    /// the file (or are configured for it but missing, to report that).
    pub tools: Vec<Tool>,
}

impl WebSave {
    pub fn new(state: &AppState, backend: &dyn Backend, path: &str, tools: &[Tool]) -> Self {
        let env = Env::new(state, backend);
        let fs = BackendFs { backend };
        let all = locate_all(&fs, &env.settings, path);
        let mut located = Vec::new();
        let mut kept = Vec::new();
        for &tool in tools {
            let Some(web) = web_tool(tool) else {
                kept.push(tool);
                continue;
            };
            match all.iter().find(|(t, _)| *t == web).map(|(_, r)| r) {
                Some(Ok(l)) => {
                    located.push((tool, l.clone()));
                    kept.push(tool);
                }
                Some(Err(Skip::NoPackage)) => kept.push(tool),
                _ => {}
            }
        }
        Self {
            env,
            located,
            tools: kept,
        }
    }

    fn find(&self, tool: Tool) -> Option<&Located> {
        self.located
            .iter()
            .find(|(t, _)| *t == tool)
            .map(|(_, l)| l)
    }

    /// The tool's script or executable (absolute), where Actions on Save look for it.
    pub fn program(&self, tool: Tool) -> Option<String> {
        self.find(tool).map(|l| match &l.launcher {
            Launcher::Node { script } => absolute(&self.env.root, script),
            Launcher::Executable { path } => absolute(&self.env.root, path),
        })
    }

    /// The command line that fixes the saved `file` (absolute) in place.
    pub fn command(&self, tool: Tool, file: &str) -> Result<CommandLine> {
        let l = self
            .find(tool)
            .ok_or_else(|| ForgeError::Other(format!("{} does not apply", tool.label())))?;
        let cfg = self.env.settings.config(l.tool);
        let rel = file
            .replace('\\', "/")
            .strip_prefix(&format!("{}/", self.env.root.trim_end_matches('/')))
            .map(str::to_string)
            .unwrap_or_else(|| file.to_string());
        Ok(self
            .env
            .command_line(&command::fix_in_place(l, &cfg, &self.env.root, &rel)))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use forge_backend::LocalBackend;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    /// Scripts are written and run one test at a time ("Text file busy" otherwise).
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn write(dir: &Path, rel: &str, body: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, body).unwrap();
    }

    fn script(dir: &Path, rel: &str, body: &str) {
        write(dir, rel, &format!("#!/bin/sh\n{body}\n"));
        std::fs::set_permissions(dir.join(rel), std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A project whose packages are shell scripts, run by a fake `node` (`sh <script>`).
    fn project() -> (tempfile::TempDir, Env) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        script(p, ".bin/node", "s=$1; shift; exec sh \"$s\" \"$@\"");
        write(p, "package.json", r#"{"name":"shop"}"#);
        write(p, "eslint.config.js", "export default [];");
        write(p, ".stylelintrc.json", "{}");
        write(p, ".prettierrc", "{}");
        for (pkg, bin) in [
            ("eslint", "bin/eslint.js"),
            ("stylelint", "bin/stylelint.mjs"),
            ("prettier", "bin/prettier.cjs"),
        ] {
            write(
                p,
                &format!("node_modules/{pkg}/package.json"),
                &format!(r#"{{"version":"1.0.0","bin":{{"{pkg}":"{bin}"}}}}"#),
            );
        }
        let root = p.to_string_lossy().into_owned();
        let mut settings = WebToolsSettings::from_settings(&|_| None);
        settings.node = format!("{root}/.bin/node");
        settings.timeout_secs = 20;
        let env = Env {
            settings,
            node: format!("{root}/.bin/node"),
            root,
        };
        (dir, env)
    }

    #[test]
    fn checks_the_editor_text_through_stdin() {
        let _serial = serial();
        let (dir, env) = project();
        let p = dir.path();
        // ESLint: reports the stdin file name with one problem per `==` line.
        write(
            p,
            "node_modules/eslint/bin/eslint.js",
            r#"for a; do f=$a; done
[ "$ESLINT_USE_FLAT_CONFIG" = true ] || exit 7
n=$(grep -n '==' | head -1 | cut -d: -f1)
echo "[{\"filePath\":\"$f\",\"messages\":[{\"ruleId\":\"eqeqeq\",\"severity\":2,\"message\":\"Expected '==='\",\"line\":$n,\"column\":3}]}]"
exit 1"#,
        );
        // Stylelint 16: the report on stderr.
        write(
            p,
            "node_modules/stylelint/bin/stylelint.mjs",
            r#"cat > /dev/null
echo '[{"source":"<input css 1>","warnings":[{"line":1,"column":1,"rule":"block-no-empty","severity":"error","text":"Unexpected empty block (block-no-empty)"}]}]' >&2
exit 2"#,
        );
        write(p, "src/a.js", "let a = 1;\n");
        write(p, "src/a.css", "a {}\n");
        let backend = LocalBackend::open(p).unwrap();

        let js = check_file(&backend, &env, "src/a.js", Some("let a;\nif (a == 1) {}\n")).unwrap();
        assert_eq!(js.tools, LINTERS.to_vec());
        assert_eq!(js.problems.len(), 1, "{:?}", js.outcomes);
        assert_eq!(
            (js.problems[0].file.as_str(), js.problems[0].line),
            ("src/a.js", 1)
        );
        assert_eq!(js.outcomes.len(), 1);

        let css = check_file(&backend, &env, "src/a.css", None).unwrap();
        assert_eq!(css.problems[0].tool, WebTool::Stylelint);
        assert_eq!(css.problems[0].file, "src/a.css");
        assert_eq!(css.problems[0].message, "Unexpected empty block");
    }

    #[test]
    fn a_configured_tool_without_its_package_is_reported_missing() {
        let _serial = serial();
        let (dir, env) = project();
        let p = dir.path();
        std::fs::remove_dir_all(p.join("node_modules/eslint")).unwrap();
        write(p, "a.ts", "x\n");
        let backend = LocalBackend::open(p).unwrap();
        let r = check_file(&backend, &env, "a.ts", None).unwrap();
        assert!(r.problems.is_empty());
        assert_eq!(r.outcomes[0].status, OutcomeStatus::Missing);
        assert!(r.outcomes[0].message.contains("node_modules/eslint"));
    }

    #[test]
    fn fixes_and_formats_the_editor_text() {
        let _serial = serial();
        let (dir, mut env) = project();
        let p = dir.path();
        // eslint --fix-dry-run: `var` becomes `let` in the JSON output.
        write(
            p,
            "node_modules/eslint/bin/eslint.js",
            r#"for a; do f=$a; done
[ "$1" = --fix-dry-run ] || exit 9
out=$(sed 's/var /let /')
printf '[{"filePath":"%s","messages":[],"output":"%s\\n"}]' "$f" "$out""#,
        );
        // prettier --stdin-filepath: double quotes become single quotes.
        write(p, "node_modules/prettier/bin/prettier.cjs", "tr '\"' \"'\"");
        write(p, "a.js", "var a = \"x\";\n");
        let backend = LocalBackend::open(p).unwrap();
        let f = fix(&backend, &env, WebTool::Eslint, "a.js", "var a = 1;\n").unwrap();
        assert!(f.changed);
        assert_eq!(f.text, "let a = 1;\n");

        // Reformat Code uses Prettier only when "On Reformat Code" is on.
        assert!(format(&backend, &env, "a.js", "var a = \"x\";\n")
            .unwrap()
            .is_none());
        for c in &mut env.settings.tools {
            c.on_reformat = true;
        }
        let r = format(&backend, &env, "a.js", "var a = \"x\";\n")
            .unwrap()
            .unwrap();
        assert_eq!(r.tool, Some(WebTool::Prettier));
        assert_eq!(r.text, "var a = 'x';\n");
        // A tool that does not apply says why.
        let err = fix(&backend, &env, WebTool::Biome, "a.js", "").unwrap_err();
        assert!(
            err.to_string().contains("No Biome configuration file"),
            "{err}"
        );
    }

    #[test]
    fn save_runs_the_located_package_with_node() {
        let _serial = serial();
        let (dir, env) = project();
        let p = dir.path();
        write(
            p,
            "node_modules/prettier/bin/prettier.cjs",
            "for a; do f=$a; done\nsed -i 's/  */ /g' \"$f\"",
        );
        write(p, "src/a.ts", "let  a  = 1;\n");
        let backend = LocalBackend::open(p).unwrap();
        let save = WebSave {
            located: Vec::new(),
            tools: Vec::new(),
            env,
        };
        let fs = BackendFs { backend: &backend };
        let l = locate(
            &fs,
            &save.env.settings.config(WebTool::Prettier),
            "src/a.ts",
        )
        .unwrap();
        let save = WebSave {
            located: vec![(Tool::Prettier, l)],
            tools: vec![Tool::Prettier],
            ..save
        };
        let root = save.env.root.clone();
        assert_eq!(
            save.program(Tool::Prettier).unwrap(),
            format!("{root}/node_modules/prettier/bin/prettier.cjs")
        );
        let report = forge_save_actions::tools::run_tools_with(
            &backend,
            "src/a.ts",
            &save.tools,
            &|t| save.program(t),
            &|t, cmd| save.command(t, cmd.args.last().map(String::as_str).unwrap_or("")),
            Duration::from_secs(20),
        )
        .unwrap();
        assert!(report.changed, "{:?}", report.outcomes);
        assert_eq!(
            std::fs::read_to_string(p.join("src/a.ts")).unwrap(),
            "let a = 1;\n"
        );
    }

    #[test]
    fn validate_reports_the_version() {
        let _serial = serial();
        let (dir, env) = project();
        write(
            dir.path(),
            "node_modules/eslint/bin/eslint.js",
            "echo v9.25.1",
        );
        let backend = LocalBackend::open(dir.path()).unwrap();
        let v = validate(&backend, &env, &ToolConfig::new(WebTool::Eslint)).unwrap();
        assert!(v.ok, "{}", v.message);
        assert_eq!(v.message, "v9.25.1");
        assert!(v
            .command_line
            .ends_with("node_modules/eslint/bin/eslint.js --version"));
        let biome = validate(&backend, &env, &ToolConfig::new(WebTool::Biome)).unwrap();
        assert!(!biome.ok && biome.message.contains("@biomejs/biome"));
    }
}
