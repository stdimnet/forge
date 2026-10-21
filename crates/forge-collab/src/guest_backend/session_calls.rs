//! A guest session's project calls: sending a [`HostCall`], matching the host's answer, and
//! the events the host pushes (file changes, diagnostics).
//!
//! This file is a child module of `guest` (see the `#[path]` declaration there), so it can use
//! the session's private state.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_backend::FsSink;
use forge_core::FsChanges;
use tokio::runtime::Handle;
use tokio::sync::oneshot;

use super::*;
use crate::guest_backend::calls::{HostCall, LspQuery};

/// Receives the diagnostics of a file (JSON, a list of `EditorDiagnostic`).
pub type DiagnosticsSink = Arc<dyn Fn(String, Vec<u8>) + Send + Sync>;

type Waiter = oneshot::Sender<Result<Vec<u8>, String>>;

pub(crate) struct GuestCalls {
    next: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, Waiter>>>,
    fs: Mutex<Option<FsSink>>,
    diagnostics: Mutex<Option<DiagnosticsSink>>,
    /// The runtime of the session, to time out calls made from blocking threads.
    runtime: Handle,
}

impl GuestCalls {
    pub(crate) fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
            pending: Arc::default(),
            fs: Mutex::new(None),
            diagnostics: Mutex::new(None),
            runtime: Handle::current(),
        }
    }

    fn complete(&self, id: u64, result: Result<Vec<u8>, String>) {
        let waiter = self.pending.lock().expect("calls lock").remove(&id);
        if let Some(tx) = waiter {
            let _ = tx.send(result);
        }
    }

    /// The session is over: every call in flight fails.
    pub(crate) fn fail_all(&self) {
        self.pending.lock().expect("calls lock").clear();
    }

    fn fs_changed(&self, changes: FsChanges) {
        let sink = self.fs.lock().expect("calls lock").clone();
        if let Some(sink) = sink {
            sink(changes);
        }
    }

    fn diagnostics(&self, path: String, diagnostics: Vec<u8>) {
        let sink = self.diagnostics.lock().expect("calls lock").clone();
        if let Some(sink) = sink {
            sink(path, diagnostics);
        }
    }
}

const NO_ANSWER: &str = "The host did not answer.";

impl GuestSession {
    /// Handle the messages of the project backend; `true` if `msg` was one of them.
    pub(crate) fn dispatch_call_message(&self, msg: &HostMessage) -> bool {
        match msg {
            HostMessage::CallResult { id, result } => {
                self.calls
                    .complete(*id, result.clone().map(ByteBuf::into_vec));
                true
            }
            HostMessage::FsChanged {
                changed,
                removed,
                git,
            } => {
                self.calls.fs_changed(FsChanges {
                    changed: changed.clone(),
                    removed: removed.clone(),
                    git: *git,
                });
                true
            }
            HostMessage::Diagnostics { path, diagnostics } => {
                self.calls
                    .diagnostics(path.clone(), diagnostics.clone().into_vec());
                true
            }
            _ => false,
        }
    }

    fn start_call(
        &self,
        call: &HostCall,
        timeout: Duration,
    ) -> Result<oneshot::Receiver<Result<Vec<u8>, String>>, String> {
        if self.state().ended {
            return Err("The session has ended.".into());
        }
        let payload = call.encode().map_err(|e| e.to_string())?;
        let id = self.calls.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.calls
            .pending
            .lock()
            .expect("calls lock")
            .insert(id, tx);
        let pending = self.calls.pending.clone();
        // Dropping the waiter ends the wait of whoever has the receiver.
        self.calls.runtime.spawn(async move {
            tokio::time::sleep(timeout).await;
            pending.lock().expect("calls lock").remove(&id);
        });
        self.send(GuestMessage::Call {
            id,
            payload: ByteBuf::from(payload),
        });
        Ok(rx)
    }

    /// Ask the host and wait for its answer (the MessagePack encoding of the call's reply).
    pub async fn call(&self, call: &HostCall, timeout: Duration) -> Result<Vec<u8>, String> {
        let rx = self.start_call(call, timeout)?;
        match rx.await {
            Ok(result) => result,
            Err(_) => Err(NO_ANSWER.into()),
        }
    }

    /// [`call`](Self::call) for threads that may block (the `Backend` methods). Must not be
    /// used from the async runtime's worker threads.
    pub fn call_blocking(&self, call: &HostCall, timeout: Duration) -> Result<Vec<u8>, String> {
        let rx = self.start_call(call, timeout)?;
        match rx.blocking_recv() {
            Ok(result) => result,
            Err(_) => Err(NO_ANSWER.into()),
        }
    }

    /// The round trip to the host, in milliseconds.
    pub async fn ping(&self) -> Result<u64, String> {
        let start = std::time::Instant::now();
        self.call(&HostCall::Ping, Duration::from_secs(10)).await?;
        Ok(start.elapsed().as_millis() as u64)
    }

    /// A language server request answered by the host's servers (JSON reply).
    pub async fn lsp(&self, query: LspQuery) -> Result<Vec<u8>, String> {
        self.call(&HostCall::Lsp(query), Duration::from_secs(60))
            .await
    }

    /// Where file changes of the host's project go once the guest called `Watch`.
    pub fn set_fs_sink(&self, sink: Option<FsSink>) {
        *self.calls.fs.lock().expect("calls lock") = sink;
    }

    /// Where the host's language server diagnostics for the guest's open files go.
    pub fn set_diagnostics_sink(&self, sink: Option<DiagnosticsSink>) {
        *self.calls.diagnostics.lock().expect("calls lock") = sink;
    }

    /// The host's name and its label (`name (machine)`).
    pub fn host(&self) -> (String, String) {
        let st = self.state();
        let name = st
            .participants
            .iter()
            .find(|p| p.id == ParticipantId::HOST)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        (name, self.host_label.clone())
    }
}
