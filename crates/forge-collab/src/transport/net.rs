//! The network transport: iroh (QUIC with TLS 1.3, peers identified by Ed25519 keys).
//!
//! Configured for direct connections only: no relay servers, no address lookup (DNS or
//! pkarr), no port mapping, no network probes. The guest dials the addresses from the link;
//! iroh authenticates the host by its key. Relays, LAN discovery and NAT traversal are
//! opt-in settings of a later stage.
//!
//! Every session uses a fresh key pair, so links cannot be correlated across sessions, and an
//! endpoint only exists between Start Session (or Join) and the end of the session.

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use iroh::endpoint::presets;
use iroh::{
    Endpoint, EndpointAddr, NetReportConfig, PublicKey, RelayMode, SecretKey, TransportAddr,
    Watcher,
};

use super::{BoxFuture, Connection, PeerKey, Transport};
use crate::protocol::ALPN;

/// Where the host listens (settings `codeWithMe.listenAddress` and `codeWithMe.listenPort`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ListenOptions {
    /// `None`: all interfaces.
    pub address: Option<IpAddr>,
    /// 0: any free port.
    pub port: u16,
}

pub struct NetTransport {
    endpoint: Endpoint,
    /// Addresses for the link, fixed when the endpoint is bound.
    addresses: Vec<SocketAddr>,
}

fn err(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}

fn builder(accept: bool) -> iroh::endpoint::Builder {
    Endpoint::builder(presets::Minimal)
        .secret_key(SecretKey::generate())
        .relay_mode(RelayMode::Disabled)
        .clear_address_lookup()
        .net_report_config(NetReportConfig::minimal())
        .alpns(if accept {
            vec![ALPN.to_vec()]
        } else {
            Vec::new()
        })
}

/// Addresses worth putting into a link: no unspecified, multicast or IPv6 link-local
/// addresses (they need a scope the link cannot carry).
fn dialable(addr: &SocketAddr) -> bool {
    let ip = addr.ip();
    !ip.is_unspecified()
        && !ip.is_multicast()
        && !matches!(ip, IpAddr::V6(v6) if (v6.segments()[0] & 0xffc0) == 0xfe80)
}

impl NetTransport {
    /// A host endpoint that accepts Code With Me connections. This opens the UDP socket(s);
    /// call it only when the user starts a session.
    pub async fn listen(opts: ListenOptions) -> io::Result<Self> {
        let mut b = builder(true);
        if opts.address.is_some() || opts.port != 0 {
            b = b.clear_ip_transports();
            let addrs: Vec<SocketAddr> = match opts.address {
                Some(ip) => vec![SocketAddr::new(ip, opts.port)],
                None => vec![
                    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), opts.port),
                    SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), opts.port),
                ],
            };
            for a in addrs {
                b = b.bind_addr(a).map_err(err)?;
            }
        }
        let endpoint = b.bind().await.map_err(err)?;
        let addresses = Self::local_addresses(&endpoint, opts.address).await;
        Ok(Self {
            endpoint,
            addresses,
        })
    }

    /// A guest endpoint: it dials out and accepts nothing.
    pub async fn dialer() -> io::Result<Self> {
        let endpoint = builder(false).bind().await.map_err(err)?;
        Ok(Self {
            endpoint,
            addresses: Vec::new(),
        })
    }

    async fn local_addresses(endpoint: &Endpoint, only: Option<IpAddr>) -> Vec<SocketAddr> {
        // Interface addresses are discovered right after binding.
        let mut watch = endpoint.watch_addr();
        let mut found: Vec<SocketAddr> = endpoint.addr().ip_addrs().copied().collect();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while found.is_empty() && tokio::time::Instant::now() < deadline {
            if tokio::time::timeout_at(deadline, watch.updated())
                .await
                .is_err()
            {
                break;
            }
            found = endpoint.addr().ip_addrs().copied().collect();
        }
        // Loopback, so a second Forge on the same machine can join.
        for bound in endpoint.bound_sockets() {
            if bound.ip().is_unspecified() && bound.is_ipv4() {
                found.push(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                    bound.port(),
                ));
            } else if !bound.ip().is_unspecified() {
                found.push(bound);
            }
        }
        if let Some(ip) = only {
            found.retain(|a| a.ip() == ip);
        }
        found.retain(dialable);
        found.sort();
        found.dedup();
        found.truncate(16);
        found
    }
}

