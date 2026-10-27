//! Jupyter server tokens in the OS keychain (service `forge-jupyter`, account
//! `server:<base URL>`). The server URL itself is a setting; only the token is secret.

const SERVICE: &str = "forge-jupyter";

fn entry(base: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, &format!("server:{}", base.trim_end_matches('/')))
        .map_err(|e| format!("Keychain unavailable: {e}"))
}

pub fn store_token(base: &str, token: &str) -> Result<(), String> {
    entry(base)?
        .set_password(token)
        .map_err(|e| format!("Cannot store the token in the keychain: {e}"))
}

pub fn token(base: &str) -> Option<String> {
    entry(base).ok()?.get_password().ok()
}

pub fn delete_token(base: &str) -> Result<(), String> {
    match entry(base)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Cannot remove the token from the keychain: {e}")),
    }
}
