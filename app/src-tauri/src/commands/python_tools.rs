//! PyForge's run tools (PyCharm's tests, debugger, console and profiler); the logic lives
//! in `packs/pyforge/testing`:
//!
//! - Runs: `run::build` hands `python`, `pytest`, `unittest` and `tox` configurations to
//!   [`build`]: the command through the configuration's or the project's interpreter
//!   (local, Docker, Docker Compose, SSH, WSL) or its run target, a debugger's arguments
//!   after the interpreter options, report folders, Run with Coverage (pytest-cov,
//!   Cobertura) and Profile (cProfile into Forge's cache folder).
//! - Tests: discovery with the project's runner (`python.testing.runner`, detection),
//!   Rerun Failed Tests with the classes read from the files, tox environments.
//! - Debugging: whether debugpy is installed, and where a debug run listens
//!   (`python_debug_plan`); the UI starts the run with `-m debugpy --listen` and attaches.
//! - Python Console: a REPL process per console tab (`forge_console.py`), its output
//!   rendered by `forge-output`, the helper's replies (prompts, variables) separated.
//! - Profiler: `.pstats` snapshots in `<cache dir>/forge/profiles/<project>`, read into hot
//!   spots and a call tree.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_backend::{Backend, Process};
use forge_core::{project_state, ForgeError, Result};
use forge_process::output::{OutputPathMapping, RunInput, RunOutput, RunOutputOptions};
use forge_process::{CommandLine, OutputStream, RunConfiguration, RunEvent, RunSink};
use forge_testing::{CoverageFormat, FailedTest, TestFile};
use pyforge_testing::command::{self, relative_to, Context, PytestReporter};
use pyforge_testing::console::{Piece, Splitter};
use pyforge_testing::detect::{self, ProjectTesting};
use pyforge_testing::pstats::{self, ProfileView, TreeLimits};
use pyforge_testing::tox::{self, ToxEnv};
use pyforge_testing::{config, discover};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::python::{self, python_command, run_captured, setting_bool, setting_string};
use crate::commands::run::Built;
use crate::commands::test_frameworks::{self, ReportDir, TestSetup};
use crate::commands::testing::{self, CoverageSetup};
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Configuration types built here.
pub(crate) fn is_python_type(type_id: &str) -> bool {
    type_id == "python"
        || config::TYPES.contains(&type_id)
        || pyforge_frameworks::run::handles(type_id)
}

/// What a run request adds to a configuration.
pub(crate) struct Extras {
    pub run_id: String,
    pub env: std::collections::BTreeMap<String, String>,
    /// A debugger's arguments (`-m debugpy --listen …`), placed after the interpreter
    /// options.
    pub head: Vec<String>,
    pub debug_port: Option<forge_runtargets::DebugPort>,
    pub coverage: bool,
    pub profile: bool,
    pub output_filters: Vec<String>,
}

/// Where a Profile run's snapshot is read after it exited.
#[derive(Debug, Clone)]
pub(crate) struct ProfileSetup {
    /// A project folder (remote interpreters, run targets) the snapshot is moved out of …
    pub dir: Option<ReportDir>,
    /// … and its name there, or (without `dir`) the snapshot's final local path.
    pub file: String,
    pub name: String,
    /// Remote → local paths of the run, for the snapshot's source files.
    pub mappings: Vec<OutputPathMapping>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

fn root_string(root: &Path) -> String {
    root.to_string_lossy().replace('\\', "/")
}

/// `<cache dir>/forge/profiles/<project>`.
fn profiles_dir(root: &str) -> Result<PathBuf> {
    dirs::cache_dir()
        .map(|d| {
            d.join("forge")
                .join("profiles")
                .join(project_state::project_key(root))
        })
        .ok_or_else(|| ForgeError::Other("no cache directory".into()))
}

fn safe_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    s.chars().take(60).collect()
}

