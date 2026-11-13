//! RustForge (`rustforge-lang-rust`), mirroring the `rust_*` and `cargo_*` IPC commands for
//! the browser-mode mock: Cargo workspaces from `cargo metadata`, test discovery with the
//! workspace's packages, `fn main` run targets, Cargo command lines, Clippy problems and
//! toolchain files.

use forge_runtools::Configuration;
use rustforge_lang_rust::{CargoContext, Selection};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetadataRequest {
    json: String,
    root: String,
}

/// `{ json, root }` → `CargoWorkspace` (`cargo_workspace`).
#[wasm_bindgen]
pub fn rust_parse_metadata(request_json: &str) -> String {
    match parse::<MetadataRequest>(request_json)
        .and_then(|r| rustforge_lang_rust::parse_metadata(&r.json, &r.root))
    {
        Ok(ws) => to_json(&ws),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileRequest {
    path: String,
    text: String,
    #[serde(default)]
    context: Option<CargoContext>,
}

/// `{ path, text, context? }` → `TestFile` (`test_discover` for Rust files).
#[wasm_bindgen]
pub fn rust_discover(request_json: &str) -> String {
    match parse::<FileRequest>(request_json) {
        Ok(r) => to_json(&rustforge_lang_rust::discover_file(
            &r.path,
            &r.text,
            r.context.as_ref(),
        )),
        Err(e) => error(e),
    }
}

/// `{ path, text, context? }` → `RunTarget[]` (`rust_file_targets`).
#[wasm_bindgen]
pub fn rust_run_targets(request_json: &str) -> String {
    match parse::<FileRequest>(request_json) {
        Ok(r) => to_json(&rustforge_lang_rust::run_targets(
            &r.path,
            &r.text,
            r.context.as_ref(),
        )),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct CommandRequest {
    configuration: Configuration,
    #[serde(default)]
    context: CargoContext,
}

/// `{ configuration, context }` → `CargoCommand` of a "Cargo" configuration.
#[wasm_bindgen]
pub fn rust_command_line(request_json: &str) -> String {
    match parse::<CommandRequest>(request_json)
        .and_then(|r| rustforge_lang_rust::command_line(&r.configuration, &r.context))
    {
        Ok(c) => to_json(&c),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct ActionRequest {
    action: String,
    #[serde(default)]
    package: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    context: CargoContext,
}

/// `{ action, package?, target?, context }` → `CargoCommand` (`cargo_command`).
#[wasm_bindgen]
pub fn rust_action_command(request_json: &str) -> String {
    match parse::<ActionRequest>(request_json).and_then(|r| {
        let sel = Selection {
            package: r.package,
            target: r.target,
            ..Selection::default()
        };
        rustforge_lang_rust::action_command(&r.action, &sel, &r.context)
    }) {
        Ok(c) => to_json(&c),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct ProblemsRequest {
    json: String,
    #[serde(default)]
    workspace: String,
}

/// `{ json, workspace }` → `RustProblem[]` (`rust_clippy`).
#[wasm_bindgen]
pub fn rust_problems(request_json: &str) -> String {
    match parse::<ProblemsRequest>(request_json) {
        Ok(r) => to_json(&rustforge_lang_rust::problems(&r.json, &r.workspace)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct TextRequest {
    text: String,
}

/// `{ text }` → `ToolchainFile` of a `rust-toolchain.toml`.
#[wasm_bindgen]
pub fn rust_toolchain_file(request_json: &str) -> String {
    match parse::<TextRequest>(request_json)
        .and_then(|r| rustforge_lang_rust::toolchain::parse_toolchain_file(&r.text))
    {
        Ok(f) => to_json(&f),
        Err(e) => error(e),
    }
}
