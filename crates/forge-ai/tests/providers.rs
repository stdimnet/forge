mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{dead_base_url, MockServer, Route};
use forge_ai::{
    cancellable, collect_text, AiError, AiFeature, AiService, CancellationToken, CompletionRequest,
    Delta, MemoryStore, Message, ProviderConfig, ProviderKind, Usage,
};
use futures_util::StreamExt;

fn service() -> AiService {
    AiService::new(None, Arc::new(MemoryStore::default()))
        .unwrap()
        .with_env(|_| None)
        .with_client(reqwest::Client::builder().no_proxy().build().unwrap())
}

fn add(svc: &AiService, kind: ProviderKind, id: &str, base: &str, key: Option<&str>) {
    let mut cfg = ProviderConfig::preset(kind, id);
    cfg.base_url = base.into();
    svc.save_provider(cfg, key).unwrap();
}

fn request() -> CompletionRequest {
    CompletionRequest {
        system: Some("You write commit messages.".into()),
        max_tokens: Some(256),
        ..CompletionRequest::new(vec![Message::user("Summarize the diff")])
    }
}

const ANTHROPIC_SSE: &str = "event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"claude-sonnet-5-5\",\"usage\":{\"input_tokens\":12,\"output_tokens\":1}}}\n\n\
event: content_block_start\n\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
event: ping\n\
data: {\"type\":\"ping\"}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Fix \"}}\n\n\
event: content_block_delta\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Grüße 🔥 bug\"}}\n\n\
event: content_block_stop\n\
data: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
event: message_delta\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\n\
event: message_stop\n\
data: {\"type\":\"message_stop\"}\n\n";

#[tokio::test]
async fn anthropic_complete_sends_headers_and_parses_text() {
    let server = MockServer::start(vec![Route::json(
        "POST",
        "/messages",
        200,
        r#"{"id":"msg_1","type":"message","model":"claude-sonnet-5-5","content":[{"type":"text","text":"Fix login "},{"type":"text","text":"redirect"}],"stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":4}}"#,
    )])
    .await;
    let svc = service();
    add(
        &svc,
        ProviderKind::Anthropic,
        "anthropic",
        &server.base,
        Some("sk-ant-test"),
    );
    let c = svc
        .provider("anthropic")
        .unwrap()
        .complete(request())
        .await
        .unwrap();
    assert_eq!(c.text, "Fix login redirect");
    assert_eq!(c.stop_reason.as_deref(), Some("end_turn"));
    assert_eq!(
        c.usage,
        Some(Usage {
            input_tokens: 10,
            output_tokens: 4
        })
    );

    let req = server.last();
    assert_eq!(req.path, "/v1/messages");
    assert_eq!(req.headers["x-api-key"], "sk-ant-test");
    assert_eq!(req.headers["anthropic-version"], "2023-06-01");
    let body = req.json();
    assert_eq!(body["model"], "claude-sonnet-5-5");
    assert_eq!(body["system"], "You write commit messages.");
    assert_eq!(body["max_tokens"], 256);
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(
        body["messages"][0]["content"][0]["text"],
        "Summarize the diff"
    );
    assert!(body.get("stream").is_none());
}

#[tokio::test]
async fn anthropic_stream_handles_arbitrary_chunk_boundaries() {
    for chunk_size in [1, 3, 7, 64, 4096] {
        let server = MockServer::start(vec![
            Route::sse("/messages", ANTHROPIC_SSE, chunk_size).with_delay(Duration::ZERO)
        ])
        .await;
        let svc = service();
        add(
            &svc,
            ProviderKind::Anthropic,
            "anthropic",
            &server.base,
            Some("k"),
        );
        let stream = svc
            .provider("anthropic")
            .unwrap()
            .stream(request())
            .await
            .unwrap();
        let items: Vec<Delta> = stream.map(|d| d.unwrap()).collect().await;
        let text: String = items
            .iter()
            .filter_map(|d| match d {
                Delta::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(text, "Fix Grüße 🔥 bug", "chunk size {chunk_size}");
        assert_eq!(
            items.last(),
            Some(&Delta::Finish {
                stop_reason: Some("end_turn".into()),
                usage: Some(Usage {
                    input_tokens: 12,
                    output_tokens: 5
                }),
            })
        );
        assert_eq!(server.last().json()["stream"], true);
    }
}

#[tokio::test]
async fn anthropic_stream_error_event_becomes_error() {
    let body = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{}}\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n\
event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n";
    let server = MockServer::start(vec![Route::sse("/messages", body, 5)]).await;
    let svc = service();
    add(
        &svc,
        ProviderKind::Anthropic,
        "anthropic",
        &server.base,
        Some("k"),
    );
    let mut stream = svc
        .provider("anthropic")
        .unwrap()
        .stream(request())
        .await
        .unwrap();
    assert_eq!(
        stream.next().await.unwrap().unwrap(),
        Delta::Text { text: "Hi".into() }
    );
    let err = stream.next().await.unwrap().unwrap_err();
    assert!(err.to_string().contains("overloaded"), "{err}");
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn anthropic_401_maps_to_invalid_api_key() {
    let server = MockServer::start(vec![Route::json(
        "POST",
        "/messages",
        401,
        r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#,
    )])
    .await;
    let svc = service();
    add(
        &svc,
        ProviderKind::Anthropic,
        "anthropic",
        &server.base,
        Some("bad"),
    );
    let err = svc
        .provider("anthropic")
        .unwrap()
        .complete(request())
        .await
        .unwrap_err();
    assert!(matches!(err, AiError::InvalidApiKey { .. }));
    assert_eq!(err.to_string(), "Anthropic: invalid API key");
}

#[tokio::test]
async fn anthropic_without_key_fails_before_request() {
    let server = MockServer::start(vec![]).await;
    let svc = service();
    add(
        &svc,
        ProviderKind::Anthropic,
        "anthropic",
        &server.base,
        None,
    );
    let err = svc
        .provider("anthropic")
        .unwrap()
        .complete(request())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("ANTHROPIC_API_KEY"), "{err}");
    assert!(server.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn anthropic_lists_models() {
    let server = MockServer::start(vec![Route::json(
        "GET",
        "/models",
        200,
        r#"{"data":[{"type":"model","id":"claude-sonnet-5-5","display_name":"Claude Sonnet 5.5"},{"type":"model","id":"claude-haiku-5","display_name":"Claude Haiku 5"}],"has_more":false}"#,
    )])
    .await;
    let svc = service();
    add(
        &svc,
        ProviderKind::Anthropic,
        "anthropic",
        &server.base,
        Some("k"),
    );
    let models = svc.models("anthropic").await.unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].name.as_deref(), Some("Claude Sonnet 5.5"));
    assert_eq!(server.last().headers["x-api-key"], "k");
}

const OPENAI_SSE: &str = "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"}}]}\n\n\
: OPENROUTER PROCESSING\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Add \"}}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"tests ✓\"}}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":3}}\n\n\
data: [DONE]\n\n";

