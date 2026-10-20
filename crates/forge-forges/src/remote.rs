//! Detect the code host and project of a repository from its git remotes
//! (`https://`, `ssh://`, scp-like `git@host:path`, nested GitLab groups, self-hosted
//! servers known through accounts).

use crate::accounts::Account;
use crate::model::{HostKind, RepoRef};

/// A parsed remote URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteUrl {
    pub host: String,
    pub port: Option<u16>,
    /// `http` for plain-http remotes; ssh/git remotes use `https` for the web UI.
    pub scheme: &'static str,
    /// SSH remotes (the port belongs to SSH, not to the web server).
    pub ssh: bool,
    /// Repository path without leading slash and `.git` suffix.
    pub path: String,
}

pub fn parse_remote_url(url: &str) -> Option<RemoteUrl> {
    let url = url.trim();
    let (scheme, rest, ssh) = if let Some(r) = url.strip_prefix("https://") {
        ("https", r, false)
    } else if let Some(r) = url.strip_prefix("http://") {
        ("http", r, false)
    } else if let Some(r) = url.strip_prefix("ssh://") {
        ("https", r, true)
    } else if let Some(r) = url.strip_prefix("git+ssh://") {
        ("https", r, true)
    } else if let Some(r) = url.strip_prefix("git://") {
        ("https", r, true)
    } else if !url.contains("://") && url.contains(':') {
        // scp-like: [user@]host:path
        let (authority, path) = url.split_once(':')?;
        let host = authority.rsplit('@').next()?;
        return finish(host, None, "https", true, path);
    } else {
        return None;
    };
    let (authority, path) = rest.split_once('/')?;
    let host_port = authority.rsplit('@').next()?;
    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().ok()),
        None => (host_port, None),
    };
    finish(host, port, scheme, ssh, path)
}

fn finish(
    host: &str,
    port: Option<u16>,
    scheme: &'static str,
    ssh: bool,
    path: &str,
) -> Option<RemoteUrl> {
    let path = path
        .trim_matches('/')
        .trim_end_matches(".git")
        .trim_end_matches('/');
    if host.is_empty() || !path.contains('/') {
        return None;
    }
    Some(RemoteUrl {
        host: host.to_lowercase(),
        port,
        scheme,
        ssh,
        path: path.to_string(),
    })
}

/// Well-known public hosts and host-name hints for self-hosted servers.
pub fn guess_kind(host: &str) -> Option<HostKind> {
    let host = host.to_lowercase();
    match host.as_str() {
        "github.com" | "www.github.com" => return Some(HostKind::Github),
        "gitlab.com"
        | "www.gitlab.com"
        | "salsa.debian.org"
        | "invent.kde.org"
        | "gitlab.gnome.org"
        | "gitlab.freedesktop.org" => return Some(HostKind::Gitlab),
        "codeberg.org" | "gitea.com" | "next.forgejo.org" | "code.forgejo.org" => {
            return Some(HostKind::Forgejo)
        }
        _ => {}
    }
    if host.contains("gitlab") {
        Some(HostKind::Gitlab)
    } else if host.contains("forgejo") || host.contains("gitea") || host.contains("codeberg") {
        Some(HostKind::Forgejo)
    } else if host.contains("github") {
        Some(HostKind::Github)
    } else {
        None
    }
}

/// Split a server URL (`https://git.example.com:8443/gitlab`) into host, port and path.
fn server_parts(server: &str) -> Option<(String, Option<u16>, String)> {
    let rest = server
        .trim()
        .strip_prefix("https://")
        .or_else(|| server.trim().strip_prefix("http://"))
        .unwrap_or(server.trim());
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().ok()),
        None => (authority, None),
    };
    Some((
        host.to_lowercase(),
        port,
        path.trim_matches('/').to_string(),
    ))
}

/// Normalize a server URL: scheme added, no trailing slash.
pub fn normalize_server(server: &str) -> String {
    let s = server.trim().trim_end_matches('/');
    if s.starts_with("http://") || s.starts_with("https://") {
        s.to_string()
    } else {
        format!("https://{s}")
    }
}

/// Resolve one remote. Accounts win (they know self-hosted servers, ports and relative URL
/// roots), then well-known hosts and host-name hints.
pub fn resolve_remote(name: &str, url: &str, accounts: &[Account]) -> Option<RepoRef> {
    let remote = parse_remote_url(url)?;
    for acc in accounts {
        let Some((host, port, prefix)) = server_parts(&acc.server) else {
            continue;
        };
        if host != remote.host || (!remote.ssh && port.is_some() && port != remote.port) {
            continue;
        }
        let path = if prefix.is_empty() {
            remote.path.as_str()
        } else {
            match remote.path.strip_prefix(&prefix) {
                Some(p) => p.trim_start_matches('/'),
                // SSH remotes have no relative URL root.
                None if remote.ssh => remote.path.as_str(),
                None => continue,
            }
        };
        if !path.contains('/') {
            continue;
        }
        return Some(RepoRef {
            kind: acc.kind,
            server: normalize_server(&acc.server),
            path: path.to_string(),
            remote: name.to_string(),
        });
    }
    let kind = guess_kind(&remote.host)?;
    let port = match (remote.ssh, remote.port) {
        (false, Some(p)) => format!(":{p}"),
        _ => String::new(),
    };
    Some(RepoRef {
        kind,
        server: format!("{}://{}{}", remote.scheme, remote.host, port),
        path: remote.path,
        remote: name.to_string(),
    })
}

