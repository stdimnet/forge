//! Repair and reset models (`forge-maintenance` without its disk part), mirroring the
//! `maintenance_invalidate_model` and `settings_export_info` IPC commands.

use forge_maintenance::groups::count_groups;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

/// The Invalidate Caches dialog.
#[wasm_bindgen]
pub fn maintenance_invalidate_model() -> String {
    to_json(&forge_maintenance::invalidate_model())
}

#[derive(Deserialize)]
struct CountRequest {
    keys: Vec<String>,
    #[serde(default)]
    files: Vec<String>,
}

/// `{ keys, files }` (user setting keys, existing config files) → the settings groups with
/// their counts.
#[wasm_bindgen]
pub fn maintenance_settings_groups(request_json: &str) -> String {
    match serde_json::from_str::<CountRequest>(request_json) {
        Ok(req) => {
            let files: Vec<&str> = req.files.iter().map(String::as_str).collect();
            to_json(&count_groups(req.keys.iter().map(String::as_str), &files))
        }
        Err(e) => to_json(&serde_json::json!({ "error": e.to_string() })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_json() {
        assert!(maintenance_invalidate_model().contains("Invalidate and Restart"));
        let out = maintenance_settings_groups(r#"{"keys":["editor.font.size","keymap.active"]}"#);
        assert!(
            out.contains(r#""id":"keymap""#) && out.contains(r#""id":"editor""#),
            "{out}"
        );
    }
}
