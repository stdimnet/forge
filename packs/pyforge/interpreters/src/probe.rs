//! Parsers for what Python and the environment tools print or store: `python --version`,
//! the probe script's JSON, `pyvenv.cfg`, conda's `conda-meta`, `py -0p` (the Windows
//! launcher), `conda env list --json` and the one-path answers of `poetry env info -p`,
//! `pipenv --venv`, `hatch env find` and `uv python find`.

use serde::{Deserialize, Serialize};

/// `Python 3.12.4` (stdout, or stderr for Python 2) → `3.12.4`.
pub fn parse_version(output: &str) -> Option<String> {
    output.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("Python ")?;
        let v = rest.split_whitespace().next()?;
        v.chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
            .then(|| v.to_string())
    })
}

/// Arguments for the probe: `python -c <script>` prints [`InterpreterInfo`] as JSON.
pub const PROBE_SCRIPT: &str = "import json, platform, sys\nprint(json.dumps({'version': platform.python_version(), 'implementation': platform.python_implementation(), 'prefix': sys.prefix, 'basePrefix': getattr(sys, 'base_prefix', sys.prefix), 'executable': sys.executable, 'platform': sys.platform}))";

/// What the probe reports.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InterpreterInfo {
    pub version: String,
    pub implementation: String,
    pub prefix: String,
    pub base_prefix: String,
    pub executable: String,
    pub platform: String,
}

impl InterpreterInfo {
    /// A virtual environment (its prefix differs from the base installation).
    pub fn is_virtual(&self) -> bool {
        !self.prefix.is_empty() && self.prefix != self.base_prefix
    }
}

/// The probe's JSON (the last JSON line, in case `sitecustomize` printed something).
pub fn parse_probe(stdout: &str) -> Result<InterpreterInfo, String> {
    stdout
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str::<InterpreterInfo>(l.trim()).ok())
        .filter(|i| !i.version.is_empty())
        .ok_or_else(|| "The interpreter did not report its version".to_string())
}

/// The keys of a `pyvenv.cfg`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PyvenvCfg {
    /// The base interpreter's folder.
    pub home: String,
    /// `version` / `version_info` (`3.12.4` or `3.12.4.final.0`).
    pub version: String,
    /// Created by uv (`uv = 0.4.18`).
    pub uv: bool,
    /// Created by virtualenv.
    pub virtualenv: bool,
    pub prompt: String,
}

pub fn parse_pyvenv_cfg(text: &str) -> PyvenvCfg {
    let mut cfg = PyvenvCfg::default();
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim().to_string();
        match k.trim() {
            "home" => cfg.home = v,
            "version" | "version_info" => {
                // `3.12.4.final.0` → `3.12.4`.
                let parts: Vec<&str> = v
                    .split('.')
                    .take_while(|p| p.chars().all(|c| c.is_ascii_digit()))
                    .collect();
                if cfg.version.is_empty() || k.trim() == "version" {
                    cfg.version = parts.join(".");
                }
            }
            "uv" => cfg.uv = true,
            "virtualenv" => cfg.virtualenv = true,
            "prompt" => cfg.prompt = v.trim_matches(['\'', '"']).to_string(),
            _ => {}
        }
    }
    cfg
}

/// The Python version of a conda environment from its `conda-meta` file names
/// (`python-3.11.9-h955ad1f_0.json`).
pub fn conda_python_version<'a>(meta_files: impl IntoIterator<Item = &'a str>) -> Option<String> {
    meta_files.into_iter().find_map(|f| {
        let rest = f.strip_prefix("python-")?.strip_suffix(".json")?;
        let version = rest.split('-').next()?;
        version
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
            .then(|| version.to_string())
    })
}

