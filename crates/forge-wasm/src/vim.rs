//! Vim emulation bridge (`forge-vim`).
//!
//! The editor calls these synchronously for every key press (desktop app and browser mode
//! alike): a keydown handler must decide on `preventDefault` before it returns. One engine
//! holds the global Vim state (registers, macros, mappings, options) and a handle per
//! attached editor. The keymap resolver asks [`claims`] whether the focused editor's Vim
//! takes a key before the IDE keymap sees it.

use std::cell::RefCell;

use forge_vim::{DomKey, Handle, HandlerOverride, Sync, Vim};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wasm_bindgen::prelude::*;

use crate::to_json;

thread_local! {
    static VIM: RefCell<Vim> = RefCell::new({
        let mut v = Vim::new();
        v.enabled = false;
        v
    });
}

fn parse<T: DeserializeOwned>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| format!("invalid vim request: {e}"))
}

fn result_json<T: Serialize>(r: Result<T, String>) -> String {
    match r {
        Ok(v) => to_json(&v),
        Err(e) => to_json(&serde_json::json!({ "error": e })),
    }
}

/// Whether the focused editor's Vim takes this key (the IDE keymap then leaves it alone).
pub(crate) fn claims(e: &forge_keymap::KeyEvent) -> bool {
    let key = DomKey {
        key: e.key.clone(),
        code: e.code.clone(),
        ctrl_key: e.ctrl_key,
        alt_key: e.alt_key,
        shift_key: e.shift_key,
        meta_key: e.meta_key,
    };
    VIM.with_borrow(|v| v.claims(&key))
}

#[derive(Deserialize)]
struct AttachReq {
    text: String,
}

#[derive(Serialize)]
struct Attached {
    handle: Handle,
}

/// `{text}` → `{handle}`: start tracking an editor.
#[wasm_bindgen]
pub fn vim_attach(json: &str) -> String {
    result_json(parse::<AttachReq>(json).map(|r| Attached {
        handle: VIM.with_borrow_mut(|v| v.attach(&r.text)),
    }))
}

#[derive(Deserialize)]
struct HandleReq {
    handle: Option<Handle>,
}

/// `{handle}` → `true`: forget an editor.
#[wasm_bindgen]
pub fn vim_detach(json: &str) -> String {
    result_json(parse::<HandleReq>(json).map(|r| {
        if let Some(h) = r.handle {
            VIM.with_borrow_mut(|v| v.detach(h));
        }
        true
    }))
}

