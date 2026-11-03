//! Relays and NAT traversal: which relay an endpoint may use, and how a connection travels.
//!
//! Connections are direct by default: the guest dials the host's addresses from the link and
//! nothing else is contacted. A relay is used only when the user sets one:
//!
//! - **Relay server** (`codeWithMe.relayUrl`): a self-hosted `iroh-relay`. The host keeps a
//!   connection to it and puts its URL into the session link (`r=`); a guest dials the host's
//!   direct addresses and the relay at the same time. Both ends try to punch a hole through
//!   their NATs over the relay; if that works the connection moves to the direct path,
//!   otherwise it stays relayed. The relay only forwards end-to-end encrypted QUIC packets.
//! - **Public relays** (`codeWithMe.usePublicRelays`, off by default): the iroh relays run
//!   by n0, an explicit opt-in. Forge runs no relay or other service of its own.
//! - **Relay token**: for relays with token access control (`access.shared_token`). It is a
//!   secret, kept in the OS keychain and never put into links; guests that use the same relay
//!   enter it as well.
//! - **LAN discovery** (`codeWithMe.lanDiscovery`): mDNS on the local network, so a guest
//!   finds the host even when its address changed. It never leaves the local network.
//!
//! This module is pure (no network); the transport turns [`RelayOptions`] into an endpoint
//! configuration (`transport::relay`).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Longest relay URL (the link parser has the same limit).
pub const MAX_RELAY_URL_LEN: usize = 256;
/// Longest relay token.
pub const MAX_RELAY_TOKEN_LEN: usize = 512;

/// How an endpoint may use relays. `Debug` never prints the token.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct RelayOptions {
    /// A self-hosted relay (normalized with [`normalize_relay_url`]).
    pub url: Option<String>,
    /// Bearer token for `url`.
    pub token: Option<String>,
    /// Use n0's public relays when `url` is not set.
    pub public: bool,
    /// Find peers on the local network with mDNS.
    pub lan_discovery: bool,
}

impl fmt::Debug for RelayOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RelayOptions")
            .field("url", &self.url)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field("public", &self.public)
            .field("lan_discovery", &self.lan_discovery)
            .finish()
    }
}

/// The relay an endpoint ends up with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayPlan {
    /// Direct connections only: no relay, nothing outside the given addresses is contacted.
    None,
    /// One relay (self-hosted, or the one from a session link).
    Custom { url: String, token: Option<String> },
    /// n0's public relays.
    Public,
}

impl RelayOptions {
    /// The options of a host, from the settings and the keychain.
    pub fn host(url: Option<String>, token: Option<String>, public: bool, lan: bool) -> Self {
        let token = url.as_ref().and(token);
        Self {
            public: public && url.is_none(),
            url,
            token,
            lan_discovery: lan,
        }
    }

    /// The options of a guest joining a session whose link names `link_relay`. The guest
    /// uses the host's relay (it is where the host can be reached), with its own token only
    /// if it is configured for that same relay. It never falls back to public relays: if the
    /// host uses them, the link names the public relay.
    pub fn guest(link_relay: Option<&str>, own: &RelayOptions) -> Self {
        let url = link_relay.and_then(|r| normalize_relay_url(r).ok().flatten());
        let token = match (&url, &own.url) {
            (Some(a), Some(b)) if same_relay(a, b) => own.token.clone(),
            _ => None,
        };
        Self {
            url,
            token,
            public: false,
            lan_discovery: own.lan_discovery,
        }
    }

    pub fn plan(&self) -> RelayPlan {
        match (&self.url, self.public) {
            (Some(url), _) => RelayPlan::Custom {
                url: url.clone(),
                token: self.token.clone(),
            },
            (None, true) => RelayPlan::Public,
            (None, false) => RelayPlan::None,
        }
    }

    /// Whether the endpoint talks to anything beyond the peers' own addresses (a relay).
    /// LAN discovery stays on the local network and does not count.
    pub fn contacts_servers(&self) -> bool {
        self.plan() != RelayPlan::None
    }
}

