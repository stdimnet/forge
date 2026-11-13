//! Composer version constraints (`^1.2`, `~1.2.3`, `1.2.*`, `>=1.0 <2.0`, `1.0 - 2.0`,
//! `^7.4 || ^8.0`, `dev-main`, `@dev` / `@beta` stability flags, `1.0.x-dev as 1.0.0`
//! aliases), parsed into ranges like Composer's `VersionParser::parseConstraints`.

use crate::version::{Stability, Version};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq)]
enum Bound {
    Any,
    Cmp(Op, Version),
    Branch(String),
}

impl Bound {
    fn matches(&self, v: &Version) -> bool {
        match self {
            Bound::Any => true,
            Bound::Branch(name) => v.branch() == Some(name.as_str()),
            Bound::Cmp(op, b) => {
                if v.branch().is_some() || b.branch().is_some() {
                    return match op {
                        Op::Eq => v == b,
                        Op::Ne => v != b,
                        _ => false,
                    };
                }
                let ord = v.cmp(b);
                match op {
                    Op::Eq => ord.is_eq(),
                    Op::Ne => !ord.is_eq(),
                    Op::Lt => ord.is_lt(),
                    Op::Le => ord.is_le(),
                    Op::Gt => ord.is_gt(),
                    Op::Ge => ord.is_ge(),
                }
            }
        }
    }
}

/// A parsed constraint: alternatives (`||`) of conjunctions (space or comma).
#[derive(Debug, Clone, PartialEq)]
pub struct Constraint {
    alternatives: Vec<Vec<Bound>>,
    /// The lowest stability the constraint asks for: an `@flag` or a pre-release written
    /// in it (`^2.0@beta`, `>=1.0-RC1`).
    stability: Option<Stability>,
}

impl Constraint {
    /// Parse a constraint; `None` when Composer would reject it.
    pub fn parse(text: &str) -> Option<Constraint> {
        let mut text = text.trim();
        // `1.0.x-dev as 1.0.0`: the alias does not restrict anything.
        if let Some(i) = text.find(" as ") {
            text = text[..i].trim();
        }
        if text.is_empty() {
            return None;
        }
        let mut stability: Option<Stability> = None;
        let mut alternatives = Vec::new();
        for alt in split_or(text) {
            let mut bounds = Vec::new();
            for token in and_tokens(alt)? {
                let (bs, st) = parse_single(&token)?;
                if let Some(st) = st {
                    stability = Some(stability.map_or(st, |s| s.min(st)));
                }
                bounds.extend(bs);
            }
            if bounds.is_empty() {
                return None;
            }
            alternatives.push(bounds);
        }
        Some(Constraint {
            alternatives,
            stability,
        })
    }

    /// Whether `v` is in the range, whatever its stability.
    pub fn matches(&self, v: &Version) -> bool {
        self.alternatives
            .iter()
            .any(|all| all.iter().all(|b| b.matches(v)))
    }

    /// Whether Composer could install `v` for this constraint with `minimum-stability`
    /// `minimum`: in the range and stable enough (the constraint's own flags lower the
    /// minimum).
    pub fn allows(&self, v: &Version, minimum: Stability) -> bool {
        let min = self.stability.map_or(minimum, |s| s.min(minimum));
        self.matches(v) && v.stability() >= min
    }

    /// The newest of `versions` the constraint allows (stable unless the constraint asks
    /// for less).
    pub fn latest_allowed<'a, I>(&self, versions: I) -> Option<Version>
    where
        I: IntoIterator<Item = &'a str>,
    {
        versions
            .into_iter()
            .filter_map(Version::parse)
            .filter(|v| !v.is_branch() && self.allows(v, Stability::Stable))
            .max()
    }
}

/// The constraint `composer require` writes for a version: `^2.1` for 2.1.3, `^0.4.2` for
/// 0.4.2 and `^3.0@beta` for 3.0.0-beta1 (Composer's `VersionSelector`).
pub fn recommended(v: &Version) -> String {
    if v.is_branch() {
        return v.original().to_string();
    }
    let base = if v.major() == 0 {
        format!("^0.{}.{}", v.minor(), v.patch())
    } else {
        format!("^{}.{}", v.major(), v.minor())
    };
    match v.stability() {
        Stability::Stable => base,
        s => format!("{base}@{}", s.name()),
    }
}

