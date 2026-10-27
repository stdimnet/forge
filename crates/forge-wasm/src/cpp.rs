//! CForge (`cforge-cmake`, `cforge-lang-cpp`), mirroring the `cmake_*` / `cpp_*` IPC commands
//! for the browser-mode mock: build problems with project paths, CMakeLists completion,
//! profiles from the settings and `CMakePresets.json`, and configure log messages. The
//! GoogleTest, Catch2 and CTest providers are in the test framework registry
//! (`testing.rs`).

use std::collections::BTreeMap;

use cforge_cmake::{MacroContext, ProfileEntry};
use cforge_lang_cpp::diagnostics::{self, BuildProblem};
use serde::{Deserialize, Serialize};
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
struct ProblemsRequest {
    output: String,
    /// Absolute project root.
    root: String,
    /// Project-relative build folder.
    #[serde(default)]
    build_dir: String,
    /// Project files (relative), for relative paths of the output.
    #[serde(default)]
    files: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProblemView {
    path: Option<String>,
    #[serde(flatten)]
    problem: BuildProblem,
    notes: Vec<ProblemView>,
}

fn view(mut p: BuildProblem, resolve: &dyn Fn(&str) -> Option<String>) -> ProblemView {
    let notes = std::mem::take(&mut p.notes)
        .into_iter()
        .map(|n| view(n, resolve))
        .collect();
    ProblemView {
        path: resolve(&p.file),
        problem: p,
        notes,
    }
}

/// `{ output, root, buildDir, files }` → build problems with project paths
/// (`cpp_build_problems`).
#[wasm_bindgen]
pub fn cpp_build_problems(request_json: &str) -> String {
    match parse::<ProblemsRequest>(request_json) {
        Ok(r) => {
            let exists = |p: &str| r.files.iter().any(|f| f == p);
            let resolve = |f: &str| diagnostics::project_path(f, &r.root, &r.build_dir, &exists);
            let list: Vec<ProblemView> = diagnostics::parse_build_output(&r.output)
                .into_iter()
                .map(|p| view(p, &resolve))
                .collect();
            to_json(&list)
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct CompleteRequest {
    text: String,
    offset: usize,
}

/// `{ text, offset }` → completion or null (`cmake_complete`).
#[wasm_bindgen]
pub fn cmake_complete(request_json: &str) -> String {
    match parse::<CompleteRequest>(request_json) {
        Ok(r) => to_json(&cforge_cmake::complete::complete(&r.text, r.offset)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfilesRequest {
    #[serde(default)]
    entries: Vec<ProfileEntry>,
    /// Project files by relative path (the presets and what they include).
    #[serde(default)]
    files: BTreeMap<String, String>,
    root: String,
}

/// `{ entries, files, root }` → `{ profiles, presetFiles, presetErrors }` (`cmake_profiles`
/// without the active one).
#[wasm_bindgen]
pub fn cmake_profiles(request_json: &str) -> String {
    match parse::<ProfilesRequest>(request_json) {
        Ok(r) => {
            let presets = cforge_cmake::presets::load(&|p| r.files.get(p).cloned());
            let ctx = MacroContext {
                source_dir: r.root.clone(),
                host_system_name: "Linux".into(),
                env: BTreeMap::new(),
            };
            to_json(&json!({
                "profiles": cforge_cmake::profiles(&r.entries, &presets, &ctx),
                "presetFiles": presets.files,
                "presetErrors": presets.errors,
            }))
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct LogRequest {
    text: String,
}

/// `{ text }` → configure messages (`cmake_reload`'s `messages`).
#[wasm_bindgen]
pub fn cmake_parse_log(request_json: &str) -> String {
    match parse::<LogRequest>(request_json) {
        Ok(r) => to_json(&cforge_cmake::parse_log(&r.text)),
        Err(e) => error(e),
    }
}
