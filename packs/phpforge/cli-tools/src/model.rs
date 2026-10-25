//! Command line tools as configured in Settings | Tools | Command Line Tool Support, their
//! settings codec (an `objects` setting: tables with string values) and auto-detection.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The kind of a tool: which list format it understands and how it is started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolKind {
    #[serde(rename = "composer")]
    Composer,
    #[serde(rename = "artisan")]
    Artisan,
    #[serde(rename = "symfony")]
    Symfony,
    #[serde(rename = "wp-cli")]
    WpCli,
    #[serde(rename = "drush")]
    Drush,
    /// "Tool based on Symfony Console": any `bin/…` script built on Symfony Console.
    #[serde(rename = "symfony-based")]
    SymfonyBased,
    /// "Tool based on PHP script": a PHP script without a command list.
    #[serde(rename = "php-script")]
    PhpScript,
}

/// How a tool describes its commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListFormat {
    /// `list --format=json` of Symfony Console.
    Symfony,
    /// `wp cli cmd-dump`.
    WpCli,
    /// No command list (completion offers nothing but history).
    None,
}

impl ToolKind {
    pub const ALL: [ToolKind; 7] = [
        ToolKind::Composer,
        ToolKind::Artisan,
        ToolKind::Symfony,
        ToolKind::WpCli,
        ToolKind::Drush,
        ToolKind::SymfonyBased,
        ToolKind::PhpScript,
    ];

    pub fn id(self) -> &'static str {
        match self {
            ToolKind::Composer => "composer",
            ToolKind::Artisan => "artisan",
            ToolKind::Symfony => "symfony",
            ToolKind::WpCli => "wp-cli",
            ToolKind::Drush => "drush",
            ToolKind::SymfonyBased => "symfony-based",
            ToolKind::PhpScript => "php-script",
        }
    }

    pub fn from_id(id: &str) -> Option<ToolKind> {
        ToolKind::ALL.into_iter().find(|k| k.id() == id)
    }

    /// The name PHPStorm shows in the "Choose Tool to Add" list.
    pub fn label(self) -> &'static str {
        match self {
            ToolKind::Composer => "Composer",
            ToolKind::Artisan => "Laravel Artisan",
            ToolKind::Symfony => "Symfony Console",
            ToolKind::WpCli => "WP-CLI",
            ToolKind::Drush => "Drush",
            ToolKind::SymfonyBased => "Tool based on Symfony Console",
            ToolKind::PhpScript => "Tool based on PHP script",
        }
    }

    pub fn default_alias(self) -> &'static str {
        match self {
            ToolKind::Composer => "c",
            ToolKind::Artisan => "a",
            ToolKind::Symfony => "s",
            ToolKind::WpCli => "wp",
            ToolKind::Drush => "drush",
            ToolKind::SymfonyBased | ToolKind::PhpScript => "",
        }
    }

    /// The path a new tool of this kind starts with (project-relative or a program name).
    pub fn default_path(self) -> &'static str {
        match self {
            ToolKind::Composer => "composer",
            ToolKind::Artisan => "artisan",
            ToolKind::Symfony => "bin/console",
            ToolKind::WpCli => "wp",
            ToolKind::Drush => "vendor/bin/drush",
            ToolKind::SymfonyBased | ToolKind::PhpScript => "",
        }
    }

    pub fn format(self) -> ListFormat {
        match self {
            ToolKind::WpCli => ListFormat::WpCli,
            ToolKind::PhpScript => ListFormat::None,
            _ => ListFormat::Symfony,
        }
    }
}

/// "Show console in": where the output of a command goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShowConsoleIn {
    #[default]
    ToolWindow,
    Popup,
}

impl ShowConsoleIn {
    pub fn id(self) -> &'static str {
        match self {
            ShowConsoleIn::ToolWindow => "toolWindow",
            ShowConsoleIn::Popup => "popup",
        }
    }
}

