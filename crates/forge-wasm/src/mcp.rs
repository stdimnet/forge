//! The MCP server (`forge_mcp`) for the browser-mode mock: "Copy client configuration"
//! builds the same snippets as the desktop app's `mcp_client_configs`.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::parsers::parse;
use crate::to_json;

#[derive(Deserialize)]
struct ConfigsRequest {
    url: String,
    token: String,
    exe: String,
}

/// `{ url, token, exe }` → `ClientConfig[]`.
#[wasm_bindgen]
pub fn mcp_client_configs(request_json: &str) -> String {
    match parse::<ConfigsRequest>(request_json) {
        Ok(r) => to_json(&forge_mcp::configs(&r.url, &r.token, &r.exe)),
        Err(e) => e,
    }
}
