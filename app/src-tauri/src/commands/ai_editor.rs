//! AI in the editor (`forge-ai-editor` + `forge-ai` providers): inline (ghost text)
//! completion, "Explain with AI" streamed into the AI Chat, and "Fix with AI", "Generate
//! Documentation with AI" and "Generate Tests with AI" proposals shown as diffs.
//!
//! No code leaves the editor unless a provider is configured and enabled, AI is on for the
//! project (`ai.enabled`) and the file is not excluded by `.aiignore`; inline completion
//! additionally needs `ai.completion.enabled` and honours the per-language switch. Nothing
//! here runs project code, so it behaves the same in Safe Mode.

use std::collections::HashMap;
use std::sync::Mutex;

use forge_ai::{AiFeature, CancellationToken, CompletionRequest, FimRequest, Message};
use forge_ai_editor::inline::CompletionMode;
use forge_ai_editor::policy::IGNORE_FILES;
use forge_ai_editor::{
    build_context, build_prompt, postprocess, ActionKind, ActionRequest, AiIgnore, Feature,
    InlineRequest, MultilinePolicy, Policy, Proposal, Verdict,
};
use forge_core::{ForgeError, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::commands::ai::{start_stream, stream_event_name, AiState, StreamEvent};
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Settings read here (schema: `app/src-tauri/plugins/packs.toml`, Tools | AI Assistant).
pub const PROJECT_ENABLED: &str = "ai.enabled";
pub const COMPLETION_ENABLED: &str = "ai.completion.enabled";
pub const COMPLETION_MODE: &str = "ai.completion.mode";
pub const COMPLETION_MULTILINE: &str = "ai.completion.multiline";
pub const DISABLED_LANGUAGES: &str = "ai.completion.disabledLanguages";

/// Running inline completion requests, by the editor's request id.
#[derive(Default)]
pub struct AiEditorState {
    inline: Mutex<HashMap<String, CancellationToken>>,
}

fn setting<T: serde::de::DeserializeOwned>(state: &AppState, key: &str, default: T) -> T {
    state.read_settings().get_as(key).unwrap_or(default)
}

/// The policy of the open project (settings + `.aiignore` / `.aiexclude`).
async fn policy(state: &AppState) -> Policy {
    let project_enabled = setting(state, PROJECT_ENABLED, true);
    let disabled_languages = setting::<Vec<String>>(state, DISABLED_LANGUAGES, Vec::new());
    let ignore = match state.backend() {
        Ok(backend) => blocking(move || {
            let texts: Vec<String> = IGNORE_FILES
                .iter()
                .filter_map(|f| backend.read_bytes(f).ok())
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .collect();
            Ok(AiIgnore::merged(texts.iter().map(String::as_str)))
        })
        .await
        .unwrap_or_default(),
        Err(_) => AiIgnore::default(),
    };
    Policy {
        project_enabled,
        disabled_languages,
        ignore,
    }
}

fn denied(v: &Verdict) -> ForgeError {
    ForgeError::Other(v.message().unwrap_or("not allowed").to_string())
}

/// What the editor shows for a suggestion request.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineAnswer {
    /// Ghost text to show; `None` when there is nothing to suggest.
    pub text: Option<String>,
    /// Why nothing was asked (`disabled`, `cancelled`, a policy message) — not an error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

fn skipped(reason: impl Into<String>) -> InlineAnswer {
    InlineAnswer {
        text: None,
        reason: Some(reason.into()),
    }
}

/// One inline completion. `temperature` > 0 asks for an alternative (Next Proposal).
#[tauri::command]
pub async fn ai_inline_complete(
    request: InlineRequest,
    request_id: String,
    temperature: Option<f32>,
    state: State<'_, AppState>,
    ai: State<'_, AiState>,
    editor: State<'_, AiEditorState>,
) -> Result<InlineAnswer> {
    if !setting(&state, COMPLETION_ENABLED, false) {
        return Ok(skipped("disabled"));
    }
    let mut request = request;
    request.multiline = setting(&state, COMPLETION_MULTILINE, MultilinePolicy::Auto);
    let ctx = build_context(&request);
    let verdict = policy(&state)
        .await
        .check(&request.path, ctx.language.id(), Feature::InlineCompletion);
    if !verdict.is_allowed() {
        return Ok(skipped(verdict.message().unwrap_or_default()));
    }
    let (cfg, model) = ai
        .service
        .selection(None, None, Some(AiFeature::Completion))?;
    let provider = ai.service.build(&cfg);
    let mode = setting(&state, COMPLETION_MODE, CompletionMode::Auto);
    let use_fim = match mode {
        CompletionMode::Fim => true,
        CompletionMode::Chat => false,
        CompletionMode::Auto => provider.supports_fim(),
    };
    let prompt = build_prompt(&ctx);
    let temperature = Some(temperature.unwrap_or(0.0));

    let token = CancellationToken::new();
    if let Some(old) = editor
        .inline
        .lock()
        .expect("inline lock")
        .insert(request_id.clone(), token.clone())
    {
        old.cancel();
    }
    let call = async {
        if use_fim {
            provider
                .fim(FimRequest {
                    model,
                    prompt: prompt.fim_prompt.clone(),
                    suffix: prompt.fim_suffix.clone(),
                    max_tokens: Some(prompt.max_tokens),
                    temperature,
                    stop: prompt.stop.clone(),
                })
                .await
        } else {
            provider
                .complete(CompletionRequest {
                    model,
                    system: Some(prompt.system.clone()),
                    messages: vec![Message::user(prompt.user.clone())],
                    max_tokens: Some(prompt.max_tokens),
                    temperature,
                    stop: Vec::new(),
                })
                .await
        }
    };
    let result = tokio::select! {
        biased;
        _ = token.cancelled() => None,
        r = call => Some(r),
    };
    editor.inline.lock().expect("inline lock").remove(&request_id);
    match result {
        None => Ok(skipped("cancelled")),
        Some(r) => Ok(InlineAnswer {
            text: postprocess(&r?.text, &ctx),
            reason: None,
        }),
    }
}

/// Stop a running inline completion (the user typed on or moved the caret).
#[tauri::command]
pub fn ai_inline_cancel(request_id: String, editor: State<'_, AiEditorState>) -> Result<()> {
    if let Some(t) = editor
        .inline
        .lock()
        .expect("inline lock")
        .remove(&request_id)
    {
        t.cancel();
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummary {
    pub id: String,
    pub name: String,
    pub model: Option<String>,
    /// Runs on this machine (Ollama, LM Studio, llama.cpp).
    pub local: bool,
    /// Completion uses fill-in-the-middle (else a chat prompt).
    pub fim: bool,
}

/// The status bar widget and the AI entries of the editor menus.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiEditorStatus {
    pub project_enabled: bool,
    pub completion_enabled: bool,
    /// Provider for inline completion, or why there is none.
    pub completion: Option<ProviderSummary>,
    pub completion_problem: Option<String>,
    /// Provider for the actions (Explain, Fix, Generate), or why there is none.
    pub actions: Option<ProviderSummary>,
    pub actions_problem: Option<String>,
    /// Inline completion verdict for `path`.
    pub file: Option<Verdict>,
    /// Actions verdict for `path`.
    pub file_actions: Option<Verdict>,
}

#[tauri::command]
pub async fn ai_editor_status(
    path: Option<String>,
    language: Option<String>,
    state: State<'_, AppState>,
    ai: State<'_, AiState>,
) -> Result<AiEditorStatus> {
    let mode = setting(&state, COMPLETION_MODE, CompletionMode::Auto);
    let summary = |feature| match ai.service.selection(None, None, Some(feature)) {
        Ok((cfg, model)) => (
            Some(ProviderSummary {
                model: model.or_else(|| cfg.default_model.clone()),
                local: cfg.kind.is_local(),
                fim: match mode {
                    CompletionMode::Fim => true,
                    CompletionMode::Chat => false,
                    CompletionMode::Auto => cfg.kind.supports_fim(),
                },
                id: cfg.id,
                name: cfg.name,
            }),
            None,
        ),
        Err(e) => (None, Some(e.to_string())),
    };
    let (completion, completion_problem) = summary(AiFeature::Completion);
    let (actions, actions_problem) = summary(AiFeature::Chat);
    let (file, file_actions) = match path {
        Some(p) => {
            let policy = policy(&state).await;
            let lang = forge_ai_editor::language_of(&p, language.as_deref());
            (
                Some(policy.check(&p, lang.id(), Feature::InlineCompletion)),
                Some(policy.check(&p, lang.id(), Feature::Action)),
            )
        }
        None => (None, None),
    };
    Ok(AiEditorStatus {
        project_enabled: setting(&state, PROJECT_ENABLED, true),
        completion_enabled: setting(&state, COMPLETION_ENABLED, false),
        completion,
        completion_problem,
        actions,
        actions_problem,
        file,
        file_actions,
    })
}

async fn check_action(state: &AppState, request: &ActionRequest) -> Result<()> {
    let lang = forge_ai_editor::language_of(&request.path, request.language.as_deref());
    let verdict = policy(state)
        .await
        .check(&request.path, lang.id(), Feature::Action);
    if verdict.is_allowed() {
        Ok(())
    } else {
        Err(denied(&verdict))
    }
}

/// "Fix with AI", "Generate Documentation with AI" or "Generate Tests with AI": ask the
/// actions provider and return the proposed change (nothing is written).
#[tauri::command]
pub async fn ai_action(
    request: ActionRequest,
    state: State<'_, AppState>,
    ai: State<'_, AiState>,
) -> Result<Proposal> {
    if request.kind == ActionKind::Explain {
        return Err(ForgeError::Other(
            "Explain with AI streams into the AI Chat (ai_explain)".into(),
        ));
    }
    check_action(&state, &request).await?;
    let mut request = request;
    let prompt = forge_ai_editor::prompt(&request).map_err(ForgeError::Other)?;
    if let Some(test_path) = prompt
        .test_path
        .clone()
        .filter(|t| *t != request.path)
    {
        let backend = state.backend()?;
        request.existing_test = blocking(move || {
            Ok(backend
                .read_bytes(&test_path)
                .ok()
                .map(|b| String::from_utf8_lossy(&b).into_owned()))
        })
        .await?;
    }
    // The existing test file changes the prompt.
    let prompt = forge_ai_editor::prompt(&request).map_err(ForgeError::Other)?;
    let (p, model) = ai.service.resolve(None, None, Some(AiFeature::Chat))?;
    let reply = p
        .complete(CompletionRequest {
            model,
            system: Some(prompt.system),
            messages: vec![Message::user(prompt.user)],
            max_tokens: Some(if request.kind == ActionKind::Tests {
                8192
            } else {
                4096
            }),
            temperature: Some(0.2),
            stop: Vec::new(),
        })
        .await?;
    forge_ai_editor::proposal(&request, &reply.text).map_err(ForgeError::Other)
}

/// What the AI Chat shows for an explanation turn.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainStarted {
    pub title: String,
    /// The code that was sent, as Markdown (the turn's prompt).
    pub prompt: String,
    pub provider: String,
    pub model: Option<String>,
}

/// "Explain with AI": stream the explanation as `ai://stream/<streamId>` events (subscribe
/// first; `ai_cancel` stops it).
#[tauri::command]
pub async fn ai_explain(
    request: ActionRequest,
    stream_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
    ai: State<'_, AiState>,
) -> Result<ExplainStarted> {
    check_action(&state, &request).await?;
    let prompt = forge_ai_editor::explain_prompt(&request).map_err(ForgeError::Other)?;
    let (cfg, model) = ai.service.selection(None, None, Some(AiFeature::Chat))?;
    let provider = ai.service.build(&cfg);
    let started = ExplainStarted {
        title: prompt.title.clone(),
        prompt: prompt.user.clone(),
        provider: cfg.name.clone(),
        model: model.clone().or_else(|| cfg.default_model.clone()),
    };
    let event = stream_event_name(&stream_id);
    let emit = move |payload: StreamEvent| {
        if let Err(e) = app.emit(&event, payload) {
            tracing::warn!("ai explain emit failed: {e}");
        }
    };
    let req = CompletionRequest {
        model,
        system: Some(prompt.system),
        messages: vec![Message::user(prompt.user)],
        max_tokens: Some(2048),
        temperature: Some(0.3),
        stop: Vec::new(),
    };
    start_stream(provider, req, stream_id, emit, &ai).await?;
    Ok(started)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_match_the_ipc_contract() {
        let r: InlineRequest = serde_json::from_value(serde_json::json!({
            "path": "src/a.ts", "language": "typescript", "text": "let x = ", "offset": 8,
            "related": [{ "path": "src/b.ts", "text": "export {}" }]
        }))
        .unwrap();
        assert_eq!((r.offset, r.related.len()), (8, 1));
        let a: ActionRequest = serde_json::from_value(serde_json::json!({
            "kind": "fix", "path": "a.php", "text": "<?php", "from": 0, "to": 0,
            "problems": [{ "message": "Undefined variable", "line": 0 }]
        }))
        .unwrap();
        assert_eq!(a.kind, ActionKind::Fix);
        assert_eq!(
            serde_json::to_value(skipped("disabled")).unwrap(),
            serde_json::json!({ "text": null, "reason": "disabled" })
        );
    }
}
