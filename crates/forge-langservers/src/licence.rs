//! Intelephense licence key: kept in the OS keychain (service `forge-lsp`, account
//! `intelephense`), never in settings files. Intelephense runs free without it; with a
//! valid key it is passed as `initializationOptions.licenceKey` and premium features
//! unlock. Licences are bought at <https://intelephense.com>; Forge has no purchase flow.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;

pub const KEYCHAIN_SERVICE: &str = "forge-lsp";
pub const INTELEPHENSE_ACCOUNT: &str = "intelephense";
/// Where licences are sold (shown as a link).
pub const INTELEPHENSE_URL: &str = "https://intelephense.com";
/// The legacy plaintext setting that is migrated into the keychain.
pub const LEGACY_SETTING_KEY: &str = "php.lsp.licenceKey";

pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn set(&self, account: &str, secret: &str) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

/// The platform keychain (macOS Keychain, Windows Credential Manager, Secret Service).
#[derive(Debug, Default)]
pub struct KeyringStore;

impl KeyringStore {
    fn entry(account: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|e| e.to_string())
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        match Self::entry(account)?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), String> {
        Self::entry(account)?
            .set_password(secret)
            .map_err(|e| e.to_string())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// In-memory store for tests and the browser-less fallback.
#[derive(Debug, Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        Ok(self.0.lock().expect("poisoned").get(account).cloned())
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), String> {
        self.0
            .lock()
            .expect("poisoned")
            .insert(account.into(), secret.into());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        self.0.lock().expect("poisoned").remove(account);
        Ok(())
    }
}

/// Trim and check a key as typed. `Ok(None)` for an empty input (free mode).
pub fn normalize_key(raw: &str) -> Result<Option<String>, String> {
    let key: String = raw.trim().to_string();
    if key.is_empty() {
        return Ok(None);
    }
    let valid_chars = key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid_chars || !(8..=64).contains(&key.len()) {
        return Err("This does not look like an Intelephense licence key.".into());
    }
    Ok(Some(key))
}

