//! Compare Directories (`forge-diff`) for browser mode: the mock lists the demo folders and
//! the folder diff itself is the same Rust code as in the desktop app.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

#[derive(Deserialize)]
struct Item {
    path: String,
    dir: bool,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct Request {
    left: Vec<Item>,
    right: Vec<Item>,
}

fn listing(items: Vec<Item>) -> forge_diff::Listing {
    forge_diff::Listing {
        entries: items
            .into_iter()
            .map(|i| {
                if i.dir {
                    forge_diff::ListedEntry::dir(i.path, None)
                } else {
                    forge_diff::ListedEntry::file(i.path, i.text.as_bytes(), None)
                }
            })
            .collect(),
        truncated: false,
    }
}

/// `{ left: {path, dir, text}[], right: … }` (paths relative to each folder) → `DirCompare`.
#[wasm_bindgen]
pub fn compare_dir_listings(request_json: &str) -> String {
    match serde_json::from_str::<Request>(request_json) {
        Ok(r) => to_json(&forge_diff::compare_listings(
            &listing(r.left),
            &listing(r.right),
        )),
        Err(e) => to_json(&serde_json::json!({ "error": e.to_string() })),
    }
}