/// The command of a Python configuration, as `run::build` returns it.
pub(crate) fn build(state: &AppState, cfg: &RunConfiguration, x: Extras) -> Result<Built> {
    // Django Server, Flask, FastAPI and Celery run as Python configurations; Django tests
    // take the project's manage.py and settings.
    let mut server_port: Option<u16> = None;
    let prepared;
    let cfg = if pyforge_frameworks::run::handles(&cfg.type_id) {
        let (c, port) = crate::commands::python_web::to_python(state, cfg)?;
        server_port = port;
        prepared = c;
        &prepared
    } else if cfg.type_id == config::DJANGO {
        let mut c = cfg.clone();
        crate::commands::python_web::fill_django_tests(state, &mut c)?;
        prepared = c;
        &prepared
    } else {
        cfg
    };
    let backend = state.backend()?;
    let root = backend.info().root.clone();
    let run_on = cfg.run_on.clone().filter(|t| !t.is_local());
    let interp = python::resolve(state, &cfg.interpreter).map(|(i, _)| i);
    let local = backend.info().remote.is_none()
        && run_on.is_none()
        && interp.as_ref().is_none_or(|i| !i.kind.is_remote());
    let cwd = python::working_dir(&root, &cfg.working_directory);
    let cwd_text = root_string(&cwd);
    let mut tests = TestSetup::default();
    let mut program: Option<String> = None;
    let mut cmd = if cfg.type_id == "python" {
        let mut head = x.head.clone();
        if x.profile {
            let name = cfg.name.clone();
            let stamp = now_ms();
            let setup = if local {
                let dir = profiles_dir(&root_string(&root))?;
                std::fs::create_dir_all(&dir).map_err(|e| ForgeError::io(&dir, e))?;
                let file = dir.join(format!("{stamp}-{}.pstats", safe_name(&name)));
                ProfileSetup {
                    dir: None,
                    file: file.to_string_lossy().into_owned(),
                    name,
                    mappings: Vec::new(),
                }
            } else {
                let (arg, dir) = test_frameworks::report_dir(false, &root, &x.run_id, "profile");
                head.extend(command::profile_args(
                    &relative_to(&cwd_text, &format!("{arg}/profile.pstats")),
                    "",
                ));
                ProfileSetup {
                    dir: Some(dir),
                    file: "profile.pstats".into(),
                    name,
                    mappings: Vec::new(),
                }
            };
            if setup.dir.is_none() {
                head.extend(command::profile_args(&setup.file, ""));
            }
            tests.profile = Some(setup);
        }
        python::run_command(state, cfg, &head)?.0
    } else {
        let rt = test_frameworks::to_runtools(cfg)?;
        let (report_arg, report_dir) = test_frameworks::report_dir(local, &root, &x.run_id, "");
        let (coverage_arg, coverage_dir) =
            test_frameworks::report_dir(local, &root, &x.run_id, "coverage");
        if x.coverage && cfg.type_id != config::PYTEST {
            return Err(ForgeError::Other(format!(
                "'{}' configurations cannot run with coverage",
                cfg.type_id
            )));
        }
        let ctx = Context {
            cwd: cwd_text.clone(),
            report_dir: report_arg,
            coverage_file: x.coverage.then(|| format!("{coverage_arg}/coverage.xml")),
            reporter: PytestReporter::from_setting(&setting_string(
                state,
                "python.pytest.reporter",
            )),
            tox_program: setting_string(state, "python.tox.path"),
            debug_args: x.head.clone(),
        };
        let pc = command::build(&rt, &ctx).map_err(ForgeError::Other)?;
        program = pc.program.clone();
        let mut cmd = CommandLine::new(pc.program.as_deref().unwrap_or("python")).args(pc.args);
        cmd.cwd = cwd.clone();
        cmd.env.extend(pc.env);
        cmd.env.extend(cfg.env.clone());
        tests = TestSetup {
            report: pc.report,
            flavor: pc.flavor,
            report_dir: pc.report_files.then_some(report_dir),
            configuration: Some(cfg.clone()),
            location_rules: Vec::new(),
            default_file: pc.default_file,
            coverage: pc.coverage_file.map(|_| CoverageSetup {
                dir: Some(coverage_dir),
                file: "coverage.xml".into(),
                collect: None,
                format: CoverageFormat::Cobertura,
                name: cfg.name.clone(),
                location_rules: Vec::new(),
            }),
            profile: None,
            native: None,
        };
        cmd
    };
    cmd.env.extend(x.env);
    let mut publish: Vec<u16> = x
        .debug_port
        .as_ref()
        .filter(|d| d.direction == forge_runtargets::DebugDirection::IdeConnects && d.port > 0)
        .map(|d| vec![d.port])
        .unwrap_or_default();
    // A web server in a container or on a remote host is published like the debugger.
    publish.extend(server_port);
    if let Some(target) = run_on {
        if program.is_none() {
            cmd.program = "python3".into();
        }
        let mut built = crate::commands::run::on_target(
            state,
            &target,
            cmd,
            None,
            Vec::new(),
            x.debug_port,
            x.output_filters,
            tests,
        )?;
        if let Some(p) = built.tests.profile.as_mut() {
            p.mappings = built.path_mappings.clone();
        }
        return Ok(built);
    }
    if program.is_some() {
        // tox from `python.tox.path` runs on this machine.
        return Ok(Built {
            cmd,
            path_mappings: Vec::new(),
            debug_server: None,
            output_filters: x.output_filters,
            tests,
        });
    }
    let interp = interp.ok_or_else(python::no_interpreter)?;
    let (cmd, path_mappings) =
        python::through_serving(state, backend.as_ref(), &interp, None, cmd, publish)?;
    if let Some(p) = tests.profile.as_mut() {
        p.mappings = path_mappings.clone();
    }
    Ok(Built {
        cmd,
        path_mappings,
        debug_server: None,
        output_filters: x.output_filters,
        tests,
    })
}

