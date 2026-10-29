//! Get from VCS: the `git clone` / `git ls-remote` invocations, their credentials and the
//! progress lines git prints while cloning.
//!
//! HTTPS credentials (a code host token) are handed to git for one invocation only: a
//! `credential.helper` given with `-c` prints them from environment variables, so the
//! token is neither on the command line nor written to any git config or credential store.
//! SSH URLs authenticate with the user's keys and SSH agent as usual.

use serde::{Deserialize, Serialize};

/// Environment variables the per-invocation credential helper reads.
pub const USERNAME_ENV: &str = "FORGE_GIT_USERNAME";
pub const PASSWORD_ENV: &str = "FORGE_GIT_PASSWORD";

/// The inline helper: answers `get` with the variables above and ignores `store`/`erase`.
pub const CREDENTIAL_HELPER: &str = "!f() { test \"$1\" = get || return 0; echo \"username=${FORGE_GIT_USERNAME}\"; echo \"password=${FORGE_GIT_PASSWORD}\"; }; f";

/// A username/token pair for an HTTPS remote.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct HttpsCredential {
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for HttpsCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpsCredential")
            .field("username", &self.username)
            .finish_non_exhaustive()
    }
}

/// A git invocation without the program: arguments plus extra environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitInvocation {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// Whether `url` is an HTTP(S) URL (the only kind a token helps with).
pub fn is_http_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

/// `host[:port]` of an HTTP(S) URL, lower-cased, without user info.
pub fn http_host(url: &str) -> Option<String> {
    let url = url.trim();
    let rest = url
        .split_once("://")
        .filter(|(scheme, _)| {
            scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http")
        })?
        .1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// Check a repository URL: not empty, no option-like value, no line breaks.
pub fn validate_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("Repository URL is empty".into());
    }
    if url.starts_with('-') || url.contains(['\n', '\r', '\0']) {
        return Err(format!("Invalid repository URL: {url:?}"));
    }
    Ok(url.to_string())
}

/// The directory name git would pick for `url`: the last path segment without `.git`
/// (`git@github.com:acme/shop.git` → `shop`).
pub fn repo_name_from_url(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches(['/', '\\']);
    let url = url.strip_suffix(".git").unwrap_or(url);
    let url = url.trim_end_matches(['/', '\\']);
    let last = url.rsplit(['/', '\\', ':']).next()?;
    let name: String = last
        .chars()
        .filter(|c| !matches!(c, '<' | '>' | '"' | '|' | '?' | '*'))
        .collect();
    let name = name.trim().to_string();
    (!name.is_empty() && name != "." && name != "..").then_some(name)
}

fn with_credential(
    args: &mut Vec<String>,
    env: &mut Vec<(String, String)>,
    credential: Option<&HttpsCredential>,
) {
    if let Some(c) = credential {
        // An empty value first resets the configured helpers, so no store keeps the token.
        args.extend([
            "-c".into(),
            "credential.helper=".into(),
            "-c".into(),
            format!("credential.helper={CREDENTIAL_HELPER}"),
        ]);
        env.push((USERNAME_ENV.into(), c.username.clone()));
        env.push((PASSWORD_ENV.into(), c.password.clone()));
    }
}

/// Environment of network invocations: never prompt in a terminal.
fn base_env(ssh_batch: bool) -> Vec<(String, String)> {
    let mut env = vec![
        ("LC_ALL".to_string(), "C".to_string()),
        ("GIT_TERMINAL_PROMPT".to_string(), "0".to_string()),
    ];
    if ssh_batch {
        env.push(("GIT_SSH_COMMAND".into(), "ssh -o BatchMode=yes".into()));
    }
    env
}

