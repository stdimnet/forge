//! TODO scanning (`forge-find::todo`) and bookmarks (`forge-bookmarks`), mirroring the
//! `todo_scan` and `bookmarks_apply` IPC commands over in-memory state.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

#[derive(Deserialize)]
struct TodoFiles {
    request: forge_find::todo::TodoRequest,
    /// `[path, text]` of every project file (the browser-mode demo project).
    files: Vec<(String, String)>,
}

/// `{ request, files }` → `TodoResult`.
#[wasm_bindgen]
pub fn todo_scan(request_json: &str) -> String {
    match serde_json::from_str::<TodoFiles>(request_json) {
        Ok(r) => to_json(&forge_find::todo::scan_todos(
            &r.request,
            r.files.iter().map(|(p, t)| (p.as_str(), t.as_str())),
        )),
        Err(e) => to_json(&serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct BookmarksApply {
    /// The current state; `null` starts empty.
    state: Option<forge_bookmarks::Bookmarks>,
    ops: Vec<forge_bookmarks::Op>,
}

/// `{ state, ops }` → the new `Bookmarks`.
#[wasm_bindgen]
pub fn bookmarks_apply(request_json: &str) -> String {
    match serde_json::from_str::<BookmarksApply>(request_json) {
        Ok(r) => {
            let mut state = r.state.unwrap_or_default();
            for op in r.ops {
                state.apply(op);
            }
            to_json(&state)
        }
        Err(e) => to_json(&serde_json::json!({ "error": e.to_string() })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_json() {
        let out = todo_scan(r#"{"request":{},"files":[["a.php","<?php // TODO: x"]]}"#);
        assert!(out.contains(r#""text":"TODO: x""#), "{out}");
        let out =
            bookmarks_apply(r#"{"state":null,"ops":[{"op":"toggle","path":"a.php","line":2}]}"#);
        assert!(out.contains(r#""line":2"#), "{out}");
    }
}