// ---- Tests -------------------------------------------------------------------------------

/// Project detection, re-read at most every few seconds (gutter discovery asks often).
#[derive(Default)]
pub struct PythonToolsState {
    detected: Mutex<Option<(String, Instant, ProjectTesting)>>,
}

fn project_testing(state: &AppState, tools: &PythonToolsState) -> ProjectTesting {
    let Ok(backend) = state.backend() else {
        return ProjectTesting::default();
    };
    let root = root_string(&backend.info().root);
    let mut guard = tools.detected.lock().expect("python tools lock");
    if let Some((r, at, d)) = guard.as_ref() {
        if *r == root && at.elapsed() < Duration::from_secs(10) {
            return d.clone();
        }
    }
    let d = detect::detect(|f| backend.read_optional(f).ok().flatten());
    *guard = Some((root, Instant::now(), d.clone()));
    d
}

/// The tests of a Python file with the project's runner (`test_discover`).
pub(crate) fn discover_file(
    state: &AppState,
    tools: &PythonToolsState,
    path: &str,
    text: &str,
) -> TestFile {
    let scan = discover::scan(path, text);
    if scan.items.is_empty() {
        return TestFile::default();
    }
    let project = project_testing(state, tools);
    let runner = detect::choose_runner(
        &setting_string(state, "python.testing.runner"),
        &project,
        &scan,
    );
    discover::test_file(path, &scan, runner)
}

/// Rerun Failed Tests of pytest and unittest configurations (`test_rerun_failed`).
pub(crate) fn rerun_failed(
    state: &AppState,
    cfg: &RunConfiguration,
    failed: &[FailedTest],
) -> Result<Option<RunConfiguration>> {
    let backend = state.backend()?;
    let rt = test_frameworks::to_runtools(cfg)?;
    match pyforge_testing::rerun::rerun_failed(&rt, failed, |p| {
        backend.read_optional(p).ok().flatten()
    }) {
        Some(c) => test_frameworks::from_runtools(c, cfg).map(Some),
        None => Ok(None),
    }
}

