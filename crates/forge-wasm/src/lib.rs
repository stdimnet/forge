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
mod remote;

use wasm_bindgen::prelude::*;

mod agents;
mod cli_tools;
mod codeintel;
mod codestyle;
mod colorscheme;
mod compare;
mod composer;
mod containers;
mod cpp;
mod cpp_tools;
mod dbobjects;
mod deploy;
mod devenv;
mod editing;
mod emmet;
mod filetypes;
mod go;
mod hierarchy;
mod injections;
mod inspections;
mod java;
mod json_schema;
mod jupyter;
mod jvm_extras;
mod keymap;
mod kotlin;
mod livetemplates;
mod localhistory;
mod lua;
mod maintenance;
mod php_interpreters;
mod profiler;
mod project_structure;
mod python;
mod python_testing;
mod python_web;
mod runtools;
mod rust;
mod save_actions;
mod scopes;
mod scratches;
mod search;
mod settings;
mod spell;
mod spring;
mod sqlcode;
mod ssr;
mod stacktrace;
mod structure;
mod templates;
mod terminal;
mod test_frameworks;
mod testing;
mod text;
mod vcs;
mod viewers;
mod vim;
mod web;
mod window;
mod wordpress;
mod workbench;

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
