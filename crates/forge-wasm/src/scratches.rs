//! Scratch files (`forge-scratches`), mirroring the pure parts of the `scratches_*` IPC
//! commands for the browser-mode mock: the language list, naming, gutter markers, run
//! command lines and the `.http` parser and response rendering. Storage and sending stay in
//! the mock (in memory, canned responses).

use std::cell::RefCell;
use std::collections::BTreeMap;

use forge_filetypes::{FileTypeRegistry, FileTypeSpec};
use forge_scratches::http::{self, HttpResponse};
use forge_scratches::{languages, naming, run, LanguageRequest, RunContext};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

thread_local! {
    static REGISTRY: RefCell<FileTypeRegistry> = RefCell::new(FileTypeRegistry::with_builtins());
}

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

fn parse<'a, T: Deserialize<'a>>(text: &'a str) -> Result<T, String> {
    serde_json::from_str(text).map_err(error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginFileTypes {
    id: String,
    #[serde(default)]
    contributions: Contributions,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Contributions {
    #[serde(default)]
    file_types: Vec<FileTypeSpec>,
}

/// Register the plugins' file types (`PluginInfo[]` JSON) for the language list.
#[wasm_bindgen]
pub fn scratches_load(plugins_json: &str) -> String {
    let plugins: Vec<PluginFileTypes> = match parse(plugins_json) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let mut registry = FileTypeRegistry::with_builtins();
    for p in plugins {
        for t in p.contributions.file_types {
            registry.register(t, Some(&p.id));
        }
    }
    let count = languages::languages(&registry).len();
    REGISTRY.with(|r| *r.borrow_mut() = registry);
    to_json(&count)
}

/// `LanguageRequest` → `LanguageItem[]` (the New Scratch File popup).
#[wasm_bindgen]
pub fn scratches_languages(request_json: &str) -> String {
    match parse::<LanguageRequest>(request_json) {
        Ok(req) => {
            REGISTRY.with(|r| to_json(&languages::list(languages::languages(&r.borrow()), &req)))
        }
        Err(e) => e,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewName {
    /// File type id of the new scratch.
    language: String,
    existing: Vec<String>,
}

/// `{ language, existing }` → `{ name, extension }` of a new scratch (`scratch_1.php`).
#[wasm_bindgen]
pub fn scratches_new_name(request_json: &str) -> String {
    let req: NewName = match parse(request_json) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let Some(ext) = REGISTRY.with(|r| languages::extension_for(&r.borrow(), &req.language)) else {
        return error(format!("unknown language '{}'", req.language));
    };
    let name = naming::next_name(&req.existing, naming::base_name(&ext), &ext);
    let template = forge_scratches::store::template_for(&ext);
    to_json(&json!({ "name": name, "extension": ext, "template": template }))
}

/// `{ existing: [name, modified][] }` → `{ name, reused }` for "New Scratch Buffer".
#[wasm_bindgen]
pub fn scratches_next_buffer(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        existing: Vec<(String, u64)>,
    }
    match parse::<Req>(request_json) {
        Ok(r) => {
            let (name, reused) = naming::next_buffer(&r.existing);
            to_json(&json!({ "name": name, "reused": reused }))
        }
        Err(e) => e,
    }
}

/// `{ name }` → `null` or the reason the name is invalid.
#[wasm_bindgen]
pub fn scratches_validate_name(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        name: String,
    }
    match parse::<Req>(request_json) {
        Ok(r) => to_json(&naming::validate_name(&r.name).err().map(|e| e.to_string())),
        Err(e) => e,
    }
}

/// `{ name }` → the scratch's runner (`php`, `http`, …) or `null`.
#[wasm_bindgen]
pub fn scratches_runner(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        name: String,
    }
    match parse::<Req>(request_json) {
        Ok(r) => to_json(&run::runner_for(&r.name)),
        Err(e) => e,
    }
}

/// `{ name, content }` → gutter markers.
#[wasm_bindgen]
pub fn scratches_markers(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        name: String,
        content: String,
    }
    match parse::<Req>(request_json) {
        Ok(r) => to_json(&run::markers(&r.name, &r.content)),
        Err(e) => e,
    }
}

/// `{ file, context: RunContext }` → `RunCommand`.
#[wasm_bindgen]
pub fn scratches_run_command(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        file: String,
        #[serde(default)]
        context: RunContext,
    }
    match parse::<Req>(request_json) {
        Ok(r) => match run::run_command(&r.file, &r.context) {
            Ok(cmd) => to_json(&cmd),
            Err(e) => error(e),
        },
        Err(e) => e,
    }
}

