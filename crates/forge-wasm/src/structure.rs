//! The editor's code structure, used synchronously by the editor in the desktop app and in
//! browser mode alike (like the keymap resolver): Sticky Lines and breadcrumbs follow every
//! scroll and caret move, folding asks per line, so an IPC round trip would lag behind.
//!
//! - `editor_structure`: scopes and custom folding regions (`forge_codeintel::structure`).
//! - `editor_doc_comments`: PHPDoc / JSDoc / LuaDoc comments rendered into documentation
//!   parts (`forge_markdown::doc_comments`).
//! - `structure_group_by_regions`: Structure view nodes grouped under their regions.

use forge_codeintel::{editor_structure as structure_of, group_by_regions, Region};
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
    }
}
