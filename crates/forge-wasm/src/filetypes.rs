//! File type detection (`forge-filetypes`), mirroring the `filetypes_detect` IPC command.
//! The desktop app builds the registry from its plugin manifests; browser mode loads the
//! same manifests (the `builtinPlugins.json` snapshot) once with [`filetypes_load`].

use std::cell::RefCell;

use forge_filetypes::{DetectRequest, FileTypeRegistry, FileTypeSpec};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

thread_local! {
    static REGISTRY: RefCell<FileTypeRegistry> = RefCell::new(FileTypeRegistry::with_builtins());
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginFileTypes {
    id: String,
    #[serde(default)]
    contributions: Contributions,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Contributions {
    #[serde(default)]
    file_types: Vec<FileTypeSpec>,
}

/// Register the file types of plugins (`PluginInfo[]` JSON) on top of the bundled ones.
/// Returns the number of known file types.
#[wasm_bindgen]
pub fn filetypes_load(plugins_json: &str) -> String {
    let plugins: Vec<PluginFileTypes> = match serde_json::from_str(plugins_json) {
        Ok(p) => p,
        Err(e) => return to_json(&serde_json::json!({ "error": e.to_string() })),
    };
    let mut registry = FileTypeRegistry::with_builtins();
    for p in plugins {
        for t in p.contributions.file_types {
            registry.register(t, Some(&p.id));
        }
    }
    let count = registry.specs().count();
    REGISTRY.with(|r| *r.borrow_mut() = registry);
    to_json(&count)
}

/// `DetectRequest` → `FileTypeInfo[]`.
#[wasm_bindgen]
pub fn filetypes_detect(request_json: &str) -> String {
    match serde_json::from_str::<DetectRequest>(request_json) {
        Ok(req) => REGISTRY.with(|r| to_json(&r.borrow().resolve(&req))),
        Err(e) => to_json(&serde_json::json!({ "error": e.to_string() })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_plugin_types_and_detects() {
        let n = filetypes_load(
            r#"[{"id":"forge.compose","contributions":{"fileTypes":[{"id":"docker-compose","name":"Docker Compose","fileNames":["compose.yaml"],"indentSize":2}]}}]"#,
        );
        assert!(n.parse::<usize>().unwrap() > 50);
        let out = filetypes_detect(
            r#"{"files":[{"path":"compose.yaml"},{"path":"x.sql.twig"}],"inactivePlugins":[]}"#,
        );
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v[0]["id"], "docker-compose");
        assert_eq!(v[0]["indent"], "  ");
        assert_eq!(v[1]["label"], "Twig (SQL)");
        let out = filetypes_detect(
            r#"{"files":[{"path":"compose.yaml"}],"inactivePlugins":["forge.compose"]}"#,
        );
        assert!(out.contains(r#""id":"yaml""#), "{out}");
    }
}
