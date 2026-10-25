//! Color schemes (`forge-colorscheme`), mirroring the `color_scheme_*` commands for the
//! browser-mode mock. The mock passes the current settings values with every call
//! (`settings`) and stores the settings writes that come back.

use forge_colorscheme::{export_icls, import_icls, SchemeChange, SchemeSettings, SettingWrite};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wasm_bindgen::prelude::*;

use crate::to_json;

#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    settings: Value,
    #[serde(default)]
    dark: bool,
    #[serde(default)]
    change: Option<SchemeChange>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

fn run<T: Serialize>(json: &str, f: impl FnOnce(SchemeSettings, Request) -> Result<T, String>) -> String {
    let r = serde_json::from_str::<Request>(json)
        .map_err(|e| format!("invalid color scheme request: {e}"))
        .and_then(|r| f(SchemeSettings::from_json(&r.settings), r));
    match r {
        Ok(v) => to_json(&v),
        Err(e) => to_json(&serde_json::json!({ "error": e })),
    }
}

/// `{settings, dark}` → `SchemesSnapshot`.
#[wasm_bindgen]
pub fn colorscheme_state(json: &str) -> String {
    run(json, |s, r| Ok(s.snapshot(r.dark)))
}

/// `{settings, dark, change}` → `SettingWrite[]`.
#[wasm_bindgen]
pub fn colorscheme_apply(json: &str) -> String {
    run(json, |s, r| {
        let change = r.change.ok_or("missing change")?;
        Ok(s.apply(change, r.dark))
    })
}

/// `{settings, text}` → `SettingWrite[]` that add the imported scheme and select it.
#[wasm_bindgen]
pub fn colorscheme_import(json: &str) -> String {
    run(json, |s, r| -> Result<Vec<SettingWrite>, String> {
        let imported = import_icls(&r.text.unwrap_or_default())?;
        Ok(s.add_imported(imported.scheme))
    })
}

/// `{settings, name}` → the scheme as .icls text.
#[wasm_bindgen]
pub fn colorscheme_export(json: &str) -> String {
    run(json, |s, r| Ok(export_icls(&s, &r.name.unwrap_or_default())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_the_commands() {
        let snap: Value = serde_json::from_str(&colorscheme_state(r#"{"dark":true}"#)).unwrap();
        assert_eq!(snap["active"], "Ember");
        assert_eq!(snap["followsTheme"], true);
        let writes: Value = serde_json::from_str(&colorscheme_apply(
            r#"{"dark":true,"change":{"op":"duplicate","name":"Mine"}}"#,
        ))
        .unwrap();
        assert_eq!(writes[1]["value"], "Mine");
        let imported: Value = serde_json::from_str(&colorscheme_import(
            r#"{"text":"<scheme name=\"X\" parent_scheme=\"Default\"/>"}"#,
        ))
        .unwrap();
        assert_eq!(imported[0]["value"][0]["parent"], "Anvil");
        let bad: Value = serde_json::from_str(&colorscheme_import(r#"{"text":"<x/>"}"#)).unwrap();
        assert!(bad["error"].is_string());
        let text: Value =
            serde_json::from_str(&colorscheme_export(r#"{"name":"Ember"}"#)).unwrap();
        assert!(text.as_str().unwrap().contains("parent_scheme=\"Darcula\""));
    }
}
