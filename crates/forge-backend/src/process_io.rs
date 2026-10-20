//! Processes started through a [`Backend`] (on this machine or on a remote host) used like
//! local child processes: line by line with cancelation ([`run_lines`], artisan and
//! Symfony console runs) or as async stdio streams ([`spawn_stdio`], agent CLIs).

use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::task::{Context, Poll};
use std::time::Duration;

use forge_core::Result;
use forge_process::{CommandLine, OutputStream, RunEvent, RunSink};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{mpsc as tmpsc, oneshot};

use crate::{Backend, Process};

/// Splits streamed text into lines (without `\r\n` / `\n`).
#[derive(Debug, Default)]
pub struct LineSplitter {
    partial: String,
}

impl LineSplitter {
    pub fn push(&mut self, text: &str) -> Vec<String> {
        self.partial.push_str(text);
        let mut out = Vec::new();
        while let Some(i) = self.partial.find('\n') {
            let mut line: String = self.partial.drain(..=i).collect();
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
            out.push(line);
        }
        out
    }

    /// The last line when the output did not end with a newline.
    pub fn finish(&mut self) -> Option<String> {
        (!self.partial.is_empty()).then(|| std::mem::take(&mut self.partial))
    }
}

enum Msg {
    Line(String, bool),
    Exit(Option<i32>),
}

/// Run `cmd` through `backend`, calling `on_line(text, stderr)` for every output line until
/// it exits or `cancel` is set (the process is killed then). Returns the exit code (`None`
/// when it was killed or the connection was lost).
pub fn run_lines(
    backend: &dyn Backend,
    cmd: &CommandLine,
    cancel: &AtomicBool,
    on_line: &mut dyn FnMut(String, bool),
) -> Result<Option<i32>> {
    let (tx, rx) = mpsc::channel::<Msg>();
    let splitters = std::sync::Mutex::new((LineSplitter::default(), LineSplitter::default()));
    let sink: RunSink = Arc::new(move |event| match event {
        RunEvent::Output { stream, text } => {
            let stderr = stream != OutputStream::Stdout;
            let mut s = splitters.lock().expect("splitter lock");
            let splitter = if stderr { &mut s.1 } else { &mut s.0 };
            for line in splitter.push(&text) {
                let _ = tx.send(Msg::Line(line, stderr));
            }
        }
        RunEvent::Exited { code, .. } => {
            let mut s = splitters.lock().expect("splitter lock");
            if let Some(line) = s.0.finish() {
                let _ = tx.send(Msg::Line(line, false));
            }
            if let Some(line) = s.1.finish() {
                let _ = tx.send(Msg::Line(line, true));
            }
            let _ = tx.send(Msg::Exit(code));
        }
        RunEvent::Started { .. } => {}
    });
    let process = backend.start_process(cmd, sink)?;
    let mut killed = false;
    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Msg::Line(text, stderr)) => on_line(text, stderr),
            Ok(Msg::Exit(code)) => return Ok(if killed { None } else { code }),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if cancel.load(Ordering::Relaxed) && !killed {
                    killed = true;
                    process.kill(Duration::from_secs(2));
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(None),
        }
    }
}

/// Async reader over chunks pushed from a sink.
pub struct ChunkReader {
    rx: tmpsc::UnboundedReceiver<Vec<u8>>,
    buf: Vec<u8>,
    pos: usize,
}

impl AsyncRead for ChunkReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        out: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        loop {
            if self.pos < self.buf.len() {
                let n = (self.buf.len() - self.pos).min(out.remaining());
                let pos = self.pos;
                out.put_slice(&self.buf[pos..pos + n]);
                self.pos += n;
                return Poll::Ready(Ok(()));
            }
            match self.rx.poll_recv(cx) {
                Poll::Ready(Some(chunk)) => {
                    self.buf = chunk;
                    self.pos = 0;
                }
                // End of stream.
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

enum Input {
    Data(Vec<u8>),
    Close,
}

/// Async writer feeding a process' stdin in order (writes happen on a helper thread, since
/// [`Process::write_stdin`] blocks).
pub struct StdinWriter {
    tx: Option<mpsc::Sender<Input>>,
}

impl AsyncWrite for StdinWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let sent = self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.send(Input::Data(data.to_vec())).is_ok());
        Poll::Ready(if sent {
            Ok(data.len())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "stdin is closed",
            ))
        })
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Input::Close);
        }
        Poll::Ready(Ok(()))
    }
}

impl Drop for StdinWriter {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Input::Close);
        }
    }
}

