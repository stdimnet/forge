//! The interpreter model and how it is stored in the settings.
//!
//! Interpreters live in two `objects` settings (lists of tables with string values):
//! `php.interpreters` in the user settings (available in every project) and
//! `php.projectInterpreters` in the project settings ("Visible only for this project").
//! `php.interpreter` selects the project's CLI interpreter by name; a value that names no
//! interpreter is the path of a local `php` executable (`php`, `/usr/bin/php8.3`), so
//! settings written before named interpreters existed keep working.
//!
//! List fields are stored as text: compose files one per line, path mappings one
//! `local<TAB>remote` pair per line, configuration options one `name=value` per line and
//! extensions comma separated.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::mapping::PathMapping;

/// Setting with the interpreters of every project (user settings).
pub const GLOBAL_KEY: &str = "php.interpreters";
/// Setting with the interpreters visible only in the open project (project settings).
pub const PROJECT_KEY: &str = "php.projectInterpreters";
/// Setting that selects the project's CLI interpreter (a name or a local path).
pub const SELECTED_KEY: &str = "php.interpreter";

/// Where PHP runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InterpreterKind {
    /// A `php` executable on this machine (or on the project's host for remote projects).
    #[default]
    Local,
    /// `docker run --rm` of an image.
    Docker,
    /// A service of Docker Compose files (`docker compose run` / `exec`).
    Compose,
    /// A `php` on a host of Settings | Tools | SSH Configurations.
    Ssh,
    /// A distribution of the Windows Subsystem for Linux.
    Wsl,
}

impl InterpreterKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Docker => "docker",
            Self::Compose => "compose",
            Self::Ssh => "ssh",
            Self::Wsl => "wsl",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "docker" => Self::Docker,
            "compose" => Self::Compose,
            "ssh" => Self::Ssh,
            "wsl" => Self::Wsl,
            _ => Self::Local,
        }
    }

    /// PHP runs somewhere else than the project files: paths need mappings.
    pub fn is_remote(self) -> bool {
        self != Self::Local
    }
}

/// Docker Compose "Lifecycle".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Lifecycle {
    /// "Always start a new container ('docker compose run')".
    #[default]
    Run,
    /// "Connect to existing container ('docker compose exec')".
    Exec,
}

/// One "Configuration options" row: passed as `-d name=value`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigOption {
    pub name: String,
    pub value: String,
}

/// A configured PHP CLI interpreter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PhpInterpreter {
    pub id: String,
    pub name: String,
    pub kind: InterpreterKind,
    /// The PHP executable: a local path, or the path inside the container / on the host.
    /// Empty = `php`.
    pub path: String,
    /// Docker: the Docker context to use (empty = the default server).
    pub docker_server: String,
    /// Docker: the image (`php:8.3-cli`).
    pub image: String,
    /// Docker Compose: configuration files, relative to the project root or absolute.
    pub compose_files: Vec<String>,
    /// Docker Compose: the service.
    pub service: String,
    pub lifecycle: Lifecycle,
    /// SSH: the id of a saved SSH configuration.
    pub ssh_config: String,
    /// WSL: the distribution (`Ubuntu-24.04`); empty = the default one.
    pub distribution: String,
    /// Path mappings set by the user (the automatic ones are derived, see [`crate::mapping`]).
    pub mappings: Vec<PathMapping>,
    /// "Configuration options", passed as `-d name=value`.
    pub options: Vec<ConfigOption>,
    /// "Debugger extension": a zend_extension loaded with `-dzend_extension=` when Xdebug is
    /// not loaded by the php.ini.
    pub debugger_extension: String,
    /// Detected on Reload (cached so the dialog and the language level need no process).
    pub version: String,
    pub config_file: String,
    pub additional_ini: String,
    pub xdebug: String,
    pub extensions: Vec<String>,
    /// In `php.projectInterpreters` ("Visible only for this project"); not stored.
    pub project: bool,
}

fn lines(s: &str) -> impl Iterator<Item = &str> {
    s.lines().map(str::trim).filter(|l| !l.is_empty())
}

impl PhpInterpreter {
    /// A local interpreter for an executable path (the legacy `php.interpreter` value).
    pub fn local(path: &str) -> Self {
        let path = match path.trim() {
            "" => "php",
            p => p,
        };
        Self {
            name: path.to_string(),
            path: path.to_string(),
            ..Default::default()
        }
    }

