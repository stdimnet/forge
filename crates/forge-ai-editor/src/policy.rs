//! Whether code may be sent to an AI provider: the per-project switch (`ai.enabled`), the
//! languages turned off for inline completion and the `.aiignore` file in the project root
//! (gitignore syntax; `.aiexclude` is read too). Every AI request with code passes
//! [`Policy::check`] first.

use serde::Serialize;

/// Files read for exclusions, in the project root.
pub const IGNORE_FILES: [&str; 2] = [".aiignore", ".aiexclude"];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    negate: bool,
    /// Only matches directories (pattern ended with `/`).
    dir_only: bool,
    /// Matched against the whole path (pattern contained a `/`), else against each name.
    anchored: bool,
    segments: Vec<String>,
}

/// Parsed `.aiignore` rules. The last matching rule wins; `!` re-includes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AiIgnore {
    rules: Vec<Rule>,
}

impl AiIgnore {
    pub fn parse(text: &str) -> Self {
        let mut rules = Vec::new();
        for line in text.lines() {
            let line = line.trim_end();
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let (negate, pat) = match line.strip_prefix('!') {
                Some(p) => (true, p),
                None => (false, line.strip_prefix('\\').unwrap_or(line)),
            };
            let dir_only = pat.ends_with('/');
            let pat = pat.trim_end_matches('/');
            let anchored = pat.contains('/');
            let pat = pat.trim_start_matches('/');
            if pat.is_empty() {
                continue;
            }
            rules.push(Rule {
                negate,
                dir_only,
                anchored,
                segments: pat.split('/').map(String::from).collect(),
            });
        }
        Self { rules }
    }

    /// Several ignore files merged (later files win).
    pub fn merged<'a>(texts: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            rules: texts
                .into_iter()
                .flat_map(|t| Self::parse(t).rules)
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Whether the project-relative `path` (a file) is excluded, itself or through one of
    /// its folders.
    pub fn is_ignored(&self, path: &str) -> bool {
        let path = path.trim_start_matches("./").trim_start_matches('/');
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        // A folder excluded by a rule excludes everything below it (like git, a file
        // inside cannot be re-included).
        for end in 1..parts.len() {
            if self.verdict(&parts[..end], true) == Some(true) {
                return true;
            }
        }
        self.verdict(&parts, false) == Some(true)
    }

    fn verdict(&self, parts: &[&str], is_dir: bool) -> Option<bool> {
        let mut out = None;
        for r in &self.rules {
            if r.dir_only && !is_dir {
                continue;
            }
            let hit = if r.anchored {
                glob_segments(&r.segments, parts)
            } else {
                parts.last().is_some_and(|name| glob(&r.segments[0], name))
            };
            if hit {
                out = Some(!r.negate);
            }
        }
        out
    }
}

/// Path match where `**` spans any number of segments.
fn glob_segments(pat: &[String], parts: &[&str]) -> bool {
    match pat.first() {
        None => parts.is_empty(),
        Some(p) if p == "**" => (0..=parts.len()).any(|k| glob_segments(&pat[1..], &parts[k..])),
        Some(p) => !parts.is_empty() && glob(p, parts[0]) && glob_segments(&pat[1..], &parts[1..]),
    }
}

/// One-segment glob: `*`, `?` and `[abc]` / `[a-z]` / `[!x]`.
fn glob(pat: &str, name: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let n: Vec<char> = name.chars().collect();
    glob_at(&p, &n)
}

fn glob_at(p: &[char], n: &[char]) -> bool {
    match p.first() {
        None => n.is_empty(),
        Some('*') => (0..=n.len()).any(|k| glob_at(&p[1..], &n[k..])),
        Some('?') => !n.is_empty() && glob_at(&p[1..], &n[1..]),
        Some('[') => {
            let Some(close) = p.iter().skip(1).position(|&c| c == ']').map(|i| i + 1) else {
                return !n.is_empty() && n[0] == '[' && glob_at(&p[1..], &n[1..]);
            };
            let Some(&c) = n.first() else {
                return false;
            };
            let mut set = &p[1..close];
            let negate = matches!(set.first(), Some('!') | Some('^'));
            if negate {
                set = &set[1..];
            }
            let mut hit = false;
            let mut i = 0;
            while i < set.len() {
                if i + 2 < set.len() && set[i + 1] == '-' {
                    hit |= set[i] <= c && c <= set[i + 2];
                    i += 3;
                } else {
                    hit |= set[i] == c;
                    i += 1;
                }
            }
            hit != negate && glob_at(&p[close + 1..], &n[1..])
        }
        Some(&c) => n.first() == Some(&c) && glob_at(&p[1..], &n[1..]),
    }
}

