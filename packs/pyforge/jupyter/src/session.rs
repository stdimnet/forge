//! A notebook's kernel session over either connection: runs cells and follows their
//! messages ([`SessionEvent`]s keyed by cell id, with outputs already rendered), answers
//! requests (variables, View as DataFrame) and interrupts, restarts or shuts the kernel down.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

use crate::dataframe::DataFrame;
use crate::kernel::ZmqKernel;
use crate::output::{render, Output, Rendered};
use crate::protocol::{
    event_of, execute_content, user_expression_json, ConnectionInfo, KernelEvent, Message,
};
use crate::server::{ServerAddress, ServerKernel};
use crate::transport::{Channel, TransportError, DISCONNECTED};
use crate::variables::Variable;

/// The Python helper of the Variables view and View as DataFrame.
pub const HELPER: &str = include_str!("forge_jupyter.py");

/// What the notebook editor hears from its kernel.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SessionEvent {
    /// `starting`, `idle`, `busy`, `restarting`, `dead`.
    Status {
        state: String,
    },
    ExecuteInput {
        cell_id: String,
        execution_count: Option<u64>,
    },
    Output {
        cell_id: String,
        output: Output,
        rendered: Rendered,
        #[serde(skip_serializing_if = "Option::is_none")]
        display_id: Option<String>,
    },
    UpdateDisplay {
        display_id: String,
        output: Output,
        rendered: Rendered,
    },
    ClearOutput {
        cell_id: String,
        wait: bool,
    },
    /// The cell finished: `ok`, `error` or `aborted`.
    Done {
        cell_id: String,
        status: String,
        execution_count: Option<u64>,
    },
    /// The connection to the kernel ended.
    Disconnected,
}

pub type EventSink = Arc<dyn Fn(SessionEvent) + Send + Sync>;

/// Where the kernel runs.
pub enum Connection {
    Zmq(ZmqKernel),
    Server(ServerKernel),
}

struct Running {
    cell_id: String,
    idle: bool,
    replied: bool,
}

#[derive(Default)]
struct Tracking {
    /// Request → cell, kept until both its reply and its last output (idle) arrived.
    cells: HashMap<String, Running>,
    /// Request → whoever waits for its reply.
    replies: HashMap<String, oneshot::Sender<Message>>,
}

pub struct Session {
    pub id: String,
    conn: Connection,
    tracking: Arc<Mutex<Tracking>>,
    reader: tokio::task::JoinHandle<()>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

impl Session {
    /// Connect to a local kernel (started by the caller with `info`'s connection file) and
    /// wait until it answers.
    pub async fn connect_zmq(
        info: &ConnectionInfo,
        root: Option<String>,
        sink: EventSink,
        timeout: Duration,
    ) -> Result<Session, TransportError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let conn = Connection::Zmq(ZmqKernel::connect(info, tx, timeout).await?);
        let s = Session::with(conn, rx, root, sink);
        s.wait_ready(timeout).await?;
        Ok(s)
    }

    /// Start a kernel on a Jupyter server.
    pub async fn connect_server(
        server: ServerAddress,
        kernel_name: &str,
        root: Option<String>,
        sink: EventSink,
        timeout: Duration,
    ) -> Result<Session, TransportError> {
        let (tx, rx) = mpsc::unbounded_channel();
        let id = new_id();
        let conn = Connection::Server(ServerKernel::start(server, kernel_name, &id, tx).await?);
        let s = Session::with(conn, rx, root, sink);
        s.wait_ready(timeout).await?;
        Ok(s)
    }

    fn with(
        conn: Connection,
        mut rx: mpsc::UnboundedReceiver<(Channel, Message)>,
        root: Option<String>,
        sink: EventSink,
    ) -> Session {
        let tracking = Arc::new(Mutex::new(Tracking::default()));
        let t = tracking.clone();
        let reader = tokio::spawn(async move {
            while let Some((channel, msg)) = rx.recv().await {
                if msg.msg_type() == DISCONNECTED {
                    sink(SessionEvent::Disconnected);
                    return;
                }
                handle(&t, &sink, root.as_deref(), channel, msg);
            }
        });
        Session {
            id: new_id(),
            conn,
            tracking,
            reader,
        }
    }

