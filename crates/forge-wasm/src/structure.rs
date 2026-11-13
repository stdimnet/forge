//! The editor's code structure, used synchronously by the editor in the desktop app and in
//! browser mode alike (like the keymap resolver): Sticky Lines and breadcrumbs follow every
//! scroll and caret move, folding asks per line, so an IPC round trip would lag behind.
//!
//! - `editor_structure`: scopes and custom folding regions (`forge_codeintel::structure`).
//! - `editor_doc_comments`: PHPDoc / JSDoc / LuaDoc comments rendered into documentation
//!   parts (`forge_markdown::doc_comments`).
//! - `structure_group_by_regions`: Structure view nodes grouped under their regions.
//! - `file_structure_view`: the File Structure popup's rows (toggles, speed search, caret).
//! - `nav_bar_segments`: the Navigation Bar's segments for a file and the caret's scopes.
//! - `editor_method_target`: where Next/Previous Method moves the caret.

use forge_codeintel::{
    editor_structure as structure_of, group_by_regions, Region, STRUCTURE_LANGUAGES,
};
use serde::Deserialize;
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

use crate::to_json;

#[derive(Deserialize)]
struct DocumentRequest {
    text: String,
    /// Forge file type id (`php`, `typescript`, `vue`, `lua`, ...).
    language: String,
}

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| to_json(&json!({ "error": e.to_string() })))
}

/// `{ text, language }` → `{ scopes: Scope[], regions: Region[] }`.
#[wasm_bindgen]
pub fn editor_structure(request_json: &str) -> String {
    match parse::<DocumentRequest>(request_json) {
        Ok(r) => to_json(&structure_of(&r.text, &r.language)),
        Err(e) => e,
    }
}

/// `{ text, language }` → `{ line, endLine, column, doc: HoverDoc }[]`.
#[wasm_bindgen]
pub fn editor_doc_comments(request_json: &str) -> String {
    match parse::<DocumentRequest>(request_json) {
        Ok(r) => to_json(&forge_markdown::doc_comments(&r.text, &r.language)),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct MethodRequest {
    text: String,
    language: String,
    line: u32,
    column: u32,
    forward: bool,
}

/// `{ text, language, line, column, forward }` → `{ line, column } | null`.
#[wasm_bindgen]
pub fn editor_method_target(request_json: &str) -> String {
    match parse::<MethodRequest>(request_json) {
        Ok(r) => to_json(&forge_codeintel::method_target(
            &structure_of(&r.text, &r.language),
            r.line,
            r.column,
            r.forward,
        )),
        Err(e) => e,
    }
}

/// `{ id, label }[]` of the languages with Sticky Lines and breadcrumbs.
#[wasm_bindgen]
pub fn editor_structure_languages() -> String {
    let list: Vec<Value> = STRUCTURE_LANGUAGES
        .iter()
        .map(|(id, label)| json!({ "id": id, "label": label }))
        .collect();
    to_json(&list)
}

#[derive(Deserialize)]
struct GroupRequest {
    nodes: Vec<Value>,
    regions: Vec<Region>,
}

/// `{ nodes, regions }` → the nodes with region nodes (`kind: "region"`) around members.
#[wasm_bindgen]
pub fn structure_group_by_regions(request_json: &str) -> String {
    match parse::<GroupRequest>(request_json) {
        Ok(r) => to_json(&group_by_regions(r.nodes, &r.regions)),
        Err(e) => e,
    }
}

/// `FileStructureRequest` → `{ rows, selected?, hasFields, hasLambdas, hasInherited }`.
#[wasm_bindgen]
pub fn file_structure_view(request_json: &str) -> String {
    match parse::<forge_codeintel::FileStructureRequest>(request_json) {
        Ok(r) => to_json(&forge_codeintel::file_structure_view(&r)),
        Err(e) => e,
    }
}

/// `{ project, path, scopes, showMembers }` → `NavSegment[]`.
#[wasm_bindgen]
pub fn nav_bar_segments(request_json: &str) -> String {
    match parse::<forge_codeintel::NavBarRequest>(request_json) {
        Ok(r) => to_json(&forge_codeintel::nav_bar_segments(&r)),
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_answer_json() {
        let out: Value = serde_json::from_str(&editor_structure(
            r#"{"text":"<?php\nclass A {\n  // region R\n  function b() {\n  }\n  // endregion\n}\n","language":"php"}"#,
        ))
        .unwrap();
        assert_eq!(out["scopes"][0]["name"], "A");
        assert_eq!(out["scopes"][1]["name"], "b()");
        assert_eq!(out["regions"][0]["name"], "R");
        let next: Value = serde_json::from_str(&editor_method_target(
            r#"{"text":"<?php\nclass A {\n  function b() {\n  }\n}\n","language":"php","line":1,"column":7,"forward":true}"#,
        ))
        .unwrap();
        assert_eq!(
            (next["line"].clone(), next["column"].clone()),
            (2.into(), 11.into())
        );

        let docs: Value = serde_json::from_str(&editor_doc_comments(
            r#"{"text":"/** Hi. @return int */","language":"php"}"#,
        ))
        .unwrap();
        assert_eq!(docs[0]["line"], 0);

        let grouped: Value = serde_json::from_str(&structure_group_by_regions(
            r#"{"nodes":[{"name":"b","line":2}],"regions":[{"name":"R","line":1,"column":2,"endLine":4,"endColumn":14}]}"#,
        ))
        .unwrap();
        assert_eq!(grouped[0]["kind"], "region");
        assert!(editor_structure("{").contains("error"));

        let view: Value = serde_json::from_str(&file_structure_view(
            r#"{"nodes":[{"name":"A","kind":"class","line":0,"children":[{"name":"run","kind":"method","line":1}]}],"query":"run"}"#,
        ))
        .unwrap();
        assert_eq!(view["rows"][1]["node"]["name"], "run");
        assert_eq!(view["selected"], 1);
        let segs: Value = serde_json::from_str(&nav_bar_segments(
            r#"{"project":"p","path":"a/b.ts","scopes":[],"showMembers":true}"#,
        ))
        .unwrap();
        assert_eq!(segs[2]["kind"], "file");
        assert!(editor_structure_languages().contains("\"PHP\""));
    }
}