/// A process with async stdio.
pub struct StdioProcess {
    pub stdin: StdinWriter,
    pub stdout: ChunkReader,
    pub stderr: ChunkReader,
    /// Resolves with the exit code (`None` when killed by a signal or the connection was
    /// lost).
    pub exit: oneshot::Receiver<Option<i32>>,
    pub process: Arc<dyn Process>,
}

/// Start `cmd` through `backend` with async stdio streams. Blocks until it started.
pub fn spawn_stdio(backend: &dyn Backend, cmd: &CommandLine) -> Result<StdioProcess> {
    let (out_tx, out_rx) = tmpsc::unbounded_channel::<Vec<u8>>();
    let (err_tx, err_rx) = tmpsc::unbounded_channel::<Vec<u8>>();
    let (exit_tx, exit_rx) = oneshot::channel();
    let streams = std::sync::Mutex::new(Some((out_tx, err_tx, exit_tx)));
    let sink: RunSink = Arc::new(move |event| {
        let mut guard = streams.lock().expect("streams lock");
        match event {
            RunEvent::Output { stream, text } => {
                if let Some((out, err, _)) = guard.as_ref() {
                    let target = if stream == OutputStream::Stdout {
                        out
                    } else {
                        err
                    };
                    let _ = target.send(text.into_bytes());
                }
            }
            RunEvent::Exited { code, signal } => {
                // Dropping the senders ends both readers.
                if let Some((_, _, exit)) = guard.take() {
                    let _ = exit.send(if signal.is_some() { None } else { code });
                }
            }
            RunEvent::Started { .. } => {}
        }
    });
    let process = backend.start_process(cmd, sink)?;
    let (in_tx, in_rx) = mpsc::channel::<Input>();
    let writer = process.clone();
    std::thread::Builder::new()
        .name("process-stdin".into())
        .spawn(move || {
            while let Ok(input) = in_rx.recv() {
                match input {
                    Input::Data(data) => {
                        if writer.write_stdin(&data).is_err() {
                            break;
                        }
                    }
                    Input::Close => {
                        let _ = writer.close_stdin();
                        break;
                    }
                }
            }
        })
        .map_err(|e| forge_core::ForgeError::Other(e.to_string()))?;
    Ok(StdioProcess {
        stdin: StdinWriter { tx: Some(in_tx) },
        stdout: ChunkReader {
            rx: out_rx,
            buf: Vec::new(),
            pos: 0,
        },
        stderr: ChunkReader {
            rx: err_rx,
            buf: Vec::new(),
            pos: 0,
        },
        exit: exit_rx,
        process,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalBackend;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    #[test]
    fn splits_lines_across_chunks() {
        let mut s = LineSplitter::default();
        assert_eq!(s.push("a\r\nb"), vec!["a"]);
        assert_eq!(s.push("c\n\nd"), vec!["bc", ""]);
        assert_eq!(s.finish().as_deref(), Some("d"));
        assert_eq!(s.finish(), None);
    }

    #[cfg(unix)]
    #[test]
    fn runs_line_by_line_and_cancels() {
        let dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::open(dir.path()).unwrap();
        let cmd = CommandLine::new("sh").args(["-c", "echo one; echo two >&2; printf three"]);
        let mut lines = Vec::new();
        let code = run_lines(&backend, &cmd, &AtomicBool::new(false), &mut |l, e| {
            lines.push((l, e))
        })
        .unwrap();
        assert_eq!(code, Some(0));
        lines.sort();
        assert_eq!(
            lines,
            vec![
                ("one".to_string(), false),
                ("three".to_string(), false),
                ("two".to_string(), true)
            ]
        );

        let cancel = AtomicBool::new(true);
        let cmd = CommandLine::new("sh").args(["-c", "sleep 30"]);
        let code = run_lines(&backend, &cmd, &cancel, &mut |_, _| {}).unwrap();
        assert_eq!(code, None);
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stdio_streams_like_a_child_process() {
        let dir = tempfile::tempdir().unwrap();
        let backend = LocalBackend::open(dir.path()).unwrap();
        let cmd = CommandLine::new("sh").args(["-c", "read x; echo \"got $x\"; cat; echo err >&2"]);
        let mut p = spawn_stdio(&backend, &cmd).unwrap();
        p.stdin.write_all(b"hello\nrest").await.unwrap();
        p.stdin.shutdown().await.unwrap();
        let mut lines = BufReader::new(p.stdout).lines();
        assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("got hello"));
        assert_eq!(lines.next_line().await.unwrap().as_deref(), Some("rest"));
        assert_eq!(lines.next_line().await.unwrap(), None);
        let mut err = String::new();
        p.stderr.read_to_string(&mut err).await.unwrap();
        assert_eq!(err, "err\n");
        assert_eq!(p.exit.await.unwrap(), Some(0));
    }
}
