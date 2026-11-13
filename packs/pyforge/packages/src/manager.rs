//! Which tool installs packages into an environment, and its command lines: pip for plain
//! environments and system Pythons, `uv pip` for uv environments, `poetry add/remove/update`
//! for Poetry and `pipenv install/uninstall/update` for Pipenv (the last two also record the
//! dependency in the project, like PyCharm does).

use pyforge_interpreters::EnvKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Manager {
    Pip,
    Uv,
    Poetry,
    Pipenv,
}

impl Manager {
    pub fn label(self) -> &'static str {
        match self {
            Manager::Pip => "pip",
            Manager::Uv => "uv",
            Manager::Poetry => "Poetry",
            Manager::Pipenv => "Pipenv",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Install,
    Uninstall,
    Upgrade,
}

/// The manager of an environment. `uv_lock`: the project has a `uv.lock`.
pub fn choose(kind: EnvKind, uv_lock: bool) -> Manager {
    match kind {
        EnvKind::Uv => Manager::Uv,
        EnvKind::Poetry => Manager::Poetry,
        EnvKind::Pipenv => Manager::Pipenv,
        EnvKind::Venv | EnvKind::Virtualenv if uv_lock => Manager::Uv,
        _ => Manager::Pip,
    }
}

/// The program and arguments for `action` on `packages` (requirement specs such as
/// `Django>=4.2`) in the environment of `python`.
pub fn command(
    manager: Manager,
    action: Action,
    packages: &[String],
    python: &str,
) -> (String, Vec<String>) {
    let mut args: Vec<String> = Vec::new();
    let program = match manager {
        Manager::Pip => {
            args.extend(["-m".into(), "pip".into()]);
            match action {
                Action::Install => args.push("install".into()),
                Action::Upgrade => args.extend(["install".into(), "--upgrade".into()]),
                Action::Uninstall => args.extend(["uninstall".into(), "--yes".into()]),
            }
            python.to_string()
        }
        Manager::Uv => {
            args.push("pip".into());
            match action {
                Action::Install => args.push("install".into()),
                Action::Upgrade => args.extend(["install".into(), "--upgrade".into()]),
                Action::Uninstall => args.push("uninstall".into()),
            }
            args.extend(["--python".into(), python.to_string()]);
            "uv".into()
        }
        Manager::Poetry => {
            args.push(
                match action {
                    Action::Install => "add",
                    Action::Uninstall => "remove",
                    Action::Upgrade => "update",
                }
                .into(),
            );
            "poetry".into()
        }
        Manager::Pipenv => {
            args.push(
                match action {
                    Action::Install => "install",
                    Action::Uninstall => "uninstall",
                    Action::Upgrade => "update",
                }
                .into(),
            );
            "pipenv".into()
        }
    };
    args.extend(packages.iter().cloned());
    (program, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(m: Manager, a: Action, p: &[&str]) -> String {
        let (prog, args) = command(
            m,
            a,
            &p.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            "/p/.venv/bin/python",
        );
        format!("{prog} {}", args.join(" "))
    }

    #[test]
    fn commands_per_manager() {
        assert_eq!(
            line(Manager::Pip, Action::Install, &["requests>=2"]),
            "/p/.venv/bin/python -m pip install requests>=2"
        );
        assert_eq!(
            line(Manager::Pip, Action::Uninstall, &["rich"]),
            "/p/.venv/bin/python -m pip uninstall --yes rich"
        );
        assert_eq!(
            line(Manager::Uv, Action::Upgrade, &["rich"]),
            "uv pip install --upgrade --python /p/.venv/bin/python rich"
        );
        assert_eq!(
            line(Manager::Poetry, Action::Install, &["rich"]),
            "poetry add rich"
        );
        assert_eq!(
            line(Manager::Pipenv, Action::Upgrade, &["rich"]),
            "pipenv update rich"
        );
        assert_eq!(choose(EnvKind::Venv, true), Manager::Uv);
        assert_eq!(choose(EnvKind::Conda, false), Manager::Pip);
        assert_eq!(choose(EnvKind::Docker, true), Manager::Pip);
    }
}
