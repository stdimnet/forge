//! Third-party plugin models (`forge-plugins` without disk and network): manifest
//! validation, the permission prompt and the marketplace index, mirroring the
//! `plugin_inspect` and `marketplace_list` IPC commands in browser mode.

use forge_plugins::manifest::{Permissions, PluginManifest};
use forge_plugins::marketplace::{self, Index, InstalledVersion};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl std::fmt::Display) -> String {
    to_json(&serde_json::json!({ "error": e.to_string() }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarketplaceRequest {
    index: String,
    forge_version: String,
    #[serde(default)]
    installed: Vec<InstalledVersion>,
    #[serde(default)]
    query: String,
}

/// `{ index, forgeVersion, installed, query }` → `{ name, entries, warnings }`.
#[wasm_bindgen]
pub fn plugins_marketplace(request_json: &str) -> String {
    let req: MarketplaceRequest = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    match Index::parse(&req.index) {
        Ok((index, warnings)) => {
            let entries = marketplace::entries(&index, &req.forge_version, &req.installed);
            to_json(&serde_json::json!({
                "name": index.name,
                "entries": marketplace::search(&entries, &req.query),
                "warnings": warnings,
            }))
        }
        Err(e) => error(e),
    }
}

/// Permissions → the lines of the install prompt.
#[wasm_bindgen]
pub fn plugins_permission_lines(permissions_json: &str) -> String {
    match serde_json::from_str::<Permissions>(permissions_json) {
        Ok(p) => to_json(&forge_plugins::permissions::summary(&p)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ValidateRequest {
    manifest: String,
    forge_version: String,
}

/// `{ manifest, forgeVersion }` → the manifest's problems (`[]` when valid).
#[wasm_bindgen]
pub fn plugins_validate_manifest(request_json: &str) -> String {
    let req: ValidateRequest = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    match PluginManifest::parse(&req.manifest) {
        Ok(m) => to_json(&m.validate(&req.forge_version)),
        Err(e) => error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_json() {
        let index = r#"{"formatVersion":1,"name":"Demo","plugins":[{"id":"acme.tool","name":"Tool",
            "versions":[{"version":"1.0.0","engines":{"forge":">=0.1.0"},"url":"t.forgeplugin",
            "sha256":"0000000000000000000000000000000000000000000000000000000000000000",
            "permissions":{"network":["api.acme.dev"]}}]}]}"#;
        let req = serde_json::json!({ "index": index, "forgeVersion": "0.1.0", "query": "tool" });
        let out = plugins_marketplace(&req.to_string());
        assert!(out.contains(r#""id":"acme.tool""#) && out.contains("Connect to the internet"), "{out}");
        assert!(plugins_permission_lines(r#"{"editor":["read"]}"#).contains("editor.read"));
        let bad = serde_json::json!({ "manifest": "{}", "forgeVersion": "0.1.0" });
        assert!(plugins_validate_manifest(&bad.to_string()).contains("error"));
    }
}