/// `{handle | null}` → `true`: the focused editor (null on blur).
#[wasm_bindgen]
pub fn vim_focus(json: &str) -> String {
    result_json(parse::<HandleReq>(json).map(|r| {
        VIM.with_borrow_mut(|v| v.focus(r.handle));
        true
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyReq {
    handle: Handle,
    #[serde(default)]
    event: Option<DomKey>,
    #[serde(default)]
    sync: Sync,
}

/// `{handle, event?, sync}` → `Response`. Without `event` it only syncs and continues
/// queued keys (after the effects of a `resume` answer were performed).
#[wasm_bindgen]
pub fn vim_key(json: &str) -> String {
    result_json(parse::<KeyReq>(json).map(|r| {
        VIM.with_borrow_mut(|v| match &r.event {
            Some(e) => v.key_event(r.handle, e, &r.sync),
            None => v.key(r.handle, None, &r.sync),
        })
    }))
}

/// `{handle, sync}` → `Response`: `timeoutlen` passed without a key.
#[wasm_bindgen]
pub fn vim_timeout(json: &str) -> String {
    result_json(
        parse::<KeyReq>(json).map(|r| VIM.with_borrow_mut(|v| v.timeout(r.handle, &r.sync))),
    )
}

/// `{handle}` → `Response`: the editor's mode and caret without a key.
#[wasm_bindgen]
pub fn vim_state(json: &str) -> String {
    result_json(parse::<KeyReq>(json).map(|r| VIM.with_borrow_mut(|v| v.state(r.handle))))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigureReq {
    enabled: bool,
    /// `vim.shortcutHandlers` setting.
    #[serde(default)]
    handlers: Value,
}

fn overrides(v: &Value) -> Vec<HandlerOverride> {
    serde_json::from_value(v.clone()).unwrap_or_default()
}

/// `{enabled, handlers}` → `true`: apply the settings.
#[wasm_bindgen]
pub fn vim_configure(json: &str) -> String {
    result_json(parse::<ConfigureReq>(json).map(|r| {
        VIM.with_borrow_mut(|v| {
            v.enabled = r.enabled;
            v.handlers.set_overrides(overrides(&r.handlers));
        });
        true
    }))
}

#[derive(Deserialize)]
struct RcReq {
    text: String,
}

/// `{text}` → `RcReport`: apply a `~/.ideavimrc`.
#[wasm_bindgen]
pub fn vim_load_rc(json: &str) -> String {
    result_json(parse::<RcReq>(json).map(|r| VIM.with_borrow_mut(|v| v.load_rc(&r.text))))
}

#[derive(Deserialize)]
struct HandlersReq {
    #[serde(default)]
    handlers: Value,
}

/// `{handlers}` → `HandlerRow[]`: the handler table with these settings (plus the rc file).
#[wasm_bindgen]
pub fn vim_handlers(json: &str) -> String {
    result_json(parse::<HandlersReq>(json).map(|r| {
        VIM.with_borrow(|v| {
            let mut h = v.handlers.clone();
            h.set_overrides(overrides(&r.handlers));
            h.table()
        })
    }))
}

#[derive(Deserialize)]
struct ClipboardReq {
    text: String,
}

/// `{text}` → `true`: the system clipboard changed (the `+` and `*` registers).
#[wasm_bindgen]
pub fn vim_set_clipboard(json: &str) -> String {
    result_json(parse::<ClipboardReq>(json).map(|r| {
        VIM.with_borrow_mut(|v| v.set_clipboard(&r.text));
        true
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle(json: &str) -> Handle {
        let v: Value = serde_json::from_str(json).unwrap();
        v["handle"].as_u64().unwrap() as Handle
    }

    #[test]
    fn handles_keys_through_json() {
        vim_configure(r#"{"enabled":true,"handlers":[]}"#);
        let h = handle(&vim_attach(r#"{"text":"foo bar"}"#));
        let key = |k: &str, code: &str| {
            let r: Value = serde_json::from_str(&vim_key(&format!(
                r#"{{"handle":{h},"event":{{"key":"{k}","code":"{code}"}},"sync":{{"selection":[{{"anchor":0,"head":0}}]}}}}"#
            )))
            .unwrap();
            r
        };
        key("d", "KeyD");
        let r = key("w", "KeyW");
        assert_eq!(
            r["changes"][0],
            serde_json::json!({"from":0,"to":4,"insert":""})
        );
        assert_eq!(r["mode"], "normal");
        let r = key("i", "KeyI");
        assert_eq!(r["mode"], "insert");
        let r = key("x", "KeyX");
        assert_eq!(r["consumed"], false);
        vim_focus(&format!(r#"{{"handle":{h}}}"#));
        let ev = forge_keymap::KeyEvent {
            key: "Escape".into(),
            code: "Escape".into(),
            ..Default::default()
        };
        assert!(claims(&ev));
        vim_focus(r#"{"handle":null}"#);
        assert!(!claims(&ev));
        let rows: Value = serde_json::from_str(&vim_handlers(
            r#"{"handlers":[{"shortcut":"Ctrl+R","normal":"ide","insert":"","visual":""}]}"#,
        ))
        .unwrap();
        let r = rows
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["shortcut"] == "Ctrl+R")
            .unwrap();
        assert_eq!(r["normal"], "ide");
        let report: Value =
            serde_json::from_str(&vim_load_rc(r#"{"text":"set hls\nfoo"}"#)).unwrap();
        assert_eq!(report["applied"], 1);
        assert_eq!(report["problems"][0]["line"], 2);
        vim_detach(&format!(r#"{{"handle":{h}}}"#));
    }
}
