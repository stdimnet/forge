//! PHP test frameworks (PHPUnit, Pest, Codeception, Behat, PHPSpec): Settings | PHP | Test
//! Frameworks (configured plus detected frameworks), the command lines of their run
//! configurations, the tests of a file for the gutter, Behat step navigation, Rerun Failed
//! Tests and the Test History of the Run tool window. The logic lives in
//! `phpforge-test-frameworks` and `forge_process::test_history`.
//!
//! Codeception and Behat write JUnit reports into a folder that is read when the run exits
//! and then deleted: the system temp dir when the project and the interpreter are local,
//! otherwise `.forge/test-reports/<run>` in the project, which the interpreter (Docker, SSH,
//! WSL, a remote project) reaches through its path mappings.

use std::path::PathBuf;

use forge_backend::Backend;
use forge_core::project_state;
use forge_core::{CollectRequest, ForgeError, Result};
use forge_process::output::links::LinkContext;
use forge_process::output::{JUnitFlavor, TestNode, TestReport, TestSummary, TestTree};
use forge_process::test_history::{self, TestRunEntry, TestRunRecord};
use forge_process::{split_args, CommandLine, RunConfiguration};
use forge_runtools::Configuration;
use phpforge_interpreters::InterpreterKind;
use phpforge_test_frameworks::discover::TestFile;
use phpforge_test_frameworks::frameworks::{self, Framework, FrameworkConfig};
use phpforge_test_frameworks::steps::{self, StepDefinition};
use phpforge_test_frameworks::{command, rerun, FailedTest};
use serde::Serialize;
use tauri::State;

use crate::commands::php_interpreters as interpreters;
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Folder of JUnit reports inside the project when the run cannot use the temp dir.
const PROJECT_REPORTS: &str = ".forge/test-reports";

/// Whether a configuration type is built here (Pest, Codeception, Behat, PHPSpec).
pub(crate) fn is_test_type(type_id: &str) -> bool {
    command::TYPES.contains(&type_id)
}

fn detect(backend: &dyn Backend) -> Vec<FrameworkConfig> {
    let mut present: Vec<String> = Vec::new();
    for dir in ["", "vendor/bin", "tests"] {
        if let Ok(entries) = backend.list_dir(dir) {
            present.extend(entries.into_iter().map(|e| e.path));
        }
    }
    let composer = backend.read_optional("composer.json").ok().flatten();
    frameworks::detect(&|p| present.iter().any(|x| x == p), composer.as_deref())
}

/// The frameworks of Settings | PHP | Test Frameworks plus the detected ones.
pub(crate) fn frameworks(state: &AppState, backend: &dyn Backend) -> Vec<FrameworkConfig> {
    let configured = frameworks::from_setting(state.read_settings().get(frameworks::SETTINGS_KEY));
    frameworks::effective(configured, detect(backend))
}

/// Configured and detected test frameworks of the open project.
#[tauri::command]
pub async fn test_frameworks_list(state: State<'_, AppState>) -> Result<Vec<FrameworkConfig>> {
    let backend = state.backend()?;
    let configured = frameworks::from_setting(state.read_settings().get(frameworks::SETTINGS_KEY));
    blocking(move || Ok(frameworks::effective(configured, detect(backend.as_ref())))).await
}

fn to_runtools(cfg: &RunConfiguration) -> Result<Configuration> {
    serde_json::to_value(cfg)
        .and_then(serde_json::from_value)
        .map_err(|e| ForgeError::Other(e.to_string()))
}

fn from_runtools(cfg: Configuration, like: &RunConfiguration) -> Result<RunConfiguration> {
    let mut out: RunConfiguration = serde_json::to_value(cfg)
        .and_then(serde_json::from_value)
        .map_err(|e| ForgeError::Other(e.to_string()))?;
    // Fields `Configuration` does not carry.
    out.env = like.env.clone();
    out.interpreter = like.interpreter.clone();
    out.store_as_project_file = like.store_as_project_file;
    Ok(out)
}

/// Where a run's JUnit reports go and how they are read back.
#[derive(Debug, Clone)]
pub(crate) enum ReportDir {
    /// A local folder (the system temp dir).
    Local(PathBuf),
    /// A project folder read through the backend.
    Project(String),
}

