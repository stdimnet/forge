//! OpenSSH `known_hosts`: which host keys are trusted. Supports plain and hashed
//! (`|1|salt|hash`) host names, `[host]:port` entries, wildcards, negations and the
//! `@revoked` marker (`@cert-authority` lines are ignored: Forge does not verify host
//! certificates). Fingerprints are shown like OpenSSH: `SHA256:` + unpadded base64.

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use base64::Engine;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};

use crate::ssh_config::wildcard_match;

/// One key line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownHost {
    /// 0-based line number in the file.
    pub line: usize,
    pub marker: Option<Marker>,
    /// Comma-separated host patterns as written (may be hashed).
    pub hosts: String,
    pub key_type: String,
    /// Base64 key blob.
    pub key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Revoked,
    CertAuthority,
}

/// The verdict for a host key the server presented.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum HostKeyStatus {
    /// A line for this host has exactly this key.
    Trusted,
    /// No line for this host and key type: ask the user to confirm the fingerprint.
    Unknown,
    /// The host has a different key of this type: possible man-in-the-middle attack.
    #[serde(rename_all = "camelCase")]
    Changed {
        line: usize,
        old_fingerprint: String,
    },
    /// The key is marked `@revoked`.
    Revoked,
}

/// Parse a known_hosts file; malformed lines are skipped.
pub fn parse(text: &str) -> Vec<KnownHost> {
    text.lines()
        .enumerate()
        .filter_map(|(i, raw)| {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let mut fields = line.split_whitespace();
            let mut first = fields.next()?;
            let marker = match first {
                "@revoked" => Some(Marker::Revoked),
                "@cert-authority" => Some(Marker::CertAuthority),
                _ => None,
            };
            if marker.is_some() {
                first = fields.next()?;
            }
            let key_type = fields.next()?.to_string();
            let key = fields.next()?.to_string();
            Some(KnownHost {
                line: i,
                marker,
                hosts: first.to_string(),
                key_type,
                key,
            })
        })
        .collect()
}

