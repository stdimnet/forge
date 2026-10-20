//! Running an install command (`npm install -g …`, `pipx install …`) with streamed
//! output. Commands come from the catalog only and run without a shell.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, Notify};

use crate::catalog::InstallOption;
use crate::detect::Detector;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stream {
    Stdout,
    Stderr,
    /// Lines written by Forge itself (the command line, the result).
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallOutcome {
    Succeeded,
    Failed { code: Option<i32> },
    Cancelled,
}

/// The program and arguments to run: the program is resolved like a language server
/// (npm, pipx and go are often outside a desktop session's `PATH`).
pub fn resolve_command(
    option: &InstallOption,
    detector: &Detector,
) -> Result<(PathBuf, Vec<String>), String> {
    if !option.runnable() {
        return Err(format!(
            "{} commands are not run by Forge; run it in a terminal",
            option.tool
        ));
    }
    let (program, args) = option
        .command
        .split_first()
        .ok_or_else(|| "empty install command".to_string())?;
    let path = detector.find(program).ok_or_else(|| {
        format!("{program} was not found. Install it first or run the command in a terminal.")
    })?;
    Ok((path, args.to_vec()))
}

/// Run `program args…`, sending every output line to `on_line`. `cancel` stops it.
pub async fn run(
    program: PathBuf,
    args: Vec<String>,
    extra_path: Vec<PathBuf>,
    cancel: Arc<Notify>,
    mut on_line: impl FnMut(Stream, String),
) -> InstallOutcome {
    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // The tool's own directory first, so npm finds node next to it.
    let mut path_dirs: Vec<PathBuf> = program
        .parent()
        .map(|p| vec![p.to_path_buf()])
        .unwrap_or_default();
    path_dirs.extend(extra_path);
    if let Some(current) = std::env::var_os("PATH") {
        path_dirs.extend(std::env::split_paths(&current));
    }
    if let Ok(joined) = std::env::join_paths(path_dirs) {
        cmd.env("PATH", joined);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            on_line(
                Stream::Info,
                format!("Could not start {}: {e}", program.display()),
            );
            return InstallOutcome::Failed { code: None };
        }
    };
    let (tx, mut rx) = mpsc::unbounded_channel::<(Stream, String)>();
    if let Some(out) = child.stdout.take() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let _ = tx.send((Stream::Stdout, l));
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let _ = tx.send((Stream::Stderr, l));
            }
        });
    }
    drop(tx);
    let status = loop {
        tokio::select! {
            Some((stream, line)) = rx.recv() => on_line(stream, line),
            status = child.wait() => break status,
            _ = cancel.notified() => {
                let _ = child.kill().await;
                while let Ok((stream, line)) = rx.try_recv() {
                    on_line(stream, line);
                }
                return InstallOutcome::Cancelled;
            }
        }
    };
    // Drain what the readers still hold.
    while let Some((stream, line)) = rx.recv().await {
        on_line(stream, line);
    }
    match status {
        Ok(s) if s.success() => InstallOutcome::Succeeded,
        Ok(s) => InstallOutcome::Failed { code: s.code() },
        Err(e) => {
            on_line(Stream::Info, format!("{e}"));
            InstallOutcome::Failed { code: None }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::detect::DetectEnv;

    #[tokio::test]
    async fn streams_output_and_reports_the_exit_code() {
        let mut lines = Vec::new();
        let outcome = run(
            PathBuf::from("/bin/sh"),
            vec![
                "-c".into(),
                "echo added 1 package; echo warn >&2; exit 3".into(),
            ],
            vec![],
            Arc::new(Notify::new()),
            |s, l| lines.push((s, l)),
        )
        .await;
        assert_eq!(outcome, InstallOutcome::Failed { code: Some(3) });
        assert!(lines.contains(&(Stream::Stdout, "added 1 package".into())));
        assert!(lines.contains(&(Stream::Stderr, "warn".into())));

        let ok = run(
            PathBuf::from("/bin/sh"),
            vec!["-c".into(), "true".into()],
            vec![],
            Arc::new(Notify::new()),
            |_, _| {},
        )
        .await;
        assert_eq!(ok, InstallOutcome::Succeeded);
    }

    #[tokio::test]
    async fn can_be_cancelled() {
        let cancel = Arc::new(Notify::new());
        let c = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            c.notify_one();
        });
        let started = std::time::Instant::now();
        let outcome = run(
            PathBuf::from("/bin/sh"),
            vec!["-c".into(), "sleep 20".into()],
            vec![],
            cancel,
            |_, _| {},
        )
        .await;
        assert_eq!(outcome, InstallOutcome::Cancelled);
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    #[test]
    fn only_runnable_options_with_an_installed_tool_resolve() {
        let tmp = tempfile::tempdir().unwrap();
        crate::detect::tests::fake_bin(&tmp.path().join("npm"));
        let d = Detector::new(DetectEnv {
            path: Some(tmp.path().as_os_str().to_owned()),
            ..Default::default()
        });
        let npm = InstallOption {
            tool: "npm".into(),
            command: vec!["npm".into(), "install".into(), "-g".into(), "x".into()],
            url: None,
            platforms: vec![],
        };
        let (program, args) = resolve_command(&npm, &d).unwrap();
        assert_eq!(program, tmp.path().join("npm"));
        assert_eq!(args, vec!["install", "-g", "x"]);

        let pipx = InstallOption {
            tool: "pipx".into(),
            command: vec!["pipx".into()],
            ..npm.clone()
        };
        assert!(resolve_command(&pipx, &d)
            .unwrap_err()
            .contains("pipx was not found"));
        let pacman = InstallOption {
            tool: "pacman".into(),
            command: vec!["sudo".into(), "pacman".into()],
            ..npm
        };
        assert!(resolve_command(&pacman, &d)
            .unwrap_err()
            .contains("terminal"));
    }
}
