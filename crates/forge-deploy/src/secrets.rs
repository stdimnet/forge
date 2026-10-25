//! FTP/FTPS passwords in the OS keychain (service `forge-deploy`, account
//! `ftp://user@host:port`, so projects deploying with the same login share it). SFTP
//! servers use their SSH configuration's keychain entry (service `forge-ssh`).

use crate::config::{DeployServer, ServerKind};
use crate::{DeployError, Result};

const KEYRING_SERVICE: &str = "forge-deploy";

/// The keychain account of a server's password, `None` for servers without one.
pub fn account(server: &DeployServer) -> Option<String> {
    (server.kind.is_ftp() && !server.anonymous).then(|| {
        format!(
            "ftp://{}@{}:{}",
            server.user.trim(),
            server.host.trim(),
            server.ftp_port()
        )
    })
}

fn other(e: impl std::fmt::Display) -> DeployError {
    DeployError::context("Keychain", e)
}

/// The stored password of a server.
pub fn get(server: &DeployServer) -> Option<String> {
    let account = account(server)?;
    keyring::Entry::new(KEYRING_SERVICE, &account)
        .ok()?
        .get_password()
        .ok()
}

/// Store (or with `None` / an empty text delete) the password of a server.
pub fn set(server: &DeployServer, password: Option<&str>) -> Result<()> {
    let Some(account) = account(server) else {
        return Ok(());
    };
    let entry = keyring::Entry::new(KEYRING_SERVICE, &account).map_err(other)?;
    match password {
        Some(p) if !p.is_empty() => entry.set_password(p).map_err(other),
        _ => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(other(e)),
        },
    }
}

/// Whether the server has a password in the keychain (SFTP: its SSH configuration).
pub fn has_secret(server: &DeployServer) -> bool {
    match server.kind {
        ServerKind::Sftp => forge_remote::store::secret(&server.ssh_config).is_some(),
        ServerKind::Local => false,
        _ => get(server).is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accounts_name_the_login() {
        let ftp = DeployServer {
            kind: ServerKind::Ftps,
            host: " ftp.example.com ".into(),
            user: "deploy".into(),
            port: Some(2121),
            ..DeployServer::default()
        };
        assert_eq!(
            account(&ftp).as_deref(),
            Some("ftp://deploy@ftp.example.com:2121")
        );
        let anonymous = DeployServer {
            anonymous: true,
            ..ftp.clone()
        };
        assert_eq!(account(&anonymous), None);
        assert_eq!(account(&DeployServer::default()), None);
        // Servers without a password never touch the keychain.
        set(&anonymous, Some("x")).unwrap();
    }
}