/// Split at `||` (or a single `|`), Composer's OR.
fn split_or(text: &str) -> Vec<&str> {
    text.split('|')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

const OPS: [&str; 9] = [">=", "<=", "<>", "!=", "==", ">", "<", "=", "~>"];

/// The AND-ed parts of one alternative: split at commas and spaces, keeping `>= 1.0`
/// together and joining hyphen ranges (`1.0 - 2.0`) into one token.
fn and_tokens(alt: &str) -> Option<Vec<String>> {
    let raw: Vec<&str> = alt
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let t = raw[i];
        if OPS.contains(&t) || t == "^" || t == "~" {
            // An operator written apart from its version.
            let next = raw.get(i + 1)?;
            out.push(format!("{t}{next}"));
            i += 2;
            continue;
        }
        if raw.get(i + 1) == Some(&"-") {
            let upper = raw.get(i + 2)?;
            out.push(format!("{t} - {upper}"));
            i += 3;
            continue;
        }
        out.push(t.to_string());
        i += 1;
    }
    Some(out)
}

/// Numbers of a (partial) version: `1.2` → `[1, 2]` and the rest (`-beta1`, `.*`).
fn numbers(s: &str) -> Option<(Vec<u64>, &str)> {
    let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
    let mut parts = Vec::new();
    let mut rest = s;
    loop {
        let end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        if end == 0 {
            break;
        }
        parts.push(rest[..end].parse().ok()?);
        rest = &rest[end..];
        match rest.strip_prefix('.') {
            Some(r) if r.starts_with(|c: char| c.is_ascii_digit()) => rest = r,
            _ => break,
        }
    }
    (!parts.is_empty() && parts.len() <= 4).then_some((parts, rest))
}

fn padded(parts: &[u64]) -> [u64; 4] {
    let mut out = [0; 4];
    for (i, p) in parts.iter().take(4).enumerate() {
        out[i] = *p;
    }
    out
}

/// `parts` with the part at `index` incremented and the following ones zeroed.
fn bump(parts: &[u64], index: usize) -> [u64; 4] {
    let mut out = padded(parts);
    out[index] += 1;
    for p in out.iter_mut().skip(index + 1) {
        *p = 0;
    }
    out
}

/// A version with an explicit suffix, or the lowest version of its line (`-dev`).
fn lower(text: &str, parts: &[u64]) -> Option<(Version, Option<Stability>)> {
    let v = Version::parse(text)?;
    if v.has_suffix() {
        let st = v.stability();
        Some((v, (st != Stability::Stable).then_some(st)))
    } else {
        Some((Version::dev_of(padded(parts)), None))
    }
}

type Parsed = (Vec<Bound>, Option<Stability>);

