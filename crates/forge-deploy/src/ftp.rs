//! FTP and FTPS servers (suppaftp, MIT OR Apache-2.0; FTPS is explicit TLS with `AUTH TLS`
//! over rustls and the Mozilla root certificates). Listings use `MLSD` and fall back to
//! `LIST` for servers without it; timestamps are set with `MFMT` where the server has it.

use std::collections::HashMap;
use std::io::Cursor;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use suppaftp::list::{File, ListParser};
use suppaftp::types::FileType;
use suppaftp::{Mode, RustlsConnector, RustlsFtpStream, Status};

use crate::compare::Entry;
use crate::fs::ServerFs;
use crate::mapping::{join, name, normalize, parent};
use crate::{DeployError, Result};

/// How to reach an FTP server.
#[derive(Debug, Clone)]
pub struct FtpParams {
    pub host: String,
    pub port: u16,
    /// `None` logs in anonymously.
    pub user: Option<String>,
    pub password: String,
    /// FTPS: explicit TLS before logging in.
    pub secure: bool,
    pub passive: bool,
    /// Absolute folder on the server; empty for the login folder.
    pub root: String,
    pub timeout: Duration,
}

pub struct FtpFs {
    stream: RustlsFtpStream,
    /// Absolute root folder on the server, without a trailing slash (`""` is `/`).
    root: String,
    label: String,
    welcome: Option<String>,
    /// Whether `MLSD` works (`None` until tried).
    mlsd: Option<bool>,
    mfmt: bool,
    /// Folder listings of this session, dropped when the folder changes.
    cache: HashMap<String, Vec<Entry>>,
}

fn ftp_err(context: impl std::fmt::Display, e: suppaftp::FtpError) -> DeployError {
    let message = match &e {
        suppaftp::FtpError::UnexpectedResponse(r) => {
            let body = String::from_utf8_lossy(&r.body).trim().to_string();
            if body.is_empty() {
                format!("the server answered {}", r.status.code())
            } else {
                body
            }
        }
        other => other.to_string(),
    };
    DeployError::context(context, message)
}

fn tls_connector() -> Result<RustlsConnector> {
    use suppaftp::rustls;
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| DeployError::context("TLS", e))?
    .with_root_certificates(roots)
    .with_no_client_auth();
    Ok(RustlsConnector::from(Arc::new(config)))
}

