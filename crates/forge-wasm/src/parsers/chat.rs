//! The AI chat conversation (`forge_agent_cli::conversation`) for the browser-mode agent: the
//! mock feeds its scripted events into a [`Chat`] and emits the `chat` updates like the
//! desktop backend.

use std::cell::RefCell;
use std::collections::HashMap;

use forge_agent_cli::{AgentEvent, Chat, MessageContext, PermissionStatus};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use super::parse;
use crate::to_json;

thread_local! {
    static CHATS: RefCell<(u32, HashMap<u32, Chat>)> = RefCell::new((0, HashMap::new()));
}

fn with_chat(handle: u32, f: impl FnOnce(&mut Chat) -> String) -> String {
    CHATS.with(|c| match c.borrow_mut().1.get_mut(&handle) {
        Some(chat) => f(chat),
        None => to_json(&serde_json::json!({ "error": format!("no chat {handle}") })),
    })
}

/// A new conversation → its handle (a number).
#[wasm_bindgen]
pub fn chat_new() -> String {
    CHATS.with(|c| {
        let mut c = c.borrow_mut();
        c.0 += 1;
        let id = c.0;
        c.1.insert(id, Chat::new());
        to_json(&id)
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BeginRequest {
    handle: u32,
    turn_id: u64,
    prompt: String,
    #[serde(default)]
    context: MessageContext,
}

/// `{ handle, turnId, prompt, context }` → `ChatUpdate`.
#[wasm_bindgen]
pub fn chat_begin_turn(request_json: &str) -> String {
    match parse::<BeginRequest>(request_json) {
        Ok(r) => with_chat(r.handle, |c| {
            to_json(&c.begin_turn(r.turn_id, r.prompt.trim(), r.context))
        }),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct ApplyRequest {
    handle: u32,
    event: AgentEvent,
    now: f64,
}

/// `{ handle, event: AgentEvent, now }` → `ChatUpdate`.
#[wasm_bindgen]
pub fn chat_apply(request_json: &str) -> String {
    match parse::<ApplyRequest>(request_json) {
        Ok(r) => with_chat(r.handle, |c| to_json(&c.apply(&r.event, r.now as u64))),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PermissionRequest {
    handle: u32,
    request_id: String,
    status: PermissionStatus,
}

/// `{ handle, requestId, status }` → `ChatUpdate | null`.
#[wasm_bindgen]
pub fn chat_set_permission(request_json: &str) -> String {
    match parse::<PermissionRequest>(request_json) {
        Ok(r) => with_chat(r.handle, |c| {
            to_json(&c.set_permission(&r.request_id, r.status))
        }),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct DropRequest {
    handle: u32,
}

#[wasm_bindgen]
pub fn chat_drop(request_json: &str) -> String {
    if let Ok(r) = parse::<DropRequest>(request_json) {
        CHATS.with(|c| c.borrow_mut().1.remove(&r.handle));
    }
    "null".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_events_through_a_handle() {
        let h = chat_new();
        let u = chat_begin_turn(&format!(r#"{{"handle":{h},"turnId":7,"prompt":"hi"}}"#));
        assert!(
            u.contains(r#""type":"chat""#) && u.contains(r#""id":7"#),
            "{u}"
        );
        let u = chat_apply(&format!(
            r#"{{"handle":{h},"now":1,"event":{{"type":"assistantText","text":"**x**","delta":false}}}}"#
        ));
        assert!(u.contains(r#""t":"strong""#), "{u}");
        chat_drop(&format!(r#"{{"handle":{h}}}"#));
        assert!(chat_apply(&format!(
            r#"{{"handle":{h},"now":1,"event":{{"type":"interrupted"}}}}"#
        ))
        .contains("error"));
    }
}
