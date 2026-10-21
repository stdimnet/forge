//! How to launch a PHP language server: Intelephense (preferred, users bring their own
//! licence key) with phpactor as fallback. Independent of the generic LSP client; the app
//! turns [`LspServerConfig`] into a running server.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServerChoice {
    /// Intelephense if installed, else phpactor.
    #[default]
    Auto,
    Intelephense,
    Phpactor,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServerKind {
    Intelephense,
    Phpactor,
}

/// User settings relevant to the PHP language server (Settings > Languages & Frameworks > PHP).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PhpLspSettings {
    pub server: ServerChoice,
    /// Intelephense premium licence key (optional).
    pub licence_key: Option<String>,
    /// PHP language level, e.g. `8.3`.
    pub php_version: String,
    /// Intelephense `files.maxSize` in bytes.
    pub max_file_size: u64,
    /// Extra stub sets on top of the defaults.
    pub extra_stubs: Vec<String>,
    /// Additional include paths (`intelephense.environment.includePaths`).
    pub include_paths: Vec<String>,
    /// Explicit executable paths (skip PATH detection).
    pub intelephense_path: Option<PathBuf>,
    /// Run `intelephense_path` (then Intelephense's JavaScript entry point) with this
    /// Node.js: Forge's managed install needs no Node.js on `PATH`.
    pub intelephense_node: Option<PathBuf>,
    pub phpactor_path: Option<PathBuf>,
    /// Where Intelephense stores its cache; defaults to `<cache dir>/forge/intelephense`.
    pub storage_path: Option<PathBuf>,
    /// Brace placement of Reformat Code (`php.codeStyle.braces`).
    pub braces: crate::style::Braces,
    /// composer.json, relative to the project root (`php.composer.json`).
    pub composer_json: String,
    /// The language level follows `require.php` of composer.json.
    pub sync_with_composer: bool,
    /// Packages in `vendor/` are indexed and resolved as libraries.
    pub vendor_libraries: bool,
}

impl Default for PhpLspSettings {
    fn default() -> Self {
        Self {
            server: ServerChoice::Auto,
            licence_key: None,
            php_version: "8.3".into(),
            max_file_size: 5_000_000,
            extra_stubs: Vec::new(),
            include_paths: Vec::new(),
            intelephense_path: None,
            intelephense_node: None,
            phpactor_path: None,
            storage_path: None,
            braces: crate::style::Braces::Psr12,
            composer_json: "composer.json".into(),
            sync_with_composer: true,
            vendor_libraries: true,
        }
    }
}

impl PhpLspSettings {
    /// The language level to use: from composer.json when synchronized and it has a
    /// `require.php` constraint, else `php_version`.
    pub fn effective_level(&self, composer_json: Option<&str>) -> (String, bool) {
        match composer_json
            .filter(|_| self.sync_with_composer)
            .and_then(crate::composer::language_level)
        {
            Some(level) => (level, true),
            None => (self.php_version.clone(), false),
        }
    }
}

/// Setting keys (see `app/src-tauri/plugins/php.toml`) that change the language server.
pub const LSP_SETTING_KEYS: &[&str] = &[
    "php.languageLevel",
    "php.includePaths",
    "php.lsp.server",
    "php.lsp.maxFileSize",
    crate::style::BRACES_KEY,
    crate::composer::JSON_KEY,
    crate::composer::SYNC_KEY,
    crate::composer::LIBRARIES_KEY,
];

/// Whether a change of `key` requires the PHP language server to be reconfigured.
pub fn affects_server(key: &str) -> bool {
    LSP_SETTING_KEYS.contains(&key)
}

