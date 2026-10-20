//! Where the Docker daemon listens: Settings | Build, Execution, Deployment | Docker
//! (`docker.connection`, `docker.socketPath`, `docker.tcp.*`), else `DOCKER_HOST`, else the
//! platform default (named pipe on Windows; `/var/run/docker.sock`, Docker Desktop's
//! `~/.docker/run/docker.sock`, rootless `$XDG_RUNTIME_DIR/docker.sock` or Podman elsewhere).

use serde::{Deserialize, Serialize};

use crate::{err, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TlsFiles {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cert: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Verify the daemon's certificate (`DOCKER_TLS_VERIFY`).
    pub verify: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Endpoint {
    Unix {
        path: String,
    },
    NamedPipe {
        path: String,
    },
    #[serde(rename_all = "camelCase")]
    Tcp {
        host: String,
        port: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tls: Option<TlsFiles>,
    },
}

impl Endpoint {
    /// `unix:///var/run/docker.sock`, `npipe:////./pipe/docker_engine`, `tcp://host:2376`.
    pub fn label(&self) -> String {
        match self {
            Endpoint::Unix { path } => format!("unix://{path}"),
            Endpoint::NamedPipe { path } => format!("npipe://{}", path.replace('\\', "/")),
            Endpoint::Tcp { host, port, tls } => format!(
                "{}://{host}:{port}",
                if tls.is_some() { "https" } else { "tcp" }
            ),
        }
    }

    /// `-H` value for the docker CLI (exec, compose), so it talks to the same daemon.
    pub fn docker_host(&self) -> String {
        match self {
            Endpoint::Unix { path } => format!("unix://{path}"),
            Endpoint::NamedPipe { path } => format!("npipe://{}", path.replace('\\', "/")),
            Endpoint::Tcp { host, port, .. } => format!("tcp://{host}:{port}"),
        }
    }

    /// Environment for the docker CLI: `DOCKER_HOST` and the TLS variables.
    pub fn cli_env(&self) -> Vec<(String, String)> {
        let mut env = vec![("DOCKER_HOST".to_string(), self.docker_host())];
        if let Endpoint::Tcp { tls: Some(tls), .. } = self {
            if tls.verify {
                env.push(("DOCKER_TLS_VERIFY".into(), "1".into()));
            }
            if let Some(dir) = tls.ca.as_deref().and_then(parent_dir) {
                env.push(("DOCKER_CERT_PATH".into(), dir));
            }
        }
        env
    }
}

fn parent_dir(p: &str) -> Option<String> {
    let i = p.rfind(['/', '\\'])?;
    Some(p[..i].to_string())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerSettings {
    /// `auto` | `unix` | `pipe` | `tcp`.
    #[serde(default)]
    pub connection: String,
    #[serde(default)]
    pub socket_path: String,
    #[serde(default)]
    pub tcp_url: String,
    #[serde(default)]
    pub tls: bool,
    /// Directory with `ca.pem`, `cert.pem`, `key.pem`.
    #[serde(default)]
    pub cert_path: String,
}

/// The process environment and platform, passed in so resolution stays pure.
#[derive(Debug, Clone, Default)]
pub struct Environment {
    pub docker_host: Option<String>,
    pub docker_tls_verify: bool,
    pub docker_cert_path: Option<String>,
    pub home: Option<String>,
    pub xdg_runtime_dir: Option<String>,
    pub uid: Option<u32>,
    pub windows: bool,
}

pub const DEFAULT_PIPE: &str = r"\\.\pipe\docker_engine";

/// Parse a `DOCKER_HOST`-style URL.
pub fn parse_host(url: &str, tls: Option<TlsFiles>) -> Result<Endpoint> {
    let url = url.trim();
    if let Some(p) = url.strip_prefix("unix://") {
        return Ok(Endpoint::Unix {
            path: p.to_string(),
        });
    }
    if let Some(p) = url.strip_prefix("npipe://") {
        // npipe:////./pipe/docker_engine → \\.\pipe\docker_engine
        let p = p.trim_start_matches('/').replace('/', "\\");
        return Ok(Endpoint::NamedPipe {
            path: format!(r"\\{p}"),
        });
    }
    let (rest, https) = if let Some(r) = url.strip_prefix("tcp://") {
        (r, false)
    } else if let Some(r) = url.strip_prefix("https://") {
        (r, true)
    } else if let Some(r) = url.strip_prefix("http://") {
        (r, false)
    } else if url.starts_with("ssh://") {
        return err("SSH connections are not supported yet; use a socket or TCP");
    } else if url.starts_with('/') {
        return Ok(Endpoint::Unix {
            path: url.to_string(),
        });
    } else {
        return err(format!("Unsupported Docker host `{url}`"));
    };
    let rest = rest.trim_end_matches('/');
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) if !h.ends_with(']') || h.starts_with('[') => match p.parse::<u16>() {
            Ok(port) => (h.trim_matches(['[', ']']).to_string(), port),
            Err(_) => return err(format!("Invalid port in `{url}`")),
        },
        _ => (
            rest.to_string(),
            if tls.is_some() || https { 2376 } else { 2375 },
        ),
    };
    if host.is_empty() {
        return err(format!("Missing host in `{url}`"));
    }
    let tls = tls.or_else(|| {
        https.then_some(TlsFiles {
            ca: None,
            cert: None,
            key: None,
            verify: true,
        })
    });
    Ok(Endpoint::Tcp { host, port, tls })
}

fn tls_files(dir: &str, verify: bool) -> TlsFiles {
    let dir = dir.trim_end_matches(['/', '\\']);
    let file = |n: &str| (!dir.is_empty()).then(|| format!("{dir}/{n}"));
    TlsFiles {
        ca: file("ca.pem"),
        cert: file("cert.pem"),
        key: file("key.pem"),
        verify,
    }
}

/// Socket paths tried for `auto` on Unix-like systems, most common first.
pub fn default_sockets(env: &Environment) -> Vec<String> {
    let mut out = vec!["/var/run/docker.sock".to_string()];
    if let Some(home) = &env.home {
        out.push(format!("{home}/.docker/run/docker.sock"));
        out.push(format!("{home}/.docker/desktop/docker.sock"));
        out.push(format!("{home}/.colima/default/docker.sock"));
        out.push(format!("{home}/.orbstack/run/docker.sock"));
    }
    if let Some(run) = &env.xdg_runtime_dir {
        out.push(format!("{run}/docker.sock"));
        out.push(format!("{run}/podman/podman.sock"));
    } else if let Some(uid) = env.uid {
        out.push(format!("/run/user/{uid}/docker.sock"));
        out.push(format!("/run/user/{uid}/podman/podman.sock"));
    }
    out
}

/// Pick the endpoint for the settings. `exists` tells whether a socket file exists.
pub fn resolve(
    settings: &DockerSettings,
    env: &Environment,
    exists: &dyn Fn(&str) -> bool,
) -> Result<Endpoint> {
    match settings.connection.as_str() {
        "unix" => {
            let path = settings.socket_path.trim();
            if path.is_empty() {
                Ok(Endpoint::Unix {
                    path: default_sockets(env)
                        .into_iter()
                        .find(|p| exists(p))
                        .unwrap_or_else(|| "/var/run/docker.sock".into()),
                })
            } else {
                Ok(Endpoint::Unix {
                    path: path.trim_start_matches("unix://").to_string(),
                })
            }
        }
        "pipe" => Ok(Endpoint::NamedPipe {
            path: match settings.socket_path.trim() {
                "" => DEFAULT_PIPE.to_string(),
                p => p.to_string(),
            },
        }),
        "tcp" => {
            let url = match settings.tcp_url.trim() {
                "" => "tcp://localhost:2375",
                u => u,
            };
            let tls = settings
                .tls
                .then(|| tls_files(settings.cert_path.trim(), true));
            parse_host(url, tls)
        }
        _ => {
            if let Some(host) = env.docker_host.as_deref().filter(|h| !h.trim().is_empty()) {
                let tls = env.docker_tls_verify.then(|| {
                    let dir = env
                        .docker_cert_path
                        .clone()
                        .or_else(|| env.home.as_ref().map(|h| format!("{h}/.docker")))
                        .unwrap_or_default();
                    tls_files(&dir, true)
                });
                return parse_host(host, tls);
            }
            if env.windows {
                return Ok(Endpoint::NamedPipe {
                    path: DEFAULT_PIPE.to_string(),
                });
            }
            let candidates = default_sockets(env);
            Ok(Endpoint::Unix {
                path: candidates
                    .iter()
                    .find(|p| exists(p))
                    .cloned()
                    .unwrap_or_else(|| candidates[0].clone()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> Environment {
        Environment {
            home: Some("/home/dev".into()),
            uid: Some(1000),
            ..Default::default()
        }
    }

    #[test]
    fn parses_hosts() {
        assert_eq!(
            parse_host("unix:///var/run/docker.sock", None).unwrap(),
            Endpoint::Unix {
                path: "/var/run/docker.sock".into()
            }
        );
        assert_eq!(
            parse_host("npipe:////./pipe/docker_engine", None).unwrap(),
            Endpoint::NamedPipe {
                path: DEFAULT_PIPE.into()
            }
        );
        assert_eq!(
            parse_host("tcp://10.0.0.5:2375", None).unwrap(),
            Endpoint::Tcp {
                host: "10.0.0.5".into(),
                port: 2375,
                tls: None
            }
        );
        let Endpoint::Tcp { port, tls, .. } =
            parse_host("https://docker.example.com", None).unwrap()
        else {
            panic!()
        };
        assert_eq!((port, tls.unwrap().verify), (2376, true));
        assert!(parse_host("ssh://dev@host", None).is_err());
        assert!(parse_host("tcp://host:x", None).is_err());
    }

    #[test]
    fn auto_prefers_docker_host_then_existing_sockets() {
        let mut e = env();
        let none = |_: &str| false;
        assert_eq!(
            resolve(&DockerSettings::default(), &e, &none)
                .unwrap()
                .label(),
            "unix:///var/run/docker.sock"
        );
        let desktop = |p: &str| p == "/home/dev/.docker/run/docker.sock";
        assert_eq!(
            resolve(&DockerSettings::default(), &e, &desktop)
                .unwrap()
                .label(),
            "unix:///home/dev/.docker/run/docker.sock"
        );
        e.docker_host = Some("tcp://192.168.1.4:2376".into());
        e.docker_tls_verify = true;
        let ep = resolve(&DockerSettings::default(), &e, &none).unwrap();
        let Endpoint::Tcp { tls: Some(tls), .. } = &ep else {
            panic!("{ep:?}")
        };
        assert_eq!(tls.ca.as_deref(), Some("/home/dev/.docker/ca.pem"));
        assert_eq!(
            ep.cli_env()[0],
            ("DOCKER_HOST".into(), "tcp://192.168.1.4:2376".into())
        );
        assert!(ep
            .cli_env()
            .iter()
            .any(|(k, v)| k == "DOCKER_CERT_PATH" && v == "/home/dev/.docker"));
        let win = Environment {
            windows: true,
            ..Default::default()
        };
        assert_eq!(
            resolve(&DockerSettings::default(), &win, &none)
                .unwrap()
                .label(),
            "npipe:////./pipe/docker_engine"
        );
    }

    #[test]
    fn explicit_settings() {
        let s = DockerSettings {
            connection: "tcp".into(),
            tcp_url: "tcp://build:2376".into(),
            tls: true,
            cert_path: "/certs/".into(),
            ..Default::default()
        };
        let ep = resolve(&s, &env(), &|_| false).unwrap();
        assert_eq!(ep.label(), "https://build:2376");
        let Endpoint::Tcp { tls: Some(tls), .. } = ep else {
            panic!()
        };
        assert_eq!(tls.key.as_deref(), Some("/certs/key.pem"));
        let s = DockerSettings {
            connection: "unix".into(),
            socket_path: "/run/podman.sock".into(),
            ..Default::default()
        };
        assert_eq!(
            resolve(&s, &env(), &|_| false).unwrap().label(),
            "unix:///run/podman.sock"
        );
    }
}
