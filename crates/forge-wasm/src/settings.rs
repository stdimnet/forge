//! The Settings dialog model (`forge_plugin_api::settings_model`), mirroring the
//! `settings_search` and `settings_validate` IPC commands.

use forge_plugin_api::settings_model::{self, SearchRequest, ValidateRequest};
use wasm_bindgen::prelude::*;

use crate::to_json;

/// `SearchRequest` → `SearchResponse`.
#[wasm_bindgen]
pub fn settings_search(request_json: &str) -> String {
    match serde_json::from_str::<SearchRequest>(request_json) {
        Ok(req) => to_json(&settings_model::search(&req)),
        Err(e) => to_json(&serde_json::json!({ "error": e.to_string() })),
    }
}

/// `{ spec, value }` → the problem with the value, or `null` when it is valid.
#[wasm_bindgen]
pub fn settings_validate(request_json: &str) -> String {
    match serde_json::from_str::<ValidateRequest>(request_json) {
        Ok(req) => to_json(&settings_model::validate(&req)),
        Err(e) => to_json(&serde_json::json!({ "error": e.to_string() })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_json() {
        let out = settings_search(
            r#"{"query":"font","pages":[{"id":"editor.font","title":"Font","group":"Editor"}],"highlight":["Editor Font"]}"#,
        );
        assert!(out.contains(r#""firstMatch":"editor.font""#), "{out}");
        let spec = r#"{"key":"a","title":"A","type":"int","max":3,"default":1,"group":"g"}"#;
        assert_eq!(
            settings_validate(&format!(r#"{{"spec":{spec},"value":2}}"#)),
            "null"
        );
        assert!(settings_validate(&format!(r#"{{"spec":{spec},"value":9}}"#)).contains("at most"));
    }
}
