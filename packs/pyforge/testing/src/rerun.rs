//! Rerun Failed Tests: the failed tests of a tree as pytest node ids
//! (`tests/test_cart.py::TestCart::test_add[one]`) or unittest ids
//! (`tests.test_cart.TestCart.test_add`). Reports name a test and its file; the classes
//! around it come from the file's source (`read`).

use forge_runtools::Configuration;
use forge_testing::FailedTest;

use crate::config::{self, PYTEST, UNITTEST};
use crate::discover::scan;

/// `test_add[one]` → `test_add`.
fn function(name: &str) -> &str {
    name.split('[').next().unwrap_or(name)
}

/// A project path relative to the configuration's working directory.
fn in_working_dir(cfg: &Configuration, path: &str) -> String {
    let wd = cfg.working_directory.trim().trim_end_matches('/');
    if wd.is_empty() || wd == "." || wd.starts_with('/') {
        return path.to_string();
    }
    path.strip_prefix(&format!("{wd}/"))
        .unwrap_or(path)
        .to_string()
}

/// The classes around test `name` in the file, outermost first.
fn classes_of(source: Option<&str>, path: &str, name: &str, member: Option<&str>) -> Vec<String> {
    let Some(text) = source else {
        return Vec::new();
    };
    let f = function(name);
    let found = scan(path, text);
    let candidates: Vec<_> = found
        .items
        .iter()
        .filter(|i| !i.class && i.name == f)
        .collect();
    // A member such as `TestCart.test_add` tells same-named tests apart.
    let by_member = member.and_then(|m| {
        candidates
            .iter()
            .find(|i| i.dotted() == m || i.node() == m.replace('.', "::"))
    });
    by_member
        .or(candidates.first())
        .map(|i| i.classes.clone())
        .unwrap_or_default()
}

/// The configuration that runs only the failed tests; `None` when none has a file.
pub fn rerun_failed(
    cfg: &Configuration,
    failed: &[FailedTest],
    read: impl Fn(&str) -> Option<String>,
) -> Option<Configuration> {
    let mut ids: Vec<String> = Vec::new();
    for t in failed {
        let Some(path) = t.path.as_deref().filter(|p| p.ends_with(".py")) else {
            continue;
        };
        let source = read(path);
        let classes = classes_of(source.as_deref(), path, &t.name, t.member.as_deref());
        let local = in_working_dir(cfg, path);
        let id = match cfg.type_id.as_str() {
            PYTEST => {
                let mut parts = vec![local];
                parts.extend(classes);
                parts.push(t.name.clone());
                parts.join("::")
            }
            UNITTEST => {
                let module = local
                    .trim_end_matches(".py")
                    .trim_end_matches("/__init__")
                    .replace('/', ".");
                let mut parts = vec![module];
                parts.extend(classes);
                parts.push(function(&t.name).to_string());
                parts.join(".")
            }
            _ => return None,
        };
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    if ids.is_empty() {
        return None;
    }
    let mut out = cfg.clone();
    out.options.insert("scope".into(), "nodes".into());
    out.options.insert("nodes".into(), ids.join("\n"));
    debug_assert!(config::TYPES.contains(&out.type_id.as_str()));
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "import unittest\n\nclass TestCart:\n    def test_add(self): pass\n\ndef test_add(): pass\n\nclass Checks(unittest.TestCase):\n    def test_total(self): pass\n";

    fn failed(name: &str, member: Option<&str>, path: &str) -> FailedTest {
        FailedTest {
            name: name.into(),
            member: member.map(str::to_string),
            path: Some(path.into()),
        }
    }

    #[test]
    fn pytest_node_ids_with_classes() {
        let cfg = Configuration::new("All", "pytest");
        let read = |p: &str| (p == "tests/test_cart.py").then(|| SRC.to_string());
        let out = rerun_failed(
            &cfg,
            &[
                failed(
                    "test_add[one]",
                    Some("TestCart.test_add"),
                    "tests/test_cart.py",
                ),
                failed("test_total", None, "tests/test_cart.py"),
                failed("test_gone", None, "tests/test_gone.py"),
                failed("x", None, "README.md"),
            ],
            read,
        )
        .unwrap();
        assert_eq!(out.option("scope"), "nodes");
        assert_eq!(
            out.option("nodes"),
            "tests/test_cart.py::TestCart::test_add[one]\ntests/test_cart.py::Checks::test_total\ntests/test_gone.py::test_gone"
        );
        assert!(rerun_failed(&cfg, &[], read).is_none());
    }

    #[test]
    fn unittest_ids_relative_to_the_working_dir() {
        let mut cfg = Configuration::new("All", "unittest");
        cfg.working_directory = "backend".into();
        let out = rerun_failed(
            &cfg,
            &[failed("test_total", None, "backend/tests/test_cart.py")],
            |_| Some(SRC.to_string()),
        )
        .unwrap();
        assert_eq!(out.option("nodes"), "tests.test_cart.Checks.test_total");
        assert!(rerun_failed(
            &Configuration::new("t", "tox"),
            &[failed("a", None, "a.py")],
            |_| None
        )
        .is_none());
    }
}
