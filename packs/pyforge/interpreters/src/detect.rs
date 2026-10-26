//! Finding Python interpreters on this machine:
//!
//! - the project's own environments (`.venv`, `venv`, `env` with a `pyvenv.cfg`), typed by
//!   their metadata and the project's lock files (uv, Poetry, Pipenv, virtualenv, venv),
//! - Poetry, Pipenv and Hatch environments of the project outside of it (their cache
//!   folders, or the tools' own answer through `run_tool`),
//! - conda / mamba environments (`environments.txt`, `<root>/envs/*` of the usual
//!   installations) with the version from `conda-meta`,
//! - pyenv versions, and
//! - system Pythons on `PATH`, in Homebrew and the python.org / Windows Store folders.
//!
//! Versions come from environment metadata where possible; only system Pythons are asked
//! (`version_of`). Tests build their own [`SearchEnv`] with fixture folders.

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::model::EnvKind;
use crate::probe;

/// A detected interpreter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detected {
    /// The Python executable.
    pub path: String,
    pub kind: EnvKind,
    /// `3.12.4`, empty when unknown.
    pub version: String,
    /// The environment's folder (empty for system Pythons).
    pub env_dir: String,
    /// Belongs to the open project (its folder or a tool's environment for it).
    pub project: bool,
}

/// What detection looks at.
#[derive(Debug, Clone, Default)]
pub struct SearchEnv {
    pub path: Option<OsString>,
    pub home: Option<PathBuf>,
    pub windows: bool,
    pub project_root: Option<PathBuf>,
    /// `PYENV_ROOT` (default `~/.pyenv`).
    pub pyenv_root: Option<PathBuf>,
    /// `WORKON_HOME` (Pipenv; default `~/.local/share/virtualenvs`).
    pub workon_home: Option<PathBuf>,
    /// Poetry's cache (`~/.cache/pypoetry`, macOS `~/Library/Caches/pypoetry`).
    pub poetry_cache: Option<PathBuf>,
    /// Hatch's data dir (`~/.local/share/hatch`).
    pub hatch_data: Option<PathBuf>,
    /// `%LOCALAPPDATA%` (python.org installs, the Windows Store).
    pub local_appdata: Option<PathBuf>,
    /// Leave out fixed system folders (`/usr/bin`, Homebrew, `/opt/conda`); for tests.
    pub skip_system_dirs: bool,
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

impl SearchEnv {
    /// This machine, for the project at `root`.
    pub fn system(project_root: Option<PathBuf>) -> Self {
        let home = dirs::home_dir();
        SearchEnv {
            path: std::env::var_os("PATH"),
            windows: cfg!(windows),
            project_root,
            pyenv_root: env_path("PYENV_ROOT"),
            workon_home: env_path("WORKON_HOME"),
            poetry_cache: env_path("POETRY_CACHE_DIR")
                .or_else(|| dirs::cache_dir().map(|c| c.join("pypoetry"))),
            hatch_data: env_path("HATCH_DATA_DIR")
                .or_else(|| dirs::data_local_dir().map(|d| d.join("hatch"))),
            local_appdata: env_path("LOCALAPPDATA"),
            skip_system_dirs: false,
            home,
        }
    }