/// `{ content, line, env? }` → the resolved request at `line` (`HttpRequest`) or `null`.
#[wasm_bindgen]
pub fn scratches_http_request(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        content: String,
        line: usize,
        #[serde(default)]
        env: BTreeMap<String, String>,
    }
    match parse::<Req>(request_json) {
        Ok(r) => {
            let file = http::parse(&r.content);
            let req = file
                .request_at(r.line)
                .map(|req| http::resolve(req, &file.variables, &r.env));
            to_json(&req)
        }
        Err(e) => e,
    }
}

/// `{ request?: HttpRequest, response?: HttpResponse, error?: string }` → console text.
#[wasm_bindgen]
pub fn scratches_http_render(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        request: Option<http::HttpRequest>,
        response: Option<HttpResponse>,
        error: Option<String>,
    }
    match parse::<Req>(request_json) {
        Ok(r) => {
            let mut out = String::new();
            if let Some(req) = &r.request {
                out.push_str(&http::render_request(req));
            }
            if let Some(resp) = &r.response {
                out.push_str(&http::render_response(resp));
            }
            if let Some(err) = &r.error {
                out.push_str(&http::render_error(err));
            }
            to_json(&out)
        }
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_names_and_markers() {
        let n = scratches_load(
            r#"[{"id":"forge.lua","contributions":{"fileTypes":[{"id":"lua","name":"Lua","extensions":["lua"]}]}}]"#,
        );
        assert!(n.parse::<usize>().unwrap() > 40, "{n}");
        let items: serde_json::Value =
            serde_json::from_str(&scratches_languages(r#"{"query":"","current":"php"}"#)).unwrap();
        assert_eq!(items[0]["id"], "php");
        assert_eq!(items[0]["extension"], "php");
        let name: serde_json::Value = serde_json::from_str(&scratches_new_name(
            r#"{"language":"php","existing":["scratch.php"]}"#,
        ))
        .unwrap();
        assert_eq!(name["name"], "scratch_1.php");
        assert_eq!(name["template"], "<?php\n\n");
        assert!(scratches_new_name(r#"{"language":"nope","existing":[]}"#).contains("error"));
        assert_eq!(scratches_runner(r#"{"name":"a.http"}"#), r#""http""#);
        assert_eq!(scratches_validate_name(r#"{"name":"ok.php"}"#), "null");
        let markers = scratches_markers(r#"{"name":"rest-api.http","content":"GET http://x\n"}"#);
        assert!(markers.contains(r#""kind":"http""#), "{markers}");
    }

    #[test]
    fn http_request_and_rendering() {
        let req = scratches_http_request(
            r#"{"content":"@h = localhost\n###\nGET {{h}}/a\n###\nPOST {{h}}/b\n","line":4}"#,
        );
        let v: serde_json::Value = serde_json::from_str(&req).unwrap();
        assert_eq!(v["url"], "http://localhost/b");
        let text = scratches_http_render(&format!(
            r#"{{"request":{req},"response":{{"version":"HTTP/1.1","status":200,"reason":"OK","body":"[]","size":2,"elapsedMs":5}}}}"#
        ));
        assert!(text.contains("POST http://localhost/b"), "{text}");
        assert!(text.contains("Response code: 200 (OK)"), "{text}");
        let cmd = scratches_run_command(
            r#"{"file":"/s/scratch.php","context":{"phpInterpreter":"php","autoloader":"/p/vendor/autoload.php","projectRoot":"/p"}}"#,
        );
        assert!(
            cmd.contains("auto_prepend_file=/p/vendor/autoload.php"),
            "{cmd}"
        );
    }
}
