//! The shared test runner of every pack, coverage and run targets:
//!
//! - Test frameworks: the registry of `forge-testing` (the PHP frameworks of
//!   `phpforge-test-frameworks` plus the manifests' `[[contributions.testFrameworks]]`)
//!   answers gutter discovery, Rerun Failed Tests and builds the commands of manifest
//!   frameworks (`run.rs`); `test_frameworks_detect` tells which manifest frameworks the
//!   project uses.
//! - Coverage: Run with Coverage runs (`RunRequest.coverage`) read their report when they
//!   exit ([`record_coverage`]); the Coverage tool window lists the suites
//!   (`coverage_suites`, `coverage_view`), the editor asks for line marks
//!   (`coverage_file`), and "Import a report" reads LCOV, Cobertura, Clover, JaCoCo or a Go
//!   profile from the project (`coverage_import`).
//! - Run targets: `run_target_info` gives a debugger the target, the host names and the
//!   path mappings of a configuration's "Run on".

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_process::output::links::LinkContext;
use forge_process::output::{LocationRule, OutputPathMapping};
use forge_process::RunConfiguration;
use forge_runtargets::{DebugTarget, RunTarget};
use forge_testing::coverage::{summary, Counter, CoverageNode, CoverageReport};
use forge_testing::{CoverageFormat, FailedTest, TestFile};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::commands::run::RunState;
use crate::commands::test_frameworks::{from_runtools, to_runtools, ReportDir};
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Where a Run with Coverage run's report is read after it exited.
#[derive(Debug, Clone)]
pub(crate) struct CoverageSetup {
    /// The report folder Forge created for the run (read and removed afterwards) …
    pub dir: Option<ReportDir>,
    /// … and the report's name in it, or (without `dir`) its project-relative path.
    pub file: String,
    pub format: CoverageFormat,
    /// The configuration's name (the suite's name).
    pub name: String,
    pub location_rules: Vec<LocationRule>,
}

/// A coverage suite as the Coverage tool window lists it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageSuiteInfo {
    pub id: String,
    pub name: String,
    /// Milliseconds since the epoch.
    pub at: u64,
    /// "Clover", "LCOV", …
    pub format: String,
    pub files: usize,
    pub lines: Counter,
    pub branches: Counter,
    /// "85% (17/20)".
    pub line_text: String,
    pub branch_text: String,
}

struct Suite {
    info: CoverageSuiteInfo,
    report: CoverageReport,
}

/// Coverage suites of this session, newest first (like IntelliJ's suites, at most 10).
#[derive(Default)]
pub struct CoverageState {
    suites: Mutex<Vec<Suite>>,
    next: AtomicU64,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

impl CoverageState {
    fn add(&self, name: &str, format: CoverageFormat, report: CoverageReport) -> CoverageSuiteInfo {
        let mut lines = Counter::default();
        let mut branches = Counter::default();
        for f in &report.files {
            let (l, b) = (f.line_counter(), f.branch_counter());
            lines.covered += l.covered;
            lines.total += l.total;
            branches.covered += b.covered;
            branches.total += b.total;
        }
        let info = CoverageSuiteInfo {
            id: format!("coverage-{}", self.next.fetch_add(1, Ordering::SeqCst) + 1),
            name: name.to_string(),
            at: now(),
            format: format.label().to_string(),
            files: report.files.len(),
            lines,
            branches,
            line_text: lines.text(),
            branch_text: branches.text(),
        };
        let mut suites = self.suites.lock().expect("coverage poisoned");
        suites.insert(
            0,
            Suite {
                info: info.clone(),
                report,
            },
        );
        suites.truncate(10);
        info
    }

