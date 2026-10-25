//! SFTP servers: the `sftp` subsystem (russh-sftp, Apache-2.0) on an SSH session of
//! forge-remote, so authentication, `~/.ssh/config` aliases, jump hosts and the
//! `known_hosts` check are the ones of remote development.

use std::sync::Arc;

use forge_remote::session::{ConnectError, ConnectParams, SshSession};
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, StatusCode};
use tokio::io::AsyncWriteExt;

use crate::compare::Entry;
use crate::fs::ServerFs;
use crate::mapping::{join, normalize};
use crate::{DeployError, Result};

pub struct SftpFs {
    // Field order: the SFTP channel closes before the session and its runtime.
    sftp: SftpSession,
    session: Arc<SshSession>,
    /// Drives the SSH session between calls (one worker thread).
    rt: tokio::runtime::Runtime,
    /// Absolute root folder, without a trailing slash (`""` is `/`).
    root: String,
    label: String,
}

/// A connection error for the user; host keys must be trusted in SSH Configurations.
pub fn connect_error(e: ConnectError, label: &str) -> DeployError {
    match e {
        ConnectError::HostKey { host_key } => DeployError::new(format!(
            "The host key of {label} is not trusted yet ({} {}). Use Test Connection in \
             Settings | Tools | SSH Configurations to check and trust it.",
            host_key.key_type, host_key.fingerprint
        )),
        other => DeployError::new(other.to_string()),
    }
}

fn sftp_err(context: impl std::fmt::Display, e: SftpError) -> DeployError {
    DeployError::context(context, e)
}

fn is_missing(e: &SftpError) -> bool {
    matches!(e, SftpError::Status(s) if s.status_code == StatusCode::NoSuchFile)
}

fn to_entry(path: String, attrs: &FileAttributes) -> Entry {
    let modified = attrs.mtime.map(i64::from);
    if attrs.is_dir() {
        Entry::dir(path, modified)
    } else {
        Entry::file(path, attrs.size.unwrap_or(0), modified)
    }
}

impl SftpFs {
    /// Connect with `params` (from a saved SSH configuration) and open SFTP; an empty
    /// `root` is the login folder.
    pub fn connect(params: &ConnectParams, root: &str) -> Result<Self> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()?;
        let label = format!("{}@{}", params.user, params.host);
        let root = root.trim().replace('\\', "/");
        let (session, sftp, root) = rt.block_on(async {
            let session = SshSession::connect(params)
                .await
                .map_err(|e| connect_error(e, &label))?;
            let stream = session
                .open_subsystem("sftp")
                .await
                .map_err(|e| connect_error(e, &label))?;
            let sftp = SftpSession::new(stream)
                .await
                .map_err(|e| sftp_err(format!("SFTP is not available on {label}"), e))?;
            let root = if root.is_empty() {
                sftp.canonicalize(".")
                    .await
                    .map_err(|e| sftp_err("Cannot find the login folder", e))?
            } else {
                root
            };
            Ok::<_, DeployError>((Arc::new(session), sftp, root))
        })?;
        let root = format!("/{}", normalize(&root));
        Ok(Self {
            sftp,
            session,
            rt,
            root: root.trim_end_matches('/').to_string(),
            label: format!("sftp://{label}"),
        })
    }

    /// The absolute root folder on the server.
    pub fn root(&self) -> &str {
        if self.root.is_empty() {
            "/"
        } else {
            &self.root
        }
    }

    fn full(&self, path: &str) -> String {
        let path = normalize(path);
        if path.is_empty() {
            self.root().to_string()
        } else {
            format!("{}/{path}", self.root)
        }
    }
}

impl ServerFs for SftpFs {
    fn stat(&mut self, path: &str) -> Result<Option<Entry>> {
        let full = self.full(path);
        match self.rt.block_on(self.sftp.metadata(full.clone())) {
            Ok(attrs) => Ok(Some(to_entry(normalize(path), &attrs))),
            Err(e) if is_missing(&e) => Ok(None),
            Err(e) => Err(sftp_err(full, e)),
        }
    }

    fn list(&mut self, dir: &str) -> Result<Vec<Entry>> {
        let dir = normalize(dir);
        let full = self.full(&dir);
        let read = self
            .rt
            .block_on(self.sftp.read_dir(full.clone()))
            .map_err(|e| sftp_err(format!("Cannot list {full}"), e))?;
        let mut out: Vec<Entry> = read
            .filter(|e| !matches!(e.file_name().as_str(), "." | ".."))
            .map(|e| to_entry(join(&dir, &e.file_name()), &e.metadata()))
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        let full = self.full(path);
        self.rt
            .block_on(self.sftp.read(full.clone()))
            .map_err(|e| sftp_err(format!("Cannot download {full}"), e))
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<()> {
        let full = self.full(path);
        let sftp = &self.sftp;
        self.rt
            .block_on(async {
                let mut file = sftp.create(full.clone()).await?;
                file.write_all(data).await?;
                file.shutdown().await?;
                Ok::<_, SftpError>(())
            })
            .map_err(|e| sftp_err(format!("Cannot upload {full}"), e))
    }

    fn mkdir(&mut self, path: &str) -> Result<()> {
        let full = self.full(path);
        match self.rt.block_on(self.sftp.create_dir(full.clone())) {
            Ok(()) => Ok(()),
            Err(e) => match self.stat(path)? {
                Some(existing) if existing.dir => Ok(()),
                _ => Err(sftp_err(format!("Cannot create {full}"), e)),
            },
        }
    }

    fn remove_file(&mut self, path: &str) -> Result<()> {
        let full = self.full(path);
        self.rt
            .block_on(self.sftp.remove_file(full.clone()))
            .map_err(|e| sftp_err(format!("Cannot delete {full}"), e))
    }

    fn remove_dir(&mut self, path: &str) -> Result<()> {
        let full = self.full(path);
        self.rt
            .block_on(self.sftp.remove_dir(full.clone()))
            .map_err(|e| sftp_err(format!("Cannot delete {full}"), e))
    }

    fn set_modified(&mut self, path: &str, secs: i64) -> Result<bool> {
        let full = self.full(path);
        let Ok(secs) = u32::try_from(secs) else {
            return Ok(false);
        };
        let attrs = FileAttributes {
            atime: Some(secs),
            mtime: Some(secs),
            ..FileAttributes::empty()
        };
        Ok(self
            .rt
            .block_on(self.sftp.set_metadata(full, attrs))
            .is_ok())
    }

    fn describe(&self, path: &str) -> String {
        format!("{}{}", self.label, self.full(path))
    }
}

impl Drop for SftpFs {
    fn drop(&mut self) {
        let _ = self.rt.block_on(self.sftp.close());
        let session = self.session.clone();
        self.rt.block_on(session.disconnect());
    }
}
