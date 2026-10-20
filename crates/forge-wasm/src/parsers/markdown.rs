//! Markdown (`forge-markdown`) for browser mode, mirroring `markdown_parse` / `markdown_hover`.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_markdown_and_hovers() {
        assert!(markdown_parse(r#"{"text":"**a**"}"#).contains(r#""t":"strong""#));
        assert!(markdown_hover(r#"{"markdown":"__App\\Models\\User__"}"#).contains("App\\\\Models"));
    }
}