/// The name a host is stored under: `host`, or `[host]:port` for other ports.
pub fn host_entry_name(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

fn hashed_matches(pattern: &str, name: &str) -> bool {
    let Some(rest) = pattern.strip_prefix("|1|") else {
        return false;
    };
    let Some((salt, hash)) = rest.split_once('|') else {
        return false;
    };
    let (Ok(salt), Ok(hash)) = (STANDARD.decode(salt), STANDARD.decode(hash)) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(&salt) else {
        return false;
    };
    mac.update(name.as_bytes());
    mac.verify_slice(&hash).is_ok()
}

/// Whether the host field of a line covers `host` on `port`.
pub fn hosts_match(field: &str, host: &str, port: u16) -> bool {
    let name = host_entry_name(host, port);
    if field.starts_with("|1|") {
        return hashed_matches(field, &name);
    }
    let mut matched = false;
    for pattern in field.split(',') {
        let (negated, p) = match pattern.strip_prefix('!') {
            Some(p) => (true, p),
            None => (false, pattern),
        };
        if wildcard_match(p, &name) {
            if negated {
                return false;
            }
            matched = true;
        }
    }
    matched
}

/// Check the key a server presented for `host`:`port`.
pub fn check(
    entries: &[KnownHost],
    host: &str,
    port: u16,
    key_type: &str,
    key: &str,
) -> HostKeyStatus {
    let matching: Vec<&KnownHost> = entries
        .iter()
        .filter(|e| e.marker != Some(Marker::CertAuthority) && hosts_match(&e.hosts, host, port))
        .collect();
    let revoked = entries
        .iter()
        .any(|e| e.marker == Some(Marker::Revoked) && e.key == key);
    if revoked {
        return HostKeyStatus::Revoked;
    }
    if matching.iter().any(|e| e.marker.is_none() && e.key == key) {
        return HostKeyStatus::Trusted;
    }
    match matching
        .iter()
        .find(|e| e.marker.is_none() && e.key_type == key_type)
    {
        Some(old) => HostKeyStatus::Changed {
            line: old.line,
            old_fingerprint: fingerprint(&old.key).unwrap_or_default(),
        },
        None => HostKeyStatus::Unknown,
    }
}

/// `SHA256:…` fingerprint of a base64 key blob, `None` when the blob is not base64.
pub fn fingerprint(key: &str) -> Option<String> {
    let blob = STANDARD.decode(key).ok()?;
    Some(format!(
        "SHA256:{}",
        STANDARD_NO_PAD.encode(Sha256::digest(&blob))
    ))
}

/// The line to append for a newly trusted key.
pub fn entry_line(host: &str, port: u16, key_type: &str, key: &str) -> String {
    format!("{} {key_type} {key}", host_entry_name(host, port))
}

/// `text` with `line` appended (and a newline before it when missing).
pub fn append(text: &str, line: &str) -> String {
    let mut out = text.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(line);
    out.push('\n');
    out
}

/// `text` without the key lines for `host`:`port` of `key_type` (when replacing a changed
/// key). Lines covering several hosts are kept unless they name only this one.
pub fn remove_host_key(text: &str, host: &str, port: u16, key_type: &str) -> String {
    let drop: std::collections::HashSet<usize> = parse(text)
        .into_iter()
        .filter(|e| {
            e.marker.is_none()
                && e.key_type == key_type
                && hosts_match(&e.hosts, host, port)
                && (e.hosts.starts_with("|1|") || !e.hosts.contains(','))
        })
        .map(|e| e.line)
        .collect();
    let mut out = String::new();
    for (i, l) in text.lines().enumerate() {
        if !drop.contains(&i) {
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real-looking ed25519 blobs (any base64 works for matching).
    const KEY_A: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";
    const KEY_B: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIBH1bD1mVrOR8vT3+3tW9I+gp5+rtZ5Xj5wIWmhB9i1y";

    /// A hashed entry for `name` as `ssh-keygen -H` writes it.
    fn hashed(name: &str) -> String {
        let salt = [7u8; 20];
        let mut mac = Hmac::<Sha1>::new_from_slice(&salt).unwrap();
        mac.update(name.as_bytes());
        format!(
            "|1|{}|{}",
            STANDARD.encode(salt),
            STANDARD.encode(mac.finalize().into_bytes())
        )
    }

    #[test]
    fn plain_hashed_and_port_entries() {
        let text = format!(
            "# comment\nshop.example.com,10.0.0.5 ssh-ed25519 {KEY_A}\n{} ssh-ed25519 {KEY_A}\n[git.example.com]:2222 ssh-ed25519 {KEY_B}\n",
            hashed("hashed.example.com")
        );
        let e = parse(&text);
        assert_eq!(e.len(), 3);
        assert_eq!(
            check(&e, "shop.example.com", 22, "ssh-ed25519", KEY_A),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            check(&e, "10.0.0.5", 22, "ssh-ed25519", KEY_A),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            check(&e, "hashed.example.com", 22, "ssh-ed25519", KEY_A),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            check(&e, "git.example.com", 2222, "ssh-ed25519", KEY_B),
            HostKeyStatus::Trusted
        );
        // Same host on another port is a different entry.
        assert_eq!(
            check(&e, "git.example.com", 22, "ssh-ed25519", KEY_B),
            HostKeyStatus::Unknown
        );
        assert_eq!(
            check(&e, "new.example.com", 22, "ssh-ed25519", KEY_A),
            HostKeyStatus::Unknown
        );
    }

    #[test]
    fn changed_keys_and_revocation() {
        let text =
            format!("shop.example.com ssh-ed25519 {KEY_A}\n@revoked * ssh-ed25519 {KEY_B}\n");
        let e = parse(&text);
        match check(
            &e,
            "shop.example.com",
            22,
            "ssh-ed25519",
            "AAAAC3NzaC1lZDI1NTE5AAAAIA==",
        ) {
            HostKeyStatus::Changed {
                line,
                old_fingerprint,
            } => {
                assert_eq!(line, 0);
                assert_eq!(old_fingerprint, fingerprint(KEY_A).unwrap());
            }
            other => panic!("{other:?}"),
        }
        // A key of another type is just not known yet.
        assert_eq!(
            check(&e, "shop.example.com", 22, "rsa-sha2-512", "AAAA"),
            HostKeyStatus::Unknown
        );
        assert_eq!(
            check(&e, "any.example.com", 22, "ssh-ed25519", KEY_B),
            HostKeyStatus::Revoked
        );
    }

    #[test]
    fn wildcards_and_negation() {
        let text = format!("*.example.com,!evil.example.com ssh-ed25519 {KEY_A}\n");
        let e = parse(&text);
        assert_eq!(
            check(&e, "a.example.com", 22, "ssh-ed25519", KEY_A),
            HostKeyStatus::Trusted
        );
        assert_eq!(
            check(&e, "evil.example.com", 22, "ssh-ed25519", KEY_A),
            HostKeyStatus::Unknown
        );
    }

    #[test]
    fn fingerprints_look_like_openssh() {
        let fp = fingerprint(KEY_A).unwrap();
        assert!(fp.starts_with("SHA256:"));
        assert_eq!(fp.len(), "SHA256:".len() + 43);
        assert!(!fp.ends_with('='));
        assert_eq!(fingerprint("not base64!"), None);
    }

    #[test]
    fn adding_and_replacing_entries() {
        let text = format!("other ssh-ed25519 {KEY_B}");
        let line = entry_line("shop.example.com", 2222, "ssh-ed25519", KEY_A);
        assert_eq!(line, format!("[shop.example.com]:2222 ssh-ed25519 {KEY_A}"));
        let added = append(&text, &line);
        assert_eq!(added, format!("other ssh-ed25519 {KEY_B}\n{line}\n"));
        let e = parse(&added);
        assert_eq!(
            check(&e, "shop.example.com", 2222, "ssh-ed25519", KEY_A),
            HostKeyStatus::Trusted
        );

        let removed = remove_host_key(&added, "shop.example.com", 2222, "ssh-ed25519");
        assert_eq!(removed, format!("other ssh-ed25519 {KEY_B}\n"));
    }
}