/// pytest and tox in the project (from its files).
#[tauri::command]
pub async fn python_testing_detect(
    state: State<'_, AppState>,
    tools: State<'_, PythonToolsState>,
) -> Result<ProjectTesting> {
    *tools.detected.lock().expect("python tools lock") = None;
    Ok(project_testing(&state, &tools))
}

/// The project's tox environments.
#[tauri::command]
pub async fn python_tox_environments(state: State<'_, AppState>) -> Result<Vec<ToxEnv>> {
    let backend = state.backend()?;
    blocking(move || {
        Ok(tox::environments(|f| {
            backend.read_optional(f).ok().flatten()
        }))
    })
    .await
}

// ---- Debugging ---------------------------------------------------------------------------

/// debugpy in an interpreter.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugpyStatus {
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// `Python 3.12 (shop-api)`.
    pub interpreter: String,
}

/// Whether debugpy is installed in the configuration's (or the project's) interpreter.
#[tauri::command]
pub async fn python_debugpy_status(
    interpreter: Option<String>,
    state: State<'_, AppState>,
) -> Result<DebugpyStatus> {
    let (interp, _) = python::resolve(&state, interpreter.as_deref().unwrap_or(""))
        .ok_or_else(python::no_interpreter)?;
    let backend = state.backend()?;
    let cmd = python_command(
        &state,
        backend.as_ref(),
        &interp,
        &["-c", "import debugpy; print(debugpy.__version__)"],
    )?;
    let label = interp.label();
    blocking(move || {
        let out = run_captured(backend.as_ref(), &cmd, Duration::from_secs(60));
        Ok(DebugpyStatus {
            installed: out.is_ok(),
            version: out
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            interpreter: label,
        })
    })
    .await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugMapping {
    pub local_root: String,
    pub remote_root: String,
}

/// Where a debug run listens and how Forge reaches it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugPlan {
    /// `--listen <listenHost>:<port>`.
    pub listen_host: String,
    /// Forge connects to `<connectHost>:<port>`.
    pub connect_host: String,
    pub port: u16,
    /// Remote → local folders for breakpoints and frames (debugpy's `pathMappings`).
    pub path_mappings: Vec<DebugMapping>,
    pub just_my_code: bool,
    pub sub_process: bool,
}

fn absolute(root: &str, local: &str) -> String {
    if local.is_empty() {
        root.to_string()
    } else if local.starts_with('/') || local.contains(":/") || local.contains(":\\") {
        local.to_string()
    } else {
        format!("{root}/{local}")
    }
}

/// How a debug run of `configuration` listens: local interpreters on 127.0.0.1;
/// containers, SSH, WSL and run targets on every interface, with the port published or
/// forwarded and the project's path mappings.
#[tauri::command]
pub async fn python_debug_plan(
    configuration: RunConfiguration,
    state: State<'_, AppState>,
) -> Result<DebugPlan> {
    let port =
        forge_ports::free_port().map_err(|e| ForgeError::Other(format!("no free port: {e}")))?;
    let backend = state.backend()?;
    let root = root_string(&backend.info().root);
    let just_my_code = setting_bool(&state, "python.debugger.justMyCode", true);
    let sub_process = setting_bool(&state, "python.debugger.subProcess", true);
    if let Some(target) = configuration.run_on.clone().filter(|t| !t.is_local()) {
        let docker = testing::docker_cli(&state);
        let ctx = testing::target_context(backend.as_ref(), &target, &docker);
        let dt = forge_runtargets::debug_target(&target, &ctx.mappings);
        return Ok(DebugPlan {
            listen_host: "0.0.0.0".into(),
            connect_host: dt.target_host,
            port,
            path_mappings: dt
                .mappings
                .iter()
                .map(|m| DebugMapping {
                    local_root: absolute(&root, &m.local),
                    remote_root: m.remote.clone(),
                })
                .collect(),
            just_my_code,
            sub_process,
        });
    }
    let (interp, _) =
        python::resolve(&state, &configuration.interpreter).ok_or_else(python::no_interpreter)?;
    if !interp.kind.is_remote() {
        return Ok(DebugPlan {
            listen_host: "127.0.0.1".into(),
            connect_host: "127.0.0.1".into(),
            port,
            path_mappings: Vec::new(),
            just_my_code,
            sub_process,
        });
    }
    let prepared =
        crate::commands::php_interpreters::prepare(&state, backend.as_ref(), interp.wrap_target());
    Ok(DebugPlan {
        listen_host: "0.0.0.0".into(),
        connect_host: "127.0.0.1".into(),
        port,
        path_mappings: prepared
            .output_mappings()
            .into_iter()
            .map(|m| DebugMapping {
                local_root: m.local,
                remote_root: m.remote,
            })
            .collect(),
        just_my_code,
        sub_process,
    })
}