#[tokio::test]
async fn openai_complete_uses_bearer_and_system_message() {
    let server = MockServer::start(vec![Route::json(
        "POST",
        "/chat/completions",
        200,
        r#"{"id":"c1","model":"gpt-4.1","choices":[{"index":0,"message":{"role":"assistant","content":"Refactor auth"},"finish_reason":"stop"}],"usage":{"prompt_tokens":20,"completion_tokens":2}}"#,
    )])
    .await;
    let svc = service();
    add(
        &svc,
        ProviderKind::OpenAi,
        "openai",
        &server.base,
        Some("sk-test"),
    );
    let c = svc
        .provider("openai")
        .unwrap()
        .complete(request())
        .await
        .unwrap();
    assert_eq!(c.text, "Refactor auth");
    assert_eq!(
        c.usage,
        Some(Usage {
            input_tokens: 20,
            output_tokens: 2
        })
    );
    let req = server.last();
    assert_eq!(req.path, "/v1/chat/completions");
    assert_eq!(req.headers["authorization"], "Bearer sk-test");
    let body = req.json();
    assert_eq!(body["model"], "gpt-4.1");
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][1]["content"], "Summarize the diff");
    assert_eq!(body["max_tokens"], 256);
}

#[tokio::test]
async fn openrouter_stream_sends_extra_headers_and_handles_splits() {
    for chunk_size in [1, 2, 5, 13, 4096] {
        let server = MockServer::start(vec![Route::sse(
            "/chat/completions",
            OPENAI_SSE,
            chunk_size,
        )
        .with_delay(Duration::ZERO)])
        .await;
        let svc = service();
        add(
            &svc,
            ProviderKind::OpenRouter,
            "openrouter",
            &server.base,
            Some("or-key"),
        );
        let stream = svc
            .provider("openrouter")
            .unwrap()
            .stream(request())
            .await
            .unwrap();
        let items: Vec<Delta> = stream.map(|d| d.unwrap()).collect().await;
        assert_eq!(
            items,
            vec![
                Delta::Text {
                    text: "Add ".into()
                },
                Delta::Text {
                    text: "tests ✓".into()
                },
                Delta::Finish {
                    stop_reason: Some("stop".into()),
                    usage: Some(Usage {
                        input_tokens: 9,
                        output_tokens: 3
                    }),
                },
            ],
            "chunk size {chunk_size}"
        );
        let req = server.last();
        assert_eq!(req.headers["x-title"], "Forge IDE");
        assert!(req.headers.contains_key("http-referer"));
        assert_eq!(req.headers["authorization"], "Bearer or-key");
    }
}

