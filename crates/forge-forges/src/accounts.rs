//! Code host accounts. The list lives in `<config dir>/forge/forges.toml`; tokens are kept
//! in the OS keychain (service `forge-forges`, account = account id), never in files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::{HostError, HostResult};
use crate::model::HostKind;
use crate::remote::normalize_server;

pub const KEYCHAIN_SERVICE: &str = "forge-forges";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub kind: HostKind,
    /// Web URL of the server, e.g. `https://gitlab.example.com`.
    pub server: String,
    /// Login of the token owner, filled in when the token is verified.
    #[serde(default)]
    pub username: Option<String>,
    /// API root override (defaults are derived from `server`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_url: Option<String>,
}

impl Account {
    /// The REST API root for this account.
    pub fn api_base(&self) -> String {
        if let Some(api) = self.api_url.as_deref().filter(|s| !s.trim().is_empty()) {
            return api.trim_end_matches('/').to_string();
        }
        api_base(self.kind, &self.server)
    }
}

/// Default REST API root for a server.
pub fn api_base(kind: HostKind, server: &str) -> String {
    let server = normalize_server(server);
    match kind {
        HostKind::Gitlab => format!("{server}/api/v4"),
        HostKind::Forgejo => format!("{server}/api/v1"),
        // `https://dev.azure.com/<organization>`: the API lives below the organization.
        HostKind::Azure => server,
        HostKind::Github => {
            if server == "https://github.com" || server == "https://www.github.com" {
                "https://api.github.com".into()
            } else {
                format!("{server}/api/v3")
            }
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountsFile {
    #[serde(default)]
    pub accounts: Vec<Account>,
}

impl AccountsFile {
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("forge").join("forges.toml"))
    }

    pub fn load(path: &Path) -> HostResult<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|e| HostError::Config(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(HostError::Config(e.to_string())),
        }
    }

    pub fn save(&self, path: &Path) -> HostResult<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| HostError::Config(e.to_string()))?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| HostError::Config(e.to_string()))?;
        std::fs::write(path, text).map_err(|e| HostError::Config(e.to_string()))
    }
}

pub trait SecretStore: Send + Sync {
    fn get(&self, account_id: &str) -> HostResult<Option<String>>;
    fn set(&self, account_id: &str, secret: &str) -> HostResult<()>;
    fn delete(&self, account_id: &str) -> HostResult<()>;
}

/// The platform keychain (macOS Keychain, Windows Credential Manager, Secret Service).
#[derive(Debug, Default)]
pub struct KeyringStore;

impl KeyringStore {
    fn entry(id: &str) -> HostResult<keyring::Entry> {
        keyring::Entry::new(KEYCHAIN_SERVICE, id).map_err(|e| HostError::Keychain(e.to_string()))
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, id: &str) -> HostResult<Option<String>> {
        match Self::entry(id)?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(HostError::Keychain(e.to_string())),
        }
    }

    fn set(&self, id: &str, secret: &str) -> HostResult<()> {
        Self::entry(id)?
            .set_password(secret)
            .map_err(|e| HostError::Keychain(e.to_string()))
    }

    fn delete(&self, id: &str) -> HostResult<()> {
        match Self::entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(HostError::Keychain(e.to_string())),
        }
    }
}

/// In-memory store for tests and environments without a keychain.
#[derive(Debug, Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, id: &str) -> HostResult<Option<String>> {
        Ok(self.0.lock().expect("poisoned").get(id).cloned())
    }

    fn set(&self, id: &str, secret: &str) -> HostResult<()> {
        self.0
            .lock()
            .expect("poisoned")
            .insert(id.into(), secret.into());
        Ok(())
    }

    fn delete(&self, id: &str) -> HostResult<()> {
        self.0.lock().expect("poisoned").remove(id);
        Ok(())
    }
}

/// Environment variables consulted when the keychain has no token for an account on the
/// host's public server.
pub fn env_vars(kind: HostKind) -> &'static [&'static str] {
    match kind {
        HostKind::Gitlab => &["GITLAB_TOKEN"],
        HostKind::Forgejo => &["FORGEJO_TOKEN", "GITEA_TOKEN"],
        HostKind::Github => &["GITHUB_TOKEN", "GH_TOKEN"],
        HostKind::Azure => &["AZURE_DEVOPS_EXT_PAT"],
    }
}

/// A unique, readable account id: `gitlab-gitlab.example.com`, `-2` suffix on clashes.
pub fn account_id(kind: HostKind, server: &str, taken: &[Account]) -> String {
    let host = normalize_server(server)
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .replace(['/', ':'], "-");
    let base = format!("{}-{}", kind.id(), host.trim_end_matches('-'));
    let mut id = base.clone();
    let mut n = 2;
    while taken.iter().any(|a| a.id == id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_roots() {
        assert_eq!(
            api_base(HostKind::Github, "github.com"),
            "https://api.github.com"
        );
        assert_eq!(
            api_base(HostKind::Github, "https://ghe.acme.dev/"),
            "https://ghe.acme.dev/api/v3"
        );
        assert_eq!(
            api_base(HostKind::Gitlab, "https://example.com/gitlab"),
            "https://example.com/gitlab/api/v4"
        );
        assert_eq!(
            api_base(HostKind::Forgejo, "https://codeberg.org"),
            "https://codeberg.org/api/v1"
        );
    }

    #[test]
    fn ids_are_unique() {
        let a = Account {
            id: account_id(HostKind::Gitlab, "https://gitlab.com", &[]),
            kind: HostKind::Gitlab,
            server: "https://gitlab.com".into(),
            username: None,
            api_url: None,
        };
        assert_eq!(a.id, "gitlab-gitlab.com");
        assert_eq!(
            account_id(HostKind::Gitlab, "gitlab.com", &[a]),
            "gitlab-gitlab.com-2"
        );
    }

    #[test]
    fn file_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forge/forges.toml");
        assert!(AccountsFile::load(&path).unwrap().accounts.is_empty());
        let file = AccountsFile {
            accounts: vec![Account {
                id: "forgejo-git.acme.dev".into(),
                kind: HostKind::Forgejo,
                server: "https://git.acme.dev".into(),
                username: Some("marlon".into()),
                api_url: None,
            }],
        };
        file.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("token"));
        assert_eq!(AccountsFile::load(&path).unwrap().accounts, file.accounts);
    }
}
