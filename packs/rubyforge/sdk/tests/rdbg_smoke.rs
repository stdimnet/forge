//! Smoke tests with a real Ruby: detect this machine's interpreters, and debug a script
//! with rdbg (the `debug` gem) — run it under `rdbg --open`, attach, stop at a breakpoint
//! and read the variables. Ignored by default; run with
//! `cargo test -p rubyforge-sdk -- --ignored` (`FORGE_RDBG=/path/to/rdbg` or rdbg of the
//! detected Ruby).

use std::path::Path;
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

use forge_dap::{DapManager, LaunchMode, LaunchRequest};
use forge_debug::{BreakpointSpec, DebugEvent, DebugServer, EventSink, SessionStatus};
use rubyforge_lang_ruby::{rdbg_command, RubyCommand};
use rubyforge_sdk::detect::system_runner;
use rubyforge_sdk::{detect, RdbgAdapter, RubyEnv, ADAPTER};
use tokio::sync::mpsc;

const APP: &str = "items = [1250, 725]\nsum = 0\nitems.each do |i|\n  sum += i\nend\nputs sum\n";

#[test]
#[ignore]
fn detects_this_machines_ruby() {
    let s = detect(&RubyEnv::system(), "", None, &system_runner);
    if s.interpreters.is_empty() {
        eprintln!("no Ruby: skipped");
        return;
    }
    let c = s.chosen.expect("a chosen interpreter");
    assert!(!c.interpreter.version.is_empty(), "{c:?}");
    assert!(s.tools.contains_key("gem"), "{:?}", s.tools);
    eprintln!(
        "{} ({}), tools: {:?}",
        c.interpreter.name,
        c.reason,
        s.tools.keys()
    );
}

#[tokio::test]
#[ignore]
async fn rdbg_stops_at_a_breakpoint() {
    let status = detect(&RubyEnv::system(), "", None, &system_runner);
    let rdbg = std::env::var("FORGE_RDBG")
        .ok()
        .or_else(|| status.tools.get("rdbg").cloned());
    let (Some(rdbg), Some(chosen)) = (rdbg, status.chosen.clone()) else {
        eprintln!("no rdbg: skipped");
        return;
    };
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("app.rb"), APP).unwrap();
    let port = 41000 + (std::process::id() % 2000) as u16;
    let cmd = rdbg_command(
        &rdbg,
        &RubyCommand {
            program: chosen.interpreter.path.clone(),
            args: vec!["app.rb".into()],
            ..RubyCommand::default()
        },
    );
    let mut child = std::process::Command::new(&cmd.program)
        .args(
            cmd.args
                .iter()
                .map(|a| a.replace("${port}", &port.to_string())),
        )
        .current_dir(root.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    let (tx, mut events) = mpsc::unbounded_channel();
    let slot: Arc<OnceLock<Weak<DapManager>>> = Arc::new(OnceLock::new());
    let hook = Arc::clone(&slot);
    let sink: EventSink = Arc::new(move |e: DebugEvent| {
        if matches!(e, DebugEvent::Breakpoints { .. }) {
            if let Some(m) = hook.get().and_then(Weak::upgrade) {
                m.breakpoints_changed();
            }
        }
        let _ = tx.send(e);
    });
    let server = Arc::new(DebugServer::new(Arc::clone(&sink)));
    server.open_project(Some(root.path()));
    let manager = Arc::new(DapManager::new(sink, server.clone()));
    manager.set_project_root(Some(root.path().to_path_buf()));
    slot.set(Arc::downgrade(&manager)).ok();
    manager.register_adapter(Arc::new(RdbgAdapter::new(rdbg)));
    server
        .add_breakpoint(BreakpointSpec {
            file: Some("app.rb".into()),
            line: Some(4),
            ..Default::default()
        })
        .unwrap();

    let mut request = LaunchRequest::launch(ADAPTER, "");
    request.program = None;
    request.mode = LaunchMode::AttachServer {
        host: "127.0.0.1".into(),
        port,
    };
    // rdbg listens once Ruby has started.
    let mut info = None;
    for _ in 0..100 {
        match manager.start(request.clone()).await {
            Ok(i) => {
                info = Some(i);
                break;
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
    let id = info.expect("attached to rdbg").id;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    let paused = loop {
        let e = tokio::time::timeout_at(deadline, events.recv())
            .await
            .expect("timed out waiting for the breakpoint")
            .expect("events");
        if let DebugEvent::Session { session } = e {
            if session.id == id && session.status == SessionStatus::Paused {
                break session;
            }
        }
    };
    assert_eq!(paused.location.as_ref().map(|l| l.line), Some(4));
    let session = manager.session(&id).unwrap();
    let vars = session.variables(0, 0).await.unwrap();
    assert!(vars.iter().any(|v| v.name == "sum"), "{vars:?}");
    session.stop().await;
    let _ = child.kill();
    let _ = child.wait();
    assert!(Path::new(&root.path().join("app.rb")).is_file());
}
