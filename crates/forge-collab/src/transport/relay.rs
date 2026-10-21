//! iroh endpoint configuration for relays, hole punching and LAN discovery, and the path a
//! connection uses.
//!
//! [`crate::relay::RelayOptions`] says what the user allowed; this module turns it into an
//! endpoint:
//!
//! - no relay (the default): `RelayMode::Disabled`. The endpoint only talks to the addresses
//!   it is given, and contacts no server at all;
//! - a relay server: `RelayMode::Custom` with that one relay (and the token, if any), so the
//!   endpoint keeps a connection to it, can be reached through it and punches holes over it;
//! - public relays (explicit opt-in): n0's default relay map.
//!
//! There is never DNS or pkarr address lookup, with or without relays: addresses come from the
//! session link. LAN discovery adds iroh's mDNS lookup, which stays on the local network.

use std::io;
use std::time::Duration;

use iroh::endpoint::{Builder, Connection as IrohConnection};
use iroh::tls::CaTlsConfig;
use iroh::{Endpoint, NetReportConfig, RelayConfig, RelayMap, RelayMode, TransportAddr, Watcher};
use iroh_mdns_address_lookup::MdnsAddressLookup;

use crate::relay::{ConnectionPath, PathKind, RelayOptions, RelayPlan, RelayStatus};

/// How long Start Session / Join wait for the relay connection before they report why it
/// does not work.
pub const RELAY_TIMEOUT: Duration = Duration::from_secs(8);

/// Everything the transport needs beyond where to listen.
#[derive(Debug, Clone, Default)]
pub struct NetConfig {
    pub relay: RelayOptions,
    /// Trust other roots than the system's when connecting to the relay (a relay with a
    /// certificate from a private CA; the tests use a self-signed one). `None`: the web PKI
    /// roots.
    pub ca_tls: Option<CaTlsConfig>,
}

impl NetConfig {
    pub fn with_relay(relay: RelayOptions) -> Self {
        Self {
            relay,
            ca_tls: None,
        }
    }
}

fn io_err(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}

/// The relay mode of a plan.
pub fn relay_mode(plan: &RelayPlan) -> io::Result<RelayMode> {
    Ok(match plan {
        RelayPlan::None => RelayMode::Disabled,
        RelayPlan::Public => RelayMode::Default,
        RelayPlan::Custom { url, token } => {
            let url: iroh::RelayUrl = url
                .parse()
                .map_err(|e| io_err(format!("invalid relay server {url}: {e}")))?;
            let mut config = RelayConfig::from(url);
            if let Some(token) = token {
                config = config.with_auth_token(token.clone());
            }
            RelayMode::Custom(RelayMap::from(config))
        }
    })
}

/// Apply the relay settings to an endpoint builder. `accept`: the endpoint is a host.
pub(crate) fn configure(b: Builder, cfg: &NetConfig, accept: bool) -> io::Result<Builder> {
    let plan = cfg.relay.plan();
    let mut b = b.relay_mode(relay_mode(&plan)?);
    if plan != RelayPlan::None {
        // The relay is picked by measuring its latency over HTTPS (the QUIC probe is optional
        // on self-hosted relays). The probes only go to the relay in use.
        let mut report = NetReportConfig::default();
        report.captive_portal_check = false;
        b = b.net_report_config(report);
    }
    if let Some(ca) = &cfg.ca_tls {
        b = b.ca_tls_config(ca.clone());
    }
    if cfg.relay.lan_discovery {
        // Hosts announce themselves; guests only listen.
        b = b.address_lookup(MdnsAddressLookup::builder().advertise(accept));
    }
    Ok(b)
}