/// Parse `git remote -v` output into `(name, fetch url)` pairs.
pub fn parse_remote_v(out: &str) -> Vec<(String, String)> {
    let mut seen = Vec::<(String, String)>::new();
    for line in out.lines() {
        let mut parts = line.split_whitespace();
        let (Some(name), Some(url)) = (parts.next(), parts.next()) else {
            continue;
        };
        if parts.next() == Some("(push)") && seen.iter().any(|(n, _)| n == name) {
            continue;
        }
        if !seen.iter().any(|(n, _)| n == name) {
            seen.push((name.to_string(), url.to_string()));
        }
    }
    seen
}

/// All code-host repositories among the remotes, `origin` first, then `upstream`.
pub fn detect(remotes: &[(String, String)], accounts: &[Account]) -> Vec<RepoRef> {
    let rank = |n: &str| match n {
        "origin" => 0,
        "upstream" => 1,
        _ => 2,
    };
    let mut refs: Vec<RepoRef> = remotes
        .iter()
        .filter_map(|(n, u)| resolve_remote(n, u, accounts))
        .collect();
    refs.sort_by_key(|r| rank(&r.remote));
    refs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acc(kind: HostKind, server: &str) -> Account {
        Account {
            id: "a".into(),
            kind,
            server: server.into(),
            username: None,
            api_url: None,
        }
    }

    #[test]
    fn parses_remote_forms() {
        let r = parse_remote_url("https://gitlab.com/acme/backend/api.git").unwrap();
        assert_eq!(
            (r.host.as_str(), r.path.as_str(), r.ssh),
            ("gitlab.com", "acme/backend/api", false)
        );
        let r = parse_remote_url("git@github.com:Reeperk/Forge.git").unwrap();
        assert_eq!(
            (r.host.as_str(), r.path.as_str(), r.ssh),
            ("github.com", "Reeperk/Forge", true)
        );
        let r = parse_remote_url("ssh://git@git.acme.dev:2222/acme/acme-shop.git").unwrap();
        assert_eq!(
            (r.host.as_str(), r.port, r.path.as_str()),
            ("git.acme.dev", Some(2222), "acme/acme-shop")
        );
        let r = parse_remote_url("https://oauth2:secret@code.example.org:8443/a/b/").unwrap();
        assert_eq!(
            (r.host.as_str(), r.port, r.path.as_str()),
            ("code.example.org", Some(8443), "a/b")
        );
        assert!(parse_remote_url("/local/path/repo").is_none());
        assert!(parse_remote_url("https://github.com/only-owner").is_none());
    }

    #[test]
    fn detects_public_hosts() {
        let r = resolve_remote("origin", "git@gitlab.com:group/sub/proj.git", &[]).unwrap();
        assert_eq!(r.kind, HostKind::Gitlab);
        assert_eq!(r.server, "https://gitlab.com");
        assert_eq!(r.path, "group/sub/proj");
        assert_eq!(r.owner_repo(), ("group/sub", "proj"));
        let r = resolve_remote("origin", "https://codeberg.org/forgejo/forgejo", &[]).unwrap();
        assert_eq!(r.kind, HostKind::Forgejo);
        assert!(resolve_remote("origin", "https://git.example.com/a/b.git", &[]).is_none());
    }

    #[test]
    fn accounts_resolve_self_hosted_servers() {
        let accounts = [
            acc(HostKind::Forgejo, "https://git.acme.dev"),
            acc(HostKind::Gitlab, "https://example.com/gitlab/"),
        ];
        let r = resolve_remote(
            "origin",
            "ssh://git@git.acme.dev:2222/acme/acme-shop.git",
            &accounts,
        )
        .unwrap();
        assert_eq!(
            (r.kind, r.server.as_str(), r.path.as_str()),
            (HostKind::Forgejo, "https://git.acme.dev", "acme/acme-shop")
        );
        let r = resolve_remote(
            "upstream",
            "https://example.com/gitlab/team/app.git",
            &accounts,
        )
        .unwrap();
        assert_eq!(
            (r.kind, r.server.as_str(), r.path.as_str()),
            (HostKind::Gitlab, "https://example.com/gitlab", "team/app")
        );
        // HTTP port is part of the server identity.
        let r = resolve_remote(
            "origin",
            "http://localhost:3000/me/repo.git",
            &[acc(HostKind::Forgejo, "http://localhost:3000")],
        )
        .unwrap();
        assert_eq!(r.server, "http://localhost:3000");
    }

    #[test]
    fn parses_git_remote_v_and_orders_origin_first() {
        let out = "upstream\thttps://github.com/up/repo.git (fetch)\nupstream\thttps://github.com/up/repo.git (push)\norigin\tgit@github.com:me/repo.git (fetch)\norigin\tgit@github.com:me/repo.git (push)\nlocal\t/tmp/x (fetch)\n";
        let remotes = parse_remote_v(out);
        assert_eq!(remotes.len(), 3);
        let refs = detect(&remotes, &[]);
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].remote, "origin");
        assert_eq!(refs[0].path, "me/repo");
        assert_eq!(refs[1].path, "up/repo");
    }
}
