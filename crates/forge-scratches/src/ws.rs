//! `WEBSOCKET` requests (feature `client`): connect, send the request's messages (waiting
//! for the server where the file says `=== wait-for-server`), then keep the connection
//! open and report every message until the server closes it or the run is stopped.
//! Messages typed into the Run console are sent too.

use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

use crate::http::{websocket_messages, HttpRequest};

/// What happens on the connection, for the live message log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsEvent {
    Connected { status: u16 },
    Sent(String),
    Received(String),
    /// A binary message: its size.
    ReceivedBinary(usize),
    Closed { code: Option<u16>, reason: String },
    Error(String),
}

/// Control of a running connection.
#[derive(Debug)]
pub enum WsControl {
    Send(String),
    Close,
}

/// Run a resolved `WEBSOCKET` request. `control` receives console input and the stop
/// request; `events` gets the log. Returns when the connection ends.
pub async fn run(
    req: &HttpRequest,
    connect_timeout: Duration,
    mut control: mpsc::UnboundedReceiver<WsControl>,
    events: &(dyn Fn(WsEvent) + Send + Sync),
) {
    let mut request = match req.url.as_str().into_client_request() {
        Ok(r) => r,
        Err(e) => return events(WsEvent::Error(format!("Invalid URL '{}': {e}", req.url))),
    };
    for h in &req.headers {
        let (Ok(name), Ok(value)) = (
            tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(h.name.as_bytes()),
            HeaderValue::from_str(&h.value),
        ) else {
            continue;
        };
        request.headers_mut().insert(name, value);
    }
    let connected =
        tokio::time::timeout(connect_timeout, tokio_tungstenite::connect_async(request)).await;
    let (stream, response) = match connected {
        Err(_) => return events(WsEvent::Error("Connection timed out".into())),
        Ok(Err(e)) => return events(WsEvent::Error(format!("Connection failed: {e}"))),
        Ok(Ok(s)) => s,
    };
    events(WsEvent::Connected {
        status: response.status().as_u16(),
    });
    let (mut write, mut read) = stream.split();
    let mut pending = websocket_messages(req.body.as_deref()).into_iter().peekable();
    let mut received_since_send = 0usize;
    let started = Instant::now();
    loop {
        // Send queued messages that do not wait for the server.
        while let Some(m) = pending.peek() {
            if m.wait_for_server && received_since_send == 0 {
                break;
            }
            let m = pending.next().expect("peeked");
            if m.wait_for_server {
                received_since_send = 0;
            }
            if let Err(e) = write.send(Message::text(m.text.clone())).await {
                return events(WsEvent::Error(format!("Cannot send: {e}")));
            }
            events(WsEvent::Sent(m.text));
        }
        tokio::select! {
            msg = read.next() => match msg {
                Some(Ok(Message::Text(t))) => {
                    received_since_send += 1;
                    events(WsEvent::Received(t.to_string()));
                }
                Some(Ok(Message::Binary(b))) => {
                    received_since_send += 1;
                    events(WsEvent::ReceivedBinary(b.len()));
                }
                Some(Ok(Message::Close(frame))) => {
                    let (code, reason) = frame
                        .map(|f| (Some(u16::from(f.code)), f.reason.to_string()))
                        .unwrap_or((None, String::new()));
                    return events(WsEvent::Closed { code, reason });
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => return events(WsEvent::Error(format!("Connection error: {e}"))),
                None => return events(WsEvent::Closed { code: None, reason: String::new() }),
            },
            c = control.recv() => match c {
                Some(WsControl::Send(text)) => {
                    let text = text.trim_end_matches(['\n', '\r']).to_string();
                    if let Err(e) = write.send(Message::text(text.clone())).await {
                        return events(WsEvent::Error(format!("Cannot send: {e}")));
                    }
                    events(WsEvent::Sent(text));
                }
                Some(WsControl::Close) | None => {
                    let _ = write.send(Message::Close(None)).await;
                    let _ = tokio::time::timeout(Duration::from_secs(2), async {
                        while let Some(Ok(m)) = read.next().await {
                            if matches!(m, Message::Close(_)) { break; }
                        }
                    })
                    .await;
                    return events(WsEvent::Closed {
                        code: Some(1000),
                        reason: format!("closed by Forge after {} s", started.elapsed().as_secs()),
                    });
                }
            }
        }
    }
}

/// Console text of an event (ANSI styles like the HTTP client's).
pub fn render(e: &WsEvent) -> String {
    match e {
        WsEvent::Connected { status } => format!("\x1b[32mConnected ({status} Switching Protocols)\x1b[0m\n"),
        WsEvent::Sent(t) => format!("\x1b[36m▶ {t}\x1b[0m\n"),
        WsEvent::Received(t) => format!("◀ {t}\n"),
        WsEvent::ReceivedBinary(n) => format!("◀ <binary message, {n} bytes>\n"),
        WsEvent::Closed { code, reason } => match code {
            Some(c) if reason.is_empty() => format!("\x1b[90mConnection closed ({c})\x1b[0m\n"),
            Some(c) => format!("\x1b[90mConnection closed ({c}: {reason})\x1b[0m\n"),
            None => "\x1b[90mConnection closed\x1b[0m\n".to_string(),
        },
        WsEvent::Error(m) => format!("\x1b[31m{m}\x1b[0m\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::parse;
    use std::sync::{Arc, Mutex};

    /// An echo server that greets first and closes after "bye".
    async fn echo_server() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(sock).await.unwrap();
            ws.send(Message::text("welcome")).await.unwrap();
            while let Some(Ok(m)) = ws.next().await {
                match m {
                    Message::Text(t) if t.as_str() == "bye" => {
                        ws.close(None).await.ok();
                        break;
                    }
                    Message::Text(t) => ws.send(Message::text(format!("echo: {t}"))).await.unwrap(),
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        });
        format!("ws://127.0.0.1:{}/socket", addr.port())
    }

    #[tokio::test]
    async fn sends_messages_and_logs_the_conversation() {
        let url = echo_server().await;
        let text = format!("WEBSOCKET {url}\n\nhello\n=== wait-for-server\nsecond\n");
        let req = parse(&text).requests[0].clone();
        let log: Arc<Mutex<Vec<WsEvent>>> = Arc::default();
        let (tx, rx) = mpsc::unbounded_channel();
        let l2 = log.clone();
        let tx2 = tx.clone();
        let sink = move |e: WsEvent| {
            // Type a message once the second one was echoed, then "bye".
            if let WsEvent::Received(t) = &e {
                if t == "echo: second" {
                    tx2.send(WsControl::Send("bye\n".into())).unwrap();
                }
            }
            l2.lock().unwrap().push(e);
        };
        tokio::time::timeout(Duration::from_secs(10), run(&req, Duration::from_secs(5), rx, &sink))
            .await
            .unwrap();
        drop(tx);
        let log = log.lock().unwrap().clone();
        assert_eq!(log[0], WsEvent::Connected { status: 101 });
        let texts: Vec<String> = log[1..]
            .iter()
            .map(|e| render(e).replace("\x1b[36m", "").replace("\x1b[0m", "").replace("\x1b[90m", ""))
            .collect();
        assert_eq!(
            texts,
            vec![
                "▶ hello\n",
                "◀ welcome\n",
                "▶ second\n",
                "◀ echo: hello\n",
                "◀ echo: second\n",
                "▶ bye\n",
                "Connection closed\n",
            ]
        );
    }

    #[tokio::test]
    async fn stops_on_request_and_reports_failures() {
        let url = echo_server().await;
        let req = parse(&format!("WEBSOCKET {url}\n")).requests[0].clone();
        let (tx, rx) = mpsc::unbounded_channel();
        let log: Arc<Mutex<Vec<WsEvent>>> = Arc::default();
        let l2 = log.clone();
        let sink = move |e: WsEvent| {
            if matches!(e, WsEvent::Received(_)) {
                tx.send(WsControl::Close).unwrap();
            }
            l2.lock().unwrap().push(e);
        };
        run(&req, Duration::from_secs(5), rx, &sink).await;
        assert!(matches!(log.lock().unwrap().last(), Some(WsEvent::Closed { code: Some(1000), .. })));

        let port = forge_ports::free_port().unwrap();
        let req = parse(&format!("WEBSOCKET ws://127.0.0.1:{port}/")).requests[0].clone();
        let (_tx, rx) = mpsc::unbounded_channel();
        let log: Arc<Mutex<Vec<WsEvent>>> = Arc::default();
        let l2 = log.clone();
        run(&req, Duration::from_secs(5), rx, &move |e| l2.lock().unwrap().push(e)).await;
        assert!(matches!(&log.lock().unwrap()[0], WsEvent::Error(m) if m.starts_with("Connection failed")));
    }
}