impl ReportDir {
    /// Create the (empty) folder before the run starts.
    pub(crate) fn prepare(&self, backend: &dyn Backend) -> Result<()> {
        match self {
            ReportDir::Local(d) => {
                let _ = std::fs::remove_dir_all(d);
                std::fs::create_dir_all(d).map_err(|e| ForgeError::io(d, e))
            }
            ReportDir::Project(rel) => {
                let _ = backend.delete(rel);
                backend.create_dir(rel)
            }
        }
    }

    /// The JUnit XML texts in the folder; the folder is removed.
    pub(crate) fn take(&self, backend: &dyn Backend) -> Vec<String> {
        let mut out = Vec::new();
        match self {
            ReportDir::Local(dir) => {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
                    files.sort();
                    for f in files
                        .iter()
                        .filter(|f| f.extension().is_some_and(|x| x == "xml"))
                    {
                        if let Ok(t) = std::fs::read_to_string(f) {
                            out.push(t);
                        }
                    }
                }
                let _ = std::fs::remove_dir_all(dir);
            }
            ReportDir::Project(rel) => {
                if let Ok(mut entries) = backend.list_dir(rel) {
                    entries.sort_by(|a, b| a.path.cmp(&b.path));
                    for e in entries.iter().filter(|e| e.path.ends_with(".xml")) {
                        if let Ok(t) = backend.read_file(&e.path) {
                            out.push(t);
                        }
                    }
                }
                let _ = backend.delete(rel);
            }
        }
        out
    }
}

/// What a test run needs besides its command line.
#[derive(Debug, Clone, Default)]
pub(crate) struct TestSetup {
    pub report: TestReport,
    pub flavor: JUnitFlavor,
    pub report_dir: Option<ReportDir>,
    /// The configuration, stored with the results in the Test History.
    pub configuration: Option<RunConfiguration>,
}

fn framework_for(
    state: &AppState,
    backend: &dyn Backend,
    f: Framework,
    interpreter: &str,
) -> Option<FrameworkConfig> {
    let name = interpreters::resolve(state, interpreter).name;
    let list = frameworks(state, backend);
    frameworks::resolve(&list, f, &name).cloned()
}

/// PHPUnit configurations: the executable, configuration and bootstrap files of Settings |
/// PHP | Test Frameworks.
pub(crate) fn apply_phpunit_defaults(state: &AppState, cfg: &mut RunConfiguration) -> Result<()> {
    let backend = state.backend()?;
    let fw = framework_for(
        state,
        backend.as_ref(),
        Framework::Phpunit,
        &cfg.interpreter,
    );
    let mut c = to_runtools(cfg)?;
    command::phpunit_defaults(&mut c, fw.as_ref());
    cfg.options = c.options;
    Ok(())
}

/// The command line of a Pest, Codeception, Behat or PHPSpec configuration for a local
/// `php`, and its test setup. `run_id` names the report folder.
pub(crate) fn command_line(
    state: &AppState,
    cfg: &RunConfiguration,
    php: &str,
    root: &std::path::Path,
    run_id: &str,
) -> Result<(CommandLine, TestSetup)> {
    let backend = state.backend()?;
    let framework = Framework::from_id(&cfg.type_id)
        .ok_or_else(|| ForgeError::Other(format!("unknown test framework `{}`", cfg.type_id)))?;
    let fw = framework_for(state, backend.as_ref(), framework, &cfg.interpreter);
    let local = backend.info().remote.is_none()
        && interpreters::resolve(state, &cfg.interpreter).kind == InterpreterKind::Local;
    let safe: String = run_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let (arg_dir, dir) = if local {
        let dir = std::env::temp_dir().join("forge-test-reports").join(&safe);
        (dir.to_string_lossy().into_owned(), ReportDir::Local(dir))
    } else {
        let rel = format!("{PROJECT_REPORTS}/{safe}");
        (
            root.join(&rel).to_string_lossy().into_owned(),
            ReportDir::Project(rel),
        )
    };
    let built =
        command::build(&to_runtools(cfg)?, fw.as_ref(), &arg_dir).map_err(ForgeError::Other)?;
    let uses_dir = built.report == TestReport::JUnitFile;
    let php = match cfg.interpreter.trim() {
        "" => php.to_string(),
        i => i.to_string(),
    };
    let mut cmd = CommandLine::new(php)
        .args(split_args(&cfg.interpreter_options))
        .args(built.args)
        .args(split_args(&cfg.arguments));
    cmd.cwd = match cfg.working_directory.trim() {
        "" => root.to_path_buf(),
        d if std::path::Path::new(d).is_absolute() => PathBuf::from(d),
        d => root.join(d),
    };
    cmd.env.extend(cfg.env.clone());
    Ok((
        cmd,
        TestSetup {
            report: built.report,
            flavor: built.flavor,
            report_dir: uses_dir.then_some(dir),
            configuration: Some(cfg.clone()),
        },
    ))
}

