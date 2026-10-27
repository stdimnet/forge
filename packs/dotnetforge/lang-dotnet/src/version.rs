//! NuGet versions (SemVer 2.0 with an optional fourth number, `1.2.3.4-beta.1+meta`),
//! version ranges as written in package references (`[1.0,2.0)`, `1.*`) and the newest
//! version for the "Upgrade" column of the NuGet tool window.

use std::cmp::Ordering;

/// A parsed NuGet version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NuGetVersion {
    pub numbers: [u64; 4],
    /// Dot-separated prerelease labels (`beta.1`), empty for releases.
    pub prerelease: String,
}

impl NuGetVersion {
    pub fn parse(v: &str) -> Option<NuGetVersion> {
        let v = v.trim();
        let v = v.split('+').next().unwrap_or(v);
        let (core, pre) = match v.split_once('-') {
            Some((c, p)) => (c, p.to_string()),
            None => (v, String::new()),
        };
        let mut numbers = [0u64; 4];
        let parts: Vec<&str> = core.split('.').collect();
        if parts.is_empty() || parts.len() > 4 || parts[0].is_empty() {
            return None;
        }
        for (i, p) in parts.iter().enumerate() {
            numbers[i] = p.parse().ok()?;
        }
        Some(NuGetVersion {
            numbers,
            prerelease: pre,
        })
    }

    pub fn is_prerelease(&self) -> bool {
        !self.prerelease.is_empty()
    }
}

/// Compare prerelease labels the SemVer way (numeric labels numerically, case-insensitive
/// otherwise, fewer labels first).
pub fn compare_labels(a: &str, b: &str) -> Ordering {
    let mut x = a.split('.');
    let mut y = b.split('.');
    loop {
        match (x.next(), y.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(p), Some(q)) => {
                let o = match (p.parse::<u64>(), q.parse::<u64>()) {
                    (Ok(m), Ok(n)) => m.cmp(&n),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    _ => p.to_ascii_lowercase().cmp(&q.to_ascii_lowercase()),
                };
                if o != Ordering::Equal {
                    return o;
                }
            }
        }
    }
}

impl Ord for NuGetVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.numbers.cmp(&other.numbers).then_with(|| {
            match (self.prerelease.is_empty(), other.prerelease.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                _ => compare_labels(&self.prerelease, &other.prerelease),
            }
        })
    }
}

impl PartialOrd for NuGetVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Compare two version strings (unparsable ones sort first, by text).
pub fn compare(a: &str, b: &str) -> Ordering {
    match (NuGetVersion::parse(a), NuGetVersion::parse(b)) {
        (Some(x), Some(y)) => x.cmp(&y),
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        _ => a.cmp(b),
    }
}

/// The lowest version a reference accepts: `1.2.3`, `[1.2.3]`, `[1.0,2.0)`, `(,2.0]`
/// (unknown), `1.*` (`1.0.0`).
pub fn range_minimum(range: &str) -> Option<String> {
    let r = range.trim();
    if r.is_empty() {
        return None;
    }
    if r.starts_with('[') || r.starts_with('(') {
        let inner = r.trim_start_matches(['[', '(']).trim_end_matches([']', ')']);
        let low = inner.split(',').next().unwrap_or("").trim();
        return (!low.is_empty()).then(|| low.to_string());
    }
    if let Some(prefix) = r.strip_suffix(".*").or_else(|| r.strip_suffix('*')) {
        let p = prefix.trim_end_matches('.');
        return Some(if p.is_empty() {
            "0.0.0".into()
        } else if p.contains('.') {
            format!("{p}.0")
        } else {
            format!("{p}.0.0")
        });
    }
    Some(r.to_string())
}

/// The newest of `versions` (prereleases only with `prerelease`).
pub fn latest<'a>(versions: impl IntoIterator<Item = &'a str>, prerelease: bool) -> Option<&'a str> {
    versions
        .into_iter()
        .filter(|v| {
            NuGetVersion::parse(v).is_some_and(|p| prerelease || !p.is_prerelease())
        })
        .max_by(|a, b| compare(a, b))
}

/// The upgrade for an installed version: the newest version above it (a prerelease only
/// when `prerelease` is on or the installed one is a prerelease).
pub fn upgrade<'a>(installed: &str, versions: &'a [String], prerelease: bool) -> Option<&'a str> {
    let current = range_minimum(installed)?;
    let cur = NuGetVersion::parse(&current)?;
    let pre = prerelease || cur.is_prerelease();
    latest(versions.iter().map(String::as_str), pre)
        .filter(|l| NuGetVersion::parse(l).is_some_and(|v| v > cur))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_compares() {
        assert_eq!(NuGetVersion::parse("1.2.3.4").unwrap().numbers, [1, 2, 3, 4]);
        assert!(NuGetVersion::parse("2.0.0-beta.1+sha.abc").unwrap().is_prerelease());
        assert!(NuGetVersion::parse("x.1").is_none());
        assert!(NuGetVersion::parse("1.2.3.4.5").is_none());
        assert_eq!(compare("1.10.0", "1.9.0"), Ordering::Greater);
        assert_eq!(compare("2.0.0-beta.2", "2.0.0-beta.10"), Ordering::Less);
        assert_eq!(compare("2.0.0-rc.1", "2.0.0"), Ordering::Less);
        assert_eq!(compare("1.0", "1.0.0.0"), Ordering::Equal);
        assert_eq!(compare_labels("alpha", "alpha.1"), Ordering::Less);
        assert_eq!(compare_labels("1", "alpha"), Ordering::Less);
    }

    #[test]
    fn ranges_and_upgrades() {
        assert_eq!(range_minimum("[1.0,2.0)").as_deref(), Some("1.0"));
        assert_eq!(range_minimum("(,2.0]"), None);
        assert_eq!(range_minimum("6.*").as_deref(), Some("6.0.0"));
        assert_eq!(range_minimum("6.1.*").as_deref(), Some("6.1.0"));
        assert_eq!(range_minimum("13.0.3").as_deref(), Some("13.0.3"));
        let versions: Vec<String> = ["6.4.0", "6.5.0", "6.6.2", "7.0.0-preview.1"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(upgrade("6.5.0", &versions, false), Some("6.6.2"));
        assert_eq!(upgrade("6.5.0", &versions, true), Some("7.0.0-preview.1"));
        assert_eq!(upgrade("6.6.2", &versions, false), None);
        assert_eq!(upgrade("7.0.0-alpha", &versions, false), Some("7.0.0-preview.1"));
        assert_eq!(upgrade("", &versions, false), None);
        assert_eq!(latest(["1.0.0", "bad", "1.2.0"], false), Some("1.2.0"));
    }
}
