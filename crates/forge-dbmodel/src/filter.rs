//! The schema filter of a data source (its Schemas tab), like DataGrip's introspection
//! scope. Each pattern is one of:
//!
//! - `name`: a schema by name (case-insensitive),
//! - a glob with `*` and `?`: `*` is every schema, `tenant_*` some of them,
//! - `:current`: the connection's current schema,
//! - `/regex/` (or `/regex/i` to ignore case): a regular expression,
//! - `!pattern`: excludes what `pattern` matches (after the other patterns).
//!
//! No patterns means `:current` (the first schema when none is current); only exclusions
//! start from every schema. The filter is applied when the data source is introspected, so
//! hidden schemas are never loaded.

use regex_lite::Regex;

use crate::model::SchemaName;

#[derive(Debug, Clone)]
enum Matcher {
    Current,
    Glob(String),
    Regex(Regex),
}

impl Matcher {
    fn parse(text: &str) -> Result<Self, String> {
        let t = text.trim();
        if t.eq_ignore_ascii_case(":current") {
            return Ok(Matcher::Current);
        }
        if let Some(body) = t.strip_prefix('/') {
            let (body, flags) = match body.rfind('/') {
                Some(i) => (&body[..i], &body[i + 1..]),
                None => (body, ""),
            };
            let src = if flags.contains('i') {
                format!("(?i){body}")
            } else {
                body.to_string()
            };
            let re = Regex::new(&format!("^(?:{src})$"))
                .map_err(|e| format!("invalid regular expression {t}: {e}"))?;
            return Ok(Matcher::Regex(re));
        }
        Ok(Matcher::Glob(t.to_lowercase()))
    }

    fn matches(&self, s: &SchemaName) -> bool {
        match self {
            Matcher::Current => s.current,
            Matcher::Glob(g) => glob(g.as_bytes(), s.name.to_lowercase().as_bytes()),
            Matcher::Regex(re) => re.is_match(&s.name),
        }
    }
}

/// `*` and `?` wildcards over bytes (names are compared lowercased).
fn glob(p: &[u8], s: &[u8]) -> bool {
    let (mut pi, mut si) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while si < s.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some((pi, si));
            pi += 1;
        } else if let Some((sp, ss)) = star {
            pi = sp + 1;
            si = ss + 1;
            star = Some((sp, ss + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == b'*')
}

/// A parsed schema filter.
#[derive(Debug, Clone, Default)]
pub struct SchemaFilter {
    include: Vec<Matcher>,
    exclude: Vec<Matcher>,
}

impl SchemaFilter {
    /// Parse patterns; entries may also hold several patterns separated by `,` or new lines.
    pub fn parse<S: AsRef<str>>(patterns: &[S]) -> Result<Self, String> {
        let mut f = SchemaFilter::default();
        for entry in patterns {
            for p in entry
                .as_ref()
                .split([',', '\n'])
                .map(str::trim)
                .filter(|p| !p.is_empty())
            {
                match p.strip_prefix('!') {
                    Some(rest) => f.exclude.push(Matcher::parse(rest)?),
                    None => f.include.push(Matcher::parse(p)?),
                }
            }
        }
        Ok(f)
    }

    /// Set `visible` on every schema.
    pub fn apply(&self, schemas: &mut [SchemaName]) {
        let default_current = self.include.is_empty() && self.exclude.is_empty();
        let any_current = schemas.iter().any(|s| s.current);
        for (i, s) in schemas.iter_mut().enumerate() {
            let included = if default_current {
                s.current || (!any_current && i == 0)
            } else if self.include.is_empty() {
                true
            } else {
                self.include.iter().any(|m| m.matches(s))
            };
            s.visible = included && !self.exclude.iter().any(|m| m.matches(s));
        }
    }

    /// Whether one schema passes (`current`: it is the connection's current schema).
    pub fn matches(&self, name: &str, current: bool) -> bool {
        let mut one = [SchemaName::new(name, current)];
        if self.include.is_empty() && self.exclude.is_empty() {
            return current;
        }
        self.apply(&mut one);
        one[0].visible
    }
}

/// Apply `patterns` to `schemas`; invalid patterns are an error.
pub fn apply_patterns<S: AsRef<str>>(
    patterns: &[S],
    schemas: &mut [SchemaName],
) -> Result<(), String> {
    SchemaFilter::parse(patterns)?.apply(schemas);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(current: &str, all: &[&str]) -> Vec<SchemaName> {
        all.iter()
            .map(|n| SchemaName::new(*n, *n == current))
            .collect()
    }

    fn visible(patterns: &[&str], schemas: &[SchemaName]) -> Vec<String> {
        let mut s = schemas.to_vec();
        apply_patterns(patterns, &mut s).unwrap();
        s.into_iter()
            .filter(|s| s.visible)
            .map(|s| s.name)
            .collect()
    }

    #[test]
    fn default_is_the_current_schema() {
        let s = names("shop", &["information_schema", "mysql", "shop", "sys"]);
        assert_eq!(visible(&[], &s), ["shop"]);
        let none = names("", &["b", "a"]);
        assert_eq!(visible(&[], &none), ["b"]);
    }

    #[test]
    fn globs_current_regex_and_exclusions() {
        let s = names(
            "public",
            &[
                "audit", "pg_toast", "public", "tenant_1", "tenant_2", "TenantX",
            ],
        );
        assert_eq!(visible(&["*"], &s).len(), 6);
        assert_eq!(visible(&["tenant_*"], &s), ["tenant_1", "tenant_2"]);
        assert_eq!(visible(&["tenant_?"], &s), ["tenant_1", "tenant_2"]);
        assert_eq!(visible(&[":current", "audit"], &s), ["audit", "public"]);
        assert_eq!(visible(&["/tenant_\\d+/"], &s), ["tenant_1", "tenant_2"]);
        assert_eq!(
            visible(&["/tenant.*/i"], &s),
            ["tenant_1", "tenant_2", "TenantX"]
        );
        assert_eq!(
            visible(&["*", "!pg_*", "!/Tenant.*/"], &s),
            ["audit", "public", "tenant_1", "tenant_2"]
        );
        assert_eq!(
            visible(&["!pg_*, !tenant_*"], &s),
            ["audit", "public", "TenantX"]
        );
        assert_eq!(visible(&["PUBLIC"], &s), ["public"]);
    }

    #[test]
    fn rejects_bad_regex() {
        assert!(SchemaFilter::parse(&["/(/"]).is_err());
        let f = SchemaFilter::parse(&["sales"]).unwrap();
        assert!(f.matches("SALES", false));
        assert!(!f.matches("hr", true));
        assert!(SchemaFilter::parse::<&str>(&[]).unwrap().matches("x", true));
    }
}