/// The tests of a file for the gutter icons and Run Context Configuration.
#[tauri::command]
pub fn test_discover(path: String, text: String) -> TestFile {
    phpforge_test_frameworks::discover(&path, &text)
}

/// The configuration that reruns the failed tests of a run.
#[tauri::command]
pub fn test_rerun_failed(
    configuration: RunConfiguration,
    failed: Vec<FailedTest>,
) -> Result<Option<RunConfiguration>> {
    match rerun::rerun_failed(&to_runtools(&configuration)?, &failed) {
        Some(c) => from_runtools(c, &configuration).map(Some),
        None => Ok(None),
    }
}

/// The Behat step definitions the step on `line` (0-based) of a feature file uses; context
/// classes are searched in the project's PHP files.
#[tauri::command]
pub async fn behat_step_definitions(
    text: String,
    line: u32,
    state: State<'_, AppState>,
) -> Result<Vec<StepDefinition>> {
    let backend = state.backend()?;
    blocking(move || {
        if steps::step_at(&text, line).is_none() {
            return Ok(Vec::new());
        }
        let files = backend.collect_files(&CollectRequest {
            extensions: vec!["php".into()],
            max_files: 20_000,
            max_file_size: 512 * 1024,
            max_total: 128 * 1024 * 1024,
        })?;
        let defs: Vec<StepDefinition> = files
            .iter()
            .filter(|f| steps::may_define_steps(&f.text))
            .flat_map(|f| steps::definitions(&f.path, &f.text))
            .collect();
        Ok(steps::step_targets(&text, line, &defs))
    })
    .await
}

fn history_dir() -> Result<PathBuf> {
    project_state::default_test_history_dir()
        .ok_or_else(|| ForgeError::Other("no data directory".into()))
}

fn display_root(state: &AppState) -> Result<String> {
    state
        .project_info()
        .map(|i| i.display_root())
        .ok_or_else(|| ForgeError::Other("no project is open".into()))
}

/// Remember a finished test run in the Test History.
pub(crate) fn record_run(
    state: &AppState,
    configuration: RunConfiguration,
    at: u64,
    tree: &TestTree,
) {
    let Ok(root) = display_root(state) else {
        return;
    };
    let summary = tree.summary();
    let record = TestRunRecord {
        id: String::new(),
        name: configuration.name.clone(),
        configuration,
        at,
        summary: summary.text,
        counts: summary.counts,
        tree: tree.export(),
    };
    std::thread::spawn(move || {
        if let Err(e) = history_dir().and_then(|d| test_history::save(&d, &root, record)) {
            tracing::warn!("cannot store the test run: {e}");
        }
    });
}

/// The stored test runs of the project, newest first.
#[tauri::command]
pub async fn test_history_list(state: State<'_, AppState>) -> Result<Vec<TestRunEntry>> {
    let root = display_root(&state)?;
    blocking(move || Ok(test_history::list(&history_dir()?, &root))).await
}

/// A stored run as the Run tool window shows it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredRun {
    pub entry: TestRunEntry,
    pub configuration: RunConfiguration,
    pub nodes: Vec<TestNode>,
    pub summary: TestSummary,
}

#[tauri::command]
pub async fn test_history_open(id: String, state: State<'_, AppState>) -> Result<StoredRun> {
    let root = display_root(&state)?;
    let project = state.project_root()?.display().to_string();
    blocking(move || {
        let record = test_history::load(&history_dir()?, &root, &id)?;
        let tree = TestTree::import(&record.tree, LinkContext::new(Some(&project), None));
        Ok(StoredRun {
            entry: (&record).into(),
            configuration: record.configuration,
            nodes: tree.snapshot(),
            summary: tree.summary(),
        })
    })
    .await
}
