//! AI in the editor (`forge_ai_editor`) for the browser-mode mock: the fake provider's
//! canned answers go through the same post-processing, prompts, proposals and `.aiignore`
//! policy as the desktop app's `ai_inline_complete`, `ai_action` and `ai_explain`.

use forge_ai_editor::{
    build_context, postprocess, ActionRequest, AiIgnore, Feature, InlineRequest, Policy,
};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::parsers::parse;
use crate::to_json;

fn error(message: impl Into<String>) -> String {
    to_json(&serde_json::json!({ "error": message.into() }))
}

#[derive(Deserialize)]
struct InlineFinish {
    request: InlineRequest,
    raw: String,
}

/// `{ request, raw }` → `{ text, language, multiline }`: the ghost text the editor shows for
/// a raw model answer.
#[wasm_bindgen]
pub fn ai_inline_finish(request_json: &str) -> String {
    let r = match parse::<InlineFinish>(request_json) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let ctx = build_context(&r.request);
    to_json(&serde_json::json!({
        "text": postprocess(&r.raw, &ctx),
        "language": ctx.language.id(),
        "multiline": ctx.multiline,
    }))
}

/// `ActionRequest` → `ActionPrompt` (or `{ error }` when the action does not apply).
#[wasm_bindgen]
pub fn ai_action_prompt(request_json: &str) -> String {
    match parse::<ActionRequest>(request_json) {
        Ok(r) => match forge_ai_editor::prompt(&r) {
            Ok(p) => to_json(&p),
            Err(e) => error(e),
        },
        Err(e) => e,
    }
}

#[derive(Deserialize)]
struct ProposalRequest {
    request: ActionRequest,
    reply: String,
}

/// `{ request, reply }` → `Proposal` (or `{ error }`).
#[wasm_bindgen]
pub fn ai_action_proposal(request_json: &str) -> String {
    match parse::<ProposalRequest>(request_json) {
        Ok(r) => match forge_ai_editor::proposal(&r.request, &r.reply) {
            Ok(p) => to_json(&p),
            Err(e) => error(e),
        },
        Err(e) => e,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolicyRequest {
    path: String,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    inline: bool,
    #[serde(default = "yes")]
    project_enabled: bool,
    #[serde(default)]
    disabled_languages: Vec<String>,
    /// Contents of `.aiignore` / `.aiexclude`.
    #[serde(default)]
    ignore: Vec<String>,
}

fn yes() -> bool {
    true
}

/// `{ path, language?, inline, projectEnabled, disabledLanguages, ignore }` → `Verdict`.
#[wasm_bindgen]
pub fn ai_policy_check(request_json: &str) -> String {
    let r = match parse::<PolicyRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let policy = Policy {
        project_enabled: r.project_enabled,
        disabled_languages: r.disabled_languages,
        ignore: AiIgnore::merged(r.ignore.iter().map(String::as_str)),
    };
    let lang = forge_ai_editor::language_of(&r.path, r.language.as_deref());
    let feature = if r.inline {
        Feature::InlineCompletion
    } else {
        Feature::Action
    };
    to_json(&policy.check(&r.path, lang.id(), feature))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_inline_actions_and_policy() {
        let out = ai_inline_finish(
            r#"{"request":{"path":"a.ts","text":"sum()","offset":4},"raw":"```ts\na, b)\n```"}"#,
        );
        assert!(out.contains(r#""text":"a, b""#), "{out}");
        let p = ai_action_prompt(r#"{"kind":"fix","path":"a.php","text":"<?php","from":0,"to":0}"#);
        assert_eq!(p, r#"{"error":"No problems at the caret"}"#);
        let prop = ai_action_proposal(
            r#"{"request":{"kind":"tests","path":"src/m.ts","text":"export const one = () => 1;\n","from":0,"to":0},"reply":"```ts\ntest('one', () => {});\n```"}"#,
        );
        assert!(
            prop.contains(r#""path":"src/m.test.ts""#) && prop.contains(r#""newFile":true"#),
            "{prop}"
        );
        let v = ai_policy_check(r#"{"path":".env","ignore":[".env\n"]}"#);
        assert!(v.contains(r#""status":"ignored""#), "{v}");
        assert!(ai_policy_check("nope").contains("error"));
    }
}
