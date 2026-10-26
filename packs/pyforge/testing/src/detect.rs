//! Which Python test tools a project uses, from its files (nothing is run, so it also
//! works in Safe Mode): pytest by its configuration (`pytest.ini`, `conftest.py`,
//! `[tool.pytest.ini_options]`, `[tool:pytest]`, tox's `[pytest]`) or a dependency, tox by
//! `tox.ini`, `tox.toml`, `[tool.tox]` or `[tox:tox]`.

use serde::{Deserialize, Serialize};

use crate::discover::{Runner, Scan};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTesting {
    pub pytest: bool,
    pub tox: bool,
    /// Why (`pytest.ini`, `pyproject.toml: [tool.pytest.ini_options]`, …).
    pub reasons: Vec<String>,
}

/// The files detection reads (relative to the project root).
pub const FILES: [&str; 12] = [
    "pytest.ini",
    "conftest.py",
    "tests/conftest.py",
    "pyproject.toml",
    "setup.cfg",
    "tox.ini",
    "tox.toml",
    "requirements.txt",
    "requirements-dev.txt",
    "requirements-test.txt",
    "dev-requirements.txt",
    "Pipfile",
];

/// Whether a dependency list names `pytest` (not `pytest-cov` alone).
fn names_pytest(text: &str) -> bool {
    text.lines().any(|l| {
        let t = l.trim().trim_start_matches(['"', '\'', '-', ' ']);
        let name: String = t
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
            .collect();
        name.eq_ignore_ascii_case("pytest")
    })
}

pub fn detect(read: impl Fn(&str) -> Option<String>) -> ProjectTesting {
    let mut out = ProjectTesting::default();
    let pytest = |why: String, out: &mut ProjectTesting| {
        out.pytest = true;
        out.reasons.push(why);
    };
    for f in ["pytest.ini", "conftest.py", "tests/conftest.py"] {
        if read(f).is_some() {
            pytest(f.to_string(), &mut out);
        }
    }
    if let Some(t) = read("pyproject.toml") {
        if t.contains("[tool.pytest") {
            pytest("pyproject.toml: [tool.pytest.ini_options]".into(), &mut out);
        } else if names_pytest(&t) {
            pytest("pyproject.toml: dependency".into(), &mut out);
        }
        if t.contains("[tool.tox") {
            out.tox = true;
            out.reasons.push("pyproject.toml: [tool.tox]".into());
        }
    }
    if let Some(t) = read("setup.cfg") {
        if t.contains("[tool:pytest]") {
            pytest("setup.cfg: [tool:pytest]".into(), &mut out);
        }
        if t.contains("[tox:tox]") {
            out.tox = true;
            out.reasons.push("setup.cfg: [tox:tox]".into());
        }
    }
    if let Some(t) = read("tox.ini") {
        out.tox = true;
        out.reasons.push("tox.ini".into());
        if t.contains("[pytest]") {
            pytest("tox.ini: [pytest]".into(), &mut out);
        }
    }
    if read("tox.toml").is_some() {
        out.tox = true;
        out.reasons.push("tox.toml".into());
    }
    for f in [
        "requirements.txt",
        "requirements-dev.txt",
        "requirements-test.txt",
        "dev-requirements.txt",
        "Pipfile",
    ] {
        if read(f).is_some_and(|t| names_pytest(&t)) {
            pytest(format!("{f}: pytest"), &mut out);
        }
    }
    out
}

/// The runner of a file: the setting `python.testing.runner` (`pytest`, `unittest`) wins;
/// `auto` takes pytest when the project uses it, else what the source suggests.
pub fn choose_runner(setting: &str, project: &ProjectTesting, scan: &Scan) -> Runner {
    match setting {
        "pytest" => Runner::Pytest,
        "unittest" => Runner::Unittest,
        _ if project.pytest => Runner::Pytest,
        _ => scan.suggested_runner(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::scan;

    #[test]
    fn detects_pytest_and_tox() {
        let files = |name: &str| match name {
            "pyproject.toml" => Some(
                "[project]\n[project.optional-dependencies]\ndev = [\n  \"pytest>=8\",\n]\n"
                    .to_string(),
            ),
            "tox.ini" => Some("[tox]\nenvlist = py312\n".to_string()),
            _ => None,
        };
        let d = detect(files);
        assert!(d.pytest && d.tox);
        assert_eq!(d.reasons, ["pyproject.toml: dependency", "tox.ini"]);
        let none =
            detect(|n| (n == "requirements.txt").then(|| "pytest-cov==5\nfastapi\n".to_string()));
        assert!(!none.pytest && !none.tox);
    }

    #[test]
    fn runner_choice() {
        let unit = scan(
            "tests/test_a.py",
            "import unittest\nclass A(unittest.TestCase):\n    def test_x(self): pass\n",
        );
        let plain = ProjectTesting::default();
        assert_eq!(choose_runner("auto", &plain, &unit), Runner::Unittest);
        let with_pytest = ProjectTesting {
            pytest: true,
            ..Default::default()
        };
        assert_eq!(choose_runner("auto", &with_pytest, &unit), Runner::Pytest);
        assert_eq!(
            choose_runner("unittest", &with_pytest, &unit),
            Runner::Unittest
        );
    }
}
