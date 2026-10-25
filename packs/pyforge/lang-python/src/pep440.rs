//! PEP 440 versions and version specifiers (`>=2.31,<3`, `~=1.4.2`, `==4.2.*`).
//!
//! Versions are normalized while parsing (`1.0RC1` = `1.0rc1`, `1.0-1` = `1.0.post1`,
//! `v2` = `2`) and ordered like pip orders them: epoch, release (trailing zeros do not
//! count), then dev < pre < final < post, and local versions after their public version.

use std::cmp::Ordering;
use std::fmt;

/// Pre-release phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Pre {
    Alpha,
    Beta,
    Rc,
}

impl Pre {
    fn tag(self) -> &'static str {
        match self {
            Pre::Alpha => "a",
            Pre::Beta => "b",
            Pre::Rc => "rc",
        }
    }
}

/// One segment of a local version label (`+ubuntu.1`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LocalPart {
    Num(u64),
    Text(String),
}

impl PartialOrd for LocalPart {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LocalPart {
    /// Numbers sort after text, numbers numerically, text lexicographically.
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (LocalPart::Num(a), LocalPart::Num(b)) => a.cmp(b),
            (LocalPart::Text(a), LocalPart::Text(b)) => a.cmp(b),
            (LocalPart::Num(_), LocalPart::Text(_)) => Ordering::Greater,
            (LocalPart::Text(_), LocalPart::Num(_)) => Ordering::Less,
        }
    }
}

/// A PEP 440 version.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Version {
    pub epoch: u64,
    pub release: Vec<u64>,
    pub pre: Option<(Pre, u64)>,
    pub post: Option<u64>,
    pub dev: Option<u64>,
    pub local: Vec<LocalPart>,
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.epoch != 0 {
            write!(f, "{}!", self.epoch)?;
        }
        let release: Vec<String> = self.release.iter().map(u64::to_string).collect();
        write!(f, "{}", release.join("."))?;
        if let Some((kind, n)) = self.pre {
            write!(f, "{}{n}", kind.tag())?;
        }
        if let Some(n) = self.post {
            write!(f, ".post{n}")?;
        }
        if let Some(n) = self.dev {
            write!(f, ".dev{n}")?;
        }
        if !self.local.is_empty() {
            let parts: Vec<String> = self
                .local
                .iter()
                .map(|p| match p {
                    LocalPart::Num(n) => n.to_string(),
                    LocalPart::Text(t) => t.clone(),
                })
                .collect();
            write!(f, "+{}", parts.join("."))?;
        }
        Ok(())
    }
}

/// Reads a normalized version string left to right.
struct Cursor<'a> {
    s: &'a [u8],
    i: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn number(&mut self) -> Option<u64> {
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
        if start == self.i {
            return None;
        }
        std::str::from_utf8(&self.s[start..self.i])
            .ok()?
            .parse()
            .ok()
    }

    /// Skip one optional separator (`.`, `-`, `_`).
    fn separator(&mut self) -> bool {
        if matches!(self.peek(), Some(b'.' | b'-' | b'_')) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    /// One of `words` (longest first), returning its index.
    fn word(&mut self, words: &[&str]) -> Option<usize> {
        let rest = &self.s[self.i..];
        let (idx, w) = words
            .iter()
            .enumerate()
            .filter(|(_, w)| rest.starts_with(w.as_bytes()))
            .max_by_key(|(_, w)| w.len())?;
        self.i += w.len();
        Some(idx)
    }
}

