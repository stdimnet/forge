//! Language injections (`forge-injections`), mirroring the `injections_*` IPC commands. The
//! editor calls `injections_find` and `injections_fragment` synchronously (highlighting and
//! Edit Fragment follow every keystroke), in the desktop app and in browser mode alike.

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn run<T: DeserializeOwned, R: serde::Serialize>(json: &str, f: impl FnOnce(T) -> R) -> String {
    match serde_json::from_str::<T>(json) {
        Ok(v) => to_json(&f(v)),
        Err(e) => to_json(&json!({ "error": e.to_string() })),
    }
}

/// `{ text, path, disabled?, uninjected? }` → `Injection[]`.
#[wasm_bindgen]
pub fn injections_find(request_json: &str) -> String {
    run(request_json, |r| forge_injections::find(&r))
}

/// `{ text, path, offset, disabled?, uninjected? }` → `Injection | null`.
#[wasm_bindgen]
pub fn injections_at(request_json: &str) -> String {
    run(request_json, |r| forge_injections::injection_at(&r))
}

/// `{ text, path, offset, disabled?, uninjected? }` → `Fragment | null`.
#[wasm_bindgen]
pub fn injections_fragment(request_json: &str) -> String {
    run(request_json, |r| forge_injections::fragment(&r))
}

#[derive(Deserialize)]
struct EscapeRequest {
    kind: forge_injections::LiteralKind,
    text: String,
}

/// `{ kind, text }` → the text escaped for a literal of that kind.
#[wasm_bindgen]
pub fn injections_escape(request_json: &str) -> String {
    run(request_json, |r: EscapeRequest| {
        forge_injections::escape(r.kind, &r.text)
    })
}

/// `{ text, path, offset, language }` → `Edit | null`.
#[wasm_bindgen]
pub fn injections_inject(request_json: &str) -> String {
    run(request_json, |r| forge_injections::inject(&r))
}

/// `{ text, path, offset, disabled?, uninjected? }` → `ContextAction[]`.
#[wasm_bindgen]
pub fn injections_context_actions(request_json: &str) -> String {
    run(request_json, |r| forge_injections::context_actions(&r))
}

/// The built-in rules for Settings | Editor | Language Injections.
#[wasm_bindgen]
pub fn injections_rules() -> String {
    to_json(&forge_injections::rule_infos())
}

/// The injectable languages.
#[wasm_bindgen]
pub fn injections_languages() -> String {
    to_json(&forge_injections::languages())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_and_escapes_through_json() {
        let req =
            json!({ "text": "<?php $q = 'SELECT * FROM t';", "path": "a.php", "disabled": [] });
        let out = injections_find(&req.to_string());
        assert!(out.contains(r#""language":"sql""#), "{out}");
        let esc = injections_escape(r#"{"kind":"phpSingle","text":"it's"}"#);
        assert_eq!(esc, r#""it\\'s""#);
        assert!(injections_find("nope").contains("error"));
        assert!(injections_rules().contains("php-sql-keywords"));
        assert!(injections_languages().contains("RegExp"));
    }
}
