//! Interactive shell sessions on a pseudo terminal, used by the Terminal tool window.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use forge_core::{ForgeError, Result};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};

use crate::shell::{default_shell, shell_args};
use crate::utf8::Utf8Decoder;

/// How to start a terminal session.
#[derive(Debug, Clone, Default)]
pub struct PtyOptions {
    /// Shell executable; `None` uses [`default_shell`].
    pub shell: Option<String>,
    /// Shell arguments; `None` uses the customary ones for the shell (e.g. `-l` for bash).
    pub args: Option<Vec<String>>,
    pub cwd: PathBuf,
    /// Extra environment variables on top of the inherited environment.
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
    /// Shell integration (see [`crate::integration`]): its scripts are written to this
    /// folder. Only for shells started with their default arguments.
    pub integration_dir: Option<PathBuf>,
}

/// Something that happened in a terminal session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PtyEvent {
    /// Terminal output (UTF-8, including escape sequences).
    Output { data: String },
    /// The shell exited.
    Exited { code: Option<i32> },
}

pub type PtySink = Arc<dyn Fn(PtyEvent) + Send + Sync>;

/// A running shell on a pseudo terminal.
pub struct PtySession {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    shell: String,
    pid: Option<u32>,
}

impl PtySession {
    /// Start the shell; output and the exit are delivered to `sink` from a reader thread.
    pub fn spawn(opts: PtyOptions, sink: PtySink) -> Result<Self> {
        let size = PtySize {
            rows: opts.rows.max(2),
            cols: opts.cols.max(10),
            pixel_width: 0,
            pixel_height: 0,
        };
        let pair = native_pty_system().openpty(size).map_err(other)?;
        let shell = opts
            .shell
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(default_shell);
        let mut cmd = CommandBuilder::new(&shell);
        let mut extra_env = Vec::new();
        match opts.args {
            Some(args) => cmd.args(args),
            None => {
                let defaults = shell_args(&shell);
                let integration = opts.integration_dir.as_deref().and_then(|dir| {
                    let user_zdotdir = std::env::var_os("ZDOTDIR")
                        .or_else(|| std::env::var_os("HOME"))
                        .map(PathBuf::from);
                    crate::integration::prepare(&shell, &defaults, dir, user_zdotdir)
                        .map_err(|e| tracing::warn!("shell integration unavailable: {e}"))
                        .ok()
                        .flatten()
                });
                match integration {
                    Some(i) => {
                        cmd.args(i.args);
                        extra_env = i.env;
                    }
                    None => cmd.args(defaults),
                }
            }
        }
        cmd.cwd(&opts.cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "Forge");
        for (k, v) in extra_env.iter().chain(&opts.env) {
            cmd.env(k, v);
        }
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| ForgeError::Other(format!("cannot start {shell}: {e}")))?;
        // The child holds its own copy; keeping ours would prevent EOF on exit.
        drop(pair.slave);
        let pid = child.process_id();
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader().map_err(other)?;
        let writer = pair.master.take_writer().map_err(other)?;

