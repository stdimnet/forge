//! WebAssembly bridge for the browser-mode UI.
//!
//! Forge's rule is "Rust computes, TypeScript renders". In the desktop app the UI calls Rust
//! through Tauri IPC; in browser mode (`pnpm dev`, screenshots, tests) the same Rust code runs
//! as WebAssembly so mocks never re-implement logic in TypeScript.
//!
//! Only **pure** crates belong here (no file system, processes, network or tokio). Each area
//! adds a module that wraps its crate with `#[wasm_bindgen]` functions taking and returning
//! JSON strings, mirroring the matching IPC command:
//!
//! ```ignore
//! #[wasm_bindgen]
//! pub fn keymap_resolve(request_json: &str) -> String { … }
//! ```
//!
//! Rebuild the bundle with `pnpm --dir app/ui wasm` (output: `app/ui/src/wasm/pkg`).

mod parsers;

use wasm_bindgen::prelude::*;

mod filetypes;
mod keymap;
mod scratches;
mod search;
mod settings;

/// Version of the bridge, used by the UI to check that the bundle is current.
#[wasm_bindgen]
pub fn bridge_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Serialize a result for JS; errors become `{ "error": "…" }`.
pub(crate) fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|e| format!("{{\"error\":{:?}}}", e.to_string()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::bridge_version().is_empty());
        assert_eq!(super::to_json(&vec![1, 2]), "[1,2]");
    }
}