/// Which feature asks: inline completion also honours the per-language switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    InlineCompletion,
    Action,
}

/// The answer of [`Policy::check`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum Verdict {
    Allowed,
    /// AI is turned off for the project (Settings | Tools | AI Assistant).
    ProjectDisabled {
        message: String,
    },
    /// Inline completion is off for the file's language.
    LanguageDisabled {
        message: String,
    },
    /// The file matches `.aiignore`.
    Ignored {
        message: String,
    },
}

impl Verdict {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Verdict::Allowed)
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            Verdict::Allowed => None,
            Verdict::ProjectDisabled { message }
            | Verdict::LanguageDisabled { message }
            | Verdict::Ignored { message } => Some(message),
        }
    }
}

/// The settings and exclusions that decide whether code may leave the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// `ai.enabled` (project setting).
    pub project_enabled: bool,
    /// `ai.completion.disabledLanguages`: language ids (`markdown`, `php`, ...).
    pub disabled_languages: Vec<String>,
    pub ignore: AiIgnore,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            project_enabled: true,
            disabled_languages: Vec::new(),
            ignore: AiIgnore::default(),
        }
    }
}

impl Policy {
    /// May code of the project-relative `path` (language id `language`) be sent?
    pub fn check(&self, path: &str, language: &str, feature: Feature) -> Verdict {
        if !self.project_enabled {
            return Verdict::ProjectDisabled {
                message: "AI features are turned off for this project".into(),
            };
        }
        if self.ignore.is_ignored(path) {
            return Verdict::Ignored {
                message: format!("{path} is excluded from AI by .aiignore"),
            };
        }
        if feature == Feature::InlineCompletion
            && self
                .disabled_languages
                .iter()
                .any(|l| l.trim().eq_ignore_ascii_case(language))
        {
            return Verdict::LanguageDisabled {
                message: format!("Inline completion is turned off for {language}"),
            };
        }
        Verdict::Allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_gitignore_style_patterns() {
        let ig = AiIgnore::parse(
            "# secrets\n.env\n*.pem\nconfig/secrets/\n/build\nsrc/**/generated/*.ts\n!keep.pem\nnotes-[0-9].md\n",
        );
        assert!(ig.is_ignored(".env"));
        assert!(ig.is_ignored("deploy/.env"));
        assert!(ig.is_ignored("certs/server.pem"));
        assert!(!ig.is_ignored("certs/keep.pem"));
        assert!(ig.is_ignored("config/secrets/db.php"));
        assert!(!ig.is_ignored("config/secrets.php"));
        assert!(ig.is_ignored("build/app.js"));
        assert!(!ig.is_ignored("src/build/app.js"));
        assert!(ig.is_ignored("src/a/b/generated/api.ts"));
        assert!(ig.is_ignored("src/generated/api.ts"));
        assert!(!ig.is_ignored("src/generated/api.js"));
        assert!(ig.is_ignored("notes-3.md"));
        assert!(!ig.is_ignored("notes-x.md"));
        assert!(!ig.is_ignored("app/Models/User.php"));
    }

    #[test]
    fn excluded_folders_cannot_be_reincluded_by_files() {
        let ig = AiIgnore::merged(["vendor/\n", "!vendor/acme/lib.php\n"]);
        assert!(ig.is_ignored("vendor/acme/lib.php"));
        assert!(!AiIgnore::parse("").is_ignored("a"));
        assert!(AiIgnore::parse("").is_empty());
    }

    #[test]
    fn checks_project_switch_ignore_file_and_languages() {
        let mut p = Policy {
            disabled_languages: vec!["Markdown".into()],
            ignore: AiIgnore::parse("*.secret.php"),
            ..Policy::default()
        };
        assert!(p
            .check("a.php", "php", Feature::InlineCompletion)
            .is_allowed());
        assert!(matches!(
            p.check("README.md", "markdown", Feature::InlineCompletion),
            Verdict::LanguageDisabled { .. }
        ));
        assert!(p
            .check("README.md", "markdown", Feature::Action)
            .is_allowed());
        let v = p.check("keys.secret.php", "php", Feature::Action);
        assert_eq!(
            v.message(),
            Some("keys.secret.php is excluded from AI by .aiignore")
        );
        p.project_enabled = false;
        assert!(matches!(
            p.check("a.php", "php", Feature::Action),
            Verdict::ProjectDisabled { .. }
        ));
        assert_eq!(
            serde_json::to_value(Verdict::Allowed).unwrap(),
            serde_json::json!({ "status": "allowed" })
        );
    }
}
