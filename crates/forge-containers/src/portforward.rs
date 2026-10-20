//! Kubernetes port forwarding (`kubectl port-forward`, PHPStorm's Forward Ports): the
//! `portforward` subresource over a WebSocket with the `v4.channel.k8s.io` channel protocol,
//! and the list of active forwards the Services tool window shows.
//!
//! One WebSocket carries one TCP connection to one pod port: channel 0 is the data stream,
//! channel 1 the error stream. Every WebSocket message starts with its channel byte; the
//! first two bytes the server sends on each channel are the port (little endian).

use serde::{Deserialize, Serialize};

use crate::{err, Result};

/// Offered WebSocket subprotocols.
pub const PROTOCOLS: &[&str] = &["v4.channel.k8s.io", "portforward.k8s.io"];

pub const DATA_CHANNEL: u8 = 0;
pub const ERROR_CHANNEL: u8 = 1;

/// `GET …/pods/<pod>/portforward?ports=<port>`.
pub fn portforward_path(ns: &str, pod: &str, port: u16) -> String {
    format!(
        "/api/v1/namespaces/{}/pods/{}/portforward?ports={port}",
        crate::kube::encode_query(ns),
        crate::kube::encode_query(pod)
    )
}

/// A data message for the server.
pub fn data_message(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 1);
    out.push(DATA_CHANNEL);
    out.extend_from_slice(data);
    out
}

/// What a server message means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Data(Vec<u8>),
    /// Text on the error channel (e.g. `error forwarding port 80 to pod …`).
    Error(String),
}

/// Splits server messages into channels and strips the port prefixes.
#[derive(Debug, Clone)]
pub struct ChannelDecoder {
    port: u16,
    /// Prefix bytes still expected per channel.
    prefix_left: [usize; 2],
    prefix: [Vec<u8>; 2],
}

impl ChannelDecoder {
    pub fn new(port: u16) -> Self {
        ChannelDecoder {
            port,
            prefix_left: [2, 2],
            prefix: [Vec::new(), Vec::new()],
        }
    }

    /// Decode one binary WebSocket message; `None` for messages that carry no payload (port
    /// prefixes, empty frames).
    pub fn push(&mut self, msg: &[u8]) -> Result<Option<Event>> {
        let Some((&channel, mut data)) = msg.split_first() else {
            return Ok(None);
        };
        let ch = match channel {
            DATA_CHANNEL => 0,
            ERROR_CHANNEL => 1,
            other => return err(format!("Unexpected port-forward channel {other}")),
        };
        if self.prefix_left[ch] > 0 {
            let n = self.prefix_left[ch].min(data.len());
            self.prefix[ch].extend_from_slice(&data[..n]);
            self.prefix_left[ch] -= n;
            data = &data[n..];
            if self.prefix_left[ch] == 0 {
                let p = u16::from_le_bytes([self.prefix[ch][0], self.prefix[ch][1]]);
                if p != self.port {
                    return err(format!(
                        "The server forwards port {p}, expected {}",
                        self.port
                    ));
                }
            }
        }
        if data.is_empty() {
            return Ok(None);
        }
        Ok(Some(if ch == 0 {
            Event::Data(data.to_vec())
        } else {
            Event::Error(String::from_utf8_lossy(data).trim().to_string())
        }))
    }
}

/// `8080:80`, `80` (same local port) or `:80` (any free local port) → `(local, remote)`;
/// local `0` means any free port.
pub fn parse_spec(spec: &str) -> Result<(u16, u16)> {
    let spec = spec.trim();
    let port = |s: &str, what: &str| -> Result<u16> {
        match s.trim().parse::<u16>() {
            Ok(p) if p > 0 || what == "local" => Ok(p),
            _ => err(format!("Invalid {what} port `{}`", s.trim())),
        }
    };
    match spec.split_once(':') {
        Some(("", remote)) => Ok((0, port(remote, "pod")?)),
        Some((local, remote)) => Ok((port(local, "local")?, port(remote, "pod")?)),
        None => {
            let p = port(spec, "pod")?;
            Ok((p, p))
        }
    }
}

