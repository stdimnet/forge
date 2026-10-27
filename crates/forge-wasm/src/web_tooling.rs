//! Web tooling for the browser-mode mock, mirroring the IPC commands of
//! `commands/web_tooling.rs` and the import commands of `commands/web.rs`: File Watchers
//! (`forge_runtools::watchers`), Vite/Webpack aliases (`webforge_lang_web`), OpenAPI,
//! GraphQL, Nx, Nuxt and Deno (`webforge_web_tooling`), and cURL import/export
//! (`forge_scratches::curl`). The mock passes the demo project's files in.

use std::collections::BTreeMap;

use forge_runtools::watchers::{self, FileWatcher, PlanInput, Trigger};
use serde::Deserialize;
use wasm_bindgen::prelude::*;
use webforge_lang_web::{import_paths, SourceFile, Sources};
use webforge_web_tooling::{deno, graphql, nuxt, nx, openapi};

use crate::parsers::parse;
use crate::to_json;

// ---- Imports and aliases -----------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportRequest {
    #[serde(default)]
    files: Vec<SourceFile>,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    path: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    offset: usize,
}

/// `{ files, paths, path, content, offset }` -> `Location | null`.
#[wasm_bindgen]
pub fn web_import_goto(request_json: &str) -> String {
    match parse::<ImportRequest>(request_json) {
        Ok(r) => {
            let s = Sources::new(r.files, r.paths);
            to_json(&import_paths::goto(&s, &r.path, &r.content, r.offset))
        }
        Err(e) => e,
    }
}

/// `{ files, paths, path, content, offset }` -> `ImportCompletion | null`.
#[wasm_bindgen]
pub fn web_import_complete(request_json: &str) -> String {
    match parse::<ImportRequest>(request_json) {
        Ok(r) => {
            let s = Sources::new(r.files, r.paths);
            to_json(&import_paths::complete(&s, &r.path, &r.content, r.offset))
        }
        Err(e) => e,
    }
}

/// `{ files, paths }` -> `Alias[]`.
#[wasm_bindgen]
pub fn web_aliases(request_json: &str) -> String {
    match parse::<ImportRequest>(request_json) {
        Ok(r) => to_json(&Sources::new(r.files, r.paths).bundler_aliases().to_vec()),
        Err(e) => e,
    }
}

// ---- File Watchers ---------------------------------------------------------------------

/// `{}` -> `WatcherTemplate[]`.
#[wasm_bindgen]
pub fn file_watchers_templates(_request_json: &str) -> String {
    to_json(&watchers::templates())
}

#[derive(Deserialize)]
struct ValidateRequest {
    watcher: FileWatcher,
    #[serde(default)]
    others: Vec<FileWatcher>,
}

