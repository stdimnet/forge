//! A host on a remote SSH project can share it with Code With Me guests: the host's backend
//! is a `RemoteBackend` (here over an in-memory pipe to the agent) and guests browse, read,
//! search and see git through it, with the same exclusions as for a local project.

use std::sync::Arc;
use std::time::Duration;

use forge_backend::rpc::RpcClient;
use forge_backend::{Backend, RemoteBackend, RemoteHost};
use forge_collab::excludes::Excludes;
use forge_collab::guest::{GuestEvent, GuestSession};
use forge_collab::guest_backend::CollabBackend;
use forge_collab::host::{DevAutoAccept, HostConfig, HostSession};
use forge_collab::permissions::Permissions;
use forge_collab::transport::memory::MemoryNetwork;
use forge_core::search::FindRequest;

fn git(dir: &std::path::Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .args(["-c", "commit.gpgsign=false"])
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
async fn a_host_on_an_ssh_project_shares_it_with_guests() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::write(root.join("app/Order.php"), "<?php\nclass Order {}\n").unwrap();
    std::fs::write(root.join(".env"), "APP_KEY=secret\n").unwrap();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["add", "-A", "-f"]);
    git(root, &["commit", "-q", "-m", "Initial"]);
    std::fs::write(root.join("app/Order.php"), "<?php\nclass Order { }\n").unwrap();
    std::fs::write(root.join(".env"), "APP_KEY=rotated\n").unwrap();

    // The host's project is on the agent, reached like over SSH.
    let (client_end, agent_end) = tokio::io::duplex(1 << 16);
    let (agent_read, agent_write) = tokio::io::split(agent_end);
    tokio::spawn(forge_remote_agent::serve(agent_read, agent_write));
    let (read, write) = tokio::io::split(client_end);
    let client = RpcClient::spawn(read, write);
    let root_str = root.to_string_lossy().into_owned();
    let remote = tokio::task::spawn_blocking(move || {
        RemoteBackend::open(
            Arc::new(client),
            RemoteHost::ssh("shop.example.com", 22, "deploy"),
            &root_str,
        )
    })
    .await
    .unwrap()
    .unwrap();
    let backend: Arc<dyn Backend> = Arc::new(remote);

    let net = MemoryNetwork::new();
    let host = HostSession::start(
        Arc::new(net.endpoint([1; 32])),
        backend,
        HostConfig {
            host_name: "Robert".into(),
            machine: None,
            permissions: Permissions::READ_ONLY,
            excludes: Excludes::defaults(),
            protected: Excludes::protected_defaults(),
            max_file_size: 1 << 20,
            notice: None,
        },
        Arc::new(DevAutoAccept),
        Arc::new(|_| {}),
    );
    let guest = GuestSession::join(
        Arc::new(net.endpoint([2; 32])),
        &host.link(),
        "Alex",
        Arc::new(|_: GuestEvent| {}),
    )
    .await
    .unwrap();
    let session = guest.clone();
    let view = tokio::task::spawn_blocking(move || CollabBackend::open(session))
        .await
        .unwrap()
        .unwrap();
    let view = Arc::new(view);
    assert!(view.info().remote.is_none() && view.info().collab.is_some());

    let b = view.clone();
    let listed = tokio::task::spawn_blocking(move || b.list_dir("").unwrap())
        .await
        .unwrap();
    let names: Vec<&str> = listed.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"app"), "{names:?}");
    assert!(!names.contains(&".env"), "{names:?}");

    let b = view.clone();
    let (text, env, hits, status) = tokio::task::spawn_blocking(move || {
        let text = b.read_file("app/Order.php").unwrap();
        let env = b.read_file(".env");
        let hits = b
            .find_in_files(&FindRequest {
                query: "rotated|secret|class Order".into(),
                regex: true,
                ..FindRequest::default()
            })
            .unwrap();
        let status = b.repo().unwrap().status().unwrap();
        (text, env, hits, status)
    })
    .await
    .unwrap();
    assert_eq!(text, "<?php\nclass Order { }\n");
    assert!(env.is_err());
    assert_eq!(hits.files.len(), 1);
    assert_eq!(hits.files[0].path, "app/Order.php");
    let paths: Vec<&str> = status.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        ["app/Order.php"],
        "the hidden .env is not in the status"
    );

    // Read-only: a write is refused on the host before it reaches the agent.
    let b = view.clone();
    let r = tokio::task::spawn_blocking(move || b.write_file("app/Order.php", "x"))
        .await
        .unwrap();
    assert!(r.is_err());
    assert_eq!(
        std::fs::read_to_string(root.join("app/Order.php")).unwrap(),
        "<?php\nclass Order { }\n"
    );
    tokio::time::sleep(Duration::from_millis(10)).await;
}
