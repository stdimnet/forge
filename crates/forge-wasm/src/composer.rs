//! Composer (`phpforge-composer`) for the browser-mode mocks of the `composer_*` IPC
//! commands. Browser mode has no network: the mocks hand in the demo project's files and a
//! small demo catalog standing in for Packagist; everything else is computed here exactly
//! like in the desktop app.

use std::collections::HashMap;

use phpforge_composer::complete::{self, CompletionData, Request};
use phpforge_composer::lock;
use phpforge_composer::manifest;
use phpforge_composer::outdated;
use phpforge_composer::packagist::{PackageInfo, SearchHit};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

fn error(e: String) -> String {
    to_json(&json!({ "error": e }))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Req {
    text: String,
    offset: usize,
    lock: Option<String>,
    installed_json: Option<String>,
    /// Demo stand-in for Packagist.
    catalog: Vec<PackageInfo>,
}

fn installed(r: &Req) -> Vec<lock::InstalledPackage> {
    lock::installed(r.lock.as_deref(), r.installed_json.as_deref())
}

fn latest(catalog: &[PackageInfo]) -> HashMap<String, String> {
    catalog
        .iter()
        .filter_map(|p| Some((p.name.clone(), p.latest()?.original().to_string())))
        .collect()
}

/// `{ text, lock?, installedJson? }` → `ComposerProject` without the path fields.
#[wasm_bindgen]
pub fn composer_project(request_json: &str) -> String {
    match parse::<Req>(request_json) {
        Ok(r) => to_json(&json!({
            "requirements": manifest::requirements(&r.text),
            "installed": installed(&r),
            "hasLock": r.lock.is_some(),
        })),
        Err(e) => error(e),
    }
}

/// `{ text, lock, catalog }` → `OutdatedMarker[]`.
#[wasm_bindgen]
pub fn composer_markers(request_json: &str) -> String {
    match parse::<Req>(request_json) {
        Ok(r) => to_json(&outdated::markers(
            &r.text,
            &installed(&r),
            &latest(&r.catalog),
        )),
        Err(e) => error(e),
    }
}

fn search(catalog: &[PackageInfo], query: &str) -> Vec<SearchHit> {
    let q = query.trim().to_ascii_lowercase();
    catalog
        .iter()
        .filter(|p| {
            !q.is_empty()
                && (p.name.contains(&q) || p.description.to_ascii_lowercase().contains(&q))
        })
        .map(|p| SearchHit {
            name: p.name.clone(),
            description: p.description.clone(),
            downloads: 0,
            favers: 0,
            repository: p.repository.clone(),
            abandoned: p.abandoned.is_some(),
        })
        .collect()
}

/// `{ query, catalog }` → `SearchHit[]` (the demo search).
#[wasm_bindgen]
pub fn composer_search(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct SearchReq {
        query: String,
        catalog: Vec<PackageInfo>,
    }
    match parse::<SearchReq>(request_json) {
        Ok(r) => to_json(&search(&r.catalog, &r.query)),
        Err(e) => error(e),
    }
}

/// `{ text, offset, lock, catalog }` → `CompletionOut | null`.
#[wasm_bindgen]
pub fn composer_complete(request_json: &str) -> String {
    match parse::<Req>(request_json) {
        Ok(r) => {
            let mut data = CompletionData {
                installed: installed(&r),
                ..CompletionData::default()
            };
            match complete::request(&r.text, r.offset) {
                Some(Request::Packages { prefix }) => data.search = search(&r.catalog, &prefix),
                Some(Request::Versions { package, .. }) => {
                    data.package = r.catalog.iter().find(|p| p.name == package).cloned()
                }
                None => return to_json(&serde_json::Value::Null),
            }
            to_json(&complete::items(&r.text, r.offset, &data))
        }
        Err(e) => error(e),
    }
}

/// `{ text, offset, lock, catalog }` → `HoverOut | null`.
#[wasm_bindgen]
pub fn composer_hover(request_json: &str) -> String {
    match parse::<Req>(request_json) {
        Ok(r) => {
            let name = complete::hover_target(&r.text, r.offset).map(|t| t.0);
            let info = name.and_then(|n| r.catalog.iter().find(|p| p.name == n));
            to_json(&complete::hover(&r.text, r.offset, &installed(&r), info))
        }
        Err(e) => error(e),
    }
}
