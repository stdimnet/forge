//! The tools and their configurations (Settings | Languages & Frameworks | JavaScript |
//! Code Quality Tools): one configuration per tool, read from the `web.<id>.*` settings and
//! the tools' Actions on Save keys.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A web code quality tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WebTool {
    Eslint,
    Prettier,
    Stylelint,
    Biome,
}

impl WebTool {
    /// In the order the settings page lists them.
    pub const ALL: [WebTool; 4] = [
        WebTool::Eslint,
        WebTool::Prettier,
        WebTool::Stylelint,
        WebTool::Biome,
    ];

    /// The id in settings keys (`web.<id>.*`) and in the UI.
    pub fn id(self) -> &'static str {
        match self {
            WebTool::Eslint => "eslint",
            WebTool::Prettier => "prettier",
            WebTool::Stylelint => "stylelint",
            WebTool::Biome => "biome",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            WebTool::Eslint => "ESLint",
            WebTool::Prettier => "Prettier",
            WebTool::Stylelint => "Stylelint",
            WebTool::Biome => "Biome",
        }
    }

    /// The npm package (`node_modules/<package>`).
    pub fn package(self) -> &'static str {
        match self {
            WebTool::Biome => "@biomejs/biome",
            t => t.id(),
        }
    }

    /// The command name in the package's `bin` field.
    pub fn bin(self) -> &'static str {
        self.id()
    }

    /// The package's script when its package.json has no `bin` field.
    pub fn default_entry(self) -> &'static str {
        match self {
            WebTool::Eslint => "bin/eslint.js",
            WebTool::Prettier => "bin/prettier.cjs",
            WebTool::Stylelint => "bin/stylelint.mjs",
            WebTool::Biome => "bin/biome",
        }
    }

    /// Reports problems (on the fly, Inspect Code).
    pub fn lints(self) -> bool {
        self != WebTool::Prettier
    }

    /// Formats files (Reformat Code).
    pub fn formats(self) -> bool {
        matches!(self, WebTool::Prettier | WebTool::Biome)
    }

    /// The configuration files the tool finds on its own, nearest folder first; in one
    /// folder the first existing name wins.
    pub fn config_files(self) -> &'static [&'static str] {
        match self {
            WebTool::Eslint => &[
                "eslint.config.js",
                "eslint.config.mjs",
                "eslint.config.cjs",
                "eslint.config.ts",
                "eslint.config.mts",
                "eslint.config.cts",
                ".eslintrc.js",
                ".eslintrc.cjs",
                ".eslintrc.yaml",
                ".eslintrc.yml",
                ".eslintrc.json",
                ".eslintrc",
            ],
            WebTool::Prettier => &[
                ".prettierrc",
                ".prettierrc.json",
                ".prettierrc.yaml",
                ".prettierrc.yml",
                ".prettierrc.json5",
                ".prettierrc.js",
                ".prettierrc.mjs",
                ".prettierrc.cjs",
                ".prettierrc.toml",
                "prettier.config.js",
                "prettier.config.mjs",
                "prettier.config.cjs",
                "prettier.config.ts",
            ],
            WebTool::Stylelint => &[
                "stylelint.config.js",
                "stylelint.config.mjs",
                "stylelint.config.cjs",
                ".stylelintrc",
                ".stylelintrc.json",
                ".stylelintrc.yaml",
                ".stylelintrc.yml",
                ".stylelintrc.js",
                ".stylelintrc.mjs",
                ".stylelintrc.cjs",
            ],
            WebTool::Biome => &["biome.json", "biome.jsonc"],
        }
    }

    /// The package.json key that holds the tool's configuration.
    pub fn package_json_key(self) -> Option<&'static str> {
        match self {
            WebTool::Eslint => Some("eslintConfig"),
            WebTool::Prettier => Some("prettier"),
            WebTool::Stylelint => Some("stylelint"),
            WebTool::Biome => None,
        }
    }

    /// PHPStorm's default "Run for files" patterns.
    pub fn default_files(self) -> &'static str {
        match self {
            WebTool::Eslint => "{**/*,*}.{js,ts,jsx,tsx,cjs,mjs,vue,html}",
            WebTool::Prettier => DEFAULT_PRETTIER_FILES,
            WebTool::Stylelint => "{**/*,*}.{css,scss,less,vue}",
            WebTool::Biome => "{**/*,*}.{js,ts,jsx,tsx,cjs,cts,mjs,mts,json,jsonc,css}",
        }
    }

    /// The settings key of one option (`web.eslint.config`).
    pub fn key(self, option: &str) -> String {
        format!("web.{}.{option}", self.id())
    }

    /// "Run for files": Prettier keeps its key from Actions on Save.
    pub fn files_key(self) -> String {
        match self {
            WebTool::Prettier => "actionsOnSave.prettier.files".into(),
            t => t.key("files"),
        }
    }

    /// "Run … on save" (shared with Settings | Tools | Actions on Save).
    pub fn on_save_key(self) -> &'static str {
        match self {
            WebTool::Eslint => "actionsOnSave.eslint",
            WebTool::Prettier => "actionsOnSave.prettier.enabled",
            WebTool::Stylelint => "actionsOnSave.stylelint",
            WebTool::Biome => "actionsOnSave.biome",
        }
    }
}

