//! Profiler snapshots (`forge-profiler`), mirroring `profiler_open` for the browser-mode
//! mock: the demo projects' Xdebug, SPX and V8 snapshots, and snapshot files read in the
//! browser (Base64).

use base64::Engine;
use forge_profiler::{OpenOptions, SnapshotKind};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DemoRequest {
    kind: SnapshotKind,
    #[serde(default)]
    root: String,
    #[serde(default)]
    metric: Option<usize>,
}

/// `{ kind, root, metric? }` → `Snapshot` of the demo project at `root`.
#[wasm_bindgen]
pub fn profiler_demo(request_json: &str) -> String {
    match serde_json::from_str::<DemoRequest>(request_json) {
        Ok(r) => to_json(&forge_profiler::demo::snapshot(r.kind, &r.root, r.metric)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenRequest {
    name: String,
    base64: String,
    #[serde(default)]
    companion_base64: Option<String>,
    #[serde(default)]
    metric: Option<usize>,
}

/// `{ name, base64, companionBase64?, metric? }` → `Snapshot` (`profiler_open`).
#[wasm_bindgen]
pub fn profiler_parse(request_json: &str) -> String {
    let r = match serde_json::from_str::<OpenRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let decode = |s: &str| base64::engine::general_purpose::STANDARD.decode(s.trim());
    let data = match decode(&r.base64) {
        Ok(d) => d,
        Err(e) => return error(e),
    };
    let companion = r.companion_base64.as_deref().and_then(|c| decode(c).ok());
    match forge_profiler::open(
        &r.name,
        &data,
        OpenOptions {
            metric: r.metric,
            companion: companion.as_deref(),
            maps: None,
        },
    ) {
        Ok(s) => to_json(&s),
        Err(e) => error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_profiler_open() {
        let x = profiler_demo(r#"{"kind":"xdebug","root":"~/Projects/acme-shop"}"#);
        assert!(x.contains("\"functions\"") && x.contains("PriceCalculator"), "{x}");
        let heap = profiler_demo(r#"{"kind":"v8-heap"}"#);
        assert!(heap.contains("\"classes\""));
        let text = forge_profiler::demo::cachegrind_text("/p");
        let b64 = base64::engine::general_purpose::STANDARD.encode(text);
        let s = profiler_parse(&format!(r#"{{"name":"cachegrind.out.1","base64":"{b64}","metric":1}}"#));
        assert!(s.contains("\"metric\":1"), "{s}");
        assert!(profiler_parse(r#"{"name":"x.txt","base64":"aGVsbG8="}"#).contains("error"));
    }
}