/// One line of `py -0p`: (`3.12`, path). The default is marked with `*`.
pub fn parse_py_launcher(output: &str) -> Vec<(String, String)> {
    output
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let tag = l.strip_prefix("-V:").or_else(|| l.strip_prefix('-'))?;
            let mut parts = tag.split_whitespace();
            let version = parts
                .next()?
                .trim_end_matches("-64")
                .trim_end_matches("-32")
                .to_string();
            let rest: Vec<&str> = parts.filter(|p| *p != "*").collect();
            let path = rest.join(" ");
            (!path.is_empty() && path.to_ascii_lowercase().ends_with(".exe"))
                .then_some((version, path))
        })
        .collect()
}

/// `conda env list --json` → environment prefixes.
pub fn parse_conda_envs(json: &str) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|v| {
            v.get("envs").and_then(|e| e.as_array()).map(|a| {
                a.iter()
                    .filter_map(|p| p.as_str().map(String::from))
                    .collect()
            })
        })
        .unwrap_or_default()
}

/// The path a tool printed (`poetry env info -p`, `pipenv --venv`, …): its last non-empty
/// line, when it looks like a path.
pub fn parse_env_path(output: &str) -> Option<String> {
    let line = output.lines().map(str::trim).rfind(|l| !l.is_empty())?;
    let looks_like_path = line.starts_with('/')
        || line.starts_with('~')
        || line.as_bytes().get(1) == Some(&b':')
        || line.starts_with("\\\\");
    looks_like_path.then(|| line.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions_and_probe() {
        assert_eq!(parse_version("Python 3.12.4\n").as_deref(), Some("3.12.4"));
        assert_eq!(
            parse_version("Python 3.13.0rc2").as_deref(),
            Some("3.13.0rc2")
        );
        assert_eq!(parse_version("bash: python: not found"), None);
        let info = parse_probe("hello from sitecustomize\n{\"version\": \"3.12.4\", \"prefix\": \"/p/.venv\", \"basePrefix\": \"/usr\", \"executable\": \"/p/.venv/bin/python\"}\n").unwrap();
        assert_eq!(info.version, "3.12.4");
        assert!(info.is_virtual());
        assert!(parse_probe("Traceback").is_err());
    }

    #[test]
    fn parses_environment_metadata() {
        let cfg = parse_pyvenv_cfg("home = /usr/bin\ninclude-system-site-packages = false\nversion_info = 3.12.4.final.0\nuv = 0.4.18\nprompt = shop-api\n");
        assert_eq!(cfg.version, "3.12.4");
        assert!(cfg.uv && !cfg.virtualenv);
        assert_eq!(cfg.prompt, "shop-api");
        let cfg = parse_pyvenv_cfg("home = /usr/bin\nversion = 3.11.2\nvirtualenv = 20.26.0\n");
        assert_eq!(cfg.version, "3.11.2");
        assert!(cfg.virtualenv);
        assert_eq!(
            conda_python_version([
                "openssl-3.0.14-h5eee18b_0.json",
                "python-3.11.9-h955ad1f_0.json"
            ])
            .as_deref(),
            Some("3.11.9")
        );
        assert_eq!(conda_python_version(["python-dateutil-2.9.0.json"]), None);
    }

    #[test]
    fn parses_tool_output() {
        let py = " -V:3.12 *        C:\\Users\\me\\AppData\\Local\\Programs\\Python\\Python312\\python.exe\n -V:3.11          C:\\Python311\\python.exe\n";
        assert_eq!(
            parse_py_launcher(py),
            vec![
                (
                    "3.12".into(),
                    "C:\\Users\\me\\AppData\\Local\\Programs\\Python\\Python312\\python.exe".into()
                ),
                ("3.11".into(), "C:\\Python311\\python.exe".into()),
            ]
        );
        assert_eq!(
            parse_conda_envs("{\"envs\": [\"/opt/conda\", \"/opt/conda/envs/ml\"]}").len(),
            2
        );
        assert_eq!(
            parse_env_path(
                "Creating virtualenv...\n/home/me/.cache/pypoetry/virtualenvs/shop-x-py3.12\n"
            )
            .as_deref(),
            Some("/home/me/.cache/pypoetry/virtualenvs/shop-x-py3.12")
        );
        assert_eq!(
            parse_env_path("No virtualenv has been created for this project yet!"),
            None
        );
    }
}
