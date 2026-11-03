//! Run/Debug configurations and the processes of the Run tool window.
//!
//! Configurations are stored in `<project>/.forge/runConfigurations.toml` (shared) and
//! `.forge/runConfigurations.local.toml`. A started run streams `RunEvent`s on
//! `run://<runId>`: the raw `started`, `output` (stdout/stderr) and `exited` events, plus what
//! the UI renders, computed by `forge_process::output`: `console` (styled console lines with
//! file links) and, for test runners, `tests` (test tree node updates).

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_backend::Process;
use forge_core::{ForgeError, Result};
use forge_process::output::{OutputPathMapping, RunInput, RunOutput, RunOutputOptions};
use forge_process::{
    BuildContext, CommandLine, KindRegistry, OutputStream, RunConfiguration, RunConfigurations,
    RunEvent, RunSink, TemplateKind,
};
use phpforge_interpreters::{PhpInterpreter, Program};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::php_interpreters as interpreters;
use crate::commands::test_frameworks::{self, TestSetup};
use crate::commands::testing::{self, CoverageSetup};
use crate::commands::workspace::blocking;
use crate::state::AppState;

struct Run {
    process: Arc<dyn Process>,
    output: Arc<Mutex<RunOutput>>,
}

/// A run's process and its rendered output.
pub(crate) type RunHandles = (Arc<dyn Process>, Arc<Mutex<RunOutput>>);

pub struct RunState {
    runs: Mutex<HashMap<String, Run>>,
    /// Observers of runs about to start (the Spring Boot run dashboard), by run id.
    pub observers: Mutex<HashMap<String, crate::commands::spring::RunObserver>>,
    next_id: AtomicU64,
    /// Configuration kinds known to the backend; plugins register more at startup.
    pub kinds: KindRegistry,
    /// Settings that hold default interpreters of manifest kinds (`lua.interpreter`).
    interpreter_settings: Vec<String>,
    /// Test frameworks of every pack: the PHP frameworks and the manifests' ones.
    pub tests: forge_testing::TestFrameworks,
    /// Settings with the executables of manifest test frameworks (`programSetting`).
    program_settings: Vec<String>,
}

impl RunState {
    /// Built-in kinds plus the manifest-declared ones (types with a `command` template).
    pub fn new(app: &AppState) -> Self {
        let mut kinds = KindRegistry::builtin();
        let mut interpreter_settings = Vec::new();
        kinds.register(Box::new(super::scratches::ScratchKind));
        let plugins = app.plugins.read().expect("plugins lock poisoned");
        let mut tests = forge_testing::TestFrameworks::new();
        tests.register(Box::new(phpforge_test_frameworks::PhpTestFrameworks));
        tests.register(Box::new(pyforge_testing::PythonTestFrameworks));
        tests.register_specs(plugins.test_frameworks());
        // Node.js test runners: their manifest specs with WebForge's Node.js specifics.
        webforge_js_testing::register(&mut tests, plugins.test_frameworks().cloned());
        // Go Test: the manifest spec with GoForge's discovery and command lines.
        goforge_lang_go::provider::register(&mut tests, plugins.test_frameworks().cloned());
        // Cargo Test: the manifest spec with RustForge's discovery and command lines.
        rustforge_lang_rust::provider::register(&mut tests, plugins.test_frameworks().cloned());
        // RSpec and Minitest: the manifest specs with RubyForge's discovery and commands.
        rubyforge_lang_ruby::provider::register(&mut tests, plugins.test_frameworks().cloned());
        // .NET Tests: the manifest spec with DotNetForge's discovery, filters and TRX reports.
        dotnetforge_lang_dotnet::provider::register(&mut tests, plugins.test_frameworks().cloned());
        // JavaForge: JUnit and TestNG replace their manifest declarations (same ids).
        tests.register(Box::new(javaforge_lang_java::JavaTests::junit()));
        tests.register(Box::new(javaforge_lang_java::JavaTests::testng()));
        // KotlinForge: Kotest specs and JUnit tests in Kotlin files.
        tests.register(Box::new(kotlinforge_lang_kotlin::KotlinTests));
        // CForge: GoogleTest, Catch2 and CTest replace their manifest declarations.
        cforge_lang_cpp::register(&mut tests);
        // JavaForge's JVM extras: Spock, ScalaTest and MUnit.
        tests.register(Box::new(javaforge_lang_groovy::SpockTests));
        tests.register(Box::new(javaforge_lang_scala::ScalaTests::scalatest()));
        tests.register(Box::new(javaforge_lang_scala::ScalaTests::munit()));
        let program_settings = plugins
            .test_frameworks()
            .filter_map(|f| f.program_setting.clone())
            .collect();
        for t in plugins.run_configuration_types() {
            if t.command.is_empty() {
                continue;
            }
            interpreter_settings.extend(t.interpreter_setting.clone());
            kinds.register(Box::new(TemplateKind {
                id: t.id.clone(),
                command: t.command.clone(),
                default_interpreter: t.default_interpreter.clone(),
                interpreter_setting: t.interpreter_setting.clone(),
                required: t
                    .fields
                    .iter()
                    .filter(|f| f.required)
                    .map(|f| (f.key.clone(), f.label.clone()))
                    .collect(),
            }));
        }
        Self {
            runs: Mutex::default(),
            observers: Mutex::default(),
            next_id: AtomicU64::new(0),
            kinds,
            interpreter_settings,
            tests,
            program_settings,
        }
    }

