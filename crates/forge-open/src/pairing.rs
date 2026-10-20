//! Pairing browsers with Forge.
//!
//! "Pair New Browser" shows a short-lived 6-digit code. The extension sends it once to
//! `POST /api/forge/pair` and receives a random 256-bit token that authorizes every later
//! request. Forge keeps only the SHA-256 hash of each token (in
//! `<config dir>/forge/browser-companion.json`), so the file holds no usable secret; the
//! token itself lives only in the browser's extension storage.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// How long a pairing code stays valid.
pub const CODE_TTL_SECS: u64 = 300;
/// Wrong guesses before the code is burned.
pub const MAX_ATTEMPTS: u32 = 5;
/// `last_used_at` is persisted at most this often per client.
const LAST_USED_GRANULARITY_SECS: u64 = 60;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PairError {
    #[error("no pairing in progress; click \"Pair New Browser\" in Forge first")]
    NotPairing,
    #[error("the pairing code has expired")]
    Expired,
    #[error("wrong pairing code")]
    WrongCode,
    #[error("too many wrong codes; start pairing again in Forge")]
    TooManyAttempts,
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    getrandom::getrandom(&mut buf).expect("the OS random number generator is unavailable");
    buf
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hash_token(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

/// Compare without an early exit, so timing does not reveal matching prefixes.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A paired browser as persisted (token hash only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedClient {
    pub id: String,
    pub name: String,
    /// `chrome-extension://…` origin seen while pairing; later requests from another
    /// extension origin are refused even with the right token.
    #[serde(default)]
    pub origin: Option<String>,
    pub token_hash: String,
    pub created_at: u64,
    #[serde(default)]
    pub last_used_at: Option<u64>,
}

/// What the UI shows about a paired browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub id: String,
    pub name: String,
    pub origin: Option<String>,
    pub created_at: u64,
    pub last_used_at: Option<u64>,
}

impl From<&PairedClient> for ClientInfo {
    fn from(c: &PairedClient) -> Self {
        Self {
            id: c.id.clone(),
            name: c.name.clone(),
            origin: c.origin.clone(),
            created_at: c.created_at,
            last_used_at: c.last_used_at,
        }
    }
}

/// The code currently shown in Forge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingCode {
    /// Six digits, shown as `123 456`.
    pub code: String,
    pub expires_at: u64,
    #[serde(skip)]
    attempts: u32,
}

/// Result of a successful pairing: the token goes to the browser once and is never stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Paired {
    pub client_id: String,
    pub token: String,
}

#[derive(Debug, Default)]
pub struct Pairing {
    path: Option<PathBuf>,
    clients: Vec<PairedClient>,
    pending: Option<PairingCode>,
}

#[derive(Serialize, Deserialize, Default)]
struct StoreFile {
    #[serde(default)]
    clients: Vec<PairedClient>,
}

