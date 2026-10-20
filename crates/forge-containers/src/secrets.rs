//! Docker registry credentials in the OS keychain (service `forge-containers`, account
//! `registry:<server>`). The list of servers and user names is not secret and is passed in
//! by the caller (settings); only passwords and tokens go to the keychain.

use crate::{Error, Result};

const SERVICE: &str = "forge-containers";

fn entry(server: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, &format!("registry:{}", normalize_server(server)))
        .map_err(|e| Error(format!("Keychain unavailable: {e}")))
}

/// `https://index.docker.io/v1/`, `docker.io` and empty all mean Docker Hub.
pub fn normalize_server(server: &str) -> String {
    let s = server
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    match s {
        "" | "docker.io" | "index.docker.io" | "index.docker.io/v1" | "registry-1.docker.io" => {
            "docker.io".into()
        }
        other => other.to_string(),
    }
}

/// Registry of an image reference (`ghcr.io/acme/app:1` → `ghcr.io`, `mysql` → `docker.io`).
pub fn registry_of(image: &str) -> String {
    match image.split_once('/') {
        Some((first, _)) if first.contains('.') || first.contains(':') || first == "localhost" => {
            first.to_string()
        }
        _ => "docker.io".into(),
    }
}

pub fn store_password(server: &str, password: &str) -> Result<()> {
    entry(server)?
        .set_password(password)
        .map_err(|e| Error(format!("Cannot store the password in the keychain: {e}")))
}

pub fn password(server: &str) -> Option<String> {
    entry(server).ok()?.get_password().ok()
}

pub fn delete_password(server: &str) -> Result<()> {
    match entry(server)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(Error(format!(
            "Cannot remove the password from the keychain: {e}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servers() {
        assert_eq!(normalize_server("https://index.docker.io/v1/"), "docker.io");
        assert_eq!(normalize_server("ghcr.io/"), "ghcr.io");
        assert_eq!(registry_of("mysql:8.4"), "docker.io");
        assert_eq!(registry_of("acme/app"), "docker.io");
        assert_eq!(registry_of("ghcr.io/acme/app:1"), "ghcr.io");
        assert_eq!(registry_of("localhost:5000/app"), "localhost:5000");
    }
}