impl PhpLspSettings {
    /// Build the settings from Forge's settings store (`get` returns the effective value of
    /// a dotted key). Missing or malformed values keep their defaults. The licence key is
    /// not a setting (it lives in the OS keychain) and is passed separately.
    pub fn from_settings(get: impl Fn(&str) -> Option<Value>) -> Self {
        let mut s = Self::default();
        if let Some(level) = get("php.languageLevel").and_then(|v| v.as_str().map(String::from)) {
            if !level.trim().is_empty() {
                s.php_version = level.trim().to_string();
            }
        }
        if let Some(server) = get("php.lsp.server").and_then(|v| serde_json::from_value(v).ok()) {
            s.server = server;
        }
        if let Some(braces) =
            get(crate::style::BRACES_KEY).and_then(|v| serde_json::from_value(v).ok())
        {
            s.braces = braces;
        }
        if let Some(path) =
            get(crate::composer::JSON_KEY).and_then(|v| v.as_str().map(str::trim).map(String::from))
        {
            if !path.is_empty() {
                s.composer_json = path;
            }
        }
        if let Some(sync) = get(crate::composer::SYNC_KEY).and_then(|v| v.as_bool()) {
            s.sync_with_composer = sync;
        }
        if let Some(libraries) = get(crate::composer::LIBRARIES_KEY).and_then(|v| v.as_bool()) {
            s.vendor_libraries = libraries;
        }
        if let Some(kb) = get("php.lsp.maxFileSize").and_then(|v| v.as_u64()) {
            s.max_file_size = kb.saturating_mul(1000);
        }
        if let Some(paths) = get("php.includePaths").and_then(|v| v.as_array().cloned()) {
            s.include_paths = paths
                .iter()
                .filter_map(|p| p.as_str())
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(String::from)
                .collect();
        }
        s
    }
}

/// Everything needed to spawn and initialize a language server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LspServerConfig {
    pub kind: ServerKind,
    pub name: String,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub language_ids: Vec<String>,
    pub file_extensions: Vec<String>,
    pub initialization_options: Value,
    /// Sent with `workspace/didChangeConfiguration` and answered for `workspace/configuration`.
    pub settings: Value,
}

/// Intelephense's built-in stub set (its default `intelephense.stubs`).
pub const DEFAULT_STUBS: &[&str] = &[
    "apache",
    "bcmath",
    "bz2",
    "calendar",
    "com_dotnet",
    "Core",
    "ctype",
    "curl",
    "date",
    "dba",
    "dom",
    "enchant",
    "exif",
    "FFI",
    "fileinfo",
    "filter",
    "fpm",
    "ftp",
    "gd",
    "gettext",
    "gmp",
    "hash",
    "iconv",
    "imap",
    "intl",
    "json",
    "ldap",
    "libxml",
    "mbstring",
    "meta",
    "mysqli",
    "oci8",
    "odbc",
    "openssl",
    "pcntl",
    "pcre",
    "PDO",
    "pdo_ibm",
    "pdo_mysql",
    "pdo_pgsql",
    "pdo_sqlite",
    "pgsql",
    "Phar",
    "posix",
    "pspell",
    "random",
    "readline",
    "Reflection",
    "session",
    "shmop",
    "SimpleXML",
    "snmp",
    "soap",
    "sockets",
    "sodium",
    "SPL",
    "sqlite3",
    "standard",
    "superglobals",
    "sysvmsg",
    "sysvsem",
    "sysvshm",
    "tidy",
    "tokenizer",
    "xml",
    "xmlreader",
    "xmlrpc",
    "xmlwriter",
    "xsl",
    "Zend OPcache",
    "zip",
    "zlib",
];

/// Framework stub sets added when the project uses the framework (detected from
/// composer.json). Names Intelephense does not know are ignored by it.
pub fn framework_stubs(composer_json: Option<&str>) -> Vec<String> {
    let Some(v) = composer_json.and_then(|s| serde_json::from_str::<Value>(s).ok()) else {
        return Vec::new();
    };
    let mut deps: Vec<String> = Vec::new();
    for key in ["require", "require-dev"] {
        if let Some(obj) = v.get(key).and_then(|r| r.as_object()) {
            deps.extend(obj.keys().cloned());
        }
    }
    let has = |prefix: &str| deps.iter().any(|d| d.starts_with(prefix));
    let mut out = Vec::new();
    if has("laravel/") || has("illuminate/") {
        out.push("laravel".to_string());
    }
    if has("symfony/") {
        out.push("symfony".to_string());
    }
    if has("doctrine/") {
        out.push("doctrine".to_string());
    }
    if has("phpunit/") {
        out.push("phpunit".to_string());
    }
    for (pkg, stub) in [
        ("ext-redis", "redis"),
        ("ext-mongodb", "mongodb"),
        ("ext-imagick", "imagick"),
        ("ext-memcached", "memcached"),
        ("ext-swoole", "swoole"),
    ] {
        if has(pkg) {
            out.push(stub.to_string());
        }
    }
    out
}