impl Transport for NetTransport {
    fn local_key(&self) -> PeerKey {
        *self.endpoint.id().as_bytes()
    }

    fn addresses(&self) -> Vec<SocketAddr> {
        self.addresses.clone()
    }

    fn accept(&self) -> BoxFuture<'_, Option<Connection>> {
        Box::pin(async move {
            loop {
                let incoming = self.endpoint.accept().await?;
                let Ok(conn) = incoming.await else { continue };
                let remote = *conn.remote_id().as_bytes();
                // The guest opens the stream with its knock.
                let accepted =
                    tokio::time::timeout(Duration::from_secs(10), conn.accept_bi()).await;
                let Ok(Ok((send, recv))) = accepted else {
                    continue;
                };
                return Some(Connection {
                    remote,
                    send: Box::new(send),
                    recv: Box::new(recv),
                    keepalive: Box::new(conn),
                });
            }
        })
    }

    fn connect(
        &self,
        host: PeerKey,
        addrs: Vec<SocketAddr>,
    ) -> BoxFuture<'_, io::Result<Connection>> {
        Box::pin(async move {
            let key = PublicKey::from_bytes(&host).map_err(err)?;
            let addr = EndpointAddr::from_parts(key, addrs.into_iter().map(TransportAddr::Ip));
            let conn = self.endpoint.connect(addr, ALPN).await.map_err(err)?;
            let (send, recv) = conn.open_bi().await.map_err(err)?;
            Ok(Connection {
                remote: host,
                send: Box::new(send),
                recv: Box::new(recv),
                keepalive: Box::new(conn),
            })
        })
    }

    fn close(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move { self.endpoint.close().await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{GuestMessage, MAX_GUEST_FRAME};
    use crate::transport::{read_msg, write_msg};

    #[test]
    fn link_addresses_are_dialable() {
        assert!(dialable(&"192.168.1.5:4000".parse().unwrap()));
        assert!(dialable(&"127.0.0.1:4000".parse().unwrap()));
        assert!(!dialable(&"0.0.0.0:4000".parse().unwrap()));
        assert!(!dialable(&"[fe80::1]:4000".parse().unwrap()));
        assert!(dialable(&"[2001:db8::1]:4000".parse().unwrap()));
    }

    /// Two endpoints on the loopback interface; nothing leaves the machine.
    #[tokio::test]
    async fn host_and_guest_connect_over_loopback() {
        let host = NetTransport::listen(ListenOptions {
            address: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            port: 0,
        })
        .await
        .unwrap();
        let addrs = host.addresses();
        assert!(
            !addrs.is_empty() && addrs.iter().all(|a| a.ip().is_loopback()),
            "{addrs:?}"
        );
        let guest = NetTransport::dialer().await.unwrap();
        let host_key = host.local_key();
        let accept = tokio::spawn(async move {
            let mut c = host.accept().await.unwrap();
            let msg: Option<GuestMessage> = read_msg(&mut c.recv, MAX_GUEST_FRAME).await.unwrap();
            (c.remote, msg, host)
        });
        let mut out = guest.connect(host_key, addrs).await.unwrap();
        write_msg(&mut out.send, &GuestMessage::Leave, MAX_GUEST_FRAME)
            .await
            .unwrap();
        let (remote, msg, host) = accept.await.unwrap();
        assert_eq!(remote, guest.local_key());
        assert_eq!(msg, Some(GuestMessage::Leave));
        // A wrong key is refused by the TLS handshake.
        let wrong = NetTransport::dialer().await.unwrap();
        let fake = NetTransport::dialer().await.unwrap().local_key();
        let res = tokio::time::timeout(
            Duration::from_secs(2),
            wrong.connect(fake, host.addresses()),
        )
        .await;
        assert!(!matches!(res, Ok(Ok(_))));
        host.close().await;
        guest.close().await;
    }
}