    /// The PHP executable (`php` when empty).
    pub fn php(&self) -> &str {
        match self.path.trim() {
            "" => "php",
            p => p,
        }
    }

    /// The server name of debug runs (`PHP_IDE_CONFIG=serverName=…`), which picks this
    /// interpreter's path mappings in the debugger: the name without spaces and separators.
    pub fn debug_server_name(&self) -> String {
        let name: String = self
            .name
            .trim()
            .chars()
            .map(|c| {
                if c.is_whitespace() || c == '&' || c == ';' {
                    '-'
                } else {
                    c
                }
            })
            .collect();
        if name.is_empty() {
            format!("interpreter-{}", self.id)
        } else {
            name
        }
    }

    /// `major.minor.patch` of the detected version, if known.
    pub fn version(&self) -> Option<&str> {
        Some(self.version.trim()).filter(|v| !v.is_empty())
    }

    /// Read one table of an `objects` setting.
    pub fn from_setting(table: &Map<String, Value>, project: bool) -> Self {
        let s = |k: &str| {
            table
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        let mappings = s("mappings")
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| {
                let (local, remote) = l.split_once('\t')?;
                Some(PathMapping {
                    local: local.trim().to_string(),
                    remote: remote.trim().to_string(),
                })
            })
            .collect();
        let options = lines(&s("options"))
            .map(|l| match l.split_once('=') {
                Some((n, v)) => ConfigOption {
                    name: n.trim().to_string(),
                    value: v.trim().to_string(),
                },
                None => ConfigOption {
                    name: l.to_string(),
                    value: String::new(),
                },
            })
            .collect();
        Self {
            id: s("id"),
            name: s("name"),
            kind: InterpreterKind::parse(&s("kind")),
            path: s("path"),
            docker_server: s("dockerServer"),
            image: s("image"),
            compose_files: lines(&s("composeFiles")).map(String::from).collect(),
            service: s("service"),
            lifecycle: if s("lifecycle") == "exec" {
                Lifecycle::Exec
            } else {
                Lifecycle::Run
            },
            ssh_config: s("sshConfig"),
            distribution: s("distribution"),
            mappings,
            options,
            debugger_extension: s("debuggerExtension"),
            version: s("version"),
            config_file: s("configFile"),
            additional_ini: s("additionalIni"),
            xdebug: s("xdebug"),
            extensions: s("extensions")
                .split(',')
                .map(str::trim)
                .filter(|e| !e.is_empty())
                .map(String::from)
                .collect(),
            project,
        }
    }