/// Prettier's "Run for files" default (also Actions on Save's).
pub const DEFAULT_PRETTIER_FILES: &str =
    "*.js; *.ts; *.jsx; *.tsx; *.cjs; *.cts; *.mjs; *.mts; *.vue; *.astro";

/// Check the edited file while typing (debounced), not only when it opens and is saved.
pub const ON_THE_FLY_KEY: &str = "web.tools.onTheFly";
/// Seconds one run may take.
pub const TIMEOUT_KEY: &str = "web.tools.timeout";
/// The Node.js interpreter (Languages & Frameworks | Node.js).
pub const NODE_KEY: &str = "web.node.interpreter";
/// Biome takes over its file types from ESLint and Prettier when it applies to a file.
pub const BIOME_PRIORITY_KEY: &str = "web.biome.priority";

pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// PHPStorm's three ways to configure a tool.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Disabled,
    /// The package from the nearest `node_modules`, for files below a configuration file.
    #[default]
    Automatic,
    /// The configured package, configuration file and options, for every matching file.
    Manual,
}

/// One tool's configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ToolConfig {
    pub tool: WebTool,
    pub mode: Mode,
    /// Manual: the package folder (`node_modules/eslint`) or an executable; relative paths
    /// start at the project root. Empty: the nearest `node_modules/<package>`.
    pub package: String,
    /// Manual: the configuration file. Empty: the one the tool finds.
    pub config: String,
    /// Manual: extra command line options, split like a shell does.
    pub options: String,
    /// Manual (ESLint): working directories, `;`-separated; the deepest one holding a file
    /// is its working directory.
    pub working_dirs: String,
    /// "Run for files" patterns.
    pub files: String,
    /// "Run … on save".
    pub on_save: bool,
    /// Prettier, Biome: "Run on Reformat Code action".
    pub on_reformat: bool,
}

impl Default for ToolConfig {
    fn default() -> Self {
        Self::new(WebTool::Eslint)
    }
}

impl ToolConfig {
    pub fn new(tool: WebTool) -> Self {
        Self {
            tool,
            mode: Mode::Automatic,
            package: String::new(),
            config: String::new(),
            options: String::new(),
            working_dirs: String::new(),
            files: tool.default_files().to_string(),
            on_save: false,
            on_reformat: false,
        }
    }

    /// Read the tool's settings through `get` (the effective value of a key).
    pub fn from_settings(tool: WebTool, get: &dyn Fn(&str) -> Option<Value>) -> Self {
        let text = |k: &str| {
            get(k)
                .and_then(|v| v.as_str().map(|s| s.trim().to_string()))
                .unwrap_or_default()
        };
        let flag = |k: &str| get(k).and_then(|v| v.as_bool()).unwrap_or(false);
        let mut c = Self::new(tool);
        c.mode = get(&tool.key("mode"))
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        c.package = text(&tool.key("path"));
        c.config = text(&tool.key("config"));
        c.options = text(&tool.key("options"));
        c.working_dirs = text(&tool.key("workingDirectories"));
        let files = text(&tool.files_key());
        if !files.is_empty() {
            c.files = files;
        }
        c.on_save = flag(tool.on_save_key());
        c.on_reformat = flag(&tool.key("onReformat"));
        c
    }

    /// Manual settings count only in manual mode.
    fn manual(&self) -> bool {
        self.mode == Mode::Manual
    }

    pub fn custom_package(&self) -> Option<&str> {
        Some(self.package.as_str()).filter(|p| self.manual() && !p.is_empty())
    }

    pub fn custom_config(&self) -> Option<&str> {
        Some(self.config.as_str()).filter(|p| self.manual() && !p.is_empty())
    }

