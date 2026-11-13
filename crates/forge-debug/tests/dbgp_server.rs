//! End-to-end tests of the DBGp server against a scripted fake Xdebug engine.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use forge_debug::protocol::{b64_encode, frame_packet, parse_command, Command};
use forge_debug::{
    BreakReason, BreakpointKind, BreakpointSpec, DebugConfig, DebugEvent, DebugServer, PathMapping,
    ServerConfig, SessionStatus, StepKind,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

const WAIT: Duration = Duration::from_secs(5);

/// A fake Xdebug: connects, sends `init`, then answers commands as the test tells it.
struct Engine {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

impl Engine {
    async fn connect(port: u16, file_uri: &str) -> Self {
        let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let (r, w) = stream.into_split();
        let mut engine = Self {
            reader: BufReader::new(r),
            writer: w,
        };
        engine
            .send(&format!(
                r#"<?xml version="1.0" encoding="iso-8859-1"?>
<init xmlns="urn:debugger_protocol_v1" xmlns:xdebug="https://xdebug.org/dbgp/xdebug" fileuri="{file_uri}" language="PHP" xdebug:language_version="8.3.4" protocol_version="1.0" appid="4711" idekey="FORGE"><engine version="3.3.2"><![CDATA[Xdebug]]></engine></init>"#
            ))
            .await;
        engine
    }

    async fn send(&mut self, xml: &str) {
        self.writer.write_all(&frame_packet(xml)).await.unwrap();
        self.writer.flush().await.unwrap();
    }

    async fn command(&mut self) -> Command {
        let mut buf = Vec::new();
        let n = tokio::time::timeout(WAIT, self.reader.read_until(0, &mut buf))
            .await
            .expect("timed out waiting for a command")
            .unwrap();
        assert!(n > 0, "IDE closed the connection");
        buf.pop();
        parse_command(std::str::from_utf8(&buf).unwrap()).unwrap()
    }

    /// Wait for the next command, failing on any other.
    async fn expect(&mut self, name: &str) -> Command {
        let cmd = self.command().await;
        assert_eq!(cmd.name, name, "unexpected command {cmd:?}");
        cmd
    }

    async fn reply(&mut self, cmd: &Command, attrs: &str, body: &str) {
        let xml = format!(
            r#"<?xml version="1.0" encoding="iso-8859-1"?>
<response xmlns="urn:debugger_protocol_v1" xmlns:xdebug="https://xdebug.org/dbgp/xdebug" command="{}" transaction_id="{}" {attrs}>{body}</response>"#,
            cmd.name, cmd.tid
        );
        self.send(&xml).await;
    }

    async fn reply_break(&mut self, cmd: &Command, file_uri: &str, line: u32) {
        self.reply(
            cmd,
            r#"status="break" reason="ok""#,
            &format!(r#"<xdebug:message filename="{file_uri}" lineno="{line}"></xdebug:message>"#),
        )
        .await;
    }

    /// Answer the server lookup with the given `$_SERVER` values (missing = error 300).
    async fn answer_identity(&mut self, vars: &[(&str, &str)]) {
        for expected in ["PHP_IDE_CONFIG", "SERVER_NAME"] {
            let cmd = self.expect("property_get").await;
            assert_eq!(cmd.arg('c'), Some("1"));
            assert!(cmd.arg('n').unwrap().contains(expected));
            self.answer_server_var(&cmd, vars, expected).await;
            if expected == "SERVER_NAME" && vars.iter().any(|(k, _)| *k == "SERVER_NAME") {
                let port = self.expect("property_get").await;
                self.answer_server_var(&port, vars, "SERVER_PORT").await;
            }
        }
    }

    async fn answer_server_var(&mut self, cmd: &Command, vars: &[(&str, &str)], key: &str) {
        match vars.iter().find(|(k, _)| *k == key) {
            Some((_, v)) => {
                let body = format!(
                    r#"<property name="{key}" type="string" size="{}" encoding="base64"><![CDATA[{}]]></property>"#,
                    v.len(),
                    b64_encode(v)
                );
                self.reply(cmd, "", &body).await;
            }
            None => {
                self.reply(
                    cmd,
                    "",
                    r#"<error code="300"><message><![CDATA[can not get property]]></message></error>"#,
                )
                .await
            }
        }
    }

    /// Answer feature negotiation and `stdout`, collecting breakpoint_set commands, until
    /// the first continuation command arrives.
    async fn setup(&mut self) -> (Command, Vec<Command>) {
        let mut breakpoints = Vec::new();
        let mut next_id = 1;
        loop {
            let cmd = self.command().await;
            match cmd.name.as_str() {
                "feature_set" | "stdout" => self.reply(&cmd, r#"success="1""#, "").await,
                "breakpoint_set" => {
                    self.reply(&cmd, &format!(r#"state="enabled" id="{next_id}""#), "")
                        .await;
                    next_id += 1;
                    breakpoints.push(cmd);
                }
                "run" | "step_into" => return (cmd, breakpoints),
                other => panic!("unexpected command during setup: {other}"),
            }
        }
    }
}

struct Ide {
    server: Arc<DebugServer>,
    events: mpsc::UnboundedReceiver<DebugEvent>,
    port: u16,
}

impl Ide {
    async fn start(root: &Path, config: DebugConfig) -> Self {
        let (tx, events) = mpsc::unbounded_channel();
        let server = Arc::new(DebugServer::new(Arc::new(move |e| {
            let _ = tx.send(e);
        })));
        server.set_config(config);
        server.open_project(Some(root));
        let port = server.listen(0).await.unwrap();
        let mut ide = Self {
            server,
            events,
            port,
        };
        ide.event(|e| {
            matches!(
                e,
                DebugEvent::Listening {
                    listening: true,
                    ..
                }
            )
        })
        .await;
        ide
    }

    async fn event(&mut self, pred: impl Fn(&DebugEvent) -> bool) -> DebugEvent {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let e = tokio::time::timeout_at(deadline, self.events.recv())
                .await
                .expect("timed out waiting for an event")
                .expect("event channel closed");
            if pred(&e) {
                return e;
            }
        }
    }

    async fn paused(&mut self) -> forge_debug::SessionInfo {
        match self
            .event(|e| matches!(e, DebugEvent::Session { session } if session.status == SessionStatus::Paused))
            .await
        {
            DebugEvent::Session { session } => session,
            _ => unreachable!(),
        }
    }
}

fn uri(root: &Path, rel: &str) -> String {
    format!("file://{}/{rel}", root.display())
}

fn line_bp(file: &str, line: u32, condition: Option<&str>) -> BreakpointSpec {
    BreakpointSpec {
        file: Some(file.into()),
        line: Some(line),
        condition: condition.map(str::to_string),
        ..Default::default()
    }
}

#[tokio::test]
async fn local_session_breakpoints_inspection_and_end() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let mut ide = Ide::start(&root, DebugConfig::default()).await;
    ide.server
        .add_breakpoint(line_bp(
            "app/Http/Controllers/OrderController.php",
            24,
            Some("$total > 100"),
        ))
        .unwrap();
    ide.server
        .add_breakpoint(BreakpointSpec {
            kind: BreakpointKind::Exception,
            exception: Some("RuntimeException".into()),
            ..Default::default()
        })
        .unwrap();
    assert!(root.join(".forge/breakpoints.json").exists());

    let mut engine = Engine::connect(ide.port, &uri(&root, "public/index.php")).await;
    engine.answer_identity(&[]).await;
    let (run, bps) = engine.setup().await;
    assert_eq!(run.name, "run");
    assert_eq!(bps.len(), 2);
    assert_eq!(bps[0].arg('t'), Some("conditional"));
    assert_eq!(
        bps[0].arg('f').map(str::to_string),
        Some(uri(&root, "app/Http/Controllers/OrderController.php"))
    );
    assert_eq!(bps[0].arg('n'), Some("24"));
    assert_eq!(bps[0].data.as_deref(), Some("$total > 100"));
    assert_eq!(bps[1].arg('t'), Some("exception"));
    assert_eq!(bps[1].arg('x'), Some("RuntimeException"));

    let started = ide.event(|e| matches!(e, DebugEvent::Session { .. })).await;
    let DebugEvent::Session { session } = started else {
        unreachable!()
    };
    assert_eq!(session.server_name, "CLI");
    assert_eq!(session.file.as_deref(), Some("public/index.php"));

    // Script output is forwarded while running.
    engine
        .send(&format!(
            r#"<stream xmlns="urn:debugger_protocol_v1" type="stdout" encoding="base64"><![CDATA[{}]]></stream>"#,
            b64_encode("Hello\n")
        ))
        .await;
    let out = ide.event(|e| matches!(e, DebugEvent::Output { .. })).await;
    assert!(matches!(out, DebugEvent::Output { ref text, .. } if text == "Hello\n"));

    engine
        .reply_break(
            &run,
            &uri(&root, "app/Http/Controllers/OrderController.php"),
            24,
        )
        .await;
    let paused = ide.paused().await;
    let loc = paused.location.unwrap();
    assert_eq!(
        loc.file.as_deref(),
        Some("app/Http/Controllers/OrderController.php")
    );
    assert_eq!(loc.line, 24);
    assert_eq!(paused.reason, Some(BreakReason::Breakpoint));

    let session = ide.server.session(&paused.id).unwrap();

    // Stack with mapped and unmapped frames.
    let s = Arc::clone(&session);
    let stack = tokio::spawn(async move { s.stack().await });
    let cmd = engine.expect("stack_get").await;
    engine
        .reply(&cmd, "", &format!(
            r#"<stack where="App\Http\Controllers\OrderController-&gt;show" level="0" type="file" filename="{}" lineno="24"></stack><stack where="{{main}}" level="1" type="file" filename="file:///usr/share/php/boot.php" lineno="3"></stack>"#,
            uri(&root, "app/Http/Controllers/OrderController.php")
        ))
        .await;
    let stack = stack.await.unwrap().unwrap();
    assert_eq!(stack.len(), 2);
    assert_eq!(
        stack[0].r#where,
        "App\\Http\\Controllers\\OrderController->show"
    );
    assert_eq!(
        stack[0].file.as_deref(),
        Some("app/Http/Controllers/OrderController.php")
    );
    assert_eq!(stack[1].file, None);

    // Locals.
    let s = Arc::clone(&session);
    let vars = tokio::spawn(async move { s.variables(0, 0).await });
    let cmd = engine.expect("context_get").await;
    assert_eq!(cmd.arg('d'), Some("0"));
    assert_eq!(cmd.arg('c'), Some("0"));
    engine
        .reply(&cmd, r#"context="0""#, r#"<property name="$order" fullname="$order" type="object" classname="App\Models\Order" children="1" numchildren="2" page="0" pagesize="100"></property><property name="$total" fullname="$total" type="float"><![CDATA[149.9]]></property>"#)
        .await;
    let vars = vars.await.unwrap().unwrap();
    assert_eq!(vars[0].classname.as_deref(), Some("App\\Models\\Order"));
    assert_eq!(vars[1].value.as_deref(), Some("149.9"));

    // Expanding a page of children.
    let s = Arc::clone(&session);
    let prop = tokio::spawn(async move { s.property("$order->lines", 0, 0, 1).await });
    let cmd = engine.expect("property_get").await;
    assert_eq!(cmd.arg('n'), Some("$order->lines"));
    assert_eq!(cmd.arg('p'), Some("1"));
    engine
        .reply(&cmd, "", r#"<property name="lines" fullname="$order-&gt;lines" type="array" children="1" numchildren="120" page="1" pagesize="100"><property name="100" fullname="$order-&gt;lines[100]" type="int"><![CDATA[7]]></property></property>"#)
        .await;
    let prop = prop.await.unwrap().unwrap();
    assert_eq!(prop.num_children, 120);
    assert_eq!(prop.page, 1);
    assert_eq!(prop.children[0].fullname, "$order->lines[100]");

    // Evaluate: expression travels base64 encoded, errors come back as engine errors.
    let s = Arc::clone(&session);
    let eval = tokio::spawn(async move { s.eval("strtoupper($order->status)", 0).await });
    let cmd = engine.expect("eval").await;
    assert_eq!(cmd.data.as_deref(), Some("strtoupper($order->status)"));
    engine
        .reply(
            &cmd,
            "",
            &format!(
                r#"<property type="string" size="4" encoding="base64"><![CDATA[{}]]></property>"#,
                b64_encode("PAID")
            ),
        )
        .await;
    let result = eval.await.unwrap().unwrap();
    assert_eq!(result.value.as_deref(), Some("PAID"));
    assert_eq!(result.fullname, "strtoupper($order->status)");

    let s = Arc::clone(&session);
    let eval = tokio::spawn(async move { s.eval("nope(", 0).await });
    let cmd = engine.expect("eval").await;
    engine
        .reply(
            &cmd,
            "",
            r#"<error code="206"><message><![CDATA[error evaluating code]]></message></error>"#,
        )
        .await;
    assert!(eval.await.unwrap().is_err());

    // A breakpoint added while suspended reaches the engine; removing it too.
    let added = ide
        .server
        .add_breakpoint(line_bp("app/Models/Order.php", 10, None))
        .unwrap();
    let cmd = engine.expect("breakpoint_set").await;
    assert_eq!(cmd.arg('t'), Some("line"));
    engine.reply(&cmd, r#"id="77""#, "").await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    ide.server.remove_breakpoint(added.id).unwrap();
    let cmd = engine.expect("breakpoint_remove").await;
    assert_eq!(cmd.arg('d'), Some("77"));
    engine.reply(&cmd, "", "").await;

    // Step over to the end of the script.
    session.continue_with(StepKind::StepOver).unwrap();
    let step = engine.expect("step_over").await;
    engine
        .reply(&step, r#"status="stopping" reason="ok""#, "")
        .await;
    let stop = engine.expect("stop").await;
    engine
        .reply(&stop, r#"status="stopped" reason="ok""#, "")
        .await;
    ide.event(|e| matches!(e, DebugEvent::Ended { .. })).await;
    assert!(ide.server.sessions().is_empty());
}

#[tokio::test]
async fn unknown_server_is_accepted_with_path_mappings() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let mut ide = Ide::start(&root, DebugConfig::default()).await;
    ide.server
        .add_breakpoint(line_bp(
            "app/Http/Controllers/OrderController.php",
            24,
            None,
        ))
        .unwrap();

    let mut engine = Engine::connect(ide.port, "file:///var/www/html/public/index.php").await;
    engine
        .answer_identity(&[("SERVER_NAME", "shop.test"), ("SERVER_PORT", "8080")])
        .await;
    let incoming = ide
        .event(|e| matches!(e, DebugEvent::Incoming { .. }))
        .await;
    let DebugEvent::Incoming { session } = incoming else {
        unreachable!()
    };
    assert_eq!(session.status, SessionStatus::AwaitingAcceptance);
    assert_eq!(session.server_name, "shop.test:8080");
    assert_eq!(session.file, None);

    let server = ServerConfig {
        id: "shop".into(),
        name: "shop.test".into(),
        host: "shop.test".into(),
        port: 8080,
        debugger: "Xdebug".into(),
        use_path_mappings: true,
        mappings: vec![PathMapping {
            local: String::new(),
            remote: "/var/www/html".into(),
        }],
    };
    let srv = Arc::clone(&ide.server);
    let id = session.id.clone();
    let accept = tokio::spawn(async move { srv.accept_connection(&id, server).await });
    let (run, bps) = engine.setup().await;
    accept.await.unwrap().unwrap();
    assert_eq!(run.name, "run");
    assert_eq!(
        bps[0].arg('f'),
        Some("file:///var/www/html/app/Http/Controllers/OrderController.php")
    );
    assert_eq!(ide.server.servers().len(), 1);
    assert!(root.join(".forge/php-servers.json").exists());

    engine
        .reply_break(
            &run,
            "file:///var/www/html/app/Http/Controllers/OrderController.php",
            24,
        )
        .await;
    let paused = ide.paused().await;
    assert_eq!(
        paused.location.unwrap().file.as_deref(),
        Some("app/Http/Controllers/OrderController.php")
    );
    assert_eq!(paused.server_id.as_deref(), Some("shop"));

    // A second connection from the same server is recognized without asking.
    let mut second = Engine::connect(ide.port, "file:///var/www/html/public/index.php").await;
    second
        .answer_identity(&[("SERVER_NAME", "shop.test"), ("SERVER_PORT", "8080")])
        .await;
    let (run2, _) = second.setup().await;
    assert_eq!(run2.name, "run");
    assert_eq!(ide.server.sessions().len(), 2);
}

#[tokio::test]
async fn ignored_connection_is_detached() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let mut ide = Ide::start(&root, DebugConfig::default()).await;
    let mut engine = Engine::connect(ide.port, "file:///srv/app/index.php").await;
    engine.answer_identity(&[]).await;
    let DebugEvent::Incoming { session } = ide
        .event(|e| matches!(e, DebugEvent::Incoming { .. }))
        .await
    else {
        unreachable!()
    };
    let srv = Arc::clone(&ide.server);
    let ignore = tokio::spawn(async move { srv.ignore_connection(&session.id).await });
    let detach = engine.expect("detach").await;
    engine.reply(&detach, r#"status="stopping""#, "").await;
    ignore.await.unwrap().unwrap();
    ide.event(|e| matches!(e, DebugEvent::Ended { .. })).await;
}

#[tokio::test]
async fn first_line_break_and_unmapped_files_are_stepped_out() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let config = DebugConfig {
        break_at_first_line: true,
        ..DebugConfig::default()
    };
    let mut ide = Ide::start(&root, config).await;
    let mut engine = Engine::connect(ide.port, &uri(&root, "public/index.php")).await;
    engine.answer_identity(&[]).await;
    let (first, _) = engine.setup().await;
    assert_eq!(first.name, "step_into");
    engine
        .reply_break(&first, &uri(&root, "public/index.php"), 3)
        .await;
    let paused = ide.paused().await;
    assert_eq!(paused.reason, Some(BreakReason::FirstLine));

    let session = ide.server.session(&paused.id).unwrap();
    session.continue_with(StepKind::StepInto).unwrap();
    let step = engine.expect("step_into").await;
    engine
        .reply_break(&step, "file:///usr/share/php/Symfony/Polyfill.php", 12)
        .await;
    // Not in the project: the server steps out on its own.
    let out = engine.expect("step_out").await;
    engine
        .reply_break(&out, &uri(&root, "public/index.php"), 4)
        .await;
    let paused = ide.paused().await;
    assert_eq!(paused.reason, Some(BreakReason::Step));
    assert_eq!(paused.location.unwrap().line, 4);

    // Run to cursor sets a temporary breakpoint.
    let s = Arc::clone(&session);
    let run_to = tokio::spawn(async move { s.run_to("app/Models/User.php", 20).await });
    let bp = engine.expect("breakpoint_set").await;
    assert_eq!(bp.arg('r'), Some("1"));
    assert_eq!(bp.arg('n'), Some("20"));
    engine.reply(&bp, r#"id="9""#, "").await;
    run_to.await.unwrap().unwrap();
    let run = engine.expect("run").await;
    engine
        .reply_break(&run, &uri(&root, "app/Models/User.php"), 20)
        .await;
    let paused = ide.paused().await;
    assert_eq!(paused.location.unwrap().line, 20);

    // Stop while suspended, then the engine hangs up.
    let s = Arc::clone(&session);
    let stop = tokio::spawn(async move { s.stop().await });
    let cmd = engine.expect("stop").await;
    engine.reply(&cmd, r#"status="stopped""#, "").await;
    stop.await.unwrap();
    ide.event(|e| matches!(e, DebugEvent::Ended { .. })).await;
}

#[tokio::test]
async fn muting_removes_breakpoints_from_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let mut ide = Ide::start(&root, DebugConfig::default()).await;
    ide.server
        .add_breakpoint(line_bp("a.php", 2, None))
        .unwrap();
    ide.server.set_muted(true).unwrap();
    let mut engine = Engine::connect(ide.port, &uri(&root, "a.php")).await;
    engine.answer_identity(&[]).await;
    let (run, bps) = engine.setup().await;
    assert!(bps.is_empty(), "muted breakpoints are not sent");
    engine.reply_break(&run, &uri(&root, "a.php"), 1).await;
    ide.paused().await;
    ide.server.set_muted(false).unwrap();
    let cmd = engine.expect("breakpoint_set").await;
    assert_eq!(cmd.arg('n'), Some("2"));
}

#[tokio::test]
async fn busy_port_reports_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let ide = Ide::start(dir.path(), DebugConfig::default()).await;
    let other = DebugServer::new(Arc::new(|_| {}));
    let err = other.listen(ide.port).await.unwrap_err();
    assert!(err.to_string().contains("busy"), "{err}");
    ide.server.stop_listening();
    assert_eq!(ide.server.listening_port(), None);
}

#[tokio::test]
async fn cli_scripts_use_the_server_whose_mappings_cover_them() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let mut ide = Ide::start(&root, DebugConfig::default()).await;
    ide.server
        .save_servers(vec![ServerConfig {
            id: "docker-cli".into(),
            name: "docker-cli".into(),
            host: "localhost".into(),
            port: 80,
            debugger: "Xdebug".into(),
            use_path_mappings: true,
            mappings: vec![PathMapping {
                local: String::new(),
                remote: "/app".into(),
            }],
        }])
        .unwrap();
    let mut engine = Engine::connect(ide.port, "file:///app/bin/console").await;
    engine.answer_identity(&[]).await;
    let (run, _) = engine.setup().await;
    engine
        .reply_break(&run, "file:///app/src/Kernel.php", 9)
        .await;
    let paused = ide.paused().await;
    assert_eq!(paused.server_id.as_deref(), Some("docker-cli"));
    assert_eq!(
        paused.location.unwrap().file.as_deref(),
        Some("src/Kernel.php")
    );
}

#[tokio::test]
async fn interpreter_servers_map_runs_named_by_php_ide_config() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let mut ide = Ide::start(&root, DebugConfig::default()).await;
    ide.server.set_interpreter_servers(vec![ServerConfig {
        id: "interpreter:compose".into(),
        name: "compose-app".into(),
        host: String::new(),
        port: 80,
        debugger: "Xdebug".into(),
        use_path_mappings: true,
        mappings: vec![PathMapping {
            local: String::new(),
            remote: "/var/www/html".into(),
        }],
    }]);
    // Not stored in the project.
    assert!(ide.server.servers().is_empty());
    let mut engine = Engine::connect(ide.port, "file:///var/www/html/artisan").await;
    engine
        .answer_identity(&[("PHP_IDE_CONFIG", "serverName=compose-app")])
        .await;
    let (run, _) = engine.setup().await;
    engine
        .reply_break(&run, "file:///var/www/html/app/Models/User.php", 3)
        .await;
    let paused = ide.paused().await;
    assert_eq!(paused.server_id.as_deref(), Some("interpreter:compose"));
    assert_eq!(
        paused.location.unwrap().file.as_deref(),
        Some("app/Models/User.php")
    );
}

/// A project on a remote host: its breakpoints live there (through the project files), the
/// listener is reached through a forwarded port and the host's paths are the project's.
#[tokio::test]
async fn remote_project_listens_through_a_forwarded_port() {
    let files = Arc::new(forge_core::MemoryFiles::new());
    let (tx, mut events) = mpsc::unbounded_channel();
    let server = Arc::new(DebugServer::new(Arc::new(move |e| {
        let _ = tx.send(e);
    })));
    let root = Path::new("/var/www/shop");
    server.open_project_files(Some((root.to_path_buf(), files.clone())));
    server
        .add_breakpoint(line_bp(
            "app/Http/Controllers/OrderController.php",
            12,
            None,
        ))
        .unwrap();
    let stored = files.get(".forge/breakpoints.json").unwrap();
    assert!(stored.contains("OrderController.php"), "{stored}");

    // The "tunnel" reports the remote port; the IDE listens on a local one.
    let (shown, local) = server
        .listen_with(0, |bound| async move { Ok((9003, bound)) })
        .await
        .unwrap();
    assert_eq!(shown, 9003);
    assert_eq!(server.listening_port(), Some(9003));
    let listening = loop {
        let e = tokio::time::timeout(WAIT, events.recv())
            .await
            .unwrap()
            .unwrap();
        if matches!(e, DebugEvent::Listening { .. }) {
            break e;
        }
    };
    assert!(matches!(
        listening,
        DebugEvent::Listening {
            listening: true,
            port: 9003,
            ..
        }
    ));

    // Xdebug on the host reports host paths: they are the project's paths.
    let mut engine = Engine::connect(local, &uri(root, "public/index.php")).await;
    engine.answer_identity(&[]).await;
    let (_, bps) = engine.setup().await;
    assert_eq!(
        bps[0].arg('f').map(str::to_string),
        Some(uri(root, "app/Http/Controllers/OrderController.php"))
    );

    // A failing tunnel stops the listener and reports why.
    let err = server
        .listen_with(0, |_| async {
            Err::<(u16, ()), _>(forge_debug::DebugError::Other(
                "port 9003 is busy on the host".into(),
            ))
        })
        .await
        .unwrap_err();
    assert!(err.to_string().contains("busy on the host"));
    assert_eq!(server.listening_port(), None);
}

#[tokio::test]
async fn logpoints_temporary_breakpoints_and_pass_counts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let mut ide = Ide::start(&root, DebugConfig::default()).await;
    let logpoint = ide
        .server
        .add_breakpoint(BreakpointSpec {
            suspend: Some(false),
            log_message: true,
            log_expression: Some("$order->status".into()),
            ..line_bp("app/Order.php", 5, None)
        })
        .unwrap();
    let temporary = ide
        .server
        .add_breakpoint(BreakpointSpec {
            temporary: true,
            ..line_bp("app/Order.php", 8, None)
        })
        .unwrap();
    ide.server
        .add_breakpoint(BreakpointSpec {
            pass_count: Some(3),
            ..line_bp("app/Order.php", 10, None)
        })
        .unwrap();

    let mut engine = Engine::connect(ide.port, &uri(&root, "index.php")).await;
    engine.answer_identity(&[]).await;
    let (run, bps) = engine.setup().await;
    assert_eq!(bps.len(), 3);
    assert_eq!(bps[0].arg('t'), Some("line"));
    assert_eq!(bps[0].arg('r'), None);
    assert_eq!(bps[1].arg('r'), Some("1"), "temporary");
    assert_eq!(bps[2].arg('h'), Some("3"), "pass count");
    assert_eq!(bps[2].arg('o'), Some(">="));

    // The logpoint: evaluate, log, and run on without suspending.
    engine
        .reply_break(&run, &uri(&root, "app/Order.php"), 5)
        .await;
    let eval = engine.expect("eval").await;
    assert_eq!(eval.data.as_deref(), Some("$order->status"));
    engine
        .reply(
            &eval,
            "",
            &format!(
                r#"<property type="string" size="4" encoding="base64"><![CDATA[{}]]></property>"#,
                b64_encode("PAID")
            ),
        )
        .await;
    let resumed = engine.expect("run").await;
    let DebugEvent::Output { stream, text, .. } = ide
        .event(|e| matches!(e, DebugEvent::Output { stream, .. } if stream == "log"))
        .await
    else {
        unreachable!()
    };
    assert_eq!(stream, "log");
    assert_eq!(text, "Breakpoint reached at app/Order.php:5\n\"PAID\"\n");
    assert!(ide
        .server
        .breakpoints()
        .breakpoints
        .iter()
        .any(|b| b.id == logpoint.id));

    // The temporary breakpoint suspends once and is removed.
    engine
        .reply_break(&resumed, &uri(&root, "app/Order.php"), 8)
        .await;
    let removed = ide
        .event(|e| matches!(e, DebugEvent::Breakpoints { snapshot } if snapshot.breakpoints.len() == 2))
        .await;
    let DebugEvent::Breakpoints { snapshot } = removed else {
        unreachable!()
    };
    assert!(snapshot.breakpoints.iter().all(|b| b.id != temporary.id));
    let paused = ide.paused().await;
    assert_eq!(paused.reason, Some(BreakReason::Breakpoint));
    let stored = std::fs::read_to_string(root.join(".forge/breakpoints.json")).unwrap();
    assert!(!stored.contains("\"line\": 8"), "{stored}");

    // Console completion: variables after `$`, members after `->`.
    let session = ide.server.session(&paused.id).unwrap();
    let s = Arc::clone(&session);
    let vars = tokio::spawn(async move { s.complete("count($or", 0).await });
    let locals = engine.expect("context_get").await;
    assert_eq!(locals.arg('c'), Some("0"));
    engine
        .reply(&locals, "", r#"<property name="$order" fullname="$order" type="object" classname="App\Order" children="1" numchildren="1"></property><property name="$total" fullname="$total" type="int"><![CDATA[3]]></property>"#)
        .await;
    let globals = engine.expect("context_get").await;
    assert_eq!(globals.arg('c'), Some("1"));
    engine
        .reply(&globals, "", r#"<property name="$_ORIGIN" fullname="$_ORIGIN" type="array" children="0" numchildren="0"></property>"#)
        .await;
    let vars = vars.await.unwrap().unwrap();
    assert_eq!(vars.len(), 1);
    assert_eq!(vars[0].label, "$order");
    assert_eq!(vars[0].detail, "App\\Order");

    let s = Arc::clone(&session);
    let members = tokio::spawn(async move { s.complete("$order->st", 0).await });
    let get = engine.expect("property_get").await;
    assert_eq!(get.arg('n'), Some("$order"));
    engine
        .reply(&get, "", r#"<property name="$order" fullname="$order" type="object" classname="App\Order" children="1" numchildren="2"><property name="status" fullname="$order-&gt;status" facet="public" type="string"><![CDATA[PAID]]></property><property name="id" fullname="$order-&gt;id" facet="public" type="int"><![CDATA[1]]></property></property>"#)
        .await;
    let members = members.await.unwrap().unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].insert, "status");
}
