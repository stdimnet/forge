//! Recorded JSONL sessions parse into the expected normalized events.

use forge_agent_cli::{claude::ClaudeParser, gemini::GeminiParser, AgentEvent, EventParser};
use serde_json::json;

fn parse_all(parser: &mut dyn EventParser, fixture: &str) -> Vec<AgentEvent> {
    fixture.lines().flat_map(|l| parser.parse_line(l)).collect()
}

#[test]
fn claude_session_fixture() {
    let events = parse_all(
        &mut ClaudeParser::default(),
        include_str!("fixtures/claude_session.jsonl"),
    );

    let AgentEvent::SessionStarted {
        session_id,
        model,
        tools,
        permission_mode,
        ..
    } = &events[0]
    else {
        panic!("expected SessionStarted, got {:?}", events[0]);
    };
    assert_eq!(session_id, "5f0c1d2e-8a7b-4c3d-9e1f-2a3b4c5d6e7f");
    assert_eq!(model.as_deref(), Some("claude-sonnet-4-5-20250929"));
    assert!(tools.contains(&"Edit".to_string()));
    assert_eq!(permission_mode.as_deref(), Some("acceptEdits"));

    // Streamed deltas are emitted, and the final assistant blocks of the same message are
    // not repeated.
    let texts: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::AssistantText { text, delta, .. } => Some((text.as_str(), *delta)),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts,
        vec![
            ("I'll look at the ", true),
            ("`Order` model first.", true),
            ("Added `scopePaid()` to `Order`.", false),
        ]
    );
    let thinking: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::Thinking { .. }))
        .collect();
    assert_eq!(thinking.len(), 2);

    let tools: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::ToolUse { name, input, .. } => Some((name.as_str(), input.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(tools.len(), 3);
    assert_eq!(
        tools[0],
        (
            "Read",
            json!({"file_path": "/home/dev/acme-shop/app/Models/Order.php"})
        )
    );
    assert_eq!(tools[2].1["command"], "php artisan test");

    let results: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::ToolResult {
                tool_use_id,
                is_error,
                content,
            } => Some((tool_use_id.as_str(), *is_error, content.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 3);
    assert!(results[0].2.contains("namespace App\\Models;"));
    assert_eq!(results[2].0, "toolu_03Bash");
    assert!(results[2].1, "bash failure is an error result");

    match events.last().unwrap() {
        AgentEvent::TurnFinished {
            result,
            is_error,
            duration_ms,
            num_turns,
            cost_usd,
            usage,
        } => {
            assert_eq!(result.as_deref(), Some("Added `scopePaid()` to `Order`."));
            assert!(!is_error);
            assert_eq!(*duration_ms, Some(23810));
            assert_eq!(*num_turns, Some(4));
            assert_eq!(*cost_usd, Some(0.04127));
            let usage = usage.as_ref().unwrap();
            assert_eq!(usage.output_tokens, 302);
            assert_eq!(usage.cache_read_tokens, 38211);
        }
        other => panic!("expected TurnFinished, got {other:?}"),
    }
}

#[test]
fn claude_error_result() {
    let mut p = ClaudeParser::default();
    let ev = p.parse_line(
        r#"{"type":"result","subtype":"error_max_turns","is_error":true,"duration_ms":10,"num_turns":30,"session_id":"s"}"#,
    );
    assert!(matches!(
        &ev[0],
        AgentEvent::TurnFinished { is_error: true, result: Some(r), .. } if r == "error max turns"
    ));
}

#[test]
fn events_serialize_for_the_ui() {
    let ev = AgentEvent::ToolResult {
        tool_use_id: "t".into(),
        content: "ok".into(),
        is_error: false,
    };
    assert_eq!(
        serde_json::to_value(&ev).unwrap(),
        json!({"type": "toolResult", "toolUseId": "t", "content": "ok", "isError": false})
    );
}

#[test]
fn gemini_session_fixture() {
    let events = parse_all(
        &mut GeminiParser,
        include_str!("fixtures/gemini_session.jsonl"),
    );
    assert_eq!(events.len(), 6, "{events:#?}");
    assert!(
        matches!(&events[0], AgentEvent::SessionStarted { model: Some(m), .. } if m == "gemini-2.5-pro")
    );
    assert!(matches!(&events[1], AgentEvent::ToolUse { name, .. } if name == "read_file"));
    assert!(
        matches!(&events[3], AgentEvent::ToolResult { is_error: true, content, .. } if content == "0 occurrences found")
    );
    assert!(matches!(
        &events[4],
        AgentEvent::AssistantText { delta: true, .. }
    ));
    assert!(matches!(
        &events[5],
        AgentEvent::TurnFinished {
            is_error: false,
            duration_ms: Some(3900),
            ..
        }
    ));
}
