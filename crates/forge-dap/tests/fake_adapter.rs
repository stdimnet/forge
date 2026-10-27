//! End-to-end tests of the DAP client against the simulated adapter (`forge-dap-fake`), over
//! stdio, TCP connect, TCP listen and an in-process DAP server. No real debugger needed.

use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

use forge_dap::fake::DEMO_SOURCE;
use forge_dap::{DapManager, LaunchMode, LaunchRequest, ManifestAdapter};
use forge_debug::{
    BreakReason, BreakpointKind, BreakpointSpec, DebugEvent, DebugServer, EventSink, SessionInfo,
    SessionStatus, StepKind,
};
use forge_plugin_api::{DebugAdapterContribution, DebugAdapterTransport};
use serde_json::json;
use tokio::sync::mpsc;

const FAKE: &str = env!("CARGO_BIN_EXE_forge-dap-fake");

struct Harness {
    manager: Arc<DapManager>,
    server: Arc<DebugServer>,
    events: mpsc::UnboundedReceiver<DebugEvent>,
    root: tempfile::TempDir,
}

fn spec(r#type: &str, transport: DebugAdapterTransport, args: &[&str]) -> DebugAdapterContribution {
    DebugAdapterContribution {
        r#type: r#type.into(),
        label: "Fake C".into(),
        command: FAKE.into(),
        args: args.iter().map(|a| a.to_string()).collect(),
        transport,
        languages: vec!["c".into()],
        launch: json!({"program": "${program}", "args": "${args}", "cwd": "${cwd}", "stopOnEntry": "${stopOnEntry}"}),
        attach: json!({"pid": "${pid}"}),
        core_dump: json!({"program": "${program}", "coreFile": "${core}"}),
        core_dump_request: Some("attach".into()),
        ..Default::default()
    }
}

fn harness() -> Harness {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("demo.c"), DEMO_SOURCE).unwrap();
    let (tx, events) = mpsc::unbounded_channel();
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
    for (t, transport, args) in [
        ("fake", DebugAdapterTransport::Stdio, vec![]),
        (
            "fake-listen",
            DebugAdapterTransport::Listen,
            vec!["--connect", "${port}"],
        ),
        (
            "fake-connect",
            DebugAdapterTransport::Connect,
            vec!["--listen", "${port}"],
        ),
        (
            "fake-server",
            DebugAdapterTransport::Connect,
            vec!["--serve", "${port}"],
        ),
    ] {
        manager.register_adapter(Arc::new(ManifestAdapter::new(spec(t, transport, &args))));
    }
    Harness {
        manager,
        server,
        events,
        root,
    }
}

impl Harness {
    fn program(&self) -> String {
        self.root.path().join("demo").to_string_lossy().into_owned()
    }

    fn launch(&self, adapter: &str) -> LaunchRequest {
        let mut r = LaunchRequest::launch(adapter, &self.program());
        r.cwd = Some(self.root.path().to_string_lossy().into_owned());
        r
    }

    fn line_bp(&self, line: u32) -> BreakpointSpec {
        BreakpointSpec {
            file: Some("demo.c".into()),
            line: Some(line),
            ..Default::default()
        }
    }

    async fn next(&mut self, what: &str, mut pred: impl FnMut(&DebugEvent) -> bool) -> DebugEvent {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            let e = tokio::time::timeout_at(deadline, self.events.recv())
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
                .expect("event channel closed");
            if pred(&e) {
                return e;
            }
        }
    }

    async fn paused(&mut self, id: &str) -> SessionInfo {
        let e = self
            .next("a pause", |e| {
                matches!(e, DebugEvent::Session { session } if session.id == id && session.status == SessionStatus::Paused)
            })
            .await;
        let DebugEvent::Session { session } = e else {
            unreachable!()
        };
        session
    }

    async fn output(&mut self, id: &str, text: &str) -> String {
        let e = self
            .next(&format!("output '{text}'"), |e| {
                matches!(e, DebugEvent::Output { session_id, text: t, .. } if session_id == id && t.contains(text))
            })
            .await;
        let DebugEvent::Output { stream, .. } = e else {
            unreachable!()
        };
        stream
    }

    /// Forget queued events, so the next pause is one that follows a resume.
    fn drain(&mut self) {
        while self.events.try_recv().is_ok() {}
    }

    async fn ended(&mut self, id: &str) {
        self.next(
            "the end",
            |e| matches!(e, DebugEvent::Ended { session_id } if session_id == id),
        )
        .await;
    }
}

