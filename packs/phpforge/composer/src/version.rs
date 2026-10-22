//! Composer package versions (`v1.2.3`, `1.2`, `2.0.0-RC1`, `1.0.0-beta.2`, `1.x-dev`,
//! `dev-main`), normalized and ordered like Composer's `VersionParser` and
//! `Comparator`.

use std::cmp::Ordering;
use std::fmt;

/// Stability of a version, from least to most stable (`minimum-stability`, `@dev` flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stability {
    Dev,
    Alpha,
    Beta,
    RC,
    Stable,
}

impl Stability {
    pub fn parse(s: &str) -> Option<Stability> {
        Some(match s.to_ascii_lowercase().as_str() {
            "dev" => Stability::Dev,
            "alpha" | "a" => Stability::Alpha,
            "beta" | "b" => Stability::Beta,
            "rc" => Stability::RC,
            "stable" => Stability::Stable,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Stability::Dev => "dev",
            Stability::Alpha => "alpha",
            Stability::Beta => "beta",
            Stability::RC => "RC",
            Stability::Stable => "stable",
        }
    }
}

/// The pre-release (or patch) suffix of a version. The order is Composer's:
/// `dev < alpha < beta < RC < (none) < patch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Suffix {
    Dev,
    Alpha(u64),
    Beta(u64),
    RC(u64),
    None,
    Patch(u64),
}

/// A normalized version: four numeric parts plus a suffix, or a branch (`dev-main`).
/// Equality and order ignore how it was written (`v1.2` equals `1.2.0.0`).
#[derive(Debug, Clone)]
pub struct Version {
    parts: [u64; 4],
    suffix: Suffix,
    /// `dev-<name>` branches, which only equal themselves.
    branch: Option<String>,
    /// The text it was parsed from (`v12.1.0`).
    original: String,
}

impl Version {
    /// Parse a version as Composer writes it in composer.lock or Packagist metadata.
    pub fn parse(text: &str) -> Option<Version> {
        let original = text.trim().to_string();
        let s = original.as_str();
        // `1.0.0+build` metadata does not take part in comparisons.
        let s = s.split('+').next().unwrap_or(s);
        let lower = s.to_ascii_lowercase();
        if let Some(name) = lower.strip_prefix("dev-") {
            if name.is_empty() {
                return None;
            }
            return Some(Version {
                parts: [0; 4],
                suffix: Suffix::Dev,
                branch: Some(name.to_string()),
                original,
            });
        }
        let s = lower.strip_prefix('v').unwrap_or(&lower);
        // Branch aliases: `1.x-dev`, `2.1.x-dev`.
        if let Some(head) = s
            .strip_suffix(".x-dev")
            .or_else(|| s.strip_suffix(".*-dev"))
        {
            let mut parts = [9_999_999; 4];
            for (i, p) in head.split('.').enumerate() {
                if i >= 4 {
                    return None;
                }
                parts[i] = p.parse().ok()?;
            }
            return Some(Version {
                parts,
                suffix: Suffix::Dev,
                branch: None,
                original,
            });
        }
        let digits_end = s
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(s.len());
        let (numbers, rest) = s.split_at(digits_end);
        let numbers = numbers.trim_end_matches('.');
        if numbers.is_empty() {
            return None;
        }
        let mut parts = [0u64; 4];
        for (i, p) in numbers.split('.').enumerate() {
            if i >= 4 {
                return None;
            }
            parts[i] = p.parse().ok()?;
        }
        let suffix = parse_suffix(rest)?;
        Some(Version {
            parts,
            suffix,
            branch: None,
            original,
        })
    }

    /// The text the version was parsed from.
    pub fn original(&self) -> &str {
        &self.original
    }

    pub fn major(&self) -> u64 {
        self.parts[0]
    }

    pub fn minor(&self) -> u64 {
        self.parts[1]
    }

    pub fn patch(&self) -> u64 {
        self.parts[2]
    }

    pub fn is_branch(&self) -> bool {
        self.branch.is_some() || self.parts[0] == 9_999_999
    }

    pub fn stability(&self) -> Stability {
        if self.is_branch() {
            return Stability::Dev;
        }
        match self.suffix {
            Suffix::Dev => Stability::Dev,
            Suffix::Alpha(_) => Stability::Alpha,
            Suffix::Beta(_) => Stability::Beta,
            Suffix::RC(_) => Stability::RC,
            Suffix::None | Suffix::Patch(_) => Stability::Stable,
        }
    }

    pub fn is_stable(&self) -> bool {
        self.stability() == Stability::Stable
    }

    /// Normalized form, like Composer's `version_normalized` (`12.1.0.0`, `2.0.0.0-RC1`).
    pub fn normalized(&self) -> String {
        if let Some(b) = &self.branch {
            return format!("dev-{b}");
        }
        let [a, b, c, d] = self.parts;
        let base = format!("{a}.{b}.{c}.{d}");
        match self.suffix {
            Suffix::None => base,
            Suffix::Dev => format!("{base}-dev"),
            Suffix::Alpha(n) => format!("{base}-alpha{n}"),
            Suffix::Beta(n) => format!("{base}-beta{n}"),
            Suffix::RC(n) => format!("{base}-RC{n}"),
            Suffix::Patch(n) => format!("{base}-patch{n}"),
        }
    }