/// Abstraction over the file system for executable lookup (testable).
pub trait ExecutableProbe {
    fn is_executable(&self, path: &Path) -> bool;
}

pub struct FsProbe;

impl ExecutableProbe for FsProbe {
    fn is_executable(&self, path: &Path) -> bool {
        path.is_file()
    }
}

/// Directories searched for language server executables: `PATH` plus the usual npm global
/// bin directories (servers are commonly installed with `npm i -g`) and composer's global bin.
pub fn search_dirs(path_env: Option<&str>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = path_env
        .map(|p| std::env::split_paths(p).collect())
        .unwrap_or_default();
    if let Ok(prefix) = std::env::var("NPM_CONFIG_PREFIX") {
        dirs.push(PathBuf::from(&prefix).join("bin"));
        dirs.push(PathBuf::from(prefix));
    }
    if cfg!(windows) {
        if let Ok(appdata) = std::env::var("APPDATA") {
            dirs.push(PathBuf::from(appdata).join("npm"));
        }
    }
    if let Some(home) = home {
        for rel in [
            ".npm-global/bin",
            ".local/share/npm/bin",
            ".local/bin",
            ".volta/bin",
            ".config/composer/vendor/bin",
            ".composer/vendor/bin",
            "AppData/Roaming/npm",
            "AppData/Roaming/Composer/vendor/bin",
        ] {
            dirs.push(home.join(rel));
        }
    }
    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.dedup();
    dirs
}

/// Find `name` (plus `.cmd`/`.exe`/`.bat` variants on Windows) in `dirs`.
pub fn find_executable(
    name: &str,
    dirs: &[PathBuf],
    probe: &dyn ExecutableProbe,
) -> Option<PathBuf> {
    let variants: Vec<String> = if cfg!(windows) {
        vec![
            format!("{name}.cmd"),
            format!("{name}.exe"),
            format!("{name}.bat"),
            name.to_string(),
        ]
    } else {
        vec![name.to_string()]
    };
    dirs.iter()
        .flat_map(|d| variants.iter().map(move |v| d.join(v)))
        .find(|p| probe.is_executable(p))
}

/// Paths of installed servers, for the settings page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedServers {
    pub intelephense: Option<PathBuf>,
    pub phpactor: Option<PathBuf>,
}

pub fn detect(
    settings: &PhpLspSettings,
    dirs: &[PathBuf],
    probe: &dyn ExecutableProbe,
) -> DetectedServers {
    let pick = |explicit: &Option<PathBuf>, name: &str| {
        explicit
            .clone()
            .filter(|p| probe.is_executable(p))
            .or_else(|| find_executable(name, dirs, probe))
    };
    DetectedServers {
        intelephense: pick(&settings.intelephense_path, "intelephense"),
        phpactor: pick(&settings.phpactor_path, "phpactor"),
    }
}

pub fn default_storage_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("forge")
        .join("intelephense")
}

