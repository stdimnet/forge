//! Analyze External Stack Trace (`forge_output::stacktrace`), mirroring the
//! `stacktrace_analyze` command over the browser-mode demo project's file list.

use std::collections::HashSet;

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

#[derive(Deserialize)]
struct Analyze {
    text: String,
    #[serde(default)]
    context: forge_output::stacktrace::TraceContext,
    /// Every project file (project-relative).
    files: Vec<String>,
}

/// `{ text, context, files }` → `TraceAnalysis`.
#[wasm_bindgen]
pub fn stacktrace_analyze(request_json: &str) -> String {
    match serde_json::from_str::<Analyze>(request_json) {
        Ok(r) => {
            let files: HashSet<&str> = r.files.iter().map(String::as_str).collect();
            to_json(&forge_output::stacktrace::analyze(
                &r.text,
                &r.context,
                &|p: &str| files.contains(p),
            ))
        }
        Err(e) => to_json(&serde_json::json!({ "error": e.to_string() })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_json() {
        let out = stacktrace_analyze(
            r##"{"text":"#0 /var/www/html/app/A.php(3): x()","context":{"mappings":[{"local":"","remote":"/var/www/html"}]},"files":["app/A.php"]}"##,
        );
        assert!(out.contains(r#""target":"app/A.php""#), "{out}");
        assert!(out.contains(r#""kind":"php""#), "{out}");
    }
}