/// The interpreter that runs debugpy's adapter for Attach to Process: the project's,
/// when it is a local one.
pub(crate) fn adapter_python(state: &AppState) -> Option<String> {
    let (interp, _) = python::resolve(state, "")?;
    (!interp.kind.is_remote()).then(|| interp.python().to_string())
}

// ---- Python Console ----------------------------------------------------------------------

struct ConsoleSession {
    process: Arc<dyn Process>,
    output: Arc<Mutex<RunOutput>>,
}

#[derive(Default)]
pub struct PythonConsoleState {
    sessions: Mutex<HashMap<String, ConsoleSession>>,
    next: AtomicU64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleStarted {
    pub id: String,
    pub command_line: String,
    /// `Python 3.12 (shop-api)`.
    pub interpreter: String,
}

fn emit<T: Serialize>(app: &AppHandle, event: &str, payload: &T) {
    if let Err(err) = app.emit(event, payload) {
        tracing::warn!("cannot emit {event}: {err}");
    }
}

/// Start a Python Console in the project's interpreter. Output arrives on
/// `python-console://<id>` as `{ type: "console", …ConsoleDelta }`, the helper's replies as
/// `{ type: "message", message }` and the end as `{ type: "exited", code }`.
#[tauri::command]
pub async fn python_console_start(
    id: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
    consoles: State<'_, PythonConsoleState>,
) -> Result<ConsoleStarted> {
    state.require_trusted("Running the Python Console")?;
    let (interp, _) = python::resolve(&state, "").ok_or_else(python::no_interpreter)?;
    let backend = state.backend()?;
    let root = backend.info().root.clone();
    let args = command::console(
        &setting_string(&state, "python.console.interpreterOptions"),
        setting_bool(&state, "python.console.ipython", true),
    );
    let mut cmd = CommandLine::new("python").args(args);
    cmd.cwd = root.clone();
    cmd.env.insert("PYTHONIOENCODING".into(), "utf-8".into());
    cmd.env.insert("PYTHONUNBUFFERED".into(), "1".into());
    let (cmd, path_mappings) =
        python::through_serving(&state, backend.as_ref(), &interp, None, cmd, Vec::new())?;
    let id = id.unwrap_or_else(|| {
        format!(
            "console-{}",
            consoles.next.fetch_add(1, Ordering::SeqCst) + 1
        )
    });
    let output = Arc::new(Mutex::new(RunOutput::new(&RunOutputOptions {
        root: Some(root_string(&root)),
        cwd: Some(root_string(&root)),
        cycle_buffer_kb: Some(1024),
        path_mappings,
        ..RunOutputOptions::default()
    })));
    let event = format!("python-console://{id}");
    let sink_output = output.clone();
    let splitter = Arc::new(Mutex::new(Splitter::new()));
    let sink_app = app.clone();
    let sink: RunSink =
        Arc::new(move |e: RunEvent| match e {
            RunEvent::Started { .. } => {}
            RunEvent::Output { stream, text } => {
                let stderr = matches!(stream, OutputStream::Stderr);
                let pieces = if stderr {
                    vec![Piece::Text { text }]
                } else {
                    splitter.lock().expect("console splitter").feed(&text)
                };
                for p in pieces {
                    match p {
                        Piece::Text { text } => {
                            let rendered = sink_output.lock().expect("console output").handle(
                                &RunInput::Output {
                                    stream: if stderr { "stderr" } else { "stdout" }.into(),
                                    text,
                                },
                            );
                            for r in &rendered {
                                emit(&sink_app, &event, r);
                            }
                        }
                        Piece::Message { message } => emit(
                            &sink_app,
                            &event,
                            &serde_json::json!({ "type": "message", "message": message }),
                        ),
                    }
                }
            }
            RunEvent::Exited { code, .. } => emit(
                &sink_app,
                &event,
                &serde_json::json!({ "type": "exited", "code": code }),
            ),
        });
    let start_cmd = cmd.clone();
    let start_backend = backend.clone();
    let process = blocking(move || start_backend.start_process(&start_cmd, sink)).await?;
    let old = consoles
        .sessions
        .lock()
        .expect("consoles")
        .insert(id.clone(), ConsoleSession { process, output });
    if let Some(old) = old {
        old.process.kill(Duration::from_millis(500));
    }
    Ok(ConsoleStarted {
        id,
        command_line: cmd.display(),
        interpreter: interp.label(),
    })
}

/// Send a request to a console (`exec`, `runfile`, `vars`, `children`, `input`); `echo`
/// is shown as typed input first.
#[tauri::command]
pub fn python_console_send(
    id: String,
    op: String,
    fields: Option<Value>,
    echo: Option<String>,
    app: AppHandle,
    consoles: State<'_, PythonConsoleState>,
) -> Result<()> {
    let sessions = consoles.sessions.lock().expect("consoles");
    let s = sessions
        .get(&id)
        .ok_or_else(|| ForgeError::Other(format!("no console {id}")))?;
    if let Some(text) = echo {
        let rendered = s
            .output
            .lock()
            .expect("console output")
            .handle(&RunInput::Input { text });
        for r in &rendered {
            emit(&app, &format!("python-console://{id}"), r);
        }
    }
    let line = pyforge_testing::console::request(&op, fields.unwrap_or(Value::Null));
    s.process.write_stdin(line.as_bytes())
}

#[tauri::command]
pub async fn python_console_stop(
    id: String,
    consoles: State<'_, PythonConsoleState>,
) -> Result<()> {
    let removed = consoles.sessions.lock().expect("consoles").remove(&id);
    if let Some(s) = removed {
        blocking(move || {
            s.process.kill(Duration::from_secs(1));
            Ok(())
        })
        .await?;
    }
    Ok(())
}

const CONSOLE_HISTORY: &str = "python-console-history";
const HISTORY_LIMIT: usize = 200;

fn history_dir() -> Result<PathBuf> {
    project_state::default_list_dir(CONSOLE_HISTORY)
        .ok_or_else(|| ForgeError::Other("no state directory".into()))
}

/// The console's command history of the project, oldest first.
#[tauri::command]
pub async fn python_console_history(state: State<'_, AppState>) -> Result<Vec<String>> {
    let root = root_string(&state.project_root()?);
    blocking(move || Ok(project_state::load_list(&history_dir()?, &root))).await
}

#[tauri::command]
pub async fn python_console_history_add(
    entry: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>> {
    let root = root_string(&state.project_root()?);
    blocking(move || {
        let dir = history_dir()?;
        let mut list: Vec<String> = project_state::load_list(&dir, &root);
        list.retain(|e| *e != entry);
        if !entry.trim().is_empty() {
            list.push(entry);
        }
        let skip = list.len().saturating_sub(HISTORY_LIMIT);
        let list: Vec<String> = list.into_iter().skip(skip).collect();
        project_state::save_list(&dir, &root, &list)?;
        Ok(list)
    })
    .await
}

// ---- Profiler ----------------------------------------------------------------------------

/// A profiler snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInfo {
    pub id: String,
    /// The configuration that was profiled.
    pub name: String,
    /// Milliseconds since the epoch.
    pub at: u64,
    pub path: String,
    #[serde(default)]
    pub mappings: Vec<OutputPathMapping>,
}

fn meta_path(pstats: &Path) -> PathBuf {
    pstats.with_extension("json")
}

fn list_profiles(dir: &Path) -> Vec<ProfileInfo> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<ProfileInfo> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "pstats"))
        .map(|p| {
            let id = p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let meta: Option<ProfileInfo> = std::fs::read_to_string(meta_path(&p))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok());
            let at = id
                .split('-')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or_default();
            ProfileInfo {
                name: meta
                    .as_ref()
                    .map(|m| m.name.clone())
                    .unwrap_or_else(|| id.clone()),
                at: meta.as_ref().map_or(at, |m| m.at),
                mappings: meta.map(|m| m.mappings).unwrap_or_default(),
                path: p.to_string_lossy().into_owned(),
                id,
            }
        })
        .collect();
    out.sort_by_key(|p| std::cmp::Reverse(p.at));
    out
}

