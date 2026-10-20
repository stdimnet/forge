//! The agent's services against a temporary project, driven in-process through
//! `RemoteBackend` over an in-memory pipe (the same code path as over SSH).

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_backend::protocol::{BrowseResult, Call, Hello, PROTOCOL_VERSION};
use forge_backend::rpc::RpcClient;
use forge_backend::{Backend, RemoteBackend, RemoteHost, TerminalSpec};
use forge_core::FsChanges;
use forge_lsp::LspServerConfig;
use forge_process::{CommandLine, PtyEvent, RunEvent};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const T: Duration = Duration::from_secs(20);

fn connect() -> RpcClient {
    let (client_end, agent_end) = tokio::io::duplex(1 << 16);
    let (agent_read, agent_write) = tokio::io::split(agent_end);
    tokio::spawn(forge_remote_agent::serve(agent_read, agent_write));
    let (read, write) = tokio::io::split(client_end);
    RpcClient::spawn(read, write)
}

fn host() -> RemoteHost {
    RemoteHost {
        host: "shop.example.com".into(),
        port: 22,
        user: "deploy".into(),
        config_id: None,
    }
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    std::fs::write(p.join("composer.json"), r#"{"require":{"php":"^8.2"}}"#).unwrap();
    std::fs::create_dir_all(p.join("app/Models")).unwrap();
    std::fs::write(
        p.join("app/Models/Order.php"),
        "<?php\nnamespace App\\Models;\n\n/** An order of the shop. */\nclass Order\n{\n    public function total(): int { return 0; }\n}\n",
    )
    .unwrap();
    std::fs::write(p.join(".gitignore"), "/vendor\n").unwrap();
    std::fs::create_dir_all(p.join("vendor")).unwrap();
    dir
}

/// Run blocking backend code off the async runtime.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.unwrap()
}

fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while !cond() {
        assert!(start.elapsed() < T, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hello_ping_and_browse() {
    let client = connect();
    let hello: Hello = client
        .call(
            Call::Hello {
                version: "test".into(),
                protocol: PROTOCOL_VERSION,
            },
            T,
        )
        .await
        .unwrap();
    assert_eq!(hello.protocol, PROTOCOL_VERSION);
    assert_eq!(hello.version, forge_remote_agent::VERSION);
    assert_eq!(hello.os, std::env::consts::OS);
    client.call::<()>(Call::Ping, T).await.unwrap();

    let wrong = client
        .call::<Hello>(
            Call::Hello {
                version: "old".into(),
                protocol: PROTOCOL_VERSION + 1,
            },
            T,
        )
        .await;
    assert!(wrong.unwrap_err().to_string().contains("protocol"));

    let dir = project();
    let listing: BrowseResult = client
        .call(
            Call::Browse {
                path: dir.path().to_string_lossy().into(),
            },
            T,
        )
        .await
        .unwrap();
    assert!(listing.entries.iter().any(|e| e.name == "app" && e.dir));

    // Calls before a project is open fail cleanly.
    let err = client
        .call::<String>(
            Call::ReadFile {
                path: "composer.json".into(),
            },
            T,
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("no project is open"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn files_search_git_and_detection() {
    let dir = project();
    let root = dir.path().canonicalize().unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "init"]);

    let client = connect();
    let root_str = root.to_string_lossy().into_owned();
    let backend =
        blocking(move || RemoteBackend::open(Arc::new(client), host(), &root_str).unwrap()).await;
    let backend = Arc::new(backend);
    assert_eq!(backend.info().root, root);
    assert_eq!(
        backend.info().display_root(),
        format!("deploy@shop.example.com:{}", root.display())
    );

    let b = backend.clone();
    blocking(move || {
        let names: Vec<(String, bool)> = b
            .list_dir("")
            .unwrap()
            .into_iter()
            .map(|e| (e.name, e.ignored))
            .collect();
        assert_eq!(
            names,
            vec![
                ("app".to_string(), false),
                ("vendor".to_string(), true),
                (".gitignore".to_string(), false),
                ("composer.json".to_string(), false),
            ]
        );
        b.write_file("app/Http/Kernel.php", "<?php\n// ü\n")
            .unwrap();
        assert_eq!(b.read_file("app/Http/Kernel.php").unwrap(), "<?php\n// ü\n");
        assert!(b.read_file("../outside").is_err());

        // Project view file operations run on the agent's machine.
        b.create_dir("app/Jobs").unwrap();
        b.create_file("app/Jobs/SendMail.php").unwrap();
        assert!(b.create_file("app/Jobs/SendMail.php").is_err());
        b.rename("app/Jobs/SendMail.php", "app/Jobs/SendInvoice.php")
            .unwrap();
        assert_eq!(b.read_file("app/Jobs/SendInvoice.php").unwrap(), "");
        b.delete("app/Jobs").unwrap();
        assert!(b.read_file("app/Jobs/SendInvoice.php").is_err());
        assert!(b.delete("../outside").is_err());

        let hits = b.find_files("ordr", 10).unwrap();
        assert_eq!(hits[0].path, "app/Models/Order.php");

        let info = b.detect_project().unwrap();
        assert!(info.uses("php"));

        // git runs on the agent's machine, its output is parsed here.
        let repo = b.repo().unwrap();
        let status = repo.status().unwrap();
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert!(status
            .entries
            .iter()
            .any(|e| e.path == "app/Http/Kernel.php"));
        repo.stage(&["app/Http/Kernel.php".to_string()]).unwrap();
        repo.commit("Add kernel", false, None).unwrap();
        assert!(repo.status().unwrap().entries.is_empty());
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn php_index_runs_on_the_agent() {
    let dir = project();
    let root = dir.path().to_string_lossy().into_owned();
    let client = connect();
    let backend =
        blocking(move || RemoteBackend::open(Arc::new(client), host(), &root).unwrap()).await;
    blocking(move || {
        let progress = Arc::new(Mutex::new(Vec::new()));
        let seen = progress.clone();
        let php = backend.php();
        let stats = php
            .index_workspace(Arc::new(move |done, total| {
                seen.lock().unwrap().push((done, total))
            }))
            .unwrap();
        assert!(stats.classes >= 1, "{stats:?}");
        assert!(!progress.lock().unwrap().is_empty());

        let hits = php.search("Order", Default::default(), 10).unwrap();
        assert_eq!(hits[0].name, "Order");

        let text = backend.read_file("app/Models/Order.php").unwrap();
        let offset = text.find("Order\n{").unwrap() + 1;
        let hover = php
            .hover("app/Models/Order.php", offset, None)
            .unwrap()
            .unwrap();
        assert!(
            hover.markdown.contains("An order of the shop"),
            "{}",
            hover.markdown
        );

        let structure = php
            .file_structure("app/Models/Order.php", Some(text), false)
            .unwrap();
        assert_eq!(structure.nodes[0].name, "Order");
        assert_eq!(structure.nodes[0].children[0].name, "total");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn terminals_processes_and_watching() {
    let dir = project();
    let root = dir.path().canonicalize().unwrap();
    let root_str = root.to_string_lossy().into_owned();
    let client = connect();
    let backend = Arc::new(
        blocking(move || RemoteBackend::open(Arc::new(client), host(), &root_str).unwrap()).await,
    );

    let b = backend.clone();
    let watch_root = root.clone();
    blocking(move || {
        // Watching: a file created on the remote side arrives as a change.
        let changes: Arc<Mutex<Vec<FsChanges>>> = Arc::default();
        let sink = changes.clone();
        let _guard = b
            .watch(Arc::new(move |c| sink.lock().unwrap().push(c)))
            .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        std::fs::write(watch_root.join("app/new.php"), "<?php\n").unwrap();
        wait_until("a file change", || {
            changes
                .lock()
                .unwrap()
                .iter()
                .any(|c| c.changed.iter().any(|p| p == "app/new.php"))
        });

        // Terminal: a shell on the agent's machine.
        let output = Arc::new(Mutex::new(String::new()));
        let exited = Arc::new(Mutex::new(false));
        let (out, ex) = (output.clone(), exited.clone());
        let term = b
            .open_terminal(
                TerminalSpec {
                    shell: Some("/bin/sh".into()),
                    args: None,
                    cwd: watch_root.join("app"),
                    env: vec![("FORGE_TEST".into(), "remote-shell".into())],
                    cols: 80,
                    rows: 24,
                },
                Arc::new(move |e| match e {
                    PtyEvent::Output { data } => out.lock().unwrap().push_str(&data),
                    PtyEvent::Exited { .. } => *ex.lock().unwrap() = true,
                }),
            )
            .unwrap();
        assert_eq!(term.cwd(), watch_root.join("app"));
        term.resize(100, 30).unwrap();
        term.write(b"echo \"$FORGE_TEST\"; exit\n").unwrap();
        wait_until("terminal output", || {
            output.lock().unwrap().contains("remote-shell")
        });
        wait_until("the shell to exit", || *exited.lock().unwrap());

        // Process with stdin and an exit code.
        let events: Arc<Mutex<Vec<RunEvent>>> = Arc::default();
        let sink = events.clone();
        let mut cmd = CommandLine::new("sh");
        cmd.args = vec!["-c".into(), "read x; echo \"got $x\"; exit 3".into()];
        cmd.cwd = watch_root.clone();
        let process = b
            .start_process(&cmd, Arc::new(move |e| sink.lock().unwrap().push(e)))
            .unwrap();
        process.write_stdin(b"hello\n").unwrap();
        wait_until("the process to exit", || {
            events
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, RunEvent::Exited { .. }))
        });
        let events = events.lock().unwrap();
        assert!(events
            .iter()
            .any(|e| matches!(e, RunEvent::Output { text, .. } if text.contains("got hello"))));
        assert!(events
            .iter()
            .any(|e| matches!(e, RunEvent::Exited { code: Some(3), .. })));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn language_server_stdio_is_proxied() {
    let dir = project();
    let root = dir.path().to_string_lossy().into_owned();
    let client = connect();
    let backend =
        blocking(move || RemoteBackend::open(Arc::new(client), host(), &root).unwrap()).await;
    // `cat` echoes its stdin: whatever goes into the channel comes back out.
    let config = LspServerConfig {
        id: "echo".into(),
        name: None,
        language_ids: vec![],
        file_extensions: vec![],
        command: "cat".into(),
        args: vec![],
        initialization_options: serde_json::Value::Null,
        settings: serde_json::Value::Null,
    };
    let launcher = backend.lsp_launcher();
    let mut io = launcher
        .launch(&config, &backend.info().root)
        .await
        .unwrap();
    let message = b"Content-Length: 2\r\n\r\n{}";
    io.stdin.write_all(message).await.unwrap();
    let mut buf = vec![0u8; message.len()];
    tokio::time::timeout(T, io.stdout.read_exact(&mut buf))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&buf, message);
    io.process.stop(Duration::from_secs(1)).await;
    // The agent stops the server and closes the channel: the stream ends.
    let mut rest = Vec::new();
    tokio::time::timeout(T, io.stdout.read_to_end(&mut rest))
        .await
        .unwrap()
        .unwrap();
    assert!(rest.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn language_servers_run_on_the_agent() {
    let dir = project();
    let root = dir.path().canonicalize().unwrap();
    let root_str = root.to_string_lossy().into_owned();
    let client = connect();
    let backend =
        blocking(move || RemoteBackend::open(Arc::new(client), host(), &root_str).unwrap()).await;
    let manager =
        forge_lsp::LspManager::with_launcher(backend.info().root.clone(), backend.lsp_launcher());
    manager.register_server(LspServerConfig {
        id: "fake".into(),
        name: Some("Fake LS".into()),
        language_ids: vec!["php".into()],
        file_extensions: vec!["php".into()],
        command: env!("CARGO_BIN_EXE_forge-remote-agent-fake-lsp").into(),
        args: vec![],
        initialization_options: serde_json::json!({}),
        settings: serde_json::json!({}),
    });
    let file = backend.resolve("app/Models/Order.php").unwrap();
    let mut events = manager.subscribe();
    manager
        .did_open(&file, "<?php\n// TODO remote\n")
        .await
        .unwrap();
    // Diagnostics computed by the server on the agent's machine arrive here.
    let (uri, diagnostics) = tokio::time::timeout(T, async {
        loop {
            if let Ok(forge_lsp::LspEvent::Diagnostics {
                uri, diagnostics, ..
            }) = events.recv().await
            {
                return (uri, diagnostics);
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(forge_lsp::uri_to_path(&uri).unwrap(), file);
    assert_eq!(diagnostics.len(), 1);
    let hover = manager
        .hover(&file, forge_lsp::lsp_types::Position::new(1, 3))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(hover.contents, "**hover** 1:3");
    manager.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn calls_fail_once_the_connection_is_gone() {
    let client = connect();
    client.call::<()>(Call::Ping, T).await.unwrap();
    client.shutdown();
    tokio::time::timeout(T, client.closed()).await.unwrap();
    let err = client.call::<()>(Call::Ping, T).await.unwrap_err();
    assert!(err.to_string().contains("lost"));
}
