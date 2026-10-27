//! DotNetForge (`dotnetforge-lang-dotnet`), mirroring the `dotnet_*` and `nuget_*` IPC
//! commands for the browser-mode mock: the solution and its projects from the demo files,
//! build output as problems, test discovery, run / build / NuGet / template / cleanup
//! command lines, `dotnet new list`, NuGet search answers and upgrades.

use std::collections::BTreeMap;

use dotnetforge_lang_dotnet::command::{self as cmdline, FormatRequest, NewProject};
use dotnetforge_lang_dotnet::DotnetContext;
use forge_runtools::Configuration;
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
struct WorkspaceRequest {
    /// Project files by path (solutions, projects, props, launch settings, global.json).
    files: BTreeMap<String, String>,
    #[serde(default)]
    solution: String,
    #[serde(default)]
    configuration: String,
}

/// `{ files, solution?, configuration? }` → `DotnetWorkspace` (`dotnet_workspace`).
#[wasm_bindgen]
pub fn dotnet_load_workspace(request_json: &str) -> String {
    match parse::<WorkspaceRequest>(request_json) {
        Ok(r) => {
            let paths: Vec<String> = r.files.keys().cloned().collect();
            let read = |p: &str| r.files.get(p).cloned();
            to_json(&dotnetforge_lang_dotnet::load_workspace(
                &paths,
                &read,
                &r.solution,
                &r.configuration,
            ))
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct BuildRequest {
    output: String,
    root: String,
}

/// `{ output, root }` → `BuildReport` (`dotnet_build_problems`).
#[wasm_bindgen]
pub fn dotnet_build_report(request_json: &str) -> String {
    match parse::<BuildRequest>(request_json) {
        Ok(r) => to_json(&dotnetforge_lang_dotnet::parse_build_output(&r.output, &r.root)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct FileRequest {
    path: String,
    text: String,
    #[serde(default)]
    context: Option<DotnetContext>,
}

/// `{ path, text, context? }` → `TestFile` (`test_discover` for C# / F# files).
#[wasm_bindgen]
pub fn dotnet_discover(request_json: &str) -> String {
    match parse::<FileRequest>(request_json) {
        Ok(r) => to_json(&dotnetforge_lang_dotnet::discover_file(&r.path, &r.text, r.context.as_ref())),
        Err(e) => error(e),
    }
}

/// `{ path, text }` → `SourceScan` (test classes, entry points, declared classes).
#[wasm_bindgen]
pub fn dotnet_scan(request_json: &str) -> String {
    match parse::<FileRequest>(request_json) {
        Ok(r) => to_json(&dotnetforge_lang_dotnet::scan::scan_source(&r.path, &r.text, None)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct RunRequest {
    configuration: Configuration,
    #[serde(default)]
    context: DotnetContext,
}

/// `{ configuration, context }` → `DotnetCommand` of a ".NET Project" configuration.
#[wasm_bindgen]
pub fn dotnet_run_command(request_json: &str) -> String {
    match parse::<RunRequest>(request_json).and_then(|r| cmdline::run_command(&r.configuration, &r.context)) {
        Ok(c) => to_json(&c),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct ActionRequest {
    action: String,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    context: DotnetContext,
}

/// `{ action, target?, context }` → `DotnetCommand` (`dotnet_command`).
#[wasm_bindgen]
pub fn dotnet_action_command(request_json: &str) -> String {
    match parse::<ActionRequest>(request_json)
        .and_then(|r| cmdline::action_command(&r.action, r.target.as_deref(), &r.context))
    {
        Ok(c) => to_json(&c),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct PackageRequest {
    action: String,
    project: String,
    package: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    context: DotnetContext,
}

/// `{ action, project, package, version?, prerelease?, context }` → `DotnetCommand`
/// (`nuget_command`).
#[wasm_bindgen]
pub fn dotnet_package_command(request_json: &str) -> String {
    match parse::<PackageRequest>(request_json).and_then(|r| {
        cmdline::package_command(&r.action, &r.project, &r.package, r.version.as_deref(), r.prerelease, &r.context)
    }) {
        Ok(c) => to_json(&c),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct NewRequest {
    request: NewProject,
    #[serde(default)]
    context: DotnetContext,
}

/// `{ request, context }` → `DotnetCommand[]` (`dotnet_new_project`).
#[wasm_bindgen]
pub fn dotnet_new_project(request_json: &str) -> String {
    match parse::<NewRequest>(request_json).and_then(|r| cmdline::new_project_commands(&r.request, &r.context)) {
        Ok(c) => to_json(&c),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct FormatCommandRequest {
    request: FormatRequest,
    #[serde(default)]
    context: DotnetContext,
}

/// `{ request, context }` → `DotnetCommand` (`dotnet_cleanup_command`).
#[wasm_bindgen]
pub fn dotnet_format_command(request_json: &str) -> String {
    match parse::<FormatCommandRequest>(request_json).and_then(|r| cmdline::format_command(&r.request, &r.context)) {
        Ok(c) => to_json(&c),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct TextRequest {
    text: String,
}

/// `{ text }` → `Template[]` of `dotnet new list` (Forge's common templates when empty).
#[wasm_bindgen]
pub fn dotnet_templates(request_json: &str) -> String {
    match parse::<TextRequest>(request_json) {
        Ok(r) => {
            let t = dotnetforge_lang_dotnet::templates::parse_template_list(&r.text);
            to_json(&if t.is_empty() {
                dotnetforge_lang_dotnet::templates::common_templates()
            } else {
                t
            })
        }
        Err(e) => error(e),
    }
}

/// `{ text }` → `SearchResult` of a NuGet search answer.
#[wasm_bindgen]
pub fn nuget_parse_search(request_json: &str) -> String {
    match parse::<TextRequest>(request_json).and_then(|r| dotnetforge_lang_dotnet::nuget::parse_search(&r.text)) {
        Ok(s) => to_json(&s),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct UpgradeRequest {
    installed: String,
    versions: Vec<String>,
    #[serde(default)]
    prerelease: bool,
}

/// `{ installed, versions, prerelease }` → the upgrade version or `null`.
#[wasm_bindgen]
pub fn nuget_upgrade(request_json: &str) -> String {
    match parse::<UpgradeRequest>(request_json) {
        Ok(r) => to_json(&dotnetforge_lang_dotnet::version::upgrade(&r.installed, &r.versions, r.prerelease)),
        Err(e) => error(e),
    }
}
