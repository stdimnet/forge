//! Jupyter server addresses as users paste them (`http://localhost:8888/lab?token=…`).

use serde::{Deserialize, Serialize};

/// A server's base URL (with its base path, without a trailing `/`) and token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerAddress {
    pub base: String,
    #[serde(default, skip_serializing)]
    pub token: String,
}

/// Split a pasted URL into the base URL and the `token` query parameter (`token` wins
/// when given). Paths of the web UI (`/lab`, `/tree`, `/notebooks/…`) are removed.
pub fn split_server_url(url: &str, token: &str) -> Result<ServerAddress, String> {
    let url = url.trim();
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| "Enter the server URL with http:// or https://".to_string())?;
    if scheme != "http" && scheme != "https" {
        return Err("Jupyter servers are reached over http:// or https://".into());
    }
    let (before_fragment, _) = rest.split_once('#').unwrap_or((rest, ""));
    let (location, query) = before_fragment
        .split_once('?')
        .unwrap_or((before_fragment, ""));
    let (host, path) = location.split_once('/').unwrap_or((location, ""));
    if host.is_empty() {
        return Err("The server URL has no host".into());
    }
    let mut segments: Vec<&str> = Vec::new();
    for seg in path.split('/').filter(|s| !s.is_empty()) {
        if matches!(
            seg,
            "lab" | "tree" | "notebooks" | "doc" | "edit" | "terminals" | "login"
        ) {
            break;
        }
        segments.push(seg);
    }
    let from_query = query
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .find(|(k, _)| *k == "token")
        .map(|(_, v)| v.to_string())
        .unwrap_or_default();
    let base = if segments.is_empty() {
        format!("{scheme}://{host}")
    } else {
        format!("{scheme}://{host}/{}", segments.join("/"))
    };
    Ok(ServerAddress {
        base,
        token: if token.trim().is_empty() {
            from_query
        } else {
            token.trim().to_string()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_pasted_urls() {
        let a = split_server_url("http://127.0.0.1:8888/lab?token=abc123", "").unwrap();
        assert_eq!(
            a,
            ServerAddress {
                base: "http://127.0.0.1:8888".into(),
                token: "abc123".into()
            }
        );
        let a =
            split_server_url(" https://hub.example.org/user/ana/tree/work?x=1 ", "tok").unwrap();
        assert_eq!(
            a,
            ServerAddress {
                base: "https://hub.example.org/user/ana".into(),
                token: "tok".into()
            }
        );
        assert!(split_server_url("localhost:8888", "").is_err());
        assert!(split_server_url("ftp://x", "").is_err());
    }
}
