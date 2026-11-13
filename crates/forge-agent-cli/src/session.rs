//! Runs agent CLI processes for a conversation and streams their normalized events: one
//! long-running process ([`Transport::Persistent`]) or one process per message
//! ([`Transport::PerTurn`]).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use forge_core::{ForgeError, Result};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{oneshot, Mutex};
use tokio::task::JoinHandle;

use serde_json::Value;

use crate::cli::{session_rule, AgentCli, AllowScope, PermissionDecision, StartOptions, Transport};
use crate::event::AgentEvent;
use crate::host::{AgentHost, LocalHost};

/// Receives every event of a session (called from a background task).
pub type EventSink = Arc<dyn Fn(AgentEvent) + Send + Sync>;

/// Max bytes of stderr kept to explain a crash.
const STDERR_TAIL: usize = 4000;

/// How long to wait for a finished per-turn process to exit before the next turn kills it.
const TURN_EXIT_WAIT: Duration = Duration::from_secs(5);

type ProcessStdin = Box<dyn AsyncWrite + Send + Unpin>;

/// stdin of a process; `None` once closed.
type Stdin = Arc<Mutex<Option<ProcessStdin>>>;

struct Running {
    stdin: Stdin,
    kill: Option<oneshot::Sender<()>>,
    alive: Arc<AtomicBool>,
    /// Ends once the process is gone and its last events were emitted.
    done: Option<JoinHandle<()>>,
}

impl Running {
    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Kill the process (if still running) and wait until its last events were emitted.
    async fn finish(mut self, wait: Duration) {
        if let Some(done) = self.done.take() {
            let abort = done.abort_handle();
            if let Some(kill) = self.kill.take() {
                let _ = kill.send(());
            }
            if tokio::time::timeout(wait, done).await.is_err() {
                abort.abort();
            }
        }
    }
}

/// Open permission requests and "allow for session" rules.
#[derive(Default)]
struct Permissions {
    /// Request id → (tool name, tool input).
    pending: HashMap<String, (String, Value)>,
    /// Keys from [`session_rule`] the user allowed for the rest of the session.
    allowed: HashSet<String>,
}

async fn write_line(stdin: &Mutex<Option<ProcessStdin>>, line: &str) -> std::io::Result<()> {
    let mut guard = stdin.lock().await;
    let stdin = guard
        .as_mut()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "stdin is closed"))?;
    stdin.write_all(line.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await
}

/// A conversation with an agent CLI. The process is (re)spawned on demand: after an
/// interrupt (and, for per-turn CLIs, after every message) the next message resumes the
/// same conversation via its session id.
pub struct AgentSession {
    adapter: Arc<dyn AgentCli>,
    host: Arc<dyn AgentHost>,
    executable: PathBuf,
    cwd: PathBuf,
    opts: StartOptions,
    sink: EventSink,
    running: Mutex<Option<Running>>,
    conversation_id: Arc<StdMutex<Option<String>>>,
    interrupted: Arc<AtomicBool>,
    permissions: Arc<StdMutex<Permissions>>,
}

impl AgentSession {
    /// Spawn the CLI in `cwd` on this machine. Must be called inside a tokio runtime.
    pub async fn start(
        adapter: Arc<dyn AgentCli>,
        cwd: PathBuf,
        opts: StartOptions,
        sink: EventSink,
    ) -> Result<Self> {
        let executable = adapter.detect(opts.executable.as_deref());
        Self::start_on(Arc::new(LocalHost), adapter, executable, cwd, opts, sink).await
    }

    /// Spawn the CLI on `host` (`executable` as detected there with
    /// [`AgentHost::detect`]); `cwd` is a path on that host.
    pub async fn start_on(
        host: Arc<dyn AgentHost>,
        adapter: Arc<dyn AgentCli>,
        executable: Option<PathBuf>,
        cwd: PathBuf,
        opts: StartOptions,
        sink: EventSink,
    ) -> Result<Self> {
        if !adapter.supported() {
            // Surface the adapter's own explanation.
            adapter.command(&PathBuf::new(), &opts)?;
        }
        let executable = executable.ok_or_else(|| {
            let place = host
                .remote_label()
                .map(|h| format!(" on {h}"))
                .unwrap_or_default();
            ForgeError::Other(format!(
                "{} was not found{place}. Install it or set its path in Settings.",
                adapter.name()
            ))
        })?;
        let session = Self {
            host,
            conversation_id: Arc::new(StdMutex::new(opts.resume.clone())),
            adapter,
            executable,
            cwd,
            opts,
            sink,
            running: Mutex::new(None),
            interrupted: Arc::new(AtomicBool::new(false)),
            permissions: Arc::default(),
        };
        if session.adapter.transport() == Transport::Persistent {
            let running = session.spawn().await?;
            *session.running.lock().await = Some(running);
        }
        Ok(session)
    }

