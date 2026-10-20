//! Per-turn process lifecycle of the Gemini CLI adapter against a fake `gemini` script
//! (Unix only: it is a shell script).
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_agent_cli::{
    AgentEvent, AgentSession, EventSink, GeminiCli, PermissionMode, StartOptions,
};
use tokio::sync::mpsc;

fn fake_cli(dir: &tempfile::TempDir) -> PathBuf {
    let path = dir.path().join("gemini");
    std::fs::write(&path, include_str!("fixtures/fake_gemini.sh")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

type Events = mpsc::UnboundedReceiver<AgentEvent>;

async fn next_matching(rx: &mut Events, pred: impl Fn(&AgentEvent) -> bool) -> Vec<AgentEvent> {
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

/// Events until the turn ended (finished, or interrupted and exited).
async fn turn(rx: &mut Events) -> Vec<AgentEvent> {
    next_matching(rx, |e| {
        matches!(
            e,
            AgentEvent::TurnFinished { .. } | AgentEvent::Exited { .. }
        )
    })
    .await
}

async fn start(dir: &tempfile::TempDir, mode: PermissionMode) -> (AgentSession, Events) {
    let (tx, rx) = mpsc::unbounded_channel();
    let sink: EventSink = Arc::new(move |e| {
        let _ = tx.send(e);
    });
    let opts = StartOptions {
        executable: Some(fake_cli(dir).display().to_string()),
        model: Some("gemini-2.5-flash".into()),
        permission_mode: mode,
        ..Default::default()
    };
    let session = AgentSession::start(Arc::new(GeminiCli), dir.path().into(), opts, sink)
        .await
        .expect("cannot start the Gemini session");
    (session, rx)
}

/// Send a message. A freshly written script can briefly fail with "Text file busy"
/// (ETXTBSY) when another test thread forks while the file is still open for writing.
async fn send(session: &AgentSession, text: &str) {
    for attempt in 0.. {
        match session.send(text).await {
            Ok(()) => return,
            Err(e) if attempt < 5 && e.to_string().to_lowercase().contains("busy") => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(e) => panic!("cannot run the fake CLI: {e}"),
        }
    }
}

fn text_of(events: &[AgentEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::AssistantText { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn every_turn_is_a_process_that_resumes_the_conversation() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut rx) = start(&dir, PermissionMode::AcceptEdits).await;

    send(&session, "hello\nsecond line").await;
    let events = turn(&mut rx).await;
    let AgentEvent::SessionStarted {
        session_id, cwd, ..
    } = &events[0]
    else {
        panic!("{events:#?}")
    };
    assert_eq!(session_id, "fake-gemini-session");
    // Gemini does not report its working directory; the session fills in the project root.
    assert_eq!(cwd.as_deref(), Some(dir.path().to_str().unwrap()));
    let text = text_of(&events);
    assert!(
        text.contains("args: --output-format stream-json --model gemini-2.5-flash --approval-mode auto_edit |"),
        "{text}"
    );
    assert!(!text.contains("--resume"), "{text}");
    // The prompt arrives on stdin, newlines intact.
    assert!(text.contains("prompt: hello (2 lines)"), "{text}");
    assert!(matches!(
        events.last(),
        Some(AgentEvent::TurnFinished {
            is_error: false,
            ..
        })
    ));
    assert_eq!(
        session.conversation_id().as_deref(),
        Some("fake-gemini-session")
    );

    // The next message is a new process that resumes the conversation. The previous
    // process ended with its turn, so no `Exited` leaks into this one.
    send(&session, "again").await;
    let events = turn(&mut rx).await;
    assert!(
        text_of(&events).contains("--resume fake-gemini-session"),
        "{events:#?}"
    );
    assert!(!events
        .iter()
        .any(|e| matches!(e, AgentEvent::Exited { .. } | AgentEvent::Error { .. })));
    session.stop().await;
}

#[tokio::test]
async fn tool_calls_use_the_common_vocabulary() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut rx) = start(&dir, PermissionMode::BypassPermissions).await;
    send(&session, "please edit app.php").await;
    let events = turn(&mut rx).await;
    let edit = events
        .iter()
        .find_map(|e| match e {
            AgentEvent::ToolUse { name, input, .. } => Some((name.clone(), input.clone())),
            _ => None,
        })
        .expect("tool call");
    assert_eq!(edit.0, "Edit");
    assert!(edit.1["file_path"].as_str().unwrap().ends_with("/app.php"));
    assert!(text_of(&events).contains("--approval-mode yolo"));
    session.stop().await;
}

#[tokio::test]
async fn error_results_are_reported_once() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut rx) = start(&dir, PermissionMode::Plan).await;
    send(&session, "fail").await;
    let events = turn(&mut rx).await;
    assert!(
        matches!(events.last(), Some(AgentEvent::TurnFinished { is_error: true, result: Some(r), .. }) if r == "Reached max session turns"),
        "{events:#?}"
    );
    // The non-zero exit code after the error result adds no second error.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(rx.try_recv().is_err());

    // A crash without a result shows stderr and the exit code.
    send(&session, "crash").await;
    let events = next_matching(&mut rx, |e| matches!(e, AgentEvent::Exited { .. })).await;
    assert!(events.iter().any(|e| matches!(e,
        AgentEvent::Error { message } if message.contains("Auth method"))));
    assert!(matches!(
        events.last(),
        Some(AgentEvent::Exited { code: Some(41) })
    ));
    session.stop().await;
}

#[tokio::test]
async fn interrupt_then_resume() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut rx) = start(&dir, PermissionMode::Default).await;

    send(&session, "first").await;
    let events = turn(&mut rx).await;
    assert!(text_of(&events).contains("--approval-mode default"));

    send(&session, "be slow").await;
    next_matching(&mut rx, |e| matches!(e, AgentEvent::SessionStarted { .. })).await;
    session.interrupt().await;
    // `interrupt` returns once the process' last events were emitted.
    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::Interrupted)),
        "{events:#?}"
    );
    assert!(matches!(events.last(), Some(AgentEvent::Exited { .. })));
    assert!(!events.iter().any(|e| matches!(e, AgentEvent::Error { .. })));

    send(&session, "continue").await;
    let events = turn(&mut rx).await;
    assert!(text_of(&events).contains("--resume fake-gemini-session"));
    session.stop().await;
}

#[tokio::test]
async fn permission_answers_are_not_supported() {
    let dir = tempfile::tempdir().unwrap();
    let (session, _rx) = start(&dir, PermissionMode::Default).await;
    let err = session
        .respond_permission(
            "r1",
            forge_agent_cli::PermissionDecision::Deny { message: None },
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("no pending permission request"));
}