    /// `python` inside an environment folder.
    pub fn python_in(&self, env_dir: &Path) -> PathBuf {
        if self.windows {
            let scripts = env_dir.join("Scripts").join("python.exe");
            if scripts.is_file() {
                return scripts;
            }
            // conda puts python.exe in the prefix itself.
            env_dir.join("python.exe")
        } else {
            env_dir.join("bin").join("python")
        }
    }
}

fn is_file(p: &Path) -> bool {
    p.is_file()
}

/// The project's name as tools derive their environment names from it: pyproject's name,
/// else the folder's.
fn project_names(root: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(text) = std::fs::read_to_string(root.join("pyproject.toml")) {
        let f = pyforge_lang_python::deps::pyproject(&text);
        if let Some(n) = f.project_name {
            names.push(n.to_ascii_lowercase().replace([' ', '.'], "-"));
        }
    }
    if let Some(dir) = root.file_name() {
        names.push(dir.to_string_lossy().to_string());
    }
    names
}

/// Collects results without duplicates (same executable).
struct Found {
    seen: HashSet<PathBuf>,
    out: Vec<Detected>,
}

impl Found {
    fn add(
        &mut self,
        path: &Path,
        kind: EnvKind,
        version: String,
        env_dir: Option<&Path>,
        project: bool,
    ) {
        let key = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        // A venv's python is a symlink to the base one: key environments by their folder.
        let key = match env_dir {
            Some(d) => d.to_path_buf(),
            None => key,
        };
        if !self.seen.insert(key) {
            return;
        }
        self.out.push(Detected {
            path: path.display().to_string(),
            kind,
            version,
            env_dir: env_dir.map(|d| d.display().to_string()).unwrap_or_default(),
            project,
        });
    }
}

/// A virtual environment at `dir` (`pyvenv.cfg` next to its python), typed by its
/// metadata or `default`.
fn add_venv(
    env: &SearchEnv,
    found: &mut Found,
    dir: &Path,
    default: EnvKind,
    project: bool,
) -> bool {
    let Ok(text) = std::fs::read_to_string(dir.join("pyvenv.cfg")) else {
        return false;
    };
    let python = env.python_in(dir);
    if !is_file(&python) {
        return false;
    }
    let cfg = probe::parse_pyvenv_cfg(&text);
    let kind = if cfg.uv {
        EnvKind::Uv
    } else if default == EnvKind::Venv && cfg.virtualenv {
        EnvKind::Virtualenv
    } else {
        default
    };
    found.add(&python, kind, cfg.version, Some(dir), project);
    true
}

/// A conda environment at `prefix`.
fn add_conda(env: &SearchEnv, found: &mut Found, prefix: &Path, project: bool) {
    if !prefix.join("conda-meta").is_dir() {
        return;
    }
    let python = if env.windows {
        prefix.join("python.exe")
    } else {
        prefix.join("bin").join("python")
    };
    if !is_file(&python) {
        return;
    }
    let names: Vec<String> = std::fs::read_dir(prefix.join("conda-meta"))
        .map(|it| {
            it.flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    let version = probe::conda_python_version(names.iter().map(String::as_str)).unwrap_or_default();
    found.add(&python, EnvKind::Conda, version, Some(prefix), project);
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Runs a tool and returns its stdout (`None` when it is missing or failed).
pub type ToolRunner<'a> = &'a dyn Fn(&str, &[&str], &Path) -> Option<String>;

/// Every interpreter that can be found. `version_of` asks a system Python for its version;
/// `run_tool` runs `poetry` / `pipenv` / `hatch` / `conda` (only when the project uses
/// them).
pub fn detect(
    env: &SearchEnv,
    version_of: &dyn Fn(&Path) -> Option<String>,
    run_tool: ToolRunner<'_>,
) -> Vec<Detected> {
    let mut found = Found {
        seen: HashSet::new(),
        out: Vec::new(),
    };
    let home = env.home.clone();

    // ---- The project's environments.
    if let Some(root) = &env.project_root {
        let has = |f: &str| root.join(f).exists();
        let default = if has("uv.lock") {
            EnvKind::Uv
        } else if has("poetry.lock") {
            EnvKind::Poetry
        } else if has("Pipfile") {
            EnvKind::Pipenv
        } else {
            EnvKind::Venv
        };
        for name in [".venv", "venv", "env", ".env"] {
            add_venv(env, &mut found, &root.join(name), default, true);
        }
        if root.join("conda-meta").is_dir() {
            add_conda(env, &mut found, root, true);
        }
        let names = project_names(root);
        let uses_poetry = has("poetry.lock")
            || std::fs::read_to_string(root.join("pyproject.toml"))
                .map(|t| t.contains("[tool.poetry"))
                .unwrap_or(false);
        // Poetry: `<cache>/virtualenvs/<name>-<hash>-py3.X`, or ask Poetry.
        if uses_poetry {
            let mut any = false;
            if let Some(cache) = &env.poetry_cache {
                for dir in subdirs(&cache.join("virtualenvs")) {
                    let n = dir
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    if names.iter().any(|p| n.starts_with(&format!("{p}-"))) {
                        any |= add_venv(env, &mut found, &dir, EnvKind::Poetry, true);
                    }
                }
            }
            if !any {
                if let Some(p) = run_tool("poetry", &["env", "info", "-p"], root)
                    .and_then(|o| probe::parse_env_path(&o))
                {
                    add_venv(env, &mut found, Path::new(&p), EnvKind::Poetry, true);
                }
            }
        }
        // Pipenv: `<WORKON_HOME>/<folder>-<hash>`, or ask Pipenv.
        if has("Pipfile") {
            let workon = env
                .workon_home
                .clone()
                .or_else(|| home.as_ref().map(|h| h.join(".local/share/virtualenvs")));
            let folder = root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let mut any = false;
            if let Some(w) = workon {
                for dir in subdirs(&w) {
                    let n = dir
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    if n.starts_with(&format!("{folder}-")) {
                        any |= add_venv(env, &mut found, &dir, EnvKind::Pipenv, true);
                    }
                }
            }
            if !any {
                if let Some(p) =
                    run_tool("pipenv", &["--venv"], root).and_then(|o| probe::parse_env_path(&o))
                {
                    add_venv(env, &mut found, Path::new(&p), EnvKind::Pipenv, true);
                }
            }
        }
        // Hatch: `<data>/env/virtual/<name>/<hash>/<env>`.
        let uses_hatch = has("hatch.toml")
            || std::fs::read_to_string(root.join("pyproject.toml"))
                .map(|t| t.contains("[tool.hatch"))
                .unwrap_or(false);
        if uses_hatch {
            if let Some(data) = &env.hatch_data {
                for name in &names {
                    for hash in subdirs(&data.join("env/virtual").join(name)) {
                        for dir in subdirs(&hash) {
                            add_venv(env, &mut found, &dir, EnvKind::Hatch, true);
                        }
                    }
                }
            }
        }
    }

    // ---- conda / mamba.
    if let Some(h) = &home {
        let mut roots: Vec<PathBuf> = [
            "miniconda3",
            "anaconda3",
            "miniforge3",
            "mambaforge",
            "micromamba",
            ".conda",
        ]
        .iter()
        .map(|n| h.join(n))
        .collect();
        if !env.skip_system_dirs {
            roots.push(PathBuf::from("/opt/conda"));
            roots.push(PathBuf::from("/opt/miniconda3"));
        }
        for root in &roots {
            add_conda(env, &mut found, root, false);
            for dir in subdirs(&root.join("envs")) {
                add_conda(env, &mut found, &dir, false);
            }
        }
        if let Ok(text) = std::fs::read_to_string(h.join(".conda/environments.txt")) {
            for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
                add_conda(env, &mut found, Path::new(line), false);
            }
        }
    }

    // ---- pyenv.
    let pyenv = env
        .pyenv_root
        .clone()
        .or_else(|| home.as_ref().map(|h| h.join(".pyenv")));
    if let Some(root) = pyenv {
        for dir in subdirs(&root.join("versions")) {
            if add_venv(env, &mut found, &dir, EnvKind::Venv, false) {
                continue;
            }
            let python = if env.windows {
                dir.join("python.exe")
            } else {
                dir.join("bin").join("python")
            };
            if is_file(&python) {
                let version = dir
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                found.add(&python, EnvKind::Pyenv, version, None, false);
            }
        }
    }

    // ---- System Pythons.
    let mut dirs: Vec<PathBuf> = env
        .path
        .as_ref()
        .map(|p| std::env::split_paths(p).collect())
        .unwrap_or_default();
    if !env.skip_system_dirs {
        if env.windows {
            if let Some(local) = &env.local_appdata {
                dirs.extend(subdirs(&local.join("Programs").join("Python")));
            }
        } else {
            dirs.extend(
                [
                    "/usr/bin",
                    "/usr/local/bin",
                    "/opt/homebrew/bin",
                    "/opt/local/bin",
                ]
                .iter()
                .map(PathBuf::from),
            );
        }
    }
    let pyenv_shims = |d: &Path| d.ends_with("shims");
    for dir in dirs.iter().filter(|d| !pyenv_shims(d)) {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .map(|it| {
                it.flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .filter(|n| is_python_name(n, env.windows))
                    .collect()
            })
            .unwrap_or_default();
        // python3.13 before python3 before python: specific names first.
        names.sort_by_key(|n| std::cmp::Reverse(n.len()));
        for n in names {
            let p = dir.join(&n);
            if !is_file(&p) || is_windows_store_stub(&p) {
                continue;
            }
            let key = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
            if found.seen.contains(&key) {
                continue;
            }
            let Some(version) = version_of(&p) else {
                continue;
            };
            found.add(&p, EnvKind::System, version, None, false);
        }
    }
    let mut out = found.out;
    // Project environments first, then newest versions.
    out.sort_by(|a, b| {
        b.project.cmp(&a.project).then_with(|| {
            let va = pyforge_lang_python::Version::parse(&a.version);
            let vb = pyforge_lang_python::Version::parse(&b.version);
            vb.cmp(&va)
        })
    });
    out
}

/// `python`, `python3`, `python3.12` (`.exe` on Windows); not `python3-config`.
fn is_python_name(name: &str, windows: bool) -> bool {
    let n = if windows {
        match name.to_ascii_lowercase().strip_suffix(".exe") {
            Some(s) => s.to_string(),
            None => return false,
        }
    } else {
        name.to_string()
    };
    let Some(rest) = n.strip_prefix("python") else {
        return false;
    };
    rest.is_empty()
        || rest.chars().all(|c| c.is_ascii_digit() || c == '.') && !rest.starts_with('.')
}

/// The Windows Store's `python.exe` alias that only opens the Store.
fn is_windows_store_stub(p: &Path) -> bool {
    p.to_string_lossy().contains("WindowsApps")
        && std::fs::metadata(p).map(|m| m.len() == 0).unwrap_or(false)
}

/// `python --version` with a timeout.
pub fn version_of(python: &Path, timeout: Duration) -> Option<String> {
    let out = run_with_timeout(python.as_os_str(), &["--version"], None, timeout)?;
    probe::parse_version(&out)
}

/// Run a program and return its stdout and stderr, or `None` on failure / timeout.
pub fn run_with_timeout(
    program: &std::ffi::OsStr,
    args: &[&str],
    cwd: Option<&Path>,
    timeout: Duration,
) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let mut child = cmd.spawn().ok()?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = String::new();
                let mut err = String::new();
                child.stdout.take()?.read_to_string(&mut out).ok()?;
                if let Some(mut e) = child.stderr.take() {
                    let _ = e.read_to_string(&mut err);
                }
                return status.success().then(|| format!("{out}{err}"));
            }
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return None,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn write(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    /// An executable shell script that prints `output`.
    pub fn fake_python(p: &Path, output: &str) {
        write(p, &format!("#!/bin/sh\necho '{output}'\n"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    fn venv(dir: &Path, cfg: &str) {
        write(&dir.join("pyvenv.cfg"), cfg);
        fake_python(&dir.join("bin/python"), "Python 0");
    }

    #[cfg(unix)]
    #[test]
    fn finds_environments_and_system_pythons() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let root = home.join("work/shop-api");
        write(
            &root.join("pyproject.toml"),
            "[project]\nname = \"shop-api\"\n\n[tool.poetry]\nname = \"shop-api\"\n",
        );
        write(&root.join("uv.lock"), "");
        venv(
            &root.join(".venv"),
            "home = /usr/bin\nversion_info = 3.12.4.final.0\nuv = 0.4.18\n",
        );
        // Poetry's cache has an environment for this project and one for another.
        let poetry = home.join(".cache/pypoetry");
        venv(
            &poetry.join("virtualenvs/shop-api-Xy12AbCd-py3.11"),
            "home = /usr/bin\nversion = 3.11.9\n",
        );
        venv(
            &poetry.join("virtualenvs/other-Ab12-py3.11"),
            "version = 3.11.9\n",
        );
        // conda base and an env.
        let conda = home.join("miniconda3");
        write(
            &conda.join("conda-meta/python-3.10.14-h955ad1f_0.json"),
            "{}",
        );
        fake_python(&conda.join("bin/python"), "");
        write(
            &conda.join("envs/ml/conda-meta/python-3.11.9-h0_0.json"),
            "{}",
        );
        fake_python(&conda.join("envs/ml/bin/python"), "");
        // pyenv.
        fake_python(&home.join(".pyenv/versions/3.9.19/bin/python"), "");
        // PATH with python3.13 and python3 (same version answered by the fake runner).
        let bin = tmp.path().join("bin");
        fake_python(&bin.join("python3.13"), "Python 3.13.0");
        fake_python(&bin.join("python3-config"), "");
        let env = SearchEnv {
            path: Some(std::env::join_paths([&bin]).unwrap()),
            home: Some(home.clone()),
            project_root: Some(root.clone()),
            poetry_cache: Some(poetry),
            skip_system_dirs: true,
            ..Default::default()
        };
        let asked = std::cell::RefCell::new(Vec::new());
        let version = |p: &Path| {
            asked.borrow_mut().push(p.to_path_buf());
            Some("3.13.0".to_string())
        };
        let found = detect(&env, &version, &|_, _, _| None);
        let got: Vec<(EnvKind, &str, bool)> = found
            .iter()
            .map(|d| (d.kind, d.version.as_str(), d.project))
            .collect();
        assert_eq!(
            got,
            vec![
                (EnvKind::Uv, "3.12.4", true),
                (EnvKind::Poetry, "3.11.9", true),
                (EnvKind::System, "3.13.0", false),
                (EnvKind::Conda, "3.11.9", false),
                (EnvKind::Conda, "3.10.14", false),
                (EnvKind::Pyenv, "3.9.19", false),
            ]
        );
        assert!(found[0].path.ends_with(".venv/bin/python"));
        assert!(found[0].env_dir.ends_with("shop-api/.venv"));
        // Only the system Python was run.
        assert_eq!(asked.borrow().len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn asks_tools_when_their_environments_are_elsewhere() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("billing");
        write(&root.join("Pipfile"), "[packages]\n");
        let elsewhere = tmp.path().join("envs/billing-AbCd");
        venv(&elsewhere, "version = 3.12.1\nvirtualenv = 20.0\n");
        let env = SearchEnv {
            home: Some(tmp.path().join("nohome")),
            project_root: Some(root.clone()),
            skip_system_dirs: true,
            ..Default::default()
        };
        let p = elsewhere.display().to_string();
        let found = detect(&env, &|_| None, &|prog, args, cwd| {
            assert_eq!(
                (prog, args, cwd),
                ("pipenv", &["--venv"][..], root.as_path())
            );
            Some(format!("{p}\n"))
        });
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, EnvKind::Pipenv);
        assert_eq!(found[0].version, "3.12.1");
    }

    #[cfg(unix)]
    #[test]
    fn runs_python_for_its_version() {
        let tmp = tempfile::tempdir().unwrap();
        let py = tmp.path().join("python3");
        fake_python(&py, "Python 3.12.7");
        assert_eq!(
            version_of(&py, Duration::from_secs(5)).as_deref(),
            Some("3.12.7")
        );
        assert_eq!(
            version_of(&tmp.path().join("missing"), Duration::from_secs(5)),
            None
        );
    }

    #[test]
    fn python_names() {
        assert!(is_python_name("python3.12", false));
        assert!(is_python_name("python", false));
        assert!(!is_python_name("python3-config", false));
        assert!(!is_python_name("pythonw", false));
        assert!(is_python_name("python.exe", true));
        assert!(!is_python_name("python3", true));
    }
}
