//! Settings | PHP | Test Frameworks: a configuration per framework and CLI interpreter, like
//! PHPStorm's. Each says where the framework comes from ("Use Composer autoloader" with the
//! path to `vendor/autoload.php`, or the path to the executable / phar) and its default
//! configuration and bootstrap files. Frameworks the project uses but that are not
//! configured are detected from composer.json, `vendor/bin` and their configuration files.
//!
//! Stored in the project setting [`SETTINGS_KEY`] as a list of string tables:
//! `{ framework, interpreter, mode, path, configFile, bootstrapFile }`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The project setting with the configured frameworks.
pub const SETTINGS_KEY: &str = "php.testFrameworks";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Framework {
    Phpunit,
    Pest,
    Codeception,
    Behat,
    Phpspec,
}

impl Framework {
    pub const ALL: [Framework; 5] = [
        Framework::Phpunit,
        Framework::Pest,
        Framework::Codeception,
        Framework::Behat,
        Framework::Phpspec,
    ];

    /// The id used in settings and as the run configuration type.
    pub fn id(self) -> &'static str {
        match self {
            Framework::Phpunit => "phpunit",
            Framework::Pest => "pest",
            Framework::Codeception => "codeception",
            Framework::Behat => "behat",
            Framework::Phpspec => "phpspec",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Framework::Phpunit => "PHPUnit",
            Framework::Pest => "Pest",
            Framework::Codeception => "Codeception",
            Framework::Behat => "Behat",
            Framework::Phpspec => "PHPSpec",
        }
    }

    /// The Composer package that brings it.
    pub fn package(self) -> &'static str {
        match self {
            Framework::Phpunit => "phpunit/phpunit",
            Framework::Pest => "pestphp/pest",
            Framework::Codeception => "codeception/codeception",
            Framework::Behat => "behat/behat",
            Framework::Phpspec => "phpspec/phpspec",
        }
    }

    /// The script in `vendor/bin`.
    pub fn bin(self) -> &'static str {
        match self {
            Framework::Phpunit => "phpunit",
            Framework::Pest => "pest",
            Framework::Codeception => "codecept",
            Framework::Behat => "behat",
            Framework::Phpspec => "phpspec",
        }
    }

    /// Configuration files the framework reads from the project root, preferred first.
    pub fn config_files(self) -> &'static [&'static str] {
        match self {
            Framework::Phpunit | Framework::Pest => {
                &["phpunit.xml", "phpunit.xml.dist", "phpunit.dist.xml"]
            }
            Framework::Codeception => &["codeception.yml", "codeception.dist.yml"],
            Framework::Behat => &[
                "behat.yml",
                "behat.yaml",
                "behat.yml.dist",
                "behat.dist.yml",
            ],
            Framework::Phpspec => &["phpspec.yml", ".phpspec.yml", "phpspec.yml.dist"],
        }
    }
}

/// Where the framework's code comes from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// "Use Composer autoloader": `path` is `vendor/autoload.php`, the executable is the
    /// framework's script in `vendor/bin`.
    #[default]
    Composer,
    /// "Path to executable / phar": `path` is the script or phar.
    Path,
}

/// One entry of Settings | PHP | Test Frameworks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameworkConfig {
    pub framework: Framework,
    /// CLI interpreter name; empty is "Local" (the project's default interpreter).
    #[serde(default)]
    pub interpreter: String,
    #[serde(default)]
    pub mode: Mode,
    /// `vendor/autoload.php` (Composer) or the executable (Path), project-relative or
    /// absolute.
    #[serde(default)]
    pub path: String,
    /// Default configuration file (`phpunit.xml`, `codeception.yml`, `behat.yml`, …).
    #[serde(default)]
    pub config_file: String,
    /// Default bootstrap file (PHPUnit, PHPSpec).
    #[serde(default)]
    pub bootstrap_file: String,
    /// Found in the project, not configured.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub detected: bool,
}

