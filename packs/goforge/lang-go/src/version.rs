//! Module versions (semantic versions with a `v`, pseudo-versions, `+incompatible`) and
//! module paths as the module proxy protocol spells them.

use std::cmp::Ordering;

/// `v1.2.3`, `v1.2.3-pre.1`, `v0.0.0-20240101120000-abcdef123456`, `v2.0.0+incompatible`.
pub fn is_valid_version(v: &str) -> bool {
    parse(v).is_some()
}

struct Parsed<'a> {
    nums: [u64; 3],
    pre: Option<&'a str>,
}

fn parse(v: &str) -> Option<Parsed<'_>> {
    let rest = v.strip_prefix('v')?;
    let rest = rest.strip_suffix("+incompatible").unwrap_or(rest);
    let (core, pre) = match rest.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (rest, None),
    };
    if core.contains('+') {
        return None;
    }
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let mut nums = [0u64; 3];
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()) || (p.len() > 1 && p.starts_with('0'))
        {
            return None;
        }
        nums[i] = p.parse().ok()?;
    }
    if let Some(p) = pre {
        if p.is_empty()
            || !p
                .split('.')
                .all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        {
            return None;
        }
    }
    Some(Parsed { nums, pre })
}

/// Whether `v` is a pseudo-version (a commit without a tag).
pub fn is_pseudo(v: &str) -> bool {
    parse(v).and_then(|p| p.pre).is_some_and(|pre| {
        let last = pre.rsplit(['-', '.']).next().unwrap_or("");
        let has_time = pre
            .split(['-', '.'])
            .any(|s| s.len() == 14 && s.chars().all(|c| c.is_ascii_digit()));
        has_time && last.len() == 12 && last.chars().all(|c| c.is_ascii_hexdigit())
    })
}

/// Semantic version order (pre-releases before releases); invalid versions sort first.
pub fn compare(a: &str, b: &str) -> Ordering {
    match (parse(a), parse(b)) {
        (None, None) => a.cmp(b),
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(x), Some(y)) => x.nums.cmp(&y.nums).then_with(|| match (x.pre, y.pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(p), Some(q)) => compare_pre(p, q),
        }),
    }
}

fn compare_pre(a: &str, b: &str) -> Ordering {
    let mut xs = a.split('.');
    let mut ys = b.split('.');
    loop {
        match (xs.next(), ys.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let o = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(m), Ok(n)) => m.cmp(&n),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    _ => x.cmp(y),
                };
                if o != Ordering::Equal {
                    return o;
                }
            }
        }
    }
}

/// Newest first; releases before pre-releases of the same list when `releases_first`.
pub fn sort_newest_first(versions: &mut [String]) {
    versions.sort_by(|a, b| compare(b, a));
}

/// The newest release (no pre-release) of a list, else the newest version.
pub fn latest(versions: &[String]) -> Option<String> {
    let release = versions
        .iter()
        .filter(|v| parse(v).is_some_and(|p| p.pre.is_none()))
        .max_by(|a, b| compare(a, b));
    release
        .or_else(|| versions.iter().max_by(|a, b| compare(a, b)))
        .cloned()
}

/// A module path in proxy URLs and the module cache: upper-case letters become `!` plus
/// the lower-case letter (`github.com/Azure/x` → `github.com/!azure/x`).
pub fn escape_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        if c.is_ascii_uppercase() {
            out.push('!');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The inverse of [`escape_path`]; `None` for an invalid escape.
pub fn unescape_path(path: &str) -> Option<String> {
    let mut out = String::with_capacity(path.len());
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        if c == '!' {
            let n = chars.next()?;
            if !n.is_ascii_lowercase() {
                return None;
            }
            out.push(n.to_ascii_uppercase());
        } else if c.is_ascii_uppercase() {
            return None;
        } else {
            out.push(c);
        }
    }
    Some(out)
}

/// The versions of a proxy `@v/list` answer (one per line), newest first.
pub fn parse_version_list(body: &str) -> Vec<String> {
    let mut v: Vec<String> = body
        .lines()
        .map(str::trim)
        .filter(|l| is_valid_version(l))
        .map(str::to_string)
        .collect();
    v.dedup();
    sort_newest_first(&mut v);
    v
}

/// The major version suffix a module path needs for `version` (`/v2` for v2.x.x).
pub fn major_suffix(version: &str) -> Option<String> {
    let p = parse(version)?;
    (p.nums[0] >= 2 && !version.ends_with("+incompatible")).then(|| format!("/v{}", p.nums[0]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_orders_versions() {
        for v in [
            "v1.2.3",
            "v0.0.0-20240101120000-abcdef123456",
            "v2.0.0+incompatible",
            "v1.0.0-rc.1",
        ] {
            assert!(is_valid_version(v), "{v}");
        }
        for v in ["1.2.3", "v1.2", "v01.2.3", "v1.2.3-", "latest"] {
            assert!(!is_valid_version(v), "{v}");
        }
        assert!(is_pseudo("v0.0.0-20240101120000-abcdef123456"));
        assert!(is_pseudo("v1.2.4-0.20240101120000-abcdef123456"));
        assert!(!is_pseudo("v1.2.4-rc.1"));
        assert_eq!(compare("v1.10.0", "v1.9.9"), Ordering::Greater);
        assert_eq!(compare("v1.0.0-rc.1", "v1.0.0"), Ordering::Less);
        assert_eq!(compare("v1.0.0-rc.2", "v1.0.0-rc.10"), Ordering::Less);
        let list = parse_version_list("v1.0.0\nv1.10.0\nv1.2.0\nv2.0.0-beta.1\nbogus\n");
        assert_eq!(list, ["v2.0.0-beta.1", "v1.10.0", "v1.2.0", "v1.0.0"]);
        assert_eq!(latest(&list).as_deref(), Some("v1.10.0"));
        assert_eq!(major_suffix("v3.1.0").as_deref(), Some("/v3"));
        assert_eq!(major_suffix("v1.1.0"), None);
    }

    #[test]
    fn escapes_module_paths() {
        assert_eq!(escape_path("github.com/Azure/go-SDK"), "github.com/!azure/go-!s!d!k");
        assert_eq!(
            unescape_path("github.com/!azure/go-!s!d!k").as_deref(),
            Some("github.com/Azure/go-SDK")
        );
        assert_eq!(unescape_path("bad/!1"), None);
        assert_eq!(unescape_path("Bad"), None);
    }
}
