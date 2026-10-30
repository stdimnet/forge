//! Shared terminals and runs (S5) end to end: a host with a fake host application (terminals
//! and runs recorded in memory) and guests over the in-memory network. The host enforces
//! Terminal and Run access whatever the guests send, and the session log records what guests
//! did without the keys they typed.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_backend::{Backend, LocalBackend};
use forge_collab::audit::SessionLog;
use forge_collab::excludes::Excludes;
use forge_collab::guest::{GuestEvent, GuestSession};
use forge_collab::guest_backend::calls::HostCall;
use forge_collab::host::{
    DevAutoAccept, HostConfig, HostServices, HostSession, LspRequest, DEV_PREVIEW_NOTICE,
};
use forge_collab::permissions::{Permissions, RunAccess, TerminalAccess};
use forge_collab::share::{RunId, SharedRun, SharedTerminal, TerminalId};
use forge_collab::transport::memory::MemoryNetwork;
use forge_process::PtyEvent;
use futures_util::future::BoxFuture;

const READ: Permissions = Permissions::READ_ONLY;
const EDIT: Permissions = Permissions::EDIT_FILES;
const FULL: Permissions = Permissions::FULL_ACCESS;

/// The host application: what the session asked it to do.
#[derive(Default)]
struct FakeApp {
    writes: Mutex<Vec<(String, String)>>,
    opened: Mutex<Vec<(String, String, u16, u16)>>,
    closed: Mutex<Vec<String>>,
    started: Mutex<Vec<(String, String)>>,
    stopped: Mutex<Vec<String>>,
    console: Mutex<Vec<(String, String)>>,
}

impl HostServices for FakeApp {
    fn lsp(&self, _req: LspRequest) -> BoxFuture<'static, Result<Vec<u8>, String>> {
        Box::pin(async { Err("no language servers".to_string()) })
    }

    fn terminal_open(
        &self,
        key: String,
        title: String,
        cols: u16,
        rows: u16,
    ) -> BoxFuture<'static, Result<(), String>> {
        self.opened.lock().unwrap().push((key, title, cols, rows));
        Box::pin(async { Ok(()) })
    }

    fn terminal_write(&self, key: &str, data: &str) -> Result<(), String> {
        self.writes
            .lock()
            .unwrap()
            .push((key.to_string(), data.to_string()));
        Ok(())
    }

    fn terminal_close(&self, key: &str) {
        self.closed.lock().unwrap().push(key.to_string());
    }

    fn run_start(
        &self,
        configuration: String,
        guest: String,
    ) -> BoxFuture<'static, Result<(), String>> {
        if configuration == "Missing" {
            return Box::pin(async { Err("No run configuration named 'Missing'".to_string()) });
        }
        self.started.lock().unwrap().push((configuration, guest));
        Box::pin(async { Ok(()) })
    }

    fn run_stop(&self, key: &str) -> Result<(), String> {
        self.stopped.lock().unwrap().push(key.to_string());
        Ok(())
    }

    fn run_write(&self, key: &str, data: &str) -> Result<(), String> {
        self.console
            .lock()
            .unwrap()
            .push((key.to_string(), data.to_string()));
        Ok(())
    }
}

struct World {
    _tmp: tempfile::TempDir,
    root: String,
    net: MemoryNetwork,
    host: Arc<HostSession>,
    app: Arc<FakeApp>,
    log: Arc<SessionLog>,
    next_key: u8,
}

/// A guest and what reached its window.
struct Guest {
    session: Arc<GuestSession>,
    denied: Arc<Mutex<Vec<String>>>,
}

