//! OpenSSH client configuration (`~/.ssh/config`): host blocks with patterns and the
//! keywords Forge uses to connect. Like `ssh`, the first value obtained for a keyword wins
//! and `IdentityFile` accumulates.

use serde::{Deserialize, Serialize};

/// One `Host` block (or the implicit block before the first `Host` line).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostBlock {
    /// Patterns of the `Host` line; empty for the global block (matches everything).
    pub patterns: Vec<String>,
    /// `(keyword in lower case, value)` in file order.
    pub options: Vec<(String, String)>,
    /// A `Match` block: Forge cannot evaluate its criteria and skips it.
    #[serde(default)]
    pub is_match: bool,
}

/// A parsed config file (with `Include`d files already inlined by the caller).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConfig {
    pub blocks: Vec<HostBlock>,
}

/// Connection settings for one host alias after applying every matching block.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedHost {
    /// The alias that was looked up.
    pub alias: String,
    /// `HostName`, or the alias itself.
    pub host_name: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    /// `IdentityFile`s with `~` and `%` tokens expanded.
    pub identity_files: Vec<String>,
    pub identities_only: bool,
    /// Set when the host needs a jump host (not supported by Forge yet; shown as a hint).
    pub proxy_jump: Option<String>,
    pub proxy_command: Option<String>,
}

/// A concrete host alias for the "Host" field's suggestions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostEntry {
    pub alias: String,
    pub host_name: String,
    pub user: Option<String>,
    pub port: Option<u16>,
}

/// Split a config line into keyword and value (`Key value`, `Key=value`, `Key = "a b"`).
fn split_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let end = line
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(line.len());
    let key = line[..end].to_ascii_lowercase();
    let rest = line[end..].trim_start();
    let rest = rest.strip_prefix('=').unwrap_or(rest).trim();
    Some((key, rest.to_string()))
}

/// Words of a value, honoring double quotes.
pub fn words(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in value.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(cur);
    }
    out
}

impl SshConfig {
    /// Parse config text. `include` is called for every `Include` argument and returns the
    /// text of the matching files (already glob-expanded); pass `|_| Vec::new()` to ignore
    /// includes.
    pub fn parse_with(text: &str, include: &mut dyn FnMut(&str) -> Vec<String>) -> Self {
        let mut config = SshConfig {
            blocks: vec![HostBlock::default()],
        };
        config.parse_into(text, include, 0);
        config
    }

    pub fn parse(text: &str) -> Self {
        Self::parse_with(text, &mut |_| Vec::new())
    }

    fn parse_into(&mut self, text: &str, include: &mut dyn FnMut(&str) -> Vec<String>, depth: u8) {
        for line in text.lines() {
            let Some((key, value)) = split_line(line) else {
                continue;
            };
            match key.as_str() {
                "host" => self.blocks.push(HostBlock {
                    patterns: words(&value),
                    options: Vec::new(),
                    is_match: false,
                }),
                "match" => self.blocks.push(HostBlock {
                    patterns: Vec::new(),
                    options: Vec::new(),
                    is_match: true,
                }),
                "include" if depth < 8 => {
                    for arg in words(&value) {
                        for text in include(&arg) {
                            self.parse_into(&text, include, depth + 1);
                        }
                    }
                }
                _ => {
                    if let Some(block) = self.blocks.last_mut() {
                        block.options.push((key, value));
                    }
                }
            }
        }
    }

    /// Settings for `alias`; `home` expands `~` and `%d`, `local_user` is `%u`.
    pub fn resolve(&self, alias: &str, home: &str, local_user: &str) -> ResolvedHost {
        let mut r = ResolvedHost {
            alias: alias.to_string(),
            ..ResolvedHost::default()
        };
        let mut host_name: Option<String> = None;
        let mut identity_raw: Vec<String> = Vec::new();
        let mut identities_only: Option<bool> = None;
        for block in &self.blocks {
            if block.is_match || !host_matches(&block.patterns, alias) {
                continue;
            }
            for (key, value) in &block.options {
                let first = words(value).into_iter().next().unwrap_or_default();
                match key.as_str() {
                    "hostname" if host_name.is_none() => host_name = Some(first),
                    "user" if r.user.is_none() => r.user = Some(first),
                    "port" if r.port.is_none() => r.port = first.parse().ok(),
                    "identityfile" => identity_raw.push(first),
                    "identitiesonly" if identities_only.is_none() => {
                        identities_only = Some(first.eq_ignore_ascii_case("yes"))
                    }
                    "proxyjump" if r.proxy_jump.is_none() && first != "none" => {
                        r.proxy_jump = Some(first)
                    }
                    "proxycommand" if r.proxy_command.is_none() && first != "none" => {
                        r.proxy_command = Some(value.clone())
                    }
                    _ => {}
                }
            }
        }
        r.host_name = host_name
            .map(|h| h.replace("%h", alias).replace("%%", "%"))
            .unwrap_or_else(|| alias.to_string());
        r.identities_only = identities_only.unwrap_or(false);
        let port = r.port.unwrap_or(22).to_string();
        let remote_user = r.user.clone().unwrap_or_else(|| local_user.to_string());
        r.identity_files = identity_raw
            .into_iter()
            .map(|f| {
                let f = match f.strip_prefix("~/") {
                    Some(rest) => format!("{home}/{rest}"),
                    None if f == "~" => home.to_string(),
                    None => f,
                };
                f.replace("%d", home)
                    .replace("%h", &r.host_name)
                    .replace("%p", &port)
                    .replace("%r", &remote_user)
                    .replace("%u", local_user)
                    .replace("%%", "%")
            })
            .collect();
        r
    }

