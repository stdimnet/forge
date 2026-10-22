//! Type, Method and Call Hierarchy views (`forge_codeintel::hierarchy`), used synchronously
//! by the Hierarchy tool window in the desktop app and in browser mode alike: toggling a
//! view, a scope or the sorting recomputes the tree from the data the `code_*_hierarchy`
//! commands answered, without asking the language servers or the index again.
//!
//! - `hierarchy_type_view`: `TypeViewRequest` → `HierarchyView`.
//! - `hierarchy_method_view`: `MethodViewRequest` → `HierarchyView`.
//! - `hierarchy_call_view`: `CallViewRequest` → `CallNode[]`.
//! - `hierarchy_merge_calls`: `{ a, b }` → `CallNode[]` (browser-mode mocks).

use forge_codeintel::hierarchy::{
    call_view, merge_call_nodes, method_view, type_view, CallNode, CallViewRequest,
    MethodViewRequest, TypeViewRequest,
};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| to_json(&json!({ "error": e.to_string() })))
}

#[wasm_bindgen]
pub fn hierarchy_type_view(request_json: &str) -> String {
    match parse::<TypeViewRequest>(request_json) {
        Ok(r) => to_json(&type_view(&r)),
        Err(e) => e,
    }
}

#[wasm_bindgen]
pub fn hierarchy_method_view(request_json: &str) -> String {
    match parse::<MethodViewRequest>(request_json) {
        Ok(r) => to_json(&method_view(&r)),
        Err(e) => e,
    }
}

#[wasm_bindgen]
pub fn hierarchy_call_view(request_json: &str) -> String {
    match parse::<CallViewRequest>(request_json) {
        Ok(r) => to_json(&call_view(&r)),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct MergeCalls {
    a: Vec<CallNode>,
    b: Vec<CallNode>,
}

#[wasm_bindgen]
pub fn hierarchy_merge_calls(request_json: &str) -> String {
    match parse::<MergeCalls>(request_json) {
        Ok(r) => to_json(&merge_call_nodes(r.a, r.b)),
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_round_trip_through_json() {
        let graph = json!({
            "root": "B",
            "types": [
                { "item": { "id": "A", "name": "A", "kind": "interface" } },
                { "item": { "id": "B", "name": "B", "kind": "class", "path": "app/B.php" }, "implements": ["A"] }
            ]
        });
        let out = hierarchy_type_view(&json!({ "graph": graph, "view": "supertypes" }).to_string());
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["roots"][0]["item"]["name"], "B");
        assert_eq!(v["roots"][0]["current"], true);
        assert_eq!(v["roots"][0]["children"][0]["item"]["name"], "A");
        assert!(hierarchy_call_view("{").contains("error"));
    }
}
