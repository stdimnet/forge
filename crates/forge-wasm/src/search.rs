//! Fuzzy matching and Search Everywhere ranking (`forge-fuzzy`), mirroring the
//! `fuzzy_filter`, `search_rank` and `fs_find_files` IPC commands.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

/// `FilterRequest` → `FilterHit[]`.
#[wasm_bindgen]
pub fn fuzzy_filter(request_json: &str) -> String {
    match parse::<forge_fuzzy::FilterRequest>(request_json) {
        Ok(req) => to_json(&forge_fuzzy::filter(&req)),
        Err(e) => to_json(&serde_json::json!({ "error": e })),
    }
}

/// `RankRequest` → `RankResponse`.
#[wasm_bindgen]
pub fn search_rank(request_json: &str) -> String {
    match parse::<forge_fuzzy::everywhere::RankRequest>(request_json) {
        Ok(req) => to_json(&forge_fuzzy::everywhere::rank(&req)),
        Err(e) => to_json(&serde_json::json!({ "error": e })),
    }
}

#[derive(Deserialize)]
struct MatchFiles {
    query: String,
    paths: Vec<String>,
    limit: usize,
}

/// `{ query, paths, limit }` → `FileMatch[]`: "Go to File" over a list of paths (the
/// browser-mode demo project instead of a file walk).
#[wasm_bindgen]
pub fn search_match_files(request_json: &str) -> String {
    match parse::<MatchFiles>(request_json) {
        Ok(r) => to_json(&forge_fuzzy::rank_files(
            &r.query,
            r.paths.iter().map(String::as_str),
            r.limit,
        )),
        Err(e) => to_json(&serde_json::json!({ "error": e })),
    }
}

#[derive(Deserialize)]
struct FindFiles {
    request: forge_find::FindRequest,
    /// `[path, text]` of every project file (the browser-mode demo project).
    files: Vec<(String, String)>,
}

/// `{ request, files }` → `FindResult`: Find in Files over in-memory files, mirroring the
/// `find_in_files` command (the desktop app walks the project instead).
#[wasm_bindgen]
pub fn find_in_files(request_json: &str) -> String {
    match parse::<FindFiles>(request_json) {
        Ok(r) => to_json(&forge_find::find_in(
            &r.request,
            r.files.iter().map(|(p, t)| (p.as_str(), t.as_str())),
        )),
        Err(e) => to_json(&serde_json::json!({ "error": e })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_json() {
        let out = fuzzy_filter(r#"{"query":"OrCo","candidates":["OrderController","Order"]}"#);
        assert!(out.contains(r#""positions":[0,1,5,6]"#), "{out}");
        let out = search_match_files(r#"{"query":"user","paths":["app/User.php"],"limit":5}"#);
        assert!(out.contains(r#""name":"User.php""#), "{out}");
        let out = search_rank(r#"{"tab":"all","query":"","perGroup":5,"sources":[]}"#);
        assert_eq!(out, r#"{"groups":[]}"#);
        assert!(search_rank("nope").contains("error"));
        let out = find_in_files(r#"{"request":{"query":"user"},"files":[["a.php","new User"]]}"#);
        assert!(out.contains(r#""ranges":[[4,8]]"#), "{out}");
    }
}
