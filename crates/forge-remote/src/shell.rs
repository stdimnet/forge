//! SSH terminal sessions (Tools | Start SSH Session…, the Terminal's "SSH Session…"): a
//! pseudo terminal on an SSH session channel, driven through [`forge_backend::Terminal`] so
//! the Terminal tool window treats it like a local shell. Output goes to a
//! [`forge_process::PtySink`] as UTF-8 text; the channel's exit status ends the session.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use forge_backend::Terminal;
use forge_core::{ForgeError, Result};
use forge_process::{PtyEvent, PtySink, Utf8Decoder};
use russh::ChannelMsg;
use tokio::sync::mpsc;

use crate::session::{ConnectError, ConnectParams, SshSession};

/// The terminal type announced to the server (what xterm.js emulates).
pub const TERM: &str = "xterm-256color";

enum Input {
    Data(Vec<u8>),
    Resize(u16, u16),
    Close,
}

/// A shell on a remote host over SSH.
pub struct SshShell {
    input: mpsc::UnboundedSender<Input>,
    /// The channel closed (the shell exited or the connection dropped).
    ended: Arc<AtomicBool>,
    label: String,
    session: Arc<SshSession>,
    /// Drives the SSH session and the channel tasks (one worker thread).
    rt: Mutex<Option<tokio::runtime::Runtime>>,
}

impl SshShell {
    /// Connect with `params`, open a shell on a `cols` x `rows` pseudo terminal and stream
    /// its output to `sink`. Blocks until the shell runs; call it off the UI thread.
    pub fn open(
        params: &ConnectParams,
        cols: u16,
        rows: u16,
        sink: PtySink,
    ) -> std::result::Result<Self, ConnectError> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|e| ConnectError::Other {
                message: e.to_string(),
            })?;
        let (session, channel) = rt.block_on(async {
            let session = SshSession::connect(params).await?;
            let channel = session.open_shell(TERM, cols, rows).await?;
            Ok::<_, ConnectError>((Arc::new(session), channel))
        })?;
        let (mut read, write) = channel.split();
        let (input, mut rx) = mpsc::unbounded_channel::<Input>();

        rt.spawn(async move {
            while let Some(msg) = rx.recv().await {
                let sent = match msg {
                    Input::Data(data) => write.data_bytes(data).await,
                    Input::Resize(cols, rows) => {
                        write
                            .window_change(cols.max(1).into(), rows.max(1).into(), 0, 0)
                            .await
                    }
                    Input::Close => {
                        let _ = write.eof().await;
                        let _ = write.close().await;
                        break;
                    }
                };
                if sent.is_err() {
                    break;
                }
            }
        });

        let output = sink.clone();
        let ended = Arc::new(AtomicBool::new(false));
        let ended_flag = ended.clone();
        rt.spawn(async move {
            let mut decoder = Utf8Decoder::new();
            let mut code: Option<i32> = None;
            while let Some(msg) = read.wait().await {
                match msg {
                    ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                        let text = decoder.decode(&data);
                        if !text.is_empty() {
                            output(PtyEvent::Output { data: text });
                        }
                    }
                    ChannelMsg::ExitStatus { exit_status } => {
                        code = Some(i32::try_from(exit_status).unwrap_or(i32::MAX))
                    }
                    ChannelMsg::ExitSignal { .. } => code = code.or(Some(-1)),
                    ChannelMsg::Close => break,
                    _ => {}
                }
            }
            ended_flag.store(true, Ordering::SeqCst);
            output(PtyEvent::Exited { code });
        });

        Ok(Self {
            input,
            ended,
            label: format!("{}@{}", params.user, params.host),
            session,
            rt: Mutex::new(Some(rt)),
        })
    }

    fn send(&self, input: Input) -> Result<()> {
        if self.ended.load(Ordering::SeqCst) {
            return Err(ForgeError::Other("The SSH session has ended".into()));
        }
        self.input
            .send(input)
            .map_err(|_| ForgeError::Other("The SSH session has ended".into()))
    }
}

impl Terminal for SshShell {
    /// `user@host`, shown as the tab's program.
    fn shell(&self) -> String {
        self.label.clone()
    }

    /// The login folder on the host (the shell's own business).
    fn cwd(&self) -> PathBuf {
        PathBuf::from("~")
    }

    fn write(&self, data: &[u8]) -> Result<()> {
        self.send(Input::Data(data.to_vec()))
    }

    fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.send(Input::Resize(cols, rows))
    }

    fn kill(&self) -> Result<()> {
        let _ = self.send(Input::Close);
        let rt = self.rt.lock().expect("ssh shell runtime").take();
        if let Some(rt) = rt {
            let session = self.session.clone();
            rt.spawn(async move { session.disconnect().await });
            // Gives the close and disconnect messages a moment, never blocks the caller's
            // runtime.
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                rt.shutdown_background();
            });
        }
        Ok(())
    }
}

impl Drop for SshShell {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}
