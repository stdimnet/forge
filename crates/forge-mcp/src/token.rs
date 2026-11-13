//! The bearer token in the OS keychain (service `forge-mcp`), so client configurations keep
//! working across restarts. Without a keychain the IDE makes a new token per start.

use crate::server::new_token;

pub const KEYCHAIN_SERVICE: &str = "forge-mcp";
const ACCOUNT: &str = "server-token";

fn entry() -> Option<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, ACCOUNT).ok()
}

/// The stored token, if the keychain has one.
pub fn stored() -> Option<String> {
    entry()?.get_password().ok().filter(|t| !t.is_empty())
}

/// The stored token, or a new one (stored when the keychain works). The flag says whether
/// it is persistent.
pub fn load_or_create() -> (String, bool) {
    if let Some(t) = stored() {
        return (t, true);
    }
    regenerate()
}

/// Replace the token (old client configurations stop working).
pub fn regenerate() -> (String, bool) {
    let token = new_token();
    let saved = entry().is_some_and(|e| e.set_password(&token).is_ok());
    (token, saved)
}