impl FrameworkConfig {
    pub fn new(framework: Framework) -> Self {
        Self {
            framework,
            interpreter: String::new(),
            mode: Mode::Composer,
            path: "vendor/autoload.php".into(),
            config_file: String::new(),
            bootstrap_file: String::new(),
            detected: false,
        }
    }

    /// The script that runs the tests: `vendor/bin/pest` next to the autoloader, or the
    /// configured executable.
    pub fn executable(&self) -> String {
        let path = self.path.trim();
        match self.mode {
            Mode::Path if !path.is_empty() => path.to_string(),
            _ => {
                let vendor = match path.rsplit_once('/') {
                    Some((dir, _)) if !path.is_empty() => dir.to_string(),
                    _ => "vendor".to_string(),
                };
                format!("{vendor}/bin/{}", self.framework.bin())
            }
        }
    }

    fn from_object(o: &Map<String, Value>) -> Option<Self> {
        let s = |k: &str| {
            o.get(k)
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string()
        };
        let framework = Framework::from_id(&s("framework"))?;
        let mut c = FrameworkConfig::new(framework);
        c.interpreter = s("interpreter");
        c.mode = if s("mode") == "path" {
            Mode::Path
        } else {
            Mode::Composer
        };
        let path = s("path");
        if !path.is_empty() || c.mode == Mode::Path {
            c.path = path;
        }
        c.config_file = s("configFile");
        c.bootstrap_file = s("bootstrapFile");
        Some(c)
    }

    /// The settings table of this entry.
    pub fn to_object(&self) -> Value {
        let mut o = Map::new();
        o.insert("framework".into(), self.framework.id().into());
        let mode = match self.mode {
            Mode::Composer => "composer",
            Mode::Path => "path",
        };
        o.insert("mode".into(), mode.into());
        o.insert("path".into(), self.path.clone().into());
        for (k, v) in [
            ("interpreter", &self.interpreter),
            ("configFile", &self.config_file),
            ("bootstrapFile", &self.bootstrap_file),
        ] {
            if !v.is_empty() {
                o.insert(k.into(), v.clone().into());
            }
        }
        Value::Object(o)
    }
}

/// The configured entries of the setting (unknown frameworks and duplicates dropped).
pub fn from_setting(value: Option<&Value>) -> Vec<FrameworkConfig> {
    let mut out: Vec<FrameworkConfig> = Vec::new();
    for entry in value.and_then(Value::as_array).into_iter().flatten() {
        if let Some(c) = entry.as_object().and_then(FrameworkConfig::from_object) {
            if !out
                .iter()
                .any(|o| o.framework == c.framework && o.interpreter == c.interpreter)
            {
                out.push(c);
            }
        }
    }
    out
}

fn requires(composer: &Value, package: &str) -> bool {
    ["require", "require-dev"]
        .iter()
        .any(|k| composer.get(k).and_then(|r| r.get(package)).is_some())
}

/// Files [`detect`] looks at, relative to the project root.
pub fn detection_files() -> Vec<String> {
    let mut out = vec!["vendor/autoload.php".to_string()];
    for f in Framework::ALL {
        out.push(format!("vendor/bin/{}", f.bin()));
        out.extend(f.config_files().iter().map(|c| c.to_string()));
    }
    out.push("tests/Pest.php".into());
    out.push("features".into());
    out.push("spec".into());
    out
}

/// The frameworks a project uses: from the packages of composer.json, the scripts in
/// `vendor/bin` and the frameworks' configuration files. `exists` answers for
/// project-relative paths.
pub fn detect(exists: &dyn Fn(&str) -> bool, composer_json: Option<&str>) -> Vec<FrameworkConfig> {
    let composer: Value = composer_json
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or(Value::Null);
    let mut out = Vec::new();
    for f in Framework::ALL {
        let config = f.config_files().iter().find(|c| exists(c));
        let used = requires(&composer, f.package())
            || exists(&format!("vendor/bin/{}", f.bin()))
            || match f {
                Framework::Pest => exists("tests/Pest.php"),
                // phpunit.xml also configures Pest; it says nothing about PHPUnit alone.
                Framework::Phpunit => config.is_some() && !requires(&composer, "pestphp/pest"),
                _ => config.is_some(),
            };
        if !used {
            continue;
        }
        let mut c = FrameworkConfig::new(f);
        if let Some(cfg) = config {
            c.config_file = cfg.to_string();
        }
        c.detected = true;
        out.push(c);
    }
    out
}

