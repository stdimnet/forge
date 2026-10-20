//! Keymap bridge (`forge-keymap`).
//!
//! Two groups of functions:
//!
//! - **Backend mirror** for the browser-mode mock of the `keymap_*` IPC commands. The mock
//!   passes the current keymap settings with every call (`settings`, `hostPlatform`) because
//!   the settings live in the mock's settings store, not here; changes come back as settings
//!   writes for the mock to store.
//! - **Sync resolver** used by the UI in both the desktop app and browser mode: the UI loads
//!   the bindings of the current keymap snapshot once and then resolves every key event
//!   synchronously, so it can still call `preventDefault` on the event.

use std::cell::RefCell;

use forge_keymap::engine::Resolver;
use forge_keymap::shortcut::{format_shortcut_text, modifier_of_event, stroke_from_event};
use forge_keymap::{
    legacy_changes, ActionRegistration, CommandInfo, KeyEvent, KeyInput, KeymapChange,
    KeymapService, KeymapSettings, KeymapSnapshot, Platform,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wasm_bindgen::prelude::*;

use crate::to_json;

thread_local! {
    static SERVICE: RefCell<KeymapService> = RefCell::new(KeymapService::new(Platform::Other));
    static SYNC: RefCell<Resolver> = RefCell::new(Resolver::default());
}

/// Settings every backend-mirror call carries.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct Context {
    /// Settings values by key (`keymap.active`, `keymap.overrides.*`).
    settings: Value,
    host_platform: Platform,
}

fn parse<T: DeserializeOwned>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| format!("invalid keymap request: {e}"))
}

fn result_json<T: Serialize>(r: Result<T, String>) -> String {
    match r {
        Ok(v) => to_json(&v),
        Err(e) => to_json(&serde_json::json!({ "error": e })),
    }
}

/// Run `f` on the service after adopting the settings of the request.
fn with_service<R, T: Serialize>(json: &str, f: impl FnOnce(&mut KeymapService, R) -> T) -> String
where
    R: DeserializeOwned,
{
    let r = parse::<Value>(json).and_then(|v| {
        let ctx: Context = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
        let req: R =
            serde_json::from_value(v).map_err(|e| format!("invalid keymap request: {e}"))?;
        Ok(SERVICE.with_borrow_mut(|s| {
            s.set_host(ctx.host_platform);
            s.update_settings(KeymapSettings::from_json(&ctx.settings));
            f(s, req)
        }))
    });
    result_json(r)
}

#[derive(Deserialize)]
struct Empty {}

