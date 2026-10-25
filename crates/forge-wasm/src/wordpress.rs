//! WordPress (`phpforge-fw-wordpress`) and CMS basics (`phpforge-fw-cms`) for the
//! browser-mode mocks of the `wordpress_*` and `cms_*` IPC commands: the demo project's
//! files are indexed and the query answered exactly like the desktop backend does.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

use crate::to_json;

#[derive(Deserialize)]
struct Request<Q> {
    /// The project's files by relative path.
    files: HashMap<String, String>,
    query: Q,
}

fn run<Q: for<'de> Deserialize<'de>>(
    request_json: &str,
    answer: impl FnOnce(&HashMap<String, String>, Q) -> Value,
) -> String {
    match serde_json::from_str::<Request<Q>>(request_json) {
        Ok(r) => to_json(&answer(&r.files, r.query)),
        Err(e) => to_json(&json!({ "error": e.to_string() })),
    }
}

/// `{ files, query }` → the answer of `phpforge_fw_wordpress::api::answer`.
#[wasm_bindgen]
pub fn wordpress_query(request_json: &str) -> String {
    run(request_json, |files, q| {
        phpforge_fw_wordpress::api::answer(&phpforge_fw_wordpress::scan_files(files), q)
    })
}

/// `{ files, query }` → the answer of `phpforge_fw_cms::api::answer`.
#[wasm_bindgen]
pub fn cms_query(request_json: &str) -> String {
    run(request_json, |files, q| {
        phpforge_fw_cms::api::answer(&phpforge_fw_cms::scan_files(files), q)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_like_the_backend() {
        let req = json!({
            "files": { "acme.php": "<?php\n/* Plugin Name: Acme */\ndo_action( 'acme_ready' );\n" },
            "query": "hooks",
        });
        let out: Value = serde_json::from_str(&wordpress_query(&req.to_string())).unwrap();
        assert_eq!(out[0]["name"], "acme_ready");
        let out: Value = serde_json::from_str(&cms_query(
            &json!({ "files": {}, "query": "detect" }).to_string(),
        ))
        .unwrap();
        assert_eq!(out, json!([]));
        let bad: Value = serde_json::from_str(&cms_query("{}")).unwrap();
        assert!(bad["error"].is_string());
    }
}
