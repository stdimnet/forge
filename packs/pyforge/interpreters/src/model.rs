//! The Python interpreter model and how it is stored in the settings.
//!
//! Like PHP's CLI interpreters, configured interpreters live in two `objects` settings:
//! `python.interpreters` (user settings, every project) and `python.projectInterpreters`
//! (project settings). `python.interpreter` selects the project's interpreter by name; a
//! value that names no interpreter is the path of a local `python` executable, which is
//! what selecting a detected environment stores.
//!
//! Docker, Docker Compose, SSH and WSL interpreters run through the PHP interpreters'
//! plumbing (`phpforge_interpreters::command::wrap`, path mappings): [`PythonInterpreter::
//! wrap_target`] describes the Python interpreter as such a target.

use phpforge_interpreters::{InterpreterKind, Lifecycle, PathMapping, PhpInterpreter};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Setting with the interpreters of every project (user settings).
pub const GLOBAL_KEY: &str = "python.interpreters";
/// Setting with the interpreters of the open project (project settings).
pub const PROJECT_KEY: &str = "python.projectInterpreters";
/// Setting that selects the project's interpreter (a name or a local path).
pub const SELECTED_KEY: &str = "python.interpreter";

/// What kind of environment an interpreter is (PyCharm's "Add Python Interpreter" types).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EnvKind {
    /// A Python installed on the machine (PATH, py launcher, Homebrew, python.org).
    #[default]
    System,
    /// A pyenv version.
    Pyenv,
    /// `python -m venv`.
    Venv,
    /// `virtualenv`.
    Virtualenv,
    /// A virtual environment managed by uv (`uv venv`, `uv sync`).
    Uv,
    Poetry,
    /// A conda / mamba environment.
    Conda,
    Pipenv,
    Hatch,
    Docker,
    Compose,
    Ssh,
    Wsl,
}

impl EnvKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Pyenv => "pyenv",
            Self::Venv => "venv",
            Self::Virtualenv => "virtualenv",
            Self::Uv => "uv",
            Self::Poetry => "poetry",
            Self::Conda => "conda",
            Self::Pipenv => "pipenv",
            Self::Hatch => "hatch",
            Self::Docker => "docker",
            Self::Compose => "compose",
            Self::Ssh => "ssh",
            Self::Wsl => "wsl",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "pyenv" => Self::Pyenv,
            "venv" => Self::Venv,
            "virtualenv" => Self::Virtualenv,
            "uv" => Self::Uv,
            "poetry" => Self::Poetry,
            "conda" => Self::Conda,
            "pipenv" => Self::Pipenv,
            "hatch" => Self::Hatch,
            "docker" => Self::Docker,
            "compose" => Self::Compose,
            "ssh" => Self::Ssh,
            "wsl" => Self::Wsl,
            _ => Self::System,
        }
    }

    /// Display name ("Virtualenv", "uv", "Docker Compose").
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Pyenv => "pyenv",
            Self::Venv => "Virtualenv",
            Self::Virtualenv => "Virtualenv",
            Self::Uv => "uv",
            Self::Poetry => "Poetry",
            Self::Conda => "Conda",
            Self::Pipenv => "Pipenv",
            Self::Hatch => "Hatch",
            Self::Docker => "Docker",
            Self::Compose => "Docker Compose",
            Self::Ssh => "SSH",
            Self::Wsl => "WSL",
        }
    }

    /// Python runs somewhere else than the project files: paths need mappings.
    pub fn is_remote(self) -> bool {
        matches!(self, Self::Docker | Self::Compose | Self::Ssh | Self::Wsl)
    }

    /// A virtual environment (packages go into it, not into a system Python).
    pub fn is_environment(self) -> bool {
        matches!(
            self,
            Self::Venv
                | Self::Virtualenv
                | Self::Uv
                | Self::Poetry
                | Self::Conda
                | Self::Pipenv
                | Self::Hatch
        )
    }
}

/// A Python interpreter: detected or configured.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PythonInterpreter {
    pub id: String,
    pub name: String,
    pub kind: EnvKind,
    /// The Python executable: a local path, or the path in the container / on the host.
    /// Empty = `python3`.
    pub path: String,
    /// The environment's folder (`.venv`, a conda prefix); empty for system Pythons.
    pub env_dir: String,
    /// `3.12.4`, from the environment's metadata or `python --version`.
    pub version: String,
    /// Docker: the Docker context (empty = the default server) and the image.
    pub docker_server: String,
    pub image: String,
    /// Docker Compose: configuration files and service.
    pub compose_files: Vec<String>,
    pub service: String,
    pub lifecycle: Lifecycle,
    /// SSH: the id of a saved SSH configuration.
    pub ssh_config: String,
    /// WSL: the distribution (empty = the default one).
    pub distribution: String,
    /// Path mappings set by the user (Docker's `/opt/project` mount, compose volumes and WSL
    /// paths are derived).
    pub mappings: Vec<PathMapping>,
    /// In `python.projectInterpreters`; not stored.
    pub project: bool,
}

