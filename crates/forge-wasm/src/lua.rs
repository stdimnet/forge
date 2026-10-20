//! LuaForge (`luaforge-lang-lua`), mirroring the `lua_*` IPC commands for the browser-mode
//! mock: the mock hands over the demo project's Lua files once (`lua_index_files`), then
//! asks for structure, hover, Go to Declaration, symbol search and run markers.

use std::cell::RefCell;
use std::collections::BTreeMap;

use luaforge_lang_lua::{hover, run, structure, ProjectIndex, SearchKind};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

thread_local! {
    static INDEX: RefCell<ProjectIndex> = RefCell::new(ProjectIndex::new(&[]));
}

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexRequest {
    files: BTreeMap<String, String>,
    #[serde(default)]
    require_paths: Vec<String>,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileRequest {
    path: String,
    #[serde(default)]
    offset: usize,
    content: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchRequest {
    query: String,
    #[serde(default)]
    kind: SearchKind,
    limit: Option<usize>,
}

/// Replace the indexed files: `{ files: { path: text }, requirePaths?, name? }` -> stats.
#[wasm_bindgen]
pub fn lua_index_files(request_json: &str) -> String {
    let req: IndexRequest = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    INDEX.with(|i| {
        let mut index = i.borrow_mut();
        index.clear();
        index.set_require_paths(&req.require_paths);
        index.set_name(&req.name);
        for (path, text) in &req.files {
            index.set_file(path, text);
        }
        to_json(&index.stats())
    })
}

fn with_file<T: serde::Serialize>(
    request_json: &str,
    f: impl FnOnce(&ProjectIndex, &FileRequest) -> T,
) -> String {
    let req: FileRequest = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    INDEX.with(|i| {
        let mut index = i.borrow_mut();
        index.update(&req.path, req.content.as_deref());
        to_json(&f(&index, &req))
    })
}

/// `{ path, content? }` -> `FileStructure | null`.
#[wasm_bindgen]
pub fn lua_file_structure(request_json: &str) -> String {
    with_file(request_json, |index, req| {
        structure::file_structure(index, &req.path)
    })
}

/// `{ path, offset, content? }` -> `Hover | null`.
#[wasm_bindgen]
pub fn lua_hover(request_json: &str) -> String {
    with_file(request_json, |index, req| {
        hover::hover(index, &req.path, req.offset)
    })
}

/// `{ path, offset, content? }` -> `Location[]`.
#[wasm_bindgen]
pub fn lua_goto(request_json: &str) -> String {
    with_file(request_json, |index, req| index.goto(&req.path, req.offset))
}

/// `{ path, content? }` -> `RunMarker[]`.
#[wasm_bindgen]
pub fn lua_run_markers(request_json: &str) -> String {
    with_file(request_json, |index, req| {
        run::run_markers(index, &req.path)
    })
}

/// `{ query, kind: "class" | "symbol", limit? }` -> `SymbolHit[]`.
#[wasm_bindgen]
pub fn lua_search_symbols(request_json: &str) -> String {
    let req: SearchRequest = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    INDEX.with(|i| {
        to_json(
            &i.borrow()
                .search(&req.query, req.kind, req.limit.unwrap_or(100)),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_then_query() {
        let stats = lua_index_files(
            r#"{"files": {"main.lua": "local util = require('util')\nutil.go()\n", "util.lua": "local M = {}\nfunction M.go() end\nreturn M\n"}}"#,
        );
        assert!(stats.contains("\"files\":2"), "{stats}");
        let goto = lua_goto(r#"{"path": "main.lua", "offset": 35}"#);
        assert!(goto.contains("\"path\":\"util.lua\""), "{goto}");
        let hits = lua_search_symbols(r#"{"query": "go", "kind": "symbol"}"#);
        assert!(hits.contains("\"name\":\"go\""), "{hits}");
        let s = lua_file_structure(r#"{"path": "util.lua"}"#);
        assert!(s.contains("\"name\":\"M\""), "{s}");
        let m = lua_run_markers(r#"{"path": "main.lua"}"#);
        assert!(m.contains("\"kind\":\"script\""), "{m}");
        assert!(lua_hover("nope").contains("error"));
    }
}
