//! Code intelligence models (`forge-codeintel`) for the browser-mode mocks of the `code_*`
//! IPC commands: usage grouping, workspace edits over the in-memory demo files, refactoring
//! previews, rename planning (edits from references, class file, conflicts, the split
//! between editors and the backend) and the order of the Alt+Enter popup.

use std::collections::{BTreeMap, HashMap};

use forge_codeintel::{
    apply_workspace_edit, bulb_for, edit_conflicts, edit_from_locations, group_usages,
    order_actions, plan_edit, preview_workspace_edit, refactor_this_actions, suggest_file_rename,
    validate_identifier, ContextAction, DocumentChange, GroupRequest, MemoryStore, Position,
    UsageLocation, WorkspaceEdit,
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
struct TestNavInput {
    path: String,
    text: String,
    #[serde(default)]
    composer_path: String,
    /// The project's files and their texts.
    files: BTreeMap<String, String>,
}

fn with_test_nav<T: serde::Serialize>(
    request_json: &str,
    f: impl Fn(&forge_codeintel::TestNavRequest, &[String], &dyn Fn(&str) -> Option<String>) -> T,
) -> String {
    match parse::<TestNavInput>(request_json) {
        Ok(input) => {
            let list: Vec<String> = input.files.keys().cloned().collect();
            let read = |p: &str| input.files.get(p).cloned();
            let req = forge_codeintel::TestNavRequest {
                path: input.path,
                text: input.text,
                composer_path: input.composer_path,
            };
            to_json(&f(&req, &list, &read))
        }
        Err(e) => to_json(&json!({ "error": e })),
    }
}

/// `{ path, text, files }` → `TestTargets` (like `code_test_targets`).
#[wasm_bindgen]
pub fn codeintel_test_targets(request_json: &str) -> String {
    with_test_nav(request_json, forge_codeintel::test_targets)
}

/// `{ path, text, composerPath?, files }` → `NewTestPlan | null` (like `code_new_test_plan`).
#[wasm_bindgen]
pub fn codeintel_new_test_plan(request_json: &str) -> String {
    with_test_nav(request_json, forge_codeintel::new_test_plan)
}

/// `TestSourceRequest` → `TestSource` or `{ error }` (the rendering of `code_create_test`).
#[wasm_bindgen]
pub fn codeintel_test_source(request_json: &str) -> String {
    match parse::<forge_codeintel::TestSourceRequest>(request_json) {
        Ok(req) => match forge_codeintel::test_source(&req) {
            Ok(file) => to_json(&file),
            Err(e) => to_json(&json!({ "error": e })),
        },
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

// ---- Highlight usages and Find Usages options ----

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HighlightInput {
    text: String,
    path: String,
    line: u32,
    character: u32,
    /// Usage ranges from an index (the PHP mocks); without them the lexical fallback.
    #[serde(default)]
    ranges: Option<Vec<forge_codeintel::Range>>,
    #[serde(default)]
    declaration: Option<forge_codeintel::Range>,
    #[serde(default)]
    title: String,
    #[serde(default)]
    element: forge_codeintel::ElementKind,
}

/// `{ text, path, line, character, ranges?, declaration?, title?, element? }` →
/// `HighlightAnswer | null` (like `code_highlight_usages`): exit points on `return` /
/// `throw`, the given ranges classified as reads and writes, else the lexical fallback.
#[wasm_bindgen]
pub fn codeintel_highlight_usages(request_json: &str) -> String {
    match parse::<HighlightInput>(request_json) {
        Ok(i) => {
            let at = Position::new(i.line, i.character);
            if let Some(exits) = forge_codeintel::exit_points(&i.text, &i.path, at) {
                return to_json(&exits);
            }
            let answer = match i.ranges {
                Some(ranges) => {
                    let h = forge_codeintel::classify_ranges(&i.text, &ranges, i.declaration);
                    (!h.is_empty()).then(|| {
                        forge_codeintel::HighlightAnswer::new(
                            forge_codeintel::HighlightTarget::Usages,
                            i.title,
                            h,
                            "index",
                        )
                        .with_element(i.element)
                    })
                }
                None => forge_codeintel::lexical_highlights(&i.text, &i.path, at),
            };
            to_json(&answer)
        }
        Err(e) => to_json(&json!({ "error": e })),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FilterInput {
    usages: Vec<UsageLocation>,
    #[serde(default)]
    options: forge_codeintel::UsageOptions,
    #[serde(default)]
    scope: forge_codeintel::UsageScope,
    #[serde(default)]
    element: forge_codeintel::ElementKind,
    /// The file Find Usages started in and the word searched (text occurrences).
    path: String,
    #[serde(default)]
    word: String,
    files: HashMap<String, String>,
}

/// `{ usages, options, scope, element, path, word, files }` → `{ usages, scope }`: the
/// usages with text occurrences, in the scope and filtered by the options (like
/// `code_find_usages`).
#[wasm_bindgen]
pub fn codeintel_filter_usages(request_json: &str) -> String {
    match parse::<FilterInput>(request_json) {
        Ok(i) => {
            let effective = i.options.effective(i.element);
            let mut usages = i.usages;
            if i.element != forge_codeintel::ElementKind::Variable && !i.word.is_empty() {
                let mut paths: Vec<&String> = i.files.keys().collect();
                paths.sort();
                let files = paths
                    .into_iter()
                    .filter(|p| i.scope.contains(p))
                    .map(|p| (p.as_str(), i.files[p].as_str()));
                let extra =
                    forge_codeintel::text_occurrences(&i.word, &i.path, files, &usages, &effective);
                usages.extend(extra);
            }
            let usages = i.scope.filter(usages);
            let usages = forge_codeintel::apply_options(usages, &effective, |u| {
                let line = i
                    .files
                    .get(&u.path)
                    .and_then(|t| t.split('\n').nth(u.range.start.line as usize))
                    .unwrap_or("");
                forge_codeintel::classify_usage(
                    line,
                    u.range.start.character,
                    u.range.end.character,
                )
            });
            to_json(&json!({ "usages": usages, "scope": i.scope.label() }))
        }
        Err(e) => to_json(&json!({ "error": e })),
    }
}

/// `{ element, title }` → `UsageTarget` (the Find Usages dialog's checkboxes).
#[wasm_bindgen]
pub fn codeintel_usage_target(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Input {
        element: forge_codeintel::ElementKind,
        title: String,
    }
    match parse::<Input>(request_json) {
        Ok(i) => to_json(&forge_codeintel::UsageTarget::new(i.element, i.title)),
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
    fn highlights_and_usage_filters_over_json() {
        let out = codeintel_highlight_usages(
            r#"{"text":"<?php\n$a = 1;\necho $a;\n","path":"a.php","line":2,"character":6}"#,
        );
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["highlights"][0]["kind"], "write");
        assert_eq!(v["summary"], "2 usages of '$a' found");

        let out = codeintel_filter_usages(
            r#"{"usages":[],"options":{"searchForTextOccurrences":true},"scope":{"kind":"projectFiles"},"element":"class","path":"app/User.php","word":"User","files":{"config/a.yaml":"model: User","vendor/x.yaml":"User"}}"#,
        );
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["usages"].as_array().unwrap().len(), 1);
        assert_eq!(v["usages"][0]["kind"], "nonCode");
        assert_eq!(v["scope"], "Project Files");

        let out = codeintel_usage_target(r#"{"element":"method","title":"charge"}"#);
        assert!(out.contains("\"label\":\"method charge\""));
    }

    #[test]
    fn related_files_over_json() {
        let out = codeintel_related(
            r#"{"path":"app/Checkout.php","text":"<?php","files":{"app/Checkout.php":"<?php","tests/CheckoutTest.php":"<?php"}}"#,
        );
        assert!(out.contains("\"path\":\"tests/CheckoutTest.php\""));
        assert!(out.contains("\"group\":\"Tests\""));
    }

    #[test]
    fn go_to_test_and_create_test_over_json() {
        let out = codeintel_test_targets(
            r#"{"path":"app/Checkout.php","text":"<?php","files":{"app/Checkout.php":"<?php","tests/CheckoutTest.php":"<?php"}}"#,
        );
        assert!(out.contains("\"path\":\"tests/CheckoutTest.php\""));
        assert!(out.contains("\"canCreate\":true"));
        let plan = codeintel_new_test_plan(
            r#"{"path":"app/Checkout.php","text":"<?php\nnamespace App;\nclass Checkout {}","files":{"composer.json":"{\"autoload\":{\"psr-4\":{\"App\\\\\":\"app/\"}}}"}}"#,
        );
        assert!(plan.contains("\"className\":\"CheckoutTest\""), "{plan}");
        let src = codeintel_test_source(
            r#"{"framework":"PHPUnit","className":"CheckoutTest","namespace":"Tests","directory":"tests","fileName":"CheckoutTest.php","superclass":"PHPUnit\\Framework\\TestCase"}"#,
        );
        assert!(src.contains("\"path\":\"tests/CheckoutTest.php\""), "{src}");
        assert!(codeintel_test_source(r#"{"fileName":"x.php"}"#).contains("error"));
    }
}

/// Refactor This in JS/TS: the language server's refactorings worded like PHPStorm
/// (`ContextAction[]` -> `ContextAction[]`).
#[wasm_bindgen]
pub fn codeintel_refactor_this_actions(request_json: &str) -> String {
    match parse::<Vec<ContextAction>>(request_json) {
        Ok(actions) => to_json(&refactor_this_actions(actions)),
        Err(e) => to_json(&json!({ "error": e })),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OptimizeInput {
    old: String,
    new: Option<String>,
    #[serde(default)]
    removed: usize,
    #[serde(default)]
    rearranged: bool,
}

/// `{ old, new, removed, rearranged }` → `OptimizeAnswer` (like `imports_optimize`): the
/// editor change and the notification.
#[wasm_bindgen]
pub fn imports_optimize_answer(request_json: &str) -> String {
    match parse::<OptimizeInput>(request_json) {
        Ok(i) => {
            let message = forge_codeintel::imports::summary(i.removed, i.rearranged, 0);
            to_json(&forge_codeintel::imports::OptimizeAnswer::new(
                &i.old,
                i.new.as_deref(),
                message,
            ))
        }
        Err(e) => to_json(&json!({ "error": e })),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FolderInput {
    /// Every file below the chosen folders.
    files: Vec<String>,
    /// VCS-changed files ("Only VCS changed files").
    changed: Option<std::collections::BTreeSet<String>>,
}

/// `{ files, changed? }` → the files Optimize Imports processes.
#[wasm_bindgen]
pub fn imports_targets(request_json: &str) -> String {
    match parse::<FolderInput>(request_json) {
        Ok(i) => to_json(&forge_codeintel::imports::optimize_targets(
            &i.files,
            i.changed.as_ref(),
        )),
        Err(e) => to_json(&json!({ "error": e })),
    }
}

/// `{ old, new }` → the one LSP edit turning `old` into `new` (or null).
#[wasm_bindgen]
pub fn imports_minimal_edit(request_json: &str) -> String {
    match parse::<OptimizeInput>(request_json) {
        Ok(i) => to_json(
            &i.new
                .as_deref()
                .and_then(|n| forge_codeintel::imports::minimal_edit(&i.old, n)),
        ),
        Err(e) => to_json(&json!({ "error": e })),
    }
}

/// `{ files, changed, removed }` → the folder notification.
#[wasm_bindgen]
pub fn imports_folder_summary(files: usize, changed: usize, removed: usize) -> String {
    forge_codeintel::imports::folder_summary(files, changed, removed)
}
