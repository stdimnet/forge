//! Backend calls plugins make through the host (`forge.fs`, `forge.http`, `forge.process`),
//! each checked against the granted permissions.

use std::collections::BTreeMap;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::manifest::Permissions;
use crate::permissions::Guard;

/// Largest response body or file a plugin call returns.
pub const MAX_BODY: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchRequest {
    pub url: String,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

/// An HTTP request to a granted host. Redirects are followed only to granted hosts.
pub async fn fetch(
    plugin_id: &str,
    granted: &Permissions,
    request: FetchRequest,
    direct: bool,
) -> Result<FetchResponse, String> {
    let guard = Guard { plugin_id, granted };
    guard.check_network(&request.url).map_err(|e| e.0)?;
    let (id, perms) = (plugin_id.to_string(), granted.clone());
    let policy = reqwest::redirect::Policy::custom(move |attempt| {
        let guard = Guard {
            plugin_id: &id,
            granted: &perms,
        };
        if attempt.previous().len() > 5 {
            attempt.error("too many redirects")
        } else if guard.check_network(attempt.url().as_str()).is_err() {
            attempt.stop()
        } else {
            attempt.follow()
        }
    });
    let mut builder = reqwest::Client::builder()
        .user_agent(concat!("Forge/", env!("CARGO_PKG_VERSION"), " plugin"))
        .timeout(Duration::from_secs(60))
        .redirect(policy);
    if direct {
        builder = builder.no_proxy();
    }
    let client = builder.build().map_err(|e| e.to_string())?;
    let method = reqwest::Method::from_bytes(
        request
            .method
            .as_deref()
            .unwrap_or("GET")
            .to_ascii_uppercase()
            .as_bytes(),
    )
    .map_err(|_| "invalid HTTP method".to_string())?;
    let mut req = client.request(method, &request.url);
    for (k, v) in &request.headers {
        req = req.header(k, v);
    }
    if let Some(body) = request.body {
        req = req.body(body);
    }
    let response = req.send().await.map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .filter_map(|(k, v)| Some((k.to_string(), v.to_str().ok()?.to_string())))
        .collect();
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BODY {
        return Err("the response is larger than 16 MB".into());
    }
    Ok(FetchResponse {
        status,
        headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Run a granted program (found on `PATH`) with arguments in `cwd` and wait for it.
pub fn run(
    plugin_id: &str,
    granted: &Permissions,
    program: &str,
    args: &[String],
    cwd: Option<&std::path::Path>,
) -> Result<RunOutput, String> {
    Guard { plugin_id, granted }
        .check_process(program)
        .map_err(|e| e.0)?;
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    // In Flatpak the granted program runs on the host.
    let out = forge_hostspawn::host_command(cmd)
        .output()
        .map_err(|e| format!("cannot start {program}: {e}"))?;
    let cut = |b: &[u8]| String::from_utf8_lossy(&b[..b.len().min(MAX_BODY)]).into_owned();
    Ok(RunOutput {
        exit_code: out.status.code(),
        stdout: cut(&out.stdout),
        stderr: cut(&out.stderr),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_langservers::managed::testserver;

    #[tokio::test]
    async fn fetch_needs_the_host_permission() {
        let server = testserver::start().await;
        server.routes.set("/hello", "world");
        let url = format!("{}/hello", server.url);
        let none = Permissions::default();
        let e = fetch(
            "acme.x",
            &none,
            FetchRequest {
                url: url.clone(),
                ..Default::default()
            },
            true,
        )
        .await
        .unwrap_err();
        assert!(e.contains("connect to 127.0.0.1"), "{e}");
        // `127.0.0.1` is not a host pattern a manifest may use; tests grant it directly.
        let granted = Permissions {
            network: vec!["127.0.0.1".into()],
            ..Permissions::default()
        };
        let r = fetch(
            "acme.x",
            &granted,
            FetchRequest {
                url,
                ..Default::default()
            },
            true,
        )
        .await
        .unwrap();
        assert_eq!((r.status, r.body.as_str()), (200, "world"));
    }

    #[test]
    fn run_needs_the_process_permission() {
        assert!(run(
            "acme.x",
            &Permissions::default(),
            "git",
            &["--version".into()],
            None
        )
        .is_err());
        let granted = Permissions {
            process: vec!["git".into()],
            ..Permissions::default()
        };
        if let Ok(out) = run("acme.x", &granted, "git", &["--version".into()], None) {
            assert!(out.stdout.contains("git version"));
        }
    }
}
