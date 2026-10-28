//! Ruby on Rails (`rubyforge-fw-rails`), mirroring the `rails_*` IPC commands for the
//! browser-mode mock: the project model (routes, controllers, models, tables, locales,
//! Stimulus, databases), Go to Declaration, completion, gutter marks, inspections and folds,
//! the generator list and the Rails command lines. Each call gets the project's files.

use std::collections::BTreeMap;

use forge_runtools::Configuration;
use rubyforge_fw_rails::{command, editor, generators, Options, Project};
use rubyforge_lang_ruby::RubyContext;
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Request {
    files: BTreeMap<String, String>,
    options: Options,
    path: String,
    content: String,
    offset: usize,
}

fn with_project(request_json: &str, f: impl FnOnce(&Project, &Request) -> String) -> String {
    let req: Request = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let files = req
        .files
        .iter()
        .filter(|(p, _)| rubyforge_fw_rails::wanted(p))
        .map(|(p, t)| (p.clone(), t.clone()))
        .collect();
    let project = Project::new(files, req.options.clone());
    f(&project, &req)
}

/// `{ files, options }` → `RailsInfo`.
#[wasm_bindgen]
pub fn rails_project(request_json: &str) -> String {
    with_project(request_json, |p, _| to_json(&p.info))
}

/// `{ files, path, content, offset }` → `Location[]`.
#[wasm_bindgen]
pub fn rails_goto(request_json: &str) -> String {
    with_project(request_json, |p, r| {
        to_json(&editor::goto(p, &r.path, &r.content, r.offset))
    })
}

/// `{ files, path, content, offset }` → `Completions | null`.
#[wasm_bindgen]
pub fn rails_complete(request_json: &str) -> String {
    with_project(request_json, |p, r| {
        to_json(&editor::complete(p, &r.path, &r.content, r.offset))
    })
}

/// `{ files, path, content }` → `FileMarks`.
#[wasm_bindgen]
pub fn rails_marks(request_json: &str) -> String {
    with_project(request_json, |p, r| {
        to_json(&editor::marks(p, &r.path, &r.content))
    })
}

/// `{ files, output? }` → `Generator[]`: from `bin/rails generate` output, else Rails' own
/// generators and the project's `lib/generators`.
#[wasm_bindgen]
pub fn rails_generators(request_json: &str) -> String {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Req {
        files: Vec<String>,
        output: Option<String>,
    }
    let req: Req = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let mut known = generators::built_in();
    known.extend(generators::project_generators(
        req.files.iter().map(String::as_str),
    ));
    match req.output {
        Some(out) => to_json(&generators::parse_generate_output(&out, &known)),
        None => to_json(&known),
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct CommandRequest {
    context: RubyContext,
    binstub: bool,
    /// `server`, `console`, `dbconsole`, `generate`, `destroy`, `routes`.
    kind: String,
    configuration: Option<Configuration>,
    environment: String,
    sandbox: bool,
    generator: String,
    arguments: String,
    options: Vec<String>,
}

/// A Rails command line → `RubyCommand`.
#[wasm_bindgen]
pub fn rails_command(request_json: &str) -> String {
    let r: CommandRequest = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let c = match r.kind.as_str() {
        "server" => {
            command::server_command(&r.configuration.unwrap_or_default(), &r.context, r.binstub)
        }
        "console" => command::console_command(&r.context, r.binstub, &r.environment, r.sandbox),
        "dbconsole" => command::dbconsole_command(&r.context, r.binstub, &r.environment),
        "generate" | "destroy" => command::generate_command(
            &r.context,
            r.binstub,
            &r.generator,
            &r.arguments,
            &r.options,
            r.kind == "destroy",
        ),
        "routes" => Ok(command::routes_command(&r.context, r.binstub)),
        other => Err(format!("unknown Rails command '{other}'")),
    };
    match c {
        Ok(c) => to_json(&c),
        Err(e) => error(e),
    }
}