    /// Ask `kernel_info` until the kernel replies and its output channel delivers (a SUB
    /// socket misses what is published before it is connected).
    async fn wait_ready(&self, timeout: Duration) -> Result<(), TransportError> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            match self
                .request(
                    Channel::Shell,
                    "kernel_info_request",
                    json!({}),
                    Duration::from_millis(1000),
                )
                .await
            {
                Ok(_) => return Ok(()),
                Err(e) if tokio::time::Instant::now() >= deadline => {
                    return Err(TransportError(format!("the kernel did not answer: {e}")))
                }
                Err(_) => {}
            }
        }
    }

    fn message(&self, msg_type: &str, content: Value) -> Message {
        Message::new(&self.id, &new_id(), msg_type, content)
    }

    async fn send(&self, channel: Channel, msg: &Message) -> Result<(), TransportError> {
        match &self.conn {
            Connection::Zmq(k) => k.send(channel, msg).await,
            Connection::Server(k) => k.send(channel, msg),
        }
    }

    /// Send a request and wait for its reply.
    pub async fn request(
        &self,
        channel: Channel,
        msg_type: &str,
        content: Value,
        timeout: Duration,
    ) -> Result<Message, TransportError> {
        let msg = self.message(msg_type, content);
        let (tx, rx) = oneshot::channel();
        self.tracking
            .lock()
            .expect("tracking")
            .replies
            .insert(msg.header.msg_id.clone(), tx);
        let sent = self.send(channel, &msg).await;
        let result = match sent {
            Ok(()) => match tokio::time::timeout(timeout, rx).await {
                Ok(Ok(reply)) => Ok(reply),
                Ok(Err(_)) => Err(TransportError("the kernel connection closed".into())),
                Err(_) => Err(TransportError(format!("no {msg_type} reply in time"))),
            },
            Err(e) => Err(e),
        };
        self.tracking
            .lock()
            .expect("tracking")
            .replies
            .remove(&msg.header.msg_id);
        result
    }

    /// Run a cell; its messages arrive as events with `cell_id`. Returns the request id.
    pub async fn execute(&self, cell_id: &str, code: &str) -> Result<String, TransportError> {
        let msg = self.message("execute_request", execute_content(code, false, None));
        let id = msg.header.msg_id.clone();
        self.tracking.lock().expect("tracking").cells.insert(
            id.clone(),
            Running {
                cell_id: cell_id.to_string(),
                idle: false,
                replied: false,
            },
        );
        if let Err(e) = self.send(Channel::Shell, &msg).await {
            self.tracking.lock().expect("tracking").cells.remove(&id);
            return Err(e);
        }
        Ok(id)
    }

    /// Evaluate `expression` (JSON-valued, see [`crate::protocol::json_expression`]) after
    /// defining the helper, without touching the history or the execution count.
    pub async fn evaluate(&self, expression: &str) -> Result<Value, TransportError> {
        let content = execute_content(HELPER, true, Some(json!({ "forge": expression })));
        let reply = self
            .request(Channel::Shell, "execute_request", content, REQUEST_TIMEOUT)
            .await?;
        if reply.content.get("status").and_then(Value::as_str) == Some("error") {
            let c = &reply.content;
            return Err(TransportError(format!(
                "{}: {}",
                c.get("ename").and_then(Value::as_str).unwrap_or("Error"),
                c.get("evalue").and_then(Value::as_str).unwrap_or("")
            )));
        }
        user_expression_json(&reply.content, "forge").map_err(|e| TransportError(e.0))
    }

    pub async fn variables(&self) -> Result<Vec<Variable>, TransportError> {
        let v = self
            .evaluate("__forge_jupyter.as_json(__forge_jupyter.variables())")
            .await?;
        serde_json::from_value(v).map_err(|e| TransportError(e.to_string()))
    }

    pub async fn children(&self, expr: &str) -> Result<Vec<Variable>, TransportError> {
        let v = self
            .evaluate(&format!(
                "__forge_jupyter.as_json(__forge_jupyter.children({}))",
                py_string(expr)
            ))
            .await?;
        serde_json::from_value(v).map_err(|e| TransportError(e.to_string()))
    }

    pub async fn frame(&self, expr: &str, limit: usize) -> Result<DataFrame, TransportError> {
        let v = self
            .evaluate(&format!(
                "__forge_jupyter.as_json(__forge_jupyter.frame({}, {limit}))",
                py_string(expr)
            ))
            .await?;
        serde_json::from_value(v).map_err(|e| TransportError(e.to_string()))
    }

    /// Interrupt the running cell.
    pub async fn interrupt(&self) -> Result<(), TransportError> {
        match &self.conn {
            Connection::Zmq(_) => self
                .request(
                    Channel::Control,
                    "interrupt_request",
                    json!({}),
                    Duration::from_secs(5),
                )
                .await
                .map(|_| ()),
            Connection::Server(k) => k.interrupt().await,
        }
    }

    /// Restart in place (server kernels). Returns false for local kernels, whose process the
    /// caller starts again.
    pub async fn restart(&self) -> Result<bool, TransportError> {
        self.forget_cells();
        match &self.conn {
            Connection::Zmq(_) => Ok(false),
            Connection::Server(k) => k.restart().await.map(|_| true),
        }
    }

    /// Ask the kernel to exit (server kernels are deleted).
    pub async fn shutdown(&self) -> Result<(), TransportError> {
        match &self.conn {
            Connection::Zmq(_) => self
                .request(
                    Channel::Control,
                    "shutdown_request",
                    json!({ "restart": false }),
                    Duration::from_secs(3),
                )
                .await
                .map(|_| ()),
            Connection::Server(k) => k.shutdown().await,
        }
    }

    fn forget_cells(&self) {
        self.tracking.lock().expect("tracking").cells.clear();
    }

    pub fn is_server(&self) -> bool {
        matches!(self.conn, Connection::Server(_))
    }
}

