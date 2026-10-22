//! Command Line Tool Support (`phpforge-cli-tools`) for the browser-mode mocks of the
//! `cli_tools_*` IPC commands: the effective tools, parsing of recorded command lists,
//! completion and the command line of a console input, computed like the desktop app.

use std::collections::HashMap;

use phpforge_cli_tools::definitions::{parse_symfony_list, parse_wp_cli_dump, CommandDef};
use phpforge_cli_tools::invoke::{parse_input, run_invocation};
use phpforge_cli_tools::model::{
    detect, effective_tools, tools_from_setting, ListFormat, Tool, ToolKind,
};
use serde::Deserialize;
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ToolsRequest {
    /// Value of `commandLineTools.tools`.
    configured: Option<Value>,
    auto_detect: Option<bool>,
    /// Project-relative paths that exist.
    files: Vec<String>,
    /// `php.composer.executable`.
    composer: String,
}

/// `{ configured, autoDetect, files, composer }` → `Tool[]` (`cli_tools_list`).
#[wasm_bindgen]
pub fn cli_tools_effective(request_json: &str) -> String {
    match parse::<ToolsRequest>(request_json) {
        Ok(r) => {
            let detected = detect(&|p| r.files.iter().any(|f| f == p), &r.composer);
            to_json(&effective_tools(
                tools_from_setting(r.configured.as_ref()),
                detected,
                r.auto_detect.unwrap_or(true),
            ))
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct ListRequest {
    kind: String,
    text: String,
}

/// `{ kind, text }` → `CommandDef[]`: a recorded `list --format=json` / `wp cli cmd-dump`.
#[wasm_bindgen]
pub fn cli_tools_parse_list(request_json: &str) -> String {
    let r = match parse::<ListRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let Some(kind) = ToolKind::from_id(&r.kind) else {
        return error(format!("unknown tool kind {}", r.kind));
    };
    let parsed = match kind.format() {
        ListFormat::WpCli => parse_wp_cli_dump(&r.text),
        ListFormat::Symfony => parse_symfony_list(&r.text),
        ListFormat::None => Some(Vec::new()),
    };
    match parsed {
        Some(c) => to_json(&c),
        None => error("not a command list"),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompleteRequest {
    input: String,
    offset: usize,
    tools: Vec<Tool>,
    #[serde(default)]
    commands: HashMap<String, Vec<CommandDef>>,
}

/// `{ input, offset, tools, commands }` → `Completion` (`cli_tools_complete`).
#[wasm_bindgen]
pub fn cli_tools_complete(request_json: &str) -> String {
    match parse::<CompleteRequest>(request_json) {
        Ok(r) => to_json(&phpforge_cli_tools::complete(
            &r.input,
            r.offset,
            &r.tools,
            &r.commands,
        )),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct InputRequest {
    input: String,
    tools: Vec<Tool>,
}

/// `{ input, tools }` → `{ tool, args, invocation, commandLine }`: what `cli_tools_run`
/// starts (before the interpreter wraps it).
#[wasm_bindgen]
pub fn cli_tools_parse_input(request_json: &str) -> String {
    let r = match parse::<InputRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    match parse_input(&r.input, &r.tools) {
        Ok(p) => {
            let inv = run_invocation(p.tool, &p.args);
            to_json(&json!({
                "tool": p.tool,
                "args": p.args,
                "commandLine": inv.display(),
                "invocation": inv,
            }))
        }
        Err(e) => error(e),
    }
}
