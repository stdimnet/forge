//! Code intelligence models (`forge-codeintel`) for the browser-mode mocks of the `code_*`
//! IPC commands: usage grouping, workspace edits over the in-memory demo files, refactoring
//! previews, rename planning (edits from references, class file, conflicts, the split
//! between editors and the backend) and the order of the Alt+Enter popup.

use std::collections::{BTreeMap, HashMap};

use forge_codeintel::{
    apply_workspace_edit, bulb_for, edit_conflicts, edit_from_locations, group_usages,
    order_actions, plan_edit, preview_workspace_edit, suggest_file_rename, validate_identifier,
    ContextAction, DocumentChange, GroupRequest, MemoryStore, Position, UsageLocation,
    WorkspaceEdit,
};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct RelatedInput {
    path: String,
    text: String,
    /// The project's files and their texts.
    files: BTreeMap<String, String>,
}

/// `{ path, text, files }` → `RelatedItem[]` (like `code_related`).
#[wasm_bindgen]
pub fn codeintel_related(request_json: &str) -> String {
    match parse::<RelatedInput>(request_json) {
        Ok(input) => {
            let list: Vec<String> = input.files.keys().cloned().collect();
            let read = |p: &str| input.files.get(p).cloned();
            to_json(&forge_codeintel::related(
                &forge_codeintel::RelatedRequest {
                    path: input.path,
                    text: input.text,
                },
                &list,
                &read,
            ))
        }
        Err(e) => to_json(&json!({ "error": e })),
    }
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

/// `ContextAction[]` → `{ actions, bulb }` like `code_context_actions`.
#[wasm_bindgen]
pub fn codeintel_context_actions(request_json: &str) -> String {
    match parse::<Vec<ContextAction>>(request_json) {
        Ok(actions) => {
            let actions = order_actions(actions);
            to_json(&json!({ "bulb": bulb_for(&actions), "actions": actions }))
        }
        Err(e) => to_json(&json!({ "error": e })),
    }
}

#[derive(Deserialize)]
struct PlanInput {
    edit: WorkspaceEdit,
    open: Vec<String>,
}

/// `{ edit, open }` → `EditPlan` (like `code_plan_edit`).
#[wasm_bindgen]
pub fn codeintel_plan_edit(request_json: &str) -> String {
    match parse::<PlanInput>(request_json) {
        Ok(input) => to_json(&plan_edit(&input.edit, &input.open)),
        Err(e) => to_json(&json!({ "error": e })),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameInput {
    /// The declaration and every usage.
    locations: Vec<UsageLocation>,
    old_name: String,
    new_name: String,
    #[serde(default)]
    declaration: Option<String>,
    #[serde(default = "yes")]
    rename_file: bool,
    files: BTreeMap<String, String>,
}

fn yes() -> bool {
    true
}

/// `{ locations, oldName, newName, declaration?, renameFile?, files }` → `{ edit, skipped,
/// fileRename, conflicts }` or `{ error }`: the index part of `code_rename`.
#[wasm_bindgen]
pub fn codeintel_rename_edit(request_json: &str) -> String {
    let input = match parse::<RenameInput>(request_json) {
        Ok(i) => i,
        Err(e) => return to_json(&json!({ "error": e })),
    };
    if let Some(problem) = validate_identifier(&input.new_name) {
        return to_json(&json!({ "error": problem }));
    }
    let read = |p: &str| input.files.get(p).cloned();
    let built = edit_from_locations(&input.locations, &input.new_name, read);
    let mut edit = built.edit;
    let mut candidates = edit.paths();
    if let Some(d) = input.declaration {
        candidates.insert(0, d);
    }
    let file_rename = suggest_file_rename(
        &candidates,
        input.old_name.trim_start_matches('$'),
        input.new_name.trim_start_matches('$'),
        read,
    );
    if let (Some(f), true) = (&file_rename, input.rename_file) {
        edit.changes.push(DocumentChange::Rename {
            from: f.from.clone(),
            to: f.to.clone(),
        });
    }
    let conflicts = edit_conflicts(&edit, |p| input.files.contains_key(p));
    to_json(&json!({
        "edit": edit,
        "skipped": built.skipped,
        "fileRename": file_rename,
        "conflicts": conflicts,
    }))
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

    #[test]
    fn renames_and_plans_over_json() {
        let out = codeintel_rename_edit(
            r#"{"locations":[
                  {"path":"app/User.php","range":{"start":{"line":0,"character":6},"end":{"line":0,"character":10}}},
                  {"path":"app/Order.php","range":{"start":{"line":0,"character":4},"end":{"line":0,"character":8}}}],
                "oldName":"User","newName":"Customer","declaration":"app/User.php",
                "files":{"app/User.php":"class User {}","app/Order.php":"new User();","app/Customer.php":""}}"#,
        );
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["fileRename"]["to"], "app/Customer.php");
        assert_eq!(v["edit"]["changes"][2]["kind"], "rename");
        assert_eq!(
            v["conflicts"][0]["message"],
            "File app/Customer.php already exists"
        );

        let bad =
            codeintel_rename_edit(r#"{"locations":[],"oldName":"User","newName":"1x","files":{}}"#);
        assert!(bad.contains("not a valid identifier"));

        let plan = codeintel_plan_edit(&format!(
            r#"{{"edit":{},"open":["app/Order.php"]}}"#,
            v["edit"]
        ));
        let p: serde_json::Value = serde_json::from_str(&plan).unwrap();
        assert_eq!(p["editors"][0]["path"], "app/Order.php");
        assert_eq!(p["fileCount"], 2);
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocUrlInput {
    text: String,
    position: Position,
    /// The file; its extension gives the language.
    path: String,
    #[serde(default)]
    declaration: Option<forge_codeintel::DocTarget>,
}

/// `{ text, position, path, declaration? }` → `{ url, symbol }`: the External
/// Documentation page (like `code_external_doc`).
#[wasm_bindgen]
pub fn codeintel_doc_url(request_json: &str) -> String {
    match parse::<DocUrlInput>(request_json) {
        Ok(input) => {
            let symbol = forge_codeintel::symbol_at(
                &input.text,
                input.position,
                forge_codeintel::language_of(&input.path),
            )
            .map(|mut s| {
                s.declaration = input.declaration;
                s
            });
            let url = symbol.as_ref().and_then(forge_codeintel::external_doc_url);
            to_json(&json!({ "url": url, "symbol": symbol }))
        }
        Err(e) => to_json(&json!({ "error": e })),
    }
}

#[derive(Deserialize)]
struct LinesInput {
    text: String,
    line: u32,
    /// The file; its extension gives the language.
    path: String,
}

/// `{ text, line, path }` → `SnippetRange` (Quick Definition).
#[wasm_bindgen]
pub fn codeintel_definition_snippet(request_json: &str) -> String {
    match parse::<LinesInput>(request_json) {
        Ok(i) => to_json(&forge_codeintel::definition_snippet(
            &i.text,
            i.line,
            forge_codeintel::language_of(&i.path),
        )),
        Err(e) => to_json(&json!({ "error": e })),
    }
}

/// `{ text, line, path }` → `ContextLine[]` (Context Info).
#[wasm_bindgen]
pub fn codeintel_context_headers(request_json: &str) -> String {
    match parse::<LinesInput>(request_json) {
        Ok(i) => to_json(&forge_codeintel::context_headers(
            &i.text,
            i.line,
            forge_codeintel::language_of(&i.path),
        )),
        Err(e) => to_json(&json!({ "error": e })),
    }
}

#[derive(Deserialize)]
struct TypeInput {
    #[serde(default)]
    hover: Option<String>,
    #[serde(default)]
    signature: Option<String>,
}

/// `{ hover?, signature? }` → the type Type Info shows, or `null`.
#[wasm_bindgen]
pub fn codeintel_type_info(request_json: &str) -> String {
    match parse::<TypeInput>(request_json) {
        Ok(i) => {
            let ty = i
                .hover
                .as_deref()
                .and_then(forge_codeintel::type_from_hover)
                .or_else(|| {
                    i.signature
                        .as_deref()
                        .and_then(forge_codeintel::type_from_signature)
                });
            to_json(&ty)
        }
        Err(e) => to_json(&json!({ "error": e })),
    }
}

#[cfg(test)]
mod doc_tests {
    use super::*;

    #[test]
    fn documentation_helpers_over_json() {
        let out = codeintel_doc_url(
            r#"{"text":"JSON.parse(a)","position":{"line":0,"character":6},"path":"a.js"}"#,
        );
        assert!(out.contains("Global_Objects/JSON/parse"));

        let out = codeintel_definition_snippet(
            r#"{"text":"/** Doc. */\nfunction a() {\n  return 1;\n}\n","line":1,"path":"a.js"}"#,
        );
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(
            (v["startLine"].as_u64(), v["endLine"].as_u64()),
            (Some(0), Some(3))
        );

        let out = codeintel_context_headers(
            r#"{"text":"class A {\n  run() {\n    x();\n  }\n}","line":2,"path":"a.ts"}"#,
        );
        assert!(out.contains("class A {"));

        assert_eq!(
            codeintel_type_info(r#"{"signature":"public function total(): float"}"#),
            "\"float\""
        );
    }

    #[test]
    fn related_files_over_json() {
        let out = codeintel_related(
            r#"{"path":"app/Checkout.php","text":"<?php","files":{"app/Checkout.php":"<?php","tests/CheckoutTest.php":"<?php"}}"#,
        );
        assert!(out.contains("\"path\":\"tests/CheckoutTest.php\""));
        assert!(out.contains("\"group\":\"Tests\""));
    }
}
