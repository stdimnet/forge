//! Terminal sequences (`forge_output::osc`), called synchronously by the terminal in the
//! desktop app and in browser mode alike: xterm.js runs OSC handlers inside its parser, so the
//! answer is needed before the handler returns, which an IPC round trip cannot give.

use forge_output::{link_target, parse_osc, Progress};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::parsers::parse;
use crate::to_json;

#[derive(Deserialize)]
struct OscRequest {
    code: u32,
    data: String,
    #[serde(default)]
    previous: Option<Progress>,
}

/// `{ code, data, previous? }` → `OscAction`.
#[wasm_bindgen]
pub fn terminal_parse_osc(request_json: &str) -> String {
    match parse::<OscRequest>(request_json) {
        Ok(req) => to_json(&parse_osc(req.code, &req.data, req.previous.as_ref())),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct LinkRequest {
    uri: String,
    #[serde(default)]
    root: Option<String>,
}

/// `{ uri, root? }` → `LinkTarget | null`.
#[wasm_bindgen]
pub fn terminal_link_target(request_json: &str) -> String {
    match parse::<LinkRequest>(request_json) {
        Ok(req) => to_json(&link_target(&req.uri, req.root.as_deref())),
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sequences_and_links() {
        assert_eq!(
            terminal_parse_osc(
                r#"{"code":9,"data":"4;2","previous":{"state":"normal","percent":40}}"#
            ),
            r#"{"kind":"progress","progress":{"state":"error","percent":40}}"#
        );
        assert_eq!(
            terminal_link_target(r#"{"uri":"file:///r/a.php#L3","root":"/r"}"#),
            r#"{"kind":"file","path":"a.php","line":3}"#
        );
        assert_eq!(terminal_link_target(r#"{"uri":"javascript:x"}"#), "null");
        assert!(terminal_parse_osc("{}").contains("error"));
    }
}
