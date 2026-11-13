//! Commit signing with GPG or SSH keys: the key to sign with ([`SigningKey`], from the
//! Forge settings, applied per commit with `-c` options and never written to git config),
//! the secret keys `gpg --list-secret-keys --with-colons` reports, SSH public keys, and the
//! signature status git reports for commits (`%G?`).

use serde::{Deserialize, Serialize};

/// `gpg.format`: which program signs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SigningFormat {
    #[default]
    Openpgp,
    Ssh,
    X509,
}

impl SigningFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            SigningFormat::Openpgp => "openpgp",
            SigningFormat::Ssh => "ssh",
            SigningFormat::X509 => "x509",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "openpgp" | "gpg" | "pgp" => Some(SigningFormat::Openpgp),
            "ssh" => Some(SigningFormat::Ssh),
            "x509" => Some(SigningFormat::X509),
            _ => None,
        }
    }
}

/// The key commits are signed with: a GPG key id or an SSH key file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SigningKey {
    pub format: SigningFormat,
    /// GPG key id / fingerprint, or the path of an SSH key (`.pub` or private).
    pub key: String,
}

impl SigningKey {
    /// The value stored in the setting `vcs.git.commit.signingKey`: `openpgp:<id>` or
    /// `ssh:<path>`. A bare value without a known prefix is a GPG key id.
    pub fn to_setting(&self) -> String {
        format!("{}:{}", self.format.as_str(), self.key)
    }

    pub fn from_setting(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        if let Some((prefix, key)) = value.split_once(':') {
            if let Some(format) = SigningFormat::parse(prefix) {
                let key = key.trim();
                return (!key.is_empty()).then(|| SigningKey {
                    format,
                    key: key.to_string(),
                });
            }
        }
        Some(SigningKey {
            format: SigningFormat::Openpgp,
            key: value.to_string(),
        })
    }
}

/// How a commit is signed: `sign` adds `-S`; with a key, `-c user.signingkey=… -c
/// gpg.format=…` choose it for this one git invocation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Signing {
    pub sign: bool,
    pub key: Option<SigningKey>,
}

impl Signing {
    /// Options that go before the git subcommand (`git <these> commit …`).
    pub fn config_args(&self) -> Vec<String> {
        let Some(key) = self.key.as_ref().filter(|_| self.sign) else {
            return Vec::new();
        };
        vec![
            "-c".into(),
            format!("gpg.format={}", key.format.as_str()),
            "-c".into(),
            format!("user.signingkey={}", key.key),
        ]
    }

    /// Options of the subcommand (`commit -S`).
    pub fn commit_args(&self) -> Vec<String> {
        if self.sign {
            vec!["-S".into()]
        } else {
            Vec::new()
        }
    }

    /// Reject values git would misread (option-like keys, line breaks).
    pub fn validate(&self) -> Result<(), String> {
        if let Some(k) = &self.key {
            if k.key.starts_with('-') || k.key.contains(['\n', '\r', '\0']) {
                return Err(format!("invalid signing key: {:?}", k.key));
            }
        }
        Ok(())
    }
}

/// Signature status of a commit, from `%G?`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SignatureStatus {
    /// `G`: a good, valid signature.
    Good,
    /// `B`: a bad signature.
    Bad,
    /// `U`: good signature, unknown validity (the key is not trusted).
    Untrusted,
    /// `X`: good signature that has expired.
    Expired,
    /// `Y`: good signature made by an expired key.
    ExpiredKey,
    /// `R`: good signature made by a revoked key.
    Revoked,
    /// `E`: the signature cannot be checked (e.g. missing key).
    Unverified,
    /// `N`: no signature.
    #[default]
    None,
}

impl SignatureStatus {
    pub fn from_code(code: &str) -> Self {
        match code.trim() {
            "G" => SignatureStatus::Good,
            "B" => SignatureStatus::Bad,
            "U" => SignatureStatus::Untrusted,
            "X" => SignatureStatus::Expired,
            "Y" => SignatureStatus::ExpiredKey,
            "R" => SignatureStatus::Revoked,
            "E" => SignatureStatus::Unverified,
            _ => SignatureStatus::None,
        }
    }

    pub fn is_none(&self) -> bool {
        *self == SignatureStatus::None
    }
}

/// A secret key from `gpg --list-secret-keys --with-colons`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpgKey {
    /// Long key id (16 hex digits) of the primary key.
    pub id: String,
    pub fingerprint: String,
    /// User ids, e.g. `Marlon <marlon@acme.dev>`.
    pub user_ids: Vec<String>,
    /// Unix seconds; `None` when the key does not expire.
    pub expires: Option<i64>,
    /// Usable for signing (primary key or a signing subkey has the `s` capability).
    pub can_sign: bool,
    /// Revoked, expired or disabled.
    pub invalid: bool,
}

