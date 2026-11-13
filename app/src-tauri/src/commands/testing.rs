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
//!   (`coverage_file`), and "Import a report" reads LCOV, Cobertura, Clover, JaCoCo, a Go
//!   profile or Istanbul JSON from the project (`coverage_import`). Show Coverage Data with
//!   several suites merges them (`coverage_merge`); Generate Coverage Report writes a suite
//!   as HTML (`coverage_export_html`).
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
use std::collections::BTreeMap;

use forge_testing::coverage::{
    folders, project_hint, summary, Counter, CoverageNode, CoverageReport,
};
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
    /// Raw data a pack's collector turns into the report first (CForge's gcov / llvm-cov).
    pub collect: Option<forge_testing::CoverageCollect>,
    /// A folder in `dir` with PHPUnit's XML report: the tests of each covered line.
    pub tests_dir: Option<String>,
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
    pub methods: Counter,
    /// "85% (17/20)".
    pub line_text: String,
    pub branch_text: String,
    pub method_text: String,
    /// The report names the tests of its lines (Show tests covering this line).
    pub per_test: bool,
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
    fn add(&self, name: &str, format: &str, report: CoverageReport) -> CoverageSuiteInfo {
        let t = report.totals();
        let info = CoverageSuiteInfo {
            id: format!("coverage-{}", self.next.fetch_add(1, Ordering::SeqCst) + 1),
            name: name.to_string(),
            at: now(),
            format: format.to_string(),
            files: report.files.len(),
            lines: t.lines,
            branches: t.branches,
            methods: t.methods,
            line_text: t.lines.text(),
            branch_text: t.branches.text(),
            method_text: t.methods.text(),
            per_test: report.has_tests(),
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
    let mut per_test = Vec::new();
    let text = if let Some(collect) = setup.collect.as_ref().filter(|c| c.kind == "cpp") {
        let dir = match &setup.dir {
            Some(ReportDir::Local(d)) => d.clone(),
            Some(ReportDir::Project(rel)) => std::path::Path::new(root).join(rel),
            _ => std::path::PathBuf::new(),
        };
        let t = crate::commands::cpp::collect_coverage(backend, collect, &dir, root);
        if let Some(d) = &setup.dir {
            d.remove(backend);
        }
        t?
    } else {
        match &setup.dir {
            Some(dir) => {
                let t = dir.read(backend, &setup.file);
                // PHPUnit's XML report: the tests of each covered line.
                if let Some(sub) = &setup.tests_dir {
                    per_test = forge_testing::coverage::phpunit_xml::collect(|name| {
                        dir.read(backend, &format!("{sub}/{name}"))
                    })
                    .unwrap_or_default();
                }
                dir.remove(backend);
                t
            }
            None => backend.read_file(&setup.file).ok(),
        }
        .ok_or_else(|| format!("no coverage report was written ({})", setup.file))?
    };
    let mut report = CoverageReport::parse(&text, setup.format)
        .or_else(|_| CoverageReport::parse_any(&text).map(|(r, _)| r))?;
    for (path, lines) in &per_test {
        report.attach_tests(path, lines);
    }
    let report = report.resolve(resolver(root, Some(cwd), mappings, &setup.location_rules));
    let state = app.state::<CoverageState>();
    Ok(state.add(&setup.name, setup.format.label(), report))
}

/// Manifest test frameworks the project uses (ids), from their `detect` rules.
fn detect(backend: &dyn Backend, specs: &[forge_testing::TestFrameworkSpec]) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for dir in ["", "spec", "test", "tests", "src"] {
        if let Ok(entries) = backend.list_dir(dir) {
            files.extend(entries.into_iter().map(|e| e.path));
        }
    }
    let read = |p: &str| backend.read_optional(p).ok().flatten();
    specs
        .iter()
        .filter(|s| s.detect(&files, &read))
        .map(|s| s.id.clone())
        .collect()
}

/// The last detection: project root, when, the ids. Gutter discovery runs on every edit;
/// the project's frameworks are looked up again after a few seconds.
static DETECTED: Mutex<Option<(std::path::PathBuf, std::time::Instant, Vec<String>)>> =
    Mutex::new(None);

fn specs(state: &AppState) -> Vec<forge_testing::TestFrameworkSpec> {
    state
        .plugins
        .read()
        .expect("plugins lock poisoned")
        .test_frameworks()
        .cloned()
        .collect()
}

#[tauri::command]
pub async fn test_frameworks_detect(state: State<'_, AppState>) -> Result<Vec<String>> {
    let backend = state.backend()?;
    let specs = specs(&state);
    blocking(move || Ok(detect(backend.as_ref(), &specs))).await
}