/// Configured entries plus the detected frameworks that have none.
pub fn effective(
    configured: Vec<FrameworkConfig>,
    detected: Vec<FrameworkConfig>,
) -> Vec<FrameworkConfig> {
    let mut out = configured;
    for d in detected {
        if !out.iter().any(|c| c.framework == d.framework) {
            out.push(d);
        }
    }
    out
}

/// The entry a run uses: the one for its interpreter, else the "Local" one, else any.
pub fn resolve<'a>(
    list: &'a [FrameworkConfig],
    framework: Framework,
    interpreter: &str,
) -> Option<&'a FrameworkConfig> {
    let of = || list.iter().filter(move |c| c.framework == framework);
    of().find(|c| !interpreter.is_empty() && c.interpreter == interpreter)
        .or_else(|| of().find(|c| c.interpreter.is_empty()))
        .or_else(|| of().next())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_frameworks_from_composer_bin_scripts_and_config_files() {
        let files = [
            "vendor/bin/behat",
            "behat.yml",
            "codeception.yml",
            "tests/Pest.php",
            "phpunit.xml",
        ];
        let found = detect(
            &|p| files.contains(&p),
            Some(r#"{"require-dev":{"pestphp/pest":"^3.0","phpspec/phpspec":"^7.5"}}"#),
        );
        let ids: Vec<(&str, &str)> = found
            .iter()
            .map(|c| (c.framework.id(), c.config_file.as_str()))
            .collect();
        assert_eq!(
            ids,
            [
                ("pest", "phpunit.xml"),
                ("codeception", "codeception.yml"),
                ("behat", "behat.yml"),
                ("phpspec", ""),
            ]
        );
        assert!(found.iter().all(|c| c.detected));
        let plain = detect(&|p| p == "phpunit.xml.dist", None);
        assert_eq!(plain[0].framework, Framework::Phpunit);
        assert_eq!(plain[0].config_file, "phpunit.xml.dist");
        assert!(detect(&|_| false, Some("{}")).is_empty());
    }

    #[test]
    fn executables_come_from_the_autoloader_or_the_path() {
        let mut c = FrameworkConfig::new(Framework::Codeception);
        assert_eq!(c.executable(), "vendor/bin/codecept");
        c.path = "tools/vendor/autoload.php".into();
        assert_eq!(c.executable(), "tools/vendor/bin/codecept");
        c.mode = Mode::Path;
        c.path = "/opt/codecept.phar".into();
        assert_eq!(c.executable(), "/opt/codecept.phar");
    }

    #[test]
    fn settings_round_trip_and_resolve_by_interpreter() {
        let value = json!([
            { "framework": "pest", "mode": "composer", "path": "vendor/autoload.php" },
            { "framework": "pest", "interpreter": "Docker PHP 8.3", "mode": "path", "path": "/app/vendor/bin/pest", "configFile": "phpunit.docker.xml" },
            { "framework": "pest" },
            { "framework": "nope" }
        ]);
        let list = from_setting(Some(&value));
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].to_object(), value[1]);
        let docker = resolve(&list, Framework::Pest, "Docker PHP 8.3").unwrap();
        assert_eq!(docker.executable(), "/app/vendor/bin/pest");
        assert_eq!(
            resolve(&list, Framework::Pest, "Other")
                .unwrap()
                .interpreter,
            ""
        );
        assert!(resolve(&list, Framework::Behat, "").is_none());
        let all = effective(
            list,
            detect(&|p| p == "vendor/bin/behat" || p == "vendor/bin/pest", None),
        );
        let ids: Vec<_> = all.iter().map(|c| (c.framework.id(), c.detected)).collect();
        assert_eq!(ids, [("pest", false), ("pest", false), ("behat", true)]);
    }
}