/// Unescape gpg's `\x3a` style escapes in user ids.
fn unescape_colons(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("\\x") {
        out.push_str(&rest[..i]);
        let hex = rest.get(i + 2..i + 4).unwrap_or("");
        match u8::from_str_radix(hex, 16) {
            Ok(b) if hex.len() == 2 => {
                out.push(b as char);
                rest = &rest[i + 4..];
            }
            _ => {
                out.push_str("\\x");
                rest = &rest[i + 2..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Parse `gpg --list-secret-keys --with-colons` (`sec`, `ssb`, `fpr` and `uid` records).
pub fn parse_gpg_secret_keys(out: &str) -> Vec<GpgKey> {
    let mut keys: Vec<GpgKey> = Vec::new();
    // Whether the last `sec`/`ssb` record belongs to the primary key (fingerprints follow).
    let mut primary = false;
    for line in out.lines() {
        let f: Vec<&str> = line.split(':').collect();
        match f.first().copied() {
            Some("sec") => {
                let validity = f.get(1).copied().unwrap_or("");
                let caps = f.get(11).copied().unwrap_or("");
                keys.push(GpgKey {
                    id: f.get(4).copied().unwrap_or("").to_string(),
                    fingerprint: String::new(),
                    user_ids: Vec::new(),
                    expires: f.get(6).and_then(|e| e.parse().ok()),
                    // Field 12 holds the usable capabilities of the whole key in upper case.
                    can_sign: caps.contains('S') || caps.contains('s'),
                    invalid: matches!(validity, "r" | "e" | "d" | "i"),
                });
                primary = true;
            }
            Some("ssb") => primary = false,
            Some("fpr") if primary => {
                if let Some(k) = keys.last_mut() {
                    if k.fingerprint.is_empty() {
                        k.fingerprint = f.get(9).copied().unwrap_or("").to_string();
                    }
                }
            }
            Some("uid") => {
                if let Some(k) = keys.last_mut() {
                    let validity = f.get(1).copied().unwrap_or("");
                    let uid = unescape_colons(f.get(9).copied().unwrap_or(""));
                    if !uid.is_empty() && !matches!(validity, "r" | "e") {
                        k.user_ids.push(uid);
                    }
                }
            }
            _ => {}
        }
    }
    keys.retain(|k| !k.id.is_empty());
    keys
}

/// An SSH public key (`~/.ssh/*.pub`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshKey {
    /// Path of the `.pub` file (git accepts it as `user.signingkey` with `gpg.format=ssh`).
    pub path: String,
    /// `ssh-ed25519`, `ssh-rsa`, `ecdsa-sha2-nistp256`, …
    pub key_type: String,
    pub comment: String,
}

/// Parse one public key line (`<type> <base64> [comment]`) of `path`.
pub fn parse_ssh_public_key(path: &str, text: &str) -> Option<SshKey> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    let mut parts = line.splitn(3, char::is_whitespace);
    let key_type = parts.next()?.to_string();
    let blob = parts.next()?;
    let known = key_type.starts_with("ssh-")
        || key_type.starts_with("ecdsa-")
        || key_type.starts_with("sk-");
    if !known || blob.is_empty() {
        return None;
    }
    Some(SshKey {
        path: path.to_string(),
        key_type,
        comment: parts.next().unwrap_or("").trim().to_string(),
    })
}

/// Parse the listing of a remote host's public keys: `<path>\t<first line>` per key.
pub fn parse_ssh_key_listing(out: &str) -> Vec<SshKey> {
    out.lines()
        .filter_map(|l| l.split_once('\t'))
        .filter_map(|(path, text)| parse_ssh_public_key(path, text))
        .collect()
}

/// The shell command that prints a host's public keys for [`parse_ssh_key_listing`].
pub const SSH_KEY_LISTING_SCRIPT: &str =
    "for f in \"$HOME\"/.ssh/*.pub; do [ -f \"$f\" ] && printf '%s\\t%s\\n' \"$f\" \"$(head -n 1 \"$f\")\"; done; true";

/// The signing setup found in git config (read-only, shown in the Configure dialog).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitSigningConfig {
    /// `user.signingkey`.
    pub signing_key: Option<String>,
    /// `gpg.format` (`openpgp` when unset).
    pub format: Option<String>,
    /// `commit.gpgsign`.
    pub gpg_sign: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_keys_round_trip_through_the_setting() {
        let gpg = SigningKey {
            format: SigningFormat::Openpgp,
            key: "3AA5C34371567BD2".into(),
        };
        assert_eq!(gpg.to_setting(), "openpgp:3AA5C34371567BD2");
        assert_eq!(SigningKey::from_setting(&gpg.to_setting()), Some(gpg));
        let ssh = SigningKey::from_setting("ssh:/home/m/.ssh/id_ed25519.pub").unwrap();
        assert_eq!(ssh.format, SigningFormat::Ssh);
        assert_eq!(ssh.key, "/home/m/.ssh/id_ed25519.pub");
        // A bare id is a GPG key; Windows paths keep their drive letter.
        assert_eq!(
            SigningKey::from_setting("ABCDEF").unwrap().format,
            SigningFormat::Openpgp
        );
        assert_eq!(
            SigningKey::from_setting("ssh:C:\\Users\\m\\.ssh\\id.pub")
                .unwrap()
                .key,
            "C:\\Users\\m\\.ssh\\id.pub"
        );
        assert_eq!(SigningKey::from_setting("  "), None);
        assert_eq!(SigningKey::from_setting("ssh:"), None);
    }

    #[test]
    fn signing_args_are_per_invocation() {
        let s = Signing {
            sign: true,
            key: SigningKey::from_setting("ssh:/k.pub"),
        };
        assert_eq!(
            s.config_args(),
            vec!["-c", "gpg.format=ssh", "-c", "user.signingkey=/k.pub"]
        );
        assert_eq!(s.commit_args(), vec!["-S"]);
        let off = Signing {
            sign: false,
            ..s.clone()
        };
        assert!(off.config_args().is_empty() && off.commit_args().is_empty());
        // Signing with the key from git config.
        let config = Signing {
            sign: true,
            key: None,
        };
        assert!(config.config_args().is_empty());
        assert_eq!(config.commit_args(), vec!["-S"]);
        let bad = Signing {
            sign: true,
            key: SigningKey::from_setting("openpgp:--exec"),
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn signature_codes() {
        assert_eq!(SignatureStatus::from_code("G"), SignatureStatus::Good);
        assert_eq!(SignatureStatus::from_code("U"), SignatureStatus::Untrusted);
        assert_eq!(SignatureStatus::from_code("E"), SignatureStatus::Unverified);
        assert_eq!(SignatureStatus::from_code("N"), SignatureStatus::None);
        assert_eq!(SignatureStatus::from_code(""), SignatureStatus::None);
        assert_eq!(
            serde_json::to_string(&SignatureStatus::ExpiredKey).unwrap(),
            "\"expiredKey\""
        );
    }

    #[test]
    fn parses_gpg_secret_keys() {
        let out = "sec:u:255:22:3AA5C34371567BD2:1700000000:1900000000::u:::scESC:::+:::23::0:\n\
fpr:::::::::0123456789ABCDEF01233AA5C34371567BD2:\n\
grp:::::::::AAAA:\n\
uid:u::::1700000000::HASH::Marlon <marlon@acme.dev>::::::::::0:\n\
uid:r::::1700000000::HASH::Old <old@acme.dev>::::::::::0:\n\
ssb:u:255:18:1111222233334444:1700000000::::::e:::+:::23:\n\
fpr:::::::::FFFF00001111222233334444:\n\
sec:e:4096:1:42E8A0B1C2D3E4F5:1500000000:1600000000::u:::sc:::+:::23::0:\n\
fpr:::::::::AAAA42E8A0B1C2D3E4F5:\n\
uid:e::::1500000000::HASH::Expired\\x3a Key <e@x.io>::::::::::0:\n\
uid:u::::1500000000::HASH::Team\\x3a Ops <ops@x.io>::::::::::0:\n";
        let keys = parse_gpg_secret_keys(out);
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].id, "3AA5C34371567BD2");
        assert_eq!(keys[0].fingerprint, "0123456789ABCDEF01233AA5C34371567BD2");
        assert_eq!(keys[0].user_ids, vec!["Marlon <marlon@acme.dev>"]);
        assert_eq!(keys[0].expires, Some(1_900_000_000));
        assert!(keys[0].can_sign && !keys[0].invalid);
        assert!(keys[1].invalid);
        assert_eq!(keys[1].user_ids, vec!["Team: Ops <ops@x.io>"]);
        assert_eq!(keys[1].expires, Some(1_600_000_000));
    }

    #[test]
    fn parses_ssh_public_keys() {
        let k = parse_ssh_public_key(
            "/home/m/.ssh/id_ed25519.pub",
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI marlon@laptop\n",
        )
        .unwrap();
        assert_eq!(k.key_type, "ssh-ed25519");
        assert_eq!(k.comment, "marlon@laptop");
        assert!(parse_ssh_public_key("x.pub", "not a key").is_none());
        assert!(parse_ssh_public_key("x.pub", "").is_none());
        let listing = "/h/.ssh/a.pub\tssh-rsa AAAAB3 a@b\n/h/.ssh/b.pub\tgarbage\n/h/.ssh/c.pub\tecdsa-sha2-nistp256 AAAAE2\n";
        let keys = parse_ssh_key_listing(listing);
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[1].path, "/h/.ssh/c.pub");
        assert_eq!(keys[1].comment, "");
    }
}