    /// Conversation id reported by the CLI (usable with `resume`).
    pub fn conversation_id(&self) -> Option<String> {
        self.conversation_id.lock().ok()?.clone()
    }

    /// Send a user message, respawning (and resuming) the process if it is not running.
    pub async fn send(&self, text: &str) -> Result<()> {
        let line = self.adapter.encode_user_message(text)?;
        let mut guard = self.running.lock().await;
        if self.adapter.transport() == Transport::PerTurn {
            // The previous turn's process normally exits right after its result.
            if let Some(previous) = guard.take() {
                previous.finish(TURN_EXIT_WAIT).await;
            }
            let running = self.spawn().await?;
            let written = write_line(&running.stdin, &line).await;
            // EOF ends the prompt.
            if let Some(mut stdin) = running.stdin.lock().await.take() {
                let _ = stdin.shutdown().await;
            }
            *guard = Some(running);
            return written.map_err(|e| ForgeError::io(&self.executable, e));
        }
        for attempt in 0..2 {
            let alive = guard.as_ref().is_some_and(Running::is_alive);
            if !alive {
                *guard = Some(self.spawn().await?);
            }
            let running = guard.as_mut().expect("process was just spawned");
            match write_line(&running.stdin, &line).await {
                Ok(()) => return Ok(()),
                Err(e) if attempt == 0 => {
                    tracing::debug!("agent stdin closed ({e}); respawning");
                    running.alive.store(false, Ordering::SeqCst);
                }
                Err(e) => return Err(ForgeError::io(&self.executable, e)),
            }
        }
        unreachable!("send loop returns on the second attempt")
    }

    /// Answer a [`AgentEvent::PermissionRequest`]. "Allow for session" also answers later
    /// requests matching the same [`session_rule`] without asking.
    pub async fn respond_permission(
        &self,
        request_id: &str,
        decision: PermissionDecision,
    ) -> Result<()> {
        let (_tool, input) = {
            let mut perms = self.permissions.lock().expect("permissions lock");
            let (tool, input) = perms.pending.remove(request_id).ok_or_else(|| {
                ForgeError::Other(format!("no pending permission request {request_id}"))
            })?;
            if let PermissionDecision::Allow {
                scope: AllowScope::Session,
                ..
            } = &decision
            {
                perms.allowed.insert(session_rule(&tool, &input));
            }
            (tool, input)
        };
        let line = self
            .adapter
            .encode_permission_response(request_id, &input, &decision)?;
        let guard = self.running.lock().await;
        let running = guard
            .as_ref()
            .filter(|r| r.alive.load(Ordering::SeqCst))
            .ok_or_else(|| {
                ForgeError::Other(format!("{} is no longer running", self.adapter.name()))
            })?;
        write_line(&running.stdin, &line)
            .await
            .map_err(|e| ForgeError::io(&self.executable, e))
    }

    /// Stop the running turn. The conversation can continue with the next `send`. Returns
    /// once the process' last events (`Interrupted`, `Exited`) were emitted, so they never
    /// reach the next turn.
    pub async fn interrupt(&self) {
        let mut guard = self.running.lock().await;
        if let Some(running) = guard.take() {
            if running.is_alive() {
                self.interrupted.store(true, Ordering::SeqCst);
            }
            running.finish(Duration::from_secs(3)).await;
        }
    }

    /// End the session and its process.
    pub async fn stop(&self) {
        if let Some(mut running) = self.running.lock().await.take() {
            if let Some(kill) = running.kill.take() {
                let _ = kill.send(());
            }
        }
    }