/// Check and normalize the "Relay server" setting. Empty means none; a missing scheme means
/// `https://`. Tokens do not belong into the URL (it ends up in session links).
pub fn normalize_relay_url(raw: &str) -> Result<Option<String>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    let invalid = |why: &str| Error::InvalidRelay(why.to_string());
    let with_scheme = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("https://{raw}")
    };
    let (scheme, rest) = with_scheme.split_once("://").expect("has a scheme");
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "https" && scheme != "http" {
        return Err(invalid("use an https:// or http:// URL"));
    }
    if with_scheme.len() > MAX_RELAY_URL_LEN {
        return Err(invalid("the URL is too long"));
    }
    if rest
        .chars()
        .any(|c| c.is_control() || c.is_whitespace() || c == '#')
    {
        return Err(invalid(
            "the URL contains spaces or other invalid characters",
        ));
    }
    if rest.contains('?') {
        return Err(invalid(
            "the URL must not have a query; enter a token with Set Token… instead",
        ));
    }
    let (authority, path) = match rest.find('/') {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    if authority.contains('@') {
        return Err(invalid(
            "the URL must not contain a user name or password; enter a token with Set Token… instead",
        ));
    }
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split_once(']').map(|(h, _)| h).unwrap_or_default(),
        None => authority.rsplit_once(':').map_or(authority, |(h, _)| h),
    };
    if host.is_empty() {
        return Err(invalid("the URL has no host name"));
    }
    if let Some((_, port)) = authority
        .rsplit_once(':')
        .filter(|_| !authority.ends_with(']'))
    {
        if port.parse::<u16>().map_or(true, |p| p == 0) {
            return Err(invalid("the port is invalid"));
        }
    }
    let path = path.trim_end_matches('/');
    Ok(Some(format!(
        "{scheme}://{}{path}",
        authority.to_ascii_lowercase()
    )))
}

/// Whether two relay URLs name the same relay (ignoring case of scheme and host, and
/// trailing slashes).
pub fn same_relay(a: &str, b: &str) -> bool {
    match (normalize_relay_url(a), normalize_relay_url(b)) {
        (Ok(Some(a)), Ok(Some(b))) => a == b,
        _ => false,
    }
}

/// Check a relay token as typed. `Ok(None)` for an empty input (no token).
pub fn normalize_relay_token(raw: &str) -> Result<Option<String>> {
    let token = raw.trim();
    if token.is_empty() {
        return Ok(None);
    }
    // Sent as `Authorization: Bearer <token>`.
    if token.len() > MAX_RELAY_TOKEN_LEN || !token.chars().all(|c| c.is_ascii_graphic()) {
        return Err(Error::InvalidRelay(
            "the relay token must be printable ASCII without spaces".into(),
        ));
    }
    Ok(Some(token.to_string()))
}

/// The relay's host name for the UI (`relay.example.com`).
pub fn relay_host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split('/').next().unwrap_or(rest);
    authority.to_string()
}

/// How a connection to a peer currently travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PathKind {
    /// UDP straight to the peer (LAN, port forward, VPN, or a punched hole).
    Direct,
    /// Through a relay server, end-to-end encrypted.
    Relay,
}

/// The path in use for one connection, for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionPath {
    pub kind: PathKind,
    /// The peer's address (`192.168.1.20:41641`) or the relay's URL.
    pub remote: String,
    /// Round-trip time on this path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtt_ms: Option<u32>,
}

impl ConnectionPath {
    /// "Direct" or "Relay (relay.example.com)", as the participants list shows it.
    pub fn label(&self) -> String {
        match self.kind {
            PathKind::Direct => "Direct".into(),
            PathKind::Relay => format!("Relay ({})", relay_host(&self.remote)),
        }
    }
}

