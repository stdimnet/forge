//! JSON Schema (`forge-jsonschema`) for the browser-mode mocks of the `json_schema_*` IPC
//! commands. Browser mode has the bundled schemas and project files only (no downloads):
//! a request carries the user mappings and the project schema files it has read so far;
//! an answer with `needs` asks for more files and is repeated with them.

use std::collections::HashMap;
use std::sync::Arc;

use forge_jsonschema::mapping::{self, Source, UserMapping};
use forge_jsonschema::{bundled, uri, DocKind, Registry, SchemaDoc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    path: String,
    #[serde(default)]
    language: String,
    content: String,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    mappings: Vec<UserMapping>,
    /// Project schema files by path.
    #[serde(default)]
    files: HashMap<String, String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    name: String,
    uri: String,
    source: Source,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

enum Prepared {
    None,
    Needs(Vec<String>),
    Ready(Registry, String, DocKind, Info),
}

fn prepare(r: &Request) -> Prepared {
    let Some(kind) = DocKind::from_language(&r.language).or_else(|| DocKind::from_path(&r.path))
    else {
        return Prepared::None;
    };
    let Some(sel) = mapping::select(&r.path, &r.content, kind, &r.mappings, None) else {
        return Prepared::None;
    };
    let mut info = Info {
        name: sel.name.clone(),
        uri: sel.uri.clone(),
        source: sel.source,
        location: None,
        error: None,
    };
    let mut reg = Registry::new();
    reg.draft = sel.version;
    if sel.uri.is_empty() {
        return Prepared::Ready(reg, sel.uri, kind, info);
    }
    let mut needs = Vec::new();
    let mut queue = vec![uri::document(&sel.uri).to_string()];
    let mut first = true;
    while let Some(u) = queue.pop() {
        if reg.contains(&u) || reg.failure(&u).is_some() {
            continue;
        }
        let error = if forge_jsonschema::load_bundled(&mut reg, &u) {
            None
        } else if let Some(path) = uri::project_path(&u) {
            match r.files.get(path) {
                Some(text) => match SchemaDoc::parse(&u, path, text.clone()) {
                    Ok(doc) => {
                        reg.insert(Arc::new(doc));
                        None
                    }
                    Err(e) => Some(format!("{path}: {e}")),
                },
                None if !r.files.contains_key(path) && !needs.contains(&path.to_string()) => {
                    needs.push(path.to_string());
                    Some(format!("Cannot read {path}"))
                }
                None => Some(format!("Cannot read {path}")),
            }
        } else {
            Some("Remote schemas are downloaded by the desktop app".to_string())
        };
        if let Some(e) = error {
            if first {
                info.error = Some(e.clone());
            }
            reg.mark_failed(&u, e);
        }
        first = false;
        queue.extend(reg.missing());
    }
    if !needs.is_empty() {
        return Prepared::Needs(needs);
    }
    info.location = reg.doc(&sel.uri).map(|d| d.location.clone());
    Prepared::Ready(reg, sel.uri, kind, info)
}

fn run(
    json_in: &str,
    f: impl FnOnce(&Request, &Registry, &str, DocKind, Info) -> serde_json::Value,
) -> String {
    let r: Request = match serde_json::from_str(json_in) {
        Ok(r) => r,
        Err(e) => return to_json(&json!({ "error": e.to_string() })),
    };
    match prepare(&r) {
        Prepared::None => to_json(&json!({ "answer": null })),
        Prepared::Needs(n) => to_json(&json!({ "needs": n })),
        Prepared::Ready(reg, root, kind, info) => {
            to_json(&json!({ "answer": f(&r, &reg, &root, kind, info) }))
        }
    }
}

/// `{ path, language, content, mappings, files }` → `{ answer: { schema, problems } }` or
/// `{ needs: [path] }`, like `json_schema_validate`.
#[wasm_bindgen]
pub fn json_schema_validate(request_json: &str) -> String {
    run(request_json, |r, reg, root, kind, info| {
        let problems = if root.is_empty() || info.error.is_some() {
            Vec::new()
        } else {
            forge_jsonschema::validate(reg, root, kind, &r.content)
        };
        json!({ "schema": info, "problems": problems })
    })
}

/// Like `json_schema_complete`.
#[wasm_bindgen]
pub fn json_schema_complete(request_json: &str) -> String {
    run(request_json, |r, reg, root, kind, _| {
        json!(forge_jsonschema::complete(
            reg, root, kind, &r.content, r.offset
        ))
    })
}

/// Like `json_schema_hover`.
#[wasm_bindgen]
pub fn json_schema_hover(request_json: &str) -> String {
    run(request_json, |r, reg, root, kind, _| {
        json!(forge_jsonschema::hover(
            reg, root, kind, &r.content, r.offset
        ))
    })
}

/// Like `json_schema_goto`.
#[wasm_bindgen]
pub fn json_schema_goto(request_json: &str) -> String {
    run(request_json, |r, reg, root, kind, _| {
        json!(forge_jsonschema::goto(
            reg, root, kind, &r.content, r.offset
        ))
    })
}

/// A bundled schema's text for `json_schema_read`, or `{ error }`.
#[wasm_bindgen]
pub fn json_schema_read(location: &str) -> String {
    match bundled::by_location(location) {
        Some(b) => to_json(
            &json!({ "text": b.text(), "detail": format!("Bundled with Forge · {}", b.uri()) }),
        ),
        None => to_json(&json!({ "error": format!("not a loaded schema: {location}") })),
    }
}

/// Like `json_schema_builtins`.
#[wasm_bindgen]
pub fn json_schema_builtins() -> String {
    let list: Vec<_> = mapping::BUILTINS
        .iter()
        .map(|b| json!({ "name": b.name, "uri": b.uri, "files": b.files }))
        .collect();
    to_json(&list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_for_project_schemas_then_validates() {
        let req = json!({
            "path": "config/app.json", "language": "json", "content": "{\"port\": \"x\"}",
            "mappings": [{"name": "App", "schema": "schemas/app.json", "files": "config/app.json"}],
        });
        let out: serde_json::Value =
            serde_json::from_str(&json_schema_validate(&req.to_string())).unwrap();
        assert_eq!(out["needs"], json!(["schemas/app.json"]));
        let mut req = req;
        req["files"] =
            json!({ "schemas/app.json": "{\"properties\": {\"port\": {\"type\": \"integer\"}}}" });
        let out: serde_json::Value =
            serde_json::from_str(&json_schema_validate(&req.to_string())).unwrap();
        assert_eq!(out["answer"]["schema"]["name"], "App");
        assert_eq!(
            out["answer"]["problems"][0]["message"],
            "Incompatible types. Required: integer. Actual: string."
        );
        let out: serde_json::Value = serde_json::from_str(&json_schema_builtins()).unwrap();
        assert_eq!(out[0]["name"], "composer.json");
        assert!(json_schema_read("json-schema://bundled/package.json").contains("\"text\""));
    }
}