/// Keep the newest `keep` snapshots.
fn prune(dir: &Path, keep: usize) {
    for old in list_profiles(dir).into_iter().skip(keep.max(1)) {
        let p = PathBuf::from(&old.path);
        let _ = std::fs::remove_file(meta_path(&p));
        let _ = std::fs::remove_file(p);
    }
}

fn store_profile(state: &AppState, root: &str, info: ProfileInfo) -> Result<ProfileInfo> {
    let path = PathBuf::from(&info.path);
    let text = serde_json::to_string(&info).map_err(|e| ForgeError::Other(e.to_string()))?;
    std::fs::write(meta_path(&path), text).map_err(|e| ForgeError::io(&path, e))?;
    let keep = state
        .read_settings()
        .get_as::<usize>("python.profiler.snapshots")
        .unwrap_or(20);
    if let Ok(dir) = profiles_dir(root) {
        prune(&dir, keep);
    }
    Ok(info)
}

/// Read a finished Profile run's snapshot into the cache folder.
pub(crate) fn record_profile(
    app: &AppHandle,
    backend: &dyn Backend,
    setup: &ProfileSetup,
    root: &str,
) -> std::result::Result<ProfileInfo, String> {
    let state = app.state::<AppState>();
    let at = now_ms();
    let target = match &setup.dir {
        None => PathBuf::from(&setup.file),
        Some(dir) => {
            let bytes = match dir {
                ReportDir::Local(d) => std::fs::read(d.join(&setup.file)).ok(),
                ReportDir::Project(rel) => {
                    backend.read_bytes(&format!("{rel}/{}", setup.file)).ok()
                }
                // Build tool report folders hold no profiles.
                ReportDir::Builds(_) => None,
            };
            dir.remove(backend);
            let bytes = bytes.ok_or("the program wrote no profile")?;
            let folder = profiles_dir(root).map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            let path = folder.join(format!("{at}-{}.pstats", safe_name(&setup.name)));
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
            path
        }
    };
    let bytes = std::fs::read(&target).map_err(|_| "the program wrote no profile".to_string())?;
    pstats::parse(&bytes)?;
    let id = target
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    store_profile(
        &state,
        root,
        ProfileInfo {
            id,
            name: setup.name.clone(),
            at,
            path: target.to_string_lossy().into_owned(),
            mappings: setup.mappings.clone(),
        },
    )
    .map_err(|e| e.to_string())
}