/// `git clone --progress -- <url> <directory>`. `ssh_batch` makes SSH fail instead of
/// asking for a password (set it unless the user configured `GIT_SSH_COMMAND`).
pub fn clone_invocation(
    url: &str,
    directory: &str,
    credential: Option<&HttpsCredential>,
    ssh_batch: bool,
) -> Result<GitInvocation, String> {
    let url = validate_url(url)?;
    let directory = directory.trim();
    if directory.is_empty() {
        return Err("Directory is empty".into());
    }
    if directory.contains(['\n', '\r', '\0']) {
        return Err(format!("Invalid directory: {directory:?}"));
    }
    let mut args = Vec::new();
    let mut env = base_env(ssh_batch);
    with_credential(
        &mut args,
        &mut env,
        credential.filter(|_| is_http_url(&url)),
    );
    args.extend([
        "clone".into(),
        "--progress".into(),
        "--".into(),
        url,
        directory.to_string(),
    ]);
    Ok(GitInvocation { args, env })
}

/// `git ls-remote --heads <url>` (the Test button).
pub fn ls_remote_invocation(
    url: &str,
    credential: Option<&HttpsCredential>,
    ssh_batch: bool,
) -> Result<GitInvocation, String> {
    let url = validate_url(url)?;
    let mut args = Vec::new();
    let mut env = base_env(ssh_batch);
    with_credential(
        &mut args,
        &mut env,
        credential.filter(|_| is_http_url(&url)),
    );
    args.extend(["ls-remote".into(), "--heads".into(), url]);
    Ok(GitInvocation { args, env })
}

/// A ref from `git ls-remote`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRef {
    pub sha: String,
    pub name: String,
}

pub fn parse_ls_remote(out: &str) -> Vec<RemoteRef> {
    out.lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(sha, name)| RemoteRef {
            sha: sha.trim().to_string(),
            name: name.trim().to_string(),
        })
        .filter(|r| !r.sha.is_empty() && !r.name.is_empty())
        .collect()
}

/// One progress report while cloning.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneProgress {
    /// The phase as git names it: `Receiving objects`, `Resolving deltas`, …
    pub phase: String,
    /// Percent of the phase, when git reports one.
    pub percent: Option<u8>,
    /// The whole cloning job, 0.0–1.0 (phases weighted by their usual duration).
    pub fraction: Option<f64>,
    /// The line itself, for the progress text.
    pub text: String,
}

/// Parse a progress line (`remote: Counting objects:  45% (9/20)`, `Receiving objects:
/// 12% (120/1000), 1.20 MiB | 2.00 MiB/s`, `Cloning into 'shop'...`). Lines are what git
/// separates with `\r` or `\n`.
pub fn parse_progress(line: &str) -> Option<CloneProgress> {
    let text = line.trim();
    if text.is_empty() {
        return None;
    }
    let body = text.strip_prefix("remote: ").unwrap_or(text).trim();
    if body.starts_with("Cloning into") {
        return Some(CloneProgress {
            phase: "Cloning".into(),
            percent: None,
            fraction: Some(0.0),
            text: text.to_string(),
        });
    }
    let (phase, rest) = body.split_once(':')?;
    let phase = phase.trim();
    if phase.is_empty() || phase.contains(['\'', '"']) {
        return None;
    }
    let percent = rest
        .split_whitespace()
        .find_map(|w| w.strip_suffix('%'))
        .and_then(|p| p.parse::<u8>().ok())
        .map(|p| p.min(100));
    let (start, end) = phase_span(phase)?;
    let fraction = percent.map(|p| start + (end - start) * f64::from(p) / 100.0);
    Some(CloneProgress {
        phase: phase.to_string(),
        percent,
        fraction: fraction.or(Some(start)),
        text: text.to_string(),
    })
}

