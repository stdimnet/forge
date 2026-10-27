//! The tools and their configurations (Settings | Languages & Frameworks | PHP | Quality
//! Tools): one configuration per tool, read from the `php.tools.<id>.*` settings.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A PHP quality tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QualityTool {
    Phpstan,
    Psalm,
    PhpCsFixer,
    Phpcs,
    Phpmd,
    Pint,
    Rector,
}

/// What the tool does with a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolKind {
    /// Reports problems only (PHPStan, Psalm, PHP Mess Detector).
    Analyzer,
    /// Reports what it would change and can fix it (PHP CS Fixer, phpcs/phpcbf, Pint, Rector).
    Fixer,
}

impl QualityTool {
    /// In the order the Quality Tools page lists them (PHPStorm's order).
    pub const ALL: [QualityTool; 7] = [
        QualityTool::Phpcs,
        QualityTool::PhpCsFixer,
        QualityTool::Phpmd,
        QualityTool::Phpstan,
        QualityTool::Psalm,
        QualityTool::Pint,
        QualityTool::Rector,
    ];

    /// The id in settings keys (`php.tools.<id>.*`) and in the UI.
    pub fn id(self) -> &'static str {
        match self {
            QualityTool::Phpstan => "phpstan",
            QualityTool::Psalm => "psalm",
            QualityTool::PhpCsFixer => "phpCsFixer",
            QualityTool::Phpcs => "phpcs",
            QualityTool::Phpmd => "phpmd",
            QualityTool::Pint => "pint",
            QualityTool::Rector => "rector",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            QualityTool::Phpstan => "PHPStan",
            QualityTool::Psalm => "Psalm",
            QualityTool::PhpCsFixer => "PHP CS Fixer",
            QualityTool::Phpcs => "PHP_CodeSniffer",
            QualityTool::Phpmd => "PHP Mess Detector",
            QualityTool::Pint => "Laravel Pint",
            QualityTool::Rector => "Rector",
        }
    }

    pub fn kind(self) -> ToolKind {
        match self {
            QualityTool::Phpstan | QualityTool::Psalm | QualityTool::Phpmd => ToolKind::Analyzer,
            _ => ToolKind::Fixer,
        }
    }

    /// The executable's name in `vendor/bin`.
    pub fn binary(self) -> &'static str {
        match self {
            QualityTool::Phpstan => "phpstan",
            QualityTool::Psalm => "psalm",
            QualityTool::PhpCsFixer => "php-cs-fixer",
            QualityTool::Phpcs => "phpcs",
            QualityTool::Phpmd => "phpmd",
            QualityTool::Pint => "pint",
            QualityTool::Rector => "rector",
        }
    }

    /// The executable that fixes files, when it is another one (`phpcbf` next to `phpcs`).
    pub fn fixer_binary(self) -> &'static str {
        match self {
            QualityTool::Phpcs => "phpcbf",
            t => t.binary(),
        }
    }

    /// The fixer's name in menus ("Reformat with phpcbf").
    pub fn fixer_label(self) -> &'static str {
        match self {
            QualityTool::Phpcs => "phpcbf",
            QualityTool::Pint => "Pint",
            t => t.label(),
        }
    }

    /// Where the project keeps the tool, relative to the root; the first existing one wins.
    pub fn candidates(self) -> Vec<String> {
        bin_candidates(self.binary())
    }

    pub fn fixer_candidates(self) -> Vec<String> {
        bin_candidates(self.fixer_binary())
    }

    /// The configuration files the tool finds on its own in the project root.
    pub fn config_files(self) -> &'static [&'static str] {
        match self {
            QualityTool::Phpstan => &["phpstan.neon", "phpstan.neon.dist", "phpstan.dist.neon"],
            QualityTool::Psalm => &["psalm.xml", "psalm.xml.dist"],
            QualityTool::PhpCsFixer => &[".php-cs-fixer.php", ".php-cs-fixer.dist.php"],
            QualityTool::Phpcs => &[
                ".phpcs.xml",
                "phpcs.xml",
                ".phpcs.xml.dist",
                "phpcs.xml.dist",
            ],
            QualityTool::Phpmd => &["phpmd.xml", "phpmd.xml.dist", ".phpmd.xml"],
            QualityTool::Pint => &["pint.json"],
            QualityTool::Rector => &["rector.php"],
        }
    }

    /// Whether the tool can check the editor text before it is saved: through stdin
    /// (phpcs, PHP CS Fixer) or a temporary copy (`phpstan --tmp-file … --instead-of …`).
    pub fn checks_unsaved_text(self) -> bool {
        matches!(
            self,
            QualityTool::Phpcs | QualityTool::PhpCsFixer | QualityTool::Phpstan
        )
    }

    /// The settings key of one option of this tool (`php.tools.phpstan.level`).
    pub fn key(self, option: &str) -> String {
        format!("php.tools.{}.{option}", self.id())
    }
}

fn bin_candidates(name: &str) -> Vec<String> {
    vec![
        format!("vendor/bin/{name}"),
        format!("vendor/bin/{name}.bat"),
    ]
}

/// How problems of a tool are shown in the editor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SeverityChoice {
    /// As the tool reports them (errors stay errors, PHPMD priorities 1–2 are warnings, …).
    #[default]
    Tool,
    Error,
    Warning,
    Weak,
}

/// The default timeout of one run, in seconds.
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Run the analyzers on the editor text while typing (debounced), not only on save.
pub const ON_THE_FLY_KEY: &str = "php.tools.onTheFly";