#[tokio::test]
async fn openai_stream_error_chunk_becomes_error() {
    let body = "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n\
data: {\"error\":{\"message\":\"model crashed\",\"code\":500}}\n\n";
    let server = MockServer::start(vec![Route::sse("/chat/completions", body, 4)]).await;
    let svc = service();
    add(&svc, ProviderKind::Ollama, "ollama", &server.base, None);
    let p = svc.provider("ollama").unwrap();
    let req = CompletionRequest {
        model: Some("qwen3:8b".into()),
        ..request()
    };
    let err = collect_text(p.stream(req).await.unwrap())
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "Ollama: model crashed");
}

#[tokio::test]
async fn openai_http_errors_are_mapped() {
    let server = MockServer::start(vec![
        Route::json(
            "POST",
            "/chat/completions",
            429,
            r#"{"error":{"message":"slow down"}}"#,
        ),
        Route::json(
            "GET",
            "/models",
            401,
            r#"{"error":{"message":"Incorrect API key provided"}}"#,
        ),
    ])
    .await;
    let svc = service();
    add(
        &svc,
        ProviderKind::OpenAi,
        "openai",
        &server.base,
        Some("sk"),
    );
    let p = svc.provider("openai").unwrap();
    let err = p.complete(request()).await.unwrap_err();
    assert!(matches!(err, AiError::RateLimited { .. }), "{err}");
    assert!(err.to_string().contains("slow down"));
    let err = p.list_models().await.unwrap_err();
    assert_eq!(err.to_string(), "OpenAI: invalid API key");
}

#[tokio::test]
async fn lm_studio_needs_no_key_and_lists_models() {
    let server = MockServer::start(vec![Route::json(
        "GET",
        "/models",
        200,
        r#"{"object":"list","data":[{"id":"qwen2.5-coder-7b-instruct","object":"model"},{"id":"deepseek-r1-distill","object":"model"}]}"#,
    )])
    .await;
    let svc = service();
    add(&svc, ProviderKind::LmStudio, "lmstudio", &server.base, None);
    let models = svc.models("lmstudio").await.unwrap();
    assert_eq!(
        models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        ["deepseek-r1-distill", "qwen2.5-coder-7b-instruct"]
    );
    assert!(!server.last().headers.contains_key("authorization"));

    let result = svc.test("lmstudio").await.unwrap();
    assert!(result.ok);
    assert_eq!(result.model_count, Some(2));
}

#[tokio::test]
async fn local_model_without_selection_reports_missing_model() {
    let server = MockServer::start(vec![]).await;
    let svc = service();
    add(&svc, ProviderKind::LlamaCpp, "llamacpp", &server.base, None);
    let err = svc
        .provider("llamacpp")
        .unwrap()
        .complete(request())
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "no model selected for llama.cpp");
}

#[tokio::test]
async fn connection_refused_gives_helpful_hint() {
    let svc = service();
    add(
        &svc,
        ProviderKind::LmStudio,
        "lmstudio",
        &dead_base_url().await,
        None,
    );
    let result = svc.test("lmstudio").await.unwrap();
    assert!(!result.ok);
    let err = result.error.unwrap();
    assert!(err.contains("is LM Studio running?"), "{err}");
}