    /// The same version with a `-dev` suffix: the lowest version of its line, which is
    /// how Composer writes lower bounds (`>=1.2.0.0-dev`).
    pub(crate) fn dev_of(parts: [u64; 4]) -> Version {
        Version {
            parts,
            suffix: Suffix::Dev,
            branch: None,
            original: String::new(),
        }
    }

    pub(crate) fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    pub(crate) fn has_suffix(&self) -> bool {
        self.suffix != Suffix::None
    }
}

fn parse_suffix(rest: &str) -> Option<Suffix> {
    let rest = rest.trim_start_matches(['-', '_', '.']);
    if rest.is_empty() {
        return Some(Suffix::None);
    }
    let name_end = rest
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(rest.len());
    let (name, number) = rest.split_at(name_end);
    let number = number.trim_start_matches(['.', '-', '_']);
    let n: u64 = if number.is_empty() {
        0
    } else {
        // `beta.2`, `RC1`; further parts (`alpha1.1`) count by their first number.
        number
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|d| d.parse().ok())?
    };
    Some(match name {
        "dev" => Suffix::Dev,
        "alpha" | "a" => Suffix::Alpha(n),
        "beta" | "b" => Suffix::Beta(n),
        "rc" => Suffix::RC(n),
        "stable" => Suffix::None,
        "patch" | "pl" | "p" => Suffix::Patch(n),
        _ => return None,
    })
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        // Branches sort after every numbered version (Composer treats them as
        // `9999999-dev`), by name among themselves.
        match (&self.branch, &other.branch) {
            (Some(a), Some(b)) => return a.cmp(b),
            (Some(_), None) => return Ordering::Greater,
            (None, Some(_)) => return Ordering::Less,
            (None, None) => {}
        }
        self.parts
            .cmp(&other.parts)
            .then(self.suffix.cmp(&other.suffix))
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.original.is_empty() {
            f.write_str(&self.normalized())
        } else {
            f.write_str(&self.original)
        }
    }
}

/// The newest version of `versions` (stable ones only when `stable_only`); branches never
/// count.
pub fn latest<'a, I>(versions: I, stable_only: bool) -> Option<Version>
where
    I: IntoIterator<Item = &'a str>,
{
    versions
        .into_iter()
        .filter_map(Version::parse)
        .filter(|v| !v.is_branch() && (!stable_only || v.is_stable()))
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap_or_else(|| panic!("cannot parse {s}"))
    }

    #[test]
    fn normalizes_like_composer() {
        assert_eq!(v("v12.1.0").normalized(), "12.1.0.0");
        assert_eq!(v("1.2").normalized(), "1.2.0.0");
        assert_eq!(v("2.0.0-RC1").normalized(), "2.0.0.0-RC1");
        assert_eq!(v("1.0.0-beta.2").normalized(), "1.0.0.0-beta2");
        assert_eq!(v("1.0.0beta3").normalized(), "1.0.0.0-beta3");
        assert_eq!(v("1.0.0-alpha").normalized(), "1.0.0.0-alpha0");
        assert_eq!(v("1.0.0-pl2").normalized(), "1.0.0.0-patch2");
        assert_eq!(v("dev-main").normalized(), "dev-main");
        assert_eq!(v("2.x-dev").stability(), Stability::Dev);
        assert_eq!(v("v1.0.0+build.5").normalized(), "1.0.0.0");
        assert!(Version::parse("banana").is_none());
        assert!(Version::parse("1.0.0-foo").is_none());
        assert!(Version::parse("1.2.3.4.5").is_none());
    }

    #[test]
    fn orders_pre_releases_before_releases() {
        let mut list: Vec<Version> = [
            "1.0.0",
            "1.0.0-RC1",
            "1.0.0-beta2",
            "1.0.0-beta10",
            "1.0.0-alpha1",
            "1.0.0-dev",
            "1.0.0-patch1",
            "0.9",
            "dev-main",
            "1.0.1",
        ]
        .iter()
        .map(|s| v(s))
        .collect();
        list.sort();
        let names: Vec<&str> = list.iter().map(|v| v.original()).collect();
        assert_eq!(
            names,
            [
                "0.9",
                "1.0.0-dev",
                "1.0.0-alpha1",
                "1.0.0-beta2",
                "1.0.0-beta10",
                "1.0.0-RC1",
                "1.0.0",
                "1.0.0-patch1",
                "1.0.1",
                "dev-main"
            ]
        );
        assert_eq!(v("v1.2.3"), v("1.2.3.0"));
    }

    #[test]
    fn stability_and_latest() {
        assert_eq!(v("1.0.0-RC2").stability(), Stability::RC);
        assert_eq!(v("1.0.0-patch1").stability(), Stability::Stable);
        assert_eq!(v("dev-feature/x").stability(), Stability::Dev);
        let versions = [
            "v12.1.0",
            "v12.3.0",
            "v13.0.0-beta1",
            "dev-master",
            "12.2.5",
        ];
        assert_eq!(latest(versions, true).unwrap().original(), "v12.3.0");
        assert_eq!(latest(versions, false).unwrap().original(), "v13.0.0-beta1");
        assert!(latest(["dev-main"], true).is_none());
    }
}