/// Where a phase lies within the whole clone (start, end).
fn phase_span(phase: &str) -> Option<(f64, f64)> {
    Some(match phase {
        "Enumerating objects" => (0.0, 0.02),
        "Counting objects" => (0.02, 0.06),
        "Compressing objects" => (0.06, 0.1),
        "Receiving objects" => (0.1, 0.8),
        "Resolving deltas" => (0.8, 0.95),
        "Updating files" | "Checking out files" => (0.95, 1.0),
        "Filtering content" => (0.95, 1.0),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_from_urls() {
        for (url, name) in [
            ("git@github.com:acme/shop.git", "shop"),
            ("https://github.com/acme/shop", "shop"),
            ("https://gitlab.com/group/sub/project.git/", "project"),
            ("ssh://git@host:2222/srv/repo.git", "repo"),
            ("/srv/git/shop.git", "shop"),
            ("C:\\repos\\shop.git", "shop"),
            ("file:///x/y.git/", "y"),
        ] {
            assert_eq!(repo_name_from_url(url).as_deref(), Some(name), "{url}");
        }
        assert_eq!(repo_name_from_url(""), None);
        assert_eq!(repo_name_from_url("https://"), None);
    }

    #[test]
    fn http_hosts() {
        assert_eq!(
            http_host("https://user@GitHub.com/acme/shop.git").as_deref(),
            Some("github.com")
        );
        assert_eq!(
            http_host("http://git.acme.dev:3000/x").as_deref(),
            Some("git.acme.dev:3000")
        );
        assert_eq!(http_host("git@github.com:acme/shop.git"), None);
        assert_eq!(http_host("ssh://git@host/x"), None);
    }

    #[test]
    fn clone_passes_tokens_only_to_https_and_only_by_environment() {
        let cred = HttpsCredential {
            username: "x-access-token".into(),
            password: "ghp_secret".into(),
        };
        let inv = clone_invocation(
            "https://github.com/acme/shop.git",
            "/home/m/src/shop",
            Some(&cred),
            true,
        )
        .unwrap();
        assert_eq!(&inv.args[..2], &["-c", "credential.helper="]);
        assert!(inv.args[3].starts_with("credential.helper=!f()"));
        assert_eq!(
            &inv.args[4..],
            &[
                "clone",
                "--progress",
                "--",
                "https://github.com/acme/shop.git",
                "/home/m/src/shop"
            ]
        );
        assert!(!inv.args.iter().any(|a| a.contains("ghp_secret")));
        assert!(inv
            .env
            .contains(&(PASSWORD_ENV.to_string(), "ghp_secret".to_string())));
        assert!(inv.env.contains(&(
            "GIT_SSH_COMMAND".to_string(),
            "ssh -o BatchMode=yes".to_string()
        )));
        assert!(!format!("{cred:?}").contains("ghp_secret"));

        let ssh =
            clone_invocation("git@github.com:acme/shop.git", "shop", Some(&cred), false).unwrap();
        assert_eq!(ssh.args[0], "clone");
        assert!(!ssh.env.iter().any(|(k, _)| k == PASSWORD_ENV));
        assert!(!ssh.env.iter().any(|(k, _)| k == "GIT_SSH_COMMAND"));

        assert!(clone_invocation("--upload-pack=x", "d", None, true).is_err());
        assert!(clone_invocation("https://x/y", " ", None, true).is_err());
        let ls = ls_remote_invocation("https://x/y", None, true).unwrap();
        assert_eq!(ls.args, vec!["ls-remote", "--heads", "https://x/y"]);
    }

    #[test]
    fn parses_ls_remote() {
        let refs = parse_ls_remote("abc\trefs/heads/main\ndef\trefs/heads/feature/x\n\n");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[1].name, "refs/heads/feature/x");
    }

    #[test]
    fn parses_progress_lines() {
        let p =
            parse_progress("Receiving objects:  45% (450/1000), 1.20 MiB | 2.00 MiB/s").unwrap();
        assert_eq!(p.phase, "Receiving objects");
        assert_eq!(p.percent, Some(45));
        assert!((p.fraction.unwrap() - 0.415).abs() < 1e-9);
        let r = parse_progress("remote: Counting objects: 100% (20/20), done.").unwrap();
        assert_eq!(r.phase, "Counting objects");
        assert_eq!(r.fraction, Some(0.06));
        let c = parse_progress("Cloning into 'shop'...").unwrap();
        assert_eq!(c.phase, "Cloning");
        assert!(
            parse_progress("warning: You appear to have cloned an empty repository.").is_none()
        );
        assert!(parse_progress("").is_none());
        let d = parse_progress("Resolving deltas: 100% (5/5), done.").unwrap();
        assert_eq!(d.fraction, Some(0.95));
    }
}
