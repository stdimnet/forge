//! PHP quality tools (Settings | Languages & Frameworks | PHP | Quality Tools): PHPStan,
//! Psalm, PHP CS Fixer, PHP_CodeSniffer, PHP Mess Detector, Laravel Pint and Rector.
//!
//! The tools run where the project lives (`Backend::start_process`, so remote projects run
//! them on their host), directly or with the PHP interpreter a tool's configuration names
//! (local, Docker, Docker Compose, SSH, WSL through `php_interpreters::prepare`, whose path
//! mappings also map the reported paths back). The command lines, the report parsing and
//! the diffs are in `phpforge-quality-tools`. Running a project's vendor tools runs project
//! code, so every command here is refused in Safe Mode.
//!
//! The Inspect Code side is [`inspect`]: problems of every tool whose inspection is on, for
//! some files or the whole project.

use std::path::{Path, PathBuf};
use std::time::Duration;

use forge_backend::process_io::{capture, Captured};
use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_process::CommandLine;
use forge_runtargets::mapping::{join, normalize, Mapper};
use phpforge_interpreters::{InterpreterKind, Program};
use phpforge_quality_tools::model::{configs_from_settings, PROJECT_INTERPRETER};
use phpforge_quality_tools::{
    check_args, command, diff, fix_args, fix_mode, parse_report, tools_to_run, CheckTrigger,
    FixMode, Input, PathContext, Problem, QualityTool, ToolArgs, ToolConfig, ToolKind,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::commands::php_interpreters::{self as interpreters, Prepared};
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Inspect Code on many files may take much longer than one file.
const INSPECT_MIN_TIMEOUT: Duration = Duration::from_secs(300);

/// How to start one tool: its configuration and the interpreter it runs with.
#[derive(Debug, Clone)]
pub(crate) struct ToolSetup {
    pub cfg: ToolConfig,
    /// `None`: the executable runs directly.
    pub interp: Option<Prepared>,
}

impl ToolSetup {
    fn direct(cfg: ToolConfig) -> Self {
        Self { cfg, interp: None }
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(self.cfg.timeout_secs)
    }

    /// The tool's PHP runs on this machine (or where the project is), so it sees the
    /// system's temporary directory.
    fn runs_locally(&self) -> bool {
        self.interp
            .as_ref()
            .is_none_or(|p| p.interp.kind == InterpreterKind::Local)
    }
}

/// Every tool's configuration.
fn configs(state: &AppState) -> Vec<ToolConfig> {
    let settings = state.read_settings();
    configs_from_settings(&|k| settings.get(k).cloned())
}

/// The interpreter of a configuration (`""`: none, the executable runs directly).
fn setup(state: &AppState, backend: &dyn Backend, cfg: ToolConfig) -> Result<ToolSetup> {
    let interp = match cfg.interpreter.as_str() {
        "" => None,
        PROJECT_INTERPRETER => Some(interpreters::resolve(state, "")),
        name => Some(
            interpreters::interpreter_set(state)
                .find(name)
                .cloned()
                .ok_or_else(|| {
                    ForgeError::Other(format!(
                        "{}: the PHP interpreter \"{name}\" is not configured",
                        cfg.tool.label()
                    ))
                })?,
        ),
    };
    Ok(ToolSetup {
        interp: interp.map(|i| interpreters::prepare(state, backend, i)),
        cfg,
    })
}

fn setups(
    state: &AppState,
    backend: &dyn Backend,
    tools: &[QualityTool],
) -> Vec<Result<ToolSetup>> {
    configs(state)
        .into_iter()
        .filter(|c| tools.contains(&c.tool))
        .map(|c| setup(state, backend, c))
        .collect()
}

fn root_of(backend: &dyn Backend) -> String {
    normalize(&backend.info().root.to_string_lossy())
}

/// The tool's executable (`fix`: phpcbf instead of phpcs): the configured path (absolute
/// or relative to the project root), else `vendor/bin`.
fn locate(backend: &dyn Backend, setup: &ToolSetup, fix: bool) -> Result<Option<PathBuf>> {
    let root = root_of(backend);
    let tool = setup.cfg.tool;
    if let Some(custom) = setup.cfg.custom_path(fix) {
        return backend.find_executable(&[], Some(&join(&root, custom)));
    }
    let candidates = if fix {
        tool.fixer_candidates()
    } else {
        tool.candidates()
    };
    for rel in candidates {
        // The `.bat` proxies only run on Windows without an interpreter.
        if setup.interp.is_some() && rel.ends_with(".bat") {
            continue;
        }
        if let Some(found) = backend.find_executable(&[], Some(&join(&root, &rel)))? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

/// The configuration file: the configured one, else the first the tool would find in the
/// project root (absolute).
fn config_file(backend: &dyn Backend, cfg: &ToolConfig) -> Option<String> {
    let root = root_of(backend);
    if !cfg.config.is_empty() {
        return Some(join(&root, &cfg.config));
    }
    let names: Vec<String> = backend
        .list_dir("")
        .ok()?
        .into_iter()
        .map(|e| e.name)
        .collect();
    cfg.tool
        .config_files()
        .iter()
        .find(|f| names.iter().any(|n| n == *f))
        .map(|f| join(&root, f))
}

/// The configuration file passed on the command line: the configured one; a found one only
/// for PHPMD (the other tools find theirs in the working directory themselves).
fn config_arg(backend: &dyn Backend, cfg: &ToolConfig) -> Option<String> {
    if !cfg.config.is_empty() || cfg.tool == QualityTool::Phpmd {
        config_file(backend, cfg)
    } else {
        None
    }
}

/// The command line that runs `program` with `args` for `setup`, and where it runs.
fn build(
    backend: &dyn Backend,
    setup: &ToolSetup,
    program: &Path,
    args: Vec<String>,
) -> Result<(CommandLine, PathContext)> {
    let root = root_of(backend);
    let mut cmd = CommandLine::new(program.to_string_lossy()).args(args);
    cmd.cwd = PathBuf::from(&root);
    let mut ctx = PathContext::local(&root);
    let Some(prepared) = &setup.interp else {
        return Ok((cmd, ctx));
    };
    // `php <tool> …` with the interpreter's PHP; absolute paths in the arguments are mapped.
    let mut php = CommandLine::new("php").args(
        std::iter::once(program.to_string_lossy().into_owned()).chain(cmd.args.iter().cloned()),
    );
    php.cwd = cmd.cwd.clone();
    let wrapped = prepared.wrap(Program::Php, &php)?;
    if prepared.interp.kind.is_remote() {
        ctx.mapper = Mapper::new(&prepared.ctx.root, &prepared.ctx.mappings);
        ctx.cwd = ctx.mapper.to_remote(&root).unwrap_or(root);
    }
    Ok((wrapped, ctx))
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

fn failure(tool: QualityTool, out: &Captured, timeout: Duration) -> String {
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
    /// Not installed in the project: skipped.
    Missing,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolOutcome {
    pub tool: QualityTool,
    pub label: String,
    pub status: OutcomeStatus,
    pub message: String,
}

impl ToolOutcome {
    fn new(tool: QualityTool, status: OutcomeStatus, message: impl Into<String>) -> Self {
        Self {
            tool,
            label: tool.label().to_string(),
            status,
            message: message.into(),
        }
    }
}

/// Problems of a check: they replace the earlier problems of `tools` in `files`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    /// The checked project files (empty for the whole project).
    pub files: Vec<String>,
    /// The tools that ran (their problems replace earlier ones).
    pub tools: Vec<QualityTool>,
    pub problems: Vec<Problem>,
    pub outcomes: Vec<ToolOutcome>,
}

fn missing(tool: QualityTool, fix: bool) -> ToolOutcome {
    let bin = if fix {
        tool.fixer_binary()
    } else {
        tool.binary()
    };
    ToolOutcome::new(
        tool,
        OutcomeStatus::Missing,
        format!(
            "{} is not installed in the project (vendor/bin/{bin}). Install it with Composer or set its path in Settings | Languages & Frameworks | PHP | Quality Tools.",
            tool.label()
        ),
    )
}

/// Run one tool's check and parse its report.
fn run_check(
    backend: &dyn Backend,
    setup: &ToolSetup,
    input: &Input,
    timeout: Duration,
    extra: &[String],
) -> (Vec<Problem>, ToolOutcome) {
    let tool = setup.cfg.tool;
    let fail = |m: String| (Vec::new(), ToolOutcome::new(tool, OutcomeStatus::Failed, m));
    let program = match locate(backend, setup, false) {
        Ok(Some(p)) => p,
        Ok(None) => return (Vec::new(), missing(tool, false)),
        Err(e) => return fail(e.to_string()),
    };
    let config = config_arg(backend, &setup.cfg);
    let ToolArgs { mut args, stdin } = check_args(&setup.cfg, config.as_deref(), input);
    args.extend(extra.iter().cloned());
    let (cmd, mut ctx) = match build(backend, setup, &program, args) {
        Ok(b) => b,
        Err(e) => return fail(e.to_string()),
    };
    ctx.aliases = input.aliases();
    let out = match capture(backend, &cmd, &stdin, timeout) {
        Ok(out) => out,
        Err(e) => return fail(format!("Cannot run {}: {e}", tool.label())),
    };
    if out.code.is_none() {
        return fail(failure(tool, &out, timeout));
    }
    match parse_report(tool, &out.stdout, &ctx, setup.cfg.severity) {
        Ok(report) => {
            let message = report.errors.join("\n");
            (
                report.problems,
                ToolOutcome::new(tool, OutcomeStatus::Ok, message),
            )
        }
        Err(_) => fail(failure(tool, &out, timeout)),
    }
}

/// Run the checks of `setups` in parallel; problems and outcomes in the tools' order.
fn run_all(
    backend: &dyn Backend,
    setups: &[ToolSetup],
    input: &dyn Fn(&ToolSetup) -> (Input, Vec<String>),
    timeout: &dyn Fn(&ToolSetup) -> Duration,
) -> (Vec<Problem>, Vec<ToolOutcome>) {
    let results: Vec<(Vec<Problem>, ToolOutcome)> = std::thread::scope(|s| {
        let handles: Vec<_> = setups
            .iter()
            .map(|setup| {
                let (input, extra) = input(setup);
                let timeout = timeout(setup);
                s.spawn(move || run_check(backend, setup, &input, timeout, &extra))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("quality tool thread"))
            .collect()
    });
    let mut problems = Vec::new();
    let mut outcomes = Vec::new();
    for (p, o) in results {
        problems.extend(p);
        outcomes.push(o);
    }
    (problems, outcomes)
}

/// A temporary copy of unsaved text, removed when dropped.
struct TempCopy(PathBuf);

impl TempCopy {
    fn write(path: &str, text: &str) -> Option<Self> {
        let name = Path::new(path).file_name()?.to_string_lossy().into_owned();
        let dir = std::env::temp_dir()
            .join("forge-quality-tools")
            .join(format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or_default()
            ));
        std::fs::create_dir_all(&dir).ok()?;
        let file = dir.join(name);
        std::fs::write(&file, text).ok()?;
        Some(Self(file))
    }
}

impl Drop for TempCopy {
    fn drop(&mut self) {
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// Check one project file with the tools whose inspection is on. With `text` (the editor
/// text differs from the file) only the tools that can read it run: phpcs and PHP CS Fixer
/// through stdin, PHPStan through a temporary copy when its PHP runs on this machine.
pub(crate) fn check_file(
    backend: &dyn Backend,
    setups: &[ToolSetup],
    path: &str,
    text: Option<&str>,
) -> Result<CheckResult> {
    let abs = normalize(&backend.resolve(path)?.to_string_lossy());
    let unsaved = text.filter(|t| backend.read_file(path).ok().as_deref() != Some(*t));
    let local_project = backend.info().remote.is_none();
    let trigger = if unsaved.is_some() {
        CheckTrigger::Unsaved
    } else {
        CheckTrigger::Saved
    };
    let configs: Vec<ToolConfig> = setups.iter().map(|s| s.cfg.clone()).collect();
    let temp_ok = local_project && setups.iter().all(ToolSetup::runs_locally);
    let run: Vec<QualityTool> = tools_to_run(&configs, trigger, temp_ok);
    let chosen: Vec<ToolSetup> = setups
        .iter()
        .filter(|s| run.contains(&s.cfg.tool))
        .cloned()
        .collect();
    let copy = match (
        unsaved,
        chosen.iter().any(|s| s.cfg.tool == QualityTool::Phpstan),
    ) {
        (Some(t), true) => TempCopy::write(path, t),
        _ => None,
    };
    let input = |s: &ToolSetup| -> (Input, Vec<String>) {
        let input = match (unsaved, s.cfg.tool, &copy) {
            (Some(t), QualityTool::Phpcs | QualityTool::PhpCsFixer, _) => Input::Stdin {
                file: abs.clone(),
                text: t.to_string(),
            },
            (Some(_), QualityTool::Phpstan, Some(c)) => Input::TempCopy {
                file: abs.clone(),
                copy: normalize(&c.0.to_string_lossy()),
            },
            _ => Input::Files(vec![abs.clone()]),
        };
        (input, Vec::new())
    };
    let (problems, outcomes) = run_all(backend, &chosen, &input, &ToolSetup::timeout);
    Ok(CheckResult {
        files: vec![path.to_string()],
        tools: chosen.iter().map(|s| s.cfg.tool).collect(),
        problems,
        outcomes,
    })
}

/// Code | Inspect Code with the quality tools: the problems of every tool whose inspection
/// is on, in `paths` (project files or folders) or the whole project (`None`; tools with
/// a configuration file check what it names).
pub(crate) fn inspect(
    backend: &dyn Backend,
    setups: &[ToolSetup],
    paths: Option<&[String]>,
) -> Result<CheckResult> {
    let root = root_of(backend);
    let chosen: Vec<ToolSetup> = setups
        .iter()
        .filter(|s| s.cfg.inspection)
        .cloned()
        .collect();
    let files: Vec<String> = match paths {
        Some(p) => p.iter().map(|f| join(&root, f)).collect(),
        None => Vec::new(),
    };
    let input = |s: &ToolSetup| -> (Input, Vec<String>) {
        if !files.is_empty() {
            return (Input::Files(files.clone()), Vec::new());
        }
        let has_config = config_file(backend, &s.cfg).is_some();
        match s.cfg.tool {
            QualityTool::Phpmd => (
                Input::Files(vec![root.clone()]),
                vec!["--exclude".into(), "vendor/*,node_modules/*".into()],
            ),
            _ if has_config => (Input::Files(Vec::new()), Vec::new()),
            QualityTool::Phpcs => (
                Input::Files(vec![root.clone()]),
                vec!["--ignore=*/vendor/*,*/node_modules/*".into()],
            ),
            _ => (Input::Files(vec![root.clone()]), Vec::new()),
        }
    };
    let timeout = |s: &ToolSetup| s.timeout().max(INSPECT_MIN_TIMEOUT);
    let (problems, outcomes) = run_all(backend, &chosen, &input, &timeout);
    Ok(CheckResult {
        files: paths.map(<[String]>::to_vec).unwrap_or_default(),
        tools: chosen.iter().map(|s| s.cfg.tool).collect(),
        problems,
        outcomes,
    })
}

/// A fixer's result for the editor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixResult {
    pub tool: Option<QualityTool>,
    /// The fixed text (the input when nothing changes).
    pub text: String,
    pub changed: bool,
    /// The tool's diff (PHP CS Fixer, Rector), for the preview.
    pub diff: String,
    /// The fixers or rectors that apply.
    pub rules: Vec<String>,
}

/// Fix the editor `text` of `path` with `setup`'s fixer, without writing the file (Pint
/// rewrites the saved file, which is restored afterwards; Rector and Pint need the text to
/// be saved).
pub(crate) fn fix(
    backend: &dyn Backend,
    setup: &ToolSetup,
    path: &str,
    text: &str,
) -> Result<FixResult> {
    let tool = setup.cfg.tool;
    let mode = fix_mode(tool)
        .ok_or_else(|| ForgeError::Other(format!("{} does not fix files", tool.label())))?;
    let program = locate(backend, setup, true)?
        .ok_or_else(|| ForgeError::Other(missing(tool, true).message))?;
    let abs = normalize(&backend.resolve(path)?.to_string_lossy());
    let config = config_arg(backend, &setup.cfg);
    let args = fix_args(&setup.cfg, config.as_deref(), &abs, text);
    let saved = backend.read_bytes(path)?;
    let needs_saved = mode == FixMode::InPlace || tool == QualityTool::Rector;
    if needs_saved && saved != text.as_bytes() {
        return Err(ForgeError::Other(format!(
            "{} works on the saved file: save {path} first",
            tool.fixer_label()
        )));
    }
    let (cmd, mut ctx) = build(backend, setup, &program, args.args)?;
    ctx.aliases = Input::Stdin {
        file: abs.clone(),
        text: String::new(),
    }
    .aliases();
    let timeout = setup.timeout();
    let out = capture(backend, &cmd, &args.stdin, timeout)?;
    let unchanged = || FixResult {
        tool: Some(tool),
        text: text.to_string(),
        ..FixResult::default()
    };
    match mode {
        FixMode::Stdout => match out.code {
            // phpcbf: 0 nothing to fix, 1 fixed, 2 some problems remain.
            Some(0..=2) if !out.stdout.is_empty() => {
                let fixed = out.stdout.clone();
                Ok(FixResult {
                    changed: fixed != text,
                    text: fixed,
                    ..unchanged()
                })
            }
            Some(0) => Ok(unchanged()),
            _ => Err(ForgeError::Other(failure(tool, &out, timeout))),
        },
        FixMode::Diff => {
            let report = parse_report(tool, &out.stdout, &ctx, setup.cfg.severity)
                .map_err(|_| ForgeError::Other(failure(tool, &out, timeout)))?;
            let Some(file_fix) = report.fixes.into_iter().next() else {
                return match report.errors.first() {
                    Some(e) => Err(ForgeError::Other(e.clone())),
                    None => Ok(unchanged()),
                };
            };
            let fixed = diff::apply(text, &file_fix.diff)
                .map_err(|e| ForgeError::Other(format!("{}: {e}", tool.label())))?;
            Ok(FixResult {
                changed: fixed != text,
                text: fixed,
                diff: file_fix.diff,
                rules: file_fix.rules,
                ..unchanged()
            })
        }
        FixMode::InPlace => {
            let after = backend.read_bytes(path)?;
            if after != saved {
                // The editor applies the result as an edit (one undo step).
                backend.write_bytes(path, &saved)?;
            }
            if out.code != Some(0) {
                return Err(ForgeError::Other(failure(tool, &out, timeout)));
            }
            let fixed = String::from_utf8_lossy(&after).into_owned();
            Ok(FixResult {
                changed: fixed != text,
                text: fixed,
                ..unchanged()
            })
        }
    }
}

/// One tool on the Quality Tools page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub tool: QualityTool,
    pub label: String,
    pub kind: ToolKind,
    /// "phpcbf", "PHP CS Fixer", … for fixers.
    pub fixer: Option<String>,
    pub config_files: Vec<String>,
    /// Where the executable was found (relative to the project root when inside it).
    pub path: Option<String>,
    pub fixer_path: Option<String>,
    /// The configured or found configuration file.
    pub config_file: Option<String>,
    pub config: ToolConfig,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityToolsInfo {
    pub tools: Vec<ToolInfo>,
    /// Names of the configured PHP interpreters.
    pub interpreters: Vec<String>,
}

fn relative(root: &str, p: &str) -> String {
    let p = normalize(p);
    forge_runtargets::mapping::strip_dir(&p, root)
        .filter(|r| !r.is_empty())
        .map(str::to_string)
        .unwrap_or(p)
}

fn tool_info(backend: Option<&dyn Backend>, cfg: ToolConfig) -> ToolInfo {
    let tool = cfg.tool;
    let setup = ToolSetup::direct(cfg.clone());
    let (path, fixer_path, config_file) = match backend {
        Some(b) => {
            let root = root_of(b);
            let found = |fix: bool| {
                locate(b, &setup, fix)
                    .ok()
                    .flatten()
                    .map(|p| relative(&root, &p.to_string_lossy()))
            };
            (
                found(false),
                (tool == QualityTool::Phpcs).then(|| found(true)).flatten(),
                config_file(b, &cfg).map(|c| relative(&root, &c)),
            )
        }
        None => (None, None, None),
    };
    ToolInfo {
        tool,
        label: tool.label().to_string(),
        kind: tool.kind(),
        fixer: fix_mode(tool).map(|_| tool.fixer_label().to_string()),
        config_files: tool.config_files().iter().map(|s| s.to_string()).collect(),
        path,
        fixer_path,
        config_file,
        config: cfg,
    }
}

/// The Quality Tools page: every tool's configuration, where it was found, its
/// configuration file, and the PHP interpreters to choose from.
#[tauri::command]
pub async fn quality_tools_info(state: State<'_, AppState>) -> Result<QualityToolsInfo> {
    let configs = configs(&state);
    let interpreters_list: Vec<String> = interpreters::interpreter_set(&state)
        .all()
        .map(|i| i.name.clone())
        .collect();
    let backend = state.backend().ok();
    blocking(move || {
        Ok(QualityToolsInfo {
            tools: configs
                .into_iter()
                .map(|c| tool_info(backend.as_deref(), c))
                .collect(),
            interpreters: interpreters_list,
        })
    })
    .await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Validation {
    pub ok: bool,
    /// The tool's `--version` line, or why it does not run.
    pub message: String,
    /// The command line that ran.
    pub command_line: String,
}

/// "Validate": run the tool's `--version` with the configuration shown in the settings
/// dialog (not applied yet).
#[tauri::command]
pub async fn quality_tools_validate(
    config: ToolConfig,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Validation> {
    state.require_trusted("Running PHP quality tools")?;
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<AppState>();
        let backend = state.backend()?;
        let setup = setup(&state, backend.as_ref(), config)?;
        validate(backend.as_ref(), &setup)
    })
    .await
}

fn validate(backend: &dyn Backend, setup: &ToolSetup) -> Result<Validation> {
    let tool = setup.cfg.tool;
    let Some(program) = locate(backend, setup, false)? else {
        return Ok(Validation {
            ok: false,
            message: missing(tool, false).message,
            command_line: String::new(),
        });
    };
    let (cmd, _) = build(backend, setup, &program, command::version_args())?;
    let command_line = std::iter::once(cmd.program.clone())
        .chain(cmd.args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ");
    let timeout = setup.timeout();
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

/// Check one file (on the fly after edits with `text`, or after opening / saving it).
#[tauri::command]
pub async fn quality_tools_check(
    path: String,
    text: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CheckResult> {
    state.require_trusted("Running PHP quality tools")?;
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<AppState>();
        let backend = state.backend()?;
        let (setups, failed) = split_setups(setups(&state, backend.as_ref(), &QualityTool::ALL));
        let mut result = check_file(backend.as_ref(), &setups, &path, text.as_deref())?;
        result.outcomes.extend(failed);
        Ok(result)
    })
    .await
}

/// Setups and the tools whose interpreter is missing (as failed outcomes); only tools
/// whose inspection is on count.
fn split_setups(all: Vec<Result<ToolSetup>>) -> (Vec<ToolSetup>, Vec<ToolOutcome>) {
    let mut ok = Vec::new();
    let mut failed = Vec::new();
    for s in all {
        match s {
            Ok(s) => ok.push(s),
            Err(e) => {
                let msg = e.to_string();
                if let Some(tool) = QualityTool::ALL
                    .into_iter()
                    .find(|t| msg.starts_with(&format!("{}:", t.label())))
                {
                    failed.push(ToolOutcome::new(tool, OutcomeStatus::Failed, msg));
                }
            }
        }
    }
    (ok, failed)
}

/// Code | Inspect Code with the quality tools (`paths`: files or folders; none: the
/// whole project).
#[tauri::command]
pub async fn quality_tools_inspect(
    paths: Option<Vec<String>>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CheckResult> {
    state.require_trusted("Running PHP quality tools")?;
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<AppState>();
        let backend = state.backend()?;
        let all = setups(&state, backend.as_ref(), &QualityTool::ALL);
        let inspected: Vec<Result<ToolSetup>> = all
            .into_iter()
            .filter(|s| s.as_ref().map_or(true, |s| s.cfg.inspection))
            .collect();
        let (setups, failed) = split_setups(inspected);
        let mut result = inspect(backend.as_ref(), &setups, paths.as_deref())?;
        result.outcomes.extend(failed);
        Ok(result)
    })
    .await
}

/// "Reformat with …", the quick fix "Fix with PHP CS Fixer / phpcbf" and Rector's preview:
/// the fixed editor text of `path`.
#[tauri::command]
pub async fn quality_tools_fix(
    path: String,
    tool: QualityTool,
    text: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<FixResult> {
    state.require_trusted("Running PHP quality tools")?;
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<AppState>();
        let backend = state.backend()?;
        let cfg = configs(&state)
            .into_iter()
            .find(|c| c.tool == tool)
            .unwrap_or_else(|| ToolConfig::new(tool));
        let setup = setup(&state, backend.as_ref(), cfg)?;
        fix(backend.as_ref(), &setup, &path, &text)
    })
    .await
}

/// The command line of a PHP tool of Actions on Save (Rector, PHP CS Fixer, phpcbf, Pint):
/// the arguments from its Quality Tools configuration, through its PHP interpreter.
pub(crate) fn save_action_command(
    state: &AppState,
    backend: &dyn Backend,
    tool: QualityTool,
    cmd: CommandLine,
) -> Result<CommandLine> {
    let cfg = configs(state)
        .into_iter()
        .find(|c| c.tool == tool)
        .unwrap_or_else(|| ToolConfig::new(tool));
    let setup = setup(state, backend, cfg)?;
    let file = cmd.args.last().cloned().unwrap_or_default();
    let config = config_arg(backend, &setup.cfg);
    let args = phpforge_quality_tools::in_place_args(&setup.cfg, config.as_deref(), &file);
    let (built, _) = build(backend, &setup, Path::new(&cmd.program), args.args)?;
    Ok(built)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use forge_backend::LocalBackend;
    use std::os::unix::fs::PermissionsExt;

    /// Scripts are written and run one test at a time ("Text file busy" otherwise).
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn script(dir: &Path, rel: &str, body: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn on(tool: QualityTool) -> ToolSetup {
        let mut cfg = ToolConfig::new(tool);
        cfg.inspection = true;
        cfg.timeout_secs = 20;
        ToolSetup::direct(cfg)
    }

    const PHPCS_REPORT: &str = r#"{"totals":{"errors":1},"files":{"STDIN":{"messages":[{"message":"Whitespace found at end of line","source":"Squiz.WhiteSpace.SuperfluousWhitespace.EndLine","fixable":true,"type":"ERROR","line":2,"column":9}]}}}"#;

    #[test]
    fn checks_saved_files_and_unsaved_text() {
        let _serial = serial();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_string_lossy().replace('\\', "/");
        // phpcs reads stdin only for unsaved text; phpstan reports the file it was given.
        // phpcs names the file it checked, `STDIN` for text from stdin.
        std::fs::write(dir.path().join("report.json"), PHPCS_REPORT).unwrap();
        script(
            dir.path(),
            "vendor/bin/phpcs",
            "for a; do f=$a; done\ncat > /dev/null\nif [ \"$f\" = - ]; then k=STDIN; else k=$f; fi\nsed \"s#STDIN#$k#\" report.json\nexit 2",
        );
        script(
            dir.path(),
            "vendor/bin/phpstan",
            "for a; do f=$a; done\necho 'Note: Using configuration file x'\necho \"{\\\"files\\\":{\\\"$f\\\":{\\\"messages\\\":[{\\\"message\\\":\\\"Undefined variable: \\$x\\\",\\\"line\\\":3,\\\"identifier\\\":\\\"variable.undefined\\\"}]}},\\\"errors\\\":[]}\"\nexit 1",
        );
        std::fs::create_dir_all(dir.path().join("app")).unwrap();
        std::fs::write(dir.path().join("app/A.php"), "<?php\necho $x;\n").unwrap();
        let backend = LocalBackend::open(dir.path()).unwrap();
        let setups = vec![
            on(QualityTool::Phpstan),
            on(QualityTool::Phpcs),
            on(QualityTool::Psalm),
        ];

        let saved = check_file(&backend, &setups, "app/A.php", None).unwrap();
        assert_eq!(saved.files, vec!["app/A.php"]);
        let stan = saved
            .problems
            .iter()
            .find(|p| p.tool == QualityTool::Phpstan)
            .unwrap();
        assert_eq!((stan.file.as_str(), stan.line), ("app/A.php", 2));
        let cs = saved
            .problems
            .iter()
            .find(|p| p.tool == QualityTool::Phpcs)
            .unwrap();
        assert_eq!((cs.file.as_str(), cs.line, cs.column), ("app/A.php", 1, 8));
        let psalm = saved
            .outcomes
            .iter()
            .find(|o| o.tool == QualityTool::Psalm)
            .unwrap();
        assert_eq!(psalm.status, OutcomeStatus::Missing);
        assert!(psalm.message.contains("vendor/bin/psalm"));

        // Unsaved text: Psalm cannot read it; PHPStan gets a temporary copy (its report
        // names the original file through --instead-of in real runs).
        let unsaved =
            check_file(&backend, &setups, "app/A.php", Some("<?php\necho $x; \n")).unwrap();
        assert_eq!(
            unsaved.tools,
            vec![QualityTool::Phpstan, QualityTool::Phpcs]
        );
        assert!(unsaved.problems.iter().all(|p| p.file == "app/A.php"));
        assert!(!root.is_empty());
    }

    #[test]
    fn a_tool_without_json_fails_with_its_error() {
        let _serial = serial();
        let dir = tempfile::tempdir().unwrap();
        script(
            dir.path(),
            "vendor/bin/psalm",
            "echo 'Could not locate a config XML file in path /p' >&2\nexit 1",
        );
        std::fs::write(dir.path().join("a.php"), "<?php\n").unwrap();
        let backend = LocalBackend::open(dir.path()).unwrap();
        let r = check_file(&backend, &[on(QualityTool::Psalm)], "a.php", None).unwrap();
        assert_eq!(r.outcomes[0].status, OutcomeStatus::Failed);
        assert_eq!(
            r.outcomes[0].message,
            "Could not locate a config XML file in path /p"
        );
    }

    #[test]
    fn inspect_runs_tools_with_a_configuration_on_its_paths() {
        let _serial = serial();
        let dir = tempfile::tempdir().unwrap();
        // Prints its arguments as a PHPMD report message.
        script(
            dir.path(),
            "vendor/bin/phpmd",
            "echo \"{\\\"files\\\":[{\\\"file\\\":\\\"app/A.php\\\",\\\"violations\\\":[{\\\"beginLine\\\":1,\\\"description\\\":\\\"$*\\\",\\\"rule\\\":\\\"X\\\",\\\"priority\\\":1}]}]}\"",
        );
        std::fs::write(dir.path().join("phpmd.xml"), "<ruleset/>").unwrap();
        let backend = LocalBackend::open(dir.path()).unwrap();
        let root = normalize(&dir.path().to_string_lossy());
        let r = inspect(&backend, &[on(QualityTool::Phpmd)], None).unwrap();
        assert_eq!(
            r.problems[0].message,
            format!("{root} json {root}/phpmd.xml --exclude vendor/*,node_modules/*")
        );
        assert_eq!(
            r.problems[0].severity,
            phpforge_quality_tools::Severity::Warning
        );
    }

    #[test]
    fn fixers_return_the_fixed_text_without_changing_the_file() {
        let _serial = serial();
        let dir = tempfile::tempdir().unwrap();
        // phpcbf: stdin to stdout.
        script(dir.path(), "vendor/bin/phpcbf", "sed 's/ *$//'\nexit 1");
        // Pint rewrites the file in place.
        script(
            dir.path(),
            "vendor/bin/pint",
            "for a; do f=$a; done\nsed -i 's/array()/[]/' \"$f\"",
        );
        // Rector prints a diff.
        std::fs::write(
            dir.path().join("rector.json"),
            r#"{"file_diffs":[{"file":"a.php","diff":"--- Original\n+++ New\n@@ -1,2 +1,2 @@\n <?php\n-$a = array();\n+$a = [];\n","applied_rectors":["Rector\\Php54\\Rector\\Array_\\LongArrayToShortArrayRector"]}]}"#,
        )
        .unwrap();
        script(dir.path(), "vendor/bin/rector", "cat rector.json");
        let text = "<?php\n$a = array();\n";
        std::fs::write(dir.path().join("a.php"), text).unwrap();
        let backend = LocalBackend::open(dir.path()).unwrap();

        let cbf = fix(
            &backend,
            &on(QualityTool::Phpcs),
            "a.php",
            "<?php  \n$a = 1;\n",
        )
        .unwrap();
        assert_eq!(cbf.text, "<?php\n$a = 1;\n");
        assert!(cbf.changed);

        let pint = fix(&backend, &on(QualityTool::Pint), "a.php", text).unwrap();
        assert_eq!(pint.text, "<?php\n$a = [];\n");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.php")).unwrap(),
            text
        );

        let rector = fix(&backend, &on(QualityTool::Rector), "a.php", text).unwrap();
        assert_eq!(rector.text, "<?php\n$a = [];\n");
        assert_eq!(
            rector.rules,
            vec!["Rector\\Php54\\Rector\\Array_\\LongArrayToShortArrayRector"]
        );
        assert!(rector.diff.starts_with("--- Original"));

        // Rector works on the saved file.
        let err = fix(&backend, &on(QualityTool::Rector), "a.php", "<?php\n").unwrap_err();
        assert!(err.to_string().contains("save a.php first"));
        assert!(fix(&backend, &on(QualityTool::Phpstan), "a.php", text).is_err());
    }

    #[test]
    fn docker_interpreters_get_mapped_paths_both_ways() {
        let _serial = serial();
        let dir = tempfile::tempdir().unwrap();
        script(dir.path(), "vendor/bin/phpstan", "exit 0");
        let backend = LocalBackend::open(dir.path()).unwrap();
        let root = normalize(&dir.path().to_string_lossy());
        let interp = phpforge_interpreters::PhpInterpreter {
            name: "Docker PHP 8.3".into(),
            kind: InterpreterKind::Docker,
            image: "php:8.3-cli".into(),
            ..Default::default()
        };
        let prepared = Prepared {
            ctx: phpforge_interpreters::WrapContext {
                root: root.clone(),
                mappings: vec![phpforge_interpreters::PathMapping {
                    local: root.clone(),
                    remote: "/opt/project".into(),
                }],
                ..Default::default()
            },
            interp,
        };
        let setup = ToolSetup {
            cfg: ToolConfig::new(QualityTool::Phpstan),
            interp: Some(prepared),
        };
        let program = locate(&backend, &setup, false).unwrap().unwrap();
        let args = check_args(
            &setup.cfg,
            None,
            &Input::Files(vec![format!("{root}/app/A.php")]),
        )
        .args;
        let (cmd, ctx) = build(&backend, &setup, &program, args).unwrap();
        assert_eq!(cmd.program, "docker");
        assert!(
            cmd.args
                .contains(&"/opt/project/vendor/bin/phpstan".to_string()),
            "{:?}",
            cmd.args
        );
        assert!(cmd.args.contains(&"/opt/project/app/A.php".to_string()));
        assert_eq!(ctx.cwd, "/opt/project");
        assert_eq!(ctx.localize("/opt/project/app/A.php"), "app/A.php");
        assert!(!setup.runs_locally());
    }

    #[test]
    fn validate_reports_the_version() {
        let _serial = serial();
        let dir = tempfile::tempdir().unwrap();
        script(
            dir.path(),
            "vendor/bin/phpstan",
            "echo 'PHPStan - PHP Static Analysis Tool 1.12.7'",
        );
        let backend = LocalBackend::open(dir.path()).unwrap();
        let v = validate(&backend, &on(QualityTool::Phpstan)).unwrap();
        assert!(v.ok);
        assert_eq!(v.message, "PHPStan - PHP Static Analysis Tool 1.12.7");
        let missing = validate(&backend, &on(QualityTool::Pint)).unwrap();
        assert!(!missing.ok && missing.message.contains("vendor/bin/pint"));
    }
}
