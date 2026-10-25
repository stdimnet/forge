//! Saved SSH configurations (Settings | Tools | SSH Configurations), stored in
//! `<config dir>/forge/ssh.toml`. Passwords and key passphrases never go there: they live
//! in the OS keychain.

use serde::{Deserialize, Serialize};

/// How to authenticate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthMethod {
    Password,
    /// A private key file (with an optional passphrase).
    KeyPair,
    /// Keys of the running ssh-agent (`SSH_AUTH_SOCK`), then the default key files and the
    /// `IdentityFile`s of `~/.ssh/config`.
    #[default]
    Agent,
}

/// One saved connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConfiguration {
    /// Stable id (keychain account prefix, Recent Projects link).
    pub id: String,
    /// Host name, IP address or a `Host` alias of `~/.ssh/config`.
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub user: String,
    #[serde(default)]
    pub auth: AuthMethod,
    /// Private key for [`AuthMethod::KeyPair`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_file: Option<String>,
    /// Ask for the password / passphrase every time instead of storing it.
    #[serde(default)]
    pub dont_save_secret: bool,
    /// Optional display name; the UI shows `user@host:port` otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// "Parse config file ~/.ssh/config": resolve the host as a `Host` alias and use its
    /// host name, port, user, identity files and jump hosts. On by default.
    #[serde(default = "default_true")]
    pub parse_config: bool,
}

fn default_port() -> u16 {
    22
}

fn default_true() -> bool {
    true
}

impl SshConfiguration {
    /// `user@host` (`user@host:port` for other ports), PHPStorm's label.
    pub fn label(&self) -> String {
        if self.port == 22 {
            format!("{}@{}", self.user, self.host)
        } else {
            format!("{}@{}:{}", self.user, self.host, self.port)
        }
    }

    /// Problems with the fields, worded for the dialog; empty when it can be used.
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.host.trim().is_empty() {
            problems.push("Host is required".to_string());
        } else if self.host.contains(char::is_whitespace) {
            problems.push("Host must not contain spaces".to_string());
        }
        if self.user.trim().is_empty() {
            problems.push("User name is required".to_string());
        }
        if self.port == 0 {
            problems.push("Port must be between 1 and 65535".to_string());
        }
        if self.auth == AuthMethod::KeyPair
            && self.key_file.as_deref().is_none_or(|k| k.trim().is_empty())
        {
            problems.push("Private key file is required".to_string());
        }
        problems
    }

    /// Whether two configurations point at the same account.
    pub fn same_target(&self, other: &SshConfiguration) -> bool {
        self.host.eq_ignore_ascii_case(&other.host)
            && self.port == other.port
            && self.user == other.user
    }
}

/// The file with all saved configurations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshConfigurations {
    #[serde(default, rename = "configuration")]
    pub configurations: Vec<SshConfiguration>,
}

impl SshConfigurations {
    pub fn get(&self, id: &str) -> Option<&SshConfiguration> {
        self.configurations.iter().find(|c| c.id == id)
    }

    /// Insert or replace by id.
    pub fn upsert(&mut self, config: SshConfiguration) {
        match self.configurations.iter_mut().find(|c| c.id == config.id) {
            Some(existing) => *existing = config,
            None => self.configurations.push(config),
        }
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.configurations.len();
        self.configurations.retain(|c| c.id != id);
        before != self.configurations.len()
    }
}

/// `user@host[:port]` (or `ssh://user@host:port`) into its parts; the user may be missing.
pub fn parse_destination(text: &str) -> Option<(Option<String>, String, Option<u16>)> {
    let text = text
        .trim()
        .trim_start_matches("ssh://")
        .trim_end_matches('/');
    if text.is_empty() {
        return None;
    }
    let (user, rest) = match text.rsplit_once('@') {
        Some((u, r)) => (Some(u.to_string()).filter(|u| !u.is_empty()), r),
        None => (None, text),
    };
    let (host, port) = if let Some(inner) = rest.strip_prefix('[') {
        // [host]:port or [ipv6]
        let (h, after) = inner.split_once(']')?;
        (
            h.to_string(),
            after.strip_prefix(':').and_then(|p| p.parse().ok()),
        )
    } else if rest.matches(':').count() == 1 {
        let (h, p) = rest.split_once(':')?;
        (h.to_string(), Some(p.parse().ok()?))
    } else {
        (rest.to_string(), None)
    };
    (!host.is_empty()).then_some((user, host, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> SshConfiguration {
        SshConfiguration {
            id: "c1".into(),
            host: "shop.example.com".into(),
            port: 22,
            user: "deploy".into(),
            auth: AuthMethod::KeyPair,
            key_file: Some("~/.ssh/id_ed25519".into()),
            dont_save_secret: false,
            name: None,
            parse_config: true,
        }
    }

    #[test]
    fn labels_and_validation() {
        let mut c = cfg();
        assert_eq!(c.label(), "deploy@shop.example.com");
        assert!(c.validate().is_empty());
        c.port = 2222;
        assert_eq!(c.label(), "deploy@shop.example.com:2222");
        c.key_file = None;
        c.user = String::new();
        assert_eq!(
            c.validate(),
            vec!["User name is required", "Private key file is required"]
        );
    }

    #[test]
    fn store_round_trips_through_toml_without_secrets() {
        let mut all = SshConfigurations::default();
        all.upsert(cfg());
        let mut changed = cfg();
        changed.port = 2200;
        all.upsert(changed);
        assert_eq!(all.configurations.len(), 1);
        let text = toml_like(&all);
        assert!(text.contains("2200"));
        assert!(!text.to_lowercase().contains("password\":"));
        assert!(all.remove("c1"));
        assert!(!all.remove("c1"));
    }

    #[test]
    fn parse_config_defaults_to_on_for_older_files() {
        let old: SshConfiguration =
            serde_json::from_str(r#"{"id":"a","host":"h","user":"u"}"#).unwrap();
        assert!(old.parse_config);
        assert_eq!(old.port, 22);
        assert_eq!(old.auth, AuthMethod::Agent);
    }

    fn toml_like(all: &SshConfigurations) -> String {
        serde_json::to_string(all).unwrap()
    }

    #[test]
    fn destinations() {
        assert_eq!(
            parse_destination("deploy@shop.example.com:2222"),
            Some((Some("deploy".into()), "shop.example.com".into(), Some(2222)))
        );
        assert_eq!(
            parse_destination("ssh://root@10.0.0.5/"),
            Some((Some("root".into()), "10.0.0.5".into(), None))
        );
        assert_eq!(
            parse_destination("[fe80::1]:22"),
            Some((None, "fe80::1".into(), Some(22)))
        );
        assert_eq!(
            parse_destination("fe80::1"),
            Some((None, "fe80::1".into(), None))
        );
        assert_eq!(parse_destination("host:notaport"), None);
        assert_eq!(parse_destination(""), None);
    }
}
