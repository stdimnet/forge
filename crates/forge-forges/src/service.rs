//! Accounts plus host construction: the entry point the IDE uses.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::accounts::{account_id, env_vars, Account, AccountsFile, SecretStore};
use crate::error::{HostError, HostResult};
use crate::forgejo::Forgejo;
use crate::github::GitHub;
use crate::gitlab::GitLab;
use crate::host::CodeHost;
use crate::http::client;
use crate::model::{HostKind, RepoRef, User};
use crate::remote::{detect, normalize_server};

/// Build a host client for an API root.
pub fn open_host(
    kind: HostKind,
    client: reqwest::Client,
    api_base: &str,
    token: Option<String>,
) -> Arc<dyn CodeHost> {
    match kind {
        HostKind::Gitlab => Arc::new(GitLab::new(client, api_base, token)),
        HostKind::Forgejo => Arc::new(Forgejo::new(client, api_base, token)),
        HostKind::Github => Arc::new(GitHub::new(client, api_base, token)),
    }
}

/// The ref that holds a merge request's head commit, also for forks.
pub fn review_ref(kind: HostKind, number: u64) -> String {
    match kind {
        HostKind::Gitlab => format!("refs/merge-requests/{number}/head"),
        HostKind::Forgejo | HostKind::Github => format!("refs/pull/{number}/head"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenSource {
    Keychain,
    Env,
    None,
}

/// An account as shown in Settings (never the token itself).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    #[serde(flatten)]
    pub account: Account,
    pub token_source: TokenSource,
}

type EnvFn = dyn Fn(&str) -> Option<String> + Send + Sync;

pub struct ForgeService {
    path: Option<PathBuf>,
    accounts: Mutex<Vec<Account>>,
    secrets: Arc<dyn SecretStore>,
    client: reqwest::Client,
    env: Box<EnvFn>,
}

impl ForgeService {
    /// Load accounts from `path` (`None` keeps them in memory only).
    pub fn new(path: Option<PathBuf>, secrets: Arc<dyn SecretStore>) -> HostResult<Self> {
        let accounts = match &path {
            Some(p) => AccountsFile::load(p)?.accounts,
            None => Vec::new(),
        };
        Ok(Self {
            path,
            accounts: Mutex::new(accounts),
            secrets,
            client: client(),
            env: Box::new(|k| std::env::var(k).ok()),
        })
    }

    /// Replace the environment lookup (tests).
    pub fn with_env(
        mut self,
        env: impl Fn(&str) -> Option<String> + Send + Sync + 'static,
    ) -> Self {
        self.env = Box::new(env);
        self
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Account>> {
        self.accounts.lock().expect("accounts lock poisoned")
    }

    fn save(&self, accounts: &[Account]) -> HostResult<()> {
        match &self.path {
            Some(p) => AccountsFile {
                accounts: accounts.to_vec(),
            }
            .save(p),
            None => Ok(()),
        }
    }

    pub fn accounts(&self) -> Vec<AccountInfo> {
        self.lock()
            .iter()
            .map(|a| AccountInfo {
                token_source: self.token(a).1,
                account: a.clone(),
            })
            .collect()
    }

    fn token(&self, acc: &Account) -> (Option<String>, TokenSource) {
        match self.secrets.get(&acc.id) {
            Ok(Some(t)) if !t.is_empty() => return (Some(t), TokenSource::Keychain),
            Ok(_) => {}
            Err(e) => tracing::warn!("keychain lookup for {} failed: {e}", acc.id),
        }
        if normalize_server(&acc.server) == acc.kind.default_server() {
            for var in env_vars(acc.kind) {
                if let Some(t) = (self.env)(var).filter(|t| !t.is_empty()) {
                    return (Some(t), TokenSource::Env);
                }
            }
        }
        (None, TokenSource::None)
    }

    /// Verify `token` against the server and store the account (replacing one for the same
    /// server). The token goes to the keychain only after it was accepted.
    pub async fn login(
        &self,
        kind: HostKind,
        server: &str,
        token: &str,
        api_url: Option<String>,
    ) -> HostResult<AccountInfo> {
        let server = normalize_server(if server.trim().is_empty() {
            kind.default_server()
        } else {
            server
        });
        let mut account = Account {
            id: String::new(),
            kind,
            server: server.clone(),
            username: None,
            api_url: api_url.filter(|u| !u.trim().is_empty()),
        };
        let host = open_host(
            kind,
            self.client.clone(),
            &account.api_base(),
            Some(token.trim().to_string()),
        );
        let me = host.current_user().await?;
        account.username = Some(me.username);
        let mut accounts = self.lock().clone();
        match accounts
            .iter()
            .position(|a| a.kind == kind && normalize_server(&a.server) == server)
        {
            Some(i) => {
                account.id = accounts[i].id.clone();
                accounts[i] = account.clone();
            }
            None => {
                account.id = account_id(kind, &server, &accounts);
                accounts.push(account.clone());
            }
        }
        self.secrets.set(&account.id, token.trim())?;
        self.save(&accounts)?;
        *self.lock() = accounts;
        Ok(AccountInfo {
            account,
            token_source: TokenSource::Keychain,
        })
    }

    pub fn logout(&self, id: &str) -> HostResult<()> {
        let mut accounts = self.lock().clone();
        accounts.retain(|a| a.id != id);
        self.save(&accounts)?;
        *self.lock() = accounts;
        self.secrets.delete(id)
    }

    fn account(&self, id: &str) -> HostResult<Account> {
        self.lock()
            .iter()
            .find(|a| a.id == id)
            .cloned()
            .ok_or_else(|| HostError::Config(format!("unknown account {id}")))
    }

    /// Check the stored token; returns its owner.
    pub async fn verify(&self, id: &str) -> HostResult<User> {
        let acc = self.account(id)?;
        let (token, _) = self.token(&acc);
        let Some(token) = token else {
            return Err(HostError::Unauthorized {
                host: acc.kind.display_name().into(),
            });
        };
        open_host(acc.kind, self.client.clone(), &acc.api_base(), Some(token))
            .current_user()
            .await
    }

    /// Code-host repositories among git remotes (`(name, url)` pairs).
    pub fn detect(&self, remotes: &[(String, String)]) -> Vec<RepoRef> {
        detect(remotes, &self.lock())
    }

    pub fn account_for(&self, repo: &RepoRef) -> Option<Account> {
        let server = normalize_server(&repo.server);
        self.lock()
            .iter()
            .find(|a| a.kind == repo.kind && normalize_server(&a.server) == server)
            .cloned()
    }

    /// A client for the repository's host, authenticated when an account exists. Without
    /// an account, public data can still be read anonymously.
    pub fn host_for(&self, repo: &RepoRef) -> Arc<dyn CodeHost> {
        match self.account_for(repo) {
            Some(acc) => {
                let (token, _) = self.token(&acc);
                open_host(acc.kind, self.client.clone(), &acc.api_base(), token)
            }
            None => {
                let probe = Account {
                    id: String::new(),
                    kind: repo.kind,
                    server: repo.server.clone(),
                    username: None,
                    api_url: None,
                };
                let (token, _) = self.token(&probe);
                open_host(repo.kind, self.client.clone(), &probe.api_base(), token)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::MemoryStore;

    #[test]
    fn env_token_only_for_public_servers() {
        let svc = ForgeService::new(None, Arc::new(MemoryStore::default()))
            .unwrap()
            .with_env(|k| (k == "GITHUB_TOKEN").then(|| "env".to_string()));
        let gh = Account {
            id: "github-github.com".into(),
            kind: HostKind::Github,
            server: "https://github.com".into(),
            username: None,
            api_url: None,
        };
        assert_eq!(svc.token(&gh), (Some("env".into()), TokenSource::Env));
        let ghe = Account {
            server: "https://ghe.acme.dev".into(),
            ..gh
        };
        assert_eq!(svc.token(&ghe), (None, TokenSource::None));
    }
}