    fn with<T>(&self, id: Option<&str>, f: impl FnOnce(&Suite) -> T) -> Option<T> {
        let suites = self.suites.lock().expect("coverage poisoned");
        let s = match id {
            Some(id) => suites.iter().find(|s| s.info.id == id),
            None => suites.first(),
        }?;
        Some(f(s))
    }
}

/// Reported paths as project paths: location rules first, then container / remote paths
/// mapped back and paths made project-relative (relative ones against the working dir).
fn resolver(
    root: &str,
    cwd: Option<&str>,
    mappings: &[OutputPathMapping],
    rules: &[LocationRule],
) -> impl Fn(&str) -> Option<String> {
    let links = LinkContext::new(Some(root), cwd).with_mappings(mappings.to_vec());
    let rules: Vec<(regex_lite::Regex, String)> = rules
        .iter()
        .filter_map(|r| {
            regex_lite::Regex::new(&r.pattern)
                .ok()
                .map(|re| (re, r.replacement.clone()))
        })
        .collect();
    move |path: &str| {
        let p = path.trim_start_matches("file://");
        let rewritten = rules
            .iter()
            .find(|(re, _)| re.is_match(p))
            .map(|(re, to)| re.replace(p, to.as_str()).into_owned());
        links.resolve(rewritten.as_deref().unwrap_or(p))
    }
}

/// Read a finished Run with Coverage run's report into a new suite; `None` when there is
/// no report (the run failed before writing one).
pub(crate) fn record_coverage(
    app: &AppHandle,
    backend: &dyn Backend,
    setup: &CoverageSetup,
    root: &str,
    cwd: &str,
    mappings: &[OutputPathMapping],
) -> std::result::Result<CoverageSuiteInfo, String> {
    let text = match &setup.dir {
        Some(dir) => {
            let t = dir.read(backend, &setup.file);
            dir.remove(backend);
            t
        }
        None => backend.read_file(&setup.file).ok(),
    }
    .ok_or_else(|| format!("no coverage report was written ({})", setup.file))?;
    let report = CoverageReport::parse(&text, setup.format)
        .or_else(|_| CoverageReport::parse_any(&text).map(|(r, _)| r))?;
    let report = report.resolve(resolver(root, Some(cwd), mappings, &setup.location_rules));
    let state = app.state::<CoverageState>();
    Ok(state.add(&setup.name, setup.format, report))
}

/// Manifest test frameworks the project uses (ids), from their `detect` rules.
#[tauri::command]
pub async fn test_frameworks_detect(state: State<'_, AppState>) -> Result<Vec<String>> {
    let backend = state.backend()?;
    let specs: Vec<forge_testing::TestFrameworkSpec> = state
        .plugins
        .read()
        .expect("plugins lock poisoned")
        .test_frameworks()
        .cloned()
        .collect();
    blocking(move || {
        let mut files: Vec<String> = Vec::new();
        for dir in ["", "spec", "test", "tests", "src"] {
            if let Ok(entries) = backend.list_dir(dir) {
                files.extend(entries.into_iter().map(|e| e.path));
            }
        }
        let read = |p: &str| backend.read_optional(p).ok().flatten();
        Ok(specs
            .iter()
            .filter(|s| s.detect(&files, &read))
            .map(|s| s.id.clone())
            .collect())
    })
    .await
}

/// The tests of a file for the gutter icons and Run Context Configuration, from the first
/// test framework that finds some.
#[tauri::command]
pub fn test_discover(
    path: String,
    text: String,
    state: State<'_, AppState>,
    runs: State<'_, RunState>,
    python: State<'_, crate::commands::python_tools::PythonToolsState>,
) -> TestFile {
    // Python files: the project's runner (pytest or unittest) builds the configurations.
    if path.to_ascii_lowercase().ends_with(".py") {
        return crate::commands::python_tools::discover_file(&state, &python, &path, &text);
    }
    runs.tests.discover(&path, &text, None)
}

/// The configuration that reruns the failed tests of a run.
#[tauri::command]
pub async fn test_rerun_failed(
    configuration: RunConfiguration,
    failed: Vec<FailedTest>,
    state: State<'_, AppState>,
    runs: State<'_, RunState>,
) -> Result<Option<RunConfiguration>> {
    // pytest / unittest: the failed tests' classes come from their files.
    if matches!(configuration.type_id.as_str(), "pytest" | "unittest") {
        return crate::commands::python_tools::rerun_failed(&state, &configuration, &failed);
    }
    match runs
        .tests
        .rerun_failed(&to_runtools(&configuration)?, &failed)
    {
        Some(c) => from_runtools(c, &configuration).map(Some),
        None => Ok(None),
    }
}

/// The coverage suites of the session, newest first.
#[tauri::command]
pub fn coverage_suites(coverage: State<'_, CoverageState>) -> Vec<CoverageSuiteInfo> {
    coverage
        .suites
        .lock()
        .expect("coverage poisoned")
        .iter()
        .map(|s| s.info.clone())
        .collect()
}

/// A suite (the newest without `id`) as the Coverage tool window shows it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageView {
    pub suite: CoverageSuiteInfo,
    pub nodes: Vec<CoverageNode>,
}

