//! Processes on this machine (`docker`, `wsl.exe`) behind a trait, so the dev container
//! and WSL flows are tested with fakes instead of a real Docker or WSL.

use std::future::Future;
use std::pin::Pin;
use std::process::Stdio;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Output of a finished process.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    /// Exit code (`None` when killed by a signal).
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Output {
    pub fn ok(stdout: impl Into<Vec<u8>>) -> Self {
        Self {
            code: Some(0),
            stdout: stdout.into(),
            stderr: Vec::new(),
        }
    }

    pub fn failed(code: i32, stderr: impl Into<Vec<u8>>) -> Self {
        Self {
            code: Some(code),
            stdout: Vec::new(),
            stderr: stderr.into(),
        }
    }

    pub fn success(&self) -> bool {
        self.code == Some(0)
    }

    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }

    /// stderr, else stdout, trimmed (for error messages).
    pub fn message(&self) -> String {
        let e = self.stderr_text();
        if e.trim().is_empty() {
            self.stdout_text().trim().to_string()
        } else {
            e.trim().to_string()
        }
    }
}

/// Which output a log chunk came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stream {
    Stdout,
    Stderr,
}

/// Where streamed output goes.
pub type LogSink<'a> = &'a (dyn Fn(Stream, &str) + Send + Sync);

/// A process speaking on its stdin/stdout (the Forge agent). Dropping it kills the process.
pub struct StdioProcess {
    pub stdout: Box<dyn AsyncRead + Send + Unpin>,
    pub stdin: Box<dyn AsyncWrite + Send + Unpin>,
    /// Keeps the process alive; dropping it ends the process.
    pub guard: Box<dyn Send + Sync>,
}

/// Starts programs on this machine.
pub trait Runner: Send + Sync {
    /// Run to completion, optionally writing `stdin` first.
    fn output<'a>(
        &'a self,
        program: &'a str,
        args: &'a [String],
        stdin: Option<Vec<u8>>,
    ) -> BoxFuture<'a, std::io::Result<Output>>;

    /// Run to completion, sending output to `sink` as it arrives. Dropping the future
    /// stops the process.
    fn stream<'a>(
        &'a self,
        program: &'a str,
        args: &'a [String],
        sink: LogSink<'a>,
    ) -> BoxFuture<'a, std::io::Result<Option<i32>>>;

    /// Start with piped stdin/stdout (stderr goes to the log).
    fn spawn_stdio(&self, program: &str, args: &[String]) -> std::io::Result<StdioProcess>;
}

/// Real processes (`tokio::process`).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemRunner;

fn command(program: &str, args: &[String]) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args)
        // Plain progress output instead of TTY animations in the build log.
        .env("BUILDKIT_PROGRESS", "plain")
        .env("COMPOSE_PROGRESS", "plain")
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        // No console window for docker.exe / wsl.exe.
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

fn spawn_error(program: &str, e: std::io::Error) -> std::io::Error {
    if e.kind() == std::io::ErrorKind::NotFound {
        std::io::Error::new(
            e.kind(),
            format!("`{program}` was not found. Install it or set its path in the settings."),
        )
    } else {
        std::io::Error::new(e.kind(), format!("Cannot start `{program}`: {e}"))
    }
}

/// Send what `reader` produces to `tx` as text chunks (UTF-8 sequences kept whole).
fn pump(
    mut reader: impl AsyncRead + Unpin + Send + 'static,
    stream: Stream,
    tx: mpsc::UnboundedSender<(Stream, String)>,
) {
    tokio::spawn(async move {
        let mut buf = vec![0u8; 8192];
        let mut pending = Vec::new();
        loop {
            match reader.read(&mut buf).await {
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

impl Runner for SystemRunner {
    fn output<'a>(
        &'a self,
        program: &'a str,
        args: &'a [String],
        stdin: Option<Vec<u8>>,
    ) -> BoxFuture<'a, std::io::Result<Output>> {
        Box::pin(async move {
            let mut cmd = command(program, args);
            cmd.stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
            let mut child = cmd.spawn().map_err(|e| spawn_error(program, e))?;
            if let (Some(bytes), Some(mut pipe)) = (stdin, child.stdin.take()) {
                // Write while the output is read, so a large input cannot dead-lock.
                tokio::spawn(async move {
                    let _ = pipe.write_all(&bytes).await;
                    let _ = pipe.shutdown().await;
                });
            }
            let out = child.wait_with_output().await?;
            Ok(Output {
                code: out.status.code(),
                stdout: out.stdout,
                stderr: out.stderr,
            })
        })
    }

    fn stream<'a>(
        &'a self,
        program: &'a str,
        args: &'a [String],
        sink: LogSink<'a>,
    ) -> BoxFuture<'a, std::io::Result<Option<i32>>> {
        Box::pin(async move {
            let mut cmd = command(program, args);
            cmd.stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let mut child = cmd.spawn().map_err(|e| spawn_error(program, e))?;
            let (tx, mut rx) = mpsc::unbounded_channel();
            if let Some(out) = child.stdout.take() {
                pump(out, Stream::Stdout, tx.clone());
            }
            if let Some(err) = child.stderr.take() {
                pump(err, Stream::Stderr, tx.clone());
            }
            drop(tx);
            while let Some((stream, text)) = rx.recv().await {
                sink(stream, &text);
            }
            Ok(child.wait().await?.code())
        })
    }

    fn spawn_stdio(&self, program: &str, args: &[String]) -> std::io::Result<StdioProcess> {
        let mut cmd = command(program, args);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| spawn_error(program, e))?;
        let stdout = child.stdout.take().expect("piped stdout");
        let stdin = child.stdin.take().expect("piped stdin");
        if let Some(mut err) = child.stderr.take() {
            let program = program.to_string();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let _ = err.read_to_end(&mut buf).await;
                let text = String::from_utf8_lossy(&buf);
                if !text.trim().is_empty() {
                    tracing::info!("{program}: {}", text.trim());
                }
            });
        }
        Ok(StdioProcess {
            stdout: Box::new(stdout),
            stdin: Box::new(stdin),
            guard: Box::new(std::sync::Mutex::new(child)),
        })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[tokio::test]
    async fn runs_streams_and_spawns() {
        let r = SystemRunner;
        let out = r
            .output(
                "sh",
                &["-c".into(), "cat; echo err >&2; exit 2".into()],
                Some(b"in".to_vec()),
            )
            .await
            .unwrap();
        assert_eq!(out.code, Some(2));
        assert_eq!(out.stdout, b"in");
        assert_eq!(out.message(), "err");

        let seen = Mutex::new(String::new());
        let sink = |s: Stream, t: &str| seen.lock().unwrap().push_str(&format!("{s:?}:{t}"));
        let code = r
            .stream("sh", &["-c".into(), "echo up".into()], &sink)
            .await
            .unwrap();
        assert_eq!(code, Some(0));
        assert_eq!(seen.lock().unwrap().as_str(), "Stdout:up\n");

        let mut p = r.spawn_stdio("cat", &[]).unwrap();
        p.stdin.write_all(b"ping").await.unwrap();
        // Closing the pipe (shutdown alone keeps it open) ends `cat`.
        p.stdin = Box::new(tokio::io::sink());
        let mut back = Vec::new();
        p.stdout.read_to_end(&mut back).await.unwrap();
        assert_eq!(back, b"ping");

        let missing = r
            .output("forge-no-such-program", &[], None)
            .await
            .unwrap_err();
        assert!(missing.to_string().contains("was not found"));
    }
}
