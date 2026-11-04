//! `docker compose …`, `docker build` / `docker run` (Dockerfile ▶) and other CLI runs with
//! their output streamed line by line. Compose has no Engine API, so these are processes;
//! everything else talks to the Engine directly ([`crate::engine`]).

use std::path::Path;
use std::process::Stdio;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::mpsc;

use crate::logs::LogStream;
use crate::{Error, Result};

/// Run `program args…` in `cwd` and send its output to `sink` as it arrives. Returns the
/// exit code (`None` when killed by a signal). Dropping the future kills the process.
pub async fn run_streaming(
    program: &str,
    args: &[String],
    env: &[(String, String)],
    cwd: &Path,
    mut sink: impl FnMut(LogStream, String) + Send,
) -> Result<Option<i32>> {
    let mut std_cmd = std::process::Command::new(program);
    std_cmd
        .args(args)
        .envs(env.iter().cloned())
        // Plain progress output instead of TTY animations.
        .env("COMPOSE_PROGRESS", "plain")
        .env("BUILDKIT_PROGRESS", "plain")
        .current_dir(cwd);
    // In Flatpak the Docker / Podman / kubectl CLI runs on the host.
    let mut cmd: tokio::process::Command = forge_hostspawn::host_command(std_cmd).into();
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error(format!("`{program}` was not found. Install the Docker CLI or set its path in Settings | Build, Execution, Deployment | Docker."))
        } else {
            Error(format!("Cannot start `{program}`: {e}"))
        }
    })?;
    let (tx, mut rx) = mpsc::unbounded_channel::<(LogStream, String)>();
    fn pump(
        mut r: impl AsyncRead + Unpin + Send + 'static,
        stream: LogStream,
        tx: mpsc::UnboundedSender<(LogStream, String)>,
    ) {
        tokio::spawn(async move {
            let mut buf = vec![0u8; 8192];
            let mut pending = Vec::new();
            loop {
                match r.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        pending.extend_from_slice(&buf[..n]);
                        let valid = match std::str::from_utf8(&pending) {
                            Ok(_) => pending.len(),
                            Err(e) if e.error_len().is_none() => e.valid_up_to(),
                            Err(_) => pending.len(),
                        };
                        let rest = pending.split_off(valid);
                        let text = String::from_utf8_lossy(&pending).into_owned();
                        pending = rest;
                        if tx.send((stream, text)).is_err() {
                            break;
                        }
                    }
                }
            }
            if !pending.is_empty() {
                let _ = tx.send((stream, String::from_utf8_lossy(&pending).into_owned()));
            }
        });
    }
    if let Some(out) = child.stdout.take() {
        pump(out, LogStream::Stdout, tx.clone());
    }
    if let Some(err) = child.stderr.take() {
        pump(err, LogStream::Stderr, tx.clone());
    }
    drop(tx);
    while let Some((stream, text)) = rx.recv().await {
        sink(stream, text);
    }
    let status = child.wait().await?;
    Ok(status.code())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn streams_both_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let mut out = String::new();
        let mut err = String::new();
        let code = run_streaming(
            "sh",
            &["-c".into(), "echo up; echo warn >&2; pwd; exit 3".into()],
            &[],
            dir.path(),
            |s, t| match s {
                LogStream::Stdout => out.push_str(&t),
                LogStream::Stderr => err.push_str(&t),
            },
        )
        .await
        .unwrap();
        assert_eq!(code, Some(3));
        assert!(out.starts_with("up\n"));
        assert_eq!(err, "warn\n");
    }

    #[tokio::test]
    async fn missing_program() {
        let e = run_streaming("forge-no-such-docker", &[], &[], Path::new("."), |_, _| {})
            .await
            .err()
            .unwrap();
        assert!(e.0.contains("was not found"));
    }
}