impl Version {
    /// Parse and normalize a version; `None` when it is not PEP 440.
    pub fn parse(text: &str) -> Option<Version> {
        let lower = text.trim().to_ascii_lowercase();
        let lower = lower.strip_prefix('v').unwrap_or(&lower);
        let (public, local) = match lower.split_once('+') {
            Some((p, l)) => (p, Some(l)),
            None => (lower, None),
        };
        let mut c = Cursor {
            s: public.as_bytes(),
            i: 0,
        };
        let mut v = Version::default();
        let first = c.number()?;
        if c.peek() == Some(b'!') {
            c.i += 1;
            v.epoch = first;
            v.release.push(c.number()?);
        } else {
            v.release.push(first);
        }
        while c.peek() == Some(b'.') && c.s.get(c.i + 1).is_some_and(u8::is_ascii_digit) {
            c.i += 1;
            v.release.push(c.number()?);
        }
        // Pre-release: a, alpha, b, beta, c, rc, pre, preview.
        let save = c.i;
        c.separator();
        let pre_words = ["a", "alpha", "b", "beta", "c", "rc", "pre", "preview"];
        if let Some(w) = c.word(&pre_words) {
            let kind = match w {
                0 | 1 => Pre::Alpha,
                2 | 3 => Pre::Beta,
                _ => Pre::Rc,
            };
            c.separator();
            v.pre = Some((kind, c.number().unwrap_or(0)));
        } else {
            c.i = save;
        }
        // Post-release: post, rev, r, or the implicit `-1`.
        let save = c.i;
        if c.peek() == Some(b'-') && c.s.get(c.i + 1).is_some_and(u8::is_ascii_digit) {
            c.i += 1;
            v.post = c.number();
        } else {
            c.separator();
            if c.word(&["post", "rev", "r"]).is_some() {
                c.separator();
                v.post = Some(c.number().unwrap_or(0));
            } else {
                c.i = save;
            }
        }
        // Development release.
        let save = c.i;
        c.separator();
        if c.word(&["dev"]).is_some() {
            c.separator();
            v.dev = Some(c.number().unwrap_or(0));
        } else {
            c.i = save;
        }
        if c.i != c.s.len() {
            return None;
        }
        if let Some(local) = local {
            for part in local.split(['.', '-', '_']) {
                if part.is_empty() || !part.bytes().all(|b| b.is_ascii_alphanumeric()) {
                    return None;
                }
                v.local.push(match part.parse::<u64>() {
                    Ok(n) => LocalPart::Num(n),
                    Err(_) => LocalPart::Text(part.to_string()),
                });
            }
        }
        Some(v)
    }

    pub fn is_prerelease(&self) -> bool {
        self.pre.is_some() || self.dev.is_some()
    }

    /// The version without its local label.
    pub fn public(&self) -> Version {
        Version {
            local: Vec::new(),
            ..self.clone()
        }
    }

    /// The release with trailing zeros removed (`1.2.0` and `1.2` are equal).
    fn release_key(&self) -> &[u64] {
        let mut end = self.release.len();
        while end > 1 && self.release[end - 1] == 0 {
            end -= 1;
        }
        &self.release[..end]
    }

    /// Ordering of the pre/post/dev part, following `packaging`'s comparison key.
    fn suffix_key(&self) -> (i8, Pre, u64, i8, u64, i8, u64) {
        // dev release of a final version sorts before its pre-releases.
        let (pre_rank, pre_kind, pre_n) = match (self.pre, self.post, self.dev) {
            (None, None, Some(_)) => (-1, Pre::Alpha, 0),
            (None, _, _) => (1, Pre::Alpha, 0),
            (Some((k, n)), _, _) => (0, k, n),
        };
        let (post_rank, post_n) = match self.post {
            None => (-1, 0),
            Some(n) => (0, n),
        };
        let (dev_rank, dev_n) = match self.dev {
            None => (1, 0),
            Some(n) => (0, n),
        };
        (
            pre_rank, pre_kind, pre_n, post_rank, post_n, dev_rank, dev_n,
        )
    }
}

fn cmp_release(a: &[u64], b: &[u64]) -> Ordering {
    let len = a.len().max(b.len());
    for i in 0..len {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            Ordering::Equal => {}
            o => return o,
        }
    }
    Ordering::Equal
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.epoch
            .cmp(&other.epoch)
            .then_with(|| cmp_release(self.release_key(), other.release_key()))
            .then_with(|| self.suffix_key().cmp(&other.suffix_key()))
            .then_with(|| self.local.cmp(&other.local))
    }
}

/// Comparison operator of a specifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    /// `~=`
    Compatible,
    /// `==`
    Equal,
    /// `!=`
    NotEqual,
    /// `<=`
    LessEqual,
    /// `>=`
    GreaterEqual,
    /// `<`
    Less,
    /// `>`
    Greater,
    /// `===`
    Arbitrary,
}

impl Operator {
    pub fn as_str(self) -> &'static str {
        match self {
            Operator::Compatible => "~=",
            Operator::Equal => "==",
            Operator::NotEqual => "!=",
            Operator::LessEqual => "<=",
            Operator::GreaterEqual => ">=",
            Operator::Less => "<",
            Operator::Greater => ">",
            Operator::Arbitrary => "===",
        }
    }

    /// The operator at the start of `s` and its length.
    pub fn prefix(s: &str) -> Option<(Operator, usize)> {
        const OPS: &[(&str, Operator)] = &[
            ("===", Operator::Arbitrary),
            ("~=", Operator::Compatible),
            ("==", Operator::Equal),
            ("!=", Operator::NotEqual),
            ("<=", Operator::LessEqual),
            (">=", Operator::GreaterEqual),
            ("<", Operator::Less),
            (">", Operator::Greater),
        ];
        OPS.iter()
            .find(|(t, _)| s.starts_with(t))
            .map(|(t, op)| (*op, t.len()))
    }
}