/// A configured (or detected) command line tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub kind: ToolKind,
    /// Shown in the settings table and the console; the kind's label when empty.
    pub name: String,
    /// What the console input starts with: `c install`, `a make:model Order`.
    pub alias: String,
    /// Project-relative script (`artisan`, `bin/console`), a `.phar`, or a program name
    /// (`composer`, `wp`) looked up on the interpreter's machine.
    pub path: String,
    /// PHP CLI interpreter name; empty = the project default (`php.interpreter`).
    pub interpreter: String,
    pub show_console_in: ShowConsoleIn,
    pub enabled: bool,
    /// Found by auto-detection, not stored in the settings.
    #[serde(default)]
    pub detected: bool,
}

impl Tool {
    /// A tool of `kind` with the kind's defaults.
    pub fn new(kind: ToolKind) -> Tool {
        Tool {
            kind,
            name: kind.label().into(),
            alias: kind.default_alias().into(),
            path: kind.default_path().into(),
            interpreter: String::new(),
            show_console_in: ShowConsoleIn::default(),
            enabled: true,
            detected: false,
        }
    }

    /// The name to show: the configured one, else the kind's label.
    pub fn display_name(&self) -> &str {
        if self.name.trim().is_empty() {
            self.kind.label()
        } else {
            &self.name
        }
    }

    /// Decode one entry of `commandLineTools.tools`. Entries without a known kind or an
    /// alias are skipped.
    pub fn from_object(entry: &Map<String, Value>) -> Option<Tool> {
        let get = |k: &str| entry.get(k).and_then(Value::as_str).unwrap_or("").trim();
        let kind = ToolKind::from_id(get("kind"))?;
        let alias = get("alias");
        if alias.is_empty() || alias.chars().any(char::is_whitespace) {
            return None;
        }
        Some(Tool {
            kind,
            name: get("name").into(),
            alias: alias.into(),
            path: match get("path") {
                "" => kind.default_path().into(),
                p => p.into(),
            },
            interpreter: get("interpreter").into(),
            show_console_in: match get("showConsoleIn") {
                "popup" => ShowConsoleIn::Popup,
                _ => ShowConsoleIn::ToolWindow,
            },
            enabled: get("enabled") != "false",
            detected: false,
        })
    }

    /// The settings entry of this tool (string values only).
    pub fn to_object(&self) -> Map<String, Value> {
        let mut m = Map::new();
        let mut put = |k: &str, v: &str| {
            m.insert(k.into(), Value::String(v.into()));
        };
        put("kind", self.kind.id());
        put("name", &self.name);
        put("alias", &self.alias);
        put("path", &self.path);
        if !self.interpreter.is_empty() {
            put("interpreter", &self.interpreter);
        }
        put("showConsoleIn", self.show_console_in.id());
        if !self.enabled {
            put("enabled", "false");
        }
        m
    }
}

/// Decode `commandLineTools.tools` (a JSON array of objects); invalid entries and later
/// duplicates of an alias are dropped.
pub fn tools_from_setting(value: Option<&Value>) -> Vec<Tool> {
    let mut out: Vec<Tool> = Vec::new();
    for entry in value.and_then(Value::as_array).into_iter().flatten() {
        if let Some(tool) = entry.as_object().and_then(Tool::from_object) {
            if !out.iter().any(|t| t.alias == tool.alias) {
                out.push(tool);
            }
        }
    }
    out
}

