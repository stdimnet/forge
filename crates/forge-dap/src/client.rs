//! One connection to a debug adapter: sends requests with sequence numbers, routes the
//! adapter's responses back to the waiting caller and hands events and reverse requests
//! (`runInTerminal`, `startDebugging`) to the session.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};

use crate::codec::{encode, read_message};
use crate::error::{DapError, DapResult};
use crate::protocol::{Event, Message, Request, Response};

type Writer = Box<dyn AsyncWrite + Send + Unpin>;

/// Receives every message in (`<-`) and out (`->`): the protocol log.
pub type Trace = Arc<dyn Fn(&str, &Value) + Send + Sync>;

/// Messages the adapter sends on its own.
#[derive(Debug, Clone)]
pub enum Incoming {
    Event(Event),
    /// A reverse request; answer it with [`DapClient::respond`].
    Request(Request),
}

pub struct DapClient {
    writer: tokio::sync::Mutex<Writer>,
    seq: AtomicI64,
    pending: Mutex<HashMap<i64, oneshot::Sender<Response>>>,
    closed: AtomicBool,
    timeout: Duration,
    /// Every message in and out is passed here (protocol log), when set.
    trace: Option<Trace>,
}

/// A request whose response is awaited later (`launch`, which some adapters only answer
/// after `configurationDone`).
pub struct PendingResponse {
    command: String,
    rx: oneshot::Receiver<Response>,
}

impl PendingResponse {
    pub async fn wait(self, timeout: Duration) -> DapResult<Value> {
        match tokio::time::timeout(timeout, self.rx).await {
            Ok(Ok(r)) => into_body(r),
            Ok(Err(_)) => Err(DapError::Closed),
            Err(_) => Err(DapError::Timeout(self.command)),
        }
    }
}

fn into_body(r: Response) -> DapResult<Value> {
    if r.success {
        Ok(r.body)
    } else {
        Err(DapError::Adapter {
            message: r.error_text(),
            command: r.command,
        })
    }
}

impl DapClient {
    /// Start reading `reader` in a task. Events and reverse requests arrive on the returned
    /// channel, which closes when the adapter disconnects.
    pub fn spawn<R>(
        reader: R,
        writer: Writer,
        timeout: Duration,
        trace: Option<Trace>,
    ) -> (Arc<Self>, mpsc::UnboundedReceiver<Incoming>)
    where
        R: AsyncBufRead + Send + Unpin + 'static,
    {
        let client = Arc::new(Self {
            writer: tokio::sync::Mutex::new(writer),
            seq: AtomicI64::new(1),
            pending: Mutex::new(HashMap::new()),
            closed: AtomicBool::new(false),
            timeout,
            trace,
        });
        let (tx, rx) = mpsc::unbounded_channel();
        let reading = Arc::clone(&client);
        tokio::spawn(async move { reading.read_loop(reader, tx).await });
        (client, rx)
    }

