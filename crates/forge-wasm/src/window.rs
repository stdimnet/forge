//! Window chrome (`forge_window`), mirroring the `window_chrome` IPC command for browser mode.

use forge_window::{resolve, Desktop, Os, SystemLayouts};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

#[derive(Deserialize)]
struct Request {
    setting: String,
    os: Os,
    desktop: Desktop,
}

/// `{ setting, os, desktop }` → `WindowChrome`.
#[wasm_bindgen]
pub fn window_chrome(request_json: &str) -> String {
    match serde_json::from_str::<Request>(request_json) {
        Ok(r) => to_json(&resolve(
            &r.setting,
            r.os,
            r.desktop,
            &SystemLayouts::default(),
        )),
        Err(e) => to_json(&serde_json::json!({ "error": e.to_string() })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_json() {
        let out = window_chrome(r#"{"setting":"gnome","os":"linux","desktop":"kde"}"#);
        assert!(out.contains(r#""style":"gnome""#), "{out}");
        assert!(window_chrome("{}").contains("error"));
    }
}