/// `…1234`: the last four characters, for display.
pub fn masked(key: &str) -> String {
    let tail: String = key
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

/// Set `licenceKey` in Intelephense's initialization options for a valid key, and remove
/// it otherwise: free mode never sends an empty or malformed key.
pub fn apply_licence(initialization_options: &mut Value, key: Option<&str>) {
    let key = key.and_then(|k| normalize_key(k).ok().flatten());
    if !initialization_options.is_object() {
        if key.is_none() {
            return;
        }
        *initialization_options = Value::Object(Default::default());
    }
    let obj = initialization_options
        .as_object_mut()
        .expect("made an object above");
    match key {
        Some(k) => {
            obj.insert("licenceKey".into(), Value::String(k));
        }
        None => {
            obj.remove("licenceKey");
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenceStatus {
    pub premium: bool,
    /// `…1234` when premium.
    pub masked_key: Option<String>,
    /// Keychain problem (e.g. no Secret Service on a headless Linux).
    pub error: Option<String>,
    pub purchase_url: String,
}

/// Licence handling on top of a [`SecretStore`].
pub struct Licence<'a> {
    store: &'a dyn SecretStore,
}

impl<'a> Licence<'a> {
    pub fn new(store: &'a dyn SecretStore) -> Self {
        Licence { store }
    }

    /// The stored key, if valid.
    pub fn key(&self) -> Result<Option<String>, String> {
        Ok(self
            .store
            .get(INTELEPHENSE_ACCOUNT)?
            .and_then(|k| normalize_key(&k).ok().flatten()))
    }

    pub fn status(&self) -> LicenceStatus {
        let (key, error) = match self.key() {
            Ok(k) => (k, None),
            Err(e) => (None, Some(e)),
        };
        LicenceStatus {
            premium: key.is_some(),
            masked_key: key.as_deref().map(masked),
            error,
            purchase_url: INTELEPHENSE_URL.into(),
        }
    }

    /// Store a new key (`None` or empty removes it). Returns whether the stored key
    /// changed, i.e. whether Intelephense must restart.
    pub fn set(&self, raw: Option<&str>) -> Result<bool, String> {
        let key = match raw {
            Some(r) => normalize_key(r)?,
            None => None,
        };
        let before = self.key().unwrap_or(None);
        match &key {
            Some(k) => self.store.set(INTELEPHENSE_ACCOUNT, k)?,
            None => self.store.delete(INTELEPHENSE_ACCOUNT)?,
        }
        Ok(before != key)
    }

    /// Move a plaintext key (old settings value) into the keychain unless one is stored
    /// already. Returns `true` when the caller should now clear the plaintext value.
    pub fn migrate(&self, plaintext: Option<&str>) -> Result<bool, String> {
        let Some(raw) = plaintext.filter(|p| !p.trim().is_empty()) else {
            return Ok(false);
        };
        if self.key()?.is_none() {
            if let Ok(Some(key)) = normalize_key(raw) {
                self.store.set(INTELEPHENSE_ACCOUNT, &key)?;
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn free_mode_never_sends_a_key() {
        let mut init = json!({"storagePath": "/tmp", "licenceKey": "OLDKEY1234"});
        apply_licence(&mut init, None);
        assert!(init.get("licenceKey").is_none());
        apply_licence(&mut init, Some("   "));
        assert!(init.get("licenceKey").is_none());
        apply_licence(&mut init, Some("bad key!"));
        assert!(init.get("licenceKey").is_none());
        apply_licence(&mut init, Some(" ABCDEFGH12345 "));
        assert_eq!(init["licenceKey"], "ABCDEFGH12345");
        assert_eq!(init["storagePath"], "/tmp");

        let mut null = Value::Null;
        apply_licence(&mut null, None);
        assert_eq!(null, Value::Null);
        apply_licence(&mut null, Some("ABCDEFGH12345"));
        assert_eq!(null, json!({"licenceKey": "ABCDEFGH12345"}));
    }

    #[test]
    fn keys_are_validated_and_masked() {
        assert_eq!(normalize_key("  "), Ok(None));
        assert_eq!(
            normalize_key(" 00AB-CD_1234 "),
            Ok(Some("00AB-CD_1234".into()))
        );
        assert!(normalize_key("short").is_err());
        assert!(normalize_key("has spaces inside").is_err());
        assert_eq!(masked("ABCDEFGH1234"), "…1234");
        assert_eq!(masked("AB"), "…AB");
    }

    #[test]
    fn set_reports_changes_and_status_follows() {
        let store = MemoryStore::default();
        let l = Licence::new(&store);
        assert_eq!(
            l.status(),
            LicenceStatus {
                premium: false,
                masked_key: None,
                error: None,
                purchase_url: INTELEPHENSE_URL.into()
            }
        );
        assert!(l.set(Some("ABCDEFGH1234")).unwrap());
        assert!(
            !l.set(Some(" ABCDEFGH1234 ")).unwrap(),
            "same key: no restart"
        );
        let s = l.status();
        assert!(s.premium);
        assert_eq!(s.masked_key.as_deref(), Some("…1234"));
        assert_eq!(
            store.get(INTELEPHENSE_ACCOUNT).unwrap().as_deref(),
            Some("ABCDEFGH1234")
        );
        assert!(l.set(Some("x y")).is_err());
        assert!(l.set(None).unwrap());
        assert!(!l.status().premium);
    }

    #[test]
    fn migrates_plaintext_keys_once() {
        let store = MemoryStore::default();
        let l = Licence::new(&store);
        assert!(!l.migrate(None).unwrap());
        assert!(!l.migrate(Some("")).unwrap());
        assert!(l.migrate(Some("PLAINKEY0001")).unwrap());
        assert_eq!(l.key().unwrap().as_deref(), Some("PLAINKEY0001"));
        // An existing keychain entry is kept; the plaintext is still cleared.
        assert!(l.migrate(Some("OTHERKEY0002")).unwrap());
        assert_eq!(l.key().unwrap().as_deref(), Some("PLAINKEY0001"));
    }
}