/// `{settings, hostPlatform}` → `KeymapSnapshot`.
#[wasm_bindgen]
pub fn keymap_actions(json: &str) -> String {
    with_service(json, |s, _: Empty| s.snapshot())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Register {
    #[serde(default)]
    actions: Vec<ActionRegistration>,
    #[serde(default)]
    commands: Option<Vec<CommandInfo>>,
}

/// `{settings, hostPlatform, actions?, commands?}` → `KeymapSnapshot`.
#[wasm_bindgen]
pub fn keymap_register_actions(json: &str) -> String {
    with_service(json, |s, r: Register| {
        if !r.actions.is_empty() {
            s.register_actions(r.actions);
        }
        if let Some(commands) = r.commands {
            s.set_commands(commands);
        }
        s.snapshot()
    })
}

#[derive(Deserialize)]
struct Change {
    change: KeymapChange,
}

/// `{settings, hostPlatform, change}` → `SettingWrite[]`.
#[wasm_bindgen]
pub fn keymap_set_override(json: &str) -> String {
    with_service(json, |s, c: Change| s.apply(c.change))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConflictsReq {
    shortcut: String,
    #[serde(default)]
    action_id: Option<String>,
}

/// `{settings, hostPlatform, shortcut, actionId?}` → `ShortcutCheck`.
#[wasm_bindgen]
pub fn keymap_conflicts(json: &str) -> String {
    with_service(json, |s, r: ConflictsReq| {
        s.conflicts(&r.shortcut, r.action_id.as_deref())
    })
}

#[derive(Deserialize)]
struct FindReq {
    shortcut: String,
}

/// `{settings, hostPlatform, shortcut}` → action ids.
#[wasm_bindgen]
pub fn keymap_find_by_shortcut(json: &str) -> String {
    with_service(json, |s, r: FindReq| s.find_by_shortcut(&r.shortcut))
}

#[derive(Deserialize)]
struct ResolveReq {
    event: KeyInput,
}

/// `{settings, hostPlatform, event}` → `Resolution` (resolver state kept between calls).
#[wasm_bindgen]
pub fn keymap_resolve(json: &str) -> String {
    with_service(json, |s, r: ResolveReq| s.resolve(&r.event))
}

#[derive(Deserialize)]
struct LegacyReq {
    #[serde(default)]
    active: Option<String>,
    #[serde(default)]
    overrides: Option<String>,
}

/// `{active, overrides}` (raw localStorage strings) → `SettingWrite[]`.
#[wasm_bindgen]
pub fn keymap_legacy_changes(json: &str) -> String {
    result_json(
        parse::<LegacyReq>(json)
            .map(|r| legacy_changes(r.active.as_deref(), r.overrides.as_deref())),
    )
}

/// Load the bindings of a `KeymapSnapshot` into the sync resolver; returns their count.
#[wasm_bindgen]
pub fn keymap_load_bindings(snapshot_json: &str) -> String {
    result_json(parse::<KeymapSnapshot>(snapshot_json).map(|snap| {
        let bindings = snap.bindings();
        let count = bindings.len();
        SYNC.with_borrow_mut(|r| r.set_bindings(bindings));
        count
    }))
}

/// Resolve one `KeyInput` with the sync resolver → `Resolution`.
#[wasm_bindgen]
pub fn keymap_handle_key(input_json: &str) -> String {
    result_json(
        parse::<KeyInput>(input_json).map(|input| SYNC.with_borrow_mut(|r| r.handle(&input))),
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StrokeOfEvent {
    /// Canonical stroke id ("Ctrl+Shift+A"), `None` for a bare modifier press.
    stroke: Option<String>,
}

/// DOM key event → canonical keystroke (for "press a shortcut" fields).
#[wasm_bindgen]
pub fn keymap_stroke_of_event(event_json: &str) -> String {
    result_json(parse::<KeyEvent>(event_json).map(|e| {
        StrokeOfEvent {
            stroke: modifier_of_event(&e)
                .is_none()
                .then(|| stroke_from_event(&e).id()),
        }
    }))
}

#[derive(Deserialize)]
struct FormatReq {
    text: String,
    platform: Platform,
}

/// `{text, platform}` → display string ("Ctrl+K, Ctrl+C" / "⇧⌘A").
#[wasm_bindgen]
pub fn keymap_format(json: &str) -> String {
    result_json(parse::<FormatReq>(json).map(|r| format_shortcut_text(&r.text, r.platform)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: &str = r#""settings":{"keymap.active":"$default"},"hostPlatform":"other""#;

    #[test]
    fn serves_snapshots_and_changes() {
        let snap: Value = serde_json::from_str(&keymap_actions(&format!("{{{DEFAULT}}}"))).unwrap();
        assert_eq!(snap["keymap"], "$default");
        let writes: Value = serde_json::from_str(&keymap_set_override(&format!(
            r#"{{{DEFAULT},"change":{{"op":"remove","actionId":"GotoClass","shortcut":"Ctrl+N"}}}}"#
        )))
        .unwrap();
        assert_eq!(
            writes,
            serde_json::json!([{ "key": "keymap.overrides.default", "value": ["GotoClass"] }])
        );
        let found: Value = serde_json::from_str(&keymap_find_by_shortcut(&format!(
            r#"{{{DEFAULT},"shortcut":"ctrl+shift+a"}}"#
        )))
        .unwrap();
        assert!(found
            .as_array()
            .unwrap()
            .contains(&Value::from("GotoAction")));
    }

    #[test]
    fn resolves_synchronously_from_a_snapshot() {
        let snap = keymap_actions(&format!("{{{DEFAULT}}}"));
        let count: usize = serde_json::from_str(&keymap_load_bindings(&snap)).unwrap();
        assert!(count > 100);
        let res = keymap_handle_key(
            r#"{"type":"keydown","key":"n","code":"KeyN","ctrlKey":true,"focus":"global","time":1}"#,
        );
        assert_eq!(res, r#"{"consumed":true,"action":"GotoClass"}"#);
    }

    #[test]
    fn reports_errors_as_json() {
        assert!(keymap_handle_key("nope").starts_with(r#"{"error":"#));
        assert_eq!(
            keymap_stroke_of_event(r#"{"key":"A","code":"KeyA","ctrlKey":true}"#),
            r#"{"stroke":"Ctrl+A"}"#
        );
        assert_eq!(
            keymap_format(r#"{"text":"Cmd+Shift+A","platform":"mac"}"#),
            r#""⇧⌘A""#
        );
    }
}
