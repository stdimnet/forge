//! Saved SSH configurations on disk (`<config dir>/forge/ssh.toml`), their passwords and
//! passphrases in the OS keychain (service `forge-ssh`, account = configuration id), and
//! the user's OpenSSH files (`~/.ssh/config`, `~/.ssh/known_hosts`).

use std::path::{Path, PathBuf};

use forge_core::{ForgeError, Result};

use crate::config::SshConfigurations;
use crate::ssh_config::{wildcard_match, SshConfig};

const KEYRING_SERVICE: &str = "forge-ssh";

fn other(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

/// `<config dir>/forge/ssh.toml`.
pub fn default_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("forge").join("ssh.toml"))
}

pub fn load_from(path: &Path) -> SshConfigurations {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| toml::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_to(path: &Path, all: &SshConfigurations) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| ForgeError::io(dir, e))?;
    }
    let text = toml::to_string_pretty(all).map_err(other)?;
    std::fs::write(path, text).map_err(|e| ForgeError::io(path, e))
}

/// The saved configurations (empty when there are none).
pub fn load() -> SshConfigurations {
    default_path().map(|p| load_from(&p)).unwrap_or_default()
}

pub fn save(all: &SshConfigurations) -> Result<()> {
    let path = default_path().ok_or_else(|| other("no config directory"))?;
    save_to(&path, all)
}

/// The stored password or passphrase of a configuration.
pub fn secret(id: &str) -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, id)
        .ok()?
        .get_password()
        .ok()
}

/// Store (or with `None` delete) the password or passphrase of a configuration.
pub fn set_secret(id: &str, secret: Option<&str>) -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, id).map_err(other)?;
    match secret {
        Some(s) if !s.is_empty() => entry.set_password(s).map_err(other),
        _ => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(other(e)),
        },
    }
}

/// `~/.ssh/known_hosts`.
pub fn known_hosts_path(home: &Path) -> PathBuf {
    home.join(".ssh").join("known_hosts")
}

/// The texts of the files an `Include` argument names (relative to `~/.ssh`, globs in the
/// last component).
fn include_texts(home: &Path, arg: &str) -> Vec<String> {
    let path = match arg.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if Path::new(arg).is_absolute() => PathBuf::from(arg),
        None => home.join(".ssh").join(arg),
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !name.contains(['*', '?']) {
        return std::fs::read_to_string(&path).into_iter().collect();
    }
    let Some(dir) = path.parent() else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .is_some_and(|n| wildcard_match(&name, &n.to_string_lossy()))
        })
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .collect()
}

/// `~/.ssh/config` with its includes (empty when missing).
pub fn read_ssh_config(home: &Path) -> SshConfig {
    let text = std::fs::read_to_string(home.join(".ssh").join("config")).unwrap_or_default();
    SshConfig::parse_with(&text, &mut |arg| include_texts(home, arg))
}

/// The local user name (`%u` in `~/.ssh/config`).
pub fn local_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AuthMethod, SshConfiguration};

    #[test]
    fn configurations_round_trip_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forge/ssh.toml");
        assert!(load_from(&path).configurations.is_empty());
        let mut all = SshConfigurations::default();
        all.upsert(SshConfiguration {
            id: "shop".into(),
            host: "shop.example.com".into(),
            port: 22,
            user: "deploy".into(),
            auth: AuthMethod::Password,
            key_file: None,
            dont_save_secret: false,
            name: Some("Shop".into()),
        });
        save_to(&path, &all).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[[configuration]]"), "{text}");
        assert_eq!(load_from(&path), all);
    }

    #[test]
    fn reads_ssh_config_with_includes() {
        let home = tempfile::tempdir().unwrap();
        let ssh = home.path().join(".ssh");
        std::fs::create_dir_all(ssh.join("conf.d")).unwrap();
        std::fs::write(
            ssh.join("config"),
            "Include conf.d/*.conf\nHost shop\n  HostName shop.example.com\n",
        )
        .unwrap();
        std::fs::write(
            ssh.join("conf.d/work.conf"),
            "Host ci\n  HostName ci.internal\n  User build\n",
        )
        .unwrap();
        std::fs::write(ssh.join("conf.d/ignored.txt"), "Host nope\n").unwrap();
        let cfg = read_ssh_config(home.path());
        let aliases: Vec<String> = cfg.hosts("/h", "me").into_iter().map(|h| h.alias).collect();
        assert_eq!(aliases, vec!["ci", "shop"]);
        assert_eq!(cfg.resolve("ci", "/h", "me").user.as_deref(), Some("build"));
    }
}