/// How the connection to the relay is doing (host: the home relay; guest: the relay of the
/// link).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayStatus {
    pub url: String,
    pub connected: bool,
    /// Why the relay cannot be used (denied token, unreachable, TLS), when it is not connected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_urls_are_normalized() {
        let n = |s: &str| normalize_relay_url(s).unwrap();
        assert_eq!(n(""), None);
        assert_eq!(n("   "), None);
        assert_eq!(
            n("relay.example.com"),
            Some("https://relay.example.com".into())
        );
        assert_eq!(
            n(" HTTPS://Relay.Example.com/ "),
            Some("https://relay.example.com".into())
        );
        assert_eq!(
            n("http://10.0.0.5:3340/"),
            Some("http://10.0.0.5:3340".into())
        );
        assert_eq!(n("https://[::1]:443"), Some("https://[::1]:443".into()));
        assert_eq!(
            n("https://example.com/relay/"),
            Some("https://example.com/relay".into())
        );
    }

    #[test]
    fn bad_relay_urls_are_refused() {
        for bad in [
            "ftp://relay.example.com",
            "https://",
            "https://:443",
            "https://relay.example.com:0",
            "https://relay.example.com:99999",
            "https://relay example.com",
            "https://relay.example.com/?token=secret",
            "https://user:secret@relay.example.com",
            "https://relay.example.com/#x",
        ] {
            assert!(normalize_relay_url(bad).is_err(), "accepted {bad}");
        }
        let long = format!("https://{}.com", "a".repeat(MAX_RELAY_URL_LEN));
        assert!(normalize_relay_url(&long).is_err());
    }

    #[test]
    fn normalized_urls_pass_the_link_parser() {
        let url = normalize_relay_url("Relay.Example.com:8443/")
            .unwrap()
            .unwrap();
        assert!(crate::link::check_relay(&url).is_ok());
    }

    #[test]
    fn same_relay_ignores_case_and_slashes() {
        assert!(same_relay(
            "https://relay.example.com/",
            "HTTPS://RELAY.example.com"
        ));
        assert!(!same_relay(
            "https://relay.example.com",
            "https://other.example.com"
        ));
        assert!(!same_relay("", ""));
    }

    #[test]
    fn direct_is_the_default() {
        let o = RelayOptions::default();
        assert_eq!(o.plan(), RelayPlan::None);
        assert!(!o.contacts_servers());
        let host = RelayOptions::host(None, Some("tok".into()), false, true);
        assert_eq!(host.plan(), RelayPlan::None);
        assert_eq!(host.token, None, "a token without a relay is not used");
    }

    #[test]
    fn a_relay_server_wins_over_public_relays() {
        let host = RelayOptions::host(
            Some("https://relay.example.com".into()),
            Some("tok".into()),
            true,
            false,
        );
        assert_eq!(
            host.plan(),
            RelayPlan::Custom {
                url: "https://relay.example.com".into(),
                token: Some("tok".into())
            }
        );
        assert!(!host.public);
        let public = RelayOptions::host(None, None, true, false);
        assert_eq!(public.plan(), RelayPlan::Public);
        assert!(public.contacts_servers());
    }

    #[test]
    fn guests_use_the_hosts_relay_and_their_token_only_for_it() {
        let own = RelayOptions::host(
            Some("https://relay.example.com".into()),
            Some("team-token".into()),
            true,
            true,
        );
        let g = RelayOptions::guest(Some("https://RELAY.example.com/"), &own);
        assert_eq!(g.url.as_deref(), Some("https://relay.example.com"));
        assert_eq!(g.token.as_deref(), Some("team-token"));
        assert!(g.lan_discovery);
        // Another relay: no token for it.
        let g = RelayOptions::guest(Some("https://other.example.com"), &own);
        assert_eq!(g.token, None);
        // A link without a relay: direct only, even with public relays allowed.
        let g = RelayOptions::guest(None, &own);
        assert_eq!(g.plan(), RelayPlan::None);
    }

    #[test]
    fn tokens_are_checked_and_never_printed() {
        assert_eq!(normalize_relay_token("  ").unwrap(), None);
        assert_eq!(
            normalize_relay_token(" abc-123_~ ").unwrap().as_deref(),
            Some("abc-123_~")
        );
        assert!(normalize_relay_token("with space").is_err());
        assert!(normalize_relay_token("ünicode").is_err());
        assert!(normalize_relay_token(&"x".repeat(MAX_RELAY_TOKEN_LEN + 1)).is_err());
        let o = RelayOptions::host(
            Some("https://r.example.com".into()),
            Some("s3cret".into()),
            false,
            false,
        );
        assert!(!format!("{o:?}").contains("s3cret"));
    }

    #[test]
    fn paths_have_labels_and_serialize_for_the_ui() {
        let p = ConnectionPath {
            kind: PathKind::Relay,
            remote: "https://relay.example.com".into(),
            rtt_ms: Some(48),
        };
        assert_eq!(p.label(), "Relay (relay.example.com)");
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["kind"], "relay");
        assert_eq!(json["rttMs"], 48);
        let d = ConnectionPath {
            kind: PathKind::Direct,
            remote: "192.168.1.20:41641".into(),
            rtt_ms: None,
        };
        assert_eq!(d.label(), "Direct");
        assert!(serde_json::to_value(&d).unwrap().get("rttMs").is_none());
    }
}
