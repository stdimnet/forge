//! Localization bridge (`forge-i18n`), used by the UI in the desktop app and in browser mode
//! alike: `t()` in `app/ui/src/i18n` formats messages synchronously through these functions.
//! The process-wide locale set here also localizes what other bridges produce in browser
//! mode (the main menu, action titles, plugin manifests).

use forge_i18n::{Args, Localizer};
use forge_plugin_api::{PluginInfo, SettingSpec};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

/// Select the UI language and the time zone for dates (minutes east of UTC).
#[wasm_bindgen]
pub fn i18n_set_locale(locale: &str, tz_offset_minutes: i32) -> String {
    forge_i18n::set_timezone_offset(tz_offset_minutes);
    forge_i18n::set_locale(locale);
    forge_i18n::current().locale().to_string()
}

/// The text of `key` in the selected language (English fallback).
#[wasm_bindgen]
pub fn i18n_t(key: &str) -> String {
    forge_i18n::t(key)
}

/// The text of `key` formatted with `args_json` (a JSON object).
#[wasm_bindgen]
pub fn i18n_t_args(key: &str, args_json: &str) -> String {
    let args: Args = serde_json::from_str(args_json).unwrap_or_default();
    forge_i18n::t_args(key, &args)
}

/// Format a message source (not a key) in the selected language, e.g. a plugin's own
/// translation.
#[wasm_bindgen]
pub fn i18n_format(message: &str, args_json: &str) -> String {
    let args: Args = serde_json::from_str(args_json).unwrap_or_default();
    let l = forge_i18n::current();
    l.format_message(message, l.locale(), &args)
}

/// Whether `key` exists in the selected language or in English.
#[wasm_bindgen]
pub fn i18n_has(key: &str) -> bool {
    forge_i18n::current().lookup(key).is_some()
}

/// The languages with a bundled catalog (`LocaleInfo[]`).
#[wasm_bindgen]
pub fn i18n_locales() -> String {
    to_json(&forge_i18n::locales())
}

/// Add translations (`{ locale, messages }`), e.g. of a third-party plugin's UI bundle.
#[wasm_bindgen]
pub fn i18n_add_messages(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        locale: String,
        messages: std::collections::BTreeMap<String, String>,
    }
    match serde_json::from_str::<Req>(request_json) {
        Ok(r) => {
            forge_i18n::add_messages(&r.locale, r.messages);
            to_json(&true)
        }
        Err(e) => to_json(&serde_json::json!({ "error": e.to_string() })),
    }
}

/// `PluginInfo[]` in the selected language (the browser-mode `plugins_list`).
#[wasm_bindgen]
pub fn i18n_localize_plugins(plugins_json: &str) -> String {
    let plugins: Vec<PluginInfo> = match serde_json::from_str(plugins_json) {
        Ok(p) => p,
        Err(e) => return to_json(&serde_json::json!({ "error": e.to_string() })),
    };
    let l: std::sync::Arc<Localizer> = forge_i18n::current();
    let out: Vec<PluginInfo> = plugins
        .iter()
        .map(|p| forge_plugin_api::l10n::localize_plugin(p, &l))
        .collect();
    to_json(&out)
}

/// `{ specs, plugins }` → `SettingSpec[]` in the selected language (the browser-mode
/// `settings_schema`).
#[wasm_bindgen]
pub fn i18n_localize_settings(request_json: &str) -> String {
    #[derive(Deserialize)]
    struct Req {
        specs: Vec<SettingSpec>,
        #[serde(default)]
        plugins: Vec<PluginInfo>,
    }
    match serde_json::from_str::<Req>(request_json) {
        Ok(r) => to_json(&forge_plugin_api::l10n::localize_settings(
            r.specs,
            &r.plugins,
            &forge_i18n::current(),
        )),
        Err(e) => to_json(&serde_json::json!({ "error": e.to_string() })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The process-wide locale is shared with the other bridge tests (the main menu), which
    // run in parallel and expect English; German lookups are covered in forge-i18n.
    #[test]
    fn formats_through_the_bridge() {
        assert_eq!(i18n_set_locale("en", 0), "en");
        assert_eq!(i18n_t("common.cancel"), "Cancel");
        assert!(i18n_has("common.cancel"));
        assert_eq!(i18n_t_args("vcs.files", r#"{"count":3}"#), "3 files");
        assert_eq!(
            i18n_format("{n, plural, one {# Datei} other {# Dateien}}", r#"{"n":3}"#),
            "3 Dateien"
        );
        assert!(i18n_locales().contains("Deutsch"));
    }
}
