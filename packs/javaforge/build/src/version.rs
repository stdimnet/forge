//! Maven version ordering (a simplified `ComparableVersion`): numbers compare as numbers,
//! qualifiers as `alpha < beta < milestone < rc < snapshot < release < sp`, unknown
//! qualifiers after releases in text order. Used for "a newer version is available" and to
//! sort the versions completion offers.

use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Num(u64),
    Text(String),
}

fn qualifier_rank(q: &str) -> Option<i32> {
    Some(match q {
        "alpha" | "a" => 1,
        "beta" | "b" => 2,
        "milestone" | "m" => 3,
        "rc" | "cr" | "preview" | "pre" | "ea" => 4,
        "snapshot" => 5,
        "" | "ga" | "final" | "release" | "jre" | "android" => 6,
        "sp" => 7,
        _ => return None,
    })
}

fn parts(v: &str) -> Vec<Part> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut digit: Option<bool> = None;
    let flush = |cur: &mut String, out: &mut Vec<Part>, digit: Option<bool>| {
        if cur.is_empty() {
            return;
        }
        out.push(if digit == Some(true) {
            Part::Num(cur.parse().unwrap_or(u64::MAX))
        } else {
            Part::Text(cur.to_ascii_lowercase())
        });
        cur.clear();
    };
    for c in v.trim().chars() {
        if c == '.' || c == '-' || c == '_' || c == '+' {
            flush(&mut cur, &mut out, digit);
            digit = None;
            continue;
        }
        let d = c.is_ascii_digit();
        if digit.is_some_and(|x| x != d) {
            flush(&mut cur, &mut out, digit);
        }
        digit = Some(d);
        cur.push(c);
    }
    flush(&mut cur, &mut out, digit);
    // `1.0.0` == `1`: trailing zeros do not count.
    while matches!(out.last(), Some(Part::Num(0))) {
        out.pop();
    }
    out
}

fn cmp_part(a: Option<&Part>, b: Option<&Part>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (Some(Part::Num(x)), Some(Part::Num(y))) => x.cmp(y),
        (Some(Part::Num(x)), None) => {
            if *x == 0 {
                Ordering::Equal
            } else {
                Ordering::Greater
            }
        }
        (None, Some(Part::Num(_))) => cmp_part(b, a).reverse(),
        // A number is newer than a qualifier (`1.1` > `1-rc1`, `1.0.1` > `1.0-sp`).
        (Some(Part::Num(_)), Some(Part::Text(_))) => Ordering::Greater,
        (Some(Part::Text(_)), Some(Part::Num(_))) => Ordering::Less,
        (Some(Part::Text(x)), None) => text_vs_release(x),
        (None, Some(Part::Text(y))) => text_vs_release(y).reverse(),
        (Some(Part::Text(x)), Some(Part::Text(y))) => {
            match (qualifier_rank(x), qualifier_rank(y)) {
                (Some(p), Some(q)) => p.cmp(&q),
                (Some(p), None) => p.cmp(&6).then(Ordering::Less),
                (None, Some(q)) => 6.cmp(&q).then(Ordering::Greater),
                (None, None) => x.cmp(y),
            }
        }
    }
}

fn text_vs_release(q: &str) -> Ordering {
    match qualifier_rank(q) {
        Some(r) => r.cmp(&6),
        None => Ordering::Greater,
    }
}

/// Compare two Maven versions.
pub fn compare(a: &str, b: &str) -> Ordering {
    let (pa, pb) = (parts(a), parts(b));
    for i in 0..pa.len().max(pb.len()) {
        let o = cmp_part(pa.get(i), pb.get(i));
        if o != Ordering::Equal {
            return o;
        }
    }
    Ordering::Equal
}

/// Not a release: alpha, beta, milestone, RC, snapshot, preview or early access builds.
pub fn is_prerelease(v: &str) -> bool {
    parts(v).iter().any(|p| match p {
        Part::Text(t) => qualifier_rank(t).is_some_and(|r| r < 6),
        Part::Num(_) => false,
    })
}

/// The newest version, releases only when `stable` (falling back to all of them).
pub fn latest<'a>(
    versions: impl IntoIterator<Item = &'a str> + Clone,
    stable: bool,
) -> Option<&'a str> {
    let pick = |only_stable: bool| {
        versions
            .clone()
            .into_iter()
            .filter(|v| !v.trim().is_empty() && (!only_stable || !is_prerelease(v)))
            .max_by(|a, b| compare(a, b))
    };
    if stable {
        pick(true).or_else(|| pick(false))
    } else {
        pick(false)
    }
}

