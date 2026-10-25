//! Opening a server of any type: SFTP through a saved SSH configuration, FTP/FTPS with the
//! password from the keychain (or the one typed into the form), local folders directly.

use std::path::{Path, PathBuf};
use std::time::Duration;

use forge_remote::session::ConnectParams;
use serde::Serialize;

use crate::config::{DeployServer, ServerKind};
use crate::fs::ServerFs;
use crate::ftp::{FtpFs, FtpParams};
use crate::local::LocalFolder;
use crate::sftp::SftpFs;
use crate::{secrets, DeployError, Result};

const TIMEOUT: Duration = Duration::from_secs(15);

/// An open server.
pub struct Opened {
    pub fs: Box<dyn ServerFs>,
    /// The absolute root folder (on the server, or on this machine for local servers).
    pub root: String,
    /// The server's greeting, if it sent one (FTP).
    pub welcome: Option<String>,
    /// A local server's folder relative to the project, when it is inside the project
    /// (never uploaded into itself).
    pub inside_project: Option<String>,
}

/// SSH parameters of a saved SSH configuration, with its stored secret unless `secret`
/// overrides it.
pub fn ssh_params(config_id: &str, secret: Option<&str>) -> Result<ConnectParams> {
    let all = forge_remote::store::load();
    let config = all.get(config_id).ok_or_else(|| {
        DeployError::new(format!(
            "The SSH configuration {config_id} does not exist anymore. Select another one."
        ))
    })?;
    let home = dirs::home_dir().unwrap_or_default();
    let ssh_config = forge_remote::store::read_ssh_config(&home);
    let mut params = ConnectParams::for_configuration(
        config,
        &ssh_config,
        &home,
        &forge_remote::store::local_user(),
    );
    params.secret = secret
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| forge_remote::store::secret(config_id));
    Ok(params)
}

/// A local server's folder: absolute, or relative to the project.
pub fn local_root(server: &DeployServer, project: Option<&Path>) -> PathBuf {
    let folder = PathBuf::from(server.root_path.trim());
    match project {
        Some(p) if folder.is_relative() => p.join(folder),
        _ => folder,
    }
}

/// Connect to a server. `secret` overrides the stored password (the form's unsaved one).
pub fn open(server: &DeployServer, secret: Option<&str>, project: Option<&Path>) -> Result<Opened> {
    match server.kind {
        ServerKind::Local => {
            let root = local_root(server, project);
            if !root.is_dir() {
                return Err(DeployError::new(format!(
                    "The folder {} does not exist",
                    root.display()
                )));
            }
            let canonical = root.canonicalize()?;
            let inside_project = match project.and_then(|p| p.canonicalize().ok()) {
                Some(p) if p == canonical => {
                    return Err(DeployError::new(
                        "The deployment folder is the project folder itself",
                    ))
                }
                Some(p) => canonical
                    .strip_prefix(&p)
                    .ok()
                    .map(|rel| rel.to_string_lossy().replace('\\', "/")),
                None => None,
            };
            Ok(Opened {
                root: canonical.display().to_string(),
                fs: Box::new(LocalFolder::new(canonical)),
                welcome: None,
                inside_project,
            })
        }
        ServerKind::Ftp | ServerKind::Ftps => {
            let password = secret
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| secrets::get(server))
                .unwrap_or_default();
            let fs = FtpFs::connect(&FtpParams {
                host: server.host.trim().to_string(),
                port: server.ftp_port(),
                user: (!server.anonymous).then(|| server.user.trim().to_string()),
                password,
                secure: server.kind == ServerKind::Ftps,
                passive: server.passive,
                root: server.root_path.clone(),
                timeout: TIMEOUT,
            })?;
            Ok(Opened {
                root: fs.root().to_string(),
                welcome: fs.welcome().map(str::to_string),
                fs: Box::new(fs),
                inside_project: None,
            })
        }
        ServerKind::Sftp => {
            let params = ssh_params(&server.ssh_config, secret)?;
            let fs = SftpFs::connect(&params, &server.root_path)?;
            Ok(Opened {
                root: fs.root().to_string(),
                fs: Box::new(fs),
                welcome: None,
                inside_project: None,
            })
        }
    }
}

/// The result of Test Connection.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub ok: bool,
    pub message: String,
    /// The root folder the server resolved (the login folder for an empty root path).
    pub root: Option<String>,
}

/// Test Connection: connect and check the root path.
pub fn test(server: &DeployServer, secret: Option<&str>, project: Option<&Path>) -> TestResult {
    match open(server, secret, project) {
        Err(e) => TestResult {
            ok: false,
            message: e.to_string(),
            root: None,
        },
        Ok(mut opened) => match opened.fs.stat("") {
            Ok(Some(e)) if e.dir => TestResult {
                ok: true,
                message: format!("Connected to {}", server.label()),
                root: Some(opened.root),
            },
            Ok(_) => TestResult {
                ok: false,
                message: format!(
                    "Connected, but the root path {} does not exist",
                    opened.root
                ),
                root: Some(opened.root),
            },
            Err(e) => TestResult {
                ok: false,
                message: e.to_string(),
                root: Some(opened.root),
            },
        },
    }
}