#[tauri::command]
pub fn coverage_view(
    id: Option<String>,
    state: State<'_, AppState>,
    coverage: State<'_, CoverageState>,
) -> Option<CoverageView> {
    let project = state.project_info().map(|i| i.name).unwrap_or_default();
    coverage.with(id.as_deref(), |s| CoverageView {
        suite: s.info.clone(),
        nodes: summary(&s.report, &project),
    })
}

/// The gutter marks of a project file in a suite (the newest without `id`).
#[tauri::command]
pub fn coverage_file(
    path: String,
    id: Option<String>,
    coverage: State<'_, CoverageState>,
) -> Vec<forge_testing::LineMark> {
    coverage
        .with(id.as_deref(), |s| {
            s.report.file(&path).map(|f| f.marks()).unwrap_or_default()
        })
        .unwrap_or_default()
}

#[tauri::command]
pub fn coverage_remove(id: String, coverage: State<'_, CoverageState>) {
    coverage
        .suites
        .lock()
        .expect("coverage poisoned")
        .retain(|s| s.info.id != id);
}

/// Read a coverage report of the project (LCOV, Cobertura, Clover, JaCoCo, Go profile).
#[tauri::command]
pub async fn coverage_import(
    path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CoverageSuiteInfo> {
    let backend = state.backend()?;
    let root = backend.info().root.to_string_lossy().replace('\\', "/");
    blocking(move || {
        let text = backend.read_file(&path)?;
        let (report, format) = CoverageReport::parse_any(&text).map_err(ForgeError::Other)?;
        let report = report.resolve(resolver(&root, None, &[], &[]));
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        Ok(app.state::<CoverageState>().add(&name, format, report))
    })
    .await
}

/// What a debugger needs to debug a configuration on its run target (`None` for local
/// runs): the target, how the program and the IDE reach each other and the path mappings.
#[tauri::command]
pub async fn run_target_info(
    configuration: RunConfiguration,
    state: State<'_, AppState>,
) -> Result<Option<DebugTarget>> {
    let Some(target) = configuration.run_on.filter(|t| !t.is_local()) else {
        return Ok(None);
    };
    let backend = state.backend()?;
    let docker = docker_cli(&state);
    blocking(move || {
        let ctx = target_context(backend.as_ref(), &target, &docker);
        Ok(Some(forge_runtargets::debug_target(&target, &ctx.mappings)))
    })
    .await
}

pub(crate) fn docker_cli(state: &AppState) -> String {
    state
        .read_settings()
        .get_as::<String>("docker.cliPath")
        .unwrap_or_default()
}

/// The target's command line context: its automatic mappings (a compose service's volumes
/// are read from the project's compose files) plus the user's.
pub(crate) fn target_context(
    backend: &dyn Backend,
    target: &RunTarget,
    docker: &str,
) -> forge_runtargets::TargetContext {
    let root = backend.info().root.to_string_lossy().replace('\\', "/");
    let compose = (target.kind == forge_runtargets::TargetKind::ComposeService)
        .then(|| {
            crate::commands::php_interpreters::compose_volumes(
                backend,
                &root,
                &target.compose_files,
                &target.service,
            )
        })
        .flatten();
    forge_runtargets::context(
        target,
        &root,
        compose.as_ref().map(|(d, v)| (d.as_str(), v.as_slice())),
        docker,
    )
}
