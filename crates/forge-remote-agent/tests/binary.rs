//! The real `forge-remote-agent` binary over its stdin/stdout, as the IDE starts it over SSH.

use std::process::Stdio;
use std::time::Duration;

use forge_backend::protocol::{Call, Hello, Opened, PROTOCOL_VERSION};
use forge_backend::rpc::RpcClient;
use forge_core::FileEntry;

const AGENT: &str = env!("CARGO_BIN_EXE_forge-remote-agent");
const T: Duration = Duration::from_secs(20);

#[test]
fn prints_its_version() {
    let out = std::process::Command::new(AGENT)
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        forge_remote_agent::version_line()
    );
}

#[tokio::test]
async fn serves_the_protocol_on_stdio_and_exits_with_its_input() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.php"), "<?php\n").unwrap();
    let mut child = tokio::process::Command::new(AGENT)
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let client = RpcClient::spawn(child.stdout.take().unwrap(), child.stdin.take().unwrap());

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
    assert_eq!(hello.version, forge_remote_agent::VERSION);
    let opened: Opened = client
        .call(
            Call::Open {
                root: dir.path().to_string_lossy().into(),
            },
            T,
        )
        .await
        .unwrap();
    assert_eq!(opened.root, dir.path().canonicalize().unwrap());
    let entries: Vec<FileEntry> = client
        .call(Call::ListDir { path: "".into() }, T)
        .await
        .unwrap();
    assert_eq!(entries[0].name, "index.php");

    // Closing the connection ends the agent.
    client.shutdown();
    let status = tokio::time::timeout(T, child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(status.success());
}
