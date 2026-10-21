//! Live templates (`forge-livetemplates`), mirroring the `live_templates_*` IPC commands.
//! The editor also calls `live_templates_expand` synchronously (Tab must decide at once
//! whether it expands), in the desktop app and in browser mode alike.

use serde::de::DeserializeOwned;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn run<T: DeserializeOwned, R: serde::Serialize>(json: &str, f: impl FnOnce(T) -> R) -> String {
    match serde_json::from_str::<T>(json) {
        Ok(v) => to_json(&f(v)),
        Err(e) => to_json(&json!({ "error": e.to_string() })),
    }
}

/// `TemplateSettings` → `Catalog`.
#[wasm_bindgen]
pub fn live_templates_catalog(settings_json: &str) -> String {
    run(settings_json, |s| forge_livetemplates::catalog(&s))
}

/// `ApplicableRequest` → `ApplicableResult`.
#[wasm_bindgen]
pub fn live_templates_applicable(request_json: &str) -> String {
    run(request_json, |r| forge_livetemplates::applicable(&r))
}

/// `ExpandRequest` → `Expansion | null`.
#[wasm_bindgen]
pub fn live_templates_expand(request_json: &str) -> String {
    run(request_json, |r| forge_livetemplates::expand(&r))
}

/// `SurroundRequest` → `SurroundItem[]`.
#[wasm_bindgen]
pub fn live_templates_surround_items(request_json: &str) -> String {
    run(request_json, |r| forge_livetemplates::surround_items(&r))
}

/// `SurroundRequest` (with `id`) → `Expansion | null`.
#[wasm_bindgen]
pub fn live_templates_surround(request_json: &str) -> String {
    run(request_json, |r| forge_livetemplates::surround_apply(&r))
}

/// `UnwrapRequest` → `UnwrapOption[]`.
#[wasm_bindgen]
pub fn live_templates_unwrap(request_json: &str) -> String {
    run(request_json, |r| forge_livetemplates::unwrap_options(&r))
}

/// `{ text }` → variable names.
#[wasm_bindgen]
pub fn live_templates_variables(request_json: &str) -> String {
    #[derive(serde::Deserialize)]
    struct Req {
        text: String,
    }
    run(request_json, |r: Req| {
        forge_livetemplates::variable_names(&r.text)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_json() {
        let out = live_templates_expand(r#"{"text":"<?php\nfore","offset":10,"languageId":"php"}"#);
        assert!(out.contains(r#""templateId":"PHP/fore""#), "{out}");
        assert_eq!(live_templates_expand(r#"{"text":"x","offset":1}"#), "null");
        assert!(live_templates_catalog("{}").contains(r#""name":"PHP""#));
        assert!(live_templates_unwrap("{}").contains("error"));
        assert_eq!(
            live_templates_variables(r#"{"text":"$A$ $END$"}"#),
            r#"["A"]"#
        );
    }
}