    /// Concrete aliases (no wildcards or negations), in file order without duplicates.
    pub fn hosts(&self, home: &str, local_user: &str) -> Vec<HostEntry> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for block in &self.blocks {
            for p in &block.patterns {
                if p.contains(['*', '?', '!']) || !seen.insert(p.clone()) {
                    continue;
                }
                let r = self.resolve(p, home, local_user);
                out.push(HostEntry {
                    alias: p.clone(),
                    host_name: r.host_name,
                    user: r.user,
                    port: r.port,
                });
            }
        }
        out
    }
}

/// Whether `alias` matches a `Host` line: some pattern matches and no negated one does.
pub fn host_matches(patterns: &[String], alias: &str) -> bool {
    if patterns.is_empty() {
        return true;
    }
    let mut matched = false;
    for p in patterns {
        if let Some(neg) = p.strip_prefix('!') {
            if wildcard_match(neg, alias) {
                return false;
            }
        } else if wildcard_match(p, alias) {
            matched = true;
        }
    }
    matched
}

/// `*` and `?` wildcards, case-insensitive (host names are).
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"
# Global defaults
ServerAliveInterval 30

Host shop
    HostName shop.example.com
    User deploy
    Port 2222
    IdentityFile ~/.ssh/id_shop

Host *.example.com !legacy.example.com
    User admin
    IdentityFile "~/.ssh/keys/%h key"
    IdentitiesOnly yes

Host staging prod-?
  HostName=%h.internal
  ProxyJump bastion

Match exec "true"
    User nobody

Host *
    User fallback
    IdentityFile ~/.ssh/id_ed25519
"#;

    fn cfg() -> SshConfig {
        SshConfig::parse(CONFIG)
    }

    #[test]
    fn first_value_wins_and_identity_files_accumulate() {
        let r = cfg().resolve("shop", "/home/me", "me");
        assert_eq!(r.host_name, "shop.example.com");
        assert_eq!(r.user.as_deref(), Some("deploy"));
        assert_eq!(r.port, Some(2222));
        assert_eq!(
            r.identity_files,
            vec!["/home/me/.ssh/id_shop", "/home/me/.ssh/id_ed25519"]
        );
        assert!(!r.identities_only);
    }

    #[test]
    fn wildcards_negations_and_tokens() {
        let r = cfg().resolve("api.example.com", "/home/me", "me");
        assert_eq!(r.host_name, "api.example.com");
        assert_eq!(r.user.as_deref(), Some("admin"));
        assert_eq!(r.port, None);
        assert_eq!(
            r.identity_files[0],
            "/home/me/.ssh/keys/api.example.com key"
        );
        assert!(r.identities_only);

        let legacy = cfg().resolve("legacy.example.com", "/home/me", "me");
        assert_eq!(legacy.user.as_deref(), Some("fallback"));

        let prod = cfg().resolve("prod-1", "/home/me", "me");
        assert_eq!(prod.host_name, "prod-1.internal");
        assert_eq!(prod.proxy_jump.as_deref(), Some("bastion"));
        // `Match` blocks are skipped rather than guessed.
        assert_ne!(prod.user.as_deref(), Some("nobody"));
    }

    #[test]
    fn unknown_hosts_fall_back_to_the_alias() {
        let r = SshConfig::parse("").resolve("10.0.0.5", "/h", "me");
        assert_eq!(r.host_name, "10.0.0.5");
        assert_eq!(r.user, None);
        assert!(r.identity_files.is_empty());
    }

    #[test]
    fn lists_concrete_hosts_for_suggestions() {
        let hosts = cfg().hosts("/home/me", "me");
        let aliases: Vec<&str> = hosts.iter().map(|h| h.alias.as_str()).collect();
        assert_eq!(aliases, vec!["shop", "staging"]);
        assert_eq!(hosts[0].port, Some(2222));
        assert_eq!(hosts[1].host_name, "staging.internal");
    }

    #[test]
    fn includes_are_inlined() {
        let mut calls = Vec::new();
        let cfg = SshConfig::parse_with("Include conf.d/*\nHost a\n  User x\n", &mut |arg| {
            calls.push(arg.to_string());
            vec!["Host b\n  HostName b.example.com\n".to_string()]
        });
        assert_eq!(calls, vec!["conf.d/*"]);
        assert_eq!(cfg.resolve("b", "/h", "me").host_name, "b.example.com");
        assert_eq!(cfg.resolve("a", "/h", "me").user.as_deref(), Some("x"));
    }

    #[test]
    fn wildcard_matching() {
        assert!(wildcard_match("*.example.com", "A.Example.com"));
        assert!(wildcard_match("prod-?", "prod-1"));
        assert!(!wildcard_match("prod-?", "prod-12"));
        assert!(wildcard_match("*", ""));
        assert!(!wildcard_match("a*b", "ac"));
    }
}
