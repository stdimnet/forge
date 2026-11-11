//! Non-interactive processes for run configurations: piped stdout/stderr streamed as text,
//! stdin for input typed into the Run console, the exit code, and kill of the whole
//! process tree (so `composer run` or `npm run` children stop too).

use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use forge_core::{ForgeError, Result};
use serde::{Deserialize, Serialize};

use crate::args::quote_arg;
use crate::queued::QueuedWriter;
use crate::utf8::Utf8Decoder;

/// A fully resolved command: what the Run tool window shows on its first line.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandLine {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory (absolute).
    #[serde(default)]
    pub cwd: PathBuf,
    /// Extra environment variables on top of the inherited environment.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// A file the program reads as its standard input (`mysql < dump.sql`); the Run console
    /// then has no input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin_file: Option<PathBuf>,
}

impl CommandLine {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            ..Self::default()
        }
    }

    pub fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }

    pub fn args<I: IntoIterator<Item = S>, S: Into<String>>(mut self, a: I) -> Self {
        self.args.extend(a.into_iter().map(Into::into));
        self
    }

    /// Shell-like display form, e.g. `/usr/bin/php vendor/bin/phpunit --teamcity`.
    pub fn display(&self) -> String {
        let mut line = std::iter::once(&self.program)
            .chain(&self.args)
            .map(|a| quote_arg(a))
            .collect::<Vec<_>>()
            .join(" ");
        if let Some(file) = &self.stdin_file {
            line.push_str(" < ");
            line.push_str(&quote_arg(&file.to_string_lossy()));
        }
        line
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputStream {
    Stdout,
    Stderr,
    /// Messages from Forge itself ("Process finished with exit code 0").
    System,
}

/// Lifecycle of a run, streamed to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RunEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        pid: u32,
        command_line: String,
    },
    Output {
        stream: OutputStream,
        text: String,
    },
    /// The process ended; `signal` is set when it was killed by one (Unix).
    Exited {
        code: Option<i32>,
        signal: Option<i32>,
    },
}

pub type RunSink = Arc<dyn Fn(RunEvent) + Send + Sync>;

/// A started process. Dropping it does not stop the process; call [`RunningProcess::kill`].
pub struct RunningProcess {
    pid: u32,
    /// Queued, so a program that does not read its input never blocks the caller.
    stdin: Option<QueuedWriter>,
    exited: Arc<AtomicBool>,
}

impl RunningProcess {
    /// Spawn `cmd`. Events go to `sink`: `Started`, then `Output`s, then exactly one `Exited`
    /// after all output has been delivered.
    pub fn start(cmd: &CommandLine, sink: RunSink) -> Result<Self> {
        let mut command = Command::new(&cmd.program);
        command.args(&cmd.args).envs(&cmd.env);
        if !cmd.cwd.as_os_str().is_empty() {
            if !cmd.cwd.is_dir() {
                return Err(ForgeError::Other(format!(
                    "working directory {} does not exist",
                    cmd.cwd.display()
                )));
            }
            command.current_dir(&cmd.cwd);
        }
        // In Flatpak the tool runs on the host (`flatpak-spawn --host`); Stop signals
        // `flatpak-spawn`, which forwards them, and `--watch-bus` ends the host process
        // if `flatpak-spawn` is killed.
        let mut command = forge_hostspawn::host_command(command);
        let input =
            match &cmd.stdin_file {
                Some(file) => Stdio::from(std::fs::File::open(file).map_err(|e| {
                    ForgeError::Other(format!("cannot read {}: {e}", file.display()))
                })?),
                None => Stdio::piped(),
            };
        command
            .stdin(input)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // Own process group, so Stop can signal the whole tree.
            command.process_group(0);
        }
        let mut child = command
            .spawn()
            .map_err(|e| ForgeError::Other(format!("cannot run {}: {e}", cmd.program)))?;
        let pid = child.id();
        sink(RunEvent::Started {
            pid,
            command_line: cmd.display(),
        });
        let stdin = match child.stdin.take() {
            Some(s) => Some(
                QueuedWriter::spawn(format!("run-stdin-{pid}"), s)
                    .map_err(|e| ForgeError::Other(e.to_string()))?,
            ),
            None => None,
        };
        let readers = [
            pump(child.stdout.take(), OutputStream::Stdout, sink.clone()),
            pump(child.stderr.take(), OutputStream::Stderr, sink.clone()),
        ];
        let exited = Arc::new(AtomicBool::new(false));
        let flag = exited.clone();
        std::thread::Builder::new()
            .name(format!("run-wait-{pid}"))
            .spawn(move || {
                let (code, signal) = wait(&mut child);
                for r in readers.into_iter().flatten() {
                    let _ = r.join();
                }
                flag.store(true, Ordering::SeqCst);
                sink(RunEvent::Exited { code, signal });
            })
            .map_err(|e| ForgeError::Other(e.to_string()))?;
        Ok(Self { pid, stdin, exited })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn has_exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst)
    }

    /// Write to the process's stdin (input typed into the Run console). Never blocks: the
    /// data is queued and written in order, even while the process does not read.
    pub fn write_stdin(&self, data: &[u8]) -> Result<()> {
        self.stdin
            .as_ref()
            .ok_or("stdin is closed")
            .and_then(|s| s.write(data))
            .map_err(|_| ForgeError::Other("stdin is closed".into()))
    }

    /// Close stdin (Ctrl+D in the console), after the input queued so far.
    pub fn close_stdin(&self) {
        if let Some(s) = &self.stdin {
            s.close();
        }
    }

    /// Stop the process and its children. On Unix the group gets SIGTERM and, if it is still
    /// running after `grace`, SIGKILL; on Windows the tree is killed with `taskkill /T /F`.
    pub fn kill(&self, grace: Duration) {
        if self.has_exited() {
            return;
        }
        kill_tree(self.pid, grace, self.exited.clone());
    }
}

