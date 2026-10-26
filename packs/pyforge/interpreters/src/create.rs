//! "Add Python Interpreter | Generate new": the commands that create an environment
//! (venv, uv, Poetry, conda) and optionally install the project's dependencies into it.

use serde::{Deserialize, Serialize};

use crate::model::EnvKind;

/// What to create.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewEnvironment {
    /// `venv`, `uv`, `poetry` or `conda`.
    pub kind: EnvKind,
    /// Absolute folder of the new environment (Poetry chooses its own).
    pub location: String,
    /// The base interpreter (venv, uv, Poetry), a path or empty for the tool's default.
    pub base_interpreter: String,
    /// Python version for conda / uv (`3.12`), when no base interpreter is given.
    pub python_version: String,
    /// Install the project's dependencies afterwards.
    pub install_dependencies: bool,
}

/// Dependency files the project has (relative paths).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectFiles {
    pub pyproject: bool,
    pub uv_lock: bool,
    pub poetry_lock: bool,
    pub requirements: Vec<String>,
}

/// One command to run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub program: String,
    pub args: Vec<String>,
}

fn step(program: &str, args: &[&str]) -> Step {
    Step {
        program: program.to_string(),
        args: args.iter().map(|a| a.to_string()).collect(),
    }
}

/// The python of an environment folder.
pub fn python_in(location: &str, windows: bool, conda: bool) -> String {
    let base = location.trim_end_matches(['/', '\\']);
    match (windows, conda) {
        (true, true) => format!("{base}\\python.exe"),
        (true, false) => format!("{base}\\Scripts\\python.exe"),
        (false, _) => format!("{base}/bin/python"),
    }
}

/// The commands, in order. Errors are worded for the dialog.
pub fn plan(
    req: &NewEnvironment,
    files: &ProjectFiles,
    windows: bool,
) -> Result<Vec<Step>, String> {
    let location = req.location.trim();
    let base = req.base_interpreter.trim();
    let needs_location = req.kind != EnvKind::Poetry;
    if needs_location && location.is_empty() {
        return Err("Location is not specified".into());
    }
    let pip_requirements = |python: &str| -> Vec<Step> {
        files
            .requirements
            .iter()
            .map(|r| step(python, &["-m", "pip", "install", "-r", r]))
            .collect()
    };
    let mut steps = Vec::new();
    match req.kind {
        EnvKind::Venv | EnvKind::Virtualenv => {
            let python = if base.is_empty() {
                if windows {
                    "python"
                } else {
                    "python3"
                }
            } else {
                base
            };
            steps.push(step(python, &["-m", "venv", location]));
            if req.install_dependencies {
                steps.extend(pip_requirements(&python_in(location, windows, false)));
            }
        }
        EnvKind::Uv => {
            let mut s = step("uv", &["venv", location]);
            let python = if base.is_empty() {
                req.python_version.trim()
            } else {
                base
            };
            if !python.is_empty() {
                s.args.extend(["--python".to_string(), python.to_string()]);
            }
            steps.push(s);
            if req.install_dependencies {
                if files.pyproject && files.uv_lock {
                    steps.push(step("uv", &["sync"]));
                } else {
                    let py = python_in(location, windows, false);
                    for r in &files.requirements {
                        steps.push(step("uv", &["pip", "install", "--python", &py, "-r", r]));
                    }
                }
            }
        }
        EnvKind::Poetry => {
            if !files.pyproject {
                return Err("Poetry needs a pyproject.toml in the project".into());
            }
            let python = if base.is_empty() {
                req.python_version.trim()
            } else {
                base
            };
            if python.is_empty() {
                return Err("Choose a base interpreter for Poetry".into());
            }
            steps.push(step("poetry", &["env", "use", python]));
            if req.install_dependencies {
                steps.push(step("poetry", &["install"]));
            }
        }
        EnvKind::Conda => {
            let version = match req.python_version.trim() {
                "" => "python".to_string(),
                v => format!("python={v}"),
            };
            steps.push(step(
                "conda",
                &["create", "--yes", "--prefix", location, &version],
            ));
            if req.install_dependencies {
                steps.extend(pip_requirements(&python_in(location, windows, true)));
            }
        }
        k => return Err(format!("Forge cannot create {} environments", k.label())),
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(steps: &[Step]) -> Vec<String> {
        steps
            .iter()
            .map(|s| format!("{} {}", s.program, s.args.join(" ")))
            .collect()
    }

    #[test]
    fn plans_each_kind() {
        let files = ProjectFiles {
            pyproject: true,
            uv_lock: true,
            requirements: vec!["requirements.txt".into()],
            ..Default::default()
        };
        let mut req = NewEnvironment {
            kind: EnvKind::Venv,
            location: "/p/.venv".into(),
            base_interpreter: "/usr/bin/python3.12".into(),
            install_dependencies: true,
            ..Default::default()
        };
        assert_eq!(
            args(&plan(&req, &files, false).unwrap()),
            vec![
                "/usr/bin/python3.12 -m venv /p/.venv",
                "/p/.venv/bin/python -m pip install -r requirements.txt"
            ]
        );
        req.kind = EnvKind::Uv;
        assert_eq!(
            args(&plan(&req, &files, false).unwrap()),
            vec!["uv venv /p/.venv --python /usr/bin/python3.12", "uv sync"]
        );
        req.kind = EnvKind::Poetry;
        assert_eq!(
            args(&plan(&req, &files, false).unwrap()),
            vec!["poetry env use /usr/bin/python3.12", "poetry install"]
        );
        req.kind = EnvKind::Conda;
        req.python_version = "3.11".into();
        req.location = "C:\\envs\\ml".into();
        assert_eq!(
            args(&plan(&req, &files, true).unwrap()),
            vec![
                "conda create --yes --prefix C:\\envs\\ml python=3.11",
                "C:\\envs\\ml\\python.exe -m pip install -r requirements.txt"
            ]
        );
        req.location = String::new();
        assert!(plan(&req, &files, false).is_err());
        req.kind = EnvKind::Docker;
        req.location = "/x".into();
        assert!(plan(&req, &files, false).is_err());
    }
}
