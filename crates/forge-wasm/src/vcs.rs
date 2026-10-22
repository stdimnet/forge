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