/// `{ watcher, others }` -> `string[]`.
#[wasm_bindgen]
pub fn file_watchers_validate(request_json: &str) -> String {
    match parse::<ValidateRequest>(request_json) {
        Ok(r) => to_json(&watchers::validate(&r.watcher, &r.others)),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanRequest {
    /// Settings objects of `tools.fileWatchers`.
    watchers: Vec<BTreeMap<String, serde_json::Value>>,
    paths: Vec<String>,
    trigger: Trigger,
    root: String,
    name: String,
    /// Project files with their text.
    #[serde(default)]
    files: BTreeMap<String, String>,
}

/// `{ watchers, paths, trigger, root, name, files }` -> `WatcherRun[]` (every scope
/// matches in browser mode).
#[wasm_bindgen]
pub fn file_watchers_plan(request_json: &str) -> String {
    let r = match parse::<PlanRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let list: Vec<FileWatcher> = r
        .watchers
        .iter()
        .map(|m| {
            FileWatcher::from_object(
                &m.iter()
                    .map(|(k, v)| {
                        let s = match v {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        (k.clone(), s)
                    })
                    .collect(),
            )
        })
        .collect();
    let files = &r.files;
    let sources = |w: &FileWatcher| -> BTreeMap<String, String> {
        files
            .iter()
            .filter(|(p, _)| w.accepts(p))
            .map(|(p, t)| (p.clone(), t.clone()))
            .collect()
    };
    let local = |name: &str| {
        let rel = format!("node_modules/.bin/{name}");
        files
            .contains_key(&rel)
            .then(|| format!("{}/{rel}", r.root.trim_end_matches('/')))
    };
    to_json(&watchers::plan(
        &list,
        &PlanInput {
            paths: &r.paths,
            trigger: r.trigger,
            root: &r.root,
            project_name: &r.name,
            in_scope: &|_, _| true,
            sources: &sources,
            resolve_program: &local,
        },
    ))
}

#[derive(Deserialize)]
struct ProblemsRequest {
    output: String,
    filters: Vec<String>,
    cwd: String,
    root: String,
}

/// `{ output, filters, cwd, root }` -> `WatcherProblem[]`.
#[wasm_bindgen]
pub fn file_watchers_problems(request_json: &str) -> String {
    match parse::<ProblemsRequest>(request_json) {
        Ok(r) => to_json(&watchers::problems(&r.output, &r.filters, &r.cwd, &r.root)),
        Err(e) => e,
    }
}

// ---- OpenAPI ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocRequest {
    path: String,
    content: String,
    #[serde(default)]
    method: String,
    #[serde(default)]
    op_path: String,
}

/// `{ path, content }` -> `{ title, spec, endpoints } | null`.
#[wasm_bindgen]
pub fn openapi_file(request_json: &str) -> String {
    match parse::<DocRequest>(request_json) {
        Ok(r) => to_json(&openapi::preview(&r.path, &r.content).map(|p| {
            serde_json::json!({
                "path": r.path,
                "title": p.title,
                "spec": p.spec,
                "endpoints": openapi::endpoints(&r.path, &r.content),
            })
        })),
        Err(e) => e,
    }
}

/// `{ path, content }` -> `Preview | null`.
#[wasm_bindgen]
pub fn openapi_preview(request_json: &str) -> String {
    match parse::<DocRequest>(request_json) {
        Ok(r) => to_json(&openapi::preview(&r.path, &r.content)),
        Err(e) => e,
    }
}

/// `{ path, content, method, opPath }` -> `string | null`.
#[wasm_bindgen]
pub fn openapi_http_request(request_json: &str) -> String {
    match parse::<DocRequest>(request_json) {
        Ok(r) => to_json(&openapi::http_request(
            &r.path, &r.content, &r.method, &r.op_path,
        )),
        Err(e) => e,
    }
}

// ---- GraphQL ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlRequest {
    /// `[path, sdl]` schema files.
    #[serde(default)]
    schema: Vec<(String, String)>,
    #[serde(default)]
    path: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    offset: usize,
}

/// `{ schema, path, content }` -> `Problem[]`.
#[wasm_bindgen]
pub fn graphql_validate(request_json: &str) -> String {
    let r = match parse::<GqlRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return e,
    };
    if graphql::is_schema_file(&r.content) {
        let mut files: Vec<(String, String)> =
            r.schema.into_iter().filter(|(p, _)| *p != r.path).collect();
        files.push((r.path.clone(), r.content));
        let set = graphql::SchemaSet::new(&files);
        return to_json(&set.problems.get(&r.path).cloned().unwrap_or_default());
    }
    if r.schema.is_empty() {
        return to_json(&graphql::syntax_problems(&r.path, &r.content));
    }
    to_json(&graphql::SchemaSet::new(&r.schema).validate(&r.path, &r.content))
}

/// `{ schema, content, offset }` -> `Completion`.
#[wasm_bindgen]
pub fn graphql_complete(request_json: &str) -> String {
    match parse::<GqlRequest>(request_json) {
        Ok(r) => to_json(&graphql::SchemaSet::new(&r.schema).complete(&r.content, r.offset)),
        Err(e) => e,
    }
}

/// `{ schema }` -> `string[]` (type names).
#[wasm_bindgen]
pub fn graphql_types(request_json: &str) -> String {
    match parse::<GqlRequest>(request_json) {
        Ok(r) => to_json(&graphql::SchemaSet::new(&r.schema).type_names()),
        Err(e) => e,
    }
}

/// `{ path, content }` -> `Config`.
#[wasm_bindgen]
pub fn graphql_config(request_json: &str) -> String {
    match parse::<DocRequest>(request_json) {
        Ok(r) => to_json(&graphql::parse_config(&r.path, &r.content)),
        Err(e) => e,
    }
}

/// `{ content }` -> `boolean`.
#[wasm_bindgen]
pub fn graphql_is_schema(request_json: &str) -> String {
    match parse::<GqlRequest>(request_json) {
        Ok(r) => to_json(&graphql::is_schema_file(&r.content)),
        Err(e) => e,
    }
}

// ---- Nx, Nuxt, Deno --------------------------------------------------------------------