/// The project interpreter (`php.interpreter`) instead of a named one.
pub const PROJECT_INTERPRETER: &str = "project";

/// One tool's configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ToolConfig {
    pub tool: QualityTool,
    /// The "On/Off" switch of the tool's inspection (on the fly, on save, Inspect Code).
    pub inspection: bool,
    /// Custom executable: absolute or relative to the project root; empty = `vendor/bin`.
    pub path: String,
    /// phpcbf's custom path (PHP_CodeSniffer only).
    pub fixer_path: String,
    /// The PHP interpreter the tool runs with: empty = the executable runs directly where
    /// the project lives, `project` = the project interpreter, else a configured name.
    pub interpreter: String,
    /// Configuration file; empty = what the tool finds in the project.
    pub config: String,
    /// PHPStan level, phpcs standard, PHPMD rulesets, PHP CS Fixer rules, Pint preset.
    pub level: String,
    /// Extra command line options, split like a shell does.
    pub options: String,
    pub timeout_secs: u64,
    pub severity: SeverityChoice,
}

impl Default for ToolConfig {
    fn default() -> Self {
        Self::new(QualityTool::Phpstan)
    }
}

impl ToolConfig {
    pub fn new(tool: QualityTool) -> Self {
        Self {
            tool,
            inspection: false,
            path: String::new(),
            fixer_path: String::new(),
            interpreter: String::new(),
            config: String::new(),
            level: String::new(),
            options: String::new(),
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            severity: SeverityChoice::Tool,
        }
    }

    /// Read the tool's settings through `get` (the effective value of a key).
    pub fn from_settings(tool: QualityTool, get: &dyn Fn(&str) -> Option<Value>) -> Self {
        let text = |o: &str| {
            get(&tool.key(o))
                .and_then(|v| v.as_str().map(|s| s.trim().to_string()))
                .unwrap_or_default()
        };
        let mut c = Self::new(tool);
        c.inspection = get(&tool.key("inspection"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        c.path = text("path");
        c.fixer_path = text("phpcbfPath");
        c.interpreter = text("interpreter");
        c.config = text("config");
        c.level = text("level");
        c.options = text("options");
        c.timeout_secs = get(&tool.key("timeout"))
            .and_then(|v| v.as_u64())
            .filter(|t| *t > 0)
            .unwrap_or(DEFAULT_TIMEOUT_SECS);
        c.severity = get(&tool.key("severity"))
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        c
    }

    /// The custom executable for analyzing (`fix = false`) or fixing.
    pub fn custom_path(&self, fix: bool) -> Option<&str> {
        let p = if fix && self.tool == QualityTool::Phpcs {
            &self.fixer_path
        } else {
            &self.path
        };
        Some(p.as_str()).filter(|p| !p.is_empty())
    }

    /// The extra options as arguments.
    pub fn extra_args(&self) -> Vec<String> {
        split_args(&self.options)
    }
}

/// Every tool's configuration.
pub fn configs_from_settings(get: &dyn Fn(&str) -> Option<Value>) -> Vec<ToolConfig> {
    QualityTool::ALL
        .iter()
        .map(|&t| ToolConfig::from_settings(t, get))
        .collect()
}

/// Split a command line like a POSIX shell: whitespace separates, quotes group.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut has = false;
    let mut quote: Option<char> = None;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => match chars.next() {
                Some(n) => cur.push(n),
                None => cur.push('\\'),
            },
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                has = true;
            }
            (None, c) if c.is_whitespace() => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ids_round_trip_and_keys() {
        for t in QualityTool::ALL {
            assert_eq!(QualityTool::from_id(t.id()), Some(t));
        }
        assert_eq!(QualityTool::Phpstan.key("level"), "php.tools.phpstan.level");
        // The keys of the earlier PHP CS Fixer / Pint paths stay.
        assert_eq!(
            QualityTool::PhpCsFixer.key("path"),
            "php.tools.phpCsFixer.path"
        );
        assert_eq!(
            QualityTool::Phpcs.fixer_candidates()[0],
            "vendor/bin/phpcbf"
        );
    }

    #[test]
    fn reads_a_configuration() {
        let get = |k: &str| match k {
            "php.tools.phpstan.inspection" => Some(json!(true)),
            "php.tools.phpstan.level" => Some(json!(" 6 ")),
            "php.tools.phpstan.interpreter" => Some(json!("Docker PHP 8.3")),
            "php.tools.phpstan.timeout" => Some(json!(60)),
            "php.tools.phpstan.severity" => Some(json!("weak")),
            "php.tools.phpstan.options" => Some(json!("--memory-limit=1G --xdebug")),
            _ => None,
        };
        let c = ToolConfig::from_settings(QualityTool::Phpstan, &get);
        assert!(c.inspection);
        assert_eq!(c.level, "6");
        assert_eq!(c.interpreter, "Docker PHP 8.3");
        assert_eq!(c.timeout_secs, 60);
        assert_eq!(c.severity, SeverityChoice::Weak);
        assert_eq!(c.extra_args(), vec!["--memory-limit=1G", "--xdebug"]);
        let off = ToolConfig::from_settings(QualityTool::Psalm, &get);
        assert!(!off.inspection);
        assert_eq!(off.timeout_secs, DEFAULT_TIMEOUT_SECS);
    }

    #[test]
    fn splits_options_like_a_shell() {
        assert_eq!(
            split_args(r#"--a "b c" 'd e' f\g "" --h="x y""#),
            vec!["--a", "b c", "d e", "f\\g", "", "--h=x y"]
        );
        assert!(split_args("   ").is_empty());
    }
}
