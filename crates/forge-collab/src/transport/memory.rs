//! An in-process network for tests: endpoints find each other by key, connections are
//! `tokio::io::duplex` pipes. Every connection runs through a relay task per direction, so
//! tests can add latency and cut connections ([`MemoryNetwork::disconnect`]) the way a
//! network would. Keys are trusted as given, which stands in for the transport's
//! authentication.

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::sync::{mpsc, Mutex as AsyncMutex};
use tokio::task::JoinHandle;

use super::{BoxFuture, Connection, PeerKey, Transport};

const PIPE: usize = 64 * 1024;

#[derive(Default)]
struct Net {
    listeners: HashMap<PeerKey, mpsc::UnboundedSender<Connection>>,
    latency: Duration,
    /// Relay tasks of each connection with the two peers' keys.
    links: Vec<(PeerKey, PeerKey, Vec<JoinHandle<()>>)>,
}

/// The shared network. Clone it to hand it to several endpoints.
#[derive(Clone, Default)]
pub struct MemoryNetwork {
    net: Arc<Mutex<Net>>,
}

impl MemoryNetwork {
    pub fn new() -> Self {
        Self::default()
    }

    /// An endpoint with the given key. It accepts connections right away.
    pub fn endpoint(&self, key: PeerKey) -> MemoryTransport {
        let (tx, rx) = mpsc::unbounded_channel();
        self.net.lock().unwrap().listeners.insert(key, tx);
        MemoryTransport {
            key,
            net: self.clone(),
            incoming: AsyncMutex::new(rx),
        }
    }

    /// Delay applied to every chunk on new connections.
    pub fn set_latency(&self, latency: Duration) {
        self.net.lock().unwrap().latency = latency;
    }

    /// Cut every connection of `key`, like a dropped network link.
    pub fn disconnect(&self, key: PeerKey) {
        let mut net = self.net.lock().unwrap();
        net.links.retain(|(a, b, tasks)| {
            let hit = *a == key || *b == key;
            if hit {
                tasks.iter().for_each(JoinHandle::abort);
            }
            !hit
        });
    }

    fn connect(&self, from: PeerKey, to: PeerKey) -> io::Result<Connection> {
        let mut net = self.net.lock().unwrap();
        let Some(listener) = net.listeners.get(&to).cloned() else {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                "no such endpoint",
            ));
        };
        let latency = net.latency;
        // dialer <-> relay <-> acceptor
        let (dialer, dialer_side) = tokio::io::duplex(PIPE);
        let (acceptor, acceptor_side) = tokio::io::duplex(PIPE);
        let (dr, dw) = tokio::io::split(dialer_side);
        let (ar, aw) = tokio::io::split(acceptor_side);
        let tasks = vec![
            tokio::spawn(relay(dr, aw, latency)),
            tokio::spawn(relay(ar, dw, latency)),
        ];
        net.links.push((from, to, tasks));
        let (recv, send) = tokio::io::split(acceptor);
        listener
            .send(Connection {
                remote: from,
                send: Box::new(send),
                recv: Box::new(recv),
                keepalive: Box::new(()),
            })
            .map_err(|_| io::Error::new(io::ErrorKind::ConnectionRefused, "endpoint closed"))?;
        let (recv, send) = tokio::io::split(dialer);
        Ok(Connection {
            remote: to,
            send: Box::new(send),
            recv: Box::new(recv),
            keepalive: Box::new(()),
        })
    }

    fn close(&self, key: PeerKey) {
        self.net.lock().unwrap().listeners.remove(&key);
        self.disconnect(key);
    }
}

async fn relay(
    mut from: tokio::io::ReadHalf<DuplexStream>,
    mut to: tokio::io::WriteHalf<DuplexStream>,
    latency: Duration,
) {
    let mut buf = vec![0u8; PIPE];
    loop {
        let n = match from.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        if !latency.is_zero() {
            tokio::time::sleep(latency).await;
        }
        if to.write_all(&buf[..n]).await.is_err() {
            break;
        }
    }
    let _ = to.shutdown().await;
}

