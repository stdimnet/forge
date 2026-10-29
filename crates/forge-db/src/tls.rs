//! Certificate files of the SSL/TLS tab: read relative to the project, the client key
//! decrypted in memory with the passphrase from the keychain (PKCS#8 `ENCRYPTED PRIVATE
//! KEY`), never written to disk again.

use std::path::{Path, PathBuf};

use pkcs8::der::pem::LineEnding;
use pkcs8::EncryptedPrivateKeyInfo;

use crate::config::SslFiles;
use crate::error::{DbError, DbResult};

/// The certificate files' contents (PEM).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TlsMaterial {
    pub ca: Option<Vec<u8>>,
    pub cert: Option<Vec<u8>>,
    /// The client key, unencrypted.
    pub key: Option<Vec<u8>>,
}

impl TlsMaterial {
    /// Client certificate and key together (`Identity::from_pem` wants both).
    pub fn identity_pem(&self) -> Option<Vec<u8>> {
        let (cert, key) = (self.cert.as_ref()?, self.key.as_ref()?);
        let mut out = cert.clone();
        if !out.ends_with(b"\n") {
            out.push(b'\n');
        }
        out.extend_from_slice(key);
        Some(out)
    }
}

/// A certificate path resolved against the project root (`~/` is the home directory).
pub fn resolve(path: &str, root: Option<&Path>) -> PathBuf {
    let p = match path.strip_prefix("~/") {
        Some(rest) => dirs::home_dir()
            .map(|h| h.join(rest))
            .unwrap_or_else(|| PathBuf::from(path)),
        None => PathBuf::from(path),
    };
    match root {
        Some(r) if p.is_relative() => r.join(p),
        _ => p,
    }
}

fn read(path: &str, what: &str, root: Option<&Path>) -> DbResult<Option<Vec<u8>>> {
    if path.trim().is_empty() {
        return Ok(None);
    }
    let p = resolve(path.trim(), root);
    std::fs::read(&p)
        .map(Some)
        .map_err(|e| DbError::Config(format!("{what} {}: {e}", p.display())))
}

/// Read the files of `ssl`; `passphrase` decrypts an encrypted client key.
pub fn load(
    ssl: &SslFiles,
    passphrase: Option<&str>,
    root: Option<&Path>,
) -> DbResult<TlsMaterial> {
    let key = read(&ssl.key_file, "client key", root)?
        .map(|k| decrypt_key(&k, passphrase))
        .transpose()?;
    Ok(TlsMaterial {
        ca: read(&ssl.ca_file, "CA certificate", root)?,
        cert: read(&ssl.cert_file, "client certificate", root)?,
        key,
    })
}

/// An unencrypted PEM private key: encrypted PKCS#8 keys are decrypted with `passphrase`,
/// other keys are returned as they are.
pub fn decrypt_key(pem: &[u8], passphrase: Option<&str>) -> DbResult<Vec<u8>> {
    let text = String::from_utf8_lossy(pem);
    if text.contains("Proc-Type: 4,ENCRYPTED") {
        return Err(DbError::Config(
            "the client key uses legacy OpenSSL encryption; convert it with `openssl pkcs8 -topk8 -in key.pem -out key.p8.pem`".into(),
        ));
    }
    if !text.contains("BEGIN ENCRYPTED PRIVATE KEY") {
        return Ok(pem.to_vec());
    }
    let passphrase = passphrase.filter(|p| !p.is_empty()).ok_or_else(|| {
        DbError::Config(
            "the client key is encrypted: enter its passphrase on the SSH/SSL tab".into(),
        )
    })?;
    let (_, der) = pkcs8::der::pem::decode_vec(text.trim().as_bytes())
        .map_err(|e| DbError::Config(format!("client key: {e}")))?;
    let info = EncryptedPrivateKeyInfo::try_from(der.as_slice())
        .map_err(|e| DbError::Config(format!("client key: {e}")))?;
    let doc = info
        .decrypt(passphrase)
        .map_err(|_| DbError::Config("wrong passphrase for the client key".into()))?;
    doc.to_pem("PRIVATE KEY", LineEnding::LF)
        .map(|p| p.as_bytes().to_vec())
        .map_err(|e| DbError::Config(format!("client key: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pkcs8::pkcs5::pbes2;
    use pkcs8::{ObjectIdentifier, PrivateKeyInfo};

    fn encrypted_key() -> String {
        let key = [7u8; 34];
        let info = PrivateKeyInfo::new(
            pkcs8::AlgorithmIdentifierRef {
                oid: ObjectIdentifier::new_unwrap("1.3.101.112"),
                parameters: None,
            },
            &key,
        );
        let params =
            pbes2::Parameters::pbkdf2_sha256_aes256cbc(1000, &[1u8; 16], &[2u8; 16]).unwrap();
        let doc = info.encrypt_with_params(params, "s3cret").unwrap();
        doc.to_pem("ENCRYPTED PRIVATE KEY", LineEnding::LF)
            .unwrap()
            .to_string()
    }

    #[test]
    fn decrypts_pkcs8_keys_in_memory() {
        let pem = encrypted_key();
        let plain = decrypt_key(pem.as_bytes(), Some("s3cret")).unwrap();
        let plain = String::from_utf8(plain).unwrap();
        assert!(plain.starts_with("-----BEGIN PRIVATE KEY-----"), "{plain}");
        let err = decrypt_key(pem.as_bytes(), Some("wrong")).unwrap_err();
        assert!(err.to_string().contains("wrong passphrase"));
        let err = decrypt_key(pem.as_bytes(), None).unwrap_err();
        assert!(err.to_string().contains("passphrase"));
        let unencrypted = b"-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n";
        assert_eq!(
            decrypt_key(unencrypted, None).unwrap(),
            unencrypted.to_vec()
        );
    }

    #[test]
    fn loads_files_relative_to_the_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("certs")).unwrap();
        std::fs::write(dir.path().join("certs/ca.pem"), "CA").unwrap();
        std::fs::write(dir.path().join("certs/client.pem"), "CERT").unwrap();
        std::fs::write(dir.path().join("certs/client.key"), encrypted_key()).unwrap();
        let files = SslFiles {
            ca_file: "certs/ca.pem".into(),
            cert_file: "certs/client.pem".into(),
            key_file: "certs/client.key".into(),
        };
        let m = load(&files, Some("s3cret"), Some(dir.path())).unwrap();
        assert_eq!(m.ca.as_deref(), Some(&b"CA"[..]));
        let id = String::from_utf8(m.identity_pem().unwrap()).unwrap();
        assert!(id.starts_with("CERT\n-----BEGIN PRIVATE KEY-----"));
        let missing = SslFiles {
            ca_file: "nope.pem".into(),
            ..Default::default()
        };
        assert!(load(&missing, None, Some(dir.path()))
            .unwrap_err()
            .to_string()
            .contains("CA certificate"));
    }
}