impl World {
    fn new(permissions: Permissions) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("shop");
        std::fs::create_dir_all(&root).unwrap();
        let backend: Arc<dyn Backend> = Arc::new(LocalBackend::open(&root).unwrap());
        let net = MemoryNetwork::new();
        let host = HostSession::start(
            Arc::new(net.endpoint([1; 32])),
            backend,
            HostConfig {
                host_name: "Robert".into(),
                machine: Some("laptop".into()),
                permissions,
                excludes: Excludes::defaults(),
                max_file_size: 64 * 1024,
                notice: Some(DEV_PREVIEW_NOTICE.into()),
            },
            Arc::new(DevAutoAccept),
            Arc::new(|_| {}),
        );
        let app = Arc::new(FakeApp::default());
        host.set_services(app.clone());
        let log = SessionLog::memory();
        host.set_audit(log.clone());
        Self {
            _tmp: tmp,
            root: root.to_string_lossy().into_owned(),
            net,
            host,
            app,
            log,
            next_key: 2,
        }
    }

    async fn join(&mut self, name: &str) -> Guest {
        let key = [self.next_key; 32];
        self.next_key += 1;
        let denied: Arc<Mutex<Vec<String>>> = Arc::default();
        let d = denied.clone();
        let session = GuestSession::join(
            Arc::new(self.net.endpoint(key)),
            &self.host.link(),
            name,
            Arc::new(move |e| {
                if let GuestEvent::Denied(reason) = e {
                    d.lock().unwrap().push(reason);
                }
            }),
        )
        .await
        .unwrap();
        Guest { session, denied }
    }

    fn log(&self) -> Vec<String> {
        self.log.lines().into_iter().map(|l| l.message).collect()
    }

    fn logged(&self, text: &str) -> usize {
        self.log().iter().filter(|l| l.contains(text)).count()
    }
}

/// What an attached terminal tab received.
#[derive(Default, Clone)]
struct Tab(Arc<Mutex<(String, Option<Option<i32>>)>>);

impl Tab {
    fn sink(&self) -> forge_process::PtySink {
        let t = self.0.clone();
        Arc::new(move |e| match e {
            PtyEvent::Output { data } => t.lock().unwrap().0.push_str(&data),
            PtyEvent::Exited { code } => t.lock().unwrap().1 = Some(code),
        })
    }
    fn text(&self) -> String {
        self.0.lock().unwrap().0.clone()
    }
    fn exited(&self) -> Option<Option<i32>> {
        self.0.lock().unwrap().1
    }
}

