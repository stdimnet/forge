//! Files guests never see (setting `codeWithMe.excludedFiles`): `.env`, private keys,
//! credentials. Patterns work like `.gitignore` lines:
//!
//! - a pattern without `/` matches the file or folder name at any depth (`*.pem`);
//! - a pattern with `/` is relative to the project root (`.forge/dataSources.toml`);
//! - a trailing `/` matches folders only (`secrets/`), and everything inside them;
//! - `!pattern` shows a file again (`!.env.example`); the last matching line wins.

use globset::{Glob, GlobBuilder, GlobMatcher};

use crate::{Error, Result};

/// The default value of `codeWithMe.excludedFiles`.
pub const DEFAULT_PATTERNS: &[&str] = &[
    ".env",
    ".env.*",
    "!.env.example",
    "!.env.dist",
    "*.pem",
    "*.key",
    "*.p12",
    "*.pfx",
    "id_rsa*",
    "id_ed25519*",
    "auth.json",
    ".npmrc",
    ".pypirc",
    ".netrc",
    "*.kdbx",
    ".forge/dataSources.toml",
    ".forge/*.local.toml",
    "config/master.key",
];

#[derive(Debug, Clone)]
struct Rule {
    matcher: GlobMatcher,
    negated: bool,
    /// Matched against the name only (no `/` in the pattern).
    name_only: bool,
    dir_only: bool,
}

/// A compiled exclude list.
#[derive(Debug, Clone, Default)]
pub struct Excludes {
    rules: Vec<Rule>,
    patterns: Vec<String>,
}

impl Excludes {
    /// Compile patterns; blank lines and `#` comments are skipped.
    pub fn new<S: AsRef<str>>(patterns: &[S]) -> Result<Self> {
        let mut rules = Vec::new();
        let mut kept = Vec::new();
        for raw in patterns {
            let line = raw.as_ref().trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (negated, rest) = match line.strip_prefix('!') {
                Some(r) => (true, r),
                None => (false, line),
            };
            let (dir_only, rest) = match rest.strip_suffix('/') {
                Some(r) => (true, r),
                None => (false, rest),
            };
            let rest = rest.strip_prefix('/').unwrap_or(rest);
            if rest.is_empty() {
                return Err(Error::InvalidPattern(line.to_string()));
            }
            // Case-insensitive: `.ENV` is the same file as `.env` on macOS and Windows.
            let glob: Glob = GlobBuilder::new(rest)
                .literal_separator(true)
                .case_insensitive(true)
                .build()
                .map_err(|_| Error::InvalidPattern(line.to_string()))?;
            rules.push(Rule {
                matcher: glob.compile_matcher(),
                negated,
                name_only: !rest.contains('/'),
                dir_only,
            });
            kept.push(line.to_string());
        }
        Ok(Self {
            rules,
            patterns: kept,
        })
    }

    /// The default list.
    pub fn defaults() -> Self {
        Self::new(DEFAULT_PATTERNS).expect("default patterns compile")
    }

    /// The active patterns (for "Files matching N exclude patterns are hidden").
    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }

    /// Whether a workspace-relative `/`-separated path is hidden from guests, either itself
    /// or through one of its folders.
    pub fn is_excluded(&self, path: &str) -> bool {
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        (1..=segments.len()).any(|n| {
            let is_dir = n < segments.len();
            self.matches(&segments[..n].join("/"), segments[n - 1], is_dir)
        })
    }

    fn matches(&self, full: &str, name: &str, is_dir: bool) -> bool {
        let mut excluded = false;
        for r in &self.rules {
            if r.dir_only && !is_dir {
                continue;
            }
            let hit = if r.name_only {
                r.matcher.is_match(name)
            } else {
                r.matcher.is_match(full)
            };
            if hit {
                excluded = !r.negated;
            }
        }
        excluded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_hide_secrets_but_not_examples() {
        let e = Excludes::defaults();
        for hidden in [
            ".env",
            ".env.local",
            "docker/.env.production",
            "storage/oauth-private.key",
            "certs/server.pem",
            "home/id_rsa",
            "home/id_ed25519.pub",
            "auth.json",
            ".npmrc",
            "secrets.kdbx",
            ".forge/dataSources.toml",
            ".forge/settings.local.toml",
            "config/master.key",
        ] {
            assert!(e.is_excluded(hidden), "{hidden} is visible");
        }
        for visible in [
            ".env.example",
            ".env.dist",
            "app/Models/User.php",
            "environment.php",
            ".forge/settings.toml",
            "config/app.php",
            "composer.json",
            "vendor/autoload.php",
        ] {
            assert!(!e.is_excluded(visible), "{visible} is hidden");
        }
    }

    #[test]
    fn matching_ignores_case_because_some_file_systems_do() {
        let e = Excludes::defaults();
        assert!(e.is_excluded(".ENV"));
        assert!(e.is_excluded("Keys/Server.PEM"));
        assert!(e.is_excluded("Auth.json"));
        assert!(!e.is_excluded(".ENV.EXAMPLE"));
    }

    #[test]
    fn folder_patterns_hide_their_contents() {
        let e = Excludes::new(&["secrets/", "/build/out"]).unwrap();
        assert!(e.is_excluded("secrets/a.txt"));
        assert!(e.is_excluded("app/secrets/deep/a.txt"));
        assert!(!e.is_excluded("secrets"), "a file named like the folder");
        assert!(e.is_excluded("build/out"));
        assert!(e.is_excluded("build/out/app.js"));
        assert!(!e.is_excluded("src/build/out"));
    }

    #[test]
    fn last_match_wins_and_comments_are_skipped() {
        let e = Excludes::new(&["# comment", "", "*.log", "!keep.log"]).unwrap();
        assert_eq!(
            e.patterns(),
            &["*.log".to_string(), "!keep.log".to_string()]
        );
        assert!(e.is_excluded("storage/laravel.log"));
        assert!(!e.is_excluded("keep.log"));
    }

    #[test]
    fn invalid_patterns_are_reported() {
        assert!(Excludes::new(&["["]).is_err());
        assert!(Excludes::new(&["!"]).is_err());
        assert!(Excludes::new(&["/"]).is_err());
    }
}
