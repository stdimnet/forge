//! Interactive shell sessions on a pseudo terminal, used by the Terminal tool window.

use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(unix)]
use std::time::Duration;

use forge_core::{ForgeError, Result};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};

use crate::queued::QueuedWriter;
use crate::shell::{default_shell, shell_args};
use crate::termenv::{terminal_env, utf8_locale_fallback, FOREIGN_TERMINAL_VARS};
use crate::utf8::Utf8Decoder;

/// How to start a terminal session.
#[derive(Debug, Clone, Default)]
pub struct PtyOptions {
    /// Shell executable; `None` uses [`default_shell`].
    pub shell: Option<String>,
    /// Shell arguments; `None` uses the customary ones for the shell (e.g. `-l` for bash).
    pub args: Option<Vec<String>>,
    pub cwd: PathBuf,
    /// Extra environment variables on top of the inherited environment and
    /// [`terminal_env`]; they win over both.
    pub env: Vec<(String, String)>,
    /// Reported as `TERM_PROGRAM_VERSION`; empty uses this crate's version.
    pub version: String,
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
    /// Input goes through a queue so a shell that does not read never blocks the caller.
    writer: QueuedWriter,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    shell: String,
    pid: Option<u32>,
    exited: Arc<AtomicBool>,
}

/// How long [`PtySession::kill`] waits after SIGHUP before it sends SIGKILL.
#[cfg(unix)]
const KILL_GRACE: Duration = Duration::from_millis(1500);

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
        let mut spec = forge_hostspawn::Spec::new(&shell);
        let mut extra_env = Vec::new();
        match opts.args {
            Some(args) => spec.args.extend(args.into_iter().map(Into::into)),
            None => {
                let defaults = shell_args(&shell);
                let integration = opts.integration_dir.as_deref().and_then(|dir| {
                    // The shell's own environment wins over Forge's.
                    let var = |key: &str| {
                        opts.env
                            .iter()
                            .rev()
                            .find(|(k, _)| k == key)
                            .map(|(_, v)| std::ffi::OsString::from(v))
                            .or_else(|| forge_hostspawn::env_var(key).map(Into::into))
                    };
                    let user_zdotdir = var("ZDOTDIR").or_else(|| var("HOME")).map(PathBuf::from);
                    crate::integration::prepare(&shell, &defaults, dir, user_zdotdir)
                        .map_err(|e| tracing::warn!("shell integration unavailable: {e}"))
                        .ok()
                        .flatten()
                });
                match integration {
                    Some(i) => {
                        spec.args.extend(i.args.into_iter().map(Into::into));
                        extra_env = i.env;
                    }
                    None => spec.args.extend(defaults.into_iter().map(Into::into)),
                }
            }
        }
        spec.cwd = Some(opts.cwd.clone());
        for var in FOREIGN_TERMINAL_VARS {
            spec.env.push(((*var).into(), None));
        }
        let version = match opts.version.trim() {
            "" => env!("CARGO_PKG_VERSION"),
            v => v,
        };
        for (k, v) in terminal_env(version) {
            spec.env.push((k.into(), Some(v.into())));
        }
        if let Some(lang) = utf8_locale_fallback(forge_hostspawn::env_var) {
            spec.env.push(("LANG".into(), Some(lang.into())));
        }
        for (k, v) in extra_env.iter().chain(&opts.env) {
            spec.env.push((k.into(), Some(v.into())));
        }
        // In Flatpak the shell runs on the host, wrapped so that it gets the terminal as its
        // controlling terminal there (the sandbox side must not claim it).
        let host = forge_hostspawn::active();
        if host {
            spec = forge_hostspawn::pty_spec(&spec);
        }
        let mut cmd = CommandBuilder::new(&spec.program);
        cmd.args(&spec.args);
        if let Some(dir) = &spec.cwd {
            cmd.cwd(dir);
        }
        for (k, v) in &spec.env {
            match v {
                Some(v) => cmd.env(k, v),
                None => cmd.env_remove(k),
            }
        }
        if host {
            cmd.set_controlling_tty(false);
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
        let writer = QueuedWriter::spawn(
            format!("pty-input-{}", pid.unwrap_or(0)),
            pair.master.take_writer().map_err(other)?,
        )
        .map_err(other)?;
        let exited = Arc::new(AtomicBool::new(false));
        let exited_flag = exited.clone();

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
                exited_flag.store(true, Ordering::SeqCst);
                sink(PtyEvent::Exited { code });
            })
            .map_err(other)?;

        Ok(Self {
            master: Mutex::new(pair.master),
            writer,
            killer: Mutex::new(killer),
            shell,
            pid,
            exited,
        })
    }

    /// The shell executable that was started.
    pub fn shell(&self) -> &str {
        &self.shell
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Send user input (keystrokes, pasted text) to the shell. Never blocks: the input is
    /// queued and written in order, even while the program in the terminal does not read.
    pub fn write(&self, data: &[u8]) -> Result<()> {
        self.writer.write(data).map_err(other)
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

    /// The program running in the foreground of the terminal instead of the shell (`sleep`,
    /// `vim`, a dev server), or `None` while the shell waits at its prompt. Unix only: the
    /// terminal's foreground process group (`tcgetpgrp`) is not the shell's own.
    pub fn foreground_process(&self) -> Option<String> {
        if self.has_exited() {
            return None;
        }
        #[cfg(unix)]
        {
            let leader = self
                .master
                .lock()
                .expect("pty master poisoned")
                .process_group_leader()?;
            let shell = self.pid? as libc::pid_t;
            if leader <= 0 || leader == shell {
                return None;
            }
            Some(process_name(leader as u32).unwrap_or_else(|| format!("process {leader}")))
        }
        #[cfg(not(unix))]
        {
            None
        }
    }

    /// Whether the shell has exited.
    pub fn has_exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst)
    }

    /// Terminate the shell: SIGHUP like a closing terminal window, then SIGKILL to the
    /// shell's process group if it still runs after a grace period. Bash can lose a SIGHUP
    /// that arrives while it redraws its prompt and would otherwise keep running.
    pub fn kill(&self) -> Result<()> {
        if self.has_exited() {
            return Ok(());
        }
        self.killer
            .lock()
            .expect("pty killer poisoned")
            .kill()
            .map_err(other)?;
        #[cfg(unix)]
        if let Some(pid) = self.pid {
            let exited = self.exited.clone();
            let _ = std::thread::Builder::new()
                .name(format!("pty-kill-{pid}"))
                .spawn(move || {
                    std::thread::sleep(KILL_GRACE);
                    // The reader thread reaps the child before it sets `exited`, so the pid
                    // cannot have been reused while the flag is still false.
                    if !exited.load(Ordering::SeqCst) {
                        let pid = pid as libc::pid_t;
                        // SAFETY: plain kill(2) calls on the shell and its process group.
                        unsafe {
                            libc::kill(-pid, libc::SIGKILL);
                            libc::kill(pid, libc::SIGKILL);
                        }
                    }
                });
        }
        Ok(())
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

/// The command name of a process (`/proc/<pid>/comm` on Linux, `ps` elsewhere).
#[cfg(unix)]
fn process_name(pid: u32) -> Option<String> {
    let name = match std::fs::read_to_string(format!("/proc/{pid}/comm")) {
        Ok(name) => name,
        Err(_) => {
            let out = std::process::Command::new("ps")
                .args(["-o", "comm=", "-p", &pid.to_string()])
                .output()
                .ok()?;
            String::from_utf8_lossy(&out.stdout).into_owned()
        }
    };
    let name = name.trim();
    let name = name.rsplit('/').next().unwrap_or(name);
    (!name.is_empty()).then(|| name.to_string())
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
                version: "9.9.9".into(),
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
    fn sets_the_terminal_environment() {
        let script = "echo \"env:$TERM:$COLORTERM:$TERM_PROGRAM:$TERM_PROGRAM_VERSION:${KITTY_WINDOW_ID-unset}\"";
        let (_s, rx) = start("/bin/sh", vec!["-c".into(), script.into()]);
        let (text, _) = collect_until(&rx, |_| false);
        assert!(
            text.contains("env:xterm-256color:truecolor:Forge:9.9.9:unset"),
            "{text:?}"
        );
    }

    /// A large paste into a terminal whose program does not read returns at once instead of
    /// blocking until the terminal's input buffer drains.
    #[test]
    fn large_input_does_not_block() {
        let (s, _rx) = start("/bin/sleep", vec!["30".into()]);
        let started = std::time::Instant::now();
        s.write(&vec![b'x'; 1024 * 1024]).unwrap();
        s.write(b"more").unwrap();
        assert!(started.elapsed() < Duration::from_secs(5));
        s.kill().unwrap();
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

    /// `sleep 100` in an interactive shell (job control on) is the terminal's foreground
    /// process; at the prompt there is none.
    #[test]
    fn detects_the_foreground_process() {
        let (s, rx) = start("/bin/sh", vec!["-i".into()]);
        s.write(b"echo ready-$((1+1))\n").unwrap();
        collect_until(&rx, |t| t.contains("ready-2"));
        assert_eq!(
            s.foreground_process(),
            None,
            "the shell waits at its prompt"
        );
        s.write(b"sleep 100\n").unwrap();
        let mut found = None;
        for _ in 0..50 {
            found = s.foreground_process();
            if found.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(found.as_deref(), Some("sleep"));
        s.kill().unwrap();
        let (_, exit) = collect_until(&rx, |_| false);
        assert!(exit.is_some());
        assert_eq!(
            s.foreground_process(),
            None,
            "an ended session runs nothing"
        );
    }

    /// A live shell with Forge's integration scripts reports its directory (OSC 7) and the
    /// prompt, command start and exit code of a failing command (OSC 133).
    fn integration_reports_directory_and_command_marks(shell: &str) {
        let Some(shell) = crate::shell::find_in_path(shell) else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let sink: PtySink = Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        });
        let home = tempfile::tempdir().unwrap();
        let scripts = tempfile::tempdir().unwrap();
        // zsh starts its new-user setup in a home without startup files; an empty one skips it.
        std::fs::write(home.path().join(".zshenv"), "").unwrap();
        let s = PtySession::spawn(
            PtyOptions {
                shell: Some(shell.display().to_string()),
                args: None,
                cwd: home.path().to_path_buf(),
                // A clean home: no user startup files.
                env: vec![("HOME".into(), home.path().display().to_string())],
                version: String::new(),
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

    #[test]
    fn bash_integration_reports_directory_and_command_marks() {
        integration_reports_directory_and_command_marks("bash");
    }

    #[test]
    fn zsh_integration_reports_directory_and_command_marks() {
        integration_reports_directory_and_command_marks("zsh");
    }
}
