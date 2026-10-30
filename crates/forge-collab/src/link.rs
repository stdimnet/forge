//! Session links: what a host sends to guests.
//!
//! ```text
//! forge://cwm/join?v=1&name=Robert&m=laptop&s=<sessionId>#h=<hostId>&k=<joinSecret>&r=<relayUrl>&a=<addr>,<addr>
//! ```
//!
//! Everything that grants access (the host's key, the join secret, where to reach it) is in
//! the fragment, so it never reaches a web server if the link is routed through a web page.
//! The same data has a text form for the Join Session dialog: `cwm1:` followed by the part
//! after `forge://cwm/join?`.
//!
//! Links arrive from chat messages and the clipboard, so the parser treats them as untrusted
//! input: every field is validated, sizes are bounded and duplicate fields are refused.

use std::fmt;
use std::net::SocketAddr;

use data_encoding::BASE32_NOPAD;
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Prefix of the deep link.
pub const URL_PREFIX: &str = "forge://cwm/join?";
/// Prefix of the copyable text form.
pub const TEXT_PREFIX: &str = "cwm1:";
/// Link format version (`v=`).
pub const LINK_VERSION: u32 = 1;

pub const SESSION_ID_LEN: usize = 16;
pub const HOST_ID_LEN: usize = 32;
pub const JOIN_SECRET_LEN: usize = 32;

/// Longest link the parser looks at.
pub const MAX_LINK_LEN: usize = 2048;
const MAX_NAME_CHARS: usize = 64;
const MAX_RELAY_LEN: usize = 256;
const MAX_ADDRS: usize = 16;

/// Characters that stay as they are in query values (RFC 3986 unreserved).
const VALUE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// A parsed session link. `Debug` never prints the join secret.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionLink {
    /// The host's user name (`name`).
    pub host_name: String,
    /// The host's machine (`m`), shown next to the name.
    pub machine: Option<String>,
    /// Random id of this session (`s`).
    pub session_id: [u8; SESSION_ID_LEN],
    /// The host's endpoint key for this session (`h`); the transport authenticates the host
    /// by it.
    pub host_id: [u8; HOST_ID_LEN],
    /// Secret that proves a guest got the link (`k`).
    pub join_secret: [u8; JOIN_SECRET_LEN],
    /// Relay server URL (`r`), if the host uses one.
    pub relay: Option<String>,
    /// Direct addresses of the host (`a`).
    pub addrs: Vec<SocketAddr>,
}

impl fmt::Debug for SessionLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionLink")
            .field("host_name", &self.host_name)
            .field("machine", &self.machine)
            .field("session_id", &encode_id(&self.session_id))
            .field("host_id", &encode_id(&self.host_id))
            .field("join_secret", &"<redacted>")
            .field("relay", &self.relay)
            .field("addrs", &self.addrs)
            .finish()
    }
}

/// What the Join Session dialog shows about a link (nothing that grants access).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSummary {
    pub host_name: String,
    /// The host's machine as the link names it (the Join Session dialog shows it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    /// `name` or `name (machine)`.
    pub host_label: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay: Option<String>,
}

/// Lowercase unpadded base32, the encoding of ids and secrets in links.
pub fn encode_id(bytes: &[u8]) -> String {
    BASE32_NOPAD.encode(bytes).to_ascii_lowercase()
}

fn decode_fixed<const N: usize>(field: &str, value: &str) -> Result<[u8; N]> {
    let bytes = BASE32_NOPAD
        .decode(value.to_ascii_uppercase().as_bytes())
        .map_err(|_| Error::InvalidLink(format!("{field} is not base32")))?;
    bytes
        .try_into()
        .map_err(|_| Error::InvalidLink(format!("{field} has the wrong length")))
}