/// `YYYYMMDDHHMMSS` in UTC (the `MFMT` argument).
pub fn ftp_timestamp(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn to_entry(dir: &str, f: &File) -> Entry {
    let modified = f
        .modified()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
        .filter(|s| *s > 0);
    let path = join(dir, f.name());
    if f.is_directory() {
        Entry::dir(path, modified)
    } else {
        Entry::file(path, f.size() as u64, modified)
    }
}

impl FtpFs {
    pub fn connect(p: &FtpParams) -> Result<Self> {
        let target = format!("{}:{}", p.host, p.port);
        let addr = (p.host.as_str(), p.port)
            .to_socket_addrs()
            .map_err(|e| DeployError::context(format!("Cannot resolve {}", p.host), e))?
            .next()
            .ok_or_else(|| DeployError::new(format!("Cannot resolve {}", p.host)))?;
        let mut stream = RustlsFtpStream::connect_timeout(addr, p.timeout)
            .map_err(|e| ftp_err(format!("Cannot connect to {target}"), e))?;
        if p.secure {
            stream = stream
                .into_secure(tls_connector()?, &p.host)
                .map_err(|e| ftp_err(format!("TLS with {target} failed"), e))?;
        }
        if p.passive {
            stream.set_mode(Mode::Passive);
            // Servers behind NAT announce their private address in PASV replies.
            stream.set_passive_nat_workaround(true);
        } else {
            stream = stream.active_mode(p.timeout);
        }
        let (user, password) = match &p.user {
            Some(u) => (u.clone(), p.password.clone()),
            None => ("anonymous".to_string(), "forge@".to_string()),
        };
        stream
            .login(user.as_str(), password.as_str())
            .map_err(|e| ftp_err(format!("Login to {target} as {user} failed"), e))?;
        stream
            .transfer_type(FileType::Binary)
            .map_err(|e| ftp_err("TYPE I", e))?;
        let welcome = stream.get_welcome_msg().map(|s| s.trim().to_string());
        let root = if p.root.trim().is_empty() {
            stream.pwd().map_err(|e| ftp_err("PWD", e))?
        } else {
            p.root.trim().replace('\\', "/")
        };
        let mfmt = stream
            .feat()
            .map(|f| f.keys().any(|k| k.eq_ignore_ascii_case("MFMT")))
            .unwrap_or(false);
        let root = format!("/{}", normalize(&root));
        Ok(Self {
            stream,
            root: root.trim_end_matches('/').to_string(),
            label: format!(
                "{}://{user}@{}",
                if p.secure { "ftps" } else { "ftp" },
                if p.port == 21 { p.host.clone() } else { target },
            ),
            welcome,
            mlsd: None,
            mfmt,
            cache: HashMap::new(),
        })
    }

    /// The server's greeting (`220 …`).
    pub fn welcome(&self) -> Option<&str> {
        self.welcome.as_deref()
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

    fn changed(&mut self, path: &str) {
        self.cache.remove(parent(&normalize(path)));
        self.cache.remove(&normalize(path));
    }

    fn read_dir(&mut self, dir: &str) -> Result<Vec<Entry>> {
        let full = self.full(dir);
        if self.mlsd != Some(false) {
            match self.stream.mlsd(Some(&full)) {
                Ok(lines) => {
                    self.mlsd = Some(true);
                    return Ok(lines
                        .iter()
                        .filter(|l| {
                            let l = l.to_ascii_lowercase();
                            !l.contains("type=cdir;") && !l.contains("type=pdir;")
                        })
                        .filter_map(|l| ListParser::parse_mlsd(l).ok())
                        .filter(|f| !matches!(f.name(), "." | "..") && !f.name().contains('/'))
                        .map(|f| to_entry(dir, &f))
                        .collect());
                }
                Err(suppaftp::FtpError::UnexpectedResponse(r))
                    if matches!(r.status, Status::BadCommand | Status::NotImplemented)
                        || self.mlsd.is_none() && r.status != Status::FileUnavailable =>
                {
                    self.mlsd = Some(false);
                }
                Err(e) => return Err(ftp_err(format!("Cannot list {full}"), e)),
            }
        }
        let lines = self
            .stream
            .list(Some(&full))
            .map_err(|e| ftp_err(format!("Cannot list {full}"), e))?;
        Ok(lines
            .iter()
            .filter_map(|l| {
                ListParser::parse_posix(l)
                    .or_else(|_| ListParser::parse_dos(l))
                    .ok()
            })
            .filter(|f| !matches!(f.name(), "." | ".."))
            .map(|f| to_entry(dir, &f))
            .collect())
    }
}

impl ServerFs for FtpFs {
    fn stat(&mut self, path: &str) -> Result<Option<Entry>> {
        let path = normalize(path);
        if path.is_empty() {
            let root = self.root().to_string();
            return Ok(self
                .stream
                .cwd(&root)
                .ok()
                .map(|_| Entry::dir(String::new(), None)));
        }
        let dir = parent(&path).to_string();
        let listing = match self.list(&dir) {
            Ok(l) => l,
            // The folder does not exist (550) or cannot be read.
            Err(_) => return Ok(None),
        };
        Ok(listing.into_iter().find(|e| name(&e.path) == name(&path)))
    }

    fn list(&mut self, dir: &str) -> Result<Vec<Entry>> {
        let dir = normalize(dir);
        if let Some(cached) = self.cache.get(&dir) {
            return Ok(cached.clone());
        }
        let mut entries = self.read_dir(&dir)?;
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        self.cache.insert(dir, entries.clone());
        Ok(entries)
    }

    fn read(&mut self, path: &str) -> Result<Vec<u8>> {
        let full = self.full(path);
        self.stream
            .retr_as_buffer(&full)
            .map(Cursor::into_inner)
            .map_err(|e| ftp_err(format!("Cannot download {full}"), e))
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<()> {
        let full = self.full(path);
        self.changed(path);
        self.stream
            .put_file(&full, &mut Cursor::new(data))
            .map(|_| ())
            .map_err(|e| ftp_err(format!("Cannot upload {full}"), e))
    }

    fn mkdir(&mut self, path: &str) -> Result<()> {
        let full = self.full(path);
        self.changed(path);
        match self.stream.mkdir(&full) {
            Ok(()) => Ok(()),
            Err(e) => match self.stat(path)? {
                Some(existing) if existing.dir => Ok(()),
                _ => Err(ftp_err(format!("Cannot create {full}"), e)),
            },
        }
    }

    fn remove_file(&mut self, path: &str) -> Result<()> {
        let full = self.full(path);
        self.changed(path);
        self.stream
            .rm(&full)
            .map_err(|e| ftp_err(format!("Cannot delete {full}"), e))
    }

    fn remove_dir(&mut self, path: &str) -> Result<()> {
        let full = self.full(path);
        self.changed(path);
        self.stream
            .rmdir(&full)
            .map_err(|e| ftp_err(format!("Cannot delete {full}"), e))
    }

    fn set_modified(&mut self, path: &str, secs: i64) -> Result<bool> {
        if !self.mfmt {
            return Ok(false);
        }
        let full = self.full(path);
        self.changed(path);
        let command = format!("MFMT {} {full}", ftp_timestamp(secs));
        Ok(self.stream.custom_command(command, &[Status::File]).is_ok())
    }

    fn describe(&self, path: &str) -> String {
        format!("{}{}", self.label, self.full(path))
    }
}

impl Drop for FtpFs {
    fn drop(&mut self) {
        let _ = self.stream.quit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_mfmt_timestamps() {
        assert_eq!(ftp_timestamp(0), "19700101000000");
        assert_eq!(ftp_timestamp(1_700_000_000), "20231114221320");
        assert_eq!(ftp_timestamp(951_782_400), "20000229000000");
    }
}