#[derive(Deserialize)]
struct FilesRequest {
    #[serde(default)]
    files: BTreeMap<String, String>,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    root: String,
    #[serde(default)]
    text: String,
}

/// `{ files }` -> `Workspace`.
#[wasm_bindgen]
pub fn nx_workspace(request_json: &str) -> String {
    match parse::<FilesRequest>(request_json) {
        Ok(r) => to_json(&nx::workspace(&r.files)),
        Err(e) => e,
    }
}

/// `{ root, paths }` -> `{ root, routes, components }`.
#[wasm_bindgen]
pub fn nuxt_project(request_json: &str) -> String {
    match parse::<FilesRequest>(request_json) {
        Ok(r) => {
            let paths = r.paths.iter().map(String::as_str);
            to_json(&serde_json::json!({
                "root": r.root,
                "routes": nuxt::routes(&r.root, paths.clone()),
                "components": nuxt::components(&r.root, paths),
            }))
        }
        Err(e) => e,
    }
}

/// `{ text }` -> `Task[]`.
#[wasm_bindgen]
pub fn deno_tasks(request_json: &str) -> String {
    match parse::<FilesRequest>(request_json) {
        Ok(r) => to_json(&deno::tasks(&r.text)),
        Err(e) => e,
    }
}

// ---- cURL ------------------------------------------------------------------------------

#[derive(Deserialize)]
struct CurlRequest {
    #[serde(default)]
    command: String,
    #[serde(default)]
    request: Option<forge_scratches::http::HttpRequest>,
}

/// `{ command }` -> `{ text } | { error }` (Import from cURL).
#[wasm_bindgen]
pub fn scratches_http_from_curl(request_json: &str) -> String {
    match parse::<CurlRequest>(request_json) {
        Ok(r) => match forge_scratches::curl::parse_curl(&r.command) {
            Ok(req) => {
                to_json(&serde_json::json!({ "text": forge_scratches::curl::to_http(&req) }))
            }
            Err(e) => to_json(&serde_json::json!({ "error": e })),
        },
        Err(e) => e,
    }
}

/// `{ request }` (a resolved request) -> `string` (Copy as cURL).
#[wasm_bindgen]
pub fn scratches_http_to_curl(request_json: &str) -> String {
    match parse::<CurlRequest>(request_json) {
        Ok(CurlRequest {
            request: Some(mut req),
            ..
        }) => {
            if req.method == "GRAPHQL" {
                if let Ok(r) = forge_scratches::http::graphql_as_http(&req) {
                    req = r;
                }
            }
            to_json(&forge_scratches::curl::to_curl(&req))
        }
        Ok(_) => to_json(&serde_json::json!({ "error": "no request" })),
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_answer_json() {
        let v: serde_json::Value = serde_json::from_str(&file_watchers_templates("{}")).unwrap();
        assert_eq!(v[0]["id"], "scss");
        let plan = file_watchers_plan(
            r#"{"watchers": [{"name": "SCSS", "fileType": "scss", "program": "sass", "arguments": "$FileName$:$FileNameWithoutExtension$.css", "workingDirectory": "$FileDir$"}],
                "paths": ["css/app.scss"], "trigger": "save", "root": "/p", "name": "p"}"#,
        );
        let v: serde_json::Value = serde_json::from_str(&plan).unwrap();
        assert_eq!(v[0]["commandLine"], "sass app.scss:app.css");
        let spec = "openapi: 3.0.0\ninfo: {title: T, version: '1'}\npaths:\n  /a:\n    get:\n      responses: {'200': {description: ok}}\n";
        let v: serde_json::Value = serde_json::from_str(&openapi_file(
            &serde_json::json!({ "path": "api.yaml", "content": spec }).to_string(),
        ))
        .unwrap();
        assert_eq!(v["endpoints"][0]["path"], "/a");
        let v: serde_json::Value = serde_json::from_str(&graphql_complete(
            r#"{"schema": [["s.graphql", "type Query { me: String }"]], "content": "{ ", "offset": 2}"#,
        ))
        .unwrap();
        assert_eq!(v["items"][0]["label"], "me");
        let v: serde_json::Value = serde_json::from_str(&scratches_http_from_curl(
            r#"{"command": "curl https://x.dev -d a=1"}"#,
        ))
        .unwrap();
        assert!(v["text"]
            .as_str()
            .unwrap()
            .starts_with("POST https://x.dev\n"));
    }
}
