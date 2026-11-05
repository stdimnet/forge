//! The access token of a private marketplace in the OS keychain (service `forge-plugins`,
//! account `marketplace:<scope>`, where the scope is the index's host, see
//! [`crate::marketplace::token_scope`]). It is never written to the settings, and a token
//! stored for one marketplace is not used for another one.

use crate::marketplace::token_scope;

const SERVICE: &str = "forge-plugins";

fn entry(index_url: &str) -> Result<keyring::Entry, String> {
    let scope = token_scope(index_url).ok_or_else(|| format!("invalid index URL {index_url}"))?;
    keyring::Entry::new(SERVICE, &format!("marketplace:{scope}"))
        .map_err(|e| format!("Keychain unavailable: {e}"))
}

/// The token stored for the marketplace at `index_url`.
pub fn marketplace_token(index_url: &str) -> Option<String> {
    entry(index_url)
        .ok()?
        .get_password()
        .ok()
        .filter(|t| !t.trim().is_empty())
}

/// Store (or with `None` / an empty token, remove) the token for `index_url`.
pub fn set_marketplace_token(index_url: &str, token: Option<&str>) -> Result<(), String> {
    let entry = entry(index_url)?;
    match token.map(str::trim).filter(|t| !t.is_empty()) {
        Some(token) => entry
            .set_password(token)
            .map_err(|e| format!("Cannot store the token in the keychain: {e}")),
        None => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("Cannot remove the token from the keychain: {e}")),
        },
    }
}
