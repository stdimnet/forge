//! Deployment (`forge-deploy`) for browser mode: the mock keeps the servers and their files
//! in memory; validation, path mappings and the Sync with Deployed comparison are the same
//! Rust code as in the desktop app.

use forge_deploy::compare::Entry;
use forge_deploy::mapping::{ExcludeNames, Resolution};
use forge_deploy::{CompareBy, DeployServer};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl std::fmt::Display) -> String {
    to_json(&serde_json::json!({ "error": e.to_string() }))
}

/// `DeployServer` → the problems shown under the form.
#[wasm_bindgen]
pub fn deploy_validate(server_json: &str) -> String {
    match serde_json::from_str::<DeployServer>(server_json) {
        Ok(s) => to_json(&s.validate()),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveRequest {
    server: DeployServer,
    paths: Vec<String>,
    /// "Exclude items by name" of Deployment | Options.
    #[serde(default)]
    exclude_names: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Resolved {
    local: String,
    /// `mapped`, `notMapped` or `excluded`.
    status: &'static str,
    deployment: Option<String>,
    /// Where it is on the server, root path included.
    display: Option<String>,
    web_url: Option<String>,
}

/// `{ server, paths, excludeNames }` → where every project path is deployed.
#[wasm_bindgen]
pub fn deploy_resolve(request_json: &str) -> String {
    let r = match serde_json::from_str::<ResolveRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let names = ExcludeNames::parse(&r.exclude_names);
    let out: Vec<Resolved> = r
        .paths
        .iter()
        .map(|p| {
            let local = forge_deploy::mapping::normalize(p);
            let (status, deployment) = match r.server.resolve(p) {
                _ if names.matches_path(&local) => ("excluded", None),
                Resolution::Mapped(d) => ("mapped", Some(d)),
                Resolution::NotMapped => ("notMapped", None),
                Resolution::Excluded => ("excluded", None),
            };
            Resolved {
                local,
                status,
                display: deployment.as_deref().map(|d| r.server.display_path(d)),
                web_url: deployment.as_ref().and_then(|_| r.server.web_url_of(p)),
                deployment,
            }
        })
        .collect();
    to_json(&out)
}

#[derive(Deserialize)]
struct Item {
    path: String,
    dir: bool,
    #[serde(default)]
    text: String,
    #[serde(default)]
    modified: Option<i64>,
}

#[derive(Deserialize)]
struct CompareRequest {
    local: Vec<Item>,
    remote: Vec<Item>,
    #[serde(default)]
    by: CompareBy,
}

fn entries(items: Vec<Item>) -> Vec<Entry> {
    items
        .into_iter()
        .map(|i| {
            if i.dir {
                Entry::dir(i.path, i.modified)
            } else {
                Entry {
                    digest: Some(forge_diff::digest(i.text.as_bytes())),
                    ..Entry::file(i.path, i.text.len() as u64, i.modified)
                }
            }
        })
        .collect()
}

/// `{ local: {path, dir, text, modified}[], remote: …, by }` → `DirCompare`.
#[wasm_bindgen]
pub fn deploy_compare(request_json: &str) -> String {
    match serde_json::from_str::<CompareRequest>(request_json) {
        Ok(r) => to_json(&forge_deploy::compare::compare(
            &entries(r.local),
            &entries(r.remote),
            r.by,
        )),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct NodesRequest {
    server: DeployServer,
    /// `{ path, dir, size, modified }` relative to the root path.
    entries: Vec<Entry>,
}

/// Remote Host: `{ server, entries }` → the tree nodes (mappings, exclusions, order).
#[wasm_bindgen]
pub fn deploy_remote_nodes(request_json: &str) -> String {
    match serde_json::from_str::<NodesRequest>(request_json) {
        Ok(r) => to_json(&forge_deploy::browse::nodes(&r.server, &r.entries)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct NameRequest {
    /// The folder of a new entry, or the path of the renamed one (with `rename`).
    path: String,
    name: String,
    #[serde(default)]
    rename: bool,
}

/// Remote Host: `{ path, name, rename }` → `{ path }` of the new or renamed entry, or
/// `{ error }` for an invalid name.
#[wasm_bindgen]
pub fn deploy_entry_path(request_json: &str) -> String {
    let r = match serde_json::from_str::<NameRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let path = if r.rename {
        forge_deploy::browse::renamed_path(&r.path, &r.name)
    } else {
        forge_deploy::browse::child_path(&r.path, &r.name)
    };
    match path {
        Ok(path) => to_json(&serde_json::json!({ "path": path })),
        Err(e) => error(e),
    }
}