fn pump(
    stream: Option<impl Read + Send + 'static>,
    kind: OutputStream,
    sink: RunSink,
) -> Option<std::thread::JoinHandle<()>> {
    let mut stream = stream?;
    std::thread::Builder::new()
        .name("run-pump".into())
        .spawn(move || {
            let mut decoder = Utf8Decoder::new();
            let mut buf = [0u8; 8192];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let text = decoder.decode(&buf[..n]);
                        if !text.is_empty() {
                            sink(RunEvent::Output { stream: kind, text });
                        }
                    }
                }
            }
            let text = decoder.finish();
            if !text.is_empty() {
                sink(RunEvent::Output { stream: kind, text });
            }
        })
        .ok()
}

fn wait(child: &mut Child) -> (Option<i32>, Option<i32>) {
    match child.wait() {
        Ok(status) => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                (status.code(), status.signal())
            }
            #[cfg(not(unix))]
            {
                (status.code(), None)
            }
        }
        Err(_) => (None, None),
    }
}

#[cfg(unix)]
fn kill_tree(pid: u32, grace: Duration, exited: Arc<AtomicBool>) {
    let pgid = -(pid as libc::pid_t);
    // SAFETY: plain syscall on a process group we created.
    unsafe {
        libc::kill(pgid, libc::SIGTERM);
    }
    std::thread::spawn(move || {
        let step = Duration::from_millis(50);
        let mut waited = Duration::ZERO;
        while waited < grace {
            if exited.load(Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(step);
            waited += step;
        }
        // SAFETY: as above.
        unsafe {
            libc::kill(pgid, libc::SIGKILL);
        }
    });
}

#[cfg(windows)]
fn kill_tree(pid: u32, _grace: Duration, _exited: Arc<AtomicBool>) {
    let mut taskkill = Command::new("taskkill");
    taskkill.args(["/T", "/F", "/PID", &pid.to_string()]);
    let _ = forge_hostspawn::local_command(taskkill)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::{mpsc, Mutex};

    fn run(cmd: CommandLine) -> (RunningProcess, mpsc::Receiver<RunEvent>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let sink: RunSink = Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        });
        (RunningProcess::start(&cmd, sink).expect("start"), rx)
    }

    type Exit = Option<(Option<i32>, Option<i32>)>;

    fn drain(rx: &mpsc::Receiver<RunEvent>) -> (String, String, Exit) {
        let (mut out, mut err) = (String::new(), String::new());
        while let Ok(e) = rx.recv_timeout(Duration::from_secs(10)) {
            match e {
                RunEvent::Output {
                    stream: OutputStream::Stdout,
                    text,
                } => out += &text,
                RunEvent::Output { text, .. } => err += &text,
                RunEvent::Exited { code, signal } => return (out, err, Some((code, signal))),
                RunEvent::Started { .. } => {}
            }
        }
        (out, err, None)
    }

    #[test]
    fn streams_output_env_cwd_and_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let mut cmd = CommandLine::new("/bin/sh")
            .args(["-c", "echo out:$FORGE_VAR; pwd; echo oops >&2; exit 7"]);
        cmd.cwd = dir.path().to_path_buf();
        cmd.env.insert("FORGE_VAR".into(), "42".into());
        let (_p, rx) = run(cmd);
        match rx.recv_timeout(Duration::from_secs(5)).unwrap() {
            RunEvent::Started { command_line, .. } => {
                assert!(command_line.starts_with("/bin/sh -c '"))
            }
            e => panic!("expected Started, got {e:?}"),
        }
        let (out, err, exit) = drain(&rx);
        assert!(out.contains("out:42"), "{out}");
        let canonical = dir.path().canonicalize().unwrap();
        assert!(out.contains(canonical.to_str().unwrap()), "{out}");
        assert_eq!(err.trim(), "oops");
        assert_eq!(exit, Some((Some(7), None)));
    }

    #[test]
    fn stdin_reaches_the_process() {
        let (p, rx) = run(CommandLine::new("/bin/sh").args(["-c", "read x; echo got:$x"]));
        p.write_stdin(b"forge\n").unwrap();
        let (out, _, exit) = drain(&rx);
        assert!(out.contains("got:forge"), "{out}");
        assert_eq!(exit, Some((Some(0), None)));
    }

    /// Input for a process that never reads stdin is queued instead of blocking the caller
    /// once the pipe buffer (64 KB on Linux) is full.
    #[test]
    fn large_input_does_not_block() {
        let (p, _rx) = run(CommandLine::new("/bin/sleep").arg("30"));
        let started = std::time::Instant::now();
        p.write_stdin(&vec![b'x'; 1024 * 1024]).unwrap();
        p.write_stdin(b"more").unwrap();
        assert!(started.elapsed() < Duration::from_secs(5));
        p.kill(Duration::from_millis(100));
    }

    #[cfg(unix)]
    #[test]
    fn a_file_becomes_the_programs_input() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("in;put.txt");
        std::fs::write(&file, "from the file\n").unwrap();
        let mut cmd = CommandLine::new("cat");
        cmd.stdin_file = Some(file.clone());
        assert_eq!(cmd.display(), format!("cat < '{}'", file.display()));
        let (p, rx) = run(cmd.clone());
        assert!(p.write_stdin(b"typed").is_err());
        let (out, _, exit) = drain(&rx);
        assert_eq!(out, "from the file\n");
        assert_eq!(exit, Some((Some(0), None)));
        cmd.stdin_file = Some(dir.path().join("missing.sql"));
        assert!(RunningProcess::start(&cmd, Arc::new(|_| {})).is_err());
    }

    #[test]
    fn closed_stdin_refuses_input() {
        let (p, rx) = run(CommandLine::new("/bin/cat"));
        p.write_stdin(b"before close\n").unwrap();
        p.close_stdin();
        assert!(p.write_stdin(b"after").is_err());
        let (out, _, exit) = drain(&rx);
        assert!(out.contains("before close"), "{out}");
        assert_eq!(exit, Some((Some(0), None)));
    }

    #[test]
    fn kill_stops_the_whole_tree() {
        // The shell starts a grandchild that would outlive a plain kill of the shell.
        let (p, rx) = run(CommandLine::new("/bin/sh").args(["-c", "sleep 30 & sleep 30; wait"]));
        std::thread::sleep(Duration::from_millis(200));
        let started = std::time::Instant::now();
        p.kill(Duration::from_millis(500));
        let (_, _, exit) = drain(&rx);
        let (code, signal) = exit.expect("process exited");
        assert!(code.is_none() && signal.is_some(), "{code:?} {signal:?}");
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(p.has_exited());
    }

    #[test]
    fn missing_program_and_cwd_are_errors() {
        let sink: RunSink = Arc::new(|_| {});
        assert!(
            RunningProcess::start(&CommandLine::new("/no/such/forge-binary"), sink.clone())
                .is_err()
        );
        let mut cmd = CommandLine::new("/bin/true");
        cmd.cwd = "/no/such/dir".into();
        let err = RunningProcess::start(&cmd, sink).err().unwrap();
        assert!(err.to_string().contains("does not exist"));
    }
}
