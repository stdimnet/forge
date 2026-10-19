//! Process lifecycle tests against a fake CLI script (Unix only: it is a shell script).
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_agent_cli::{AgentEvent, AgentSession, ClaudeCode, EventSink, StartOptions};
use tokio::sync::mpsc;

fn fake_cli(dir: &tempfile::TempDir) -> PathBuf {
    let path = dir.path().join("claude");
    std::fs::write(&path, include_str!("fixtures/fake_claude.sh")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

async fn next_matching(
    rx: &mut mpsc::UnboundedReceiver<AgentEvent>,
    pred: impl Fn(&AgentEvent) -> bool,
) -> Vec<AgentEvent> {
    let mut seen = Vec::new();
    loop {
        let ev = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("timed out waiting for agent event")
            .expect("event channel closed");
        let done = pred(&ev);
        seen.push(ev);
        if done {
            return seen;
        }
    }
}

async fn start(dir: &tempfile::TempDir) -> (AgentSession, mpsc::UnboundedReceiver<AgentEvent>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let sink: EventSink = Arc::new(move |e| {
        let _ = tx.send(e);
    });
    let opts = StartOptions {
        executable: Some(fake_cli(dir).display().to_string()),
        model: Some("opus".into()),
        ..Default::default()
    };
    let session = AgentSession::start(Arc::new(ClaudeCode), dir.path().into(), opts, sink)
        .await
        .unwrap();
    (session, rx)
}

#[tokio::test]
async fn conversation_turns_stream_events() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut rx) = start(&dir).await;

    session.send("hello").await.unwrap();
    let events = next_matching(&mut rx, |e| matches!(e, AgentEvent::TurnFinished { .. })).await;
    assert!(
        matches!(&events[0], AgentEvent::SessionStarted { session_id, .. } if session_id == "fake-session")
    );
    assert!(events.iter().any(|e| matches!(e,
        AgentEvent::AssistantText { text, .. } if text.contains("--model opus") && text.contains("--input-format stream-json"))));
    assert_eq!(session.conversation_id().as_deref(), Some("fake-session"));

    // Same process answers the second turn (no second init).
    session.send("again").await.unwrap();
    let events = next_matching(&mut rx, |e| matches!(e, AgentEvent::TurnFinished { .. })).await;
    assert!(!events
        .iter()
        .any(|e| matches!(e, AgentEvent::SessionStarted { .. })));

    session.stop().await;
    next_matching(&mut rx, |e| matches!(e, AgentEvent::Exited { .. })).await;
}

#[tokio::test]
async fn interrupt_then_resume() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut rx) = start(&dir).await;

    session.send("first").await.unwrap();
    next_matching(&mut rx, |e| matches!(e, AgentEvent::TurnFinished { .. })).await;

    session.send("be slow").await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    session.interrupt().await;
    let events = next_matching(&mut rx, |e| matches!(e, AgentEvent::Exited { .. })).await;
    assert!(events.iter().any(|e| matches!(e, AgentEvent::Interrupted)));
    assert!(!events.iter().any(|e| matches!(e, AgentEvent::Error { .. })));

    // The next message respawns the CLI and resumes the conversation.
    session.send("continue").await.unwrap();
    let events = next_matching(&mut rx, |e| matches!(e, AgentEvent::TurnFinished { .. })).await;
    assert!(events.iter().any(|e| matches!(e,
        AgentEvent::AssistantText { text, .. } if text.contains("--resume fake-session"))));
    session.stop().await;
}

#[tokio::test]
async fn missing_executable_is_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    let sink: EventSink = Arc::new(|_| {});
    let opts = StartOptions {
        executable: Some(dir.path().join("nope").display().to_string()),
        ..Default::default()
    };
    let err = AgentSession::start(Arc::new(ClaudeCode), dir.path().into(), opts, sink)
        .await
        .err()
        .unwrap();
    assert!(err.to_string().contains("Claude Code was not found"));
}
