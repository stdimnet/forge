//! Runs one agent CLI process per conversation and streams its normalized events.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use forge_core::{ForgeError, Result};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::{oneshot, Mutex};

use serde_json::Value;

use crate::cli::{session_rule, AgentCli, AllowScope, PermissionDecision, StartOptions};
use crate::event::AgentEvent;

/// Receives every event of a session (called from a background task).
pub type EventSink = Arc<dyn Fn(AgentEvent) + Send + Sync>;

/// Max bytes of stderr kept to explain a crash.
const STDERR_TAIL: usize = 4000;

struct Running {
    stdin: Arc<Mutex<ChildStdin>>,
    kill: Option<oneshot::Sender<()>>,
    alive: Arc<AtomicBool>,
}

/// Open permission requests and "allow for session" rules.
#[derive(Default)]
struct Permissions {
    /// Request id → (tool name, tool input).
    pending: HashMap<String, (String, Value)>,
    /// Keys from [`session_rule`] the user allowed for the rest of the session.
    allowed: HashSet<String>,
}

async fn write_line(stdin: &Mutex<ChildStdin>, line: &str) -> std::io::Result<()> {
    let mut stdin = stdin.lock().await;
    stdin.write_all(line.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    stdin.flush().await
}

/// A conversation with an agent CLI. The process is (re)spawned on demand: after an
/// interrupt the next message resumes the same conversation via its session id.
pub struct AgentSession {
    adapter: Arc<dyn AgentCli>,
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
    /// Spawn the CLI in `cwd`. Must be called inside a tokio runtime.
    pub async fn start(
        adapter: Arc<dyn AgentCli>,
        cwd: PathBuf,
        opts: StartOptions,
        sink: EventSink,
    ) -> Result<Self> {
        if !adapter.supported() {
            // Surface the adapter's own explanation.
            adapter.command(&PathBuf::new(), &opts)?;
        }
        let executable = adapter.detect(opts.executable.as_deref()).ok_or_else(|| {
            ForgeError::Other(format!(
                "{} was not found. Install it or set its path in Settings.",
                adapter.name()
            ))
        })?;
        let session = Self {
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
        let running = session.spawn()?;
        *session.running.lock().await = Some(running);
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
        for attempt in 0..2 {
            let alive = guard
                .as_ref()
                .is_some_and(|r| r.alive.load(Ordering::SeqCst));
            if !alive {
                *guard = Some(self.spawn()?);
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

    /// Stop the running turn. The conversation can continue with the next `send`.
    pub async fn interrupt(&self) {
        let mut guard = self.running.lock().await;
        if let Some(running) = guard.as_mut() {
            if running.alive.load(Ordering::SeqCst) {
                self.interrupted.store(true, Ordering::SeqCst);
                if let Some(kill) = running.kill.take() {
                    let _ = kill.send(());
                }
            }
        }
        *guard = None;
    }

    /// End the session and its process.
    pub async fn stop(&self) {
        if let Some(mut running) = self.running.lock().await.take() {
            if let Some(kill) = running.kill.take() {
                let _ = kill.send(());
            }
        }
    }

    fn spawn(&self) -> Result<Running> {
        let mut opts = self.opts.clone();
        opts.resume = self.conversation_id();
        let spec = self.adapter.command(&self.executable, &opts)?;
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args)
            .current_dir(&self.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW: don't flash a console for `claude.cmd`.
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd.spawn().map_err(|e| ForgeError::io(&spec.program, e))?;
        tracing::info!(
            "started {} in {}",
            spec.program.display(),
            self.cwd.display()
        );

        let stdin = Arc::new(Mutex::new(child.stdin.take().expect("stdin is piped")));
        let stdout = child.stdout.take().expect("stdout is piped");
        let mut stderr = child.stderr.take().expect("stderr is piped");
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
        let reader = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                for event in parser.parse_line(&line) {
                    match &event {
                        AgentEvent::SessionStarted { session_id, .. } => {
                            if let Ok(mut id) = conversation.lock() {
                                *id = Some(session_id.clone());
                            }
                        }
                        AgentEvent::PermissionRequest {
                            request_id,
                            tool_name,
                            input,
                            ..
                        } => {
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
        tokio::spawn(async move {
            let status = tokio::select! {
                status = child.wait() => status.ok(),
                _ = kill_rx => {
                    let _ = child.kill().await;
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
            if was_interrupted {
                sink(AgentEvent::Interrupted);
            } else if status.is_some_and(|s| !s.success()) {
                let message = if stderr.is_empty() {
                    "the agent process exited unexpectedly".to_string()
                } else {
                    stderr
                };
                sink(AgentEvent::Error { message });
            }
            sink(AgentEvent::Exited {
                code: status.and_then(|s| s.code()),
            });
        });

        Ok(Running {
            stdin,
            kill: Some(kill_tx),
            alive,
        })
    }
}
