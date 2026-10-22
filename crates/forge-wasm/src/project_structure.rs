//! Project structure (`forge_scopes::structure`) and the Project view's rows
//! (`forge_scopes::view`), called synchronously by the UI in both modes ("Mark Directory
//! as", Settings | Directories, File Nesting dialog) and by the browser-mode mocks
//! (`project_tree_list`, excluded flags of `fs_list_dir`, Find in Files).

use std::collections::HashMap;

use forge_scopes::structure::{self, ExcludeSet, MarkAction, StructureConfig};
use forge_scopes::view::{self, ViewEntry, ViewOptions};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn run<T: for<'de> Deserialize<'de>, R: serde::Serialize>(
    json: &str,
    f: impl FnOnce(T) -> R,
) -> String {
    match serde_json::from_str::<T>(json) {
        Ok(req) => to_json(&f(req)),
        Err(e) => to_json(&serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateReq {
    config: StructureConfig,
    path: String,
    #[serde(default)]
    is_dir: bool,
}

/// `{ config, path, isDir }` → `FolderState`.
#[wasm_bindgen]
pub fn project_structure_state(request_json: &str) -> String {
    run(request_json, |r: StateReq| {
        structure::state(&r.config, &r.path, r.is_dir)
    })
}

#[derive(Deserialize)]
struct MarkReq {
    config: StructureConfig,
    path: String,
    action: MarkAction,
}

/// `{ config, path, action }` → the new `projectStructure.folders`.
#[wasm_bindgen]
pub fn project_structure_mark(request_json: &str) -> String {
    run(request_json, |r: MarkReq| {
        structure::mark(&r.config, &r.path, r.action)
    })
}

#[derive(Deserialize)]
struct DescribeReq {
    config: StructureConfig,
}

/// `{ config }` → `FolderRow[]` (Settings | Directories).
#[wasm_bindgen]
pub fn project_structure_describe(request_json: &str) -> String {
    run(request_json, |r: DescribeReq| {
        structure::describe(&r.config)
    })
}

#[derive(Deserialize)]
struct ExcludedReq {
    config: StructureConfig,
    /// `[path, isDir]` pairs.
    paths: Vec<(String, bool)>,
}

/// `{ config, paths: [path, isDir][] }` → `boolean[]` (excluded or hidden).
#[wasm_bindgen]
pub fn project_structure_excluded(request_json: &str) -> String {
    run(request_json, |r: ExcludedReq| {
        let set = ExcludeSet::new(&r.config);
        r.paths
            .iter()
            .map(|(p, dir)| set.excludes(p, *dir))
            .collect::<Vec<bool>>()
    })
}

#[derive(Deserialize)]
struct ArrangeReq {
    entries: Vec<ViewEntry>,
    /// Listings of other folders (Compact Middle Packages looks into them).
    #[serde(default)]
    listing: HashMap<String, Vec<ViewEntry>>,
    #[serde(default)]
    options: ViewOptions,
}

/// `{ entries, listing, options }` → `TreeRow[]`.
#[wasm_bindgen]
pub fn project_view_arrange(request_json: &str) -> String {
    run(request_json, |r: ArrangeReq| {
        let listing = r.listing;
        let mut list = |dir: &str| listing.get(dir).cloned().unwrap_or_default();
        view::arrange(r.entries, &mut list, &r.options)
    })
}

/// PHPStorm's predefined File Nesting rules.
#[wasm_bindgen]
pub fn project_view_nesting_defaults() -> String {
    to_json(&view::default_nesting_rules())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_json() {
        let state: serde_json::Value = serde_json::from_str(&project_structure_state(
            r#"{"config":{"folders":[],"excludePatterns":""},"path":"node_modules","isDir":true}"#,
        ))
        .unwrap();
        assert_eq!(state["kind"], "excluded");
        assert_eq!(state["reason"], "default");
        let marks =
            project_structure_mark(r#"{"config":{},"path":"storage/logs","action":"excluded"}"#);
        assert_eq!(marks, r#"[{"path":"storage/logs","kind":"excluded"}]"#);
        let flags = project_structure_excluded(
            r#"{"config":{"excludePatterns":"*.log"},"paths":[["a.log",false],["src",true]]}"#,
        );
        assert_eq!(flags, "[true,false]");
        let rows: serde_json::Value = serde_json::from_str(&project_view_arrange(
            r#"{"entries":[{"name":"a.ts","path":"a.ts","kind":"file"},{"name":"a.js","path":"a.js","kind":"file"}]}"#,
        ))
        .unwrap();
        assert_eq!(rows[0]["nested"][0]["name"], "a.js");
        assert!(project_view_nesting_defaults().contains("composer.lock"));
    }
}
