//! Run Anything, launch plans and External Tools (`forge-runtools`) for browser mode,
//! mirroring the `run_anything_*`, `run_launch_plan` and `external_tool_*` IPC commands.
//! The mocks collect what the desktop backend reads from the project (scripts, files).

use forge_runtools::anything::{self, HistoryEntry, SuggestRequest};
use forge_runtools::external::{self, ExternalTool};
use forge_runtools::launch::{self, PlanContext};
use forge_runtools::macros::{self, MacroContext};
use forge_runtools::Configuration;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

fn error(e: String) -> String {
    to_json(&serde_json::json!({ "error": e }))
}

/// `SuggestRequest` (with npm / Composer scripts and PHP files filled in) → `SuggestResponse`.
#[wasm_bindgen]
pub fn run_anything_suggest(request_json: &str) -> String {
    match parse::<SuggestRequest>(request_json) {
        Ok(req) => to_json(&anything::suggest(&req)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct Resolve {
    command: String,
    #[serde(default)]
    cwd: String,
}

/// `{ command, cwd? }` → the temporary `RunConfiguration`.
#[wasm_bindgen]
pub fn run_anything_resolve(request_json: &str) -> String {
    match parse::<Resolve>(request_json).and_then(|r| anything::resolve(&r.command, &r.cwd)) {
        Ok(cfg) => to_json(&cfg),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct PushHistory {
    history: Vec<HistoryEntry>,
    entry: HistoryEntry,
}

/// `{ history, entry }` → the new history.
#[wasm_bindgen]
pub fn run_anything_push_history(request_json: &str) -> String {
    match parse::<PushHistory>(request_json) {
        Ok(r) => to_json(&anything::push_history(&r.history, r.entry)),
        Err(e) => error(e),
    }
}

/// Composer script names of a `composer.json` text.
#[wasm_bindgen]
pub fn composer_scripts(composer_json: &str) -> String {
    to_json(&anything::composer_scripts(composer_json))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Plan {
    configurations: Vec<Configuration>,
    target: String,
    #[serde(default)]
    package_manager: String,
}

/// `{ configurations, target, packageManager? }` → `LaunchPlan`.
#[wasm_bindgen]
pub fn run_launch_plan(request_json: &str) -> String {
    match parse::<Plan>(request_json).and_then(|p| {
        launch::plan(
            &p.configurations,
            &p.target,
            &PlanContext {
                package_manager: p.package_manager,
            },
        )
    }) {
        Ok(plan) => to_json(&plan),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolRequest {
    #[serde(default)]
    tool: Option<ExternalTool>,
    #[serde(default)]
    context: MacroContext,
    #[serde(default)]
    root: String,
    #[serde(default)]
    name: String,
}

/// `{ context, root, name }` → `MacroInfo[]`.
#[wasm_bindgen]
pub fn external_tool_macros(request_json: &str) -> String {
    match parse::<ToolRequest>(request_json) {
        Ok(r) => to_json(&macros::list(&r.context.with_project(&r.root, &r.name))),
        Err(e) => error(e),
    }
}

/// `{ tool, context, root, name }` → `ToolCommand`.
#[wasm_bindgen]
pub fn external_tool_command(request_json: &str) -> String {
    match parse::<ToolRequest>(request_json).and_then(|r| {
        let tool = r.tool.ok_or("no tool")?;
        external::command(&tool, &r.context.with_project(&r.root, &r.name))
    }) {
        Ok(cmd) => to_json(&cmd),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct Validate {
    tool: ExternalTool,
    #[serde(default)]
    others: Vec<ExternalTool>,
}

/// `{ tool, others }` → problems.
#[wasm_bindgen]
pub fn external_tool_validate(request_json: &str) -> String {
    match parse::<Validate>(request_json) {
        Ok(v) => to_json(&external::validate(&v.tool, &v.others)),
        Err(e) => error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_suggestions_plans_and_tools() {
        let out = run_anything_suggest(r#"{"query":"?"}"#);
        assert!(out.contains("\"help\":true"), "{out}");
        let out = run_anything_resolve(r#"{"command":"php artisan migrate"}"#);
        assert!(out.contains("\"type\":\"artisan\""), "{out}");
        let out = run_launch_plan(
            r#"{"configurations":[{"name":"a","type":"shell","beforeLaunch":[{"type":"npm","value":"build"}]}],"target":"a"}"#,
        );
        assert!(out.contains("Run npm script 'build'"), "{out}");
        let out = external_tool_command(
            r#"{"tool":{"name":"ls","program":"ls","arguments":"$FileName$"},"context":{"filePath":"app/User.php"},"root":"/p","name":"p"}"#,
        );
        assert!(out.contains("\"args\":[\"User.php\"]"), "{out}");
        assert!(external_tool_validate(r#"{"tool":{"name":""}}"#).contains("Specify"));
        assert!(external_tool_macros(r#"{"root":"/p"}"#).contains("$ProjectFileDir$"));
        assert_eq!(composer_scripts(r#"{"scripts":{"t":"x"}}"#), r#"["t"]"#);
        assert!(
            run_anything_push_history(r#"{"history":[],"entry":{"command":"x"}}"#)
                .contains("\"x\"")
        );
    }
}
