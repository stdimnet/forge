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

#[derive(Deserialize)]
struct BranchName {
    name: String,
    #[serde(default)]
    current: bool,
}

#[derive(Deserialize)]
struct BranchTreeRequest {
    #[serde(default)]
    local: Vec<BranchName>,
    #[serde(default)]
    remote: Vec<BranchName>,
    /// Configured remote names.
    #[serde(default)]
    remotes: Vec<String>,
}

/// `{ local: [{ name, current }], remote: [{ name }], remotes }` → `{ localTree, remoteTree }`:
/// the branches grouped into folders, as `git_branches` answers (`forge_vcs::branch_tree`).
#[wasm_bindgen]
pub fn vcs_branch_tree(request_json: &str) -> String {
    match parse::<BranchTreeRequest>(request_json) {
        Ok(r) => to_json(&serde_json::json!({
            "localTree": forge_vcs::branch_tree::local_branch_tree(
                r.local.iter().map(|b| (b.name.as_str(), b.current)),
            ),
            "remoteTree": forge_vcs::branch_tree::remote_branch_tree(
                r.remote.iter().map(|b| b.name.as_str()),
                &r.remotes,
            ),
        })),
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

#[derive(Deserialize)]
struct HunksRequest {
    base: String,
    current: String,
    #[serde(default)]
    include: Option<Vec<String>>,
}

/// `{ base, current }` → `ChangeHunk[]`; with `include` (hunk ids) → the text a partial
/// commit stores (`string`).
#[wasm_bindgen]
pub fn vcs_change_hunks(request_json: &str) -> String {
    match parse::<HunksRequest>(request_json) {
        Ok(r) => match r.include {
            Some(ids) => to_json(&forge_vcs::hunks::apply_hunks(&r.base, &r.current, &ids)),
            None => to_json(&forge_vcs::hunks::change_hunks(&r.base, &r.current)),
        },
        Err(e) => e,
    }
}

/// One changelist operation of the git mock.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ChangelistOp {
    View,
    Create {
        name: String,
        #[serde(default)]
        comment: String,
        #[serde(default)]
        active: bool,
    },
    Edit {
        id: String,
        name: String,
        #[serde(default)]
        comment: String,
        #[serde(default)]
        active: bool,
    },
    SetActive {
        id: String,
    },
    Delete {
        id: String,
    },
    Move {
        paths: Vec<String>,
        #[serde(default)]
        hunks: Option<Vec<String>>,
        target: String,
        #[serde(default)]
        create: bool,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangelistRequest {
    /// The stored state (`Changelists` JSON), `null` at first.
    state: Option<String>,
    /// Changed tracked files.
    changed: Vec<String>,
    /// Current hunk ids per file.
    #[serde(default)]
    hunks: std::collections::BTreeMap<String, Vec<String>>,
    op: ChangelistOp,
}

#[derive(serde::Serialize)]
struct ChangelistAnswer {
    state: String,
    view: Vec<forge_vcs::ChangelistView>,
    error: Option<String>,
}

/// `{ state, changed, hunks, op }` → `{ state, view, error }`: the changelist model of
/// `git_changelist_*` for the git mock (`forge_vcs::changelists`).
#[wasm_bindgen]
pub fn vcs_changelists(request_json: &str) -> String {
    let r = match parse::<ChangelistRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let mut cl = r
        .state
        .as_deref()
        .map(forge_vcs::Changelists::from_json)
        .unwrap_or_default();
    cl.sync(&r.changed);
    let hunks_of = |p: &str| r.hunks.get(p).cloned().unwrap_or_default();
    let res = match r.op {
        ChangelistOp::View => Ok(()),
        ChangelistOp::Create {
            name,
            comment,
            active,
        } => cl.create(&name, &comment, active).map(drop),
        ChangelistOp::Edit {
            id,
            name,
            comment,
            active,
        } => cl.edit(&id, &name, &comment, active),
        ChangelistOp::SetActive { id } => cl.set_active(&id),
        ChangelistOp::Delete { id } => cl.delete(&id),
        ChangelistOp::Move {
            paths,
            hunks,
            target,
            create,
        } => (if create {
            cl.ensure(&target)
        } else {
            Ok(target)
        })
        .and_then(|target| match (hunks, paths.first()) {
            (Some(ids), Some(path)) => cl.move_hunks(path, &ids, &target, &hunks_of(path)),
            _ => cl.move_files(&paths, &target),
        }),
    };
    let moved: Vec<String> = cl.hunks.keys().cloned().collect();
    for p in moved {
        cl.normalize_file(&p, &hunks_of(&p));
    }
    to_json(&ChangelistAnswer {
        state: cl.to_json(),
        view: cl.view(&r.changed, &hunks_of),
        error: res.err(),
    })
}

#[derive(Deserialize)]
struct PreviewRequest {
    patch: String,
    #[serde(default)]
    base: String,
    /// Current texts of the project files the patch may touch.
    files: std::collections::BTreeMap<String, String>,
}

/// `{ patch, base, files }` → `PatchPreviewFile[]` (Apply Patch dialog in the git mock).
#[wasm_bindgen]
pub fn vcs_patch_preview(request_json: &str) -> String {
    match parse::<PreviewRequest>(request_json) {
        Ok(r) => to_json(&forge_vcs::patch::preview_patch(&r.patch, &r.base, &|p| {
            r.files.get(p).cloned()
        })),
        Err(e) => e,
    }
}
