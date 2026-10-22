//! Code style schemes (`forge-codestyle`), mirroring the `code_style_*` commands and
//! `editor_file_format`. The settings layers come from the browser-mode settings store.

use forge_codestyle::scheme::Layer;
use forge_codestyle::{ExportFormat, Layers, PostFormatRequest, SchemesState, Values};
use forge_text::FormatRequest;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

#[derive(Deserialize)]
struct LayersJson {
    #[serde(default)]
    user: Layer,
    /// `null` without a project.
    #[serde(default)]
    project: Option<Layer>,
}

impl LayersJson {
    fn layers(&self) -> Layers<'_> {
        Layers {
            user: &self.user,
            project: self.project.as_ref(),
        }
    }
}

/// `()` → `CodeStyleModel`.
#[wasm_bindgen]
pub fn codestyle_model() -> String {
    to_json(forge_codestyle::model())
}

/// `{ user, project }` → `SchemesState`.
#[wasm_bindgen]
pub fn codestyle_schemes(request_json: &str) -> String {
    match serde_json::from_str::<LayersJson>(request_json) {
        Ok(r) => to_json(&forge_codestyle::load(r.layers())),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct SaveRequest {
    #[serde(flatten)]
    layers: LayersJson,
    schemes: SchemesState,
}

/// `{ user, project, schemes }` → `{ writes: SettingWrite[] }` or `{ error }`.
#[wasm_bindgen]
pub fn codestyle_plan_save(request_json: &str) -> String {
    match serde_json::from_str::<SaveRequest>(request_json) {
        Ok(r) => match forge_codestyle::plan_save(r.layers.layers(), &r.schemes) {
            Ok(writes) => to_json(&json!({ "writes": writes })),
            Err(e) => error(e),
        },
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct PreviewRequest {
    language: String,
    values: Values,
}

/// `{ language, values }` → the formatted sample (or `null`).
#[wasm_bindgen]
pub fn codestyle_preview(request_json: &str) -> String {
    match serde_json::from_str::<PreviewRequest>(request_json) {
        Ok(r) => to_json(&forge_codestyle::preview(&r.language, &r.values)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct ExportRequest {
    name: String,
    values: Values,
    format: ExportFormat,
}

/// `{ name, values, format }` → the exported text.
#[wasm_bindgen]
pub fn codestyle_export(request_json: &str) -> String {
    match serde_json::from_str::<ExportRequest>(request_json) {
        Ok(r) => to_json(&forge_codestyle::export(&r.name, &r.values, r.format)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct ImportRequest {
    text: String,
}

/// `{ text }` → `ImportedScheme` or `{ error }`.
#[wasm_bindgen]
pub fn codestyle_import(request_json: &str) -> String {
    match serde_json::from_str::<ImportRequest>(request_json) {
        Ok(r) => match forge_codestyle::import(&r.text) {
            Ok(s) => to_json(&s),
            Err(e) => error(e),
        },
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct PostFormat {
    #[serde(flatten)]
    layers: LayersJson,
    request: PostFormatRequest,
}

/// `{ user, project, request }` → the text after the code style pass.
#[wasm_bindgen]
pub fn codestyle_post_format(request_json: &str) -> String {
    match serde_json::from_str::<PostFormat>(request_json) {
        Ok(r) => {
            let values = forge_codestyle::selected_values(r.layers.layers());
            to_json(&forge_codestyle::post_format(&r.request, &values))
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct FileFormat {
    #[serde(flatten)]
    layers: LayersJson,
    /// `FormatRequest` with the effective settings and `.editorconfig` files.
    request: FormatRequest,
}

/// `{ user, project, request }` → `FileFormat`, with the selected scheme's code style
/// (as `editor_file_format` resolves it).
#[wasm_bindgen]
pub fn codestyle_file_format(request_json: &str) -> String {
    match serde_json::from_str::<FileFormat>(request_json) {
        Ok(r) => {
            let mut req = r.request;
            let effective: Map<String, Value> = req.settings;
            req.settings = forge_codestyle::effective_settings(&effective, r.layers.layers());
            req.language_id = req.language_id.map(|id| {
                forge_codestyle::model()
                    .language_for_file_type(&id)
                    .map_or(id, |l| l.id.clone())
            });
            to_json(&forge_text::resolve_format(&req))
        }
        Err(e) => error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_format_uses_the_scheme() {
        let out = codestyle_file_format(
            r#"{"user": {"css.codeStyle.indent": 2}, "project": null,
                "request": {"path": "a.scss", "languageId": "scss", "settings": {}}}"#,
        );
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["indentLabel"], "2 spaces");
        assert_eq!(v["codeStylePage"], "editor.code-style.css");
    }

    #[test]
    fn plans_and_previews() {
        let state = codestyle_schemes(r#"{"user": {}, "project": {}}"#);
        let req = format!(r#"{{"user": {{}}, "project": {{}}, "schemes": {state}}}"#);
        let v: Value = serde_json::from_str(&codestyle_plan_save(&req)).unwrap();
        assert_eq!(v["writes"], json!([]));
        let preview = codestyle_preview(r#"{"language": "json", "values": {}}"#);
        assert!(preview.contains("forge/demo"));
        assert!(codestyle_import(r#"{"text": "nope"}"#).contains("error"));
    }
}