/// One specifier: an operator and a version (`==4.2.*` has `wildcard`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Specifier {
    pub op: Operator,
    /// The version text as written (for `===` and messages).
    pub text: String,
    pub version: Option<Version>,
    pub wildcard: bool,
}

impl fmt::Display for Specifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.op.as_str(), self.text)
    }
}

impl Specifier {
    pub fn parse(text: &str) -> Result<Specifier, String> {
        let t = text.trim();
        let (op, len) = Operator::prefix(t)
            .ok_or_else(|| format!("'{t}' does not start with a comparison operator"))?;
        let raw = t[len..].trim().to_string();
        if raw.is_empty() {
            return Err(format!("'{t}' has no version"));
        }
        if op == Operator::Arbitrary {
            return Ok(Specifier {
                op,
                text: raw,
                version: None,
                wildcard: false,
            });
        }
        let (body, wildcard) = match raw.strip_suffix(".*") {
            Some(b) => (b, true),
            None => (raw.as_str(), false),
        };
        if wildcard && !matches!(op, Operator::Equal | Operator::NotEqual) {
            return Err(format!("'.*' is only allowed with == and != ('{t}')"));
        }
        let version =
            Version::parse(body).ok_or_else(|| format!("'{body}' is not a valid version"))?;
        if op == Operator::Compatible && version.release.len() < 2 {
            return Err(format!("~= needs at least two release segments ('{t}')"));
        }
        Ok(Specifier {
            op,
            text: raw,
            version: Some(version),
            wildcard,
        })
    }

    /// Whether `v` satisfies this specifier (pre-releases are accepted: the check is about
    /// a version that is already chosen, such as an installed one).
    pub fn contains(&self, v: &Version) -> bool {
        let Some(spec) = &self.version else {
            return v.to_string() == self.text.to_ascii_lowercase();
        };
        match self.op {
            Operator::Equal => equal(spec, self.wildcard, v),
            Operator::NotEqual => !equal(spec, self.wildcard, v),
            Operator::Compatible => {
                let prefix = Version {
                    epoch: spec.epoch,
                    release: spec.release[..spec.release.len() - 1].to_vec(),
                    ..Default::default()
                };
                v.public() >= *spec && equal(&prefix, true, v)
            }
            Operator::LessEqual => v.public() <= *spec,
            Operator::GreaterEqual => v.public() >= *spec,
            Operator::Less => {
                let v = v.public();
                // `<3.0` does not admit `3.0rc1`.
                v < *spec
                    && !(!spec.is_prerelease()
                        && v.is_prerelease()
                        && cmp_release(v.release_key(), spec.release_key()) == Ordering::Equal
                        && v.epoch == spec.epoch)
            }
            Operator::Greater => {
                // `>1.0` does not admit `1.0.post1` or `1.0+local`.
                let public = v.public();
                public > *spec
                    && !(spec.post.is_none()
                        && public.post.is_some()
                        && cmp_release(public.release_key(), spec.release_key()) == Ordering::Equal
                        && public.epoch == spec.epoch)
            }
            Operator::Arbitrary => unreachable!("handled above"),
        }
    }
}

fn equal(spec: &Version, wildcard: bool, v: &Version) -> bool {
    if wildcard {
        if v.epoch != spec.epoch {
            return false;
        }
        // Prefix match of the release (padded with zeros), then of the suffix when the
        // prefix names one (`==1.0rc1.*` is unusual; compare the release only).
        let n = spec.release.len();
        (0..n).all(|i| v.release.get(i).copied().unwrap_or(0) == spec.release[i])
    } else if spec.local.is_empty() {
        v.public() == *spec
    } else {
        v == spec
    }
}

/// A comma-separated list of specifiers; all must match.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpecifierSet(pub Vec<Specifier>);

impl SpecifierSet {
    pub fn parse(text: &str) -> Result<SpecifierSet, String> {
        let mut out = Vec::new();
        for part in text.split(',') {
            if part.trim().is_empty() {
                continue;
            }
            out.push(Specifier::parse(part)?);
        }
        Ok(SpecifierSet(out))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, v: &Version) -> bool {
        self.0.iter().all(|s| s.contains(v))
    }

