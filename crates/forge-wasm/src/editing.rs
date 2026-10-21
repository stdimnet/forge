//! Editor actions (`forge-editing`), mirroring `editor_postfix_templates`,
//! `editor_postfix_complete`, `editor_complete_statement` and `editor_expand_word`, and the
//! session clipboard history of Paste from History (`clipboard_history_*`).

use std::cell::RefCell;

use forge_editing::{ClipEntry, ClipboardHistory, ExpandRequest, PostfixRequest, StatementRequest};
use serde::Deserialize;
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

thread_local! {
    static CLIPBOARD: RefCell<ClipboardHistory> = RefCell::new(ClipboardHistory::default());
}

/// Arguments of the `clipboard_history_*` commands.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ClipboardArgs {
    entry: Option<ClipEntry>,
    query: Option<String>,
    index: Option<usize>,
    max: Option<usize>,
}

fn with_clipboard<T: serde::Serialize>(
    args_json: &str,
    f: impl FnOnce(&mut ClipboardHistory, ClipboardArgs) -> T,
) -> String {
    match parse::<ClipboardArgs>(args_json) {
        Ok(args) => CLIPBOARD.with(|h| {
            let mut h = h.borrow_mut();
            if let Some(max) = args.max {
                h.set_max(max);
            }
            to_json(&f(&mut h, args))
        }),
        Err(e) => e,
    }
}

/// `{ entry, max? }` → `bool`.
#[wasm_bindgen]
pub fn clipboard_history_add(args_json: &str) -> String {
    with_clipboard(args_json, |h, a| a.entry.is_some_and(|e| h.add(e)))
}

/// `{ query?, max? }` → `ClipItem[]`.
#[wasm_bindgen]
pub fn clipboard_history_list(args_json: &str) -> String {
    with_clipboard(args_json, |h, a| h.items(a.query.as_deref().unwrap_or("")))
}

/// `{ index }` → `bool`.
#[wasm_bindgen]
pub fn clipboard_history_remove(args_json: &str) -> String {
    with_clipboard(args_json, |h, a| {
        a.index.and_then(|i| h.remove(i)).is_some()
    })
}

/// `{ index }` → `ClipEntry | null`.
#[wasm_bindgen]
pub fn clipboard_history_promote(args_json: &str) -> String {
    with_clipboard(args_json, |h, a| a.index.and_then(|i| h.promote(i)))
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

    #[test]
    fn clipboard_history_bridge() {
        assert_eq!(
            clipboard_history_add(r#"{"entry":{"text":"a"},"max":5}"#),
            "true"
        );
        assert_eq!(clipboard_history_add(r#"{"entry":{"text":"b"}}"#), "true");
        let list = clipboard_history_list(r#"{"query":"b"}"#);
        assert!(list.contains("\"preview\":\"b\""), "{list}");
        assert!(clipboard_history_promote(r#"{"index":1}"#).contains("\"text\":\"a\""));
        assert_eq!(clipboard_history_remove(r#"{"index":0}"#), "true");
        assert_eq!(clipboard_history_remove(r#"{"index":9}"#), "false");
        assert!(clipboard_history_list("{").contains("error"));
    }
}
