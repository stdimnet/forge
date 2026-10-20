//! The client end of the agent [`protocol`](crate::protocol): calls with out-of-order
//! answers, pushed events and byte channels over one stream pair.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::task::{Context, Poll};
use std::time::Duration;

use forge_core::{ForgeError, Result};
use serde::de::DeserializeOwned;
use serde_bytes::ByteBuf;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::sync::{mpsc, oneshot, watch};

use crate::protocol::{decode_value, io, Call, Event, Frame};

/// Receives the agent's events (from the connection's reader task: keep it quick).
pub type EventHandler = Arc<dyn Fn(Event) + Send + Sync>;

/// Message on a byte channel: data, or `None` when the peer closed it.
pub type ChannelMsg = Option<Vec<u8>>;

type Answer = std::result::Result<Vec<u8>, String>;

enum Waiter {
    Async(oneshot::Sender<Answer>),
    Blocking(std::sync::mpsc::Sender<Answer>),
}

impl Waiter {
    fn send(self, answer: Answer) {
        match self {
            Waiter::Async(tx) => {
                let _ = tx.send(answer);
            }
            Waiter::Blocking(tx) => {
                let _ = tx.send(answer);
            }
        }
    }
}

struct Inner {
    out: mpsc::UnboundedSender<Option<Frame>>,
    pending: Mutex<HashMap<u64, Waiter>>,
    next_id: AtomicU64,
    channels: Mutex<HashMap<u32, mpsc::UnboundedSender<ChannelMsg>>>,
    next_channel: AtomicU32,
    handler: RwLock<Option<EventHandler>>,
    closed: AtomicBool,
    closed_tx: watch::Sender<bool>,
}

/// Connection to a Forge agent. Cheap to clone; all clones share the connection.
#[derive(Clone)]
pub struct RpcClient {
    inner: Arc<Inner>,
}

/// Error for calls on a lost connection.
pub fn connection_lost() -> ForgeError {
    ForgeError::Other("the connection to the remote host was lost".into())
}

impl RpcClient {
    /// Start the reader and writer tasks for `reader` / `writer` (the agent's stdout and
    /// stdin) on the current tokio runtime.
    pub fn spawn<R, W>(reader: R, writer: W) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (out, mut out_rx) = mpsc::unbounded_channel::<Option<Frame>>();
        let inner = Arc::new(Inner {
            out,
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            channels: Mutex::new(HashMap::new()),
            next_channel: AtomicU32::new(1),
            handler: RwLock::new(None),
            closed: AtomicBool::new(false),
            closed_tx: watch::channel(false).0,
        });

        let weak = Arc::downgrade(&inner);
        tokio::spawn(async move {
            let mut writer = writer;
            while let Some(Some(frame)) = out_rx.recv().await {
                if let Err(e) = io::write_frame(&mut writer, &frame).await {
                    tracing::debug!("agent connection write failed: {e}");
                    break;
                }
            }
            let _ = writer.shutdown().await;
            if let Some(inner) = weak.upgrade() {
                mark_closed(&inner);
            }
        });

        let weak = Arc::downgrade(&inner);
        tokio::spawn(async move {
            let mut reader = reader;
            loop {
                let frame = io::read_frame(&mut reader).await;
                let Some(inner) = weak.upgrade() else { break };
                match frame {
                    Ok(Some(frame)) => dispatch(&inner, frame),
                    Ok(None) => {
                        mark_closed(&inner);
                        break;
                    }
                    Err(e) => {
                        tracing::warn!("agent connection: {e}");
                        mark_closed(&inner);
                        break;
                    }
                }
            }
        });

