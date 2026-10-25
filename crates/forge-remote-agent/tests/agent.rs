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
use phpforge_lang_php::hierarchy::HierarchyQuery;
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
        kind: Default::default(),
    }
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    std::fs::write(p.join("composer.json"), r#"{"require":{"php":"^8.2"}}"#).unwrap();
    std::fs::create_dir_all(p.join("app/Models")).unwrap();
    std::fs::write(
        p.join("app/Models/Order.php"),
        "<?php\nnamespace App\\Models;\n\n/** An order of the shop. TODO: taxes */\nclass Order\n{\n    public function total(): int { return 0; }\n}\n",
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
        // Excluded folders (Settings | Directories) apply on the host.
        let excluded = forge_core::excludes::StructureConfig {
            folders: vec![forge_core::excludes::FolderMark {
                path: "app".into(),
                kind: forge_core::excludes::FolderKind::Excluded,
            }],
            ..Default::default()
        };
        assert!(b.set_structure(excluded).unwrap());
        assert!(b.list_dir("").unwrap()[0].excluded);
        assert!(b.find_files("Order", 10, false).unwrap().is_empty());
        assert!(b.set_structure(Default::default()).unwrap());
        b.write_file("app/Http/Kernel.php", "<?php\n// ü\n")
            .unwrap();
        assert_eq!(b.read_file("app/Http/Kernel.php").unwrap(), "<?php\n// ü\n");
        assert!(b.read_file("../outside").is_err());
        // Files in other encodings travel as bytes (Windows-1252 "café").
        b.write_bytes("legacy.txt", b"caf\xE9\r\n").unwrap();
        assert_eq!(b.read_bytes("legacy.txt").unwrap(), b"caf\xE9\r\n");
        assert!(b.read_bytes("../outside").is_err());
        b.delete("legacy.txt").unwrap();

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

        let hits = b.find_files("ordr", 10, false).unwrap();
        assert_eq!(hits[0].path, "app/Models/Order.php");

        // Find in Files searches on the agent's machine.
        let found = b
            .find_in_files(&forge_core::search::FindRequest {
                query: "function total".into(),
                file_mask: "*.php".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(found.files[0].path, "app/Models/Order.php");
        assert_eq!(found.files[0].matches[0].line, 6);

        // TODO comments are scanned on the agent's machine too.
        let todos = b.find_todos(&Default::default()).unwrap();
        assert_eq!(todos.files[0].items[0].text, "TODO: taxes");

        // Language indexes get every matching file in one call (vendor/ is ignored).
        let files = b
            .collect_files(&forge_core::CollectRequest {
                extensions: vec!["php".into()],
                max_files: 100,
                max_file_size: 1 << 20,
                max_total: 1 << 20,
            })
            .unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"app/Models/Order.php"), "{paths:?}");
        assert!(paths.iter().all(|p| !p.starts_with("vendor/")), "{paths:?}");
        assert!(files.iter().any(|f| f.text.contains("function total")));

        // Structural Search parses the files on the agent's machine.
        let found = b
            .structural_search(&forge_ssr::SsrRequest {
                template: "public function $name$() { $body$; }".into(),
                constraints: vec![forge_ssr::Constraint::named("body").count(0, None)],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(found.files[0].path, "app/Models/Order.php");
        assert_eq!(found.files[0].matches[0].bindings[0].text, "total");

        // The Scopes page lists the project's files on the agent's machine.
        let all = b.list_files(1000).unwrap();
        assert!(all.iter().any(|p| p == "app/Models/Order.php"), "{all:?}");
        assert!(all.iter().all(|p| !p.starts_with("vendor/")), "{all:?}");

        // Compare Directories reads both folders on the agent's machine.
        let diff = b.compare_dirs("app/Models", "vendor").unwrap();
        let rows: Vec<(&str, forge_diff::DiffStatus)> = diff
            .entries
            .iter()
            .map(|e| (e.path.as_str(), e.status))
            .collect();
        assert_eq!(rows, [("Order.php", forge_diff::DiffStatus::LeftOnly)]);
        assert!(b.compare_dirs("app", "../outside").is_err());

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

        let hits = php.search("Order", Default::default(), 10, false).unwrap();
        assert_eq!(hits[0].name, "Order");

        // Auto Import: unresolved class names with candidates from the agent's index.
        let unresolved = php
            .unresolved_classes(
                "app/Http/Show.php",
                Some("<?php\nnamespace App\\Http;\n\nnew Order();\n".into()),
            )
            .unwrap();
        assert_eq!(unresolved[0].name, "Order");
        assert_eq!(unresolved[0].candidates, vec!["App\\Models\\Order"]);

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
            .file_structure("app/Models/Order.php", Some(text.clone()), false)
            .unwrap();
        assert_eq!(structure.nodes[0].name, "Order");
        assert_eq!(structure.nodes[0].children[0].name, "total");

        // Navigation runs next to the index: Ctrl+Click on the declaration means "show
        // usages", the gutter markers come back, PHP's own names are left to the stubs.
        let def = php
            .definition("app/Models/Order.php", offset, Some(text.clone()))
            .unwrap();
        assert!(def.on_declaration, "{def:?}");
        assert!(php
            .line_markers("app/Models/Order.php", Some(text.clone()))
            .is_ok());
        // Type and Call Hierarchy from the agent's index.
        let types = php
            .hierarchy("app/Models/Order.php", offset, HierarchyQuery::Type, None)
            .unwrap();
        assert_eq!(types.graph.unwrap().root, "App\\Models\\Order");
        let at_total = text.find("total()").unwrap() + 1;
        let call = php
            .hierarchy(
                "app/Models/Order.php",
                at_total,
                HierarchyQuery::Call,
                Some(text.clone()),
            )
            .unwrap();
        assert_eq!(call.title.as_deref(), Some("Order::total()"));
        let with_throwable = format!("{text}\nfunction f(\\Throwable $e) {{}}\n");
        let at = with_throwable.find("Throwable $e").unwrap() + 1;
        let def = php
            .definition("app/Models/Order.php", at, Some(with_throwable))
            .unwrap();
        assert_eq!(def.external.unwrap().classes, vec!["Throwable"]);

        // Find Usages with options and Highlight Usages in File answer from the agent too.
        let usages = php
            .usages(
                "app/Models/Order.php",
                offset,
                Default::default(),
                Some(text.clone()),
            )
            .unwrap()
            .unwrap();
        assert_eq!(usages.title, "Order");
        let total = text.find("total()").unwrap() + 1;
        let highlights = php
            .highlights("app/Models/Order.php", total, Some(text.clone()))
            .unwrap()
            .unwrap();
        assert_eq!(highlights.title, "total");
        assert_eq!(highlights.highlights.len(), 1, "{highlights:?}");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn php_metadata_inspections_and_libraries_on_the_agent() {
    let dir = project();
    let p = dir.path();
    std::fs::write(
        p.join("composer.json"),
        r#"{"autoload":{"psr-4":{"App\\":"app/"}}}"#,
    )
    .unwrap();
    std::fs::write(
        p.join(".phpstorm.meta.php"),
        "<?php\nnamespace PHPSTORM_META {\n    expectedArguments(\\App\\Models\\Order::setStatus(), 0, 'draft', 'paid');\n}\n",
    )
    .unwrap();
    std::fs::write(
        p.join("app/Models/Order.php"),
        "<?php\nnamespace App\\Models;\n\nclass Order\n{\n    #[\\JetBrains\\PhpStorm\\Deprecated('Use pay()')]\n    public function settle(): void {}\n    public function setStatus(string $s): void {}\n    public function check(#[\\JetBrains\\PhpStorm\\Language('SQL')] string $where): void {}\n}\n",
    )
    .unwrap();
    std::fs::create_dir_all(p.join("app/Misc")).unwrap();
    std::fs::write(
        p.join("app/Misc/Thing.php"),
        "<?php\nnamespace App\\Wrong;\n\nclass Thing {}\n",
    )
    .unwrap();
    let lib = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(lib.path().join("Legacy")).unwrap();
    std::fs::write(
        lib.path().join("Legacy/Helper.php"),
        "<?php\nnamespace Legacy;\nclass Helper {}\n",
    )
    .unwrap();
    let lib_dir = lib.path().to_string_lossy().into_owned();
    let root = p.to_string_lossy().into_owned();
    let client = connect();
    let backend =
        blocking(move || RemoteBackend::open(Arc::new(client), host(), &root).unwrap()).await;
    blocking(move || {
        let php = backend.php();
        php.configure(phpforge_lang_php::project::PhpProjectSettings {
            include_paths: vec![lib_dir.clone()],
            ..Default::default()
        })
        .unwrap();
        php.index_workspace(Arc::new(|_, _| {})).unwrap();
        // The include path is indexed on the agent and browsable as a library.
        let hits = php.search("Helper", Default::default(), 10, false).unwrap();
        assert_eq!(hits[0].fqn, "Legacy\\Helper");
        let roots = php.libraries().unwrap();
        assert_eq!(roots.len(), 1);
        let entries = php.library_list(&roots[0].path).unwrap();
        assert_eq!(entries[0].name, "Legacy");
        let helper = format!("{}/Legacy/Helper.php", roots[0].path);
        assert!(php.library_read(&helper).unwrap().contains("class Helper"));
        assert!(php.library_read("/etc/hostname").is_err());

        // Inspections: deprecation, PSR-4 namespace.
        let src = "<?php\nnamespace App\\Http;\nuse App\\Models\\Order;\nfunction f(Order $o) { $o->settle(); $o->setStatus(''); }\n";
        let problems = php.inspect("app/Http/F.php", Some(src.into())).unwrap();
        assert_eq!(problems[0].code, "deprecated", "{problems:?}");
        let thing = php.inspect("app/Misc/Thing.php", None).unwrap();
        assert_eq!(thing[0].code, "psr4Namespace", "{thing:?}");
        // Completion from the metadata.
        let at = src.find("('')").unwrap() + 2;
        let c = php
            .complete("app/Http/F.php", at, Some(src.into()))
            .unwrap()
            .unwrap();
        let values: Vec<&str> = c.items.iter().map(|i| i.insert.as_str()).collect();
        assert_eq!(values, vec!["draft", "paid"]);
        let places = php.language_places().unwrap();
        assert_eq!(places[0].name, "check");
        assert_eq!(places[0].language, "SQL");
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
                    shell_integration: false,
                },
                Arc::new(move |e| match e {
                    PtyEvent::Output { data } => out.lock().unwrap().push_str(&data),
                    PtyEvent::Exited { .. } => *ex.lock().unwrap() = true,
                }),
            )
            .unwrap();
        assert_eq!(term.cwd(), watch_root.join("app"));
        term.resize(100, 30).unwrap();
        // The agent's shell gets the same terminal environment as a local one.
        term.write(b"echo \"env:$FORGE_TEST:$TERM_PROGRAM:$TERM_PROGRAM_VERSION\"; exit\n")
            .unwrap();
        let expected = format!("env:remote-shell:Forge:{}", env!("CARGO_PKG_VERSION"));
        wait_until("terminal output", || {
            output.lock().unwrap().contains(&expected)
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn search_index_runs_on_the_agent() {
    let dir = project();
    let root = dir.path().to_string_lossy().into_owned();
    let client = connect();
    let backend =
        blocking(move || RemoteBackend::open(Arc::new(client), host(), &root).unwrap()).await;
    blocking(move || {
        wait_until("the agent's index", || {
            let status = backend.search_status().unwrap();
            status.enabled && !status.indexing && status.files > 0
        });

        let hits = backend.find_symbols("order", true, 5, &[], false).unwrap();
        assert_eq!(hits[0].name, "Order");
        assert_eq!(hits[0].path, "app/Models/Order.php");
        assert_eq!(hits[0].kind, "class");
        assert!(backend
            .find_symbols("order", true, 5, &["php".to_string()], false)
            .unwrap()
            .is_empty());

        let found = backend
            .find_in_files(&forge_core::search::FindRequest {
                query: "shop".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(found.files[0].path, "app/Models/Order.php");

        let paged = backend
            .find_in_files_found(
                &forge_core::search::FindRequest {
                    query: "shop".into(),
                    ..Default::default()
                },
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap();
        let first = paged.paged(7, 1);
        assert_eq!(first.id, 7);
        assert_eq!(first.files[0].path, "app/Models/Order.php");
        assert_eq!(first.file_count, found.files.len());
        assert_eq!(paged.page(0, 1)[0].path, "app/Models/Order.php");
        assert_eq!(
            backend.find_files("order", 5, false).unwrap()[0].name,
            "Order.php"
        );
    })
    .await;
}

fn laravel_project() -> tempfile::TempDir {
    let dir = project();
    let p = dir.path();
    std::fs::write(
        p.join("composer.json"),
        r#"{"require":{"laravel/framework":"^12.0"},"autoload":{"psr-4":{"App\\":"app/"}}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(p.join("routes")).unwrap();
    std::fs::create_dir_all(p.join("app/Http/Controllers")).unwrap();
    std::fs::write(
        p.join("routes/web.php"),
        "<?php\nuse App\\Http\\Controllers\\OrderController;\nRoute::get('/orders/{order}', [OrderController::class, 'show'])->name('orders.show');\n",
    )
    .unwrap();
    std::fs::write(
        p.join("app/Http/Controllers/OrderController.php"),
        "<?php\nnamespace App\\Http\\Controllers;\nclass OrderController\n{\n    public function show() {}\n}\n",
    )
    .unwrap();
    dir
}

async fn open_remote(dir: &tempfile::TempDir) -> Arc<dyn Backend> {
    let root = dir.path().to_string_lossy().into_owned();
    let client = connect();
    Arc::new(blocking(move || RemoteBackend::open(Arc::new(client), host(), &root).unwrap()).await)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn frameworks_are_analysed_on_the_agent() {
    use forge_backend::frameworks::{DoctrineQuery, LaravelRoutes, SymfonyQuery};
    use forge_backend::{framework_call, FrameworkCall};

    let dir = laravel_project();
    let b = open_remote(&dir).await;
    let (info, routes, gutter, symfony, entities, related) = blocking(move || {
        let info: phpforge_fw_laravel::LaravelInfo =
            framework_call(b.as_ref(), FrameworkCall::LaravelDetect).unwrap();
        let routes: LaravelRoutes = framework_call(
            b.as_ref(),
            FrameworkCall::LaravelRoutes {
                source: "static".into(),
                php: "php".into(),
            },
        )
        .unwrap();
        // No buffer: the agent reads the file itself.
        let gutter: Vec<phpforge_fw_laravel::GutterMark> = framework_call(
            b.as_ref(),
            FrameworkCall::LaravelGutter {
                path: "app/Http/Controllers/OrderController.php".into(),
                content: None,
            },
        )
        .unwrap();
        let symfony: Option<phpforge_fw_symfony::SymfonyInfo> = framework_call(
            b.as_ref(),
            FrameworkCall::Symfony {
                config: Default::default(),
                query: SymfonyQuery::Detect,
            },
        )
        .unwrap();
        let entities: Vec<phpforge_orm_doctrine::Entity> = framework_call(
            b.as_ref(),
            FrameworkCall::Doctrine {
                config: Default::default(),
                query: DoctrineQuery::Entities { refresh: true },
            },
        )
        .unwrap();
        // Related Symbol: the agent lists and reads the project's files.
        let related: Vec<forge_codeintel::RelatedItem> = framework_call(
            b.as_ref(),
            FrameworkCall::Related {
                path: "app/Http/Controllers/OrderController.php".into(),
                content: None,
            },
        )
        .unwrap();
        (info, routes, gutter, symfony, entities, related)
    })
    .await;
    assert_eq!(related.len(), 1, "{related:?}");
    assert_eq!(
        (related[0].path.as_str(), related[0].line),
        ("routes/web.php", 2)
    );
    assert!(info.detected);
    assert_eq!(info.routes, 1);
    assert_eq!(routes.routes[0].name.as_deref(), Some("orders.show"));
    let location = routes.routes[0].action_location.clone().unwrap();
    assert_eq!(location.path, "app/Http/Controllers/OrderController.php");
    assert!(gutter.iter().any(|m| m.kind == "route"), "{gutter:?}");
    assert!(symfony.is_none());
    assert!(entities.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn project_settings_and_executables_on_the_host() {
    use forge_core::{ProjectFiles, Settings, SettingsScope};

    let dir = project();
    let b = open_remote(&dir).await;
    let (missing, value, found, custom) = blocking(move || {
        let files: Arc<dyn ProjectFiles> = Arc::new(forge_backend::BackendFiles(b.clone()));
        let missing = files.read(".forge/settings.toml").unwrap();
        let mut settings = Settings::new();
        settings.load_project_files(files.clone()).unwrap();
        settings
            .set(
                "php.interpreter",
                serde_json::json!("/usr/bin/php8.3"),
                SettingsScope::Project,
            )
            .unwrap();
        let mut again = Settings::new();
        again.load_project_files(files).unwrap();
        let value: Option<String> = again.get_as("php.interpreter");
        let found = b.find_executable(&["sh".into()], None).unwrap();
        let custom = b
            .find_executable(&["nope".into()], Some("/definitely/not/here"))
            .unwrap();
        (missing, value, found, custom)
    })
    .await;
    assert_eq!(missing, None);
    assert_eq!(value.as_deref(), Some("/usr/bin/php8.3"));
    let text = std::fs::read_to_string(dir.path().join(".forge/settings.toml")).unwrap();
    assert!(text.contains("interpreter = \"/usr/bin/php8.3\""), "{text}");
    assert!(found.unwrap().ends_with("sh"));
    assert_eq!(custom, None);
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn process_stdio_streams_and_end_of_input() {
    use tokio::io::AsyncBufReadExt;

    let dir = project();
    let backend = open_remote(&dir).await;
    // The backend routes the output; it must outlive the process.
    let b = backend.clone();
    let cwd = dir.path().to_path_buf();
    let mut p = blocking(move || {
        let mut cmd = CommandLine::new("sh").args(["-c", "cat; echo done"]);
        cmd.cwd = cwd;
        forge_backend::process_io::spawn_stdio(b.as_ref(), &cmd).unwrap()
    })
    .await;
    p.stdin.write_all(b"prompt line\n").await.unwrap();
    // `cat` only ends at the end of its input.
    p.stdin.shutdown().await.unwrap();
    let mut lines = tokio::io::BufReader::new(p.stdout).lines();
    assert_eq!(
        lines.next_line().await.unwrap().as_deref(),
        Some("prompt line")
    );
    assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("done"));
    assert_eq!(
        tokio::time::timeout(T, p.exit).await.unwrap().unwrap(),
        Some(0)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn catalog_language_servers_are_planned_on_the_host() {
    let dir = project();
    std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();
    std::fs::write(dir.path().join("app.ts"), "export const a = 1;\n").unwrap();
    let b = open_remote(&dir).await;
    let answer = blocking(move || {
        b.langservers(forge_backend::LangServersRequest {
            rescan: true,
            ..Default::default()
        })
        .unwrap()
    })
    .await;
    let lua = answer
        .servers
        .iter()
        .find(|s| s.id == "lua-language-server")
        .unwrap();
    assert_eq!(lua.skip, Some(forge_langservers::Skip::NotInProject));
    let ts = answer
        .servers
        .iter()
        .find(|s| s.id == "typescript-language-server")
        .unwrap();
    // TypeScript occurs in the project; whether it starts depends on the host.
    assert_ne!(ts.skip, Some(forge_langservers::Skip::NotInProject));
    assert_eq!(answer.platform, "linux");
    assert!(answer
        .rows
        .iter()
        .any(|r| r.id == "typescript-language-server"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn webforge_is_analysed_on_the_agent() {
    use forge_backend::{web_call, WebCall};
    use webforge_lang_web::{Component, GutterMark, Location, NpmPackage};

    let dir = project();
    let p = dir.path();
    std::fs::write(
        p.join("package.json"),
        r#"{"name":"shop","scripts":{"dev":"vite","build":"vite build"},"dependencies":{"vue":"^3"}}"#,
    )
    .unwrap();
    std::fs::write(p.join("package-lock.json"), "{}").unwrap();
    std::fs::create_dir_all(p.join("src/components")).unwrap();
    let card = "<script setup lang=\"ts\">\ndefineProps<{ total: number }>()\n</script>\n<template><div>{{ total }}</div></template>\n";
    std::fs::write(p.join("src/components/OrderCard.vue"), card).unwrap();
    let app = "<script setup>\nimport OrderCard from './components/OrderCard.vue'\n</script>\n<template><OrderCard :total=\"1\" /></template>\n";
    std::fs::write(p.join("src/App.vue"), app).unwrap();
    let b = open_remote(&dir).await;
    let (packages, components, goto, gutter) = blocking(move || {
        let packages: Vec<NpmPackage> = web_call(
            b.as_ref(),
            WebCall::NpmPackages {
                refresh: true,
                package_manager: "auto".into(),
            },
        )
        .unwrap();
        let components: Vec<Component> = web_call(
            b.as_ref(),
            WebCall::Components {
                framework: "vue".into(),
            },
        )
        .unwrap();
        let offset = app.find("<OrderCard").unwrap() + 2;
        let goto: Vec<Location> = web_call(
            b.as_ref(),
            WebCall::Goto {
                framework: "vue".into(),
                path: "src/App.vue".into(),
                content: app.into(),
                offset,
            },
        )
        .unwrap();
        let gutter: Vec<GutterMark> = web_call(
            b.as_ref(),
            WebCall::Gutter {
                framework: "vue".into(),
                path: "src/components/OrderCard.vue".into(),
                content: card.into(),
            },
        )
        .unwrap();
        (packages, components, goto, gutter)
    })
    .await;
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].package_manager.manager.id(), "npm");
    let scripts: Vec<&str> = packages[0]
        .scripts
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(scripts, vec!["dev", "build"]);
    assert!(
        components.iter().any(|c| c.name == "OrderCard"),
        "{components:?}"
    );
    assert!(
        goto.iter()
            .any(|l| l.path == "src/components/OrderCard.vue"),
        "{goto:?}"
    );
    // Line markers come from the host as well.
    assert!(gutter.iter().all(|m| !m.title.is_empty()), "{gutter:?}");
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn filters_like_stylua_run_on_the_host() {
    use std::os::unix::fs::PermissionsExt;

    let dir = project();
    // A stand-in for `stylua -`: upper-cases its input.
    let fake = dir.path().join("fake-stylua");
    std::fs::write(&fake, "#!/bin/sh\ntr a-z A-Z\n").unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let b = open_remote(&dir).await;
    let root = b.info().root.clone();
    let out = blocking(move || {
        let exe = b
            .find_executable(&["stylua".into()], Some(&fake.to_string_lossy()))
            .unwrap()
            .unwrap();
        let mut cmd = CommandLine::new(exe.to_string_lossy()).arg("-");
        cmd.cwd = root;
        forge_backend::process_io::capture(b.as_ref(), &cmd, "local x = 1\n", T).unwrap()
    })
    .await;
    assert_eq!(out.code, Some(0));
    assert_eq!(out.stdout, "LOCAL X = 1\n");
}