    /// The extra options as arguments (manual mode).
    pub fn extra_args(&self) -> Vec<String> {
        if self.manual() {
            forge_runtools::args::split_args(&self.options)
        } else {
            Vec::new()
        }
    }

    /// The manual working directories (project-relative, without trailing `/`).
    pub fn working_dirs(&self) -> Vec<String> {
        if !self.manual() {
            return Vec::new();
        }
        self.working_dirs
            .split([';', ','])
            .map(|d| d.trim().trim_start_matches("./").trim_end_matches('/'))
            .filter(|d| !d.is_empty())
            .map(|d| if d == "." { String::new() } else { d.to_string() })
            .collect()
    }

    /// Whether `path` (project-relative) matches "Run for files".
    pub fn matches(&self, path: &str) -> bool {
        forge_filetypes::matches_glob_list(path, &self.files)
    }
}

/// Every tool's configuration and the shared options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebToolsSettings {
    pub tools: Vec<ToolConfig>,
    pub on_the_fly: bool,
    pub timeout_secs: u64,
    /// Biome takes over its file types from ESLint and Prettier.
    pub biome_priority: bool,
    /// The configured Node.js interpreter (empty: detected).
    pub node: String,
}

impl WebToolsSettings {
    pub fn from_settings(get: &dyn Fn(&str) -> Option<Value>) -> Self {
        Self {
            tools: WebTool::ALL
                .iter()
                .map(|&t| ToolConfig::from_settings(t, get))
                .collect(),
            on_the_fly: get(ON_THE_FLY_KEY)
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            timeout_secs: get(TIMEOUT_KEY)
                .and_then(|v| v.as_u64())
                .filter(|t| *t > 0)
                .unwrap_or(DEFAULT_TIMEOUT_SECS),
            biome_priority: get(BIOME_PRIORITY_KEY)
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            node: get(NODE_KEY)
                .and_then(|v| v.as_str().map(|s| s.trim().to_string()))
                .unwrap_or_default(),
        }
    }

    pub fn config(&self, tool: WebTool) -> ToolConfig {
        self.tools
            .iter()
            .find(|c| c.tool == tool)
            .cloned()
            .unwrap_or_else(|| ToolConfig::new(tool))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ids_round_trip_and_keys() {
        for t in WebTool::ALL {
            assert_eq!(WebTool::from_id(t.id()), Some(t));
        }
        assert_eq!(WebTool::Eslint.key("path"), "web.eslint.path");
        assert_eq!(
            WebTool::Prettier.files_key(),
            "actionsOnSave.prettier.files"
        );
        assert_eq!(WebTool::Biome.package(), "@biomejs/biome");
    }

    #[test]
    fn reads_a_manual_configuration() {
        let get = |k: &str| match k {
            "web.eslint.mode" => Some(json!("manual")),
            "web.eslint.path" => Some(json!(" tools/eslint ")),
            "web.eslint.options" => Some(json!("--rule 'no-console: off' --quiet")),
            "web.eslint.workingDirectories" => Some(json!("packages/web/; ./packages/api ;")),
            "actionsOnSave.eslint" => Some(json!(true)),
            "web.tools.timeout" => Some(json!(90)),
            _ => None,
        };
        let s = WebToolsSettings::from_settings(&get);
        let c = s.config(WebTool::Eslint);
        assert_eq!(c.mode, Mode::Manual);
        assert_eq!(c.custom_package(), Some("tools/eslint"));
        assert_eq!(c.extra_args(), vec!["--rule", "no-console: off", "--quiet"]);
        assert_eq!(c.working_dirs(), vec!["packages/web", "packages/api"]);
        assert!(c.on_save);
        assert_eq!(s.timeout_secs, 90);
        assert!(s.on_the_fly && s.biome_priority);
        // Automatic mode ignores the manual fields.
        let auto = ToolConfig {
            mode: Mode::Automatic,
            ..c
        };
        assert_eq!(auto.custom_package(), None);
        assert!(auto.extra_args().is_empty());
    }

    #[test]
    fn default_patterns_match_their_files() {
        let c = |t| ToolConfig::new(t);
        assert!(c(WebTool::Eslint).matches("src/App.tsx"));
        assert!(c(WebTool::Eslint).matches("index.html"));
        assert!(!c(WebTool::Eslint).matches("src/app.scss"));
        assert!(c(WebTool::Stylelint).matches("src/app/card.scss"));
        assert!(c(WebTool::Stylelint).matches("src/App.vue"));
        assert!(c(WebTool::Biome).matches("biome.json"));
        assert!(c(WebTool::Prettier).matches("src/main.ts"));
    }
}
