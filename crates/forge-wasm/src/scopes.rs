//! Scopes and file colors (`forge-scopes`), called synchronously by the UI in the desktop
//! app and in browser mode: the scope choosers, pattern checks, the Include / Exclude
//! buttons and preview tree of the Scopes page, and the file colors of tabs and the
//! Project view.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

fn error(e: String) -> String {
    to_json(&serde_json::json!({ "error": e }))
}

/// `ScopeSet` → `{ entries: ScopeEntry[], problems: [name, message][] }`.
#[wasm_bindgen]
pub fn scopes_entries(request_json: &str) -> String {
    match parse::<forge_scopes::ScopeSet>(request_json) {
        Ok(set) => to_json(&serde_json::json!({
            "entries": set.entries(),
            "problems": set.problems(),
        })),
        Err(e) => error(e),
    }
}

/// `{ pattern }` → `ParseError | null`.
#[wasm_bindgen]
pub fn scopes_check(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        pattern: String,
    }
    match parse::<Req>(request_json) {
        Ok(r) => to_json(&forge_scopes::parse(&r.pattern).err()),
        Err(e) => error(e),
    }
}

/// `EditRequest` → the new pattern (Include / Exclude buttons).
#[wasm_bindgen]
pub fn scopes_edit(request_json: &str) -> String {
    match parse::<forge_scopes::edit::EditRequest>(request_json) {
        Ok(r) => to_json(&forge_scopes::edit::apply(&r)),
        Err(e) => error(e),
    }
}

/// `{ files, filter }` → `ScopeTree`.
#[wasm_bindgen]
pub fn scopes_tree(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        files: Vec<String>,
        filter: forge_scopes::ScopeFilter,
    }
    match parse::<Req>(request_json) {
        Ok(r) => to_json(&forge_scopes::tree::build(&r.files, &r.filter)),
        Err(e) => error(e),
    }
}

/// `{ filter, paths }` → `boolean[]`: which files a scope contains.
#[wasm_bindgen]
pub fn scopes_contains(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        filter: forge_scopes::ScopeFilter,
        paths: Vec<String>,
    }
    match parse::<Req>(request_json) {
        Ok(r) => match forge_scopes::Scope::new(&r.filter) {
            Ok(scope) => to_json(
                &r.paths
                    .iter()
                    .map(|p| scope.contains(p))
                    .collect::<Vec<_>>(),
            ),
            Err(e) => error(e.to_string()),
        },
        Err(e) => error(e),
    }
}

/// `{ composerJson }` → `string[]`: the test folders of a `composer.json` (browser mode's
/// `scopes_test_roots`).
#[wasm_bindgen]
pub fn scopes_composer_test_roots(request_json: &str) -> String {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Req {
        composer_json: String,
    }
    match parse::<Req>(request_json) {
        Ok(r) => to_json(&forge_scopes::composer_test_roots(&r.composer_json)),
        Err(e) => error(e),
    }
}

/// `ColorsRequest` → `(string | null)[]`: the palette color of every target.
#[wasm_bindgen]
pub fn file_colors_resolve(request_json: &str) -> String {
    match parse::<forge_scopes::colors::ColorsRequest>(request_json) {
        Ok(r) => to_json(&forge_scopes::colors::resolve(&r)),
        Err(e) => error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_json() {
        let out = scopes_check(r#"{"pattern":"file:a||"}"#);
        assert!(out.contains("Pattern expected"), "{out}");
        assert_eq!(scopes_check(r#"{"pattern":"file:a"}"#), "null");
        let out =
            scopes_edit(r#"{"pattern":"","op":"includeRecursively","path":"app","dir":true}"#);
        assert_eq!(out, r#""file:app//*""#);
        let out = scopes_tree(
            r#"{"files":["app/A.php","b.txt"],"filter":{"scope":{"pattern":"file:*.php"}}}"#,
        );
        assert!(out.contains(r#""matched":1"#), "{out}");
        let out = file_colors_resolve(
            r#"{"local":[{"scope":"Tests","color":"Green"}],"targets":[{"path":"tests/A.php"}]}"#,
        );
        assert_eq!(out, r#"["Green"]"#);
        let out = scopes_entries(r#"{"shared":[{"name":"Models","pattern":"file:x"}]}"#);
        assert!(out.contains(r#""storage":"shared""#), "{out}");
        let out = scopes_contains(
            r#"{"filter":{"scope":{"predefined":"projectTestFiles"}},"paths":["tests/A.php","a.php"]}"#,
        );
        assert_eq!(out, "[true,false]");
    }
}