    async fn spawn(&self) -> Result<Running> {
        let mut opts = self.opts.clone();
        opts.resume = self.conversation_id();
        let spec = self.adapter.command(&self.executable, &opts)?;
        let program = spec.program.clone();
        let child = self.host.spawn(spec, self.cwd.clone()).await?;
        tracing::info!(
            "started {} in {}{}",
            program.display(),
            self.cwd.display(),
            self.host
                .remote_label()
                .map(|h| format!(" on {h}"))
                .unwrap_or_default()
        );

        let stdin: Stdin = Arc::new(Mutex::new(Some(child.stdin)));
        let stdout = child.stdout;
        let mut stderr = child.stderr;
        let mut exit = child.exit;
        let kill_process = child.kill;
        let alive = Arc::new(AtomicBool::new(true));
        self.interrupted.store(false, Ordering::SeqCst);
        // Requests of a previous process can no longer be answered.
        if let Ok(mut perms) = self.permissions.lock() {
            perms.pending.clear();
        }

        let mut parser = self.adapter.parser(&opts);
        let sink = self.sink.clone();
        let conversation = self.conversation_id.clone();
        let permissions = self.permissions.clone();
        let adapter = self.adapter.clone();
        let reply_to = stdin.clone();
        let cwd = self.cwd.display().to_string();
        let turn_finished = Arc::new(AtomicBool::new(false));
        let finished = turn_finished.clone();
        let reader = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                for mut event in parser.parse_line(&line) {
                    match &mut event {
                        AgentEvent::SessionStarted {
                            session_id,
                            cwd: reported,
                            ..
                        } => {
                            if let Ok(mut id) = conversation.lock() {
                                if !session_id.is_empty() {
                                    *id = Some(session_id.clone());
                                }
                            }
                            // Paths in tool calls are shown relative to it.
                            reported.get_or_insert_with(|| cwd.clone());
                        }
                        AgentEvent::TurnFinished { .. } => finished.store(true, Ordering::SeqCst),
                        AgentEvent::PermissionRequest {
                            request_id,
                            tool_name,
                            input,
                            ..
                        } => {
                            let (request_id, tool_name, input) =
                                (&*request_id, &*tool_name, &*input);
                            let allowed = {
                                let mut perms = permissions.lock().expect("permissions lock");
                                let allowed =
                                    perms.allowed.contains(&session_rule(tool_name, input));
                                if !allowed {
                                    perms.pending.insert(
                                        request_id.clone(),
                                        (tool_name.clone(), input.clone()),
                                    );
                                }
                                allowed
                            };
                            if allowed {
                                let decision = PermissionDecision::Allow {
                                    updated_input: None,
                                    scope: AllowScope::Session,
                                };
                                match adapter
                                    .encode_permission_response(request_id, input, &decision)
                                {
                                    Ok(reply) => {
                                        if let Err(e) = write_line(&reply_to, &reply).await {
                                            tracing::warn!("cannot answer permission request: {e}");
                                        }
                                    }
                                    Err(e) => tracing::warn!("{e}"),
                                }
                                continue;
                            }
                        }
                        _ => {}
                    }
                    sink(event);
                }
            }
        });
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            let _ = stderr.read_to_end(&mut buf).await;
            let text = String::from_utf8_lossy(&buf).trim().to_string();
            let start = text
                .char_indices()
                .rev()
                .nth(STDERR_TAIL)
                .map_or(0, |(i, _)| i);
            text[start..].to_string()
        });

        let (kill_tx, kill_rx) = oneshot::channel::<()>();
        let sink = self.sink.clone();
        let interrupted = self.interrupted.clone();
        let alive_flag = alive.clone();
        let per_turn = self.adapter.transport() == Transport::PerTurn;
        let done = tokio::spawn(async move {
            // `None`: killed by us; `Some(code)` otherwise (`code` is `None` for a signal).
            let status: Option<Option<i32>> = tokio::select! {
                code = &mut exit => Some(code),
                _ = kill_rx => {
                    kill_process();
                    let _ = tokio::time::timeout(Duration::from_secs(2), &mut exit).await;
                    None
                }
            };
            alive_flag.store(false, Ordering::SeqCst);
            // Grandchildren (e.g. a command run by the agent) may keep the pipes open after
            // a kill, so don't wait for EOF forever.
            let grace = Duration::from_millis(if status.is_some() { 2000 } else { 300 });
            let reader_abort = reader.abort_handle();
            if tokio::time::timeout(grace, reader).await.is_err() {
                reader_abort.abort();
            }
            let stderr_abort = stderr_task.abort_handle();
            let stderr = match tokio::time::timeout(grace, stderr_task).await {
                Ok(Ok(s)) => s,
                _ => {
                    stderr_abort.abort();
                    String::new()
                }
            };
            let was_interrupted = interrupted.swap(false, Ordering::SeqCst);
            if per_turn && !was_interrupted && turn_finished.load(Ordering::SeqCst) {
                // A per-turn process ends with its turn; its result already said how the
                // turn went (an error result also exits non-zero).
                return;
            }
            if was_interrupted {
                sink(AgentEvent::Interrupted);
            } else if status.is_some_and(|code| code != Some(0)) {
                let message = if stderr.is_empty() {
                    "the agent process exited unexpectedly".to_string()
                } else {
                    stderr
                };
                sink(AgentEvent::Error { message });
            }
            sink(AgentEvent::Exited {
                code: status.flatten(),
            });
        });

        Ok(Running {
            stdin,
            kill: Some(kill_tx),
            alive,
            done: Some(done),
        })
    }
}
