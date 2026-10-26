//! PyForge (`pyforge-lang-python`, `pyforge-interpreters`), mirroring the `python_*` IPC
//! commands for the browser-mode mock: requirements inspections, completion contexts,
//! "Add requirement" edits, dependency files, run markers and interpreter labels.

use pyforge_interpreters::PythonInterpreter;
use pyforge_lang_python::{completion, deps, run, ImportContext, InstalledPackage};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InspectRequest {
    path: String,
    content: String,
    installed: Option<Vec<InstalledPackage>>,
    #[serde(default)]
    imports: ImportContext,
}

/// `{ path, content, installed?, imports }` -> `Problem[]`.
#[wasm_bindgen]
pub fn python_inspect(request_json: &str) -> String {
    let req: InspectRequest = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let problems = pyforge_lang_python::inspect(
        &req.path,
        &req.content,
        req.installed.as_deref(),
        &req.imports,
    )
    .unwrap_or_default();
    to_json(&problems)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileRequest {
    path: String,
    content: String,
    #[serde(default)]
    offset: u32,
}

/// `{ path, content, offset }` -> completion context or null.
#[wasm_bindgen]
pub fn python_completion_context(request_json: &str) -> String {
    let req: FileRequest = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let ctx = if req.path.ends_with("pyproject.toml") {
        completion::pyproject_context(&req.content, req.offset)
    } else if deps::is_requirements_file(&req.path) {
        completion::requirements_context(&req.content, req.offset)
    } else {
        None
    };
    to_json(&ctx)
}

/// `{ path, content }` -> the file's declared dependencies.
#[wasm_bindgen]
pub fn python_dependencies(request_json: &str) -> String {
    let req: FileRequest = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    to_json(&if req.path.ends_with("pyproject.toml") {
        deps::pyproject(&req.content)
    } else {
        deps::requirements_txt(&req.content)
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddRequest {
    target: String,
    text: String,
    requirement: String,
}

/// `{ target, text, requirement }` -> the new text.
#[wasm_bindgen]
pub fn python_add_requirement(request_json: &str) -> String {
    let req: AddRequest = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    if req.target.ends_with("pyproject.toml") {
        match deps::add_to_pyproject(&req.text, &req.requirement) {
            Some(t) => to_json(&t),
            None => error("pyproject.toml has no [project] or Poetry dependencies"),
        }
    } else {
        to_json(&deps::add_to_requirements(&req.text, &req.requirement))
    }
}

/// `{ content }` -> run markers.
#[wasm_bindgen]
pub fn python_run_markers(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        content: String,
    }
    match serde_json::from_str::<Req>(request_json) {
        Ok(r) => to_json(&run::run_markers(&r.content)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanRequest {
    request: pyforge_interpreters::create::NewEnvironment,
    #[serde(default)]
    files: pyforge_interpreters::create::ProjectFiles,
}

/// `{ request, files }` -> the steps that create a new environment.
#[wasm_bindgen]
pub fn python_environment_plan(request_json: &str) -> String {
    let req: PlanRequest = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    match pyforge_interpreters::create::plan(&req.request, &req.files, false) {
        Ok(steps) => to_json(&steps),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandRequest {
    kind: pyforge_interpreters::EnvKind,
    #[serde(default)]
    uv_lock: bool,
    action: pyforge_packages::Action,
    packages: Vec<String>,
    python: String,
}

/// `{ kind, uvLock, action, packages, python }` -> `{ manager, program, args }`.
#[wasm_bindgen]
pub fn python_packages_command(request_json: &str) -> String {
    let req: CommandRequest = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let manager = pyforge_packages::manager::choose(req.kind, req.uv_lock);
    let (program, args) = pyforge_packages::manager::command(manager, req.action, &req.packages, &req.python);
    to_json(&json!({ "manager": manager, "program": program, "args": args }))
}

/// An interpreter -> its status bar label.
#[wasm_bindgen]
pub fn python_interpreter_label(request_json: &str) -> String {
    match serde_json::from_str::<PythonInterpreter>(request_json) {
        Ok(i) => to_json(&i.label()),
        Err(e) => error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspects_and_completes() {
        let out = python_inspect(
            &json!({ "path": "requirements.txt", "content": "requests>=2\n", "installed": [] })
                .to_string(),
        );
        assert!(
            out.contains("Package requirement 'requests>=2' is not satisfied"),
            "{out}"
        );
        let ctx = python_completion_context(
            &json!({ "path": "requirements.txt", "content": "dj", "offset": 2 }).to_string(),
        );
        assert_eq!(ctx, r#"{"kind":"name","from":0,"prefix":"dj"}"#);
        let added = python_add_requirement(
            &json!({ "target": "requirements.txt", "text": "a", "requirement": "b" }).to_string(),
        );
        assert_eq!(added, r#""a\nb\n""#);
        assert_eq!(
            python_interpreter_label(r#"{"version": "3.12.1"}"#),
            r#""Python 3.12""#
        );
        assert!(
            python_run_markers(r#"{"content": "if __name__ == '__main__':\n    pass"}"#)
                .contains("\"line\":0")
        );
    }
}