#[tokio::test]
async fn cancelling_ends_stream_promptly() {
    let server = MockServer::start(vec![
        Route::sse("/chat/completions", OPENAI_SSE, 8).with_delay(Duration::from_millis(200))
    ])
    .await;
    let svc = service();
    add(&svc, ProviderKind::Ollama, "ollama", &server.base, None);
    let req = CompletionRequest {
        model: Some("llama3".into()),
        ..request()
    };
    let token = CancellationToken::new();
    let mut stream = cancellable(
        svc.provider("ollama").unwrap().stream(req).await.unwrap(),
        token.clone(),
    );
    let t = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        t.cancel();
    });
    let started = std::time::Instant::now();
    let mut last = None;
    while let Some(item) = stream.next().await {
        last = Some(item);
    }
    assert!(matches!(last, Some(Err(AiError::Cancelled))));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn resolve_prefers_feature_selection_then_default() {
    let server = MockServer::start(vec![Route::json(
        "POST",
        "/chat/completions",
        200,
        r#"{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}]}"#,
    )])
    .await;
    let svc = service();
    add(
        &svc,
        ProviderKind::Anthropic,
        "anthropic",
        "https://example.invalid/v1",
        Some("k"),
    );
    add(&svc, ProviderKind::Ollama, "ollama", &server.base, None);
    svc.set_default("ollama", Some("qwen3:4b"), Some(AiFeature::CommitMessages))
        .unwrap();

    let (p, model) = svc.resolve(None, None, None).unwrap();
    assert_eq!(p.name(), "Anthropic");
    assert_eq!(model, None);
    assert_eq!(p.default_model(), Some("claude-sonnet-5-5"));

    let (p, model) = svc
        .resolve(None, None, Some(AiFeature::CommitMessages))
        .unwrap();
    assert_eq!(p.name(), "Ollama");
    let c = p
        .complete(CompletionRequest { model, ..request() })
        .await
        .unwrap();
    assert_eq!(c.text, "ok");
    assert_eq!(server.last().json()["model"], "qwen3:4b");

    svc.set_default("ollama", Some("llama3"), None).unwrap();
    let state = svc.state();
    assert_eq!(state.default_provider.as_deref(), Some("ollama"));
    assert!(state
        .providers
        .iter()
        .any(|p| p.is_default && p.config.id == "ollama"));
    assert!(state
        .providers
        .iter()
        .any(|p| p.has_api_key && p.config.id == "anthropic"));
}

#[tokio::test]
async fn saving_persists_config_and_keys_stay_out_of_toml() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ai.toml");
    let store = Arc::new(MemoryStore::default());
    let svc = AiService::new(Some(path.clone()), store.clone()).unwrap();
    let saved = svc
        .save_provider(
            ProviderConfig {
                id: String::new(),
                ..ProviderConfig::preset(ProviderKind::OpenRouter, "")
            },
            Some("secret-key"),
        )
        .unwrap();
    assert_eq!(saved.id, "openrouter");
    let toml = std::fs::read_to_string(&path).unwrap();
    assert!(toml.contains("openrouter"));
    assert!(!toml.contains("secret-key"));

    let reloaded = AiService::new(Some(path), store).unwrap();
    assert_eq!(reloaded.config().providers.len(), 1);
    reloaded.delete_provider("openrouter").unwrap();
    assert!(reloaded.config().providers.is_empty());
}

#[tokio::test]
async fn fim_posts_prompt_and_suffix_to_completions() {
    let server = MockServer::start(vec![Route::json(
        "POST",
        "/completions",
        200,
        r#"{"model":"qwen2.5-coder:1.5b","choices":[{"text":"a + b;","finish_reason":"stop"}],"usage":{"prompt_tokens":12,"completion_tokens":4}}"#,
    )])
    .await;
    let svc = service();
    add(&svc, ProviderKind::Ollama, "ollama", &server.base, None);
    let (p, _) = svc.resolve(Some("ollama"), None, None).unwrap();
    assert!(p.supports_fim());
    let c = p
        .fim(forge_ai::FimRequest {
            model: Some("qwen2.5-coder:1.5b".into()),
            prompt: "function add(a, b) {\n  return ".into(),
            suffix: "\n}".into(),
            max_tokens: Some(64),
            temperature: Some(0.0),
            stop: vec!["\n".into()],
        })
        .await
        .unwrap();
    assert_eq!(c.text, "a + b;");
    assert_eq!(c.usage.unwrap().output_tokens, 4);
    let body = server.last().json();
    assert_eq!(body["prompt"], "function add(a, b) {\n  return ");
    assert_eq!(body["suffix"], "\n}");
    assert_eq!(body["stop"][0], "\n");
    assert_eq!(body["stream"], false);
}

#[tokio::test]
async fn hosted_providers_do_not_offer_fim_and_disabled_providers_are_refused() {
    let svc = service();
    add(
        &svc,
        ProviderKind::Anthropic,
        "anthropic",
        "https://example.invalid/v1",
        Some("k"),
    );
    let (p, _) = svc.resolve(None, None, None).unwrap();
    assert!(!p.supports_fim());
    let err = p
        .fim(forge_ai::FimRequest::default())
        .await
        .err()
        .unwrap();
    assert!(matches!(err, AiError::Unsupported { .. }), "{err}");

    let mut cfg = svc.config().providers[0].clone();
    cfg.enabled = false;
    svc.save_provider(cfg, None).unwrap();
    let err = svc
        .resolve(Some("anthropic"), None, None)
        .err()
        .unwrap();
    assert!(matches!(err, AiError::ProviderDisabled(_)), "{err}");
}