/// Several specs separated by commas or spaces (`8080:80, 9000`).
pub fn parse_specs(text: &str) -> Result<Vec<(u16, u16)>> {
    let specs: Vec<(u16, u16)> = text
        .split([',', ' ', ';'])
        .filter(|s| !s.trim().is_empty())
        .map(parse_spec)
        .collect::<Result<_>>()?;
    if specs.is_empty() {
        return err("Enter the ports to forward, e.g. 8080:80");
    }
    Ok(specs)
}

/// An active port forward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortForward {
    pub id: u64,
    /// The pod's node id.
    pub node_id: String,
    pub context: String,
    pub namespace: String,
    pub pod: String,
    /// Local address the listener is bound to (`127.0.0.1`).
    pub address: String,
    pub local_port: u16,
    pub remote_port: u16,
    /// Open connections through the forward.
    #[serde(default)]
    pub connections: u32,
    /// The last connection error (pod gone, port not listening).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl PortForward {
    /// `127.0.0.1:8080 → 80`.
    pub fn label(&self) -> String {
        format!(
            "{}:{} → {}",
            self.address, self.local_port, self.remote_port
        )
    }

    pub fn url(&self) -> String {
        format!("http://localhost:{}", self.local_port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs() {
        assert_eq!(parse_spec("8080:80").unwrap(), (8080, 80));
        assert_eq!(parse_spec(" 5432 ").unwrap(), (5432, 5432));
        assert_eq!(parse_spec(":80").unwrap(), (0, 80));
        assert_eq!(parse_spec("0:80").unwrap(), (0, 80));
        assert!(parse_spec("80:0").is_err());
        assert!(parse_spec("x:80").is_err());
        assert!(parse_spec("70000").is_err());
        assert_eq!(
            parse_specs("8080:80, 9000 ;:5432").unwrap(),
            [(8080, 80), (9000, 9000), (0, 5432)]
        );
        assert!(parse_specs("  ").is_err());
        assert!(parse_specs("80, x").is_err());
    }

    #[test]
    fn channels_and_port_prefixes() {
        let mut d = ChannelDecoder::new(8080);
        let port = 8080u16.to_le_bytes();
        // Prefixes may come alone or split.
        assert_eq!(d.push(&[0, port[0]]).unwrap(), None);
        assert_eq!(
            d.push(&[0, port[1], b'h', b'i']).unwrap(),
            Some(Event::Data(b"hi".to_vec()))
        );
        assert_eq!(d.push(&[1, port[0], port[1]]).unwrap(), None);
        assert_eq!(
            d.push(b"\x01error forwarding port 8080\n").unwrap(),
            Some(Event::Error("error forwarding port 8080".into()))
        );
        assert_eq!(
            d.push(&[0, b'!']).unwrap(),
            Some(Event::Data(b"!".to_vec()))
        );
        assert_eq!(d.push(&[]).unwrap(), None);
        assert!(d.push(&[7, 1]).is_err());
        let mut wrong = ChannelDecoder::new(80);
        assert!(wrong.push(&[0, port[0], port[1]]).is_err());
        assert_eq!(data_message(b"GET"), b"\x00GET");
        assert_eq!(
            portforward_path("shop", "web-1", 80),
            "/api/v1/namespaces/shop/pods/web-1/portforward?ports=80"
        );
    }

    #[test]
    fn labels() {
        let f = PortForward {
            id: 1,
            node_id: "kube/kind/shop/pods/web-1".into(),
            context: "kind".into(),
            namespace: "shop".into(),
            pod: "web-1".into(),
            address: "127.0.0.1".into(),
            local_port: 8080,
            remote_port: 80,
            connections: 0,
            error: None,
        };
        assert_eq!(f.label(), "127.0.0.1:8080 → 80");
        assert_eq!(f.url(), "http://localhost:8080");
    }
}