fn line(s: &SessionInfo) -> u32 {
    s.location.as_ref().map(|l| l.line).unwrap_or(0)
}

#[tokio::test]
async fn launch_breakpoint_stack_variables_and_stepping() {
    let mut h = harness();
    h.server.add_breakpoint(h.line_bp(17)).unwrap();
    let info = h.manager.start(h.launch("fake")).await.unwrap();
    assert_eq!(info.adapter.as_deref(), Some("fake"));
    let features = info.features.clone().unwrap();
    assert!(features.read_memory && features.disassemble && features.data_breakpoints);
    assert_eq!(features.exception_filters.len(), 2);
    let id = info.id.clone();
    let s = h.paused(&id).await;
    assert_eq!((line(&s), s.reason), (17, Some(BreakReason::Breakpoint)));
    assert_eq!(s.location.as_ref().unwrap().file.as_deref(), Some("demo.c"));

    let session = h.manager.session(&id).unwrap();
    let threads = session.threads().await.unwrap();
    assert_eq!(threads.len(), 2);
    let frames = session.stack(None).await.unwrap();
    assert_eq!(frames.len(), 1);
    assert_eq!((frames[0].r#where.as_str(), frames[0].line), ("main", 17));
    assert!(frames[0].address.is_some());
    let worker = session.stack(Some(2)).await.unwrap();
    assert_eq!(worker[0].r#where, "__futex_abstimed_wait_common");
    session.stack(Some(1)).await.unwrap();

    let vars = session.variables(0, 0).await.unwrap();
    let names: Vec<&str> = vars.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(
        names,
        ["counter", "name", "origin"],
        "registers are not in the tree"
    );
    let origin = &vars[2];
    assert_eq!(origin.classname.as_deref(), Some("struct point"));
    let children = session.children(origin.reference.unwrap()).await.unwrap();
    assert_eq!(children[0].fullname, "origin.x");
    assert_eq!(children[0].value.as_deref(), Some("3"));
    let contexts = session.contexts(0).await.unwrap();
    assert_eq!(contexts[1].name, "Registers");
    let regs = session.registers(0).await.unwrap();
    assert!(regs.iter().any(|r| r.name == "rip"));

    h.drain();

    session.continue_with(StepKind::StepInto).await.unwrap();
    let s = h.paused(&id).await;
    assert_eq!(line(&s), 6);
    assert_eq!(session.stack(None).await.unwrap().len(), 2);
    h.drain();
    session.continue_with(StepKind::StepOver).await.unwrap();
    assert_eq!(line(&h.paused(&id).await), 7);
    h.drain();
    session.continue_with(StepKind::StepOut).await.unwrap();
    assert_eq!(line(&h.paused(&id).await), 18);
    let counter = session.eval("counter", 0, "watch").await.unwrap();
    assert_eq!(counter.value.as_deref(), Some("6"));
    let err = session.eval("nope", 0, "repl").await.unwrap_err();
    assert!(err.to_string().contains("undeclared"), "{err}");
    let completions = session.complete("1 + cou", 0).await.unwrap();
    assert_eq!(completions[0].insert, "counter");

    h.drain();

    session.continue_with(StepKind::Run).await.unwrap();
    assert_eq!(h.output(&id, "forge 9").await, "stdout");
    h.output(&id, "exit code 0").await;
    h.ended(&id).await;
    assert!(h.manager.sessions().is_empty());
}

#[tokio::test]
async fn logpoints_conditions_and_hit_counts() {
    let mut h = harness();
    // A logpoint the adapter evaluates, a conditional breakpoint and a pass count.
    h.server
        .add_breakpoint(BreakpointSpec {
            suspend: Some(false),
            log_expression: Some("i".into()),
            ..h.line_bp(8)
        })
        .unwrap();
    h.server
        .add_breakpoint(BreakpointSpec {
            condition: Some("sum == 3".into()),
            ..h.line_bp(7)
        })
        .unwrap();
    let id = h.manager.start(h.launch("fake")).await.unwrap().id;
    assert_eq!(h.output(&id, "i = 0").await, "log");
    h.output(&id, "i = 2").await;
    let s = h.paused(&id).await;
    assert_eq!(line(&s), 7);
    let session = h.manager.session(&id).unwrap();
    assert_eq!(
        session
            .eval("sum", 0, "watch")
            .await
            .unwrap()
            .value
            .as_deref(),
        Some("3")
    );
    // A breakpoint that suspends and logs: Forge logs, the program stays paused. With a
    // pass count of 3 it suspends from the third hit on (after a restart).
    let mut bp = h
        .server
        .breakpoints()
        .breakpoints
        .into_iter()
        .find(|b| b.line == Some(7))
        .unwrap();
    bp.condition = None;
    bp.pass_count = Some(3);
    bp.log_message = true;
    h.server.update_breakpoint(bp).unwrap();
    session.sync_breakpoints().await;
    h.manager.restart(&id).await.unwrap();
    h.output(&id, "Breakpoint reached at demo.c:7").await;
    let s = h.paused(&id).await;
    assert_eq!(line(&s), 7);
    assert_eq!(
        session
            .eval("sum", 0, "watch")
            .await
            .unwrap()
            .value
            .as_deref(),
        Some("1"),
        "third hit: i = 2, sum = 0 + 1"
    );
    session.stop().await;
    h.ended(&id).await;
}

#[tokio::test]
async fn watchpoints_memory_disassembly_goto_and_set_value() {
    let mut h = harness();
    let mut req = h.launch("fake");
    req.stop_on_entry = true;
    let id = h.manager.start(req).await.unwrap().id;
    let s = h.paused(&id).await;
    assert_eq!((line(&s), s.reason), (14, Some(BreakReason::FirstLine)));
    let session = h.manager.session(&id).unwrap();
    session.stack(None).await.unwrap();
    let vars = session.variables(0, 0).await.unwrap();
    let counter = vars.iter().find(|v| v.name == "counter").unwrap().clone();
    let bp = session
        .add_data_breakpoint(counter.container.unwrap(), "counter", None)
        .await
        .unwrap();
    assert_eq!(bp.kind, BreakpointKind::Data);
    assert_eq!(bp.description.as_deref(), Some("counter"));
    session.sync_breakpoints().await;
    h.drain();
    session.continue_with(StepKind::Run).await.unwrap();
    let s = h.paused(&id).await;
    assert_eq!((line(&s), s.reason), (18, Some(BreakReason::Breakpoint)));

    let mem = counter.memory_reference.clone().unwrap();
    let block = session.read_memory(&mem, 0, 32, 16).await.unwrap();
    assert_eq!(block.rows.len(), 2);
    assert_eq!(block.rows[0].hex[0], "06", "counter = 6");
    assert_eq!(&block.rows[0].ascii[8..13], "forge");
    session.write_memory(&mem, 0, "07").await.unwrap();
    h.next("memory invalidation", |e| {
        matches!(e, DebugEvent::Invalidated { areas, .. } if areas.iter().any(|a| a == "memory"))
    })
    .await;
    let block = session.read_memory(&mem, 0, 4, 16).await.unwrap();
    assert_eq!(block.rows[0].hex, ["07", "00", "00", "00"]);
    let outside = session.read_memory("0x10", 0, 8, 8).await.unwrap();
    assert_eq!(outside.unreadable, 8);

    let frame = session.stack(None).await.unwrap()[0].clone();
    let ip = frame.address.clone().unwrap();
    let code = session.disassemble(&ip, -3, 8, Some(&ip)).await.unwrap();
    assert_eq!(code.len(), 8);
    let current: Vec<_> = code.iter().filter(|i| i.current).collect();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].line, Some(18));
    assert_eq!(current[0].file.as_deref(), Some("demo.c"));
    h.drain();
    session
        .continue_with(StepKind::StepOverInstruction)
        .await
        .unwrap();
    let s = h.paused(&id).await;
    assert_eq!(line(&s), 18, "still on the line, next instruction");
    let next_ip = session.stack(None).await.unwrap()[0]
        .address
        .clone()
        .unwrap();
    assert_ne!(next_ip, ip);

    h.drain();

    session.goto("demo.c", 20).await.unwrap();
    assert_eq!(line(&h.paused(&id).await), 20);
    session.stack(None).await.unwrap();
    let locals = session.variables(0, 0).await.unwrap();
    let counter = locals.iter().find(|v| v.name == "counter").unwrap();
    session
        .set_value(counter.container, Some("counter"), "counter", "42", 0)
        .await
        .unwrap();
    let again = session.eval("counter", 0, "watch").await.unwrap();
    assert_eq!(again.value.as_deref(), Some("42"));
    let modules = session.modules().await.unwrap();
    assert_eq!(modules.len(), 2);
    let sources = session.loaded_sources().await.unwrap();
    assert_eq!(sources[0].file.as_deref(), Some("demo.c"));
    session.stop().await;
    h.ended(&id).await;
}

#[tokio::test]
async fn function_and_exception_breakpoints() {
    let mut h = harness();
    h.server
        .add_breakpoint(BreakpointSpec {
            kind: BreakpointKind::Function,
            function: Some("compute".into()),
            ..Default::default()
        })
        .unwrap();
    let mut req = h.launch("fake");
    req.configuration = json!({ "crash": true });
    let id = h.manager.start(req).await.unwrap().id;
    let s = h.paused(&id).await;
    assert_eq!((line(&s), s.reason), (6, Some(BreakReason::Breakpoint)));
    // The adapter's exception filters are stored with their defaults.
    let filters: Vec<_> = h
        .server
        .breakpoints()
        .breakpoints
        .into_iter()
        .filter(|b| b.kind == BreakpointKind::Exception && b.adapter.as_deref() == Some("fake"))
        .map(|b| (b.exception.unwrap(), b.enabled))
        .collect();
    assert_eq!(
        filters,
        [
            ("signal".to_string(), true),
            ("cpp_throw".to_string(), false)
        ]
    );
    let session = h.manager.session(&id).unwrap();
    h.drain();
    session.continue_with(StepKind::Run).await.unwrap();
    let s = h.paused(&id).await;
    assert_eq!(s.reason, Some(BreakReason::Exception));
    let exc = s.exception.unwrap();
    assert_eq!(exc.name, "SIGSEGV");
    assert_eq!(exc.message.as_deref(), Some("Segmentation fault"));
    session.stop().await;
    h.ended(&id).await;
}

#[tokio::test]
async fn attach_to_a_dap_server() {
    let mut h = harness();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (r, w) = stream.into_split();
        forge_dap::fake::serve(r, w).await.unwrap();
    });
    h.server.add_breakpoint(h.line_bp(16)).unwrap();
    let mut req = h.launch("fake");
    req.mode = LaunchMode::AttachServer {
        host: "127.0.0.1".into(),
        port,
    };
    let info = h.manager.start(req).await.unwrap();
    assert!(info.features.as_ref().unwrap().attached);
    assert_eq!(line(&h.paused(&info.id).await), 16);
    // Stop detaches from an attached program.
    h.manager.session(&info.id).unwrap().stop().await;
    h.ended(&info.id).await;
    server.await.unwrap();
}

