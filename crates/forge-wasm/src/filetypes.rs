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

/// Mirrors the `filetypes_list` IPC command: `DetectRequest` → `FileTypeRow[]`.
#[wasm_bindgen]
pub fn filetypes_list(request_json: &str) -> String {
    match serde_json::from_str::<DetectRequest>(request_json) {
        Ok(req) => REGISTRY.with(|r| {
            to_json(
                &r.borrow()
                    .rows(&req.inactive_plugins, &req.user_file_types()),
            )
        }),
        Err(e) => to_json(&serde_json::json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
struct IgnoredRequest {
    names: Vec<String>,
    patterns: String,
}

/// `{ names, patterns }` → for each name whether "Ignored Files and Folders" hides it (the
/// filter of `fs_list_dir`).
#[wasm_bindgen]
pub fn filetypes_ignored(request_json: &str) -> String {
    match serde_json::from_str::<IgnoredRequest>(request_json) {
        Ok(req) => to_json(
            &req.names
                .iter()
                .map(|n| forge_filetypes::matches_glob_list(n, &req.patterns))
                .collect::<Vec<_>>(),
        ),
        Err(e) => to_json(&serde_json::json!({ "error": e.to_string() })),
    }
}

/// Mirrors the `filetypes_template_delimiters` IPC command.
#[wasm_bindgen]
pub fn filetypes_template_delimiters() -> String {
    to_json(&forge_filetypes::delimiter_catalog())
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

    #[test]
    fn lists_types_with_user_patterns() {
        let out = filetypes_list(
            r#"{"files":[],"associations":[{"fileType":"php","pattern":"*.module"}],"customTypes":[{"name":"DSL"}]}"#,
        );
        let rows: Vec<serde_json::Value> = serde_json::from_str(&out).unwrap();
        let php = rows.iter().find(|r| r["id"] == "php").unwrap();
        assert_eq!(php["added"][0], "*.module");
        assert!(rows
            .iter()
            .any(|r| r["id"] == "custom-dsl" && r["custom"] == true));
    }

    #[test]
    fn resolves_template_delimiters() {
        let out = filetypes_detect(
            r#"{"files":[{"path":"a.tex.tpl"}],"templateDelimiters":[{"template":"smarty","language":"latex","left":"[[","right":"]]"}]}"#,
        );
        assert!(out.contains(r#""left":"[[""#), "{out}");
        assert!(filetypes_template_delimiters().contains(r#""id":"gotemplate""#));
    }
}
