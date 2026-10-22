//! The three-way merge of the merge conflict resolver (`forge_vcs::merge`) for browser mode:
//! the git mock builds `git_merge_file` answers with it.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::parsers::parse;
use crate::to_json;

#[derive(Deserialize)]
struct MergeRequest {
    base: String,
    left: String,
    right: String,
}

/// `{ base, left, right }` → `MergeModel`.
#[wasm_bindgen]
pub fn vcs_merge(request_json: &str) -> String {
    match parse::<MergeRequest>(request_json) {
        Ok(r) => to_json(&forge_vcs::merge::merge_three_way(
            &r.base, &r.left, &r.right,
        )),
        Err(e) => e,
    }
}

/// `SelectionFacts` → `LogActionStates`: which Git Log actions the git mock's selection allows.
#[wasm_bindgen]
pub fn vcs_log_action_states(facts_json: &str) -> String {
    match parse::<forge_vcs::rebase::SelectionFacts>(facts_json) {
        Ok(f) => to_json(&forge_vcs::rebase::log_action_states(&f)),
        Err(e) => e,
    }
}

/// `RebaseCommit[]` (oldest first) → the initial `RebaseEntry[]` of the rebase dialog.
#[wasm_bindgen]
pub fn vcs_rebase_autosquash(commits_json: &str) -> String {
    match parse::<Vec<forge_vcs::RebaseCommit>>(commits_json) {
        Ok(c) => to_json(&forge_vcs::rebase::autosquash(&c)),
        Err(e) => e,
    }
}

#[derive(serde::Serialize)]
struct Todo {
    steps: Vec<forge_vcs::rebase::TodoStep>,
    messages: Vec<String>,
}

/// `RebaseEntry[]` (oldest first) → `{ steps, messages }`, the todo the git mock applies.
#[wasm_bindgen]
pub fn vcs_rebase_todo(entries_json: &str) -> String {
    match parse::<Vec<forge_vcs::RebaseEntry>>(entries_json) {
        Ok(e) => match forge_vcs::rebase::plan_todo(&e) {
            Ok((steps, messages)) => to_json(&Todo { steps, messages }),
            Err(err) => to_json(&serde_json::json!({ "error": err })),
        },
        Err(e) => e,
    }
}