#[tokio::test]
async fn adapters_that_listen_or_connect_back() {
    let mut h = harness();
    for adapter in ["fake-listen", "fake-connect"] {
        let mut req = h.launch(adapter);
        req.stop_on_entry = true;
        let id = h.manager.start(req).await.unwrap().id;
        assert_eq!(line(&h.paused(&id).await), 14, "{adapter}");
        h.manager.session(&id).unwrap().stop().await;
        h.ended(&id).await;
    }
}

#[tokio::test]
async fn run_in_terminal_and_child_sessions() {
    let mut h = harness();
    let mut req = LaunchRequest::launch("fake", "echo");
    req.args = vec!["hello from the terminal".into()];
    req.configuration = json!({
        "console": "integratedTerminal",
        "spawnChild": true,
        "sourcePath": h.root.path().join("demo.c"),
    });
    req.stop_on_entry = true;
    let parent = h.manager.start(req).await.unwrap().id;
    // The terminal's output, the adapter's output and the child session arrive in any order.
    let (mut terminal, mut started, mut child) = (false, false, None);
    while !(terminal && started && child.is_some()) {
        match h
            .next("terminal output and the child session", |_| true)
            .await
        {
            DebugEvent::Output {
                session_id, text, ..
            } if session_id == parent => {
                terminal |= text.contains("hello from the terminal");
                started |= text.contains("Debuggee started in terminal");
            }
            DebugEvent::Session { session }
                if session.parent_id.as_deref() == Some(parent.as_str()) =>
            {
                child = Some(session);
            }
            _ => {}
        }
    }
    assert_eq!(child.unwrap().server_name, "demo (child)");
    assert_eq!(h.manager.sessions().len(), 2);
    h.manager.stop_all().await;
    let left: Vec<_> = h
        .manager
        .sessions()
        .into_iter()
        .map(|s| (s.id, s.status, s.parent_id))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

#[tokio::test]
async fn child_sessions_connect_to_the_parents_adapter_server() {
    let mut h = harness();
    let mut req = LaunchRequest::launch("fake-server", "demo");
    req.configuration = json!({
        "spawnChild": true,
        "sourcePath": h.root.path().join("demo.c"),
    });
    req.stop_on_entry = true;
    let parent = h.manager.start(req).await.unwrap().id;
    let child = h
        .next("the child session", |e| {
            matches!(e, DebugEvent::Session { session } if session.parent_id.as_deref() == Some(parent.as_str()))
        })
        .await;
    let DebugEvent::Session { session: child } = child else {
        unreachable!()
    };
    let parent_peer = h.manager.session(&parent).unwrap().info().peer;
    assert_eq!(child.peer, parent_peer, "the child uses the same server");
    assert!(child.peer.starts_with("127.0.0.1:"));
    h.manager.stop_all().await;
}

#[tokio::test]
async fn core_dumps_attach_processes_and_restart() {
    let mut h = harness();
    let mut req = h.launch("fake");
    req.mode = LaunchMode::CoreDump {
        core: "/tmp/core.4242".into(),
    };
    assert!(!req.runs_code());
    let id = h.manager.start(req).await.unwrap().id;
    let s = h.paused(&id).await;
    assert_eq!((line(&s), s.reason), (19, Some(BreakReason::Exception)));
    let session = h.manager.session(&id).unwrap();
    assert!(
        session.continue_with(StepKind::Run).await.is_err(),
        "a core dump does not run"
    );
    session.stop().await;
    h.ended(&id).await;

    h.server.add_breakpoint(h.line_bp(15)).unwrap();
    let mut req = h.launch("fake");
    req.mode = LaunchMode::AttachProcess { pid: 777 };
    let info = h.manager.start(req).await.unwrap();
    assert_eq!(info.server_name, "demo (777)");
    assert_eq!(line(&h.paused(&info.id).await), 15);
    let restarted = h.manager.restart(&info.id).await.unwrap();
    assert_eq!(restarted.id, info.id, "restarted in place");
    assert_eq!(line(&h.paused(&info.id).await), 15);
    assert_eq!(h.manager.session(&info.id).unwrap().process_id(), Some(777));
    h.manager.stop_all().await;
}

#[tokio::test]
async fn unknown_adapters_and_failing_launches() {
    let h = harness();
    let err = h.manager.start(h.launch("nope")).await.unwrap_err();
    assert!(err.to_string().contains("No debug adapter"), "{err}");
    let mut spec = spec("broken", DebugAdapterTransport::Stdio, &[]);
    spec.command = "forge-missing-adapter-binary".into();
    h.manager
        .register_adapter(Arc::new(ManifestAdapter::new(spec)));
    let err = h.manager.start(h.launch("broken")).await.unwrap_err();
    assert!(err.to_string().contains("not installed"), "{err}");
    assert!(h.manager.sessions().is_empty());
}

/// Smoke test with GDB's built-in DAP server (`gdb -i dap`, GDB 14+) and a C compiler.
/// Run with `cargo test -p forge-dap -- --ignored`.
#[tokio::test]
#[ignore]
async fn gdb_smoke_test() {
    let mut h = harness();
    let program = h.program();
    let compiled = std::process::Command::new("cc")
        .args(["-g", "-O0", "-o", &program])
        .arg(h.root.path().join("demo.c"))
        .status();
    if !matches!(compiled, Ok(s) if s.success()) {
        eprintln!("no C compiler: skipped");
        return;
    }
    h.manager.register_adapter(Arc::new(ManifestAdapter::new(DebugAdapterContribution {
        r#type: "gdb".into(),
        label: "GDB".into(),
        command: "gdb".into(),
        args: vec!["-i".into(), "dap".into()],
        launch: json!({"program": "${program}", "args": "${args}", "cwd": "${cwd}", "stopAtBeginningOfMainSubprogram": "${stopOnEntry}"}),
        ..Default::default()
    })));
    h.server.add_breakpoint(h.line_bp(17)).unwrap();
    let id = h.manager.start(h.launch("gdb")).await.unwrap().id;
    let s = h.paused(&id).await;
    assert_eq!(line(&s), 17);
    let session = h.manager.session(&id).unwrap();
    let frames = session.stack(None).await.unwrap();
    assert_eq!(frames[0].r#where, "main");
    let vars = session.variables(0, 0).await.unwrap();
    assert!(vars.iter().any(|v| v.name == "counter"), "{vars:?}");
    session.stop().await;
    h.ended(&id).await;
}