/// A Python string literal.
pub fn py_string(s: &str) -> String {
    serde_json::to_string(s).expect("strings serialize")
}

fn handle(
    tracking: &Mutex<Tracking>,
    sink: &EventSink,
    root: Option<&str>,
    channel: Channel,
    msg: Message,
) {
    let parent = msg.parent_id().map(str::to_string);
    if matches!(channel, Channel::Shell | Channel::Control) {
        if let Some(p) = &parent {
            let waiter = tracking.lock().expect("tracking").replies.remove(p);
            if let Some(w) = waiter {
                let _ = w.send(msg.clone());
            }
        }
    }
    let Some(event) = event_of(&msg) else { return };
    let cell_of = |p: &str| {
        tracking
            .lock()
            .expect("tracking")
            .cells
            .get(p)
            .map(|r| r.cell_id.clone())
    };
    // Forget a request once its reply and its final idle status are both in.
    let finish = |p: &str, idle: bool| {
        let mut t = tracking.lock().expect("tracking");
        let done = match t.cells.get_mut(p) {
            Some(r) => {
                if idle {
                    r.idle = true;
                } else {
                    r.replied = true;
                }
                r.idle && r.replied
            }
            None => false,
        };
        if done {
            t.cells.remove(p);
        }
    };
    let out = match event {
        KernelEvent::Status { state, parent } => {
            // Status of helper requests (variables) is noise; cells and the kernel count.
            let ours = match parent.as_deref() {
                None => true,
                Some(p) => cell_of(p).is_some(),
            };
            if !ours && !matches!(state.as_str(), "starting" | "restarting" | "dead") {
                return;
            }
            if let (Some(p), "idle") = (parent.as_deref(), state.as_str()) {
                finish(p, true);
            }
            SessionEvent::Status { state }
        }
        KernelEvent::ExecuteInput {
            parent,
            execution_count,
        } => {
            let Some(cell_id) = cell_of(&parent) else {
                return;
            };
            SessionEvent::ExecuteInput {
                cell_id,
                execution_count,
            }
        }
        KernelEvent::Output {
            parent,
            output,
            display_id,
        } => {
            let Some(cell_id) = cell_of(&parent) else {
                return;
            };
            let rendered = render(&output, root);
            SessionEvent::Output {
                cell_id,
                output,
                rendered,
                display_id,
            }
        }
        KernelEvent::UpdateDisplay { display_id, output } => {
            let rendered = render(&output, root);
            SessionEvent::UpdateDisplay {
                display_id,
                output,
                rendered,
            }
        }
        KernelEvent::ClearOutput { parent, wait } => {
            let Some(cell_id) = cell_of(&parent) else {
                return;
            };
            SessionEvent::ClearOutput { cell_id, wait }
        }
        KernelEvent::ExecuteReply {
            parent,
            status,
            execution_count,
        } => {
            let Some(cell_id) = cell_of(&parent) else {
                return;
            };
            finish(&parent, false);
            SessionEvent::Done {
                cell_id,
                status,
                execution_count,
            }
        }
    };
    sink(out);
}
