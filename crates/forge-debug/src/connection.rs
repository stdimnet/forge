//! One DBGp connection: writes commands with transaction ids and routes the engine's
//! answers back to the waiting caller. Packets that are not answers (`stream`, `notify`)
//! go to a callback.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tokio::io::{AsyncBufRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::oneshot;

use crate::error::{DebugError, DebugResult};
use crate::protocol::{encode_command, read_packet};
use crate::xml::{self, Element};

type Writer = Box<dyn AsyncWrite + Send + Unpin>;

pub struct Connection {
    writer: tokio::sync::Mutex<Writer>,
    next_tid: AtomicU32,
    pending: Mutex<HashMap<u32, oneshot::Sender<Element>>>,
    closed: AtomicBool,
    timeout: Duration,
}

impl Connection {
    pub fn new(writer: Writer, timeout: Duration) -> Self {
        Self {
            writer: tokio::sync::Mutex::new(writer),
            next_tid: AtomicU32::new(1),
            pending: Mutex::new(HashMap::new()),
            closed: AtomicBool::new(false),
            timeout,
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// Read packets until the engine disconnects. Answers resolve their pending command;
    /// everything else is passed to `on_message`.
    pub async fn read_loop<R: AsyncBufRead + Unpin>(
        &self,
        mut reader: R,
        on_message: impl Fn(Element),
    ) {
        loop {
            match read_packet(&mut reader).await {
                Ok(Some(text)) => match xml::parse(&text) {
                    Ok(el) => self.dispatch(el, &on_message),
                    Err(e) => tracing::warn!("dropping malformed DBGp packet: {e}"),
                },
                Ok(None) => break,
                Err(e) => {
                    tracing::debug!("debug connection closed: {e}");
                    break;
                }
            }
        }
        self.mark_closed();
    }

    fn dispatch(&self, el: Element, on_message: &impl Fn(Element)) {
        if el.local_name() == "response" {
            if let Some(tid) = el.attr_u32("transaction_id") {
                let waiter = self.pending.lock().expect("pending lock").remove(&tid);
                if let Some(tx) = waiter {
                    let _ = tx.send(el);
                    return;
                }
            }
        }
        on_message(el);
    }

    fn mark_closed(&self) {
        self.closed.store(true, Ordering::SeqCst);
        // Dropping the senders wakes every waiter with `Closed`.
        self.pending.lock().expect("pending lock").clear();
    }

    async fn write(
        &self,
        name: &str,
        args: &[(char, String)],
        data: Option<&str>,
    ) -> DebugResult<oneshot::Receiver<Element>> {
        if self.is_closed() {
            return Err(DebugError::Closed);
        }
        let tid = self.next_tid.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().expect("pending lock").insert(tid, tx);
        let bytes = encode_command(name, tid, args, data);
        let mut w = self.writer.lock().await;
        if let Err(e) = async {
            w.write_all(&bytes).await?;
            w.flush().await
        }
        .await
        {
            self.pending.lock().expect("pending lock").remove(&tid);
            return Err(e.into());
        }
        Ok(rx)
    }

    /// Send a command and wait for its answer (bounded by the connection timeout).
    pub async fn send(
        &self,
        name: &str,
        args: &[(char, String)],
        data: Option<&str>,
    ) -> DebugResult<Element> {
        self.send_with_timeout(name, args, data, self.timeout).await
    }

    pub async fn send_with_timeout(
        &self,
        name: &str,
        args: &[(char, String)],
        data: Option<&str>,
        timeout: Duration,
    ) -> DebugResult<Element> {
        let rx = self.write(name, args, data).await?;
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(el)) => Ok(el),
            Ok(Err(_)) => Err(DebugError::Closed),
            Err(_) => Err(DebugError::Timeout),
        }
    }

    /// Send a continuation command (`run`, `step_*`); its answer arrives whenever the
    /// script suspends again, so there is no timeout.
    pub async fn send_untimed(&self, name: &str, args: &[(char, String)]) -> DebugResult<Element> {
        let rx = self.write(name, args, None).await?;
        rx.await.map_err(|_| DebugError::Closed)
    }

    /// Close the socket's write side; the engine then ends the session.
    pub async fn shutdown(&self) {
        let mut w = self.writer.lock().await;
        let _ = w.shutdown().await;
        drop(w);
        self.mark_closed();
    }
}
