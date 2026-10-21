//! File and code templates (`forge-file-templates`), mirroring `templates_list`,
//! `templates_validate`, `templates_php_namespace`, `templates_php_directory` and the rendering
//! part of `templates_create` (the browser mock keeps the new file in memory).

use forge_file_templates::{self as templates, psr4, CustomTemplate, NewFileRequest};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn parse<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| to_json(&serde_json::json!({ "error": e.to_string() })))
}

fn error(message: impl ToString) -> String {
    to_json(&serde_json::json!({ "error": message.to_string() }))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ListRequest {
    custom: serde_json::Value,
}

/// `{ custom }` (the `fileTemplates.custom` value) → `{ templates, variables }`.
#[wasm_bindgen]
pub fn templates_list(request_json: &str) -> String {
    match parse::<ListRequest>(request_json) {
        Ok(r) => to_json(&serde_json::json!({
            "templates": templates::catalog(&templates::parse_custom(&r.custom)),
            "variables": templates::predefined_variables(),
        })),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct ValidateRequest {
    text: String,
}

/// `{ text }` → `string | null`.
#[wasm_bindgen]
pub fn templates_validate(request_json: &str) -> String {
    match parse::<ValidateRequest>(request_json) {
        Ok(r) => to_json(&templates::validate(&r.text).err().map(|e| e.to_string())),
        Err(e) => e,
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Psr4Request {
    /// Text of composer.json (`null` when the project has none).
    composer_json: Option<String>,
    /// Its project-relative path.
    composer_path: String,
    dir: Option<String>,
    namespace: Option<String>,
}

fn roots(r: &Psr4Request) -> Vec<psr4::Psr4Root> {
    r.composer_json
        .as_deref()
        .map(|t| psr4::roots(t, &psr4::composer_dir(&r.composer_path)))
        .unwrap_or_default()
}

/// `{ composerJson, composerPath, dir }` → `string | null`.
#[wasm_bindgen]
pub fn templates_php_namespace(request_json: &str) -> String {
    match parse::<Psr4Request>(request_json) {
        Ok(r) => to_json(&psr4::namespace_for_dir(&roots(&r), r.dir.as_deref().unwrap_or(""))),
        Err(e) => e,
    }
}

/// `{ composerJson, composerPath, namespace }` → `string | null`.
#[wasm_bindgen]
pub fn templates_php_directory(request_json: &str) -> String {
    match parse::<Psr4Request>(request_json) {
        Ok(r) => to_json(&psr4::dir_for_namespace(&roots(&r), r.namespace.as_deref().unwrap_or(""))),
        Err(e) => e,
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct NewFileBridge {
    request: NewFileRequest,
    custom: serde_json::Value,
}

/// `{ request: NewFileRequest, custom }` → `NewFile`.
#[wasm_bindgen]
pub fn templates_new_file(request_json: &str) -> String {
    match parse::<NewFileBridge>(request_json) {
        Ok(r) => {
            let custom: Vec<CustomTemplate> = templates::parse_custom(&r.custom);
            match templates::new_file(&r.request, &custom) {
                Ok(f) => to_json(&f),
                Err(e) => error(e.message),
            }
        }
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges() {
        let list: serde_json::Value = serde_json::from_str(&templates_list("{}")).unwrap();
        assert!(list["templates"].as_array().unwrap().len() > 10);
        assert_eq!(templates_validate(r##"{"text":"#end"}"##), "\"Line 1: #end without #if\"");
        let ns = templates_php_namespace(
            r#"{"composerJson":"{\"autoload\":{\"psr-4\":{\"App\\\\\":\"app/\"}}}","composerPath":"composer.json","dir":"app/Models"}"#,
        );
        assert_eq!(ns, r#""App\\Models""#);
        let created = templates_new_file(r#"{"request":{"template":"CSS File","dir":"public","name":"site"}}"#);
        assert!(created.contains("\"path\":\"public/site.css\""), "{created}");
        assert!(templates_new_file(r#"{"request":{"template":"Nope","name":"x"}}"#).contains("error"));
    }
}