fn check_label(field: &str, value: &str) -> Result<()> {
    let chars = value.chars().count();
    if chars == 0 || chars > MAX_NAME_CHARS {
        return Err(Error::InvalidLink(format!(
            "{field} must have 1 to {MAX_NAME_CHARS} characters"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(Error::InvalidLink(format!(
            "{field} contains control characters"
        )));
    }
    Ok(())
}

pub(crate) fn check_relay(url: &str) -> Result<()> {
    let ok_scheme = url.starts_with("https://") || url.starts_with("http://");
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or_default();
    if !ok_scheme
        || rest.is_empty()
        || url.len() > MAX_RELAY_LEN
        || url
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '#')
    {
        return Err(Error::InvalidLink("the relay URL is invalid".into()));
    }
    Ok(())
}

impl SessionLink {
    /// The deep link (`forge://cwm/join?…`).
    pub fn to_url(&self) -> String {
        format!("{URL_PREFIX}{}", self.body())
    }

    /// The copyable text form (`cwm1:…`).
    pub fn to_text(&self) -> String {
        format!("{TEXT_PREFIX}{}", self.body())
    }

    fn body(&self) -> String {
        let enc = |s: &str| utf8_percent_encode(s, VALUE).to_string();
        let mut query = format!("v={LINK_VERSION}&name={}", enc(&self.host_name));
        if let Some(m) = &self.machine {
            query.push_str(&format!("&m={}", enc(m)));
        }
        query.push_str(&format!("&s={}", encode_id(&self.session_id)));
        let mut fragment = format!(
            "h={}&k={}",
            encode_id(&self.host_id),
            encode_id(&self.join_secret)
        );
        if let Some(r) = &self.relay {
            fragment.push_str(&format!("&r={}", enc(r)));
        }
        if !self.addrs.is_empty() {
            let addrs: Vec<String> = self.addrs.iter().map(|a| enc(&a.to_string())).collect();
            fragment.push_str(&format!("&a={}", addrs.join(",")));
        }
        format!("{query}#{fragment}")
    }

    /// Parse a deep link or its text form. Surrounding whitespace is ignored.
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();
        if input.len() > MAX_LINK_LEN {
            return Err(Error::InvalidLink("the link is too long".into()));
        }
        let body = input
            .strip_prefix(URL_PREFIX)
            .or_else(|| input.strip_prefix(TEXT_PREFIX))
            .ok_or_else(|| Error::InvalidLink("not a Code With Me link".into()))?;
        let (query, fragment) = body
            .split_once('#')
            .ok_or_else(|| Error::InvalidLink("the link is incomplete".into()))?;

        let mut fields: Vec<(&str, String)> = Vec::new();
        for pair in query.split('&').chain(fragment.split('&')) {
            if pair.is_empty() {
                continue;
            }
            let (key, raw) = pair.split_once('=').unwrap_or((pair, ""));
            if fields.iter().any(|(k, _)| *k == key) {
                return Err(Error::InvalidLink(format!("{key} appears twice")));
            }
            let value = percent_decode_str(raw)
                .decode_utf8()
                .map_err(|_| Error::InvalidLink(format!("{key} is not UTF-8")))?
                .into_owned();
            fields.push((key, value));
        }
        let get = |key: &str| fields.iter().find(|(k, _)| *k == key).map(|(_, v)| v);
        let require =
            |key: &str| get(key).ok_or_else(|| Error::InvalidLink(format!("{key} is missing")));

        if require("v")?.parse::<u32>().ok() != Some(LINK_VERSION) {
            return Err(Error::InvalidLink(
                "the link is from a different Forge version".into(),
            ));
        }
        let host_name = require("name")?.clone();
        check_label("name", &host_name)?;
        let machine = get("m").cloned();
        if let Some(m) = &machine {
            check_label("m", m)?;
        }
        let relay = get("r").filter(|r| !r.is_empty()).cloned();
        if let Some(r) = &relay {
            check_relay(r)?;
        }
        let mut addrs = Vec::new();
        if let Some(list) = get("a").filter(|a| !a.is_empty()) {
            for a in list.split(',') {
                if addrs.len() == MAX_ADDRS {
                    return Err(Error::InvalidLink("too many addresses".into()));
                }
                addrs.push(
                    a.parse::<SocketAddr>()
                        .map_err(|_| Error::InvalidLink(format!("{a:?} is not an address")))?,
                );
            }
        }
        Ok(Self {
            host_name,
            machine,
            session_id: decode_fixed("s", require("s")?)?,
            host_id: decode_fixed("h", require("h")?)?,
            join_secret: decode_fixed("k", require("k")?)?,
            relay,
            addrs,
        })
    }

    pub fn summary(&self) -> LinkSummary {
        LinkSummary {
            host_name: self.host_name.clone(),
            machine: self.machine.clone(),
            host_label: match &self.machine {
                Some(m) => format!("{} ({m})", self.host_name),
                None => self.host_name.clone(),
            },
            session_id: encode_id(&self.session_id),
            relay: self.relay.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SessionLink {
        SessionLink {
            host_name: "ExtremTechniker".into(),
            machine: Some("büro-pc".into()),
            session_id: [7; SESSION_ID_LEN],
            host_id: [1; HOST_ID_LEN],
            join_secret: [9; JOIN_SECRET_LEN],
            relay: Some("https://relay.example.com".into()),
            addrs: vec![
                "192.168.1.20:41641".parse().unwrap(),
                "[fe80::1]:41641".parse().unwrap(),
            ],
        }
    }

    #[test]
    fn round_trips_both_forms() {
        let link = sample();
        let url = link.to_url();
        assert!(url.starts_with("forge://cwm/join?v=1&name=ExtremTechniker&m=b%C3%BCro-pc&s="));
        assert_eq!(SessionLink::parse(&url).unwrap(), link);
        let text = link.to_text();
        assert!(text.starts_with("cwm1:v=1&"));
        assert_eq!(SessionLink::parse(&format!("  {text}\n")).unwrap(), link);
    }

    #[test]
    fn secrets_are_only_in_the_fragment() {
        let url = sample().to_url();
        let (query, fragment) = url.split_once('#').unwrap();
        assert!(!query.contains("k="));
        assert!(!query.contains("h="));
        assert!(fragment.starts_with("h="));
        assert!(fragment.contains("&k="));
    }

    #[test]
    fn minimal_link_without_relay_and_addresses() {
        let mut link = sample();
        link.relay = None;
        link.addrs.clear();
        link.machine = None;
        let parsed = SessionLink::parse(&link.to_url()).unwrap();
        assert_eq!(parsed, link);
        assert_eq!(parsed.summary().host_label, "ExtremTechniker");
    }

    #[test]
    fn summary_has_no_secrets() {
        let s = sample().summary();
        assert_eq!(s.host_label, "ExtremTechniker (büro-pc)");
        assert_eq!(s.machine.as_deref(), Some("büro-pc"));
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains(&encode_id(&[9; JOIN_SECRET_LEN])));
        assert!(!format!("{:?}", sample()).contains(&encode_id(&[9; JOIN_SECRET_LEN])));
    }

    #[test]
    fn ids_are_case_insensitive() {
        let link = sample();
        let (h, k) = (encode_id(&link.host_id), encode_id(&link.join_secret));
        let url = link
            .to_url()
            .replace(&h, &h.to_uppercase())
            .replace(&k, &k.to_uppercase());
        assert_eq!(SessionLink::parse(&url).unwrap(), link);
    }

    #[test]
    fn rejects_bad_input() {
        let good = sample().to_url();
        let bad = [
            "".to_string(),
            "https://example.com".to_string(),
            "forge://cwm/join?v=1&name=x&s=aaaa".to_string(),
            good.replace("v=1", "v=2"),
            good.replace("name=ExtremTechniker", "name="),
            good.replace("name=ExtremTechniker", "name=%07bell"),
            good.replace(
                "name=ExtremTechniker",
                &format!("name={}", "x".repeat(65)),
            ),
            good.replace("&m=b%C3%BCro-pc", "&m=%FF"),
            good.replace("h=", "h=zz"),
            good.replace("k=", "k=a"),
            good.replace("https%3A%2F%2Frelay.example.com", "ftp%3A%2F%2Fx"),
            good.replace("https%3A%2F%2Frelay.example.com", "https%3A%2F%2F"),
            good.replace("192.168.1.20%3A41641", "not-an-address"),
            format!("{good}&k=aaaa"),
            format!("{good}&a={}", vec!["1.2.3.4:1"; 17].join(",")).replace("&a=192", "&x=192"),
            format!("{good}{}", "x".repeat(MAX_LINK_LEN)),
        ];
        for b in bad {
            assert!(SessionLink::parse(&b).is_err(), "accepted {b}");
        }
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let url = sample().to_url().replace("&s=", "&future=1&s=");
        assert_eq!(SessionLink::parse(&url).unwrap(), sample());
    }
}