async fn until(what: &str, mut f: impl FnMut() -> bool) {
    for _ in 0..400 {
        if f() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

/// Give messages in flight time to arrive (for asserting that something did not happen).
async fn settle() {
    tokio::time::sleep(Duration::from_millis(150)).await;
}

fn terminal(g: &Guest, title: &str) -> Option<SharedTerminal> {
    g.session.terminals().into_iter().find(|t| t.title == title)
}

fn run(g: &Guest, title: &str) -> Option<SharedRun> {
    g.session.runs().into_iter().find(|r| r.title == title)
}

async fn call(g: &Guest, call: HostCall) -> Result<Vec<u8>, String> {
    let s = g.session.clone();
    tokio::task::spawn_blocking(move || s.call_blocking(&call, Duration::from_secs(10)))
        .await
        .unwrap()
}

// ---- terminals ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_read_only_guest_watches_a_shared_terminal_but_cannot_type() {
    let mut w = World::new(READ);
    w.host.terminal_output("term-1", "not shared yet\r\n");
    w.host
        .share_terminal("term-1", "Local", 120, 30, "$ composer install\r\n", true);
    w.host.terminal_output("term-1", "Installing...\r\n");
    let g = w.join("Alex").await;
    until("the shared terminal", || terminal(&g, "Local").is_some()).await;
    let t = terminal(&g, "Local").unwrap();
    assert_eq!(
        (t.cols, t.rows),
        (120, 30),
        "guests render at the host's size"
    );
    assert!(t.running);

    let tab = Tab::default();
    let (before, _, handle) = g.session.attach_terminal(t.id, tab.sink()).unwrap();
    assert_eq!(before, "$ composer install\r\nInstalling...\r\n");
    assert!(!before.contains("not shared yet"));
    w.host.terminal_output("term-1", "Done.\r\n");
    until("live output", || tab.text() == "Done.\r\n").await;

    // Every key is refused by the host; the log and the guest hear it once.
    for _ in 0..20 {
        handle.write(b"rm -rf /\r").unwrap();
    }
    g.session.terminal_input(TerminalId(999), "x");
    until("the refusal", || !g.denied.lock().unwrap().is_empty()).await;
    settle().await;
    assert!(
        w.app.writes.lock().unwrap().is_empty(),
        "no key reached the shell"
    );
    assert_eq!(
        g.denied.lock().unwrap().len(),
        1,
        "refused once, not per key"
    );
    assert_eq!(
        w.logged("Refused terminal input of Alex"),
        1,
        "{:?}",
        w.log()
    );
    assert_eq!(w.logged("rm -rf"), 0, "keys are never logged");
    // Read-only guests cannot open terminals on the host either.
    assert!(call(&g, HostCall::TerminalOpen { cols: 80, rows: 24 })
        .await
        .is_err());
    assert!(w.app.opened.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_guest_without_terminal_access_sees_no_terminals() {
    let mut w = World::new(EDIT);
    w.host
        .share_terminal("term-1", "Local", 80, 24, "secret output\r\n", true);
    let g = w.join("Alex").await;
    w.host.terminal_output("term-1", "more secret output\r\n");
    settle().await;
    assert!(g.session.terminals().is_empty());
    assert!(g
        .session
        .attach_terminal(TerminalId(1), Tab::default().sink())
        .is_err());
    assert!(call(&g, HostCall::TerminalOpen { cols: 80, rows: 24 })
        .await
        .is_err());
    // Getting Read-only later shows the terminal with what it printed so far.
    w.host.set_permissions(READ);
    until("the terminal", || terminal(&g, "Local").is_some()).await;
    let (before, _, _) = g
        .session
        .attach_terminal(terminal(&g, "Local").unwrap().id, Tab::default().sink())
        .unwrap();
    assert_eq!(before, "secret output\r\nmore secret output\r\n");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_full_access_guest_types_into_shared_terminals_and_opens_its_own() {
    let mut w = World::new(FULL);
    w.host.share_terminal("term-1", "Local", 80, 24, "$ ", true);
    let g = w.join("Alex").await;
    until("the shared terminal", || terminal(&g, "Local").is_some()).await;
    let t = terminal(&g, "Local").unwrap();
    let (_, _, handle) = g
        .session
        .attach_terminal(t.id, Tab::default().sink())
        .unwrap();
    handle.write(b"php artisan test\r").unwrap();
    handle.write(b"\x03").unwrap();
    until("the keys", || w.app.writes.lock().unwrap().len() == 2).await;
    assert_eq!(
        w.app.writes.lock().unwrap()[0],
        ("term-1".to_string(), "php artisan test\r".to_string())
    );
    assert_eq!(
        w.logged("Alex typed in the terminal Local"),
        1,
        "once, without keys"
    );
    assert_eq!(w.logged("artisan"), 0);

    // Its own terminal runs on the host, titled after the guest; the host sees it as a tab.
    let tab = Tab::default();
    let s = g.session.clone();
    let sink = tab.sink();
    let own = tokio::task::spawn_blocking(move || s.open_terminal_blocking(100, 40, sink))
        .await
        .unwrap()
        .unwrap();
    let (key, title, cols, rows) = w.app.opened.lock().unwrap()[0].clone();
    assert_eq!((title.as_str(), cols, rows), ("Alex: Local", 100, 40));
    assert!(w
        .host
        .shared_terminals()
        .iter()
        .any(|(k, t)| *k == key && t.title == "Alex: Local"));
    w.host.terminal_output(&key, "alex@host:~/shop$ ");
    until("its prompt", || tab.text() == "alex@host:~/shop$ ").await;
    own.write(b"ls\r").unwrap();
    until("its keys", || {
        w.app
            .writes
            .lock()
            .unwrap()
            .iter()
            .any(|(k, d)| *k == key && d == "ls\r")
    })
    .await;
    assert_eq!(w.logged("Alex opened the terminal Alex: Local"), 1);

    // It may close only its own terminal.
    let host_terminal = terminal(&g, "Local").unwrap().id;
    let err = call(
        &g,
        HostCall::TerminalClose {
            terminal: host_terminal,
        },
    )
    .await;
    assert!(err.is_err());
    assert!(w.app.closed.lock().unwrap().is_empty());
    own.kill().unwrap();
    until("the close", || w.app.closed.lock().unwrap().contains(&key)).await;
    until("the list", || terminal(&g, "Alex: Local").is_none()).await;
    assert_eq!(w.logged("Alex closed the terminal Alex: Local"), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn losing_terminal_access_closes_the_guests_terminals() {
    let mut w = World::new(FULL);
    let g = w.join("Alex").await;
    let s = g.session.clone();
    let sink = Tab::default().sink();
    tokio::task::spawn_blocking(move || s.open_terminal_blocking(80, 24, sink))
        .await
        .unwrap()
        .unwrap();
    let key = w.app.opened.lock().unwrap()[0].0.clone();
    let id = g.session.participants().last().unwrap().id;
    w.host.set_guest_permissions(
        id,
        Permissions {
            terminal: TerminalAccess::ReadOnly,
            ..FULL
        },
    );
    until("the close", || w.app.closed.lock().unwrap().contains(&key)).await;
    until("the list", || terminal(&g, "Alex: Local").is_none()).await;
    // Typing into a shared terminal is refused from now on.
    w.host.share_terminal("term-1", "Local", 80, 24, "", true);
    until("the shared terminal", || terminal(&g, "Local").is_some()).await;
    g.session
        .terminal_input(terminal(&g, "Local").unwrap().id, "whoami\r");
    until("the refusal", || !g.denied.lock().unwrap().is_empty()).await;
    assert!(w.app.writes.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unsharing_exiting_and_leaving_end_terminals() {
    let mut w = World::new(FULL);
    w.host.share_terminal("term-1", "Local", 80, 24, "", true);
    w.host
        .share_terminal("term-2", "Local (2)", 80, 24, "", true);
    let g = w.join("Alex").await;
    until("both terminals", || g.session.terminals().len() == 2).await;
    let one = Tab::default();
    let two = Tab::default();
    g.session
        .attach_terminal(terminal(&g, "Local").unwrap().id, one.sink())
        .unwrap();
    g.session
        .attach_terminal(terminal(&g, "Local (2)").unwrap().id, two.sink())
        .unwrap();
    w.host.terminal_exited("term-1", Some(130));
    until("the exit", || one.exited() == Some(Some(130))).await;
    w.host
        .share_terminal("term-2", "Local (2)", 80, 24, "", false);
    until("the unshare", || two.exited() == Some(None)).await;
    assert_eq!(
        g.session.terminals().len(),
        1,
        "the exited one stays until closed"
    );
    w.host.terminal_closed("term-1");
    until("the close", || g.session.terminals().is_empty()).await;
    assert!(w
        .log()
        .iter()
        .any(|l| l == "Robert shared the terminal Local"));
    assert_eq!(w.logged("Robert stopped sharing the terminal Local (2)"), 1);

    // A guest's terminals close when it leaves.
    let s = g.session.clone();
    let sink = Tab::default().sink();
    tokio::task::spawn_blocking(move || s.open_terminal_blocking(80, 24, sink))
        .await
        .unwrap()
        .unwrap();
    let key = w.app.opened.lock().unwrap()[0].0.clone();
    g.session.leave().await;
    until("the close", || w.app.closed.lock().unwrap().contains(&key)).await;
}

// ---- runs --------------------------------------------------------------------------------

/// The host's PHPUnit run: started, raw output, a console line with the host's path, a test.
fn host_run(w: &World, key: &str) {
    w.host.run_started(key, "UserTest", Some("phpunit"));
    w.host
        .run_event(key, r#"{"type":"started","commandLine":"phpunit"}"#);
    w.host
        .run_event(key, r#"{"type":"output","stream":"stdout","text":"raw"}"#);
    let console = format!(
        r#"{{"type":"console","lines":[{{"segments":[{{"text":"{}/tests/UserTest.php:12"}}]}}]}}"#,
        w.root
    );
    w.host.run_event(key, &console);
    w.host.run_event(
        key,
        r#"{"type":"tests","nodes":[{"id":"1","name":"testName","status":"passed"}],"summary":{}}"#,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_read_only_guest_follows_the_hosts_runs_but_cannot_start_or_stop() {
    let mut w = World::new(READ);
    host_run(&w, "run-1");
    let g = w.join("Alex").await;
    until("the run", || run(&g, "UserTest").is_some()).await;
    let r = run(&g, "UserTest").unwrap();
    assert_eq!(r.kind.as_deref(), Some("phpunit"));
    assert!(r.running);

    let live: Arc<Mutex<Vec<String>>> = Arc::default();
    let l = live.clone();
    let before = g
        .session
        .attach_run(r.id, Arc::new(move |e| l.lock().unwrap().push(e)))
        .unwrap();
    assert_eq!(
        before.len(),
        3,
        "started, console, tests; raw output is not sent: {before:?}"
    );
    assert!(
        before[1].contains("\"text\":\"/shop/tests/UserTest.php:12\""),
        "the host's folder becomes the guest's project root: {}",
        before[1]
    );
    assert!(!before.iter().any(|e| e.contains(&w.root)));

    w.host
        .run_event("run-1", r#"{"type":"exited","code":1,"signal":null}"#);
    until("the exit", || live.lock().unwrap().len() == 1).await;
    until("the list", || !run(&g, "UserTest").unwrap().running).await;
    assert_eq!(run(&g, "UserTest").unwrap().exit_code, Some(1));

    for c in [
        HostCall::RunStart {
            configuration: "UserTest".into(),
        },
        HostCall::RunStop { run: r.id },
    ] {
        assert!(call(&g, c).await.is_err());
    }
    g.session.run_input(r.id, "yes\n");
    until("the refusal", || !g.denied.lock().unwrap().is_empty()).await;
    assert!(w.app.started.lock().unwrap().is_empty());
    assert!(w.app.stopped.lock().unwrap().is_empty());
    assert!(w.app.console.lock().unwrap().is_empty());
    assert!(w.logged("Refused run of Alex") >= 1, "{:?}", w.log());

    // Closing the tab on the host removes the run for guests.
    w.host.run_disposed("run-1");
    until("the removal", || g.session.runs().is_empty()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_full_access_guest_runs_stops_and_types_into_the_hosts_configurations() {
    let mut w = World::new(FULL);
    let g = w.join("Alex").await;
    call(
        &g,
        HostCall::RunStart {
            configuration: "UserTest".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        w.app.started.lock().unwrap()[0],
        ("UserTest".to_string(), "Alex".to_string()),
        "the host application starts its own configuration by name"
    );
    // The host application starts the run; it counts as started by the guest.
    host_run(&w, "run-7");
    until("the run", || run(&g, "UserTest").is_some()).await;
    let r = run(&g, "UserTest").unwrap();
    assert_eq!(r.started_by, g.session.me());

    g.session.run_input(r.id, "yes\n");
    until("the input", || !w.app.console.lock().unwrap().is_empty()).await;
    assert_eq!(
        w.app.console.lock().unwrap()[0],
        ("run-7".to_string(), "yes\n".to_string())
    );
    call(&g, HostCall::RunStop { run: r.id }).await.unwrap();
    assert_eq!(w.app.stopped.lock().unwrap()[0], "run-7");
    assert!(call(&g, HostCall::RunStop { run: RunId(999) })
        .await
        .is_err());
    let err = call(
        &g,
        HostCall::RunStart {
            configuration: "Missing".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(err.contains("Missing"), "{err}");

    let log = w.log();
    for line in [
        "Alex ran 'UserTest'",
        "Alex typed in the console of 'UserTest'",
        "Alex stopped 'UserTest'",
    ] {
        assert!(log.iter().any(|l| l == line), "{line} in {log:?}");
    }
    assert_eq!(w.logged("yes"), 0, "console input is not logged");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn run_access_is_its_own_area() {
    // Terminal Full access, Run Read-only: typing works, running does not.
    let mut w = World::new(Permissions {
        terminal: TerminalAccess::Full,
        run: RunAccess::ReadOnly,
        ..READ
    });
    let g = w.join("Alex").await;
    assert!(call(
        &g,
        HostCall::RunStart {
            configuration: "UserTest".into()
        }
    )
    .await
    .is_err());
    assert!(call(&g, HostCall::TerminalOpen { cols: 80, rows: 24 })
        .await
        .is_ok());
    // Without Run access, runs are not shown at all.
    w.host.set_permissions(EDIT);
    host_run(&w, "run-1");
    settle().await;
    assert!(g.session.runs().is_empty());
}
