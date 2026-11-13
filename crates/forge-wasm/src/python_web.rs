//! PyForge web frameworks (`pyforge-frameworks`), mirroring the `python_web_*` IPC
//! commands for the browser-mode mock: the project model (routes, models, tasks,
//! templates), template / URL name navigation, completion, gutter marks and inspections,
//! the static manage.py commands and the run configurations' command lines. Each call gets
//! the project's files; the analysis is pure.

use std::collections::BTreeMap;

use forge_runtools::Configuration;
use pyforge_frameworks::{django, run, templates, Options, Project};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    files: BTreeMap<String, String>,
    #[serde(default)]
    options: Options,
    #[serde(default)]
    path: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    configuration: Option<Configuration>,
}

fn with_project(request_json: &str, f: impl FnOnce(&Project, &Request) -> String) -> String {
    let req: Request = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let project = Project::new(req.files.clone(), req.options.clone());
    f(&project, &req)
}

/// `{ files, options }` -> `WebProject`.
#[wasm_bindgen]
pub fn python_web_project(request_json: &str) -> String {
    with_project(request_json, |p, _| to_json(&p.info))
}

/// `{ files, options, path, content, offset }` -> `Location[]`.
#[wasm_bindgen]
pub fn python_web_goto(request_json: &str) -> String {
    with_project(request_json, |p, r| {
        to_json(&templates::goto(p, &r.path, &r.content, r.offset))
    })
}

/// `{ files, options, path, content, offset }` -> `Completions | null`.
#[wasm_bindgen]
pub fn python_web_complete(request_json: &str) -> String {
    with_project(request_json, |p, r| {
        to_json(&templates::complete(p, &r.path, &r.content, r.offset))
    })
}

/// `{ files, options, path, content }` -> `FileMarks`.
#[wasm_bindgen]
pub fn python_web_marks(request_json: &str) -> String {
    with_project(request_json, |p, r| {
        to_json(&templates::marks(p, &r.path, &r.content))
    })
}

/// `{ files, options }` -> Django's commands plus the project's `management/commands`.
#[wasm_bindgen]
pub fn python_web_manage_commands(request_json: &str) -> String {
    with_project(request_json, |p, _| to_json(&django::static_commands(p)))
}

/// `{ files, options, configuration }` -> `PythonRun`.
#[wasm_bindgen]
pub fn python_web_run_info(request_json: &str) -> String {
    with_project(request_json, |p, r| match &r.configuration {
        Some(cfg) => match run::build(cfg, &p.info) {
            Ok(run) => to_json(&run),
            Err(e) => error(e),
        },
        None => error("no configuration"),
    })
}
