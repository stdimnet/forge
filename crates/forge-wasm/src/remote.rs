//! Remote development (`forge-remote` without its SSH client): `~/.ssh/config` hosts,
//! SSH configuration validation and `known_hosts` checks, mirroring `remote_ssh_hosts`,
//! `remote_validate_config` and the host key step of `remote_connect`.

use forge_remote::{known_hosts, ssh_config::SshConfig, SshConfiguration};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl std::fmt::Display) -> String {
    to_json(&serde_json::json!({ "error": e.to_string() }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostsRequest {
    /// Text of `~/.ssh/config`.
    text: String,
    home: String,
    user: String,
}

/// `{ text, home, user }` → the concrete `Host` aliases (`HostEntry[]`).
#[wasm_bindgen]
pub fn remote_ssh_hosts(request_json: &str) -> String {
    match serde_json::from_str::<HostsRequest>(request_json) {
        Ok(r) => to_json(&SshConfig::parse(&r.text).hosts(&r.home, &r.user)),
        Err(e) => error(e),
    }
}

/// `SshConfiguration` → problems worded for the dialog (`string[]`, empty when valid).
#[wasm_bindgen]
pub fn remote_validate_config(config_json: &str) -> String {
    match serde_json::from_str::<SshConfiguration>(config_json) {
        Ok(c) => to_json(&c.validate()),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostKeyRequest {
    /// Text of `known_hosts`.
    known_hosts: String,
    host: String,
    port: u16,
    key_type: String,
    /// Base64 key blob.
    key: String,
    /// Fingerprint the user confirmed: trust the key and return the updated file.
    #[serde(default)]
    accept: Option<String>,
}

/// `{ knownHosts, host, port, keyType, key, accept? }` → `{ status, fingerprint, ..., knownHosts? }`
/// (the verdict, and the updated file when a confirmed key was added).
#[wasm_bindgen]
pub fn remote_check_host_key(request_json: &str) -> String {
    let r = match serde_json::from_str::<HostKeyRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let status = known_hosts::check(
        &known_hosts::parse(&r.known_hosts),
        &r.host,
        r.port,
        &r.key_type,
        &r.key,
    );
    let fingerprint = known_hosts::fingerprint(&r.key).unwrap_or_default();
    let mut out = serde_json::to_value(&status).unwrap_or_default();
    out["fingerprint"] = fingerprint.clone().into();
    out["keyType"] = r.key_type.clone().into();
    let confirmable = matches!(
        status,
        known_hosts::HostKeyStatus::Unknown | known_hosts::HostKeyStatus::Changed { .. }
    );
    if confirmable && r.accept.as_deref() == Some(fingerprint.as_str()) {
        let base = if matches!(status, known_hosts::HostKeyStatus::Changed { .. }) {
            known_hosts::remove_host_key(&r.known_hosts, &r.host, r.port, &r.key_type)
        } else {
            r.known_hosts.clone()
        };
        out["knownHosts"] = known_hosts::append(
            &base,
            &known_hosts::entry_line(&r.host, r.port, &r.key_type, &r.key),
        )
        .into();
        out["status"] = "trusted".into();
    }
    to_json(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";

    #[test]
    fn hosts_and_validation() {
        let out = remote_ssh_hosts(
            r#"{"text":"Host shop\n  HostName shop.example.com\n  User deploy\n","home":"/home/me","user":"me"}"#,
        );
        assert!(out.contains(r#""alias":"shop""#), "{out}");
        assert!(out.contains(r#""hostName":"shop.example.com""#), "{out}");
        let problems = remote_validate_config(r#"{"id":"","host":"","port":22,"user":"deploy"}"#);
        assert_eq!(problems, r#"["Host is required"]"#);
    }

    #[test]
    fn unknown_keys_are_trusted_after_confirmation() {
        let req = |accept: &str| {
            format!(
                r#"{{"knownHosts":"","host":"shop.example.com","port":22,"keyType":"ssh-ed25519","key":"{KEY}"{accept}}}"#
            )
        };
        let first: serde_json::Value =
            serde_json::from_str(&remote_check_host_key(&req(""))).unwrap();
        assert_eq!(first["status"], "unknown");
        let fp = first["fingerprint"].as_str().unwrap().to_string();
        let accepted: serde_json::Value = serde_json::from_str(&remote_check_host_key(&req(
            &format!(r#","accept":"{fp}""#),
        )))
        .unwrap();
        assert_eq!(accepted["status"], "trusted");
        assert_eq!(
            accepted["knownHosts"],
            format!("shop.example.com ssh-ed25519 {KEY}\n")
        );
    }
}
