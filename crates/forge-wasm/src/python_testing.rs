//! PyForge's test runners and profiler (`pyforge-testing`) for the browser-mode mock:
//! discovery with a runner, Rerun Failed Tests with the files' source, the command lines of
//! pytest / unittest / tox, tox environments and profiler snapshots, like the desktop app.

use std::collections::BTreeMap;

use forge_runtools::Configuration;
use forge_testing::FailedTest;
use pyforge_testing::command::{self, Context, PytestReporter};
use pyforge_testing::pstats::{self, TreeLimits};
use pyforge_testing::{detect, discover, rerun, tox};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscoverRequest {
    path: String,
    text: String,
    /// `python.testing.runner`.
    #[serde(default)]
    runner: String,
    /// Project files for the detection (path → text).
    #[serde(default)]
    files: BTreeMap<String, String>,
}

/// `{ path, text, runner, files }` → `TestFile` (`test_discover` for Python files).
#[wasm_bindgen]
pub fn python_test_discover(request_json: &str) -> String {
    match parse::<DiscoverRequest>(request_json) {
        Ok(r) => {
            let scan = discover::scan(&r.path, &r.text);
            let project = detect::detect(|f| r.files.get(f).cloned());
            let runner = detect::choose_runner(&r.runner, &project, &scan);
            to_json(&discover::test_file(&r.path, &scan, runner))
        }
        Err(e) => error(e),
    }
}

/// `{ files }` → `ProjectTesting`.
#[wasm_bindgen]
pub fn python_testing_detect(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        files: BTreeMap<String, String>,
    }
    match parse::<Req>(request_json) {
        Ok(r) => to_json(&detect::detect(|f| r.files.get(f).cloned())),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct RerunRequest {
    configuration: Configuration,
    failed: Vec<FailedTest>,
    #[serde(default)]
    files: BTreeMap<String, String>,
}

/// `{ configuration, failed, files }` → `Configuration | null`.
#[wasm_bindgen]
pub fn python_rerun_failed(request_json: &str) -> String {
    match parse::<RerunRequest>(request_json) {
        Ok(r) => to_json(&rerun::rerun_failed(&r.configuration, &r.failed, |p| {
            r.files.get(p).cloned()
        })),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandRequest {
    configuration: Configuration,
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    report_dir: String,
    #[serde(default)]
    coverage_file: Option<String>,
    #[serde(default)]
    reporter: String,
    #[serde(default)]
    tox_program: String,
    #[serde(default)]
    head: Vec<String>,
}

/// `{ configuration, cwd, reportDir, coverageFile?, reporter, toxProgram, head }` →
/// `PythonCommand`.
#[wasm_bindgen]
pub fn python_test_command(request_json: &str) -> String {
    match parse::<CommandRequest>(request_json) {
        Ok(r) => {
            let ctx = Context {
                cwd: r.cwd,
                report_dir: r.report_dir,
                coverage_file: r.coverage_file,
                reporter: PytestReporter::from_setting(&r.reporter),
                tox_program: r.tox_program,
                debug_args: r.head,
            };
            match command::build(&r.configuration, &ctx) {
                Ok(c) => to_json(&c),
                Err(e) => error(e),
            }
        }
        Err(e) => error(e),
    }
}

/// `{ files }` → `ToxEnv[]`.
#[wasm_bindgen]
pub fn python_tox_environments(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        files: BTreeMap<String, String>,
    }
    match parse::<Req>(request_json) {
        Ok(r) => to_json(&tox::environments(|f| r.files.get(f).cloned())),
        Err(e) => error(e),
    }
}

/// A function of a demo profile: `[file, line, name, calls, own s, total s, callers]`,
/// callers as `[index, calls, own s, total s]`.
type DemoRow = (
    String,
    u32,
    String,
    u64,
    f64,
    f64,
    Vec<(usize, u64, f64, f64)>,
);

/// `{ rows }` → `ProfileView`: a demo snapshot written as a real `.pstats` file and read
/// back by the parser the desktop app uses.
#[wasm_bindgen]
pub fn python_demo_profile(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        rows: Vec<DemoRow>,
    }
    let r = match parse::<Req>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let rows: Vec<pstats::write::Row> = r
        .rows
        .iter()
        .map(|(f, l, n, c, o, t, callers)| {
            (f.as_str(), *l, n.as_str(), *c, *o, *t, callers.clone())
        })
        .collect();
    match pstats::parse(&pstats::write::pstats(&rows)) {
        Ok(p) => to_json(&p.view(300, TreeLimits::default())),
        Err(e) => error(e),
    }
}
