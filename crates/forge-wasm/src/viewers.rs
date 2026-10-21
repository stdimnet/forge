//! Image metadata and CSV/TSV tables (`forge-viewers`) for browser mode, mirroring
//! `image_read` (from the mock's demo bytes), `csv_parse`, `csv_edit` and `csv_sort_order`.

use base64::Engine;
use forge_viewers::CsvEdit;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::parsers::parse;
use crate::to_json;

#[derive(Deserialize)]
struct ImageRequest {
    path: String,
    base64: String,
}

/// `{ path, base64 }` → `ImageData` (the mock reads demo images from base64).
#[wasm_bindgen]
pub fn image_describe(request_json: &str) -> String {
    match parse::<ImageRequest>(request_json) {
        Ok(r) => {
            let bytes = match base64::engine::general_purpose::STANDARD.decode(r.base64.trim()) {
                Ok(b) => b,
                Err(e) => return to_json(&serde_json::json!({ "error": e.to_string() })),
            };
            match forge_viewers::image_data(&r.path, &bytes) {
                Ok(data) => to_json(&data),
                Err(e) => to_json(&serde_json::json!({ "error": e })),
            }
        }
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct CsvParseRequest {
    text: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    delimiter: Option<char>,
}

/// `{ text, path?, delimiter? }` → `CsvTable`.
#[wasm_bindgen]
pub fn csv_parse(request_json: &str) -> String {
    match parse::<CsvParseRequest>(request_json) {
        Ok(r) => {
            let d = r.delimiter.unwrap_or_else(|| {
                forge_viewers::detect_delimiter(r.path.as_deref().unwrap_or(""), &r.text)
            });
            to_json(&forge_viewers::parse_table(&r.text, d))
        }
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct CsvEditRequest {
    text: String,
    delimiter: char,
    edits: Vec<CsvEdit>,
}

/// `{ text, delimiter, edits }` → the new text.
#[wasm_bindgen]
pub fn csv_edit(request_json: &str) -> String {
    match parse::<CsvEditRequest>(request_json) {
        Ok(r) => to_json(&forge_viewers::apply_edits(&r.text, r.delimiter, r.edits)),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SortRequest {
    rows: Vec<Vec<String>>,
    column: usize,
    descending: bool,
    header_rows: usize,
}

/// `{ rows, column, descending, headerRows }` → row indices.
#[wasm_bindgen]
pub fn csv_sort_order(request_json: &str) -> String {
    match parse::<SortRequest>(request_json) {
        Ok(r) => to_json(&forge_viewers::sort_order(
            &r.rows,
            r.column,
            r.descending,
            r.header_rows,
        )),
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_the_commands() {
        let svg =
            base64::engine::general_purpose::STANDARD.encode(r#"<svg width="8" height="4"/>"#);
        let out = image_describe(&format!(r#"{{"path":"a.svg","base64":"{svg}"}}"#));
        assert!(out.contains(r#""width":8"#), "{out}");
        assert!(image_describe(r#"{"path":"a.png","base64":"AAAA"}"#).contains("error"));
        assert!(csv_parse(r#"{"text":"a;b\n","path":"x.csv"}"#).contains(r#""delimiter":";""#));
        assert_eq!(
            csv_edit(r#"{"text":"a,b\n","delimiter":",","edits":[{"op":"insertRow","at":0}]}"#),
            r#"",\na,b\n""#
        );
        assert_eq!(
            csv_sort_order(
                r#"{"rows":[["h"],["b"],["a"]],"column":0,"descending":false,"headerRows":1}"#
            ),
            "[0,2,1]"
        );
    }
}
