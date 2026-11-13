//! Text files (`forge-text`), mirroring `fs_read_text`, `fs_write_text`,
//! `text_prepare_save`, `editor_file_format` and the commit message inspections. Bytes
//! travel as JSON number arrays.

use forge_text::commit::{inspect_message, MessageRules};
use forge_text::file::{read_text, write_text};
use forge_text::save::SaveRequest;
use forge_text::{Encoding, FormatRequest, LineSeparator};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl ToString) -> String {
    to_json(&serde_json::json!({ "error": e.to_string() }))
}

#[derive(Deserialize)]
struct ReadRequest {
    bytes: Vec<u8>,
    encoding: Option<Encoding>,
}

/// `{ bytes, encoding? }` → `TextFile`.
#[wasm_bindgen]
pub fn text_read(request_json: &str) -> String {
    match serde_json::from_str::<ReadRequest>(request_json) {
        Ok(r) => to_json(&read_text(&r.bytes, r.encoding)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct WriteRequest {
    text: String,
    eol: Option<LineSeparator>,
    encoding: Option<Encoding>,
    bom: Option<bool>,
}

/// `{ text, eol?, encoding?, bom? }` → `{ bytes }` or `{ error }`.
#[wasm_bindgen]
pub fn text_write(request_json: &str) -> String {
    let r = match serde_json::from_str::<WriteRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    match write_text(
        &r.text,
        r.eol.unwrap_or_default(),
        r.encoding.unwrap_or_default(),
        r.bom.unwrap_or(false),
    ) {
        Ok(bytes) => to_json(&serde_json::json!({ "bytes": bytes })),
        Err(e) => error(e),
    }
}

/// `SaveRequest` → `SaveResult`.
#[wasm_bindgen]
pub fn text_prepare_save(request_json: &str) -> String {
    match serde_json::from_str::<SaveRequest>(request_json) {
        Ok(r) => to_json(&forge_text::prepare_save(&r)),
        Err(e) => error(e),
    }
}

/// `FormatRequest` (settings and `.editorconfig` files included) → `FileFormat`.
#[wasm_bindgen]
pub fn text_file_format(request_json: &str) -> String {
    match serde_json::from_str::<FormatRequest>(request_json) {
        Ok(r) => to_json(&forge_text::resolve_format(&r)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct InspectRequest {
    message: String,
    #[serde(default)]
    rules: MessageRules,
}

/// `{ message, rules }` → `MessageProblem[]`.
#[wasm_bindgen]
pub fn text_inspect_commit_message(request_json: &str) -> String {
    match serde_json::from_str::<InspectRequest>(request_json) {
        Ok(r) => to_json(&inspect_message(&r.message, &r.rules)),
        Err(e) => error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_writes_and_formats() {
        let out = text_read(r#"{"bytes":[99,97,102,233,13,10]}"#);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["text"], "caf\u{e9}\n");
        assert_eq!(v["eol"], "\r\n");
        assert_eq!(v["encoding"], "windows-1252");
        let out = text_write(r#"{"text":"a\n","eol":"\r\n","encoding":"UTF-8","bom":false}"#);
        assert_eq!(out, r#"{"bytes":[97,13,10]}"#);
        assert!(text_write(r#"{"text":"中","encoding":"ISO-8859-1"}"#).contains("error"));
        let out = text_prepare_save(
            r#"{"text":"a  ","options":{"stripTrailingSpaces":"all","ensureNewlineAtEof":true}}"#,
        );
        assert!(out.contains(r#""text":"a\n""#), "{out}");
        let out = text_file_format(
            r#"{"path":"a.php","languageId":"php","settings":{"php.codeStyle.useTabs":true}}"#,
        );
        assert!(out.contains(r#""indentLabel":"Tab""#), "{out}");
        let out = text_inspect_commit_message(r#"{"message":"Subject\nbody"}"#);
        assert!(out.contains("missingBlankLine"), "{out}");
    }
}
