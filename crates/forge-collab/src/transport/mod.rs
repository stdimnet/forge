//! Transports: how a guest reaches the host.
//!
//! A [`Transport`] is one endpoint with a key pair. The host accepts connections, a guest
//! dials the host's key. Both directions authenticate the peer by its key (QUIC/TLS with the
//! endpoint keys for the network transport); the session code never sees addresses.
//!
//! - [`memory`]: an in-process network for tests, with injectable latency and disconnects.
//! - [`net`]: iroh (QUIC), direct connections (feature `net`).

pub mod memory;
#[cfg(feature = "net")]
pub mod net;

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;

use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::protocol;

/// An endpoint's public key (an Ed25519 key for the network transport).
pub type PeerKey = [u8; 32];

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub type BoxRead = Box<dyn AsyncRead + Send + Unpin>;
pub type BoxWrite = Box<dyn AsyncWrite + Send + Unpin>;

/// An authenticated, encrypted, ordered byte stream to one peer.
pub struct Connection {
    /// The peer's key, as authenticated by the transport.
    pub remote: PeerKey,
    pub send: BoxWrite,
    pub recv: BoxRead,
    /// Keeps the underlying connection open while the streams are used.
    pub keepalive: Box<dyn std::any::Any + Send>,
}

pub trait Transport: Send + Sync + 'static {
    /// This endpoint's key.
    fn local_key(&self) -> PeerKey;
    /// Addresses others can dial this endpoint at (for the session link).
    fn addresses(&self) -> Vec<SocketAddr>;
    /// The next incoming connection, `None` once the endpoint is closed.
    fn accept(&self) -> BoxFuture<'_, Option<Connection>>;
    /// Dial `host` at one of `addrs`. Fails unless the peer proves it holds `host`'s key.
    fn connect(
        &self,
        host: PeerKey,
        addrs: Vec<SocketAddr>,
    ) -> BoxFuture<'_, io::Result<Connection>>;
    /// Stop accepting and close every connection.
    fn close(&self) -> BoxFuture<'_, ()>;
}

/// Write one framed message.
pub async fn write_msg<W, T>(w: &mut W, msg: &T, limit: usize) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
    T: Serialize,
{
    let frame = protocol::encode(msg, limit).map_err(io::Error::other)?;
    w.write_all(&frame).await?;
    w.flush().await
}

/// Read one framed message; `Ok(None)` when the peer closed the stream between messages.
pub async fn read_msg<R, T>(r: &mut R, limit: usize) -> io::Result<Option<T>>
where
    R: AsyncRead + Unpin + ?Sized,
    T: DeserializeOwned,
{
    let mut len = [0u8; 4];
    match r.read_exact(&mut len).await {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_be_bytes(len) as usize;
    if len > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message of {len} bytes is too large"),
        ));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).await?;
    protocol::decode(&body)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
