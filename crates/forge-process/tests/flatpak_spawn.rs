//! Flatpak host spawning without Flatpak: `FLATPAK_ID` makes Forge believe it runs in a
//! sandbox, and a fake `flatpak-spawn` first on `PATH` records its arguments and runs the
//! command like `flatpak-spawn --host` would. This exercises the pipe (Run) and pseudo
//! terminal (Terminal) paths end to end on Linux.
#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use forge_process::{
    CommandLine, PtyEvent, PtyOptions, PtySession, PtySink, RunEvent, RunSink, RunningProcess,
};

struct Fake {
    log: PathBuf,
}

/// Install the fake once for this test binary, before anything asks whether Forge runs in
/// Flatpak (the answer is cached).
fn fake() -> &'static Fake {
    static FAKE: OnceLock<Fake> = OnceLock::new();
    FAKE.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap().keep();
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let fake = bin.join("flatpak-spawn");
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flatpak-spawn");
        std::fs::copy(src, &fake).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let log = dir.join("calls.log");
        let path = std::env::join_paths(
            std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
        )
        .unwrap();
        std::env::set_var("PATH", path);
        std::env::set_var("FLATPAK_ID", "dev.forge.ide");
        std::env::set_var("FAKE_FLATPAK_LOG", &log);
        assert!(forge_process::host::active(), "Flatpak mode not detected");
        Fake { log }
    })
}

/// The recorded `flatpak-spawn` calls whose arguments contain `marker`.
fn calls(marker: &str) -> Vec<Vec<String>> {
    let text = std::fs::read_to_string(&fake().log).unwrap_or_default();
    text.split('\u{1e}')
        .map(|l| {
            l.split('\u{1f}')
                .filter(|a| !a.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|args| args.iter().any(|a| a.contains(marker)))
        .collect()
}

fn run(cmd: CommandLine) -> (RunningProcess, mpsc::Receiver<RunEvent>) {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let sink: RunSink = Arc::new(move |e| {
        let _ = tx.lock().unwrap().send(e);
    });
    (RunningProcess::start(&cmd, sink).expect("start"), rx)
}

fn drain(rx: &mpsc::Receiver<RunEvent>) -> (String, Option<(Option<i32>, Option<i32>)>) {
    let mut out = String::new();
    while let Ok(e) = rx.recv_timeout(Duration::from_secs(10)) {
        match e {
            RunEvent::Output { text, .. } => out.push_str(&text),
            RunEvent::Exited { code, signal } => return (out, Some((code, signal))),
            RunEvent::Started { .. } => {}
        }
    }
    (out, None)
}

#[test]
fn run_configurations_go_through_flatpak_spawn_with_env_cwd_and_pipes() {
    fake();
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = CommandLine::new("/bin/sh").args([
        "-c",
        "echo \"value=$FORGE_FAKE_RUN\"; pwd; read line; echo \"stdin=$line\"; exit 4",
    ]);
    cmd.cwd = dir.path().to_path_buf();
    cmd.env.insert("FORGE_FAKE_RUN".into(), "run-1".into());
    let (p, rx) = run(cmd);
    p.write_stdin(b"hello host\n").unwrap();
    let (out, exit) = drain(&rx);
    assert!(out.contains("value=run-1"), "{out:?}");
    let cwd = std::fs::canonicalize(dir.path()).unwrap();
    assert!(out.contains(&cwd.display().to_string()), "{out:?}");
    assert!(out.contains("stdin=hello host"), "{out:?}");
    assert_eq!(exit, Some((Some(4), None)));

    let calls = calls("run-1");
    assert_eq!(calls.len(), 1, "{calls:?}");
    let args = &calls[0];
    assert_eq!(&args[..2], ["--host", "--watch-bus"]);
    assert!(args.contains(&format!("--directory={}", dir.path().display())));
    assert!(args.contains(&"--env=FORGE_FAKE_RUN=run-1".to_string()));
    let program = args.iter().position(|a| a == "/bin/sh").unwrap();
    assert_eq!(args[program + 1], "-c");
}

#[test]
fn stop_kills_the_host_process() {
    fake();
    let cmd = CommandLine::new("/bin/sh").args(["-c", "echo stop-ready; sleep 30 # stop-1"]);
    let (p, rx) = run(cmd);
    // Wait until the command runs, then stop it.
    loop {
        match rx.recv_timeout(Duration::from_secs(10)).expect("no output") {
            RunEvent::Output { text, .. } if text.contains("stop-ready") => break,
            _ => {}
        }
    }
    p.kill(Duration::from_millis(500));
    let (_, exit) = drain(&rx);
    assert!(exit.is_some(), "process did not stop");
    assert_eq!(calls("stop-1").len(), 1);
}

fn collect_until(
    rx: &mpsc::Receiver<PtyEvent>,
    pred: impl Fn(&str) -> bool,
) -> (String, Option<Option<i32>>) {
    let mut text = String::new();
    while let Ok(e) = rx.recv_timeout(Duration::from_secs(10)) {
        match e {
            PtyEvent::Output { data } => {
                text.push_str(&data);
                if pred(&text) {
                    return (text, None);
                }
            }
            PtyEvent::Exited { code } => return (text, Some(code)),
        }
    }
    (text, None)
}

#[test]
fn the_terminal_shell_runs_on_the_host_with_a_controlling_terminal() {
    fake();
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let sink: PtySink = Arc::new(move |e| {
        let _ = tx.lock().unwrap().send(e);
    });
    let dir = tempfile::tempdir().unwrap();
    let s = PtySession::spawn(
        PtyOptions {
            shell: Some("/bin/sh".into()),
            args: Some(vec![]),
            cwd: dir.path().to_path_buf(),
            env: vec![("FORGE_FAKE_PTY".into(), "pty-1".into())],
            version: "9.9.9".into(),
            cols: 80,
            rows: 24,
            integration_dir: None,
        },
        sink,
    )
    .expect("spawn");
    s.resize(100, 30).unwrap();
    // Opening /dev/tty only works with a controlling terminal (job control, Ctrl+C).
    s.write(
        b": </dev/tty && echo ctty-$((40+2)); stty size; echo \"$TERM:$FORGE_FAKE_PTY:done\"\n",
    )
    .unwrap();
    let (text, _) = collect_until(&rx, |t| t.contains("xterm-256color:pty-1:done"));
    assert!(text.contains("ctty-42"), "{text:?}");
    assert!(text.contains("30 100"), "{text:?}");

    let calls = calls("pty-1");
    assert_eq!(calls.len(), 1, "{calls:?}");
    let args = &calls[0];
    assert_eq!(&args[..2], ["--host", "--watch-bus"]);
    assert!(args.contains(&format!("--directory={}", dir.path().display())));
    assert!(args.contains(&"--env=TERM=xterm-256color".to_string()));
    let sh = args.iter().position(|a| a == "-c").unwrap();
    assert_eq!(args[sh - 1], "/bin/sh");
    assert_eq!(&args[sh + 2..], ["forge-terminal", "/bin/sh"]);

    // Closing the tab: the hang-up reaches the host shell, which exits.
    s.kill().unwrap();
    let (_, exit) = collect_until(&rx, |_| false);
    assert!(exit.is_some(), "no exit after kill");
}

#[test]
fn tools_are_looked_up_on_the_host_path() {
    fake();
    // The fake host runs this process's login shell environment.
    let sh = forge_process::host::which("sh").expect("sh on the host PATH");
    assert!(sh.is_absolute());
    assert!(forge_process::host::path_var().is_some());
}
