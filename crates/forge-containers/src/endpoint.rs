//! Where the Docker daemon listens: Settings | Build, Execution, Deployment | Docker
//! (`docker.connection`, `docker.socketPath`, `docker.tcp.*`, `docker.ssh.*`,
//! `docker.context`), else `DOCKER_HOST`, else the current Docker context
//! (`~/.docker/contexts`), else the platform default (named pipe on Windows;
//! `/var/run/docker.sock`, Docker Desktop's `~/.docker/run/docker.sock`, rootless
//! `$XDG_RUNTIME_DIR/docker.sock` or Podman elsewhere). `ssh://user@host` daemons are reached
//! through a forwarded Unix socket; the Docker daemon of a remote project's host through the
//! project's own SSH connection ([`Endpoint::ProjectHost`]).

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
    /// `ssh://user@host:port`: the daemon's Unix socket on the host, through SSH.
    #[serde(rename_all = "camelCase")]
    Ssh {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        user: Option<String>,
        /// Host name, address or a `Host` alias of `~/.ssh/config`.
        host: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        port: Option<u16>,
        /// The Docker socket on the host.
        socket: String,
    },
    /// The daemon on a remote project's host, reached through the project's SSH
    /// connection; CLI commands (compose, exec) run on that host.
    #[serde(rename_all = "camelCase")]
    ProjectHost {
        /// `user@host` of the project.
        via: String,
        /// The Docker socket on the host.
        socket: String,
    },
}

/// The Docker socket on SSH hosts unless the URL or the settings name another.
pub const DEFAULT_REMOTE_SOCKET: &str = "/var/run/docker.sock";

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
            Endpoint::Ssh { socket, .. } if socket != DEFAULT_REMOTE_SOCKET => {
                format!("{}{socket}", self.docker_host())
            }
            Endpoint::Ssh { .. } => self.docker_host(),
            Endpoint::ProjectHost { via, socket } => format!("ssh://{via}{socket}"),
        }
    }

    /// `-H` value for the docker CLI (exec, compose), so it talks to the same daemon.
    pub fn docker_host(&self) -> String {
        match self {
            // The CLI runs on the project's host, next to the daemon.
            Endpoint::ProjectHost { socket, .. } => format!("unix://{socket}"),
            Endpoint::Unix { path } => format!("unix://{path}"),
            Endpoint::NamedPipe { path } => format!("npipe://{}", path.replace('\\', "/")),
            Endpoint::Tcp { host, port, .. } => format!("tcp://{host}:{port}"),
            // The docker CLI tunnels `ssh://` itself (`docker system dial-stdio`).
            Endpoint::Ssh {
                user, host, port, ..
            } => {
                let user = user.as_ref().map(|u| format!("{u}@")).unwrap_or_default();
                let host = if host.contains(':') {
                    format!("[{host}]")
                } else {
                    host.clone()
                };
                match port {
                    Some(p) => format!("ssh://{user}{host}:{p}"),
                    None => format!("ssh://{user}{host}"),
                }
            }
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
    /// `ssh://user@host[:port]` for `ssh` connections.
    #[serde(default)]
    pub ssh_url: String,
    /// The Docker socket on the SSH host (empty: `/var/run/docker.sock`).
    #[serde(default)]
    pub ssh_socket: String,
    /// Docker context name for `context` connections.
    #[serde(default)]
    pub context: String,
}

/// A Docker CLI context (`docker context ls`), from
/// `~/.docker/contexts/meta/<sha256 of the name>/meta.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerContext {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `DOCKER_HOST`-style endpoint (`unix://…`, `tcp://…`, `ssh://…`).
    pub host: String,
    #[serde(default)]
    pub skip_tls_verify: bool,
    /// Directory with the context's `ca.pem`, `cert.pem` and `key.pem`, when it has TLS
    /// material (`~/.docker/contexts/tls/<id>/docker`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_dir: Option<String>,
    /// The Docker CLI's current context (`DOCKER_CONTEXT` or `currentContext`).
    #[serde(default)]
    pub current: bool,
}

