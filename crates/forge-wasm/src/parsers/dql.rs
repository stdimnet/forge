//! DQL in PHP strings (`phpforge-dql`) for the browser-mode Doctrine backend. Sites are found
//! by the lexical finder (the desktop app uses tree-sitter); the entity snapshot comes from
//! the mock's demo mappings.

use phpforge_dql::editor;
use phpforge_dql::lexical::Lexical;
use phpforge_dql::model::Snapshot;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use super::parse;
use crate::to_json;

#[derive(Deserialize)]
struct Request {
    path: String,
    content: String,
    #[serde(default)]
    offset: usize,
    snapshot: Snapshot,
}

fn with(json: &str, f: impl FnOnce(Request) -> String) -> String {
    match parse::<Request>(json) {
        Ok(r) => f(r),
        Err(e) => e,
    }
}

/// Mirrors `doctrine_complete`: `{ path, content, offset, snapshot }` → `DqlCompletion | null`.
#[wasm_bindgen]
pub fn dql_complete(request_json: &str) -> String {
    with(request_json, |r| {
        to_json(&editor::complete(
            &Lexical,
            &r.snapshot,
            &r.path,
            &r.content,
            r.offset,
        ))
    })
}

/// Mirrors `doctrine_goto`: `{ path, content, offset, snapshot }` → `Location[]`.
#[wasm_bindgen]
pub fn dql_goto(request_json: &str) -> String {
    with(request_json, |r| {
        to_json(&editor::goto(
            &Lexical,
            &r.snapshot,
            &r.path,
            &r.content,
            r.offset,
        ))
    })
}

/// Mirrors `doctrine_diagnostics`: `{ path, content, snapshot }` → `SourceDiagnostic[]`.
#[wasm_bindgen]
pub fn dql_diagnostics(request_json: &str) -> String {
    with(request_json, |r| {
        to_json(&editor::diagnostics(
            &Lexical,
            &r.snapshot,
            &r.path,
            &r.content,
        ))
    })
}

/// Mirrors `doctrine_highlight`: `{ path, content, snapshot }` → `SourceHighlight`.
#[wasm_bindgen]
pub fn dql_highlight(request_json: &str) -> String {
    with(request_json, |r| {
        to_json(&editor::highlight(
            &Lexical,
            &r.snapshot,
            &r.path,
            &r.content,
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_and_completes_with_a_json_snapshot() {
        let snapshot = r#"{"entities":[{"class":"App\\Entity\\Order","name":"Order","path":"src/Entity/Order.php","line":3,"source":"attribute","fields":[{"name":"status","type":"string","line":5}],"associations":[]}],"repositories":[]}"#;
        let content =
            "<?php $em->createQuery('SELECT o FROM App\\\\Entity\\\\Order o WHERE o.st');";
        let req = |offset: usize| {
            serde_json::json!({
                "path": "src/X.php",
                "content": content,
                "offset": offset,
                "snapshot": serde_json::from_str::<serde_json::Value>(snapshot).unwrap(),
            })
            .to_string()
        };
        let h = dql_highlight(&req(0));
        assert!(h.contains(r#""kind":"keyword""#), "{h}");
        let c = dql_complete(&req(content.find("o.st").unwrap() + 4));
        assert!(c.contains("status"), "{c}");
    }
}