/// The tools a project uses, from the project-relative paths that exist (`composer.json`,
/// `artisan`, `bin/console`, `wp-cli.phar`, `vendor/bin/wp`, `wp-config.php`, `wp-content`,
/// `vendor/bin/drush`). `composer` is the Composer executable setting.
pub fn detect(exists: &dyn Fn(&str) -> bool, composer: &str) -> Vec<Tool> {
    let mut out = Vec::new();
    let mut add = |kind: ToolKind, path: &str| {
        let mut t = Tool::new(kind);
        t.path = path.into();
        t.detected = true;
        out.push(t);
    };
    if exists("composer.json") {
        add(
            ToolKind::Composer,
            match composer.trim() {
                "" => "composer",
                c => c,
            },
        );
    }
    if exists("artisan") {
        add(ToolKind::Artisan, "artisan");
    }
    if exists("bin/console") {
        add(ToolKind::Symfony, "bin/console");
    }
    if exists("wp-cli.phar") {
        add(ToolKind::WpCli, "wp-cli.phar");
    } else if exists("vendor/bin/wp") {
        add(ToolKind::WpCli, "vendor/bin/wp");
    } else if exists("wp-config.php") || exists("wp-load.php") || exists("wp-content") {
        add(ToolKind::WpCli, "wp");
    }
    if exists("vendor/bin/drush") {
        add(ToolKind::Drush, "vendor/bin/drush");
    }
    out
}

/// The paths [`detect`] looks at.
pub const DETECTION_FILES: &[&str] = &[
    "composer.json",
    "artisan",
    "bin/console",
    "wp-cli.phar",
    "vendor/bin/wp",
    "wp-config.php",
    "wp-load.php",
    "wp-content",
    "vendor/bin/drush",
];

/// The configured tools, followed by detected ones (when `auto_detect` is on) whose kind
/// is not configured and whose alias is free.
pub fn effective_tools(configured: Vec<Tool>, detected: Vec<Tool>, auto_detect: bool) -> Vec<Tool> {
    let mut out = configured;
    if auto_detect {
        for d in detected {
            if !out.iter().any(|t| t.kind == d.kind || t.alias == d.alias) {
                out.push(d);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn settings_round_trip_and_validation() {
        let value = json!([
            { "kind": "artisan", "alias": "a", "path": "", "showConsoleIn": "popup" },
            { "kind": "composer", "alias": "c", "path": "tools/composer.phar", "interpreter": "Docker PHP 8.3", "enabled": "false" },
            { "kind": "nope", "alias": "x" },
            { "kind": "symfony", "alias": "" },
            { "kind": "symfony", "alias": "a" },
        ]);
        let tools = tools_from_setting(Some(&value));
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].path, "artisan");
        assert_eq!(tools[0].show_console_in, ShowConsoleIn::Popup);
        assert_eq!(tools[0].display_name(), "Laravel Artisan");
        assert!(!tools[1].enabled);
        assert_eq!(tools[1].interpreter, "Docker PHP 8.3");
        let back = Tool::from_object(&tools[1].to_object()).unwrap();
        assert_eq!(back, tools[1]);
    }

    #[test]
    fn detects_the_tools_a_project_uses() {
        let files = [
            "composer.json",
            "artisan",
            "wp-config.php",
            "vendor/bin/drush",
        ];
        let tools = detect(&|p| files.contains(&p), "");
        let summary: Vec<(&str, &str)> = tools
            .iter()
            .map(|t| (t.alias.as_str(), t.path.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                ("c", "composer"),
                ("a", "artisan"),
                ("wp", "wp"),
                ("drush", "vendor/bin/drush")
            ]
        );
        assert!(tools.iter().all(|t| t.detected));
        let phar = detect(&|p| p == "wp-cli.phar" || p == "wp-config.php", "");
        assert_eq!(phar[0].path, "wp-cli.phar");
    }

    #[test]
    fn configured_tools_win_over_detected_ones() {
        let mut composer = Tool::new(ToolKind::Composer);
        composer.alias = "comp".into();
        let detected = detect(&|p| p == "composer.json" || p == "artisan", "");
        let all = effective_tools(vec![composer.clone()], detected.clone(), true);
        assert_eq!(
            all.iter().map(|t| t.alias.as_str()).collect::<Vec<_>>(),
            ["comp", "a"]
        );
        assert_eq!(effective_tools(vec![composer], detected, false).len(), 1);
    }
}