/// A context's `meta.json`; `None` when it has no Docker endpoint.
pub fn parse_context_meta(json: &str) -> Option<DockerContext> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let name = v.get("Name")?.as_str()?.to_string();
    let docker = v.pointer("/Endpoints/docker")?;
    Some(DockerContext {
        name,
        description: v
            .pointer("/Metadata/Description")
            .and_then(|d| d.as_str())
            .filter(|d| !d.is_empty())
            .map(str::to_string),
        host: docker.get("Host")?.as_str()?.to_string(),
        skip_tls_verify: docker
            .get("SkipTLSVerify")
            .and_then(|b| b.as_bool())
            .unwrap_or(false),
        tls_dir: None,
        current: false,
    })
}

/// `currentContext` of `~/.docker/config.json`.
pub fn current_context_of_config(json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()?
        .get("currentContext")?
        .as_str()
        .filter(|c| !c.is_empty())
        .map(str::to_string)
}

/// The endpoint of a context.
pub fn context_endpoint(ctx: &DockerContext) -> Result<Endpoint> {
    let tls = ctx.tls_dir.as_deref().map(|dir| TlsFiles {
        verify: !ctx.skip_tls_verify,
        ..tls_files(dir, true)
    });
    let is_tcp = ctx.host.starts_with("tcp://") || ctx.host.starts_with("https://");
    parse_host(&ctx.host, tls.filter(|_| is_tcp))
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
    /// Docker CLI contexts (`~/.docker/contexts`), the current one marked.
    pub contexts: Vec<DockerContext>,
}

