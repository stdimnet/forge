//! Port-forward connections: the WebSocket upgrade to a pod's `portforward` subresource and
//! the pump between a local TCP connection and the pod port ([`crate::portforward`] has the
//! channel protocol, [`crate::ws`] the framing).

use std::sync::atomic::{AtomicU64, Ordering};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::http::{send, BoxIo, Request, Transport};
use crate::portforward::{data_message, ChannelDecoder, Event, PROTOCOLS};
use crate::ws::{self, Decoder, Message, Rng};
use crate::{Error, Result};

/// How long a forward waits for more data from the pod after the local side closed.
const DRAIN_IDLE: std::time::Duration = std::time::Duration::from_millis(1500);

/// An upgraded WebSocket connection (client side).
pub struct WsConnection {
    io: BoxIo,
    decoder: Decoder,
    rng: Rng,
}

fn seed() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    t ^ COUNTER
        .fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed)
        .rotate_left(17)
}

/// Send `req` as a WebSocket upgrade offering `protocols`.
pub async fn connect(
    transport: &Transport,
    mut req: Request,
    protocols: &[&str],
) -> Result<WsConnection> {
    let mut rng = Rng::new(seed());
    let key = ws::client_key(rng.nonce());
    req.headers.extend(ws::upgrade_headers(&key, protocols));
    let resp = send(transport, &req).await?;
    if resp.status != 101 {
        // The API server explains (pod not found, forbidden) in a JSON status.
        resp.error_for_status().await?;
        return Err(Error(
            "The server did not upgrade the connection to a WebSocket".into(),
        ));
    }
    ws::check_upgrade(
        resp.status,
        &|h| resp.header(h).map(str::to_string),
        &key,
        protocols,
    )?;
    let (io, rest) = resp.upgrade();
    let mut decoder = Decoder::new();
    decoder.push(&rest);
    Ok(WsConnection { io, decoder, rng })
}

impl WsConnection {
    /// Pump bytes between `local` and the pod port until either side closes.
    pub async fn port_forward<L>(self, port: u16, local: L) -> Result<()>
    where
        L: AsyncRead + AsyncWrite + Unpin,
    {
        let WsConnection {
            io,
            mut decoder,
            mut rng,
        } = self;
        let (mut ws_r, mut ws_w) = tokio::io::split(io);
        let (mut l_r, mut l_w) = tokio::io::split(local);
        let mut channels = ChannelDecoder::new(port);
        let mut ws_buf = vec![0u8; 32 * 1024];
        let mut l_buf = vec![0u8; 32 * 1024];
        let mut local_open = true;
        // After the local side finished sending, the pod's answer is still delivered until
        // the stream is idle for a moment (the v4 protocol cannot half-close).
        let mut deadline = tokio::time::Instant::now();
        let result: Result<()> = 'pump: loop {
            // Messages that arrived with the handshake or the last read.
            loop {
                match decoder.next() {
                    Ok(Some(Message::Binary(m))) => match channels.push(&m) {
                        Ok(Some(Event::Data(d))) => {
                            if let Err(e) = l_w.write_all(&d).await {
                                break 'pump Err(e.into());
                            }
                        }
                        Ok(Some(Event::Error(e))) if !e.is_empty() => break 'pump Err(Error(e)),
                        Ok(_) => {}
                        Err(e) => break 'pump Err(e),
                    },
                    Ok(Some(Message::Ping(p))) => {
                        let pong = ws::encode_frame(ws::OP_PONG, &p, Some(rng.mask()));
                        if let Err(e) = ws_w.write_all(&pong).await {
                            break 'pump Err(e.into());
                        }
                    }
                    Ok(Some(Message::Close(..))) => break 'pump Ok(()),
                    Ok(Some(_)) => {}
                    Ok(None) => break,
                    Err(e) => break 'pump Err(e),
                }
            }
            tokio::select! {
                n = ws_r.read(&mut ws_buf) => match n {
                    Ok(0) => break Ok(()),
                    Ok(n) => {
                        decoder.push(&ws_buf[..n]);
                        deadline = tokio::time::Instant::now() + DRAIN_IDLE;
                    }
                    Err(e) => break Err(e.into()),
                },
                _ = tokio::time::sleep_until(deadline), if !local_open => break Ok(()),
                n = l_r.read(&mut l_buf), if local_open => match n {
                    // The local side finished sending; keep delivering the pod's answer.
                    Ok(0) => {
                        local_open = false;
                        deadline = tokio::time::Instant::now() + DRAIN_IDLE;
                    }
                    Ok(n) => {
                        let frame = ws::encode_frame(ws::OP_BINARY, &data_message(&l_buf[..n]), Some(rng.mask()));
                        if let Err(e) = ws_w.write_all(&frame).await {
                            break Err(e.into());
                        }
                    }
                    Err(e) => break Err(e.into()),
                },
            }
        };
        let close = ws::encode_frame(ws::OP_CLOSE, &1000u16.to_be_bytes(), Some(rng.mask()));
        let _ = ws_w.write_all(&close).await;
        let _ = ws_w.shutdown().await;
        let _ = l_w.shutdown().await;
        result
    }
}

/// Open the `portforward` WebSocket of a pod port.
pub async fn open_port_forward(transport: &Transport, req: Request) -> Result<WsConnection> {
    connect(transport, req, PROTOCOLS).await
}

