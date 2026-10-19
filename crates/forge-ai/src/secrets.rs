//! API key storage: the OS keychain (service `forge-ai`, account = provider id) with an
//! environment-variable fallback per provider kind.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;

use crate::config::ProviderConfig;
use crate::error::{AiError, AiResult};

pub const KEYCHAIN_SERVICE: &str = "forge-ai";

pub trait SecretStore: Send + Sync {
    fn get(&self, provider_id: &str) -> AiResult<Option<String>>;
    fn set(&self, provider_id: &str, secret: &str) -> AiResult<()>;
    fn delete(&self, provider_id: &str) -> AiResult<()>;
}

/// The platform keychain (macOS Keychain, Windows Credential Manager, Secret Service).
#[derive(Debug, Default)]
pub struct KeyringStore;

impl KeyringStore {
    fn entry(provider_id: &str) -> AiResult<keyring::Entry> {
        keyring::Entry::new(KEYCHAIN_SERVICE, provider_id)
            .map_err(|e| AiError::Keychain(e.to_string()))
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, provider_id: &str) -> AiResult<Option<String>> {
        match Self::entry(provider_id)?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AiError::Keychain(e.to_string())),
        }
    }

    fn set(&self, provider_id: &str, secret: &str) -> AiResult<()> {
        Self::entry(provider_id)?
            .set_password(secret)
            .map_err(|e| AiError::Keychain(e.to_string()))
    }

    fn delete(&self, provider_id: &str) -> AiResult<()> {
        match Self::entry(provider_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AiError::Keychain(e.to_string())),
        }
    }
}

/// In-memory store for tests and environments without a keychain.
#[derive(Debug, Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, provider_id: &str) -> AiResult<Option<String>> {
        Ok(self.0.lock().expect("poisoned").get(provider_id).cloned())
    }

    fn set(&self, provider_id: &str, secret: &str) -> AiResult<()> {
        self.0
            .lock()
            .expect("poisoned")
            .insert(provider_id.into(), secret.into());
        Ok(())
    }

    fn delete(&self, provider_id: &str) -> AiResult<()> {
        self.0.lock().expect("poisoned").remove(provider_id);
        Ok(())
    }
}

/// Where a provider's key comes from (shown in the UI, never the key itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeySource {
    Keychain,
    Env,
    None,
}

/// Look up a provider's key: keychain first, then the kind's environment variable.
/// Keychain failures (e.g. no Secret Service on a headless Linux) fall through to env.
pub fn resolve_api_key(
    store: &dyn SecretStore,
    cfg: &ProviderConfig,
    env: impl Fn(&str) -> Option<String>,
) -> (Option<String>, KeySource) {
    match store.get(&cfg.id) {
        Ok(Some(k)) if !k.is_empty() => return (Some(k), KeySource::Keychain),
        Ok(_) => {}
        Err(e) => tracing::warn!("keychain lookup for {} failed: {e}", cfg.id),
    }
    if let Some(k) = cfg.kind.env_var().and_then(&env).filter(|k| !k.is_empty()) {
        return (Some(k), KeySource::Env);
    }
    (None, KeySource::None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderKind;

    #[test]
    fn keychain_wins_over_env_and_env_is_fallback() {
        let store = MemoryStore::default();
        let cfg = ProviderConfig::preset(ProviderKind::Anthropic, "anthropic");
        let env = |k: &str| (k == "ANTHROPIC_API_KEY").then(|| "env-key".to_string());
        assert_eq!(
            resolve_api_key(&store, &cfg, env),
            (Some("env-key".into()), KeySource::Env)
        );
        store.set("anthropic", "kc-key").unwrap();
        assert_eq!(
            resolve_api_key(&store, &cfg, env),
            (Some("kc-key".into()), KeySource::Keychain)
        );
        let local = ProviderConfig::preset(ProviderKind::Ollama, "ollama");
        assert_eq!(
            resolve_api_key(&store, &local, env),
            (None, KeySource::None)
        );
    }
}