    /// Register a run that is not a process started here (a WebSocket request), so Stop,
    /// console input and dispose reach it.
    pub fn register(&self, run_id: &str, process: Arc<dyn Process>, output: Arc<Mutex<RunOutput>>) {
        self.runs
            .lock()
            .expect("runs poisoned")
            .insert(run_id.to_string(), Run { process, output });
    }

    pub(crate) fn get(&self, id: &str) -> Result<RunHandles> {
        self.runs
            .lock()
            .expect("runs poisoned")
            .get(id)
            .map(|r| (r.process.clone(), r.output.clone()))
            .ok_or_else(|| ForgeError::Other(format!("no run {id}")))
    }
}

fn root(state: &AppState) -> Result<std::path::PathBuf> {
    state.project_root()
}

/// Configurations live in the project's `.forge` folder, on the project's host.
#[tauri::command]
pub async fn run_configurations_load(state: State<'_, AppState>) -> Result<RunConfigurations> {
    let backend = state.backend()?;
    blocking(move || {
        if backend.info().is_local() {
            return RunConfigurations::load(&backend.info().root);
        }
        let shared = backend.read_file(forge_process::config::PROJECT_FILE).ok();
        let local = backend.read_file(forge_process::config::LOCAL_FILE).ok();
        RunConfigurations::from_texts(shared.as_deref(), local.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn run_configurations_save(
    configurations: RunConfigurations,
    state: State<'_, AppState>,
) -> Result<()> {
    let backend = state.backend()?;
    blocking(move || {
        if backend.info().is_local() {
            return configurations.save(&backend.info().root);
        }
        let [shared, local] = configurations.to_texts()?;
        for (file, text) in [
            (forge_process::config::PROJECT_FILE, shared),
            (forge_process::config::LOCAL_FILE, local),
        ] {
            match text {
                Some(t) => backend.write_file(file, &t)?,
                // No delete on the remote side: empty out an existing file.
                None if backend.read_file(file).is_ok() => backend.write_file(file, "")?,
                None => {}
            }
        }
        Ok(())
    })
    .await
}

/// What to start: a configuration the backend builds, or a command line built by a
/// UI-side configuration type.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRequest {
    /// Id chosen by the UI so it can subscribe to `run://<runId>` before the start;
    /// generated when missing.
    pub run_id: Option<String>,
    /// The configuration's name and type when the UI builds the command line (Code With Me
    /// shows the run to guests under this name); taken from `configuration` otherwise.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub type_id: Option<String>,
    pub configuration: Option<RunConfiguration>,
    pub command: Option<CommandLine>,
    /// Extra environment, e.g. `XDEBUG_SESSION` from a debug launcher.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Extra interpreter arguments placed right after the program (e.g. `-dxdebug.mode=debug`).
    #[serde(default)]
    pub interpreter_args: Vec<String>,
    /// Build a test tree from TeamCity service messages on stdout.
    #[serde(default)]
    pub test_runner: bool,
    /// External Tools output filters (`$FILE_PATH$:$LINE$`) that link file references.
    #[serde(default)]
    pub output_filters: Vec<String>,
    /// Run with Coverage: the framework writes a coverage report that is read at the exit.
    #[serde(default)]
    pub coverage: bool,
    /// A debug run's port that the run target must make reachable (a DAP adapter).
    #[serde(default)]
    pub debug_port: Option<forge_runtargets::DebugPort>,
    /// "Run on" for command lines built by the UI (configurations carry their own).
    #[serde(default)]
    pub run_on: Option<forge_runtargets::RunTarget>,
    /// Profile (Python): run under cProfile; the run ends with a `profile` event.
    #[serde(default)]
    pub profile: bool,
    /// C/C++: run under Valgrind Memcheck or perf; the run ends with a `valgrind` / `perf`
    /// event.
    #[serde(default)]
    pub native_tool: Option<crate::commands::cpp_tools::NativeTool>,
    /// Profile with Xdebug or SPX (PHP) or V8 (Node.js); the run ends with a `profiler`
    /// event.
    #[serde(default)]
    pub profiler: Option<crate::commands::profiler::RunProfiler>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStarted {
    pub run_id: String,
    pub command_line: String,
}

/// Configuration types whose program is the PHP interpreter.
const PHP_KINDS: &[&str] = &[
    "php-script",
    "phpunit",
    "pest",
    "codeception",
    "behat",
    "phpspec",
    "artisan",
    "php-builtin-server",
];
/// The Node.js configuration type (Run with Coverage through c8 or nyc).
const NODE: &str = "node";
/// Composer types: they run through an interpreter only when they name one.
const COMPOSER_KINDS: &[&str] = &["composer", "composer-command"];

/// A request's command line, plus what a run through a PHP interpreter needs.
pub(crate) struct Built {
    pub cmd: CommandLine,
    /// Container / remote paths and their local counterparts (Run console links).
    pub path_mappings: Vec<OutputPathMapping>,
    /// The debugger's server with the interpreter's mappings.
    pub debug_server: Option<forge_debug::ServerConfig>,
    /// External Tools output filters (`$FILE_PATH$:$LINE$`).
    pub output_filters: Vec<String>,
    /// Test runs: how results are reported and what the Test History stores.
    pub tests: TestSetup,
}

fn build(state: &AppState, runs: &RunState, mut req: RunRequest) -> Result<Built> {
    let run_id = req.run_id.clone().unwrap_or_else(|| "preview".into());
    // Java debug runs: the JDWP port the JVM listens on (JavaForge's debugger).
    let java_debug = crate::commands::java::take_debug_port(&mut req.env);
    // Ruby debug runs: the port rdbg listens on (RubyForge's debugger).
    let rdbg_port = super::ruby::take_rdbg_port(&mut req.env);
    let root = root(state)?;
    let mut through: Option<(PhpInterpreter, bool)> = None;
    let output_filters = req.output_filters;
    let mut tests = TestSetup::default();
    // Ports the built-in web server serves, published by container / SSH interpreters.
    let mut publish: Vec<u16> = Vec::new();
    // "Run on" of the configuration, and the program PHP kinds run there.
    let mut run_on: Option<forge_runtargets::RunTarget> =
        req.run_on.clone().filter(|t| !t.is_local());
    let mut php_on_target = String::from("php");
    let coverage = req.coverage;
    let backend_local = state.backend()?.info().remote.is_none();
    let native_tool = req.native_tool;
    let native_type = req.configuration.as_ref().map(|c| c.type_id.clone());
    let profiler = req.profiler;
    // PHP configurations: their arguments go to PHP itself.
    let mut php_run = false;
    let mut cmd = match (req.configuration, req.command) {
        (_, Some(mut cmd)) => {
            if cmd.cwd.as_os_str().is_empty() {
                cmd.cwd = root;
            } else if cmd.cwd.is_relative() {
                cmd.cwd = root.join(&cmd.cwd);
            }
            cmd
        }
        (Some(cfg), None)
            if profiler.is_some() && super::python_tools::is_python_type(&cfg.type_id) =>
        {
            return Err(ForgeError::Other(format!(
                "'{}' configurations are profiled with Run | Profile (cProfile)",
                cfg.type_id
            )));
        }
        (Some(cfg), None) if super::python_tools::is_python_type(&cfg.type_id) => {
            // Python runs through its own interpreters (local or Docker / SSH / WSL).
            return super::python_tools::build(
                state,
                &cfg,
                super::python_tools::Extras {
                    run_id,
                    env: req.env,
                    head: req.interpreter_args,
                    debug_port: req.debug_port,
                    coverage,
                    profile: req.profile,
                    output_filters,
                },
            );
        }
        (Some(cfg), None) if req.profile => {
            return Err(ForgeError::Other(format!(
                "'{}' configurations cannot be profiled",
                cfg.type_id
            )));
        }
        (Some(mut cfg), None) => {
            // Spring Boot configurations run as Application configurations.
            crate::commands::spring::to_application(&mut cfg);
            if crate::commands::java::handles(&cfg.type_id) {
                crate::commands::java::prepare(state, &mut cfg, java_debug)?;
            }
            if crate::commands::scala::handles_test(&cfg.type_id) {
                crate::commands::scala::prepare(state, &mut cfg)?;
            }
            run_on = cfg.run_on.clone().filter(|t| !t.is_local()).or(run_on);
            let php_kind = PHP_KINDS.contains(&cfg.type_id.as_str());
            php_run = php_kind;
            let own = cfg.interpreter.trim();
            if !own.is_empty() && interpreters::interpreter_set(state).find(own).is_none() {
                php_on_target = own.to_string();
            }
            if coverage && cfg.type_id != NODE && !runs.tests.supports_coverage(&cfg.type_id) {
                return Err(ForgeError::Other(format!(
                    "'{}' configurations cannot run with coverage",
                    cfg.type_id
                )));
            }
            let composer_kind = COMPOSER_KINDS.contains(&cfg.type_id.as_str())
                && interpreters::interpreter_set(state)
                    .find(&cfg.interpreter)
                    .is_some();
            if php_kind || composer_kind {
                let interp = interpreters::resolve(state, &cfg.interpreter);
                if cfg.type_id == "php-builtin-server" && interp.kind.is_remote() {
                    let (host, port) = forge_process::config::builtin_server_address(&cfg);
                    publish.push(port);
                    // Inside a container, localhost is not reachable from the browser.
                    if matches!(
                        interp.kind,
                        phpforge_interpreters::InterpreterKind::Docker
                            | phpforge_interpreters::InterpreterKind::Compose
                    ) && matches!(host.as_str(), "localhost" | "127.0.0.1")
                    {
                        cfg.options.insert("host".into(), "0.0.0.0".into());
                    }
                }
                through = Some((interp, composer_kind));
            }
            let settings = state.read_settings();
            if (cfg.type_id == "composer" || cfg.type_id == "composer-command")
                && cfg.option("composerPath").trim().is_empty()
            {
                if let Some(composer) = settings
                    .get_as::<String>("php.composer.executable")
                    .filter(|c| !c.trim().is_empty() && c != "composer")
                {
                    cfg.options.insert("composerPath".into(), composer);
                }
            }
            drop(settings);
            // Kinds build for a local `php`; an interpreter wraps the result below.
            let php_interpreter = interpreters::local_php(state);
            let settings = state.read_settings();
            let ctx = BuildContext {
                php_interpreter,
                interpreters: runs
                    .interpreter_settings
                    .iter()
                    .filter_map(|k| settings.get_as::<String>(k).map(|v| (k.clone(), v)))
                    .collect(),
                root,
            };
            drop(settings);
            if cfg.type_id == "phpunit" {
                test_frameworks::apply_phpunit_defaults(state, &mut cfg)?;
            }
            // Reports and coverage go to the temp dir only when everything runs here.
            let local = backend_local
                && run_on.is_none()
                && through.as_ref().is_none_or(|(i, _)| !i.kind.is_remote());
            let mut cmd = if test_frameworks::is_test_type(&cfg.type_id) {
                let (cmd, setup) = test_frameworks::command_line(
                    state,
                    &cfg,
                    &ctx.php_interpreter,
                    &ctx.root,
                    &run_id,
                )?;
                tests = setup;
                cmd
            } else if runs.tests.for_type(&cfg.type_id).is_some()
                && !PHP_KINDS.contains(&cfg.type_id.as_str())
            {
                let (cmd, setup) =
                    framework_command(state, runs, &cfg, &ctx.root, &run_id, local, coverage)?;
                tests = setup;
                cmd
            } else if cfg.type_id == goforge_lang_go::GO_BUILD {
                super::go::build_run(state, &cfg)?
            } else if cfg.type_id == cforge_lang_cpp::CMAKE_APP {
                super::cpp::app_run(state, &cfg)?
            } else if cfg.type_id == rustforge_lang_rust::CARGO {
                super::rust::cargo_run(state, &cfg)?
            } else if cfg.type_id == rubyforge_lang_ruby::RUBY
                || cfg.type_id == rubyforge_lang_ruby::RAKE
            {
                super::ruby::run_command(state, &cfg)?
            } else if cfg.type_id == rubyforge_fw_rails::command::RAILS {
                super::rails::run_command(state, &cfg)?
            } else if cfg.type_id == dotnetforge_lang_dotnet::DOTNET_RUN {
                super::dotnet::run_command_line(state, &cfg)?
            } else {
                if cfg.type_id == "phpunit" {
                    tests.configuration = Some(cfg.clone());
                }
                match jvm_extras_command_line(state, &cfg)? {
                    Some(cmd) => cmd,
                    None => match crate::commands::kotlin::command_line(state, &cfg, java_debug)? {
                        Some(cmd) => cmd,
                        None => match crate::commands::java::command_line(state, &cfg, java_debug)?
                        {
                            Some(cmd) => cmd,
                            None => runs.kinds.build(&cfg, &ctx)?,
                        },
                    },
                }
            };
            // Node.js: the program runs under c8 (or nyc), which writes LCOV.
            if coverage && cfg.type_id == NODE && tests.coverage.is_none() {
                let (dir_arg, dir) =
                    test_frameworks::report_dir(local, &ctx.root, &run_id, "coverage");
                let node = cmd.program.clone();
                cmd.args = webforge_js_testing::coverage_wrap(
                    &test_frameworks::to_runtools(&cfg)?,
                    &node,
                    &dir_arg,
                    std::mem::take(&mut cmd.args),
                );
                tests.coverage = Some(CoverageSetup {
                    dir: Some(dir),
                    file: "lcov.info".into(),
                    format: forge_testing::CoverageFormat::Lcov,
                    collect: None,
                    name: cfg.name.clone(),
                    location_rules: Vec::new(),
                    tests_dir: None,
                });
            }
            // PHP test frameworks: a Clover report with Xdebug or PCOV (the configuration's
            // driver), plus PHPUnit's XML report for the tests of each line.
            if coverage && tests.coverage.is_none() {
                let (dir_arg, dir) =
                    test_frameworks::report_dir(local, &ctx.root, &run_id, "coverage");
                let backend = state.backend()?;
                // Paths relative to the working directory, as project paths.
                let cwd = cmd.cwd.clone();
                let project_path = |f: &str| {
                    let abs = if std::path::Path::new(f).is_absolute() {
                        std::path::PathBuf::from(f)
                    } else {
                        cwd.join(f)
                    };
                    abs.strip_prefix(&ctx.root)
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_else(|_| f.to_string())
                };
                let plan = phpforge_test_frameworks::provider::coverage_plan(
                    &test_frameworks::to_runtools(&cfg)?,
                    &dir_arg,
                    &|f| backend.read_file(&project_path(f)).ok(),
                )
                .map_err(ForgeError::Other)?;
                cmd.args.splice(0..0, plan.interpreter_args);
                cmd.args.extend(plan.args);
                for (k, v) in plan.env {
                    cmd.env.entry(k).or_insert(v);
                }
                tests.coverage = Some(CoverageSetup {
                    dir: plan.in_dir.then_some(dir),
                    file: if plan.in_dir {
                        plan.file
                    } else {
                        project_path(&plan.file)
                    },
                    format: plan.format,
                    collect: None,
                    name: cfg.name.clone(),
                    location_rules: Vec::new(),
                    tests_dir: plan.tests_dir,
                });
            }
            cmd
        }
        (None, None) => return Err(ForgeError::Other("nothing to run".into())),
    };
    cmd.env.extend(req.env);
    if !req.interpreter_args.is_empty() {
        cmd.args.splice(0..0, req.interpreter_args);
    }
    // Debug a Ruby, Rake, RSpec or Minitest run: under rdbg, which waits for the IDE.
    if let Some(port) = rdbg_port.filter(|_| {
        native_type
            .as_deref()
            .is_some_and(super::ruby::is_ruby_type)
    }) {
        cmd = super::ruby::wrap_rdbg(state, cmd, port);
    }
    // Profile with Xdebug / SPX (PHP) or V8 (Node.js).
    if let Some(kind) = profiler {
        let local = backend_local
            && run_on.is_none()
            && through.as_ref().is_none_or(|(i, _)| !i.kind.is_remote());
        let (wrapped, setup) = crate::commands::profiler::wrap(
            state,
            kind,
            native_type.as_deref(),
            cmd,
            &run_id,
            &self::root(state)?,
            local,
            php_run,
        )?;
        cmd = wrapped;
        tests.profiler = Some(setup);
    }
    // Run with Valgrind Memcheck / Profile with perf (C/C++).
    if let Some(tool) = native_tool {
        let (wrapped, setup) = crate::commands::cpp_tools::wrap(
            state,
            tool,
            native_type.as_deref(),
            cmd,
            &run_id,
            run_on.is_some(),
        )?;
        cmd = wrapped;
        tests.native = Some(setup);
    }
    // "Run on" a target: the command runs there unless a remote PHP interpreter already
    // decides where PHP runs.
    let remote_interpreter = through.as_ref().is_some_and(|(i, _)| i.kind.is_remote());
    if let Some(target) = run_on.filter(|_| !remote_interpreter) {
        let php = through.as_ref().is_some_and(|(_, composer)| !composer);
        return on_target(
            state,
            &target,
            cmd,
            php.then_some(php_on_target),
            publish,
            req.debug_port,
            output_filters,
            tests,
        );
    }
    let Some((interp, composer)) = through else {
        return Ok(Built {
            cmd,
            path_mappings: Vec::new(),
            debug_server: None,
            output_filters,
            tests,
        });
    };
    let backend = state.backend()?;
    let prepared = interpreters::prepare(state, backend.as_ref(), interp);
    // Composer runs as `composer …`, or as `php composer.phar …`.
    let program = if composer
        && !cmd
            .args
            .first()
            .is_some_and(|a| a.to_ascii_lowercase().ends_with(".phar"))
    {
        Program::Other(cmd.program.clone())
    } else {
        Program::Php
    };
    Ok(Built {
        cmd: prepared.wrap_serving(program, &cmd, publish)?,
        path_mappings: prepared.output_mappings(),
        debug_server: phpforge_interpreters::command::is_debug(&cmd.env)
            .then(|| prepared.debug_server())
            .flatten(),
        output_filters,
        tests,
    })
}

/// Groovy Script, sbt Task and Android App configurations (JavaForge's JVM extras).
fn jvm_extras_command_line(
    state: &AppState,
    cfg: &RunConfiguration,
) -> Result<Option<CommandLine>> {
    if let Some(cmd) = super::groovy::command_line(state, cfg)? {
        return Ok(Some(cmd));
    }
    if let Some(cmd) = super::scala::command_line(state, cfg)? {
        return Ok(Some(cmd));
    }
    super::android::command_line(state, cfg)
}

/// The command of a test framework that a manifest declares (busted, …): built by
/// `forge-testing`, with its report folder and Run with Coverage report.
fn framework_command(
    state: &AppState,
    runs: &RunState,
    cfg: &RunConfiguration,
    root: &std::path::Path,
    run_id: &str,
    local: bool,
    coverage: bool,
) -> Result<(CommandLine, TestSetup)> {
    let (report_arg, report_dir) = test_frameworks::report_dir(local, root, run_id, "");
    let (coverage_arg, coverage_dir) = test_frameworks::report_dir(local, root, run_id, "coverage");
    let settings = state.read_settings();
    let mut ctx = forge_testing::CommandContext {
        report_dir: report_arg,
        coverage_file: coverage.then(|| format!("{coverage_arg}/coverage")),
        settings: runs
            .program_settings
            .iter()
            .filter_map(|k| settings.get_as::<String>(k).map(|v| (k.clone(), v)))
            .collect(),
    };
    drop(settings);
    super::go::extend_test_context(state, &cfg.type_id, &mut ctx);
    super::cpp::extend_test_context(state, &cfg.type_id, &mut ctx);
    super::rust::extend_test_context(state, &cfg.type_id, &mut ctx);
    super::ruby::extend_test_context(state, &cfg.type_id, &mut ctx);
    super::dotnet::extend_test_context(state, &cfg.type_id, &mut ctx);
    let built = runs
        .tests
        .command(&test_frameworks::to_runtools(cfg)?, &ctx)
        .unwrap_or_else(|| Err(format!("no test framework builds '{}'", cfg.type_id)))
        .map_err(ForgeError::Other)?;
    let mut cmd = CommandLine::new(built.program).args(built.args);
    let cwd = match cfg.working_directory.trim() {
        "" => root.to_path_buf(),
        d if std::path::Path::new(d).is_absolute() => std::path::PathBuf::from(d),
        d => root.join(d),
    };
    cmd.cwd = cwd.clone();
    cmd.env.extend(built.env);
    cmd.env.extend(cfg.env.clone());
    let coverage = built.coverage.map(|c| {
        let in_dir = c
            .file
            .strip_prefix(&format!("{coverage_arg}/"))
            .map(str::to_string);
        let (dir, file) = match in_dir {
            Some(name) => (Some(coverage_dir.clone()), name),
            None => {
                // A report the tool writes into the project (relative to the working dir).
                let abs = if std::path::Path::new(&c.file).is_absolute() {
                    std::path::PathBuf::from(&c.file)
                } else {
                    cwd.join(&c.file)
                };
                let rel = abs
                    .strip_prefix(root)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| c.file.clone());
                (None, rel)
            }
        };
        CoverageSetup {
            dir,
            file,
            format: c.format,
            tests_dir: None,
            collect: c.collect,
            name: cfg.name.clone(),
            location_rules: if c.location_rules.is_empty() {
                built.location_rules.clone()
            } else {
                c.location_rules
            },
        }
    });
    Ok((
        cmd,
        TestSetup {
            report: built.report,
            flavor: built.flavor,
            report_dir: if !built.report_dirs.is_empty() {
                Some(test_frameworks::ReportDir::Builds(
                    built.report_dirs.clone(),
                ))
            } else {
                (built.report == forge_process::output::TestReport::JUnitFile).then_some(report_dir)
            },
            configuration: Some(cfg.clone()),
            location_rules: built.location_rules,
            default_file: built.default_file,
            coverage,
            profile: None,
            native: None,
            profiler: None,
        },
    ))
}

/// A command (as if run here) on a run target: wrapped by `forge-runtargets` with the
/// target's mappings; PHP programs get what Xdebug needs to reach the IDE.
#[allow(clippy::too_many_arguments)]
pub(crate) fn on_target(
    state: &AppState,
    target: &forge_runtargets::RunTarget,
    cmd: CommandLine,
    php: Option<String>,
    publish: Vec<u16>,
    debug_port: Option<forge_runtargets::DebugPort>,
    output_filters: Vec<String>,
    tests: TestSetup,
) -> Result<Built> {
    use forge_runtargets::{DebugDirection, DebugPort, Mapper};
    use phpforge_interpreters::command::{debug_port as xdebug_port, is_debug, with_client_host};
    let backend = state.backend()?;
    let ctx = testing::target_context(backend.as_ref(), target, &testing::docker_cli(state));
    let mapper = Mapper::new(&ctx.root, &ctx.mappings);
    let mut env = cmd.env.clone();
    let mut debug = debug_port;
    let mut debug_server = None;
    let server_name = target.display();
    if php.is_some() && is_debug(&env) {
        env.entry("PHP_IDE_CONFIG".into())
            .or_insert_with(|| format!("serverName={server_name}"));
        if target.kind.is_container() {
            if let Some(c) = env.get_mut("XDEBUG_CONFIG") {
                *c = with_client_host(c, "host.docker.internal");
            }
        }
        debug = debug.or(Some(DebugPort {
            port: xdebug_port(&env).unwrap_or(0),
            direction: DebugDirection::ConnectsToIde,
        }));
        debug_server = Some(forge_debug::ServerConfig {
            id: format!("target:{server_name}"),
            name: server_name.clone(),
            host: String::new(),
            port: 80,
            debugger: "Xdebug".into(),
            use_path_mappings: true,
            mappings: mapper
                .pairs()
                .iter()
                .map(|(local, remote)| forge_debug::PathMapping {
                    local: local.clone(),
                    remote: remote.clone(),
                })
                .collect(),
        });
    }
    let inv = forge_runtargets::Invocation {
        program: php.unwrap_or_else(|| cmd.program.clone()),
        program_args: Vec::new(),
        args: cmd.args.clone(),
        cwd: cmd.cwd.to_string_lossy().into_owned(),
        env,
        publish,
        debug,
    };
    let w = forge_runtargets::wrap(target, &inv, &ctx).map_err(ForgeError::Other)?;
    Ok(Built {
        cmd: CommandLine {
            program: w.program,
            args: w.args,
            cwd: std::path::PathBuf::from(w.cwd),
            env: w.env,
        },
        path_mappings: mapper
            .pairs()
            .iter()
            .map(|(local, remote)| OutputPathMapping {
                remote: remote.clone(),
                local: local.clone(),
            })
            .collect(),
        debug_server,
        output_filters,
        tests,
    })
}

/// The command line of a run configuration plus, for test configurations, the format its
/// results arrive in (agent tasks run it in their sandbox).
pub(crate) fn configuration_command(
    state: &AppState,
    runs: &RunState,
    configuration: RunConfiguration,
) -> Result<(CommandLine, Option<forge_process::output::TestReport>)> {
    let test_runner = runs.tests.is_test_type(&configuration.type_id);
    let request: RunRequest = serde_json::from_value(serde_json::json!({
        "configuration": configuration,
        "testRunner": test_runner,
    }))
    .map_err(|e| ForgeError::Other(e.to_string()))?;
    let built = build(state, runs, request)?;
    Ok((built.cmd, test_runner.then_some(built.tests.report)))
}

/// The command line a request would start, for previews in the configuration dialog.
#[tauri::command]
pub fn run_command_line(
    request: RunRequest,
    state: State<'_, AppState>,
    runs: State<'_, RunState>,
) -> Result<CommandLine> {
    build(&state, &runs, request).map(|b| b.cmd)
}

#[tauri::command]
pub async fn run_start(
    request: RunRequest,
    app: AppHandle,
    state: State<'_, AppState>,
    runs: State<'_, RunState>,
) -> Result<RunStarted> {
    let run_id = request.run_id.clone().unwrap_or_else(|| {
        let n = runs.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        format!("run-{n}")
    });
    let test_runner = request.test_runner;
    let mut request = request;
    let mut spring_run = false;
    // Code With Me: guests who see runs see this one from its first event on.
    let shared_as = request
        .configuration
        .as_ref()
        .map(|c| (c.name.clone(), Some(c.type_id.clone())))
        .or_else(|| request.name.clone().map(|n| (n, request.type_id.clone())));
    if let Some((name, kind)) = &shared_as {
        crate::commands::collab::on_run_starting(&app, &run_id, name, kind.as_deref());
    }
    if let Some(cfg) = request.configuration.as_mut() {
        // The run dashboard follows Spring Boot runs through their output.
        if cfg.type_id == javaforge_fw_spring::run::TYPE {
            let debug = request
                .env
                .contains_key(javaforge_lang_java::launch::DEBUG_PORT_ENV);
            let observer = crate::commands::spring::track_run(&app, &run_id, cfg, debug);
            runs.observers
                .lock()
                .expect("observers poisoned")
                .insert(run_id.clone(), observer);
            crate::commands::spring::to_application(cfg);
            spring_run = true;
        }
        // Application runs get their class path from JDT LS (java-debug).
        crate::commands::java::resolve_launch(&app, cfg).await;
    }
    // C/C++ runs and tests: sanitizer reports from the output.
    let sanitizer_run = request
        .configuration
        .as_ref()
        .is_some_and(|c| crate::commands::cpp_tools::observes(&c.type_id));
    if sanitizer_run {
        runs.observers
            .lock()
            .expect("observers poisoned")
            .entry(run_id.clone())
            .or_insert_with(|| crate::commands::cpp_tools::sanitizer_observer(&app, &run_id));
    }
    let started = match build(&state, &runs, request) {
        Ok(built) => {
            start_built(
                run_id.clone(),
                built,
                test_runner,
                app.clone(),
                &state,
                &runs,
            )
            .await
        }
        Err(e) => Err(e),
    };
    if started.is_err() && shared_as.is_some() {
        crate::commands::collab::on_run_disposed(&app, &run_id);
    }
    if started.is_err() && sanitizer_run {
        runs.observers
            .lock()
            .expect("observers poisoned")
            .remove(&run_id);
    }
    if started.is_err() && spring_run {
        runs.observers
            .lock()
            .expect("observers poisoned")
            .remove(&run_id);
        crate::commands::spring::forget_run(&app, &run_id);
    }
    started
}

/// Start a built command line as run `run_id`, streaming `run://<runId>` (also used by
/// the Command Line Tools Console).
pub(crate) async fn start_built(
    run_id: String,
    built: Built,
    test_runner: bool,
    app: AppHandle,
    state: &AppState,
    runs: &RunState,
) -> Result<RunStarted> {
    // Run configurations, test runners, debug runs, scripts, external tools and Before
    // launch tasks all start here.
    state.require_trusted("Running programs of the project")?;
    let Built {
        cmd,
        path_mappings,
        debug_server,
        output_filters,
        tests,
    } = built;
    if let Some(server) = debug_server {
        if let Some(debug) = app.try_state::<crate::commands::debug::DebugState>() {
            debug.add_interpreter_server(server);
        }
    }
    // Settings | Editor | General | Console, "Override console cycle buffer size (KB)".
    let cycle_buffer_kb = state
        .read_settings()
        .get_as::<usize>("run.console.cycleBufferKb")
        .or(Some(1024));
    let output = Arc::new(Mutex::new(RunOutput::new(&RunOutputOptions {
        test_runner,
        root: Some(root(state)?.display().to_string()),
        cwd: Some(cmd.cwd.display().to_string()),
        cycle_buffer_kb,
        path_mappings: path_mappings.clone(),
        output_filters,
        test_report: tests.report,
        junit_flavor: tests.flavor,
        location_rules: tests.location_rules.clone(),
        default_test_file: tests.default_file.clone(),
    })));
    let run_root = root(state)?.to_string_lossy().replace('\\', "/");
    let run_cwd = cmd.cwd.to_string_lossy().replace('\\', "/");
    let event = format!("run://{run_id}");
    let sink_output = output.clone();
    let backend = state.backend()?;
    let sink_backend = backend.clone();
    if let Some(dir) = &tests.report_dir {
        let (dir, b) = (dir.clone(), backend.clone());
        blocking(move || dir.prepare(b.as_ref())).await?;
    }
    if let Some(dir) = tests.coverage.as_ref().and_then(|c| c.dir.clone()) {
        let b = backend.clone();
        blocking(move || dir.prepare(b.as_ref())).await?;
    }
    let started_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default();
    let sink_app = app.clone();
    let observer = runs
        .observers
        .lock()
        .expect("observers poisoned")
        .remove(&run_id);
    let sink: RunSink = Arc::new(move |e: RunEvent| {
        if let Some(observe) = &observer {
            observe(&e);
        }
        let input = match &e {
            RunEvent::Started { command_line, .. } => RunInput::Started {
                command_line: command_line.clone(),
            },
            RunEvent::Output { stream, text } => RunInput::Output {
                stream: match stream {
                    OutputStream::Stderr => "stderr",
                    _ => "stdout",
                }
                .into(),
                text: text.clone(),
            },
            RunEvent::Exited { code, signal } => RunInput::Exited {
                code: *code,
                signal: *signal,
            },
        };
        let exited = matches!(e, RunEvent::Exited { .. });
        let mut out = sink_output.lock().expect("run output poisoned");
        let mut rendered = Vec::new();
        if exited {
            // JUnit reports (Codeception, Behat) are complete once the runner exited.
            if let Some(dir) = &tests.report_dir {
                for xml in dir.take(sink_backend.as_ref()) {
                    rendered.extend(out.handle(&RunInput::Report { xml }));
                }
            }
        }
        rendered.extend(out.handle(&input));
        if exited {
            if let (Some(cfg), Some(tree)) = (&tests.configuration, out.tests()) {
                if tree.counts().total > 0 {
                    let state = sink_app.state::<AppState>();
                    test_frameworks::record_run(&state, cfg.clone(), started_at, tree);
                }
            }
        }
        drop(out);
        let mut coverage_event = None;
        let mut native_event = None;
        let mut profiler_event = None;
        if exited {
            // Xdebug, SPX and V8 write their snapshots when the program exits.
            if let Some(setup) = &tests.profiler {
                profiler_event = Some(crate::commands::profiler::record(
                    sink_backend.as_ref(),
                    setup,
                    &path_mappings,
                ));
            }
            // Valgrind's XML and perf's data are complete once the program exited.
            if let Some(setup) = &tests.native {
                native_event = Some(crate::commands::cpp_tools::record(
                    &sink_app.state::<AppState>(),
                    sink_backend.as_ref(),
                    setup,
                ));
            }
            // Profile: the snapshot is complete once the program exited.
            if let Some(setup) = &tests.profile {
                coverage_event = Some(
                    match crate::commands::python_tools::record_profile(
                        &sink_app,
                        sink_backend.as_ref(),
                        setup,
                        &run_root,
                    ) {
                        Ok(profile) => serde_json::json!({ "type": "profile", "profile": profile }),
                        Err(error) => serde_json::json!({ "type": "profile", "error": error }),
                    },
                );
            }
            // Run with Coverage: the report is complete once the runner exited.
            if let Some(setup) = &tests.coverage {
                coverage_event = Some(
                    match testing::record_coverage(
                        &sink_app,
                        sink_backend.as_ref(),
                        setup,
                        &run_root,
                        &run_cwd,
                        &path_mappings,
                    ) {
                        Ok(suite) => serde_json::json!({ "type": "coverage", "suite": suite }),
                        Err(error) => serde_json::json!({ "type": "coverage", "error": error }),
                    },
                );
            }
        }
        if !exited {
            emit(&app, &event, &e);
        }
        for r in &rendered {
            emit(&app, &event, r);
        }
        if let Some(c) = &coverage_event {
            emit(&app, &event, c);
        }
        if let Some(n) = &native_event {
            emit(&app, &event, n);
        }
        if let Some(p) = &profiler_event {
            emit(&app, &event, p);
        }
        if exited {
            emit(&app, &event, &e);
        }
    });
    let start_cmd = cmd.clone();
    let process = blocking(move || backend.start_process(&start_cmd, sink)).await?;
    runs.runs
        .lock()
        .expect("runs poisoned")
        .insert(run_id.clone(), Run { process, output });
    Ok(RunStarted {
        run_id,
        command_line: cmd.display(),
    })
}

/// Stop a run and its child processes (SIGTERM, then SIGKILL after two seconds).
#[tauri::command]
pub async fn run_stop(run_id: String, runs: State<'_, RunState>) -> Result<()> {
    let process = runs.get(&run_id)?.0;
    blocking(move || {
        process.kill(Duration::from_secs(2));
        Ok(())
    })
    .await
}

/// Send console input to a run's stdin and echo it in the console.
#[tauri::command]
pub fn run_write(
    run_id: String,
    data: String,
    app: AppHandle,
    runs: State<'_, RunState>,
) -> Result<()> {
    let (process, output) = runs.get(&run_id)?;
    let echo = output
        .lock()
        .expect("run output poisoned")
        .handle(&RunInput::Input { text: data.clone() });
    for e in &echo {
        emit(&app, &format!("run://{run_id}"), e);
    }
    process.write_stdin(data.as_bytes())
}

/// Forget a finished run (its tab was closed); a running one is stopped first.
#[tauri::command]
pub async fn run_dispose(run_id: String, app: AppHandle, runs: State<'_, RunState>) -> Result<()> {
    crate::commands::collab::on_run_disposed(&app, &run_id);
    let removed = runs.runs.lock().expect("runs poisoned").remove(&run_id);
    if let Some(r) = removed {
        blocking(move || {
            r.process.kill(Duration::from_secs(2));
            Ok(())
        })
        .await?;
    }
    Ok(())
}

fn emit<T: Serialize>(app: &AppHandle, event: &str, payload: &T) {
    if let Some(run_id) = event.strip_prefix("run://") {
        crate::commands::collab::on_run_event(app, run_id, payload);
    }
    if let Err(err) = app.emit(event, payload) {
        tracing::warn!("cannot emit {event}: {err}");
    }
}
