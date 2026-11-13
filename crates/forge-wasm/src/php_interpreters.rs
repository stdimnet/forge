//! PHP CLI interpreters (`phpforge-interpreters`) for the browser-mode mocks: the command
//! line a run configuration gets through an interpreter (Docker, Compose, SSH, WSL), the
//! automatic path mappings and the Reload probes, computed exactly like the desktop app.

use std::collections::BTreeMap;

use phpforge_interpreters::mapping::{self, Mapper, PathMapping};
use phpforge_interpreters::{
    wrap, InterpreterInfo, InterpreterSet, Invocation, PhpInterpreter, Program, SshTarget,
    WrapContext,
};
use serde::Deserialize;
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct WrapRequest {
    /// Values of `php.interpreters` and `php.projectInterpreters`.
    global: Option<Value>,
    project: Option<Value>,
    /// A name or a local path (`php.interpreter` or the configuration's interpreter).
    selected: String,
    program: Program,
    args: Vec<String>,
    cwd: String,
    env: BTreeMap<String, String>,
    root: String,
    /// The compose file's directory and the service's volumes (demo data).
    compose_dir: Option<String>,
    volumes: Vec<String>,
    ssh: Option<SshTarget>,
}

/// `WrapRequest` → `{ program, args, cwd, env, kind, pathMappings }`.
#[wasm_bindgen]
pub fn php_interpreter_wrap(request_json: &str) -> String {
    let r: WrapRequest = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let set = InterpreterSet::from_settings(r.global.as_ref(), r.project.as_ref());
    let interp = set.resolve(&r.selected);
    let automatic = mapping::automatic(
        &interp,
        &r.root,
        r.compose_dir.as_deref().map(|d| (d, r.volumes.as_slice())),
    );
    let ctx = WrapContext {
        mappings: mapping::effective(&interp, automatic),
        root: r.root.clone(),
        docker: String::new(),
        ssh: r.ssh,
    };
    let inv = Invocation {
        program: r.program,
        args: r.args,
        cwd: r.cwd,
        env: r.env,
        publish: Vec::new(),
    };
    match wrap(&interp, &inv, &ctx) {
        Ok(w) => {
            let mappings: Vec<Value> = if interp.kind.is_remote() {
                Mapper::new(&ctx.root, &ctx.mappings)
                    .pairs()
                    .iter()
                    .map(|(l, rem)| json!({ "remote": rem, "local": l }))
                    .collect()
            } else {
                Vec::new()
            };
            to_json(&json!({
                "program": w.program,
                "args": w.args,
                "cwd": w.cwd,
                "env": w.env,
                "kind": interp.kind,
                "pathMappings": mappings,
            }))
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct MappingsRequest {
    interpreter: PhpInterpreter,
    root: String,
    compose_dir: Option<String>,
    volumes: Vec<String>,
}

/// `{ interpreter, root, composeDir?, volumes? }` → the automatic `PathMapping[]`.
#[wasm_bindgen]
pub fn php_interpreter_automatic_mappings(request_json: &str) -> String {
    match serde_json::from_str::<MappingsRequest>(request_json) {
        Ok(r) => {
            let list: Vec<PathMapping> = mapping::automatic(
                &r.interpreter,
                &r.root,
                r.compose_dir.as_deref().map(|d| (d, r.volumes.as_slice())),
            );
            to_json(&list)
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ProbeOutputs {
    php_v: String,
    php_ini: String,
    php_m: String,
}

/// `{ phpV, phpIni, phpM }` (outputs of `php -v`, `php --ini`, `php -m`) → `InterpreterInfo`.
#[wasm_bindgen]
pub fn php_interpreter_parse_info(request_json: &str) -> String {
    match serde_json::from_str::<ProbeOutputs>(request_json) {
        Ok(o) => match InterpreterInfo::from_outputs(&o.php_v, &o.php_ini, &o.php_m) {
            Ok(info) => to_json(&info),
            Err(e) => error(e),
        },
        Err(e) => error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_through_a_docker_interpreter() {
        let out: Value = serde_json::from_str(&php_interpreter_wrap(
            &json!({
                "global": [{ "name": "PHP 8.3 (Docker)", "kind": "docker", "image": "php:8.3-cli" }],
                "selected": "PHP 8.3 (Docker)",
                "program": { "type": "php" },
                "args": ["vendor/bin/phpunit", "--teamcity"],
                "cwd": "/p",
                "root": "/p",
            })
            .to_string(),
        ))
        .unwrap();
        assert_eq!(out["program"], "docker");
        assert_eq!(out["pathMappings"][0]["remote"], "/opt/project");
        let info: Value = serde_json::from_str(&php_interpreter_parse_info(
            &json!({ "phpV": "PHP 8.2.1 (cli)", "phpIni": "", "phpM": "[PHP Modules]\nCore\n" })
                .to_string(),
        ))
        .unwrap();
        assert_eq!(info["version"], "8.2.1");
    }
}