fn parse_single(token: &str) -> Option<Parsed> {
    // `@dev` / `^1.0@beta` stability flags.
    let (token, flag) = match token.rfind('@') {
        Some(i) => (&token[..i], Some(Stability::parse(&token[i + 1..])?)),
        None => (token, None),
    };
    let merge = |(b, s): Parsed| -> Parsed {
        let st = match (s, flag) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        (b, st)
    };
    // A commit reference (`dev-main#abc123`) pins nothing for us.
    let token = token.split('#').next().unwrap_or(token);
    if token.is_empty() || matches!(token, "*" | "x" | "X" | "*.*" | "*.*.*") {
        return Some(merge((vec![Bound::Any], None)));
    }
    let lower_token = token.to_ascii_lowercase();
    if let Some(name) = lower_token.strip_prefix("dev-") {
        return Some(merge((vec![Bound::Branch(name.to_string())], None)));
    }
    if lower_token.ends_with("-dev") && lower_token.contains(".x") {
        let v = Version::parse(token)?;
        return Some(merge((vec![Bound::Cmp(Op::Eq, v)], None)));
    }
    // Hyphen range.
    if let Some((a, b)) = token.split_once(" - ") {
        let (pa, _) = numbers(a)?;
        let (pb, rest_b) = numbers(b)?;
        let (low, st) = lower(a, &pa)?;
        let high = if pb.len() < 3 && rest_b.is_empty() {
            Bound::Cmp(Op::Lt, Version::dev_of(bump(&pb, pb.len() - 1)))
        } else {
            Bound::Cmp(Op::Le, Version::parse(b)?)
        };
        return Some(merge((vec![Bound::Cmp(Op::Ge, low), high], st)));
    }
    if let Some(v) = token.strip_prefix('^') {
        let (p, _) = numbers(v)?;
        let (low, st) = lower(v, &p)?;
        let index = if p[0] > 0 || p.len() == 1 {
            0
        } else if p[1] > 0 || p.len() == 2 {
            1
        } else {
            2
        };
        let high = Version::dev_of(bump(&p, index));
        return Some(merge((
            vec![Bound::Cmp(Op::Ge, low), Bound::Cmp(Op::Lt, high)],
            st,
        )));
    }
    if let Some(v) = token.strip_prefix("~>").or_else(|| token.strip_prefix('~')) {
        let (p, _) = numbers(v)?;
        let (low, st) = lower(v, &p)?;
        let index = p.len().saturating_sub(2);
        let high = Version::dev_of(bump(&p, index));
        return Some(merge((
            vec![Bound::Cmp(Op::Ge, low), Bound::Cmp(Op::Lt, high)],
            st,
        )));
    }
    for op in OPS {
        if let Some(v) = token.strip_prefix(op) {
            let o = match op {
                ">=" => Op::Ge,
                "<=" => Op::Le,
                "<>" | "!=" => Op::Ne,
                "==" | "=" => Op::Eq,
                ">" => Op::Gt,
                "<" => Op::Lt,
                _ => return None,
            };
            if v.to_ascii_lowercase().starts_with("dev-") {
                return Some(merge((vec![Bound::Cmp(o, Version::parse(v)?)], None)));
            }
            let (p, _) = numbers(v)?;
            let parsed = Version::parse(v)?;
            let st = (parsed.stability() != Stability::Stable).then(|| parsed.stability());
            let bound = match o {
                // `>=1.0` and `<2.0` include / exclude the pre-releases of the version.
                Op::Ge | Op::Lt if !parsed.has_suffix() => Version::dev_of(padded(&p)),
                _ => parsed,
            };
            return Some(merge((vec![Bound::Cmp(o, bound)], st)));
        }
    }
    // Wildcards: `1.2.*`, `1.x`.
    let (p, rest) = numbers(token)?;
    if matches!(rest, ".*" | ".x" | ".X") {
        let low = Version::dev_of(padded(&p));
        let high = Version::dev_of(bump(&p, p.len() - 1));
        return Some(merge((
            vec![Bound::Cmp(Op::Ge, low), Bound::Cmp(Op::Lt, high)],
            None,
        )));
    }
    // An exact version.
    let v = Version::parse(token)?;
    let st = (v.stability() != Stability::Stable).then(|| v.stability());
    Some(merge((vec![Bound::Cmp(Op::Eq, v)], st)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(constraint: &str, version: &str) -> bool {
        let c =
            Constraint::parse(constraint).unwrap_or_else(|| panic!("cannot parse {constraint}"));
        c.matches(&Version::parse(version).unwrap())
    }

    #[test]
    fn caret() {
        assert!(ok("^1.2.3", "1.2.3"));
        assert!(ok("^1.2.3", "1.9.0"));
        assert!(!ok("^1.2.3", "2.0.0"));
        assert!(!ok("^1.2.3", "2.0.0-beta1"));
        assert!(!ok("^1.2.3", "1.2.2"));
        assert!(ok("^1.2.3", "1.2.3-RC1"));
        assert!(ok("^0.3", "0.3.9"));
        assert!(!ok("^0.3", "0.4.0"));
        assert!(ok("^0.0.3", "0.0.3"));
        assert!(!ok("^0.0.3", "0.0.4"));
        assert!(ok("^0", "0.9"));
        assert!(!ok("^0", "1.0"));
        assert!(ok("^v12.0", "v12.3.0"));
    }

    #[test]
    fn tilde_and_wildcards() {
        assert!(ok("~1.2", "1.9"));
        assert!(!ok("~1.2", "2.0"));
        assert!(ok("~1.2.3", "1.2.9"));
        assert!(!ok("~1.2.3", "1.3.0"));
        assert!(ok("~1", "1.5"));
        assert!(ok("1.2.*", "1.2.7"));
        assert!(!ok("1.2.*", "1.3.0"));
        assert!(ok("1.*", "1.99"));
        assert!(ok("*", "0.0.1"));
        assert!(ok("1.x", "1.4"));
    }

    #[test]
    fn operators_ranges_and_alternatives() {
        assert!(ok(">=1.0 <2.0", "1.5"));
        assert!(!ok(">=1.0 <2.0", "2.0"));
        assert!(ok(">=1.0,<2.0", "1.0.0"));
        assert!(ok(">= 1.0 < 2.0", "1.1"));
        assert!(ok(">1.0", "1.0.1"));
        assert!(!ok(">1.0", "1.0"));
        assert!(ok("<=1.0", "1.0"));
        assert!(ok("!=1.0", "1.1"));
        assert!(!ok("!=1.0", "1.0.0"));
        assert!(ok("1.0 - 2.0", "2.0.5"));
        assert!(!ok("1.0 - 2.0", "2.1.0"));
        assert!(ok("1.0.0 - 2.1.3", "2.1.3"));
        assert!(!ok("1.0.0 - 2.1.3", "2.1.4"));
        assert!(ok("^7.4 || ^8.0", "8.3.1"));
        assert!(ok("^7.4|^8.0", "7.4.0"));
        assert!(!ok("^7.4 || ^8.0", "7.3.0"));
        assert!(ok("1.2.3", "v1.2.3"));
        assert!(!ok("1.2.3", "1.2.4"));
    }

    #[test]
    fn branches_flags_and_aliases() {
        assert!(ok("dev-main", "dev-main"));
        assert!(!ok("dev-main", "1.0.0"));
        assert!(!ok("^1.0", "dev-main"));
        assert!(ok("dev-main#abc123", "dev-main"));
        assert!(ok("1.0.x-dev as 1.0.0", "1.0.x-dev"));
        assert!(ok("@dev", "3.0.0-alpha1"));

        let beta = Constraint::parse("^2.0@beta").unwrap();
        let b1 = Version::parse("2.1.0-beta1").unwrap();
        assert!(beta.allows(&b1, Stability::Stable));
        let plain = Constraint::parse("^2.0").unwrap();
        assert!(plain.matches(&b1));
        assert!(!plain.allows(&b1, Stability::Stable));
        assert!(plain.allows(&b1, Stability::Beta));
        assert!(Constraint::parse(">=2.0-RC1")
            .unwrap()
            .allows(&Version::parse("2.0.0-RC2").unwrap(), Stability::Stable));
    }

    #[test]
    fn rejects_garbage() {
        for bad in ["", "banana", "^", ">=", "1.0 -", "^1.0@foo", "1.2.3.4.5"] {
            assert!(Constraint::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn latest_allowed_and_recommended() {
        let versions = ["v12.0.0", "v12.3.0", "v13.0.0", "v12.4.0-beta1", "dev-main"];
        let c = Constraint::parse("^12.0").unwrap();
        assert_eq!(c.latest_allowed(versions).unwrap().original(), "v12.3.0");
        let r = |s: &str| recommended(&Version::parse(s).unwrap());
        assert_eq!(r("v12.3.1"), "^12.3");
        assert_eq!(r("0.4.2"), "^0.4.2");
        assert_eq!(r("3.0.0-beta1"), "^3.0@beta");
        assert_eq!(r("dev-main"), "dev-main");
    }
}
