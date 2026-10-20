//! Data source passwords: the OS keychain (service `forge-db`, account = data source id).

use std::collections::HashMap;
use std::sync::Mutex;

use crate::error::{DbError, DbResult};

pub const KEYCHAIN_SERVICE: &str = "forge-db";

pub trait SecretStore: Send + Sync {
    fn get(&self, id: &str) -> DbResult<Option<String>>;
    fn set(&self, id: &str, secret: &str) -> DbResult<()>;
    fn delete(&self, id: &str) -> DbResult<()>;
}

/// The platform keychain (macOS Keychain, Windows Credential Manager, Secret Service).
#[derive(Debug, Default)]
pub struct KeyringStore;

impl KeyringStore {
    fn entry(id: &str) -> DbResult<keyring::Entry> {
        keyring::Entry::new(KEYCHAIN_SERVICE, id).map_err(|e| DbError::Keychain(e.to_string()))
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, id: &str) -> DbResult<Option<String>> {
        match Self::entry(id)?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(DbError::Keychain(e.to_string())),
        }
    }

    fn set(&self, id: &str, secret: &str) -> DbResult<()> {
        Self::entry(id)?
            .set_password(secret)
            .map_err(|e| DbError::Keychain(e.to_string()))
    }

    fn delete(&self, id: &str) -> DbResult<()> {
        match Self::entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(DbError::Keychain(e.to_string())),
        }
    }
}

/// In-memory store for tests and environments without a keychain.
#[derive(Debug, Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, id: &str) -> DbResult<Option<String>> {
        Ok(self.0.lock().expect("poisoned").get(id).cloned())
    }

    fn set(&self, id: &str, secret: &str) -> DbResult<()> {
        self.0
            .lock()
            .expect("poisoned")
            .insert(id.into(), secret.into());
        Ok(())
    }

    fn delete(&self, id: &str) -> DbResult<()> {
        self.0.lock().expect("poisoned").remove(id);
        Ok(())
    }
}