#[cfg(all(test, unix))]
pub(crate) mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::net::UnixListener;

    /// A fake API server: answers the port-forward upgrade for `port` and echoes data in
    /// upper case; `fail` makes it report an error on the error channel instead.
    pub(crate) async fn fake_portforward_server(port: u16) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("api.sock").to_string_lossy().into_owned();
        let listener = UnixListener::bind(&path).unwrap();
        let heads = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let heads = heads.clone();
                tokio::spawn(async move {
                    let mut reader = BufReader::new(stream);
                    let mut head = String::new();
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                            return;
                        }
                        let end = line == "\r\n";
                        head.push_str(&line);
                        if end {
                            break;
                        }
                    }
                    heads.lock().unwrap().push(head.clone());
                    let target = head.split_whitespace().nth(1).unwrap_or("").to_string();
                    let mut stream = reader.into_inner();
                    if !target.contains("/portforward?ports=") {
                        let _ = stream
                            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                            .await;
                        return;
                    }
                    if target.contains("/pods/gone/") {
                        let body = r#"{"kind":"Status","message":"pods \"gone\" not found"}"#;
                        let _ = stream
                            .write_all(
                                format!(
                                    "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\n\r\n{body}",
                                    body.len()
                                )
                                .as_bytes(),
                            )
                            .await;
                        return;
                    }
                    let key = head
                        .lines()
                        .find_map(|l| l.strip_prefix("Sec-WebSocket-Key: "))
                        .unwrap()
                        .trim()
                        .to_string();
                    assert!(head.contains("Upgrade: websocket"), "{head}");
                    assert!(head.contains("Sec-WebSocket-Protocol: v4.channel.k8s.io"));
                    let prefix = port.to_le_bytes();
                    let mut hello = format!(
                        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\nSec-WebSocket-Protocol: v4.channel.k8s.io\r\n\r\n",
                        ws::accept_key(&key)
                    )
                    .into_bytes();
                    // Port prefixes right behind the head (read together with it).
                    hello.extend(ws::encode_frame(
                        ws::OP_BINARY,
                        &[0, prefix[0], prefix[1]],
                        None,
                    ));
                    hello.extend(ws::encode_frame(
                        ws::OP_BINARY,
                        &[1, prefix[0], prefix[1]],
                        None,
                    ));
                    stream.write_all(&hello).await.unwrap();
                    let mut decoder = Decoder::new();
                    let mut buf = [0u8; 4096];
                    loop {
                        let n = match stream.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => n,
                        };
                        decoder.push(&buf[..n]);
                        while let Ok(Some(m)) = decoder.next() {
                            match m {
                                Message::Binary(b) => {
                                    assert_eq!(b[0], 0, "client data goes to channel 0");
                                    let text = String::from_utf8_lossy(&b[1..]).to_string();
                                    let reply = if text.contains("fail") {
                                        let mut e = vec![1u8];
                                        e.extend_from_slice(
                                            b"error forwarding port 80: connection refused",
                                        );
                                        e
                                    } else {
                                        let mut d = vec![0u8];
                                        d.extend_from_slice(text.to_uppercase().as_bytes());
                                        d
                                    };
                                    // Split frames to exercise the decoder.
                                    let frame = ws::encode_frame(ws::OP_BINARY, &reply, None);
                                    for piece in frame.chunks(3) {
                                        stream.write_all(piece).await.unwrap();
                                    }
                                }
                                Message::Close(..) => {
                                    let _ = stream
                                        .write_all(&ws::encode_frame(ws::OP_CLOSE, &[3, 232], None))
                                        .await;
                                    return;
                                }
                                _ => {}
                            }
                        }
                    }
                });
            }
        });
        (dir, path)
    }

    #[tokio::test]
    async fn forwards_through_the_websocket() {
        let (_d, path) = fake_portforward_server(80).await;
        let t = Transport::Unix(path);
        let req = Request::new(
            "GET",
            crate::portforward::portforward_path("shop", "web-1", 80),
        );
        let ws = open_port_forward(&t, req).await.unwrap();
        let (mut client, server_side) = tokio::io::duplex(1024);
        let pump = tokio::spawn(ws.port_forward(80, server_side));
        client.write_all(b"hello pod").await.unwrap();
        let mut buf = [0u8; 9];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"HELLO POD");
        client.shutdown().await.unwrap();
        drop(client);
        // After the local EOF the stream drains for a moment, then closes.
        let r = tokio::time::timeout(std::time::Duration::from_secs(5), pump).await;
        assert!(r.is_ok(), "pump ends");

        let req = Request::new(
            "GET",
            crate::portforward::portforward_path("shop", "web-1", 80),
        );
        let ws = open_port_forward(&t, req).await.unwrap();
        let (mut client, server_side) = tokio::io::duplex(1024);
        let pump = tokio::spawn(ws.port_forward(80, server_side));
        client.write_all(b"fail").await.unwrap();
        let e = pump.await.unwrap().unwrap_err();
        assert_eq!(e.0, "error forwarding port 80: connection refused");

        let req = Request::new(
            "GET",
            crate::portforward::portforward_path("shop", "gone", 80),
        );
        let e = open_port_forward(&t, req).await.err().unwrap();
        assert_eq!(e.0, "Not found: pods \"gone\" not found");
    }
}