        RpcClient { inner }
    }

    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::SeqCst)
    }

    /// Resolves when the connection is gone (stream ended, write failed or [`shutdown`](Self::shutdown)).
    pub async fn closed(&self) {
        let mut rx = self.inner.closed_tx.subscribe();
        while !*rx.borrow_and_update() {
            if rx.changed().await.is_err() {
                return;
            }
        }
    }

    /// Close the connection (the agent sees the end of its input and exits).
    pub fn shutdown(&self) {
        let _ = self.inner.out.send(None);
    }

    /// Route the agent's events to `handler` (replacing the previous one).
    pub fn set_event_handler(&self, handler: EventHandler) {
        *self.inner.handler.write().expect("handler lock") = Some(handler);
    }

    fn send(&self, frame: Frame) -> Result<()> {
        if self.is_closed() {
            return Err(connection_lost());
        }
        self.inner
            .out
            .send(Some(frame))
            .map_err(|_| connection_lost())
    }

    fn register(&self, waiter: Waiter) -> u64 {
        let id = self.inner.next_id.fetch_add(1, Ordering::SeqCst);
        self.inner
            .pending
            .lock()
            .expect("pending lock")
            .insert(id, waiter);
        id
    }

    fn forget(&self, id: u64) {
        self.inner.pending.lock().expect("pending lock").remove(&id);
    }

    /// Call the agent and wait for the answer.
    pub async fn call<T: DeserializeOwned>(&self, call: Call, timeout: Duration) -> Result<T> {
        let (tx, rx) = oneshot::channel();
        let id = self.register(Waiter::Async(tx));
        if let Err(e) = self.send(Frame::Request { id, call }) {
            self.forget(id);
            return Err(e);
        }
        let answer = match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(a)) => a,
            Ok(Err(_)) => return Err(connection_lost()),
            Err(_) => {
                self.forget(id);
                return Err(ForgeError::Other(
                    "the remote host did not answer in time".into(),
                ));
            }
        };
        decode_answer(answer)
    }

    /// Call the agent from a thread that may block (never from an async task).
    pub fn call_blocking<T: DeserializeOwned>(&self, call: Call, timeout: Duration) -> Result<T> {
        decode_value(&self.call_raw_blocking(call, timeout)?)
    }

    /// Like [`call_blocking`](Self::call_blocking), returning the encoded reply.
    pub fn call_raw_blocking(&self, call: Call, timeout: Duration) -> Result<Vec<u8>> {
        let (tx, rx) = std::sync::mpsc::channel();
        let id = self.register(Waiter::Blocking(tx));
        if let Err(e) = self.send(Frame::Request { id, call }) {
            self.forget(id);
            return Err(e);
        }
        match rx.recv_timeout(timeout) {
            Ok(answer) => answer.map_err(ForgeError::Other),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                self.forget(id);
                Err(ForgeError::Other(
                    "the remote host did not answer in time".into(),
                ))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(connection_lost()),
        }
    }

    /// A fresh channel id (for calls that start a stream on a client-chosen channel).
    pub fn new_channel(&self) -> u32 {
        self.inner.next_channel.fetch_add(1, Ordering::SeqCst)
    }

    /// A fresh channel whose incoming data the caller reads.
    pub fn open_channel(&self) -> (u32, mpsc::UnboundedReceiver<ChannelMsg>) {
        let id = self.new_channel();
        let (tx, rx) = mpsc::unbounded_channel();
        self.inner
            .channels
            .lock()
            .expect("channels lock")
            .insert(id, tx);
        (id, rx)
    }

    pub fn send_data(&self, channel: u32, data: Vec<u8>) -> Result<()> {
        self.send(Frame::Data {
            channel,
            data: ByteBuf::from(data),
        })
    }

    /// Close a channel (the agent stops what runs on it).
    pub fn close_channel(&self, channel: u32) {
        self.inner
            .channels
            .lock()
            .expect("channels lock")
            .remove(&channel);
        let _ = self.send(Frame::Close { channel });
    }

    /// Async reader / writer halves for a channel opened with [`open_channel`](Self::open_channel).
    pub fn channel_stream(
        &self,
        channel: u32,
        rx: mpsc::UnboundedReceiver<ChannelMsg>,
    ) -> (ChannelReader, ChannelWriter) {
        (
            ChannelReader {
                rx,
                buf: Vec::new(),
                pos: 0,
            },
            ChannelWriter {
                client: self.clone(),
                channel,
            },
        )
    }
}

fn decode_answer<T: DeserializeOwned>(answer: Answer) -> Result<T> {
    match answer {
        Ok(bytes) => decode_value(&bytes),
        Err(message) => Err(ForgeError::Other(message)),
    }
}

fn dispatch(inner: &Arc<Inner>, frame: Frame) {
    match frame {
        Frame::Response { id, result } => {
            let waiter = inner.pending.lock().expect("pending lock").remove(&id);
            if let Some(w) = waiter {
                w.send(result.map(ByteBuf::into_vec));
            }
        }
        Frame::Event(event) => {
            let handler = inner.handler.read().expect("handler lock").clone();
            if let Some(h) = handler {
                h(event);
            }
        }
        Frame::Data { channel, data } => {
            let tx = inner
                .channels
                .lock()
                .expect("channels lock")
                .get(&channel)
                .cloned();
            if let Some(tx) = tx {
                let _ = tx.send(Some(data.into_vec()));
            }
        }
        Frame::Close { channel } => {
            let tx = inner
                .channels
                .lock()
                .expect("channels lock")
                .remove(&channel);
            if let Some(tx) = tx {
                let _ = tx.send(None);
            }
        }
        Frame::Request { id, .. } => {
            tracing::warn!("unexpected request {id} from the agent");
        }
    }
}

fn mark_closed(inner: &Arc<Inner>) {
    if inner.closed.swap(true, Ordering::SeqCst) {
        return;
    }
    let pending: Vec<Waiter> = inner
        .pending
        .lock()
        .expect("pending lock")
        .drain()
        .map(|(_, w)| w)
        .collect();
    for w in pending {
        w.send(Err(connection_lost().to_string()));
    }
    for (_, tx) in inner.channels.lock().expect("channels lock").drain() {
        let _ = tx.send(None);
    }
    let _ = inner.closed_tx.send(true);
}

/// Incoming bytes of a channel as an [`AsyncRead`]; ends when the channel closes.
pub struct ChannelReader {
    rx: mpsc::UnboundedReceiver<ChannelMsg>,
    buf: Vec<u8>,
    pos: usize,
}

impl AsyncRead for ChannelReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        out: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        loop {
            if self.pos < self.buf.len() {
                let n = (self.buf.len() - self.pos).min(out.remaining());
                let start = self.pos;
                out.put_slice(&self.buf[start..start + n]);
                self.pos += n;
                return Poll::Ready(Ok(()));
            }
            match self.rx.poll_recv(cx) {
                Poll::Ready(Some(Some(data))) => {
                    self.buf = data;
                    self.pos = 0;
                }
                // Closed: end of stream.
                Poll::Ready(Some(None)) | Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Writes to a channel as an [`AsyncWrite`]; shutting it down closes the channel.
pub struct ChannelWriter {
    client: RpcClient,
    channel: u32,
}

impl AsyncWrite for ChannelWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.client.send_data(self.channel, data.to_vec()) {
            Ok(()) => Poll::Ready(Ok(data.len())),
            Err(e) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                e.to_string(),
            ))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.client.close_channel(self.channel);
        Poll::Ready(Ok(()))
    }
}
