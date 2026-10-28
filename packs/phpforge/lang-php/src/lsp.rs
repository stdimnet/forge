//! How to launch PHP's language server: php-lsp, built into Forge and its only PHP language
//! server. Independent of the generic LSP client; the app turns [`LspServerConfig`] into a
//! running server.
//!
//! php-lsp (<https://github.com/stdimnet/php-lsp>, MIT OR Apache-2.0) is linked into the
//! Forge binary and the remote agent (`packs/phpforge/builtin-lsp`), which run it in a hidden
//! `--php-lsp --stdio` mode. They announce it with [`register_builtin_server`]; binaries that
//! do not (tests, tools) simply have no built-in server.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// php-lsp as built into the running binary: how to start it (the binary itself in its
/// hidden mode) and its version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltinServer {
    pub command: PathBuf,
    pub args: Vec<String>,
    /// php-lsp's version, e.g. `0.1.0 (6e00c57)`.
    pub version: String,
}

static BUILTIN: OnceLock<BuiltinServer> = OnceLock::new();

/// Announce the built-in php-lsp of this binary (called once at startup by the Forge app
/// and the remote agent). Later calls are ignored.
pub fn register_builtin_server(server: BuiltinServer) {
    let _ = BUILTIN.set(server);
}

/// The built-in php-lsp of this binary, if it has one.
pub fn builtin_server() -> Option<&'static BuiltinServer> {
    BUILTIN.get()
}

/// User settings relevant to the PHP language server (Settings > Languages & Frameworks > PHP).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PhpLspSettings {
    /// PHP language level, e.g. `8.3`.
    pub php_version: String,
    /// `php_version` is the CLI interpreter's (`php.languageLevel` = `interpreter`).
    pub level_from_interpreter: bool,
    /// `intelephense.files.maxSize` in bytes (php-lsp reads Intelephense's settings).
    pub max_file_size: u64,
    /// Extra stub sets on top of the defaults.
    pub extra_stubs: Vec<String>,
    /// Additional include paths (`intelephense.environment.includePaths`).
    pub include_paths: Vec<String>,
    /// Where php-lsp stores its cache; defaults to `<cache dir>/forge/php-lsp`.
    pub storage_path: Option<PathBuf>,
    /// Brace placement of Reformat Code (`php.codeStyle.wrapping.*Braces`).
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
            php_version: "8.3".into(),
            level_from_interpreter: false,
            max_file_size: 5_000_000,
            extra_stubs: Vec::new(),
            include_paths: Vec::new(),
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
    "php.lsp.maxFileSize",
    crate::style::BRACES_KEY,
    crate::style::CLASS_BRACES_KEY,
    crate::style::FUNCTION_BRACES_KEY,
    crate::style::OTHER_BRACES_KEY,
    crate::composer::JSON_KEY,
    crate::composer::SYNC_KEY,
    crate::composer::LIBRARIES_KEY,
    // The language level can follow the CLI interpreter's version.
    phpforge_interpreters::model::SELECTED_KEY,
    phpforge_interpreters::model::GLOBAL_KEY,
    phpforge_interpreters::model::PROJECT_KEY,
];

/// `php.languageLevel` value: the level follows the CLI interpreter's version.
pub const LEVEL_FROM_INTERPRETER: &str = "interpreter";

/// Whether a change of `key` requires the PHP language server to be reconfigured.
pub fn affects_server(key: &str) -> bool {
    LSP_SETTING_KEYS.contains(&key)
}