pub fn intelephense_config(
    command: PathBuf,
    settings: &PhpLspSettings,
    composer_json: Option<&str>,
) -> LspServerConfig {
    let mut stubs: Vec<String> = DEFAULT_STUBS.iter().map(|s| s.to_string()).collect();
    for s in framework_stubs(composer_json)
        .into_iter()
        .chain(settings.extra_stubs.iter().cloned())
    {
        if !stubs.contains(&s) {
            stubs.push(s);
        }
    }
    let storage = settings
        .storage_path
        .clone()
        .unwrap_or_else(default_storage_path);
    let mut init = json!({
        "storagePath": storage.to_string_lossy(),
        "clearCache": false,
    });
    if let Some(key) = settings
        .licence_key
        .as_ref()
        .filter(|k| !k.trim().is_empty())
    {
        init["licenceKey"] = json!(key.trim());
    }
    let mut intelephense = json!({
        "environment": {
            "phpVersion": format!("{}.0", settings.php_version.trim()),
            "includePaths": settings.include_paths,
        },
        "files": {
            "maxSize": settings.max_file_size,
            "associations": ["*.php", "*.phtml"],
            "exclude": [
                "**/.git/**", "**/.svn/**", "**/.hg/**", "**/CVS/**", "**/.DS_Store/**",
                "**/node_modules/**", "**/bower_components/**", "**/vendor/**/{Tests,tests}/**",
                "**/.history/**", "**/vendor/**/vendor/**", "**/.forge/**"
            ],
        },
        "stubs": stubs,
        "completion": { "insertUseDeclaration": true, "fullyQualifyGlobalConstantsAndFunctions": false },
        "format": { "enable": true, "braces": settings.braces.intelephense() },
        "telemetry": { "enabled": false },
    });
    init["globalStoragePath"] = json!(storage.to_string_lossy());
    // "Add packages as libraries" off: Intelephense ignores vendor/ like Forge's index.
    if !settings.vendor_libraries {
        if let Some(exclude) = intelephense["files"]["exclude"].as_array_mut() {
            exclude.push(json!("**/vendor/**"));
        }
    }
    // A managed install: `node <dir>/node_modules/intelephense/lib/intelephense.js --stdio`.
    let node = settings
        .intelephense_node
        .clone()
        .filter(|_| settings.intelephense_path.as_ref() == Some(&command));
    let (command, args) = match node {
        Some(node) => (node, vec![command.display().to_string(), "--stdio".into()]),
        None => (command, vec!["--stdio".into()]),
    };
    LspServerConfig {
        kind: ServerKind::Intelephense,
        name: "Intelephense".into(),
        command,
        args,
        language_ids: vec!["php".into()],
        file_extensions: vec!["php".into(), "phtml".into()],
        initialization_options: init,
        settings: json!({ "intelephense": intelephense }),
    }
}

pub fn phpactor_config(command: PathBuf, settings: &PhpLspSettings) -> LspServerConfig {
    LspServerConfig {
        kind: ServerKind::Phpactor,
        name: "phpactor".into(),
        command,
        args: vec!["language-server".into()],
        language_ids: vec!["php".into()],
        file_extensions: vec!["php".into(), "phtml".into()],
        initialization_options: json!({
            "php.version": settings.php_version,
            "language_server_phpstan.enabled": false,
            "language_server_psalm.enabled": false,
            // Inlay hints, filtered by Forge's Inlay Hints settings.
            "language_server_worse_reflection.inlay_hints.enable": true,
            "language_server_worse_reflection.inlay_hints.params": true,
            "language_server_worse_reflection.inlay_hints.types": true,
            "indexer.exclude_patterns": ["/vendor/**/Tests/**/*", "/vendor/**/tests/**/*", "/node_modules/**/*"],
        }),
        settings: json!({}),
    }
}

/// Choose and configure the server according to `settings`. `None` when disabled or nothing
/// suitable is installed.
pub fn server_config(
    settings: &PhpLspSettings,
    detected: &DetectedServers,
    composer_json: Option<&str>,
) -> Option<LspServerConfig> {
    let intelephense = || {
        detected
            .intelephense
            .clone()
            .map(|p| intelephense_config(p, settings, composer_json))
    };
    let phpactor = || {
        detected
            .phpactor
            .clone()
            .map(|p| phpactor_config(p, settings))
    };
    match settings.server {
        ServerChoice::None => None,
        ServerChoice::Intelephense => intelephense(),
        ServerChoice::Phpactor => phpactor(),
        ServerChoice::Auto => intelephense().or_else(phpactor),
    }
}

