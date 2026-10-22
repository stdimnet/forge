//! Structural Search and Replace in browser mode: the template library and the variables of
//! a template (`forge-ssr` without its tree-sitter engine, which does not build to
//! WebAssembly; the browser mock answers searches from canned demo results).

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

/// `{}` → `SavedTemplate[]` (the built-in templates).
#[wasm_bindgen]
pub fn ssr_templates(_request_json: &str) -> String {
    to_json(&forge_ssr::library::builtin())
}

/// `{ template, replacement? }` → `TemplateCheck` without the syntax check.
#[wasm_bindgen]
pub fn ssr_variables(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        template: String,
        #[serde(default)]
        replacement: Option<String>,
    }
    match serde_json::from_str::<Req>(request_json) {
        Ok(r) => {
            let variables = forge_ssr::variables(&r.template);
            let unknown: Vec<String> = r
                .replacement
                .as_deref()
                .map(forge_ssr::variables)
                .unwrap_or_default()
                .into_iter()
                .filter(|v| !variables.contains(v))
                .collect();
            to_json(&forge_ssr::TemplateCheck {
                variables,
                unknown_in_replacement: unknown,
                error: None,
            })
        }
        Err(e) => to_json(&serde_json::json!({ "error": e.to_string() })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_json() {
        assert!(ssr_templates("{}").contains("method calls"));
        let out = ssr_variables(r#"{"template":"$a$->$m$()","replacement":"$a$->$x$()"}"#);
        assert_eq!(
            out,
            r#"{"variables":["a","m"],"unknownInReplacement":["x"]}"#
        );
    }
}