impl PhpLspSettings {
    /// Build the settings from Forge's settings store (`get` returns the effective value of
    /// a dotted key). Missing or malformed values keep their defaults; keys of removed
    /// options (`php.lsp.server`, `php.intelephense.*`) are not read.
    pub fn from_settings(get: impl Fn(&str) -> Option<Value>) -> Self {
        let mut s = Self::default();
        if let Some(level) = get("php.languageLevel").and_then(|v| v.as_str().map(String::from)) {
            if level.trim() == LEVEL_FROM_INTERPRETER {
                use phpforge_interpreters::model::{GLOBAL_KEY, PROJECT_KEY, SELECTED_KEY};
                let set = phpforge_interpreters::InterpreterSet::from_settings(
                    get(GLOBAL_KEY).as_ref(),
                    get(PROJECT_KEY).as_ref(),
                );
                let selected = get(SELECTED_KEY)
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default();
                if let Some(level) = set
                    .resolve(&selected)
                    .version()
                    .and_then(crate::composer::level_for_version)
                {
                    s.php_version = level;
                    s.level_from_interpreter = true;
                }
            } else if !level.trim().is_empty() {
                s.php_version = level.trim().to_string();
            }
        }
        s.braces = crate::style::Braces::from_settings(&get);
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
    pub name: String,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub language_ids: Vec<String>,
    pub file_extensions: Vec<String>,
    pub initialization_options: Value,
    /// Sent with `workspace/didChangeConfiguration` and answered for `workspace/configuration`.
    pub settings: Value,
}

/// The default stub set: Intelephense's default `intelephense.stubs`, which php-lsp reads.
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
/// composer.json). Names php-lsp does not know are ignored by it.
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

/// The PHP language server of the machine that starts the servers, for the settings page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedServers {
    /// php-lsp built into the binary that starts the servers (Forge or the remote agent).
    #[serde(default)]
    pub php_lsp: Option<BuiltinServer>,
}

pub fn detect() -> DetectedServers {
    DetectedServers {
        php_lsp: builtin_server().cloned(),
    }
}

/// Where php-lsp keeps its index cache and unpacked stubs: `<cache dir>/forge/php-lsp`
/// (never inside the project; php-lsp names each project's cache after its folders).
pub fn default_php_lsp_storage() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("forge")
        .join("php-lsp")
}

