//! `config('app.name')` keys from `config/*.php` and `env('APP_NAME')` keys from `.env`
//! files.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::php;
use crate::project::{files_under, read, Location};

/// A dotted key with a short preview of its value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyEntry {
    pub key: String,
    pub value: String,
    pub location: Location,
}

pub fn list_config(root: &Path) -> Vec<KeyEntry> {
    let mut out = Vec::new();
    for path in files_under(root, "config", ".php") {
        let Some(prefix) = path
            .strip_prefix("config/")
            .and_then(|p| p.strip_suffix(".php"))
            .map(|p| p.replace('/', "."))
        else {
            continue;
        };
        let Some(src) = read(root, &path) else {
            continue;
        };
        let Some(tree) = php::parse(&src) else {
            continue;
        };
        out.push(KeyEntry {
            key: prefix.clone(),
            value: "array".into(),
            location: Location::file(&path),
        });
        if let Some(value) = php::returned_value(&tree, &src) {
            php::flatten(value.entries(), &prefix, &mut |key, entry| {
                out.push(KeyEntry {
                    key,
                    value: entry.value.preview(),
                    location: Location::at(&path, &src, entry.offset),
                });
            });
        }
    }
    out
}

/// Variables of `.env`, completed by `.env.example` (first definition wins).
pub fn list_env(root: &Path) -> Vec<KeyEntry> {
    let mut out: Vec<KeyEntry> = Vec::new();
    for file in [".env", ".env.example"] {
        let Some(src) = read(root, file) else {
            continue;
        };
        let mut offset = 0;
        for line in src.split_inclusive('\n') {
            let start = offset;
            offset += line.len();
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let t = t.strip_prefix("export ").unwrap_or(t);
            let Some((key, value)) = t.split_once('=') else {
                continue;
            };
            let key = key.trim();
            if key.is_empty() || out.iter().any(|e| e.key == key) {
                continue;
            }
            let indent = line.len() - line.trim_start().len();
            out.push(KeyEntry {
                key: key.to_string(),
                value: value.trim().trim_matches('"').to_string(),
                location: Location::at(file, &src, start + indent),
            });
        }
    }
    out
}
