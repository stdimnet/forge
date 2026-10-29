//! PHP test frameworks (`phpforge-test-frameworks`) for the browser-mode mocks of the
//! `test_*` and `behat_step_definitions` IPC commands: detection, command lines, test
//! discovery, Rerun Failed Tests and Behat step definitions, computed like the desktop app.

use forge_output::links::LinkContext;
use forge_output::{StoredNode, TestTree};
use forge_runtools::Configuration;
use phpforge_test_frameworks::frameworks::{self, FrameworkConfig};
use phpforge_test_frameworks::steps::{self, StepDefinition};
use phpforge_test_frameworks::{command, rerun, FailedTest};
use serde::Deserialize;
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

use crate::to_json;

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct FrameworksRequest {
    /// Value of `php.testFrameworks`.
    configured: Option<Value>,
    /// Project-relative paths that exist.
    files: Vec<String>,
    composer_json: Option<String>,
}

/// `{ configured, files, composerJson }` → `FrameworkConfig[]` (`test_frameworks_list`).
#[wasm_bindgen]
pub fn test_frameworks_effective(request_json: &str) -> String {
    match parse::<FrameworksRequest>(request_json) {
        Ok(r) => {
            let detected = frameworks::detect(
                &|p| r.files.iter().any(|f| f == p),
                r.composer_json.as_deref(),
            );
            to_json(&frameworks::effective(
                frameworks::from_setting(r.configured.as_ref()),
                detected,
            ))
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandRequest {
    configuration: Configuration,
    framework: Option<FrameworkConfig>,
    report_dir: String,
}

/// `{ configuration, framework, reportDir }` → `TestCommand` (the mock command lines).
#[wasm_bindgen]
pub fn test_command(request_json: &str) -> String {
    match parse::<CommandRequest>(request_json) {
        Ok(r) => match command::build(&r.configuration, r.framework.as_ref(), &r.report_dir) {
            Ok(c) => to_json(&c),
            Err(e) => error(e),
        },
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct DiscoverRequest {
    path: String,
    text: String,
}

/// `{ path, text }` → `TestFile` (`test_discover`).
#[wasm_bindgen]
pub fn test_discover(request_json: &str) -> String {
    match parse::<DiscoverRequest>(request_json) {
        Ok(r) => to_json(&phpforge_test_frameworks::discover(&r.path, &r.text)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct RerunRequest {
    configuration: Configuration,
    failed: Vec<FailedTest>,
}

/// `{ configuration, failed }` → `Configuration | null` (`test_rerun_failed`).
#[wasm_bindgen]
pub fn test_rerun_failed(request_json: &str) -> String {
    match parse::<RerunRequest>(request_json) {
        Ok(r) => to_json(&rerun::rerun_failed(&r.configuration, &r.failed)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct SourceFile {
    path: String,
    text: String,
}

#[derive(Deserialize)]
struct StepsRequest {
    text: String,
    line: u32,
    files: Vec<SourceFile>,
}

/// `{ text, line, files }` → `StepDefinition[]` (`behat_step_definitions`).
#[wasm_bindgen]
pub fn behat_step_definitions(request_json: &str) -> String {
    match parse::<StepsRequest>(request_json) {
        Ok(r) => {
            let defs: Vec<StepDefinition> = r
                .files
                .iter()
                .filter(|f| steps::may_define_steps(&f.text))
                .flat_map(|f| steps::definitions(&f.path, &f.text))
                .collect();
            to_json(&steps::step_targets(&r.text, r.line, &defs))
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct ImportRequest {
    tree: StoredNode,
    root: Option<String>,
}

/// `{ tree, root }` → `{ nodes, summary }` (`test_history_open`).
#[wasm_bindgen]
pub fn test_history_import(request_json: &str) -> String {
    match parse::<ImportRequest>(request_json) {
        Ok(r) => {
            let tree = TestTree::import(&r.tree, LinkContext::new(r.root.as_deref(), None));
            to_json(&json!({ "nodes": tree.snapshot(), "summary": tree.summary() }))
        }
        Err(e) => error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_and_reruns_through_json() {
        let out: Value = serde_json::from_str(&test_discover(
            r#"{"path":"features/a.feature","text":"Feature: A\n  Scenario: B\n    Given c\n"}"#,
        ))
        .unwrap();
        assert_eq!(out["framework"], "behat");
        assert_eq!(out["targets"][1]["configuration"]["options"]["line"], "2");
        let rerun: Value = serde_json::from_str(&test_rerun_failed(
            r#"{"configuration":{"name":"f","type":"behat"},"failed":[{"name":"B"}]}"#,
        ))
        .unwrap();
        assert_eq!(rerun["options"]["testRunnerOptions"], "--rerun");
        let list: Value = serde_json::from_str(&test_frameworks_effective(
            r#"{"files":["vendor/bin/pest"]}"#,
        ))
        .unwrap();
        assert_eq!(list[0]["framework"], "pest");
    }
}
