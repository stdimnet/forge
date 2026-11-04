//! Agent Tasks next to the project's files: the sandbox calls of `forge-agent-tasks`
//! ([`SandboxCall`]) answered by the [`LocalBackend`](crate::LocalBackend) (in the desktop
//! app for a local project, in the Forge agent for a remote one), and processes run to the
//! end through any backend ([`run_to_end`]), so a remote task's commands and tests run on
//! its host.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_agent_tasks::commands::RunOutput;
use forge_agent_tasks::remote::{self, Reply};
use forge_core::{ForgeError, Result};
use forge_process::{CommandLine, RunEvent};
use serde::de::DeserializeOwned;

use crate::protocol::{decode_value, encode_value};
use crate::Backend;

pub use forge_agent_tasks::remote::SandboxCall;

/// Answer a sandbox call for the project at `root`; the sandboxes live in this machine's
/// cache folder. The reply is the MessagePack encoding of the call's reply type.
pub fn handle(root: &Path, call: SandboxCall) -> Result<Vec<u8>> {
    let base = remote::default_base()
        .ok_or_else(|| ForgeError::Other("this machine has no cache folder".into()))?;
    match remote::handle(root, &base, call).map_err(ForgeError::Other)? {
        Reply::Info(i) => encode_value(&i),
        Reply::Changes(c) => encode_value(&c),
        Reply::Versions(v) => encode_value(&v),
        Reply::Applied(a) => encode_value(&a),
        Reply::Removed => encode_value(&()),
        Reply::Tool(t) => encode_value(&t),
    }
}

/// Send `call` to the backend and decode the reply.
pub fn sandbox_call<T: DeserializeOwned>(backend: &dyn Backend, call: SandboxCall) -> Result<T> {
    decode_value(&backend.agent_sandbox(call)?)
}

/// Run `cmd` through `backend` until it ends, `timeout` passes or `cancel` completes
/// (the process is killed then), collecting stdout and stderr like
/// `forge_agent_tasks::commands::run_process`.
pub async fn run_to_end(
    backend: Arc<dyn Backend>,
    cmd: CommandLine,
    timeout: Duration,
    cancel: impl std::future::Future<Output = ()>,
) -> RunOutput {
    let program = cmd.program.clone();
    let out = Arc::new(Mutex::new((String::new(), String::new())));
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<i32>>();
    let tx = Arc::new(Mutex::new(Some(tx)));
    let sink: forge_process::RunSink = {
        let out = out.clone();
        Arc::new(move |event: RunEvent| match event {
            RunEvent::Output { stream, text } => {
                let mut o = out.lock().unwrap_or_else(|e| e.into_inner());
                if matches!(stream, forge_process::OutputStream::Stderr) {
                    o.1.push_str(&text);
                } else {
                    o.0.push_str(&text);
                }
            }
            RunEvent::Exited { code, .. } => {
                if let Some(tx) = tx.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    let _ = tx.send(code);
                }
            }
            RunEvent::Started { .. } => {}
        })
    };
    let started = {
        let backend = backend.clone();
        tokio::task::spawn_blocking(move || backend.start_process(&cmd, sink)).await
    };
    let process = match started {
        Ok(Ok(p)) => p,
        Ok(Err(e)) => {
            return RunOutput {
                exit_code: None,
                output: format!("could not start {program}: {e}"),
                timed_out: false,
            }
        }
        Err(e) => {
            return RunOutput {
                exit_code: None,
                output: format!("could not start {program}: {e}"),
                timed_out: false,
            }
        }
    };
    let collected = |out: &Arc<Mutex<(String, String)>>| {
        let o = out.lock().unwrap_or_else(|e| e.into_inner());
        let mut text = o.0.clone();
        if !o.1.trim().is_empty() {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&o.1);
        }
        text
    };
    tokio::select! {
        r = tokio::time::timeout(timeout, rx) => match r {
            // Output events arrive before the exit, in order.
            Ok(code) => {
                RunOutput { exit_code: code.unwrap_or(None), output: collected(&out), timed_out: false }
            }
            Err(_) => {
                process.kill(Duration::from_secs(2));
                RunOutput {
                    exit_code: None,
                    output: format!(
                        "{}\n{program} timed out after {}s and was stopped",
                        collected(&out),
                        timeout.as_secs()
                    ),
                    timed_out: true,
                }
            }
        },
        _ = cancel => {
            process.kill(Duration::from_secs(2));
            RunOutput { exit_code: None, output: "cancelled".into(), timed_out: false }
        }
    }
}