/// Wait until the endpoint is connected to its relay, or say why it is not.
pub(crate) async fn wait_online(endpoint: &Endpoint, plan: &RelayPlan) -> io::Result<()> {
    if *plan == RelayPlan::None {
        return Ok(());
    }
    // A denied token does not resolve by waiting: report it right away.
    let denied = async {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let relays = endpoint.home_relay_status().get();
            if relays.iter().any(|r| r.auth_denied_reason().is_some()) {
                return;
            }
        }
    };
    let online = tokio::time::timeout(RELAY_TIMEOUT, async {
        tokio::select! {
            () = endpoint.online() => true,
            () = denied => false,
        }
    });
    if online.await == Ok(true) {
        return Ok(());
    }
    let why = match status(endpoint, plan) {
        Some(RelayStatus {
            url,
            error: Some(error),
            ..
        }) => format!("{url}: {error}"),
        Some(RelayStatus { url, .. }) => format!("{url} did not answer"),
        None => "the relay did not answer".into(),
    };
    Err(io_err(format!("Cannot reach the relay server ({why})")))
}

/// The state of the home relay.
pub(crate) fn status(endpoint: &Endpoint, plan: &RelayPlan) -> Option<RelayStatus> {
    if *plan == RelayPlan::None {
        return None;
    }
    let relays = endpoint.home_relay_status().get();
    let relay = relays
        .iter()
        .find(|r| r.is_connected())
        .or_else(|| relays.first())?;
    Some(RelayStatus {
        url: relay.url().to_string(),
        connected: relay.is_connected(),
        error: relay.auth_denied_reason().map(str::to_string).or_else(|| {
            relay
                .last_error()
                .map(|e| format!("{e:#}"))
                .filter(|_| !relay.is_connected())
        }),
    })
}

/// The relay URL others can reach this endpoint through, for the session link.
pub(crate) fn home_relay(endpoint: &Endpoint) -> Option<String> {
    let addr = endpoint.addr();
    let url = addr.relay_urls().next()?;
    let s = url.to_string();
    Some(s.trim_end_matches('/').to_string())
}

/// The path a connection currently sends on: the selected one, else the first open one.
pub(crate) fn path_of(conn: &IrohConnection) -> Option<ConnectionPath> {
    if conn.close_reason().is_some() {
        return None;
    }
    let paths = conn.paths();
    let path = paths
        .iter()
        .find(|p| p.is_selected())
        .or_else(|| paths.iter().next())?;
    let (kind, remote) = match path.remote_addr() {
        TransportAddr::Ip(addr) => (PathKind::Direct, addr.to_string()),
        TransportAddr::Relay(url) => (
            PathKind::Relay,
            url.to_string().trim_end_matches('/').into(),
        ),
        other => (PathKind::Relay, format!("{other:?}")),
    };
    let rtt = path.rtt();
    Some(ConnectionPath {
        kind,
        remote,
        rtt_ms: (!rtt.is_zero()).then(|| u32::try_from(rtt.as_millis()).unwrap_or(u32::MAX)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_relay_means_disabled() {
        assert_eq!(relay_mode(&RelayPlan::None).unwrap(), RelayMode::Disabled);
        assert!(relay_mode(&RelayPlan::None)
            .unwrap()
            .relay_map()
            .urls::<Vec<_>>()
            .is_empty());
    }

    #[test]
    fn a_relay_server_is_the_only_relay() {
        let plan = RelayPlan::Custom {
            url: "https://relay.example.com".into(),
            token: Some("tok".into()),
        };
        let map = relay_mode(&plan).unwrap().relay_map();
        let urls: Vec<String> = map.urls::<Vec<_>>().iter().map(|u| u.to_string()).collect();
        assert_eq!(urls, ["https://relay.example.com/"]);
        let relays = map.relays::<Vec<_>>();
        assert_eq!(relays[0].auth_token.as_deref(), Some("tok"));
    }

    #[test]
    fn public_relays_are_the_default_map() {
        let map = relay_mode(&RelayPlan::Public).unwrap().relay_map();
        assert!(!map.urls::<Vec<_>>().is_empty());
    }
}
