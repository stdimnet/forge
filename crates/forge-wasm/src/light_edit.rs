//! The LightEdit window's tabs (`forge_window::light_edit`), for the browser-mode mock of the
//! `light_edit_*` IPC commands.

use forge_window::LightEditTabs;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::to_json;

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum Op {
    Open {
        path: String,
        #[serde(default)]
        line: Option<u32>,
        #[serde(default)]
        column: Option<u32>,
    },
    Close {
        id: String,
    },
    Activate {
        id: String,
    },
}

#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    tabs: Option<LightEditTabs>,
    ops: Vec<Op>,
}

#[derive(Serialize)]
struct Response {
    tabs: LightEditTabs,
    snapshot: forge_window::LightEditSnapshot,
}

/// `{ tabs?, ops: [{ op: "open", path, line?, column? } | { op: "close" | "activate", id }] }`
/// → `{ tabs, snapshot }` (pass `tabs` back in with the next call).
#[wasm_bindgen]
pub fn light_edit_apply(request_json: &str) -> String {
    let request = match serde_json::from_str::<Request>(request_json) {
        Ok(r) => r,
        Err(e) => return to_json(&serde_json::json!({ "error": e.to_string() })),
    };
    let mut tabs = request.tabs.unwrap_or_default();
    for op in request.ops {
        match op {
            Op::Open { path, line, column } => {
                tabs.open(&path, line, column);
            }
            Op::Close { id } => {
                tabs.close(&id);
            }
            Op::Activate { id } => {
                tabs.activate(&id);
            }
        }
    }
    let snapshot = tabs.snapshot();
    to_json(&Response { tabs, snapshot })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_tabs_between_calls() {
        let out = light_edit_apply(
            r#"{"ops":[{"op":"open","path":"/a.txt"},{"op":"open","path":"/b.txt","line":4}]}"#,
        );
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["snapshot"]["files"].as_array().unwrap().len(), 2);
        assert_eq!(v["snapshot"]["reveal"]["line"], 4);
        let again =
            serde_json::json!({ "tabs": v["tabs"], "ops": [{ "op": "open", "path": "/a.txt" }] });
        let v: serde_json::Value =
            serde_json::from_str(&light_edit_apply(&again.to_string())).unwrap();
        assert_eq!(v["snapshot"]["files"].as_array().unwrap().len(), 2);
        assert_eq!(v["snapshot"]["active"], "light-edit:/a.txt");
        assert!(light_edit_apply("{}").contains("error"));
    }
}
