//! GoForge (`goforge-lang-go`, `goforge-pprof`), mirroring the `go_*` and `pprof_open` IPC
//! commands for the browser-mode mock: `go.mod` / `go.work` analysis and completion
//! contexts, `go:generate` directives and commands, Go Build and module command lines,
//! go vet / golangci-lint reports and pprof views.

use base64::Engine;
use forge_runtools::Configuration;
use goforge_lang_go::GoContext;
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileRequest {
    path: String,
    content: String,
    #[serde(default)]
    offset: u32,
    #[serde(default)]
    line: Option<u32>,
}

/// `{ path, content }` → `{ kind: "mod" | "work", … }` (`go_mod_parse`).
#[wasm_bindgen]
pub fn go_mod_parse(request_json: &str) -> String {
    match parse::<FileRequest>(request_json) {
        Ok(r) if r.path.ends_with("go.work") => {
            let mut v = serde_json::to_value(goforge_lang_go::parse_go_work(&r.content))
                .unwrap_or_default();
            v["kind"] = json!("work");
            to_json(&v)
        }
        Ok(r) => {
            let mut v =
                serde_json::to_value(goforge_lang_go::parse_go_mod(&r.content)).unwrap_or_default();
            v["kind"] = json!("mod");
            to_json(&v)
        }
        Err(e) => error(e),
    }
}

/// `{ path, content, offset }` → completion context or null.
#[wasm_bindgen]
pub fn go_mod_completion_context(request_json: &str) -> String {
    match parse::<FileRequest>(request_json) {
        Ok(r) => to_json(&goforge_lang_go::gomod::completion_context(
            &r.content, r.offset,
        )),
        Err(e) => error(e),
    }
}

/// `{ content }` → `GenerateDirective[]`.
#[wasm_bindgen]
pub fn go_generate_directives(request_json: &str) -> String {
    match parse::<FileRequest>(request_json) {
        Ok(r) => to_json(&goforge_lang_go::generate_directives(&r.content)),
        Err(e) => error(e),
    }
}

/// `{ path, content, line? }` → `go generate` arguments.
#[wasm_bindgen]
pub fn go_generate_args(request_json: &str) -> String {
    match parse::<FileRequest>(request_json) {
        Ok(r) => match goforge_lang_go::generate_args(&format!("./{}", r.path), &r.content, r.line)
        {
            Ok(a) => to_json(&a),
            Err(e) => error(e),
        },
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildRequest {
    configuration: Configuration,
    #[serde(default)]
    context: GoContext,
}

/// `{ configuration, context }` → `GoCommand` of a Go Build configuration.
#[wasm_bindgen]
pub fn go_build_command(request_json: &str) -> String {
    match parse::<BuildRequest>(request_json) {
        Ok(r) => match goforge_lang_go::build_command(&r.configuration, &r.context) {
            Ok(c) => to_json(&c),
            Err(e) => error(e),
        },
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModuleRequest {
    action: String,
    #[serde(default)]
    arg: String,
    #[serde(default)]
    dir: String,
    #[serde(default)]
    context: GoContext,
}

/// `{ action, arg?, dir?, context }` → `GoCommand` (`go mod tidy`, `go get …`).
#[wasm_bindgen]
pub fn go_module_command(request_json: &str) -> String {
    match parse::<ModuleRequest>(request_json) {
        Ok(r) => match goforge_lang_go::module_command(&r.action, &r.arg, &r.dir, &r.context) {
            Ok(c) => to_json(&c),
            Err(e) => error(e),
        },
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LintRequest {
    tool: String,
    output: String,
    root: String,
}

/// `{ tool: "vet" | "golangci", output, root }` → `GoProblem[]`.
#[wasm_bindgen]
pub fn go_lint_parse(request_json: &str) -> String {
    match parse::<LintRequest>(request_json) {
        Ok(r) if r.tool == "vet" => to_json(&goforge_lang_go::parse_vet(&r.output, &r.root)),
        Ok(r) => match goforge_lang_go::parse_golangci(&r.output, &r.root, "") {
            Ok(p) => to_json(&p),
            Err(e) => error(e),
        },
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileRequest {
    /// Base64 of the profile; empty: the demo profile.
    #[serde(default)]
    base64: String,
    #[serde(default)]
    sample_index: Option<usize>,
}

/// `{ base64?, sampleIndex? }` → `ProfileView` (`pprof_open`).
#[wasm_bindgen]
pub fn pprof_view(request_json: &str) -> String {
    match parse::<ProfileRequest>(request_json) {
        Ok(r) => {
            let data = if r.base64.is_empty() {
                goforge_pprof::demo_profile()
            } else {
                match base64::engine::general_purpose::STANDARD.decode(r.base64.trim()) {
                    Ok(d) => d,
                    Err(e) => return error(e),
                }
            };
            match goforge_pprof::open(&data, r.sample_index) {
                Ok(v) => to_json(&v),
                Err(e) => error(e),
            }
        }
        Err(e) => error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_the_go_commands() {
        let m = go_mod_parse(r#"{"path":"go.mod","content":"module a.b/c\n\ngo 1.23\n"}"#);
        assert!(m.contains("\"kind\":\"mod\"") && m.contains("a.b/c"));
        let d =
            go_generate_directives(r#"{"path":"","content":"//go:generate stringer -type=K\n"}"#);
        assert!(d.contains("stringer"));
        let p = pprof_view("{}");
        assert!(p.contains("\"tree\""), "{p}");
        let b = go_build_command(
            r#"{"configuration":{"name":"a","type":"go-build","options":{"package":"./cmd/a"}}}"#,
        );
        assert!(b.contains("\"run\""), "{b}");
    }
}
