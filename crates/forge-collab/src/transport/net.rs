//! The network transport: iroh (QUIC with TLS 1.3, peers identified by Ed25519 keys).
//!
//! Direct connections are the default: no relay servers, no address lookup (DNS or pkarr), no
//! port mapping, no network probes. The guest dials the addresses from the link; iroh
//! authenticates the host by its key. A relay (self-hosted, or n0's public ones as an explicit
//! opt-in) and LAN discovery are added by [`NetConfig`]; see [`super::relay`]. With a relay,
//! both ends try to punch a hole through their NATs and move the connection to the direct
//! path when that works.
//!
//! Every session uses a fresh key pair, so links cannot be correlated across sessions, and an
//! endpoint only exists between Start Session (or Join) and the end of the session.

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Mutex;
use std::time::Duration;

use iroh::endpoint::{presets, QuicTransportConfig};
use iroh::{
    Endpoint, EndpointAddr, NetReportConfig, PublicKey, RelayMode, SecretKey, TransportAddr,
    Watcher,
};

use super::relay::{self as relay_cfg, NetConfig};
use super::{BoxFuture, Connection, PeerKey, Transport};
use crate::protocol::ALPN;
use crate::relay::{ConnectionPath, RelayPlan, RelayStatus};

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
    plan: RelayPlan,
    /// The relay of the session link: where the host is reachable (host: its home relay;
    /// guest: the relay named in the link).
    relay_url: Option<String>,
    /// Open connections, to tell how each one travels.
    conns: Mutex<HashMap<PeerKey, iroh::endpoint::Connection>>,
}

/// How long a connection may go without hearing from the peer before it counts as dropped
/// (both ends send a keep-alive every 5 seconds, iroh's heartbeat). iroh's default is 30
/// seconds: a guest whose network went away kept showing "Connected" for half a minute
/// before reconnecting. 15 seconds is three missed keep-alives; outages shorter than that
/// (a Wi-Fi reconnect, a cellular handoff) pass without a reconnect. The shorter of the two
/// ends' values applies. Measured in `tests/quic_outage.rs`.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(15);

/// A change to the iroh endpoint before it is bound. Tests use it to put both ends on a
/// simulated network (an iroh custom transport) whose link they can cut, see
/// `tests/quic_outage.rs`; on one machine iroh's hole punching finds a way around any cut
/// between two UDP sockets.
pub type EndpointHook = Box<dyn FnOnce(iroh::endpoint::Builder) -> iroh::endpoint::Builder + Send>;

fn err(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}

