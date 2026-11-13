//! Editor actions (`forge-editing`), mirroring `editor_postfix_templates`,
//! `editor_postfix_complete`, `editor_complete_statement` and `editor_expand_word`, and the
//! session clipboard history of Paste from History (`clipboard_history_*`).
//!
//! The edit history of Last/Next Edit Location (`edit_history`) and the order of Next/Previous
//! Change (`editor_change_target`) run here in the desktop app too: the editor reports every
//! document change synchronously, so places stay mapped without an IPC round trip.

use std::cell::RefCell;

use forge_editing::{
    ClipEntry, ClipboardHistory, EditHistory, EditOp, EditPlace, ExpandRequest, LineRange,
    PostfixRequest, StatementRequest,
};
use serde::{Deserialize, Serialize};
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

thread_local! {
    static EDITS: RefCell<EditHistory> = RefCell::new(EditHistory::default());
}

#[derive(Deserialize)]
struct EditHistoryRequest {
    ops: Vec<EditOp>,
    /// Also return the places (newest first).
    #[serde(default)]
    list: bool,
}

#[derive(Serialize)]
struct EditHistoryAnswer {
    /// The place the last Back/Forward goes to.
    place: Option<EditPlace>,
    #[serde(skip_serializing_if = "Option::is_none")]
    places: Option<Vec<EditPlace>>,
}

/// `{ ops: EditOp[], list? }` → `{ place: EditPlace | null, places?: EditPlace[] }`.
#[wasm_bindgen]
pub fn edit_history(request_json: &str) -> String {
    match parse::<EditHistoryRequest>(request_json) {
        Ok(r) => EDITS.with(|h| {
            let mut h = h.borrow_mut();
            let mut place = None;
            for op in r.ops {
                place = h.apply(op);
            }
            to_json(&EditHistoryAnswer {
                place,
                places: r.list.then(|| h.places()),
            })
        }),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct ChangeTargetRequest {
    ranges: Vec<LineRange>,
    line: usize,
    forward: bool,
}

/// `{ ranges, line, forward }` → the index of the hunk to show, or `null`.
#[wasm_bindgen]
pub fn editor_change_target(request_json: &str) -> String {
    match parse::<ChangeTargetRequest>(request_json) {
        Ok(r) => to_json(&forge_editing::change_target(&r.ranges, r.line, r.forward)),
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
        let out = edit_history(
            r#"{"ops":[{"op":"record","path":"a","line":3},{"op":"record","path":"a","line":30},{"op":"back","caret":{"path":"a","line":30}}],"list":true}"#,
        );
        assert!(out.starts_with(r#"{"place":{"path":"a","line":3"#), "{out}");
        assert!(out.contains(r#""places":[{"path":"a","line":30"#), "{out}");
        assert_eq!(
            editor_change_target(r#"{"ranges":[{"start":1,"end":2}],"line":5,"forward":true}"#),
            "0"
        );
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