    /// The newest of `versions` that matches, skipping pre-releases unless a specifier names
    /// one or nothing else matches.
    pub fn best<'a>(&self, versions: impl IntoIterator<Item = &'a Version>) -> Option<&'a Version> {
        let matching: Vec<&Version> = versions.into_iter().filter(|v| self.contains(v)).collect();
        let wants_pre = self
            .0
            .iter()
            .any(|s| s.version.as_ref().is_some_and(Version::is_prerelease));
        matching
            .iter()
            .filter(|v| wants_pre || !v.is_prerelease())
            .max()
            .or_else(|| matching.iter().max())
            .copied()
    }
}

impl fmt::Display for SpecifierSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.0.iter().map(Specifier::to_string).collect();
        write!(f, "{}", parts.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap_or_else(|| panic!("{s} should parse"))
    }

    fn set(s: &str) -> SpecifierSet {
        SpecifierSet::parse(s).unwrap()
    }

    #[test]
    fn parses_and_normalizes() {
        assert_eq!(v("1.0RC1").to_string(), "1.0rc1");
        assert_eq!(v("1.0-1").to_string(), "1.0.post1");
        assert_eq!(v("v2.3").to_string(), "2.3");
        assert_eq!(v("1!2.0.alpha.3").to_string(), "1!2.0a3");
        assert_eq!(v("1.0.dev").to_string(), "1.0.dev0");
        assert_eq!(v("1.0-preview2").to_string(), "1.0rc2");
        assert_eq!(v("1.0rev3").to_string(), "1.0.post3");
        assert_eq!(v("1.2+ubuntu-1").to_string(), "1.2+ubuntu.1");
        assert!(Version::parse("not-a-version").is_none());
        assert!(Version::parse("1.0+").is_none());
        assert!(Version::parse("").is_none());
    }

    #[test]
    fn orders_like_pip() {
        let ordered = [
            "1.0.dev0",
            "1.0a1",
            "1.0a2.dev1",
            "1.0a2",
            "1.0b1",
            "1.0rc1",
            "1.0",
            "1.0+local",
            "1.0.post1.dev1",
            "1.0.post1",
            "1.1",
            "2!0.1",
        ];
        for pair in ordered.windows(2) {
            assert!(v(pair[0]) < v(pair[1]), "{} < {}", pair[0], pair[1]);
        }
        assert_eq!(v("1.0"), v("1.0").public());
        assert_eq!(v("1.0.0").cmp(&v("1")), Ordering::Equal);
    }

    #[test]
    fn specifiers_match() {
        assert!(set(">=2.31,<3").contains(&v("2.32.3")));
        assert!(!set(">=2.31,<3").contains(&v("3.0")));
        assert!(!set("<3").contains(&v("3.0rc1")));
        assert!(set("<3rc2").contains(&v("3.0rc1")));
        assert!(set("~=1.4.2").contains(&v("1.4.9")));
        assert!(!set("~=1.4.2").contains(&v("1.5.0")));
        assert!(set("~=1.4").contains(&v("1.9")));
        assert!(!set("~=1.4").contains(&v("2.0")));
        assert!(set("==4.2.*").contains(&v("4.2.11")));
        assert!(!set("==4.2.*").contains(&v("4.20")));
        assert!(set("!=1.5.*").contains(&v("1.6")));
        assert!(set("==1.0").contains(&v("1.0+local")));
        assert!(!set("==1.0+a").contains(&v("1.0+b")));
        assert!(!set(">1.0").contains(&v("1.0.post1")));
        assert!(set(">1.0.post1").contains(&v("1.0.post2")));
        assert!(set("===1.0").contains(&v("1.0")));
        assert!(!set("===1.0").contains(&v("1.0.0")));
        assert!(set("").contains(&v("0.1")));
    }

    #[test]
    fn rejects_bad_specifiers() {
        assert!(SpecifierSet::parse(">=").is_err());
        assert!(SpecifierSet::parse("1.0").is_err());
        assert!(SpecifierSet::parse(">=1.*").is_err());
        assert!(SpecifierSet::parse("~=1").is_err());
        assert!(SpecifierSet::parse(">=abc").is_err());
    }

    #[test]
    fn best_skips_prereleases() {
        let versions: Vec<Version> = ["1.0", "1.1", "2.0rc1"].iter().map(|s| v(s)).collect();
        assert_eq!(set(">=1").best(&versions).unwrap().to_string(), "1.1");
        assert_eq!(
            set(">=2.0rc1").best(&versions).unwrap().to_string(),
            "2.0rc1"
        );
        assert_eq!(
            set(">=2.0.dev0").best(&versions).unwrap().to_string(),
            "2.0rc1"
        );
        assert!(set(">3").best(&versions).is_none());
        assert_eq!(set(">=1,!=1.1").to_string(), ">=1,!=1.1");
    }
}