/// The project's profiler snapshots, newest first.
#[tauri::command]
pub async fn python_profiles(state: State<'_, AppState>) -> Result<Vec<ProfileInfo>> {
    let root = root_string(&state.project_root()?);
    blocking(move || Ok(list_profiles(&profiles_dir(&root)?))).await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSnapshot {
    pub info: ProfileInfo,
    pub view: ProfileView,
}

/// A snapshot's hot spots and call tree, with project-relative files.
#[tauri::command]
pub async fn python_profile_view(
    id: String,
    state: State<'_, AppState>,
) -> Result<ProfileSnapshot> {
    let root = root_string(&state.project_root()?);
    blocking(move || {
        let info = list_profiles(&profiles_dir(&root)?)
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| ForgeError::Other(format!("no profile {id}")))?;
        let bytes =
            std::fs::read(&info.path).map_err(|e| ForgeError::io(Path::new(&info.path), e))?;
        let mut profile = pstats::parse(&bytes).map_err(ForgeError::Other)?;
        let mappings = info.mappings.clone();
        let root_prefix = format!("{root}/");
        profile.map_files(|file| {
            let f = file.replace('\\', "/");
            let local = mappings
                .iter()
                .find_map(|m| {
                    let remote = m.remote.trim_end_matches('/');
                    f.strip_prefix(&format!("{remote}/"))
                        .map(|rest| format!("{}/{rest}", m.local.trim_end_matches('/')))
                })
                .unwrap_or(f);
            Some(
                local
                    .strip_prefix(&root_prefix)
                    .map(str::to_string)
                    .unwrap_or(local),
            )
        });
        Ok(ProfileSnapshot {
            view: profile.view(300, TreeLimits::default()),
            info,
        })
    })
    .await
}

