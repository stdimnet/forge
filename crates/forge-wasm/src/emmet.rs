//! Emmet (`forge-emmet`) and color values (`forge-colors`), mirroring the `emmet_*` and
//! `colors_*` IPC commands. The editor calls `emmet_expand` and `colors_find` synchronously
//! (the expand key must decide at once; color swatches follow every edit).

use serde::de::DeserializeOwned;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn run<T: DeserializeOwned, R: serde::Serialize>(json: &str, f: impl FnOnce(T) -> R) -> String {
    match serde_json::from_str::<T>(json) {
        Ok(v) => to_json(&f(v)),
        Err(e) => to_json(&json!({ "error": e.to_string() })),
    }
}

/// `Result<T, String>` as `T` or `{ "error": … }`.
fn result<T: serde::Serialize>(r: Result<T, String>) -> serde_json::Value {
    match r {
        Ok(v) => json!(v),
        Err(e) => json!({ "error": e }),
    }
}

/// `ExpandRequest` → `Expansion | null`.
#[wasm_bindgen]
pub fn emmet_expand(request_json: &str) -> String {
    run(request_json, |r| forge_emmet::expand(&r))
}

/// `WrapRequest` → `Expansion` (or `{ error }`).
#[wasm_bindgen]
pub fn emmet_wrap(request_json: &str) -> String {
    run(request_json, |r| result(forge_emmet::wrap(&r)))
}

/// `EditPointRequest` → offset | null.
#[wasm_bindgen]
pub fn emmet_edit_point(request_json: &str) -> String {
    run(request_json, |r| forge_emmet::edit_point(&r))
}

/// `TagRequest` → `Edit[]` (or `{ error }`).
#[wasm_bindgen]
pub fn emmet_update_tag(request_json: &str) -> String {
    run(request_json, |r| result(forge_emmet::update_tag(&r)))
}

/// `TagRequest` → `Edit[]` (or `{ error }`).
#[wasm_bindgen]
pub fn emmet_remove_tag(request_json: &str) -> String {
    run(request_json, |r| result(forge_emmet::remove_tag(&r)))
}

/// `FindRequest` → `ColorMatch[]`.
#[wasm_bindgen]
pub fn colors_find(request_json: &str) -> String {
    run(request_json, |r| forge_colors::find(&r))
}

/// `FormatRequest` → string.
#[wasm_bindgen]
pub fn colors_format(request_json: &str) -> String {
    run(request_json, |r| forge_colors::format_color(&r))
}

/// `{ text }` → `{ color, hex } | null`.
#[wasm_bindgen]
pub fn colors_parse(request_json: &str) -> String {
    #[derive(serde::Deserialize)]
    struct Req {
        text: String,
    }
    run(request_json, |r: Req| forge_colors::parse_color(&r.text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_json() {
        let out = emmet_expand(r#"{"text":"<p>ul>li","offset":8,"languageId":"html"}"#);
        assert!(out.contains(r#""syntax":"html""#), "{out}");
        assert_eq!(emmet_expand(r#"{"text":"x","offset":1}"#), "null");
        assert!(emmet_wrap(
            r#"{"text":"a {}","from":0,"to":4,"abbreviation":"div","languageId":"css"}"#
        )
        .contains("error"));
        assert_eq!(
            emmet_edit_point(r#"{"text":"<p></p>","offset":0,"forward":true}"#),
            "3"
        );
        assert!(emmet_remove_tag(r#"{"text":"<b>x</b>","offset":3}"#).starts_with('['));
        assert!(emmet_update_tag("{}").contains("error"));
        let found = colors_find(r#"{"text":"a { color: red }","languageId":"css"}"#);
        assert!(found.contains(r##""hex":"#ff0000""##), "{found}");
        assert_eq!(
            colors_format(r##"{"color":{"r":0,"g":0,"b":255,"a":1},"original":"#000"}"##),
            r##""#00f""##
        );
        assert!(colors_parse(r#"{"text":"fff"}"#).contains("#ffffff"));
        assert_eq!(colors_parse(r#"{"text":"nope"}"#), "null");
    }
}