impl Environment {
    /// The current Docker context unless it is `default`.
    pub fn current_context(&self) -> Option<&DockerContext> {
        self.contexts
            .iter()
            .find(|c| c.current && c.name != "default")
    }
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
        return parse_ssh(url, "");
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

/// `ssh://[user@]host[:port][/socket/path]`; the socket defaults to `socket` (else
/// `/var/run/docker.sock`).
pub fn parse_ssh(url: &str, socket: &str) -> Result<Endpoint> {
    let rest = url.trim().trim_start_matches("ssh://");
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], Some(&rest[i..])),
        None => (rest, None),
    };
    let Some((user, host, port)) = forge_remote::config::parse_destination(authority) else {
        return err(format!("Missing host in `{url}`"));
    };
    if host.is_empty() {
        return err(format!("Missing host in `{url}`"));
    }
    if authority.contains(':') && !authority.ends_with(']') && port.is_none() {
        return err(format!("Invalid port in `{url}`"));
    }
    let socket = path
        .filter(|p| p.len() > 1)
        .map(str::to_string)
        .or_else(|| Some(socket.trim().to_string()).filter(|s| !s.is_empty()))
        .unwrap_or_else(|| DEFAULT_REMOTE_SOCKET.to_string());
    Ok(Endpoint::Ssh {
        user,
        host,
        port,
        socket,
    })
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
        "ssh" => {
            if settings.ssh_url.trim().is_empty() {
                return err("Enter the SSH host of the Docker daemon (ssh://user@host)");
            }
            parse_ssh(&settings.ssh_url, &settings.ssh_socket)
        }
        "context" => {
            let name = settings.context.trim();
            match env.contexts.iter().find(|c| c.name == name) {
                Some(ctx) => context_endpoint(ctx),
                None if name.is_empty() || name == "default" => resolve(
                    &DockerSettings::default(),
                    &Environment {
                        contexts: Vec::new(),
                        ..env.clone()
                    },
                    exists,
                ),
                None => err(format!("Docker context `{name}` not found")),
            }
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
            if let Some(ctx) = env.current_context() {
                return context_endpoint(ctx);
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
        assert_eq!(
            parse_host("ssh://dev@build.example.com", None).unwrap(),
            Endpoint::Ssh {
                user: Some("dev".into()),
                host: "build.example.com".into(),
                port: None,
                socket: DEFAULT_REMOTE_SOCKET.into()
            }
        );
        let ep = parse_ssh("ssh://dev@10.0.0.7:2222/run/user/1000/docker.sock", "").unwrap();
        assert_eq!(
            ep,
            Endpoint::Ssh {
                user: Some("dev".into()),
                host: "10.0.0.7".into(),
                port: Some(2222),
                socket: "/run/user/1000/docker.sock".into()
            }
        );
        assert_eq!(
            ep.label(),
            "ssh://dev@10.0.0.7:2222/run/user/1000/docker.sock"
        );
        assert_eq!(ep.docker_host(), "ssh://dev@10.0.0.7:2222");
        assert_eq!(
            ep.cli_env(),
            [(
                "DOCKER_HOST".to_string(),
                "ssh://dev@10.0.0.7:2222".to_string()
            )]
        );
        let alias = parse_ssh("ssh://buildbox", "/var/run/podman.sock").unwrap();
        assert_eq!(alias.label(), "ssh://buildbox/var/run/podman.sock");
        assert!(parse_ssh("ssh://", "").is_err());
        assert!(parse_ssh("ssh://dev@host:x", "").is_err());
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
        let s = DockerSettings {
            connection: "ssh".into(),
            ssh_url: "ssh://deploy@build".into(),
            ..Default::default()
        };
        assert_eq!(
            resolve(&s, &env(), &|_| false).unwrap().label(),
            "ssh://deploy@build"
        );
        assert!(resolve(
            &DockerSettings {
                connection: "ssh".into(),
                ..Default::default()
            },
            &env(),
            &|_| false
        )
        .is_err());
    }

    #[test]
    fn docker_contexts() {
        let meta = r#"{"Name":"buildbox","Metadata":{"Description":"CI builder"},"Endpoints":{"docker":{"Host":"ssh://ci@buildbox","SkipTLSVerify":false}}}"#;
        let ctx = parse_context_meta(meta).unwrap();
        assert_eq!(ctx.name, "buildbox");
        assert_eq!(ctx.description.as_deref(), Some("CI builder"));
        assert_eq!(ctx.host, "ssh://ci@buildbox");
        assert!(parse_context_meta(r#"{"Name":"k8s","Endpoints":{}}"#).is_none());
        assert_eq!(
            current_context_of_config(r#"{"auths":{},"currentContext":"buildbox"}"#).as_deref(),
            Some("buildbox")
        );
        assert_eq!(current_context_of_config("{}"), None);

        let tls = DockerContext {
            name: "prod".into(),
            description: None,
            host: "tcp://10.1.2.3:2376".into(),
            skip_tls_verify: false,
            tls_dir: Some("/home/dev/.docker/contexts/tls/abc/docker".into()),
            current: false,
        };
        let Endpoint::Tcp {
            tls: Some(files), ..
        } = context_endpoint(&tls).unwrap()
        else {
            panic!()
        };
        assert_eq!(
            files.ca.as_deref(),
            Some("/home/dev/.docker/contexts/tls/abc/docker/ca.pem")
        );
        assert!(files.verify);

        // The current context is used by `auto` unless DOCKER_HOST is set.
        let mut e = env();
        e.contexts = vec![DockerContext {
            current: true,
            ..ctx.clone()
        }];
        let auto = resolve(&DockerSettings::default(), &e, &|_| false).unwrap();
        assert_eq!(auto.label(), "ssh://ci@buildbox");
        e.docker_host = Some("unix:///x.sock".into());
        assert_eq!(
            resolve(&DockerSettings::default(), &e, &|_| false)
                .unwrap()
                .label(),
            "unix:///x.sock"
        );
        // An explicitly chosen context.
        let s = DockerSettings {
            connection: "context".into(),
            context: "buildbox".into(),
            ..Default::default()
        };
        assert_eq!(
            resolve(&s, &e, &|_| false).unwrap().label(),
            "ssh://ci@buildbox"
        );
        let s = DockerSettings {
            connection: "context".into(),
            context: "missing".into(),
            ..Default::default()
        };
        assert!(resolve(&s, &e, &|_| false).is_err());
    }
}