    async fn read_loop<R: AsyncBufRead + Unpin>(
        &self,
        mut reader: R,
        tx: mpsc::UnboundedSender<Incoming>,
    ) {
        loop {
            match read_message(&mut reader).await {
                Ok(Some(value)) => {
                    if let Some(trace) = &self.trace {
                        trace("<-", &value);
                    }
                    match serde_json::from_value::<Message>(value) {
                        Ok(Message::Response(r)) => {
                            let waiter =
                                self.pending.lock().expect("pending").remove(&r.request_seq);
                            match waiter {
                                Some(w) => {
                                    let _ = w.send(r);
                                }
                                None => tracing::debug!("unexpected DAP response to {}", r.command),
                            }
                        }
                        Ok(Message::Event(e)) => {
                            let _ = tx.send(Incoming::Event(e));
                        }
                        Ok(Message::Request(r)) => {
                            let _ = tx.send(Incoming::Request(r));
                        }
                        Err(e) => tracing::warn!("dropping malformed DAP message: {e}"),
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    tracing::debug!("debug adapter connection closed: {e}");
                    break;
                }
            }
        }
        self.mark_closed();
    }

    fn mark_closed(&self) {
        self.closed.store(true, Ordering::SeqCst);
        // Dropping the senders wakes every waiter with `Closed`.
        self.pending.lock().expect("pending").clear();
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    async fn write(&self, message: &Message) -> DapResult<()> {
        let value = serde_json::to_value(message)?;
        if let Some(trace) = &self.trace {
            trace("->", &value);
        }
        let bytes = encode(&value);
        let mut w = self.writer.lock().await;
        w.write_all(&bytes).await?;
        w.flush().await?;
        Ok(())
    }

    /// Send a request; its response is awaited with [`PendingResponse::wait`].
    pub async fn send(&self, command: &str, arguments: Value) -> DapResult<PendingResponse> {
        if self.is_closed() {
            return Err(DapError::Closed);
        }
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().expect("pending").insert(seq, tx);
        let message = Message::Request(Request {
            seq,
            command: command.to_string(),
            arguments,
        });
        if let Err(e) = self.write(&message).await {
            self.pending.lock().expect("pending").remove(&seq);
            return Err(e);
        }
        Ok(PendingResponse {
            command: command.to_string(),
            rx,
        })
    }

    /// Send a request and wait for its body (bounded by the client timeout).
    pub async fn request(&self, command: &str, arguments: Value) -> DapResult<Value> {
        self.send(command, arguments)
            .await?
            .wait(self.timeout)
            .await
    }

    /// [`Self::request`] with typed arguments and body.
    pub async fn call<A: Serialize, T: DeserializeOwned>(
        &self,
        command: &str,
        arguments: &A,
    ) -> DapResult<T> {
        let body = self
            .request(command, serde_json::to_value(arguments)?)
            .await?;
        // Adapters may omit an empty body.
        let body = if body.is_null() {
            Value::Object(Default::default())
        } else {
            body
        };
        Ok(serde_json::from_value(body)?)
    }

    /// Answer a reverse request.
    pub async fn respond(
        &self,
        request: &Request,
        success: bool,
        body: Value,
        message: Option<String>,
    ) -> DapResult<()> {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        self.write(&Message::Response(Response {
            seq,
            request_seq: request.seq,
            success,
            command: request.command.clone(),
            message,
            body,
        }))
        .await
    }

    /// Close the write side; adapters end the session on EOF.
    pub async fn shutdown(&self) {
        let mut w = self.writer.lock().await;
        let _ = w.shutdown().await;
        drop(w);
        self.mark_closed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{encode, read_message};
    use serde_json::json;
    use tokio::io::{duplex, BufReader};

    #[tokio::test]
    async fn routes_responses_events_and_reverse_requests() {
        let (ours, theirs) = duplex(64 * 1024);
        let (r, w) = tokio::io::split(ours);
        let (client, mut incoming) =
            DapClient::spawn(BufReader::new(r), Box::new(w), Duration::from_secs(2), None);
        let (tr, mut tw) = tokio::io::split(theirs);
        let adapter = tokio::spawn(async move {
            let mut tr = BufReader::new(tr);
            let req = read_message(&mut tr).await.unwrap().unwrap();
            assert_eq!(req["command"], "threads");
            let seq = req["seq"].as_i64().unwrap();
            for msg in [
                json!({"seq": 1, "type": "event", "event": "output", "body": {"output": "hi"}}),
                json!({"seq": 2, "type": "request", "command": "runInTerminal", "arguments": {"cwd": "/", "args": ["a"]}}),
                json!({"seq": 3, "type": "response", "request_seq": seq, "success": true, "command": "threads",
                       "body": {"threads": [{"id": 1, "name": "main"}]}}),
            ] {
                tw.write_all(&encode(&msg)).await.unwrap();
            }
            let answer = read_message(&mut tr).await.unwrap().unwrap();
            assert_eq!(answer["request_seq"], 2);
            assert_eq!(answer["body"]["processId"], 7);
            let req = read_message(&mut tr).await.unwrap().unwrap();
            let seq = req["seq"].as_i64().unwrap();
            let fail = json!({"seq": 4, "type": "response", "request_seq": seq, "success": false,
                              "command": "evaluate", "message": "no frame"});
            tw.write_all(&encode(&fail)).await.unwrap();
        });
        let body = client.request("threads", Value::Null).await.unwrap();
        assert_eq!(body["threads"][0]["name"], "main");
        let Some(Incoming::Event(e)) = incoming.recv().await else {
            panic!("no event")
        };
        assert_eq!(e.event, "output");
        let Some(Incoming::Request(r)) = incoming.recv().await else {
            panic!("no reverse request")
        };
        client
            .respond(&r, true, json!({"processId": 7}), None)
            .await
            .unwrap();
        let err = client.request("evaluate", json!({})).await.unwrap_err();
        assert_eq!(err.to_string(), "no frame");
        adapter.await.unwrap();
    }

    #[tokio::test]
    async fn closing_wakes_waiters_and_times_out() {
        let (ours, theirs) = duplex(1024);
        let (r, w) = tokio::io::split(ours);
        let (client, _incoming) = DapClient::spawn(
            BufReader::new(r),
            Box::new(w),
            Duration::from_millis(50),
            None,
        );
        assert!(matches!(
            client.request("threads", Value::Null).await,
            Err(DapError::Timeout(_))
        ));
        let pending = client.send("threads", Value::Null).await.unwrap();
        drop(theirs);
        assert!(matches!(
            pending.wait(Duration::from_secs(5)).await,
            Err(DapError::Closed)
        ));
    }
}