impl PythonInterpreter {
    /// A local interpreter for an executable path (what selecting a detected one stores).
    pub fn local(path: &str) -> Self {
        let path = match path.trim() {
            "" => "python3",
            p => p,
        };
        Self {
            name: path.to_string(),
            path: path.to_string(),
            ..Default::default()
        }
    }

    /// The Python executable (`python3` when empty).
    pub fn python(&self) -> &str {
        match self.path.trim() {
            "" => "python3",
            p => p,
        }
    }

    /// `3.12` of the version, if known.
    pub fn short_version(&self) -> Option<String> {
        let mut parts = self.version.trim().split('.');
        let major = parts.next().filter(|m| !m.is_empty())?;
        Some(match parts.next() {
            Some(minor) => format!("{major}.{minor}"),
            None => major.to_string(),
        })
    }

    /// The status bar text: `Python 3.12 (shop-api)`, PyCharm style.
    pub fn label(&self) -> String {
        let version = self
            .short_version()
            .map(|v| format!("Python {v}"))
            .unwrap_or_else(|| "Python".into());
        let detail = match self.kind {
            EnvKind::System | EnvKind::Pyenv => None,
            k if k.is_remote() => Some(self.name.clone()),
            _ => env_name(&self.env_dir).or_else(|| Some(self.name.clone())),
        };
        match detail.filter(|d| !d.is_empty()) {
            Some(d) => format!("{version} ({d})"),
            None => version,
        }
    }