/// Open a `.pstats` file of the project as a snapshot.
#[tauri::command]
pub async fn python_profile_open(path: String, state: State<'_, AppState>) -> Result<ProfileInfo> {
    let backend = state.backend()?;
    let root = root_string(&backend.info().root);
    let bytes = {
        let b = backend.clone();
        let p = path.clone();
        blocking(move || b.read_bytes(&p)).await?
    };
    pstats::parse(&bytes).map_err(ForgeError::Other)?;
    let at = now_ms();
    let name = path.rsplit('/').next().unwrap_or(&path).to_string();
    let dir = profiles_dir(&root)?;
    std::fs::create_dir_all(&dir).map_err(|e| ForgeError::io(&dir, e))?;
    let target = dir.join(format!("{at}-{}.pstats", safe_name(&name)));
    std::fs::write(&target, bytes).map_err(|e| ForgeError::io(&target, e))?;
    store_profile(
        &state,
        &root,
        ProfileInfo {
            id: target
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            name,
            at,
            path: target.to_string_lossy().into_owned(),
            mappings: Vec::new(),
        },
    )
}

#[tauri::command]
pub async fn python_profile_delete(id: String, state: State<'_, AppState>) -> Result<()> {
    let root = root_string(&state.project_root()?);
    blocking(move || {
        if let Some(p) = list_profiles(&profiles_dir(&root)?)
            .into_iter()
            .find(|p| p.id == id)
        {
            let path = PathBuf::from(&p.path);
            let _ = std::fs::remove_file(meta_path(&path));
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_are_listed_newest_first_and_pruned() {
        let dir = std::env::temp_dir().join(format!("forge-profiles-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (at, name) in [(1000u64, "a"), (3000, "c"), (2000, "b")] {
            let p = dir.join(format!("{at}-{name}.pstats"));
            std::fs::write(&p, b"{0").unwrap();
            let info = ProfileInfo {
                id: format!("{at}-{name}"),
                name: format!("run {name}"),
                at,
                path: p.to_string_lossy().into_owned(),
                mappings: Vec::new(),
            };
            std::fs::write(meta_path(&p), serde_json::to_string(&info).unwrap()).unwrap();
        }
        let list = list_profiles(&dir);
        assert_eq!(
            list.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["run c", "run b", "run a"]
        );
        prune(&dir, 2);
        assert_eq!(list_profiles(&dir).len(), 2);
        assert!(!dir.join("1000-a.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(safe_name("pytest for a.b"), "pytest_for_a_b");
        assert_eq!(absolute("/p", ""), "/p");
        assert_eq!(absolute("/p", "src"), "/p/src");
        assert_eq!(absolute("/p", "/opt/x"), "/opt/x");
    }
}