/// Whether `latest` is newer than `current` and worth an update: a pre-release only
/// replaces a pre-release.
pub fn is_newer(current: &str, latest: &str) -> bool {
    if current.contains("${")
        || current.contains('[')
        || current.contains('(')
        || current.contains('+')
        || current.ends_with("latest.release")
    {
        return false;
    }
    if is_prerelease(latest) && !is_prerelease(current) {
        return false;
    }
    compare(latest, current) == Ordering::Greater
}

/// The variant a version belongs to (Guava's `jre` / `android`): its text parts that are
/// not pre-release qualifiers.
fn flavor(v: &str) -> Vec<String> {
    parts(v)
        .into_iter()
        .filter_map(|p| match p {
            Part::Text(t) if qualifier_rank(&t).is_none_or(|r| r >= 6) => Some(t),
            _ => None,
        })
        .collect()
}

/// The version to update `current` to: the newest of the same variant (a release unless
/// `current` is a pre-release), when it is newer.
pub fn update_for(current: &str, versions: &[String]) -> Option<String> {
    let want = flavor(current);
    let same: Vec<&str> = versions
        .iter()
        .map(String::as_str)
        .filter(|v| flavor(v) == want)
        .collect();
    let pool: Vec<&str> = if same.is_empty() {
        versions.iter().map(String::as_str).collect()
    } else {
        same
    };
    let pick = latest(pool.iter().copied(), !is_prerelease(current))?;
    is_newer(current, pick).then(|| pick.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_maven_versions() {
        let mut v = vec![
            "1.0",
            "1.0-rc1",
            "1.0-alpha-2",
            "1.0.1",
            "1.0-SNAPSHOT",
            "1.0-beta",
            "2.0.0-M1",
            "1.10",
            "1.9",
            "1.0-sp1",
        ];
        v.sort_by(|a, b| compare(a, b));
        assert_eq!(
            v,
            [
                "1.0-alpha-2",
                "1.0-beta",
                "1.0-rc1",
                "1.0-SNAPSHOT",
                "1.0",
                "1.0-sp1",
                "1.0.1",
                "1.9",
                "1.10",
                "2.0.0-M1"
            ]
        );
        assert_eq!(compare("1.0.0", "1"), Ordering::Equal);
        assert_eq!(compare("33.3.1-jre", "33.2.0-jre"), Ordering::Greater);
        assert_eq!(compare("33.3.1-jre", "33.3.1-android"), Ordering::Equal);
        assert_eq!(compare("5.11.3", "5.11.0-RC1"), Ordering::Greater);
    }

    #[test]
    fn latest_and_newer() {
        let all = ["5.10.0", "5.11.3", "5.12.0-M1", "5.9.3"];
        assert_eq!(latest(all, true), Some("5.11.3"));
        assert_eq!(latest(all, false), Some("5.12.0-M1"));
        assert_eq!(latest(["1.0-rc1"], true), Some("1.0-rc1"));
        assert!(
            is_prerelease("6.0.0-M1")
                && is_prerelease("1.0-SNAPSHOT")
                && !is_prerelease("33.3.1-jre")
        );
        assert!(is_newer("5.10.0", "5.11.3"));
        assert!(!is_newer("5.11.3", "5.11.3"));
        assert!(!is_newer("5.11.3", "5.12.0-M1"));
        assert!(is_newer("5.12.0-M1", "5.12.0"));
        assert!(!is_newer("${junit.version}", "5.11.3"));
        assert!(!is_newer("[1.0,2.0)", "5"));
        let guava: Vec<String> = [
            "33.4.0-android",
            "33.4.0-jre",
            "32.0.0-jre",
            "33.5.0-rc1-jre",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            update_for("32.0.0-jre", &guava).as_deref(),
            Some("33.4.0-jre")
        );
        assert_eq!(
            update_for("32.0.0-android", &guava).as_deref(),
            Some("33.4.0-android")
        );
        assert_eq!(update_for("33.4.0-jre", &guava), None);
        assert_eq!(
            update_for("1.0", &["1.1".to_string()]).as_deref(),
            Some("1.1")
        );
    }
}