    /// The target for `phpforge_interpreters::command::wrap` (run `python` as the program).
    pub fn wrap_target(&self) -> PhpInterpreter {
        PhpInterpreter {
            id: self.id.clone(),
            name: self.name.clone(),
            kind: match self.kind {
                EnvKind::Docker => InterpreterKind::Docker,
                EnvKind::Compose => InterpreterKind::Compose,
                EnvKind::Ssh => InterpreterKind::Ssh,
                EnvKind::Wsl => InterpreterKind::Wsl,
                _ => InterpreterKind::Local,
            },
            path: self.python().to_string(),
            docker_server: self.docker_server.clone(),
            image: self.image.clone(),
            compose_files: self.compose_files.clone(),
            service: self.service.clone(),
            lifecycle: self.lifecycle,
            ssh_config: self.ssh_config.clone(),
            distribution: self.distribution.clone(),
            mappings: self.mappings.clone(),
            project: self.project,
            ..Default::default()
        }
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
        let lines = |k: &str| -> Vec<String> {
            s(k).lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        };
        Self {
            id: s("id"),
            name: s("name"),
            kind: EnvKind::parse(&s("kind")),
            path: s("path"),
            env_dir: s("envDir"),
            version: s("version"),
            docker_server: s("dockerServer"),
            image: s("image"),
            compose_files: lines("composeFiles"),
            service: s("service"),
            lifecycle: if s("lifecycle") == "exec" {
                Lifecycle::Exec
            } else {
                Lifecycle::Run
            },
            ssh_config: s("sshConfig"),
            distribution: s("distribution"),
            mappings: s("mappings")
                .lines()
                .filter_map(|l| {
                    let (local, remote) = l.split_once('\t')?;
                    Some(PathMapping {
                        local: local.trim().to_string(),
                        remote: remote.trim().to_string(),
                    })
                })
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
        put("envDir", self.env_dir.clone());
        put("version", self.version.clone());
        put("dockerServer", self.docker_server.clone());
        put("image", self.image.clone());
        put("composeFiles", self.compose_files.join("\n"));
        put("service", self.service.clone());
        if self.kind == EnvKind::Compose && self.lifecycle == Lifecycle::Exec {
            put("lifecycle", "exec".into());
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
        m
    }
}

/// The name of an environment folder: `.venv` and `venv` are named after their parent
/// (`~/work/shop-api/.venv` → `shop-api`), others after themselves.
pub fn env_name(env_dir: &str) -> Option<String> {
    let p = env_dir.trim().replace('\\', "/");
    let mut parts = p.trim_end_matches('/').rsplit('/');
    let last = parts.next().filter(|l| !l.is_empty())?;
    if matches!(last, ".venv" | "venv" | "env" | ".env") {
        parts
            .next()
            .map(String::from)
            .or_else(|| Some(last.to_string()))
    } else {
        Some(last.to_string())
    }
}

/// The interpreters of both settings layers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterpreterSet {
    pub global: Vec<PythonInterpreter>,
    pub project: Vec<PythonInterpreter>,
}

fn read_list(value: Option<&Value>, project: bool) -> Vec<PythonInterpreter> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_object)
                .map(|t| PythonInterpreter::from_setting(t, project))
                .filter(|i| !i.name.trim().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

impl InterpreterSet {
    pub fn from_settings(global: Option<&Value>, project: Option<&Value>) -> Self {
        Self {
            global: read_list(global, false),
            project: read_list(project, true),
        }
    }

    /// Project interpreters first: their names win.
    pub fn all(&self) -> impl Iterator<Item = &PythonInterpreter> {
        self.project.iter().chain(self.global.iter())
    }

    pub fn find(&self, name: &str) -> Option<&PythonInterpreter> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        self.all()
            .find(|i| i.name == name)
            .or_else(|| self.all().find(|i| i.id == name))
            .or_else(|| self.all().find(|i| !i.kind.is_remote() && i.path == name))
    }

    /// A configured interpreter, else a local executable path. `None` when nothing is
    /// selected.
    pub fn resolve(&self, selected: &str) -> Option<PythonInterpreter> {
        if selected.trim().is_empty() {
            return None;
        }
        Some(
            self.find(selected)
                .cloned()
                .unwrap_or_else(|| PythonInterpreter::local(selected)),
        )
    }

    pub fn to_settings(&self) -> (Value, Value) {
        let list = |l: &[PythonInterpreter]| {
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
    fn round_trips_and_resolves() {
        let docker = PythonInterpreter {
            id: "d1".into(),
            name: "Docker python:3.12".into(),
            kind: EnvKind::Compose,
            path: "/usr/local/bin/python".into(),
            compose_files: vec!["compose.yaml".into()],
            service: "api".into(),
            lifecycle: Lifecycle::Exec,
            mappings: vec![PathMapping {
                local: "".into(),
                remote: "/app".into(),
            }],
            version: "3.12.7".into(),
            ..Default::default()
        };
        let table = docker.to_setting();
        assert!(table.values().all(Value::is_string));
        assert_eq!(PythonInterpreter::from_setting(&table, false), docker);
        assert_eq!(docker.label(), "Python 3.12 (Docker python:3.12)");
        let target = docker.wrap_target();
        assert_eq!(target.kind, InterpreterKind::Compose);
        assert_eq!(target.php(), "/usr/local/bin/python");

        let set = InterpreterSet::from_settings(
            Some(
                &json!([{ "id": "a", "name": "Python 3.12", "path": "/usr/bin/python3.12", "version": "3.12.1" }]),
            ),
            Some(
                &json!([{ "id": "b", "name": "Python 3.12", "kind": "docker", "image": "python:3.12" }]),
            ),
        );
        assert_eq!(set.resolve("Python 3.12").unwrap().kind, EnvKind::Docker);
        assert_eq!(set.resolve("/usr/bin/python3.12").unwrap().id, "a");
        let venv = set.resolve("/work/shop/.venv/bin/python").unwrap();
        assert_eq!(venv.kind, EnvKind::System);
        assert!(set.resolve("").is_none());
    }

    #[test]
    fn labels_like_pycharm() {
        let venv = PythonInterpreter {
            kind: EnvKind::Uv,
            env_dir: "/home/me/work/shop-api/.venv".into(),
            version: "3.13.0".into(),
            ..Default::default()
        };
        assert_eq!(venv.label(), "Python 3.13 (shop-api)");
        let conda = PythonInterpreter {
            kind: EnvKind::Conda,
            env_dir: "/home/me/miniconda3/envs/ml".into(),
            version: "3.11.9".into(),
            ..Default::default()
        };
        assert_eq!(conda.label(), "Python 3.11 (ml)");
        assert_eq!(
            PythonInterpreter::local("/usr/bin/python3").label(),
            "Python"
        );
        assert_eq!(env_name("C:\\work\\api\\venv").as_deref(), Some("api"));
        assert!(EnvKind::Poetry.is_environment() && !EnvKind::System.is_environment());
    }
}
