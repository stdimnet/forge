//! The relay token: a secret, kept in the OS keychain (service `forge-cwm`, account
//! `relay-token`) and never in settings files, links or logs.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;

use crate::relay::normalize_relay_token;
use crate::{Error, Result};

pub const KEYCHAIN_SERVICE: &str = "forge-cwm";
pub const RELAY_TOKEN_ACCOUNT: &str = "relay-token";

pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> std::result::Result<Option<String>, String>;
    fn set(&self, account: &str, secret: &str) -> std::result::Result<(), String>;
    fn delete(&self, account: &str) -> std::result::Result<(), String>;
}

/// The platform keychain (macOS Keychain, Windows Credential Manager, Secret Service).
#[cfg(feature = "keychain")]
#[derive(Debug, Default)]
pub struct KeyringStore;

#[cfg(feature = "keychain")]
impl KeyringStore {
    fn entry(account: &str) -> std::result::Result<keyring::Entry, String> {
        keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|e| e.to_string())
    }
}

#[cfg(feature = "keychain")]
impl SecretStore for KeyringStore {
    fn get(&self, account: &str) -> std::result::Result<Option<String>, String> {
        match Self::entry(account)?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set(&self, account: &str, secret: &str) -> std::result::Result<(), String> {
        Self::entry(account)?
            .set_password(secret)
            .map_err(|e| e.to_string())
    }

    fn delete(&self, account: &str) -> std::result::Result<(), String> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// In-memory store for tests.
#[derive(Debug, Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, account: &str) -> std::result::Result<Option<String>, String> {
        Ok(self.0.lock().expect("poisoned").get(account).cloned())
    }

    fn set(&self, account: &str, secret: &str) -> std::result::Result<(), String> {
        self.0
            .lock()
            .expect("poisoned")
            .insert(account.into(), secret.into());
        Ok(())
    }

    fn delete(&self, account: &str) -> std::result::Result<(), String> {
        self.0.lock().expect("poisoned").remove(account);
        Ok(())
    }
}

/// Whether a relay token is stored, for the settings page (the token itself is never sent
/// to the UI).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayTokenStatus {
    pub set: bool,
    /// The keychain could not be read or written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The relay token in a secret store.
pub struct RelayToken<'a>(pub &'a dyn SecretStore);

impl RelayToken<'_> {
    pub fn get(&self) -> Result<Option<String>> {
        self.0
            .get(RELAY_TOKEN_ACCOUNT)
            .map_err(|e| Error::InvalidRelay(format!("cannot read the relay token: {e}")))
    }

    /// Store a token as typed; an empty one removes it.
    pub fn set(&self, raw: Option<&str>) -> Result<()> {
        let token = normalize_relay_token(raw.unwrap_or_default())?;
        let stored = match token {
            Some(t) => self.0.set(RELAY_TOKEN_ACCOUNT, &t),
            None => self.0.delete(RELAY_TOKEN_ACCOUNT),
        };
        stored.map_err(|e| Error::InvalidRelay(format!("cannot write the relay token: {e}")))
    }

    pub fn status(&self) -> RelayTokenStatus {
        match self.0.get(RELAY_TOKEN_ACCOUNT) {
            Ok(t) => RelayTokenStatus {
                set: t.is_some(),
                error: None,
            },
            Err(e) => RelayTokenStatus {
                set: false,
                error: Some(e),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_stored_checked_and_removed() {
        let store = MemoryStore::default();
        let t = RelayToken(&store);
        assert!(!t.status().set);
        t.set(Some("  s3cret-token ")).unwrap();
        assert_eq!(t.get().unwrap().as_deref(), Some("s3cret-token"));
        assert!(t.status().set);
        assert!(t.set(Some("has space")).is_err());
        assert_eq!(t.get().unwrap().as_deref(), Some("s3cret-token"));
        t.set(Some("")).unwrap();
        assert_eq!(t.get().unwrap(), None);
        t.set(Some("again")).unwrap();
        t.set(None).unwrap();
        assert!(!t.status().set);
    }

    #[test]
    fn the_status_never_contains_the_token() {
        let store = MemoryStore::default();
        RelayToken(&store).set(Some("s3cret-token")).unwrap();
        let json = serde_json::to_string(&RelayToken(&store).status()).unwrap();
        assert!(!json.contains("s3cret"));
    }
}