/// Convenience: detect on this machine and build the config for a workspace root.
pub fn resolve_for_workspace(settings: &PhpLspSettings, root: &Path) -> Option<LspServerConfig> {
    let path_env = std::env::var("PATH").ok();
    let dirs = search_dirs(path_env.as_deref(), dirs::home_dir().as_deref());
    let detected = detect(settings, &dirs, &FsProbe);
    let composer = std::fs::read_to_string(root.join(&settings.composer_json)).ok();
    let mut settings = settings.clone();
    settings.php_version = settings.effective_level(composer.as_deref()).0;
    server_config(&settings, &detected, composer.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    struct FakeProbe(HashSet<PathBuf>);

    impl ExecutableProbe for FakeProbe {
        fn is_executable(&self, path: &Path) -> bool {
            self.0.contains(path)
        }
    }

    fn exe(name: &str) -> String {
        if cfg!(windows) {
            format!("{name}.cmd")
        } else {
            name.to_string()
        }
    }

    #[test]
    fn finds_servers_in_path_and_npm_dirs() {
        let home = PathBuf::from("/home/u");
        let dirs = search_dirs(Some("/usr/bin"), Some(&home));
        assert!(dirs.contains(&PathBuf::from("/usr/bin")));
        assert!(dirs.contains(&home.join(".npm-global/bin")));
        let probe = FakeProbe(
            [
                home.join(".npm-global/bin").join(exe("intelephense")),
                PathBuf::from("/usr/bin").join(exe("phpactor")),
            ]
            .into_iter()
            .collect(),
        );
        let d = detect(&PhpLspSettings::default(), &dirs, &probe);
        assert_eq!(
            d.intelephense,
            Some(home.join(".npm-global/bin").join(exe("intelephense")))
        );
        assert_eq!(
            d.phpactor,
            Some(PathBuf::from("/usr/bin").join(exe("phpactor")))
        );
    }

    #[test]
    fn explicit_path_wins() {
        let custom = PathBuf::from("/opt/x/intelephense");
        let probe = FakeProbe([custom.clone()].into_iter().collect());
        let settings = PhpLspSettings {
            intelephense_path: Some(custom.clone()),
            ..Default::default()
        };
        assert_eq!(detect(&settings, &[], &probe).intelephense, Some(custom));
    }

    #[test]
    fn managed_intelephense_runs_with_forges_node() {
        let script =
            PathBuf::from("/data/forge/langservers/node_modules/intelephense/lib/intelephense.js");
        let node = PathBuf::from("/data/forge/runtimes/node-v22.11.0/bin/node");
        let settings = PhpLspSettings {
            intelephense_path: Some(script.clone()),
            intelephense_node: Some(node.clone()),
            ..Default::default()
        };
        let cfg = intelephense_config(script.clone(), &settings, None);
        assert_eq!(cfg.command, node);
        assert_eq!(
            cfg.args,
            vec![script.display().to_string(), "--stdio".into()]
        );
        // An executable found elsewhere starts on its own.
        let other = PathBuf::from("/usr/bin/intelephense");
        let cfg = intelephense_config(other.clone(), &settings, None);
        assert_eq!(
            (cfg.command, cfg.args),
            (other, vec!["--stdio".to_string()])
        );
    }

    #[test]
    fn intelephense_config_with_licence_and_framework_stubs() {
        let settings = PhpLspSettings {
            licence_key: Some(" ABC123 ".into()),
            php_version: "8.2".into(),
            storage_path: Some(PathBuf::from("/tmp/ip")),
            include_paths: vec!["/usr/share/php".into()],
            extra_stubs: vec!["redis".into()],
            ..Default::default()
        };
        let composer = r#"{"require": {"laravel/framework": "^12", "doctrine/dbal": "^4"}, "require-dev": {"phpunit/phpunit": "^11"}}"#;
        let detected = DetectedServers {
            intelephense: Some(PathBuf::from("/bin/intelephense")),
            phpactor: Some(PathBuf::from("/bin/phpactor")),
        };
        let cfg = server_config(&settings, &detected, Some(composer)).unwrap();
        assert_eq!(cfg.kind, ServerKind::Intelephense);
        assert_eq!(cfg.args, vec!["--stdio"]);
        assert_eq!(cfg.initialization_options["licenceKey"], "ABC123");
        assert_eq!(cfg.initialization_options["storagePath"], "/tmp/ip");
        let ip = &cfg.settings["intelephense"];
        assert_eq!(ip["environment"]["phpVersion"], "8.2.0");
        assert_eq!(ip["environment"]["includePaths"][0], "/usr/share/php");
        assert_eq!(ip["files"]["maxSize"], 5_000_000);
        let stubs: Vec<&str> = ip["stubs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect();
        for s in ["standard", "PDO", "laravel", "doctrine", "phpunit", "redis"] {
            assert!(stubs.contains(&s), "missing {s}");
        }
        assert!(!stubs.contains(&"symfony"));
        assert_eq!(ip["format"]["braces"], "psr12");
        assert!(!ip["files"]["exclude"]
            .to_string()
            .contains("\"**/vendor/**\""));
        // "Add packages as libraries" off and K&R braces.
        let no_vendor = PhpLspSettings {
            vendor_libraries: false,
            braces: crate::style::Braces::KAndR,
            ..settings.clone()
        };
        let ip = &server_config(&no_vendor, &detected, None).unwrap().settings["intelephense"];
        assert!(ip["files"]["exclude"]
            .to_string()
            .contains("\"**/vendor/**\""));
        assert_eq!(ip["format"]["braces"], "k&r");
        // The language level follows composer.json unless synchronization is off.
        let json = r#"{"require": {"php": "^8.1"}}"#;
        assert_eq!(
            settings.effective_level(Some(json)),
            ("8.1".to_string(), true)
        );
        assert_eq!(settings.effective_level(None), ("8.2".to_string(), false));
        let manual = PhpLspSettings {
            sync_with_composer: false,
            ..settings.clone()
        };
        assert_eq!(manual.effective_level(Some(json)).0, "8.2");
    }

    #[test]
    fn no_licence_key_means_no_licence_option() {
        let cfg = intelephense_config(PathBuf::from("i"), &PhpLspSettings::default(), None);
        assert!(cfg.initialization_options.get("licenceKey").is_none());
    }

    #[test]
    fn falls_back_to_phpactor_and_respects_choice() {
        let only_phpactor = DetectedServers {
            intelephense: None,
            phpactor: Some(PathBuf::from("/bin/phpactor")),
        };
        let cfg = server_config(&PhpLspSettings::default(), &only_phpactor, None).unwrap();
        assert_eq!(cfg.kind, ServerKind::Phpactor);
        assert_eq!(cfg.args, vec!["language-server"]);

        let none = PhpLspSettings {
            server: ServerChoice::None,
            ..Default::default()
        };
        assert!(server_config(&none, &only_phpactor, None).is_none());
        let want_ip = PhpLspSettings {
            server: ServerChoice::Intelephense,
            ..Default::default()
        };
        assert!(server_config(&want_ip, &only_phpactor, None).is_none());
    }

    #[test]
    fn reads_forge_settings() {
        let values = json!({
            "php.languageLevel": "8.1",
            "php.lsp.server": "phpactor",
            "php.lsp.maxFileSize": 2000,
            "php.includePaths": ["/usr/share/php", " "],
        });
        let s = PhpLspSettings::from_settings(|k| values.get(k).cloned());
        assert_eq!(s.php_version, "8.1");
        assert_eq!(s.server, ServerChoice::Phpactor);
        assert_eq!(s.max_file_size, 2_000_000);
        assert_eq!(s.include_paths, vec!["/usr/share/php"]);
        assert!(s.licence_key.is_none());

        let defaults = PhpLspSettings::from_settings(|_| None);
        assert_eq!(defaults.server, ServerChoice::Auto);
        assert_eq!(defaults.php_version, "8.3");
        let bad = PhpLspSettings::from_settings(|_| Some(json!(true)));
        assert_eq!(bad.server, ServerChoice::Auto);
        assert!(affects_server("php.lsp.server"));
        assert!(!affects_server("php.codeStyle.tabSize"));
    }

    #[test]
    fn framework_detection() {
        assert_eq!(
            framework_stubs(Some(
                r#"{"require": {"symfony/http-kernel": "*", "ext-redis": "*"}}"#
            )),
            vec!["symfony", "redis"]
        );
        assert!(framework_stubs(Some("not json")).is_empty());
        assert!(framework_stubs(None).is_empty());
    }
}
