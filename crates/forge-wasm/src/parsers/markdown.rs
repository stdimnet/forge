//! Markdown (`forge-markdown`) for browser mode, mirroring `markdown_parse`, `markdown_hover`,
//! `markdown_document` and `markdown_anchor_line`.

use forge_markdown::Flavor;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use super::parse;
use crate::to_json;

#[derive(Deserialize)]
struct ParseRequest {
    text: String,
    #[serde(default)]
    flavor: Option<Flavor>,
}

/// `{ text, flavor? }` → `Block[]`.
#[wasm_bindgen]
pub fn markdown_parse(request_json: &str) -> String {
    match parse::<ParseRequest>(request_json) {
        Ok(r) => to_json(&forge_markdown::parse(
            &r.text,
            r.flavor.unwrap_or_default(),
        )),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct HoverRequest {
    markdown: String,
}

/// `{ markdown }` → `HoverDoc`.
#[wasm_bindgen]
pub fn markdown_hover(request_json: &str) -> String {
    match parse::<HoverRequest>(request_json) {
        Ok(r) => to_json(&forge_markdown::parse_hover(&r.markdown)),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct DocumentRequest {
    text: String,
}

/// `{ text }` → `Document` (the Markdown preview).
#[wasm_bindgen]
pub fn markdown_document(request_json: &str) -> String {
    match parse::<DocumentRequest>(request_json) {
        Ok(r) => to_json(&forge_markdown::parse_document(&r.text)),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct AnchorRequest {
    text: String,
    anchor: String,
}

/// `{ text, anchor }` → 0-based line or `null`.
#[wasm_bindgen]
pub fn markdown_anchor_line(request_json: &str) -> String {
    match parse::<AnchorRequest>(request_json) {
        Ok(r) => to_json(&forge_markdown::anchor_line(&r.text, &r.anchor)),
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_markdown_and_hovers() {
        assert!(markdown_parse(r#"{"text":"**a**"}"#).contains(r#""t":"strong""#));
        assert!(markdown_document(r#"{"text":"| a |\n|---|\n| 1 |"}"#).contains(r#""t":"table""#));
        assert_eq!(
            markdown_anchor_line(r#"{"text":"x\n\n# Go","anchor":"go"}"#),
            "2"
        );
        assert!(markdown_hover(r#"{"markdown":"__App\\Models\\User__"}"#).contains("App\\\\Models"));
    }
}