impl Pairing {
    /// In-memory store (tests, browser mode).
    pub fn in_memory() -> Self {
        Self::default()
    }

    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("forge").join("browser-companion.json"))
    }

    /// Load the paired clients from `path`; a missing or broken file means "no clients".
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let clients = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<StoreFile>(&s).ok())
            .map(|f| f.clients)
            .unwrap_or_default();
        Self {
            path: Some(path),
            clients,
            pending: None,
        }
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        let write = |path: &Path| -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let json = serde_json::to_string_pretty(&StoreFile {
                clients: self.clients.clone(),
            })?;
            std::fs::write(path, json)
        };
        if let Err(e) = write(path) {
            tracing::warn!("cannot save paired browsers to {}: {e}", path.display());
        }
    }

    pub fn clients(&self) -> Vec<ClientInfo> {
        self.clients.iter().map(ClientInfo::from).collect()
    }

    /// The active code, if it has not expired.
    pub fn pending(&self, now: u64) -> Option<&PairingCode> {
        self.pending
            .as_ref()
            .filter(|p| p.expires_at > now && p.attempts < MAX_ATTEMPTS)
    }

    /// Start pairing: a fresh code replaces any previous one.
    pub fn start(&mut self, now: u64) -> PairingCode {
        let n = u32::from_le_bytes(random_bytes::<4>()) % 1_000_000;
        let code = PairingCode {
            code: format!("{n:06}"),
            expires_at: now + CODE_TTL_SECS,
            attempts: 0,
        };
        self.pending = Some(code.clone());
        code
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    /// Exchange the code for a token. The code works once.
    pub fn pair(
        &mut self,
        code: &str,
        name: &str,
        origin: Option<&str>,
        now: u64,
    ) -> Result<Paired, PairError> {
        let pending = self.pending.as_mut().ok_or(PairError::NotPairing)?;
        if pending.attempts >= MAX_ATTEMPTS {
            return Err(PairError::TooManyAttempts);
        }
        if pending.expires_at <= now {
            self.pending = None;
            return Err(PairError::Expired);
        }
        let given: String = code.chars().filter(|c| c.is_ascii_digit()).collect();
        if !constant_time_eq(given.as_bytes(), pending.code.as_bytes()) {
            pending.attempts += 1;
            return Err(if pending.attempts >= MAX_ATTEMPTS {
                PairError::TooManyAttempts
            } else {
                PairError::WrongCode
            });
        }
        self.pending = None;

        let token = hex(&random_bytes::<32>());
        let client_id = hex(&random_bytes::<8>());
        let name: String = name.trim().chars().take(80).collect();
        self.clients.push(PairedClient {
            id: client_id.clone(),
            name: if name.is_empty() {
                "Browser".into()
            } else {
                name
            },
            origin: origin.map(str::to_string),
            token_hash: hash_token(&token),
            created_at: now,
            last_used_at: Some(now),
        });
        self.save();
        Ok(Paired { client_id, token })
    }

    /// The client a token belongs to. A client paired from an extension origin only
    /// accepts requests from that origin (or without one, e.g. `curl`).
    pub fn verify(&mut self, token: &str, origin: Option<&str>, now: u64) -> Option<ClientInfo> {
        let hash = hash_token(token);
        let client = self
            .clients
            .iter_mut()
            .find(|c| constant_time_eq(c.token_hash.as_bytes(), hash.as_bytes()))?;
        if let (Some(expected), Some(got)) = (&client.origin, origin) {
            if expected != got {
                return None;
            }
        }
        let stale = client
            .last_used_at
            .is_none_or(|t| now >= t + LAST_USED_GRANULARITY_SECS);
        let info = if stale {
            client.last_used_at = Some(now);
            let info = ClientInfo::from(&*client);
            self.save();
            info
        } else {
            ClientInfo::from(&*client)
        };
        Some(info)
    }

    /// Forget a paired browser; its token stops working immediately.
    pub fn revoke(&mut self, id: &str) -> bool {
        let before = self.clients.len();
        self.clients.retain(|c| c.id != id);
        let removed = self.clients.len() != before;
        if removed {
            self.save();
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_once_with_the_right_code() {
        let mut p = Pairing::in_memory();
        assert_eq!(p.pair("123456", "x", None, 0), Err(PairError::NotPairing));

        let code = p.start(1000);
        assert_eq!(code.code.len(), 6);
        assert_eq!(code.expires_at, 1000 + CODE_TTL_SECS);
        // Spaces as shown in the UI are fine.
        let spaced = format!("{} {}", &code.code[..3], &code.code[3..]);
        let paired = p
            .pair(
                &spaced,
                "Chrome on Linux",
                Some("chrome-extension://abc"),
                1010,
            )
            .unwrap();
        assert_eq!(paired.token.len(), 64);
        assert!(p.pending(1010).is_none(), "codes work once");
        assert_eq!(
            p.pair(&code.code, "again", None, 1011),
            Err(PairError::NotPairing)
        );

        let clients = p.clients();
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].name, "Chrome on Linux");
    }

    #[test]
    fn wrong_and_expired_codes() {
        let mut p = Pairing::in_memory();
        let code = p.start(0);
        let wrong = if code.code == "000000" {
            "000001"
        } else {
            "000000"
        };
        for _ in 0..MAX_ATTEMPTS - 1 {
            assert_eq!(p.pair(wrong, "x", None, 1), Err(PairError::WrongCode));
        }
        assert_eq!(p.pair(wrong, "x", None, 1), Err(PairError::TooManyAttempts));
        assert_eq!(
            p.pair(&code.code, "x", None, 1),
            Err(PairError::TooManyAttempts),
            "a burned code stays burned"
        );

        let code = p.start(0);
        assert_eq!(
            p.pair(&code.code, "x", None, CODE_TTL_SECS),
            Err(PairError::Expired)
        );
    }

    #[test]
    fn verifies_tokens_and_origins() {
        let mut p = Pairing::in_memory();
        let code = p.start(0);
        let paired = p
            .pair(&code.code, "Chrome", Some("chrome-extension://abc"), 0)
            .unwrap();
        assert!(p
            .verify(&paired.token, Some("chrome-extension://abc"), 5)
            .is_some());
        assert!(p.verify(&paired.token, None, 5).is_some());
        assert!(p
            .verify(&paired.token, Some("chrome-extension://evil"), 5)
            .is_none());
        assert!(p.verify("nope", None, 5).is_none());

        assert!(p.revoke(&paired.client_id));
        assert!(p.verify(&paired.token, None, 6).is_none());
    }

    #[test]
    fn persists_only_token_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forge/browser-companion.json");
        let mut p = Pairing::load(&path);
        let code = p.start(0);
        let paired = p.pair(&code.code, "Firefox", None, 0).unwrap();

        let stored = std::fs::read_to_string(&path).unwrap();
        assert!(!stored.contains(&paired.token));
        assert!(stored.contains(&hash_token(&paired.token)));

        let mut reloaded = Pairing::load(&path);
        assert_eq!(reloaded.clients().len(), 1);
        assert!(reloaded.verify(&paired.token, None, 100).is_some());
    }
}
