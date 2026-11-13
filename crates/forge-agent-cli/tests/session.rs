//! Process lifecycle tests against a fake CLI script (Unix only: it is a shell script).
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_agent_cli::{
    AgentEvent, AgentSession, AllowScope, ClaudeCode, EventSink, PermissionDecision, StartOptions,
};
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
    // A freshly written script can briefly fail with "Text file busy" (ETXTBSY) when another
    // test thread forks while the file is still open for writing, so retry a few times.
    let mut attempt = 0;
    let session = loop {
        match AgentSession::start(
            Arc::new(ClaudeCode),
            dir.path().into(),
            opts.clone(),
            sink.clone(),
        )
        .await
        {
            Ok(s) => break s,
            Err(e) if attempt < 5 && e.to_string().to_lowercase().contains("busy") => {
                attempt += 1;
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(e) => panic!("cannot start the fake CLI: {e}"),
        }
    };
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

fn permission_request(events: &[AgentEvent]) -> String {
    events
        .iter()
        .find_map(|e| match e {
            AgentEvent::PermissionRequest {
                request_id,
                tool_name,
                ..
            } if tool_name == "Bash" => Some(request_id.clone()),
            _ => None,
        })
        .expect("permission request")
}

fn said(events: &[AgentEvent], text: &str) -> bool {
    events
        .iter()
        .any(|e| matches!(e, AgentEvent::AssistantText { text: t, .. } if t == text))
}

#[tokio::test]
async fn answers_permission_requests() {
    let dir = tempfile::tempdir().unwrap();
    let (session, mut rx) = start(&dir).await;

    // Deny once.
    session.send("needs-permission").await.unwrap();
    let events = next_matching(&mut rx, |e| {
        matches!(e, AgentEvent::PermissionRequest { .. })
    })
    .await;
    let id = permission_request(&events);
    session
        .respond_permission(&id, PermissionDecision::Deny { message: None })
        .await
        .unwrap();
    let events = next_matching(&mut rx, |e| matches!(e, AgentEvent::TurnFinished { .. })).await;
    assert!(said(&events, "permission: deny"));
    assert!(
        events.iter().any(|e| matches!(e,
        AgentEvent::AssistantText { text, .. } if text.contains("--permission-prompt-tool stdio")))
    );
    // Answered requests cannot be answered twice.
    assert!(session
        .respond_permission(&id, PermissionDecision::Deny { message: None })
        .await
        .is_err());

    // Allow for the session: the next identical request is answered without asking.
    session.send("needs-permission").await.unwrap();
    let events = next_matching(&mut rx, |e| {
        matches!(e, AgentEvent::PermissionRequest { .. })
    })
    .await;
    let id = permission_request(&events);
    let allow = PermissionDecision::Allow {
        updated_input: None,
        scope: AllowScope::Session,
    };
    session.respond_permission(&id, allow).await.unwrap();
    let events = next_matching(&mut rx, |e| matches!(e, AgentEvent::TurnFinished { .. })).await;
    assert!(said(&events, "permission: allow"));

    session.send("needs-permission").await.unwrap();
    let events = next_matching(&mut rx, |e| matches!(e, AgentEvent::TurnFinished { .. })).await;
    assert!(said(&events, "permission: allow"));
    assert!(!events
        .iter()
        .any(|e| matches!(e, AgentEvent::PermissionRequest { .. })));
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

/// A host that runs processes on this machine but counts them, like a remote host would
/// run them through the project backend.
#[derive(Default)]
struct CountingHost {
    spawned: std::sync::Mutex<Vec<(PathBuf, PathBuf)>>,
}

impl forge_agent_cli::AgentHost for CountingHost {
    fn detect(
        &self,
        _adapter: &dyn forge_agent_cli::AgentCli,
        override_path: Option<&str>,
    ) -> Option<PathBuf> {
        override_path.map(PathBuf::from)
    }

    fn spawn(
        &self,
        spec: forge_agent_cli::CommandSpec,
        cwd: PathBuf,
    ) -> forge_agent_cli::host::SpawnFuture {
        self.spawned
            .lock()
            .unwrap()
            .push((spec.program.clone(), cwd.clone()));
        forge_agent_cli::LocalHost.spawn(spec, cwd)
    }

    fn remote_label(&self) -> Option<String> {
        Some("deploy@shop.example.com".into())
    }
}

#[tokio::test]
async fn sessions_run_on_the_project_host() {
    use forge_agent_cli::AgentHost;

    let dir = tempfile::tempdir().unwrap();
    let host = Arc::new(CountingHost::default());
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sink: EventSink = Arc::new(move |e| {
        let _ = tx.send(e);
    });
    let missing = AgentSession::start_on(
        host.clone(),
        Arc::new(ClaudeCode),
        None,
        dir.path().into(),
        StartOptions::default(),
        sink.clone(),
    )
    .await
    .err()
    .unwrap();
    assert!(
        missing
            .to_string()
            .contains("Claude Code was not found on deploy@shop.example.com"),
        "{missing}"
    );

    let exe = fake_cli(&dir);
    let found = host.detect(&ClaudeCode, Some(&exe.display().to_string()));
    let mut attempt = 0;
    let session = loop {
        match AgentSession::start_on(
            host.clone(),
            Arc::new(ClaudeCode),
            found.clone(),
            dir.path().into(),
            StartOptions::default(),
            sink.clone(),
        )
        .await
        {
            Ok(s) => break s,
            Err(e) if attempt < 5 && e.to_string().to_lowercase().contains("busy") => {
                attempt += 1;
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(e) => panic!("cannot start the fake CLI: {e}"),
        }
    };
    session.send("hello").await.unwrap();
    next_matching(&mut rx, |e| matches!(e, AgentEvent::TurnFinished { .. })).await;
    assert_eq!(
        host.spawned.lock().unwrap().clone(),
        vec![(exe, dir.path().to_path_buf())]
    );
    session.stop().await;
    next_matching(&mut rx, |e| matches!(e, AgentEvent::Exited { .. })).await;
}