fn builder(accept: bool, cfg: &NetConfig) -> io::Result<iroh::endpoint::Builder> {
    let b = Endpoint::builder(presets::Minimal)
        .secret_key(SecretKey::generate())
        .relay_mode(RelayMode::Disabled)
        .clear_address_lookup()
        .net_report_config(NetReportConfig::minimal())
        .transport_config(
            QuicTransportConfig::builder()
                .max_idle_timeout(Some(IDLE_TIMEOUT.try_into().map_err(err)?))
                .build(),
        )
        .alpns(if accept {
            vec![ALPN.to_vec()]
        } else {
            Vec::new()
        });
    relay_cfg::configure(b, cfg, accept)
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
        Self::listen_with(opts, NetConfig::default()).await
    }

    /// Like [`Self::listen`], with a relay and LAN discovery as configured. With a relay the
    /// call returns once the relay is connected (or says why it is not), so the session link
    /// can name it.
    pub async fn listen_with(opts: ListenOptions, cfg: NetConfig) -> io::Result<Self> {
        Self::listen_hooked(opts, cfg, None).await
    }

    /// [`Self::listen_with`] with a change to the endpoint (tests).
    pub async fn listen_with_hook(
        opts: ListenOptions,
        cfg: NetConfig,
        hook: EndpointHook,
    ) -> io::Result<Self> {
        Self::listen_hooked(opts, cfg, Some(hook)).await
    }

    async fn listen_hooked(
        opts: ListenOptions,
        cfg: NetConfig,
        hook: Option<EndpointHook>,
    ) -> io::Result<Self> {
        let mut b = builder(true, &cfg)?;
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
        if let Some(hook) = hook {
            b = hook(b);
        }
        let endpoint = b.bind().await.map_err(err)?;
        let plan = cfg.relay.plan();
        if let Err(e) = relay_cfg::wait_online(&endpoint, &plan).await {
            endpoint.close().await;
            return Err(e);
        }
        let addresses = Self::local_addresses(&endpoint, opts.address).await;
        let relay_url = match &plan {
            RelayPlan::None => None,
            RelayPlan::Custom { url, .. } => Some(url.clone()),
            RelayPlan::Public => relay_cfg::home_relay(&endpoint),
        };
        Ok(Self {
            endpoint,
            addresses,
            plan,
            relay_url,
            conns: Mutex::default(),
        })
    }

    /// A guest endpoint: it dials out and accepts nothing.
    pub async fn dialer() -> io::Result<Self> {
        Self::dialer_with(NetConfig::default()).await
    }

    /// A guest endpoint that also uses the relay of the session link (see
    /// [`crate::relay::RelayOptions::guest`]).
    pub async fn dialer_with(cfg: NetConfig) -> io::Result<Self> {
        Self::dialer_hooked(cfg, None).await
    }

    /// [`Self::dialer_with`] with a change to the endpoint (tests).
    pub async fn dialer_with_hook(cfg: NetConfig, hook: EndpointHook) -> io::Result<Self> {
        Self::dialer_hooked(cfg, Some(hook)).await
    }

    async fn dialer_hooked(cfg: NetConfig, hook: Option<EndpointHook>) -> io::Result<Self> {
        let mut b = builder(false, &cfg)?;
        if let Some(hook) = hook {
            b = hook(b);
        }
        let endpoint = b.bind().await.map_err(err)?;
        let plan = cfg.relay.plan();
        let relay_url = match &plan {
            RelayPlan::Custom { url, .. } => Some(url.clone()),
            _ => None,
        };
        Ok(Self {
            endpoint,
            addresses: Vec::new(),
            plan,
            relay_url,
            conns: Mutex::default(),
        })
    }

    /// Connect to the configured relay once and report how that went (the settings page's
    /// Test Connection). Binds a short-lived endpoint that dials nobody else.
    pub async fn check_relay(cfg: NetConfig) -> io::Result<RelayStatus> {
        let plan = cfg.relay.plan();
        if plan == RelayPlan::None {
            return Err(err("No relay server is set"));
        }
        let endpoint = builder(false, &cfg)?.bind().await.map_err(err)?;
        let online = relay_cfg::wait_online(&endpoint, &plan).await;
        let status = relay_cfg::status(&endpoint, &plan);
        endpoint.close().await;
        online?;
        status.ok_or_else(|| err("The relay did not answer"))
    }

    fn remember(&self, peer: PeerKey, conn: &iroh::endpoint::Connection) {
        let mut conns = self.conns.lock().expect("connection map lock poisoned");
        conns.retain(|_, c| c.close_reason().is_none());
        conns.insert(peer, conn.clone());
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
                self.remember(remote, &conn);
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
            let mut transport_addrs: Vec<TransportAddr> =
                addrs.into_iter().map(TransportAddr::Ip).collect();
            if let Some(url) = self.relay_url.as_deref().and_then(|u| u.parse().ok()) {
                transport_addrs.push(TransportAddr::Relay(url));
            }
            let addr = EndpointAddr::from_parts(key, transport_addrs);
            let conn = self.endpoint.connect(addr, ALPN).await.map_err(err)?;
            self.remember(host, &conn);
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

    fn relay_url(&self) -> Option<String> {
        self.relay_url.clone()
    }

    fn path(&self, peer: &PeerKey) -> Option<ConnectionPath> {
        let conns = self.conns.lock().expect("connection map lock poisoned");
        relay_cfg::path_of(conns.get(peer)?)
    }

    fn relay_status(&self) -> Option<RelayStatus> {
        relay_cfg::status(&self.endpoint, &self.plan)
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

    /// With an empty relay URL (the default) Forge contacts no server: the endpoint has no
    /// relay, no address lookup and nothing to report.
    #[tokio::test]
    async fn without_a_relay_url_no_server_is_configured() {
        let cfg = NetConfig::default();
        assert!(!cfg.relay.contacts_servers());
        assert!(!cfg.relay.lan_discovery);
        let mode = relay_cfg::relay_mode(&cfg.relay.plan()).unwrap();
        assert_eq!(mode, RelayMode::Disabled);
        assert!(mode.relay_map().urls::<Vec<_>>().is_empty());

        let host = NetTransport::listen(ListenOptions {
            address: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            port: 0,
        })
        .await
        .unwrap();
        assert_eq!(host.relay_url(), None);
        assert_eq!(host.relay_status(), None);
        assert!(host.endpoint.addr().relay_urls().next().is_none());
        assert!(host.endpoint.home_relay_status().get().is_empty());
        host.close().await;

        // A token is only used together with a relay.
        let host_opts = crate::relay::RelayOptions::host(None, Some("tok".into()), false, false);
        assert_eq!(host_opts.plan(), RelayPlan::None);
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
