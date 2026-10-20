//! Run output (`forge-output`) for the browser-mode run backend and terminal: the mock feeds
//! process events into a [`RunOutput`] and emits the rendered events like the desktop app.

use std::cell::RefCell;
use std::collections::HashMap;

use forge_output::{file_links_utf16, RunInput, RunOutput, RunOutputOptions};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use super::parse;
use crate::to_json;

thread_local! {
    static RUNS: RefCell<(u32, HashMap<u32, RunOutput>)> = RefCell::new((0, HashMap::new()));
}

/// `RunOutputOptions` → handle (a number) for [`run_output_handle`].
#[wasm_bindgen]
pub fn run_output_new(options_json: &str) -> String {
    match parse::<RunOutputOptions>(options_json) {
        Ok(opts) => RUNS.with(|r| {
            let mut r = r.borrow_mut();
            r.0 += 1;
            let id = r.0;
            r.1.insert(id, RunOutput::new(&opts));
            to_json(&id)
        }),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct HandleRequest {
    handle: u32,
    input: RunInput,
}

/// `{ handle, input: RunInput }` → `OutputEvent[]`.
#[wasm_bindgen]
pub fn run_output_handle(request_json: &str) -> String {
    match parse::<HandleRequest>(request_json) {
        Ok(req) => RUNS.with(|r| match r.borrow_mut().1.get_mut(&req.handle) {
            Some(out) => to_json(&out.handle(&req.input)),
            None => to_json(&Vec::<()>::new()),
        }),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct DropRequest {
    handle: u32,
}

#[wasm_bindgen]
pub fn run_output_drop(request_json: &str) -> String {
    if let Ok(req) = parse::<DropRequest>(request_json) {
        RUNS.with(|r| r.borrow_mut().1.remove(&req.handle));
    }
    "null".into()
}

#[derive(Deserialize)]
struct LinksRequest {
    text: String,
    root: Option<String>,
}

/// Mirrors the `output_file_links` command: `{ text, root }` → `FileLink[]`.
#[wasm_bindgen]
pub fn output_file_links(request_json: &str) -> String {
    match parse::<LinksRequest>(request_json) {
        Ok(req) => to_json(&file_links_utf16(&req.text, req.root.as_deref())),
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_output_through_handles() {
        let h = run_output_new(r#"{"testRunner":false}"#);
        let events = run_output_handle(&format!(
            r#"{{"handle":{h},"input":{{"type":"output","stream":"stdout","text":"hi\n"}}}}"#
        ));
        assert!(events.contains(r#""type":"console""#), "{events}");
        run_output_drop(&format!(r#"{{"handle":{h}}}"#));
        assert_eq!(
            run_output_handle(&format!(
                r#"{{"handle":{h},"input":{{"type":"input","text":"x"}}}}"#
            )),
            "[]"
        );
        assert!(output_file_links(r#"{"text":"a/B.php:1","root":"/r"}"#).contains("a/B.php"));
    }
}
