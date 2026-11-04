//! Agent Tasks of a remote project: the sandbox is a git worktree in the host's cache
//! folder, created, read, changed, applied and removed through the agent, and the task's
//! commands run there. Driven in-process through `RemoteBackend` over an in-memory pipe.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use forge_agent_tasks::remote::{
    ApplyAnswer, ApplyMode, FileVersions, SandboxCall, SandboxInfo, SandboxRef, ToolReply, BASE_ENV,
};
use forge_agent_tasks::sandbox::FileChange;
use forge_agent_tasks::tools::ToolCall;
use forge_backend::agent_tasks::{run_to_end, sandbox_call};
use forge_backend::rpc::RpcClient;
use forge_backend::{Backend, RemoteBackend, RemoteHost};
use forge_process::CommandLine;

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

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.unwrap()
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

fn call<T: serde::de::DeserializeOwned + Send + 'static>(
    b: &Arc<dyn Backend>,
    c: SandboxCall,
) -> impl std::future::Future<Output = forge_core::Result<T>> {
    let b = b.clone();
    blocking(move || sandbox_call::<T>(b.as_ref(), c))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sandboxes_live_and_run_on_the_host() {
    // The host's cache folder (this test binary's own process).
    let cache = tempfile::tempdir().unwrap();
    std::env::set_var(BASE_ENV, cache.path());

    let project = tempfile::tempdir().unwrap();
    let p = project.path();
    git(p, &["init", "--quiet", "-b", "main"]);
    // A committer for Apply as Commit (this repository only).
    let config = p.join(".git/config");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str("[user]\n\tname = t\n\temail = t@example.com\n[commit]\n\tgpgsign = false\n");
    std::fs::write(&config, text).unwrap();
    std::fs::write(p.join("sum.sh"), "echo $((2 - 3))\n").unwrap();
    std::fs::write(p.join(".env"), "SECRET=1\n").unwrap();
    std::fs::write(p.join(".aiignore"), ".env\n").unwrap();
    git(p, &["add", "-A"]);
    git(p, &["commit", "--quiet", "-m", "init"]);
    // Uncommitted work is part of the sandbox.
    std::fs::write(p.join("notes.md"), "draft\n").unwrap();

    let root = p.to_string_lossy().into_owned();
    let client = connect();
    let backend: Arc<dyn Backend> = Arc::new(
        blocking(move || RemoteBackend::open(Arc::new(client), host(), &root).unwrap()).await,
    );

    let info: SandboxInfo = call(&backend, SandboxCall::Create { id: "t1".into() })
        .await
        .unwrap();
    assert!(Path::new(&info.dir).starts_with(cache.path()));
    assert!(Path::new(&info.dir).join("notes.md").is_file());
    let sb = SandboxRef {
        dir: info.dir.clone(),
        base: info.base.clone(),
    };

    // The built-in agent's file tools run on the host, with .aiignore and the scope.
    let tool = |c: ToolCall, scope: Vec<String>| {
        call::<ToolReply>(
            &backend,
            SandboxCall::Tool {
                sandbox: sb.clone(),
                scope,
                call: c,
            },
        )
    };
    let r = tool(
        ToolCall::EditFile {
            path: "sum.sh".into(),
            old: "2 - 3".into(),
            new: "2 + 3".into(),
        },
        Vec::new(),
    )
    .await
    .unwrap();
    assert!(r.ok, "{r:?}");
    let r = tool(
        ToolCall::ReadFile {
            path: ".env".into(),
            start_line: None,
            end_line: None,
        },
        Vec::new(),
    )
    .await
    .unwrap();
    assert!(!r.ok);
    let r = tool(
        ToolCall::WriteFile {
            path: "other.txt".into(),
            content: "x".into(),
        },
        vec!["src".into()],
    )
    .await
    .unwrap();
    assert!(!r.ok && r.output.contains("scope"), "{r:?}");

    // Commands and tests run in the sandbox on the host.
    let mut cmd = CommandLine::new("sh").args(["sum.sh"]);
    cmd.cwd = info.project_dir.clone().into();
    let out = run_to_end(
        backend.clone(),
        cmd,
        Duration::from_secs(20),
        std::future::pending(),
    )
    .await;
    assert_eq!(out.exit_code, Some(0), "{out:?}");
    assert_eq!(out.output.trim(), "5");
    let mut slow = CommandLine::new("sleep").args(["10"]);
    slow.cwd = info.project_dir.clone().into();
    let out = run_to_end(
        backend.clone(),
        slow,
        Duration::from_millis(200),
        std::future::pending(),
    )
    .await;
    assert!(out.timed_out);

    // The review: changes and both sides of a file.
    let changes: Vec<FileChange> = call(
        &backend,
        SandboxCall::Changes {
            sandbox: sb.clone(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        changes.iter().map(|c| c.path.as_str()).collect::<Vec<_>>(),
        ["sum.sh"]
    );
    let v: FileVersions = call(
        &backend,
        SandboxCall::FileVersions {
            sandbox: sb.clone(),
            path: "sum.sh".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(v.before.as_deref(), Some("echo $((2 - 3))\n"));
    assert_eq!(v.after.as_deref(), Some("echo $((2 + 3))\n"));
    // The project is untouched until the changes are applied.
    assert_eq!(
        std::fs::read_to_string(p.join("sum.sh")).unwrap(),
        "echo $((2 - 3))\n"
    );

    // Other folders are refused.
    let outside = call::<Vec<FileChange>>(
        &backend,
        SandboxCall::Changes {
            sandbox: SandboxRef {
                dir: p.to_string_lossy().into_owned(),
                base: info.base.clone(),
            },
        },
    )
    .await;
    assert!(outside.unwrap_err().to_string().contains("not a sandbox"));

    let applied: ApplyAnswer = call(
        &backend,
        SandboxCall::Apply {
            sandbox: sb.clone(),
            mode: ApplyMode::Commit {
                message: "Fix the sum".into(),
            },
            paths: Vec::new(),
        },
    )
    .await
    .unwrap();
    assert!(applied.result.applied, "{applied:?}");
    assert!(applied.created.is_some());
    assert_eq!(
        std::fs::read_to_string(p.join("sum.sh")).unwrap(),
        "echo $((2 + 3))\n"
    );

    call::<()>(
        &backend,
        SandboxCall::Remove {
            dir: info.dir.clone(),
        },
    )
    .await
    .unwrap();
    assert!(!Path::new(&info.dir).exists());
}
