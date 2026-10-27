//! Kernels over ZeroMQ (the pure-Rust `zeromq` crate, MIT): DEALER sockets for shell and
//! control, a SUB socket for iopub. Messages are signed with the connection's key; messages
//! with a wrong signature are dropped.

use std::time::{Duration, Instant};

use bytes::Bytes;
use tokio::sync::{mpsc, Mutex};
use zeromq::{DealerSendHalf, DealerSocket, Socket, SocketRecv, SocketSend, SubSocket, ZmqMessage};

use crate::protocol::{decode, encode, ConnectionInfo, Message, Signer};
use crate::transport::{disconnected, Channel, Incoming, TransportError};

/// Connected sockets of one kernel.
pub struct ZmqKernel {
    signer: Signer,
    shell: Mutex<DealerSendHalf>,
    control: Mutex<DealerSendHalf>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for ZmqKernel {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

fn frames_of(m: ZmqMessage) -> Vec<Vec<u8>> {
    m.into_vec().into_iter().map(|b| b.to_vec()).collect()
}

fn zmq_message(frames: Vec<Vec<u8>>) -> Result<ZmqMessage, TransportError> {
    ZmqMessage::try_from(frames.into_iter().map(Bytes::from).collect::<Vec<_>>())
        .map_err(|e| TransportError(e.to_string()))
}

/// Connect, retrying while the kernel is still starting.
async fn connect<S: Socket>(
    socket: &mut S,
    endpoint: &str,
    deadline: Instant,
) -> Result<(), TransportError> {
    loop {
        match socket.connect(endpoint).await {
            Ok(()) => return Ok(()),
            Err(e) if Instant::now() >= deadline => {
                return Err(TransportError(format!(
                    "cannot connect to the kernel at {endpoint}: {e}"
                )))
            }
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
}

fn spawn_reader<R>(
    mut recv: R,
    channel: Channel,
    signer: Signer,
    tx: Incoming,
) -> tokio::task::JoinHandle<()>
where
    R: SocketRecv + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            match recv.recv().await {
                Ok(m) => match decode(&frames_of(m), &signer) {
                    Ok(msg) => {
                        if tx.send((channel, msg)).is_err() {
                            return;
                        }
                    }
                    Err(e) => tracing::warn!("dropped a kernel message: {e}"),
                },
                Err(_) => {
                    let _ = tx.send((channel, disconnected()));
                    return;
                }
            }
        }
    })
}

impl ZmqKernel {
    /// Connect to a kernel's shell, control and iopub sockets; incoming messages go to `tx`.
    pub async fn connect(
        info: &ConnectionInfo,
        tx: Incoming,
        timeout: Duration,
    ) -> Result<Self, TransportError> {
        if info.signature_scheme != "hmac-sha256" && !info.key.is_empty() {
            return Err(TransportError(format!(
                "unsupported signature scheme {}",
                info.signature_scheme
            )));
        }
        let deadline = Instant::now() + timeout;
        let signer = Signer::new(&info.key);
        let mut shell = DealerSocket::new();
        connect(&mut shell, &info.endpoint(info.shell_port), deadline).await?;
        let mut control = DealerSocket::new();
        connect(&mut control, &info.endpoint(info.control_port), deadline).await?;
        let mut iopub = SubSocket::new();
        connect(&mut iopub, &info.endpoint(info.iopub_port), deadline).await?;
        iopub
            .subscribe("")
            .await
            .map_err(|e| TransportError(format!("cannot subscribe to the kernel's output: {e}")))?;
        let (shell_tx, shell_rx) = shell.split();
        let (control_tx, control_rx) = control.split();
        let tasks = vec![
            spawn_reader(shell_rx, Channel::Shell, signer.clone(), tx.clone()),
            spawn_reader(control_rx, Channel::Control, signer.clone(), tx.clone()),
            spawn_reader(iopub, Channel::Iopub, signer.clone(), tx),
        ];
        Ok(ZmqKernel {
            signer,
            shell: Mutex::new(shell_tx),
            control: Mutex::new(control_tx),
            tasks,
        })
    }

    pub async fn send(&self, channel: Channel, msg: &Message) -> Result<(), TransportError> {
        let frames = zmq_message(encode(msg, &self.signer))?;
        let socket = match channel {
            Channel::Control => &self.control,
            _ => &self.shell,
        };
        socket
            .lock()
            .await
            .send(frames)
            .await
            .map_err(|e| TransportError(format!("cannot send to the kernel: {e}")))
    }
}

/// A random signing key for a new kernel's connection.
pub fn new_key() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Five free TCP ports on 127.0.0.1 for a new kernel (shell, iopub, stdin, control, hb).
pub fn free_ports() -> std::io::Result<[u16; 5]> {
    let listeners = (0..5)
        .map(|_| std::net::TcpListener::bind("127.0.0.1:0"))
        .collect::<std::io::Result<Vec<_>>>()?;
    let mut ports = [0u16; 5];
    for (p, l) in ports.iter_mut().zip(&listeners) {
        *p = l.local_addr()?.port();
    }
    Ok(ports)
}

/// Write a connection file only the user can read (it holds the signing key).
pub fn write_connection_file(path: &std::path::Path, info: &ConnectionInfo) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(info).expect("connection info serializes");
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(text.as_bytes())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, text)
    }
}

/// `mpsc` channel type re-exported for callers.
pub type IncomingReceiver = mpsc::UnboundedReceiver<(Channel, Message)>;