    /// The table stored in an `objects` setting; empty fields are left out.
    pub fn to_setting(&self) -> Map<String, Value> {
        let mut m = Map::new();
        let mut put = |k: &str, v: String| {
            if !v.trim().is_empty() {
                m.insert(k.to_string(), Value::String(v));
            }
        };
        put("id", self.id.clone());
        put("name", self.name.clone());
        put("kind", self.kind.id().to_string());
        put("path", self.path.clone());
        put("dockerServer", self.docker_server.clone());
        put("image", self.image.clone());
        put("composeFiles", self.compose_files.join("\n"));
        put("service", self.service.clone());
        if self.kind == InterpreterKind::Compose {
            put(
                "lifecycle",
                match self.lifecycle {
                    Lifecycle::Run => "run",
                    Lifecycle::Exec => "exec",
                }
                .to_string(),
            );
        }
        put("sshConfig", self.ssh_config.clone());
        put("distribution", self.distribution.clone());
        put(
            "mappings",
            self.mappings
                .iter()
                .filter(|m| !m.remote.trim().is_empty())
                .map(|m| format!("{}\t{}", m.local.trim(), m.remote.trim()))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        put(
            "options",
            self.options
                .iter()
                .filter(|o| !o.name.trim().is_empty())
                .map(|o| format!("{}={}", o.name.trim(), o.value.trim()))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        put("debuggerExtension", self.debugger_extension.clone());
        put("version", self.version.clone());
        put("configFile", self.config_file.clone());
        put("additionalIni", self.additional_ini.clone());
        put("xdebug", self.xdebug.clone());
        put("extensions", self.extensions.join(","));
        m
    }
}

/// The interpreters of both settings layers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterpreterSet {
    pub global: Vec<PhpInterpreter>,
    pub project: Vec<PhpInterpreter>,
}

fn read_list(value: Option<&Value>, project: bool) -> Vec<PhpInterpreter> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_object)
                .map(|t| PhpInterpreter::from_setting(t, project))
                .filter(|i| !i.name.trim().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

impl InterpreterSet {
    /// From the values of [`GLOBAL_KEY`] and [`PROJECT_KEY`].
    pub fn from_settings(global: Option<&Value>, project: Option<&Value>) -> Self {
        Self {
            global: read_list(global, false),
            project: read_list(project, true),
        }
    }

    /// Project interpreters first: their names win.
    pub fn all(&self) -> impl Iterator<Item = &PhpInterpreter> {
        self.project.iter().chain(self.global.iter())
    }

    /// The interpreter named `name` (or with that id).
    pub fn find(&self, name: &str) -> Option<&PhpInterpreter> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        self.all()
            .find(|i| i.name == name)
            .or_else(|| self.all().find(|i| i.id == name))
    }

    /// What `selected` (a value of [`SELECTED_KEY`] or a run configuration's interpreter)
    /// stands for: a configured interpreter, else a local executable path.
    pub fn resolve(&self, selected: &str) -> PhpInterpreter {
        self.find(selected)
            .cloned()
            .unwrap_or_else(|| PhpInterpreter::local(selected))
    }

    /// The setting values of both lists.
    pub fn to_settings(&self) -> (Value, Value) {
        let list = |l: &[PhpInterpreter]| {
            Value::Array(l.iter().map(|i| Value::Object(i.to_setting())).collect())
        };
        (list(&self.global), list(&self.project))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trips_through_the_settings_tables() {
        let docker = PhpInterpreter {
            id: "d1".into(),
            name: "Docker php:8.3".into(),
            kind: InterpreterKind::Compose,
            path: "/usr/local/bin/php".into(),
            compose_files: vec!["docker-compose.yml".into(), "docker-compose.dev.yml".into()],
            service: "app".into(),
            lifecycle: Lifecycle::Exec,
            mappings: vec![PathMapping {
                local: "".into(),
                remote: "/var/www/html".into(),
            }],
            options: vec![ConfigOption {
                name: "memory_limit".into(),
                value: "-1".into(),
            }],
            version: "8.3.12".into(),
            extensions: vec!["Core".into(), "xdebug".into()],
            ..Default::default()
        };
        let table = docker.to_setting();
        assert_eq!(table["mappings"], "\t/var/www/html");
        assert_eq!(
            table["composeFiles"],
            "docker-compose.yml\ndocker-compose.dev.yml"
        );
        assert!(table.values().all(Value::is_string));
        assert!(!table.contains_key("image"));
        assert_eq!(PhpInterpreter::from_setting(&table, false), docker);
    }

    #[test]
    fn resolves_names_then_paths() {
        let set = InterpreterSet::from_settings(
            Some(
                &json!([{ "id": "a", "name": "PHP 8.3", "path": "/usr/bin/php8.3" }, { "name": "" }]),
            ),
            Some(&json!([{ "id": "b", "name": "PHP 8.3", "kind": "docker", "image": "php:8.3" }])),
        );
        assert_eq!(set.global.len(), 1);
        // The project's interpreter wins over a global one with the same name.
        assert_eq!(set.resolve("PHP 8.3").kind, InterpreterKind::Docker);
        assert!(set.resolve("PHP 8.3").project);
        assert_eq!(set.resolve("a").path, "/usr/bin/php8.3");
        let legacy = set.resolve("/opt/php/bin/php");
        assert_eq!(legacy.kind, InterpreterKind::Local);
        assert_eq!(legacy.php(), "/opt/php/bin/php");
        assert_eq!(set.resolve("").php(), "php");
        assert_eq!(set.resolve("PHP 8.3").debug_server_name(), "PHP-8.3");
        let (g, p) = set.to_settings();
        assert_eq!(g[0]["name"], "PHP 8.3");
        assert_eq!(p[0]["kind"], "docker");
    }
}