/// The `intelephense` settings section (php-lsp reads Intelephense's settings): language
/// level, include paths, files, stubs, completion and formatting.
fn intelephense_section(settings: &PhpLspSettings, composer_json: Option<&str>) -> Value {
    let mut stubs: Vec<String> = DEFAULT_STUBS.iter().map(|s| s.to_string()).collect();
    for s in framework_stubs(composer_json)
        .into_iter()
        .chain(settings.extra_stubs.iter().cloned())
    {
        if !stubs.contains(&s) {
            stubs.push(s);
        }
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
    // "Add packages as libraries" off: the server ignores vendor/ like Forge's index.
    if !settings.vendor_libraries {
        if let Some(exclude) = intelephense["files"]["exclude"].as_array_mut() {
            exclude.push(json!("**/vendor/**"));
        }
    }
    intelephense
}

/// php-lsp, built into Forge: started as `<binary> --php-lsp --stdio`. It reads the same
/// `intelephense` settings; its own `php-lsp` section turns off what Forge does itself:
/// PHPStan, Psalm and PHP_CodeSniffer run through Forge's Quality Tools (no duplicate
/// problems), and formatting stays with php-lsp's built-in formatter (Forge runs
/// php-cs-fixer / phpcbf / Pint through Reformat with and Actions on Save).
pub fn php_lsp_config(
    server: &BuiltinServer,
    settings: &PhpLspSettings,
    composer_json: Option<&str>,
) -> LspServerConfig {
    let storage = settings
        .storage_path
        .clone()
        .unwrap_or_else(default_php_lsp_storage);
    let off = json!({ "enable": false });
    LspServerConfig {
        name: "php-lsp".into(),
        command: server.command.clone(),
        args: server.args.clone(),
        language_ids: vec!["php".into()],
        file_extensions: vec!["php".into(), "phtml".into()],
        initialization_options: json!({
            "storagePath": storage.to_string_lossy(),
            "clearCache": false,
        }),
        settings: json!({
            "intelephense": intelephense_section(settings, composer_json),
            "php-lsp": {
                "tools": { "phpstan": off, "psalm": off, "phpcs": off },
                "format": { "provider": "builtin" },
                "index": { "cache": true },
                "inlayHint": { "parameterNames": true, "parameterTypes": true, "returnTypes": true },
            },
        }),
    }
}

/// Configure php-lsp. `None` when the binary has no built-in server.
pub fn server_config(
    settings: &PhpLspSettings,
    detected: &DetectedServers,
    composer_json: Option<&str>,
) -> Option<LspServerConfig> {
    detected
        .php_lsp
        .as_ref()
        .map(|s| php_lsp_config(s, settings, composer_json))
}

/// Convenience: the config of this binary's php-lsp for a workspace root.
pub fn resolve_for_workspace(settings: &PhpLspSettings, root: &Path) -> Option<LspServerConfig> {
    let detected = detect();
    let composer = std::fs::read_to_string(root.join(&settings.composer_json)).ok();
    let mut settings = settings.clone();
    settings.php_version = settings.effective_level(composer.as_deref()).0;
    server_config(&settings, &detected, composer.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_forge_settings() {
        let values = json!({
            "php.languageLevel": "8.1",
            "php.lsp.maxFileSize": 2000,
            "php.includePaths": ["/usr/share/php", " "],
        });
        let s = PhpLspSettings::from_settings(|k| values.get(k).cloned());
        assert_eq!(s.php_version, "8.1");
        assert_eq!(s.max_file_size, 2_000_000);
        assert_eq!(s.include_paths, vec!["/usr/share/php"]);

        let defaults = PhpLspSettings::from_settings(|_| None);
        assert_eq!(defaults, PhpLspSettings::default());
        assert_eq!(defaults.php_version, "8.3");
        let bad = PhpLspSettings::from_settings(|_| Some(json!(true)));
        assert_eq!(bad.php_version, "8.3");
        assert!(!affects_server("php.codeStyle.tabSize"));
    }

    #[test]
    fn removed_server_settings_are_ignored() {
        // Settings files of earlier versions may still choose Intelephense or phpactor, or
        // hold Intelephense's options: php-lsp starts all the same.
        for server in [
            "intelephense",
            "phpactor",
            "auto",
            "none",
            "php-lsp",
            "bogus",
        ] {
            let values = json!({
                "php.lsp.server": server,
                "php.intelephense.autoInstall": "always",
                "php.intelephense.licenceAccepted": true,
                "php.intelephense.licenceKey": "SECRET",
            });
            let s = PhpLspSettings::from_settings(|k| values.get(k).cloned());
            assert_eq!(s, PhpLspSettings::default(), "{server}");
            let detected = DetectedServers {
                php_lsp: Some(builtin()),
            };
            let cfg = server_config(&s, &detected, None).unwrap();
            assert_eq!(cfg.name, "php-lsp");
            assert!(cfg.initialization_options.get("licenceKey").is_none());
        }
        assert!(!affects_server("php.lsp.server"));
        // A peer of an earlier version still reports Intelephense / phpactor paths.
        let old: DetectedServers = serde_json::from_value(
            json!({ "phpLsp": null, "intelephense": "/bin/intelephense", "phpactor": null }),
        )
        .unwrap();
        assert!(old.php_lsp.is_none());
        let old: LspServerConfig = serde_json::from_value(json!({
            "kind": "intelephense", "name": "Intelephense", "command": "intelephense",
            "args": ["--stdio"], "languageIds": ["php"], "fileExtensions": ["php"],
            "initializationOptions": {}, "settings": {},
        }))
        .unwrap();
        assert_eq!(old.name, "Intelephense");
    }

    #[test]
    fn the_language_level_can_follow_the_interpreter() {
        let values = json!({
            "php.languageLevel": "interpreter",
            "php.interpreter": "Docker 8.2",
            "php.projectInterpreters": [{ "name": "Docker 8.2", "kind": "docker", "image": "php:8.2-cli", "version": "8.2.27" }],
        });
        let s = PhpLspSettings::from_settings(|k| values.get(k).cloned());
        assert_eq!(s.php_version, "8.2");
        assert!(s.level_from_interpreter);
        // An interpreter whose version was never detected keeps the default level.
        let unknown = json!({ "php.languageLevel": "interpreter", "php.interpreter": "php" });
        let s = PhpLspSettings::from_settings(|k| unknown.get(k).cloned());
        assert_eq!(s.php_version, "8.3");
        assert!(!s.level_from_interpreter);
        assert!(affects_server("php.interpreter"));
    }

    fn builtin() -> BuiltinServer {
        BuiltinServer {
            command: PathBuf::from("/opt/forge/forge"),
            args: vec!["--php-lsp".into(), "--stdio".into()],
            version: "0.1.0 (6e00c57)".into(),
        }
    }

    #[test]
    fn php_lsp_is_the_server() {
        let detected = DetectedServers {
            php_lsp: Some(builtin()),
        };
        let cfg = server_config(&PhpLspSettings::default(), &detected, None).unwrap();
        assert_eq!(cfg.command, PathBuf::from("/opt/forge/forge"));
        assert_eq!(cfg.args, vec!["--php-lsp", "--stdio"]);
        assert_eq!(
            serde_json::to_value(&detected).unwrap()["phpLsp"]["version"],
            "0.1.0 (6e00c57)"
        );
        // A binary without the built-in server (tests, tools) starts none.
        let none = DetectedServers::default();
        assert!(server_config(&PhpLspSettings::default(), &none, None).is_none());
    }

    #[test]
    fn php_lsp_config_maps_forges_settings() {
        let settings = PhpLspSettings {
            php_version: "8.2".into(),
            storage_path: Some(PathBuf::from("/cache/php-lsp")),
            include_paths: vec!["/usr/share/php".into()],
            max_file_size: 2_000_000,
            vendor_libraries: false,
            braces: crate::style::Braces::KAndR,
            ..Default::default()
        };
        let composer = r#"{"require": {"laravel/framework": "^12"}}"#;
        let cfg = php_lsp_config(&builtin(), &settings, Some(composer));
        assert_eq!(cfg.name, "php-lsp");
        assert_eq!(cfg.initialization_options["storagePath"], "/cache/php-lsp");
        assert!(cfg.initialization_options.get("licenceKey").is_none());
        let ip = &cfg.settings["intelephense"];
        assert_eq!(ip["environment"]["phpVersion"], "8.2.0");
        assert_eq!(ip["environment"]["includePaths"][0], "/usr/share/php");
        assert_eq!(ip["files"]["maxSize"], 2_000_000);
        assert!(ip["files"]["exclude"]
            .to_string()
            .contains("\"**/vendor/**\""));
        assert_eq!(ip["format"]["braces"], "k&r");
        let stubs = ip["stubs"].as_array().unwrap();
        for s in ["standard", "PDO", "laravel"] {
            assert!(stubs.contains(&json!(s)), "missing {s}");
        }
        assert!(!stubs.contains(&json!("symfony")));
        // Forge runs PHPStan, Psalm and PHP_CodeSniffer itself.
        let own = &cfg.settings["php-lsp"];
        for tool in ["phpstan", "psalm", "phpcs"] {
            assert_eq!(own["tools"][tool]["enable"], false, "{tool}");
        }
        assert_eq!(own["format"]["provider"], "builtin");
        // The cache lives in Forge's cache folder by default, never in the project.
        let default = php_lsp_config(&builtin(), &PhpLspSettings::default(), None);
        let storage = default.initialization_options["storagePath"]
            .as_str()
            .unwrap();
        assert!(storage.ends_with("php-lsp"), "{storage}");
    }

    #[test]
    fn language_level_follows_composer() {
        let settings = PhpLspSettings {
            php_version: "8.2".into(),
            ..Default::default()
        };
        let json = r#"{"require": {"php": "^8.1"}}"#;
        assert_eq!(
            settings.effective_level(Some(json)),
            ("8.1".to_string(), true)
        );
        assert_eq!(settings.effective_level(None), ("8.2".to_string(), false));
        let manual = PhpLspSettings {
            sync_with_composer: false,
            ..settings
        };
        assert_eq!(manual.effective_level(Some(json)).0, "8.2");
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