/// One endpoint on a [`MemoryNetwork`].
pub struct MemoryTransport {
    key: PeerKey,
    net: MemoryNetwork,
    incoming: AsyncMutex<mpsc::UnboundedReceiver<Connection>>,
}

impl Transport for MemoryTransport {
    fn local_key(&self) -> PeerKey {
        self.key
    }

    fn addresses(&self) -> Vec<SocketAddr> {
        Vec::new()
    }

    fn accept(&self) -> BoxFuture<'_, Option<Connection>> {
        Box::pin(async move { self.incoming.lock().await.recv().await })
    }

    fn connect(
        &self,
        host: PeerKey,
        _addrs: Vec<SocketAddr>,
    ) -> BoxFuture<'_, io::Result<Connection>> {
        Box::pin(async move { self.net.connect(self.key, host) })
    }

    fn close(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            self.net.close(self.key);
            self.incoming.lock().await.close();
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{GuestMessage, MAX_GUEST_FRAME};
    use crate::transport::{read_msg, write_msg};

    #[tokio::test]
    async fn peers_exchange_messages_and_see_each_others_keys() {
        let net = MemoryNetwork::new();
        let host = net.endpoint([1; 32]);
        let guest = net.endpoint([2; 32]);
        let mut out = guest.connect([1; 32], vec![]).await.unwrap();
        assert_eq!(out.remote, [1; 32]);
        let mut inc = host.accept().await.unwrap();
        assert_eq!(inc.remote, [2; 32]);
        write_msg(&mut out.send, &GuestMessage::Leave, MAX_GUEST_FRAME)
            .await
            .unwrap();
        let got: Option<GuestMessage> = read_msg(&mut inc.recv, MAX_GUEST_FRAME).await.unwrap();
        assert_eq!(got, Some(GuestMessage::Leave));
    }

    #[tokio::test]
    async fn unknown_endpoints_refuse() {
        let net = MemoryNetwork::new();
        let guest = net.endpoint([2; 32]);
        assert!(guest.connect([9; 32], vec![]).await.is_err());
    }

    #[tokio::test]
    async fn disconnect_ends_the_streams() {
        let net = MemoryNetwork::new();
        let host = net.endpoint([1; 32]);
        let guest = net.endpoint([2; 32]);
        let _out = guest.connect([1; 32], vec![]).await.unwrap();
        let mut inc = host.accept().await.unwrap();
        net.disconnect([2; 32]);
        let got: Option<GuestMessage> = read_msg(&mut inc.recv, MAX_GUEST_FRAME).await.unwrap();
        assert_eq!(got, None);
    }

    #[tokio::test]
    async fn latency_delays_delivery() {
        let net = MemoryNetwork::new();
        net.set_latency(Duration::from_millis(50));
        let host = net.endpoint([1; 32]);
        let guest = net.endpoint([2; 32]);
        let mut out = guest.connect([1; 32], vec![]).await.unwrap();
        let mut inc = host.accept().await.unwrap();
        let start = std::time::Instant::now();
        write_msg(&mut out.send, &GuestMessage::Leave, MAX_GUEST_FRAME)
            .await
            .unwrap();
        let _: Option<GuestMessage> = read_msg(&mut inc.recv, MAX_GUEST_FRAME).await.unwrap();
        assert!(start.elapsed() >= Duration::from_millis(50));
    }

    #[tokio::test]
    async fn closed_endpoints_stop_accepting() {
        let net = MemoryNetwork::new();
        let host = net.endpoint([1; 32]);
        host.close().await;
        assert!(host.accept().await.is_none());
        assert!(net
            .endpoint([2; 32])
            .connect([1; 32], vec![])
            .await
            .is_err());
    }
}