        std::thread::Builder::new()
            .name(format!("pty-{}", pid.unwrap_or(0)))
            .spawn(move || {
                let mut decoder = Utf8Decoder::new();
                let mut buf = [0u8; 16 * 1024];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let data = decoder.decode(&buf[..n]);
                            if !data.is_empty() {
                                sink(PtyEvent::Output { data });
                            }
                        }
                    }
                }
                let rest = decoder.finish();
                if !rest.is_empty() {
                    sink(PtyEvent::Output { data: rest });
                }
                let code = child.wait().ok().map(|s| s.exit_code() as i32);
                sink(PtyEvent::Exited { code });
            })
            .map_err(other)?;

        Ok(Self {
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            killer: Mutex::new(killer),
            shell,
            pid,
        })
    }

    /// The shell executable that was started.
    pub fn shell(&self) -> &str {
        &self.shell
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Send user input (keystrokes, pasted text) to the shell.
    pub fn write(&self, data: &[u8]) -> Result<()> {
        let mut w = self.writer.lock().expect("pty writer poisoned");
        w.write_all(data).map_err(other)?;
        w.flush().map_err(other)
    }

    /// Tell the shell about a new terminal size (SIGWINCH).
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.master
            .lock()
            .expect("pty master poisoned")
            .resize(PtySize {
                rows: rows.max(2),
                cols: cols.max(10),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(other)
    }

    /// Terminate the shell.
    pub fn kill(&self) -> Result<()> {
        self.killer
            .lock()
            .expect("pty killer poisoned")
            .kill()
            .map_err(other)
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

fn other(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn start(shell: &str, args: Vec<String>) -> (PtySession, mpsc::Receiver<PtyEvent>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let sink: PtySink = Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        });
        let dir = std::env::temp_dir();
        let session = PtySession::spawn(
            PtyOptions {
                shell: Some(shell.into()),
                args: Some(args),
                cwd: dir,
                env: vec![("FORGE_TEST".into(), "pty-ok".into())],
                cols: 80,
                rows: 24,
                integration_dir: None,
            },
            sink,
        )
        .expect("spawn");
        (session, rx)
    }

    /// Collect output until `pred` matches the text so far or the exit arrives.
    fn collect_until(
        rx: &mpsc::Receiver<PtyEvent>,
        pred: impl Fn(&str) -> bool,
    ) -> (String, Option<Option<i32>>) {
        let mut text = String::new();
        while let Ok(e) = rx.recv_timeout(Duration::from_secs(10)) {
            match e {
                PtyEvent::Output { data } => {
                    text.push_str(&data);
                    if pred(&text) {
                        return (text, None);
                    }
                }
                PtyEvent::Exited { code } => return (text, Some(code)),
            }
        }
        (text, None)
    }

    #[test]
    fn runs_a_command_and_reports_exit() {
        let (_s, rx) = start(
            "/bin/sh",
            vec!["-c".into(), "echo hello $FORGE_TEST; exit 3".into()],
        );
        let (text, exit) = collect_until(&rx, |_| false);
        assert!(text.contains("hello pty-ok"), "{text:?}");
        assert_eq!(exit, Some(Some(3)));
    }

    #[test]
    fn interactive_input_resize_and_kill() {
        let (s, rx) = start("/bin/sh", vec![]);
        s.resize(100, 30).unwrap();
        s.write(b"stty size; echo forge-$((40+2))\n").unwrap();
        let (text, _) = collect_until(&rx, |t| t.contains("forge-42"));
        assert!(text.contains("30 100"), "{text:?}");
        s.kill().unwrap();
        let (_, exit) = collect_until(&rx, |_| false);
        assert!(exit.is_some(), "no exit after kill");
    }

    #[test]
    fn bash_integration_reports_directory_and_command_marks() {
        let Some(bash) = crate::shell::find_in_path("bash") else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let sink: PtySink = Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        });
        let home = tempfile::tempdir().unwrap();
        let scripts = tempfile::tempdir().unwrap();
        let s = PtySession::spawn(
            PtyOptions {
                shell: Some(bash.display().to_string()),
                args: None,
                cwd: home.path().to_path_buf(),
                // A clean home: no user startup files.
                env: vec![("HOME".into(), home.path().display().to_string())],
                cols: 80,
                rows: 24,
                integration_dir: Some(scripts.path().to_path_buf()),
            },
            sink,
        )
        .expect("spawn");
        s.write(b"false\n").unwrap();
        let (text, _) = collect_until(&rx, |t| t.contains("\x1b]133;D;1\x07"));
        assert!(text.contains("\x1b]7;file://"), "{text:?}");
        assert!(
            text.contains("\x1b]133;A\x07") && text.contains("\x1b]133;C\x07"),
            "{text:?}"
        );
        assert!(text.contains("\x1b]133;D;1\x07"), "{text:?}");
        s.kill().unwrap();
    }
}
