//! Code intelligence models (`forge-codeintel`) for the browser-mode mocks of the `code_*`
//! IPC commands: usage grouping, workspace edits over the in-memory demo files, refactoring
//! previews and the order of the Alt+Enter popup.

use std::collections::{BTreeMap, HashMap};

use forge_codeintel::{
    apply_workspace_edit, group_usages, order_actions, preview_workspace_edit, ContextAction,
    GroupRequest, MemoryStore, Position, UsageLocation, WorkspaceEdit,
};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsagesInput {
    usages: Vec<UsageLocation>,
    #[serde(default)]
    declaration: Option<UsageLocation>,
    files: HashMap<String, String>,
}

/// `{ usages, declaration?, files }` → `{ byType, byFile }` (like `code_find_usages`).
#[wasm_bindgen]
pub fn codeintel_group_usages(request_json: &str) -> String {
    match parse::<UsagesInput>(request_json) {
        Ok(input) => {
            let mut req = GroupRequest {
                usages: input.usages,
                declaration: input.declaration,
                group_by_type: true,
            };
            let source = |p: &str| input.files.get(p).map(String::as_str);
            let by_type = group_usages(&req, source);
            req.group_by_type = false;
            let by_file = group_usages(&req, source);
            to_json(&json!({ "byType": by_type, "byFile": by_file }))
        }
        Err(e) => to_json(&json!({ "error": e })),
    }
}

#[derive(Deserialize)]
struct EditInput {
    edit: WorkspaceEdit,
    files: BTreeMap<String, String>,
}

/// `{ edit, files }` → `{ applied: AppliedFile[], files }` (the files after the edit) or
/// `{ error }`.
#[wasm_bindgen]
pub fn codeintel_apply_edit(request_json: &str) -> String {
    match parse::<EditInput>(request_json) {
        Ok(input) => {
            let mut store = MemoryStore(input.files);
            match apply_workspace_edit(&input.edit, &mut store) {
                Ok(applied) => to_json(&json!({ "applied": applied, "files": store.0 })),
                Err(e) => to_json(&json!({ "error": e.to_string() })),
            }
        }
        Err(e) => to_json(&json!({ "error": e })),
    }
}

/// `{ edit, files }` → `FilePreview[]` (Refactoring Preview rows).
#[wasm_bindgen]
pub fn codeintel_preview_edit(request_json: &str) -> String {
    match parse::<EditInput>(request_json) {
        Ok(input) => to_json(&preview_workspace_edit(&input.edit, |p| {
            input.files.get(p).cloned()
        })),
        Err(e) => to_json(&json!({ "error": e })),
    }
}

/// `ContextAction[]` → the same, in PHPStorm's order without duplicates.
#[wasm_bindgen]
pub fn codeintel_order_actions(request_json: &str) -> String {
    match parse::<Vec<ContextAction>>(request_json) {
        Ok(actions) => to_json(&order_actions(actions)),
        Err(e) => to_json(&json!({ "error": e })),
    }
}

#[derive(Deserialize)]
struct WordInput {
    text: String,
    position: Position,
}

/// `{ text, position }` → `[range, word] | null`.
#[wasm_bindgen]
pub fn codeintel_word_range(request_json: &str) -> String {
    match parse::<WordInput>(request_json) {
        Ok(input) => to_json(&forge_codeintel::word_range(&input.text, input.position)),
        Err(e) => to_json(&json!({ "error": e })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_and_applies_over_json() {
        let out = codeintel_group_usages(
            r#"{"usages":[{"path":"a.php","range":{"start":{"line":0,"character":4},"end":{"line":0,"character":8}}}],
                "files":{"a.php":"new User();"}}"#,
        );
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["byType"]["groups"][0]["label"], "New instance creation");
        assert_eq!(v["byFile"]["total"], 1);

        let out = codeintel_apply_edit(
            r#"{"edit":{"changes":[{"kind":"edit","path":"a.php","edits":[{"range":{"start":{"line":0,"character":4},"end":{"line":0,"character":8}},"newText":"Customer"}]}]},
                "files":{"a.php":"new User();"}}"#,
        );
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["files"]["a.php"], "new Customer();");
        assert_eq!(v["applied"][0]["before"], "new User();");
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InlayInput {
    hints: Vec<forge_codeintel::inlay::RawInlayHint>,
    text: String,
    #[serde(default)]
    language_id: Option<String>,
    /// Effective setting values by key.
    settings: HashMap<String, serde_json::Value>,
}

/// `{ hints, text, languageId?, settings }` → the hints to draw (like `lsp_inlay_hints`).
#[wasm_bindgen]
pub fn codeintel_inlay_hints(request_json: &str) -> String {
    match parse::<InlayInput>(request_json) {
        Ok(input) => {
            let filter = forge_codeintel::inlay::InlayFilter::from_settings(
                input.language_id.as_deref(),
                |k| input.settings.get(k).and_then(serde_json::Value::as_bool),
            );
            to_json(&forge_codeintel::inlay::editor_hints(
                input.hints,
                &input.text,
                &filter,
            ))
        }
        Err(e) => to_json(&json!({ "error": e })),
    }
}
