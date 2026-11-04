//! JavaForge's Minecraft plugin support (`javaforge-minecraft`) for the browser-mode mocks,
//! mirroring the `minecraft_*` IPC commands and the New Project wizard: the generator's
//! files, project detection, the descriptor `main` check and navigation, and gutter markers.

use std::collections::BTreeMap;

use javaforge_minecraft::markers::MarkerKind;
use javaforge_minecraft::{descriptor, markers, Options};
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
struct GenerateRequest {
    name: String,
    #[serde(default)]
    options: serde_json::Value,
}

/// `{ name, options }` → the generated files `[{ path, contents }]` or `{ error }`.
#[wasm_bindgen]
pub fn minecraft_generate(request_json: &str) -> String {
    let result = parse::<GenerateRequest>(request_json).and_then(|r| {
        let options = if r.options.is_null() {
            Options::default()
        } else {
            Options::from_json(&r.options)?
        };
        javaforge_minecraft::generate(&r.name, &options)
    });
    match result {
        Ok(files) => to_json(&files),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EditorRequest {
    path: String,
    content: String,
    #[serde(default)]
    pos: usize,
    /// The project's files (path → text).
    #[serde(default)]
    files: BTreeMap<String, String>,
}

/// `{ path, content, files }` → descriptor problems (`minecraft_inspect`).
#[wasm_bindgen]
pub fn minecraft_inspect(request_json: &str) -> String {
    match parse::<EditorRequest>(request_json) {
        Ok(r) => to_json(&descriptor::inspect(&r.path, &r.content, &|p| {
            r.files.contains_key(p)
        })),
        Err(e) => error(e),
    }
}

/// `{ path, content, pos, files }` → navigation targets (`minecraft_goto`).
#[wasm_bindgen]
pub fn minecraft_goto(request_json: &str) -> String {
    match parse::<EditorRequest>(request_json) {
        Ok(r) => {
            let target = descriptor::goto(&r.path, &r.content, r.pos, &|p| r.files.get(p).cloned());
            to_json(&target.into_iter().collect::<Vec<_>>())
        }
        Err(e) => error(e),
    }
}

/// `{ path, content, files }` → gutter markers (`minecraft_gutter`); the main class's marker
/// only when the build applies run-paper.
#[wasm_bindgen]
pub fn minecraft_gutter(request_json: &str) -> String {
    match parse::<EditorRequest>(request_json) {
        Ok(r) => {
            let builds: Vec<&str> = ["build.gradle.kts", "build.gradle", "pom.xml"]
                .iter()
                .filter_map(|f| r.files.get(*f).map(String::as_str))
                .collect();
            let run_paper = javaforge_minecraft::detect(&builds).is_some_and(|d| d.run_paper);
            let mut out = markers::markers(&r.content, r.path.ends_with(".kt"));
            out.retain(|m| m.kind != MarkerKind::Plugin || run_paper);
            to_json(&out)
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct ChoicesRequest {
    #[serde(default)]
    versions: Vec<String>,
}

/// `{ versions }` (refreshed Minecraft versions) → the wizard's version choices
/// `[{ value, label }]`, the pinned table merged in (`minecraft_versions_refresh`).
#[wasm_bindgen]
pub fn minecraft_version_choices(request_json: &str) -> String {
    match parse::<ChoicesRequest>(request_json) {
        Ok(r) => to_json(
            &javaforge_minecraft::versions::version_choices(&r.versions)
                .into_iter()
                .map(|(value, label)| json!({ "value": value, "label": label }))
                .collect::<Vec<_>>(),
        ),
        Err(e) => error(e),
    }
}