/// The tests of a file for the gutter icons and Run Context Configuration, from the first
/// test framework that finds some; frameworks the project uses (`detect`) come first, so a
/// `*.test.ts` file is Vitest's in a Vitest project and Jest's in a Jest project.
#[tauri::command]
pub async fn test_discover(
    path: String,
    text: String,
    state: State<'_, AppState>,
    runs: State<'_, RunState>,
    python: State<'_, crate::commands::python_tools::PythonToolsState>,
) -> Result<TestFile> {
    // Python files: the project's runner (pytest or unittest) builds the configurations.
    if path.to_ascii_lowercase().ends_with(".py") {
        return Ok(crate::commands::python_tools::discover_file(
            &state, &python, &path, &text,
        ));
    }
    // Rust files: the Cargo workspace knows their package, target and module.
    if path.ends_with(".rs")
        && state
            .plugins
            .read()
            .expect("plugins lock poisoned")
            .is_enabled(crate::commands::rust::PLUGIN)
    {
        return Ok(crate::commands::rust::discover_file(&state, &path, &text));
    }
    // Unity tests (C# in Assets/ and Packages/) and GdUnit4 suites (GDScript).
    if let Some(found) = crate::commands::gamedev::discover_file(&state, &path, &text) {
        return Ok(found);
    }
    // C# and F# files: the solution knows their project and its test framework.
    if dotnetforge_lang_dotnet::provider::is_test_source(&path)
        && state
            .plugins
            .read()
            .expect("plugins lock poisoned")
            .is_enabled(crate::commands::dotnet::PLUGIN)
    {
        return Ok(crate::commands::dotnet::discover_file(&state, &path, &text));
    }
    let backend = state.backend()?;
    let root = backend.info().root.clone();
    let specs = specs(&state);
    let cached = DETECTED
        .lock()
        .expect("detection lock")
        .as_ref()
        .filter(|(r, at, _)| *r == root && at.elapsed() < std::time::Duration::from_secs(10))
        .map(|(_, _, ids)| ids.clone());
    let detected = match cached {
        Some(ids) => ids,
        None => {
            let s = specs.clone();
            let ids = blocking(move || Ok(detect(backend.as_ref(), &s))).await?;
            *DETECTED.lock().expect("detection lock") =
                Some((root, std::time::Instant::now(), ids.clone()));
            ids
        }
    };
    let detectable: Vec<String> = specs.into_iter().map(|s| s.id).collect();
    Ok(runs
        .tests
        .discover_in_project(&path, &text, &detected, &detectable))
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
    /// The Project view's hints of every covered file and folder (folded, not compacted):
    /// "62% lines covered", "50% files, 62% lines covered".
    pub hints: BTreeMap<String, String>,
}

/// The Project view hints of a report.
pub(crate) fn hints(report: &CoverageReport) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = folders(report)
        .iter()
        .filter(|(path, _)| !path.is_empty())
        .filter_map(|(path, t)| project_hint(t, true).map(|h| (path.clone(), h)))
        .collect();
    for f in &report.files {
        if let Some(h) = project_hint(&f.totals(), false) {
            out.insert(f.path.clone(), h);
        }
    }
    out
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
        hints: hints(&s.report),
    })
}

/// Show Coverage Data with several suites: one suite of their merged reports.
#[tauri::command]
pub fn coverage_merge(
    ids: Vec<String>,
    coverage: State<'_, CoverageState>,
) -> Result<CoverageSuiteInfo> {
    let (names, reports): (Vec<String>, Vec<CoverageReport>) = {
        let suites = coverage.suites.lock().expect("coverage poisoned");
        ids.iter()
            .filter_map(|id| suites.iter().find(|s| &s.info.id == id))
            .map(|s| (s.info.name.clone(), s.report.clone()))
            .unzip()
    };
    if reports.len() < 2 {
        return Err(ForgeError::Other(
            "choose at least two coverage suites".into(),
        ));
    }
    Ok(coverage.add(
        &names.join(" + "),
        "Merged",
        CoverageReport::merged(reports),
    ))
}

/// Generate Coverage Report: a suite as one HTML file, written to `path` (desktop) and
/// returned (browser mode downloads it).
#[tauri::command]
pub async fn coverage_export_html(
    id: Option<String>,
    path: Option<String>,
    state: State<'_, AppState>,
    coverage: State<'_, CoverageState>,
) -> Result<String> {
    let (info, report) = coverage
        .with(id.as_deref(), |s| (s.info.clone(), s.report.clone()))
        .ok_or_else(|| ForgeError::Other("no coverage suite".into()))?;
    let project = state.project_info().map(|i| i.name).unwrap_or_default();
    let backend = state.backend()?;
    let html = blocking(move || {
        Ok(forge_testing::coverage::html::render(
            &report,
            &project,
            &info.name,
            &generated_at(info.at),
            |p| backend.read_file(p).ok(),
        ))
    })
    .await?;
    if let Some(p) = path {
        tokio::fs::write(&p, &html)
            .await
            .map_err(|e| ForgeError::io(p.clone(), e))?;
    }
    Ok(html)
}

/// "2026-10-28 10:41 UTC" for a time in milliseconds since the epoch.
fn generated_at(ms: u64) -> String {
    let secs = ms / 1000;
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rest / 3600,
        rest % 3600 / 60
    )
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
        Ok(app
            .state::<CoverageState>()
            .add(&name, format.label(), report))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_times_and_project_view_hints() {
        assert_eq!(generated_at(0), "1970-01-01 00:00 UTC");
        assert_eq!(generated_at(1_803_811_260_000), "2027-02-28 10:41 UTC");
        let (r, _) = CoverageReport::parse_any(
            "SF:src/a/x.js\nDA:1,1\nDA:2,0\nend_of_record\nSF:src/b.js\nDA:1,0\nend_of_record\n",
        )
        .unwrap();
        let h = hints(&r);
        assert_eq!(h["src"], "50% files, 33% lines covered");
        assert_eq!(h["src/a"], "100% files, 50% lines covered");
        assert_eq!(h["src/a/x.js"], "50% lines covered");
        assert!(!h.contains_key(""));
    }
}
