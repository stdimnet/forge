//! Actions on Save (`forge-save-actions`), mirroring `save_actions_plan`,
//! `save_actions_changed_lines` and the file types of `save_actions_info`. The settings
//! come from the browser-mode settings store.

use forge_save_actions::{ActionsOnSave, Trigger};
use serde::Deserialize;
use serde_json::Value;
use wasm_bindgen::prelude::*;

use crate::to_json;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanRequest {
    path: String,
    trigger: Trigger,
    /// Effective setting values by key.
    settings: serde_json::Map<String, Value>,
}

/// `{ path, trigger, settings }` → `SavePlan`.
#[wasm_bindgen]
pub fn save_actions_plan(request_json: &str) -> String {
    match serde_json::from_str::<PlanRequest>(request_json) {
        Ok(r) => {
            let settings = ActionsOnSave::from_settings(|k| r.settings.get(k).cloned());
            to_json(&forge_save_actions::plan(&r.path, r.trigger, &settings))
        }
        Err(e) => to_json(&serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct ChangedRequest {
    base: Option<String>,
    original: String,
    formatted: String,
}

/// `{ base, original, formatted }` → the text with only changed lines reformatted.
#[wasm_bindgen]
pub fn save_actions_changed_lines(request_json: &str) -> String {
    match serde_json::from_str::<ChangedRequest>(request_json) {
        Ok(r) => to_json(&forge_save_actions::restrict_to_changed_lines(
            r.base.as_deref(),
            &r.original,
            &r.formatted,
        )),
        Err(e) => to_json(&serde_json::json!({ "error": e.to_string() })),
    }
}

/// `{}` → the file types of the "Reformat code" filter.
#[wasm_bindgen]
pub fn save_actions_file_types(_request_json: &str) -> String {
    to_json(&forge_save_actions::file_type_choices())
}
