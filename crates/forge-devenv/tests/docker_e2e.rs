//! End to end against a real Docker Engine (skipped unless asked for): build a dev container
//! from a temporary project, start the Forge agent in it over `docker exec` and open the
//! project through a `RemoteBackend`.
//!
//! ```sh
//! pnpm --dir app/ui agents x86_64   # static agent for the container
//! FORGE_DOCKER_E2E=1 FORGE_REMOTE_AGENT_DIR=target/agents \
//!   cargo test -p forge-devenv --test docker_e2e -- --ignored --nocapture
//! ```

use std::sync::Arc;

use forge_backend::remote::Connection;
use forge_backend::{Backend, RemoteBackend, RemoteHost, RemoteKind};
use forge_devenv::agent::{start_agent, DockerTarget};
use forge_devenv::connection::{Starter, StdioConnection};
use forge_devenv::devcontainer::{load, LocalContext};
use forge_devenv::docker;
use forge_devenv::runner::{Runner, SystemRunner};
use forge_devenv::up::{up, UpOptions};
use forge_remote::agent::AgentBinaries;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Docker; set FORGE_DOCKER_E2E=1"]
async fn dev_container_end_to_end() {
    if std::env::var_os("FORGE_DOCKER_E2E").is_none() {
        eprintln!("skipped: set FORGE_DOCKER_E2E=1");
        return;
    }
    let project = tempfile::tempdir().unwrap();
    let folder = project.path().join("shop");
    std::fs::create_dir_all(folder.join(".devcontainer")).unwrap();
    std::fs::write(folder.join("index.php"), "<?php echo 'hello';\n").unwrap();
    let config_path = folder.join(".devcontainer/devcontainer.json");
    std::fs::write(
        &config_path,
        r#"{
            // a small Linux with a shell is enough for the agent
            "name": "Forge E2E",
            "image": "alpine:3.20",
            "containerEnv": { "FORGE_E2E": "1" },
            "postCreateCommand": "touch /tmp/forge-created",
            "postStartCommand": ["sh", "-c", "echo started > /tmp/forge-started"],
        }"#,
    )
    .unwrap();
    let folder_text = folder.to_string_lossy().into_owned();
    let config_text = config_path.to_string_lossy().into_owned();
    let env = |n: &str| std::env::var(n).ok();
    let cfg = load(
        &std::fs::read_to_string(&config_path).unwrap(),
        &LocalContext {
            folder: &folder_text,
            config_path: &config_text,
            env: &env,
        },
    )
    .unwrap();

    let cache = tempfile::tempdir().unwrap();
    let opts = UpOptions {
        docker: "docker".into(),
        rebuild: false,
        cache_dir: cache.path().to_path_buf(),
    };
    let log = |line: forge_devenv::up::LogLine| eprintln!("{line:?}");
    let outcome = up(&SystemRunner, &cfg, &opts, &log)
        .await
        .expect("docker up");
    assert!(outcome.created);
    let container = outcome.container_id.clone();
    let cleanup = Cleanup(container.clone());

    let check = SystemRunner
        .output(
            "docker",
            &docker::exec_args(
                &container,
                None,
                None,
                &Default::default(),
                false,
                &[
                    "sh".into(),
                    "-c".into(),
                    "test -f /tmp/forge-created && cat /tmp/forge-started && echo $FORGE_E2E"
                        .into(),
                ],
            ),
            None,
        )
        .await
        .unwrap();
    assert_eq!(check.stdout_text(), "started\n1\n", "{}", check.message());

    // Starting again finds the running container.
    let again = up(&SystemRunner, &cfg, &opts, &|_| {}).await.unwrap();
    assert_eq!(again.container_id, container);
    assert!(!again.created);

    let target = Arc::new(DockerTarget {
        docker: "docker".into(),
        container: container.clone(),
        user: cfg.remote_user.clone(),
        workdir: Some(cfg.workspace_folder.clone()),
        env: cfg.remote_env.clone(),
        name: cfg.name.clone(),
    });
    let binaries = AgentBinaries::discover(None);
    let starter: Starter = Arc::new(move || {
        let (target, binaries) = (target.clone(), binaries.clone());
        Box::pin(async move {
            start_agent(&SystemRunner, target.as_ref(), &binaries, &|m: &str| {
                eprintln!("{m}")
            })
            .await
        })
    });
    let conn = StdioConnection::open("Dev Container: Forge E2E", starter)
        .await
        .expect("agent in the container");
    let hello = conn.hello().unwrap();
    assert_eq!(hello.os, "linux");
    let host = RemoteHost {
        host: cfg.name.clone(),
        port: 0,
        user: "root".into(),
        config_id: None,
        kind: RemoteKind::DevContainer {
            container: container.clone(),
            source: folder_text.clone(),
            config: config_text.clone(),
        },
    };
    let source: Arc<dyn Connection> = conn.clone();
    let workspace = cfg.workspace_folder.clone();
    let backend =
        tokio::task::spawn_blocking(move || RemoteBackend::open(source, host, &workspace))
            .await
            .unwrap()
            .expect("open the project in the container");
    let (text, entries) = tokio::task::spawn_blocking(move || {
        (
            backend.read_file("index.php").unwrap(),
            backend.list_dir("").unwrap(),
        )
    })
    .await
    .unwrap();
    assert!(text.contains("hello"));
    assert!(entries.iter().any(|e| e.name == ".devcontainer"));
    conn.close().await;
    drop(cleanup);
}

/// Removes the container at the end, also when the test fails.
struct Cleanup(String);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", &self.0])
            .output();
    }
}
