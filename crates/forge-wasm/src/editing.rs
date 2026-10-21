//! Editor actions (`forge-editing`), mirroring `editor_postfix_templates`,
//! `editor_postfix_complete`, `editor_complete_statement` and `editor_expand_word`.

use forge_editing::{ExpandRequest, PostfixRequest, StatementRequest};
use wasm_bindgen::prelude::*;

use crate::to_json;

fn parse<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| to_json(&serde_json::json!({ "error": e.to_string() })))
}

/// `{}` → `TemplateInfo[]`.
#[wasm_bindgen]
pub fn editing_postfix_templates(_request_json: &str) -> String {
    to_json(&forge_editing::templates())
}

/// `PostfixRequest` → `PostfixCompletion | null`.
#[wasm_bindgen]
pub fn editing_postfix_complete(request_json: &str) -> String {
    match parse::<PostfixRequest>(request_json) {
        Ok(r) => to_json(&forge_editing::complete_postfix(&r)),
        Err(e) => e,
    }
}

/// `StatementRequest` → `StatementEdit | null`.
#[wasm_bindgen]
pub fn editing_complete_statement(request_json: &str) -> String {
    match parse::<StatementRequest>(request_json) {
        Ok(r) => to_json(&forge_editing::complete_statement(&r)),
        Err(e) => e,
    }
}

/// `ExpandRequest` → `ExpandResult`.
#[wasm_bindgen]
pub fn editing_expand_word(request_json: &str) -> String {
    match parse::<ExpandRequest>(request_json) {
        Ok(r) => to_json(&forge_editing::expand_word(&r)),
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_json() {
        let out = editing_postfix_complete(r#"{"language":"php","line":"$a.if"}"#);
        assert!(out.contains("\"key\":\"if\""), "{out}");
        assert_eq!(
            editing_complete_statement(r#"{"language":"md","text":"","offset":0}"#),
            "null"
        );
        assert!(editing_expand_word("{").contains("error"));
        assert!(editing_postfix_templates("{}").starts_with('['));
    }
}
