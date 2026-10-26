//! pytest, unittest and tox on the shared test runner's contribution point. Discovery and
//! Rerun Failed Tests here see only the file; the app adds what needs the project (the
//! runner chosen by `python.testing.runner` and detection, the classes of reported
//! tests) and builds the commands through the Python interpreter, so
//! [`TestFrameworkProvider::command`] is left to it.

use forge_runtools::Configuration;
use forge_testing::{FailedTest, TestFile, TestFrameworkProvider};

use crate::config::{PYTEST, TYPES};
use crate::discover::{scan, test_file};

#[derive(Debug, Default, Clone, Copy)]
pub struct PythonTestFrameworks;

impl TestFrameworkProvider for PythonTestFrameworks {
    fn id(&self) -> &str {
        "python"
    }

    fn name(&self) -> &str {
        "Python test runners"
    }

    fn types(&self) -> Vec<String> {
        TYPES.iter().map(|t| t.to_string()).collect()
    }

    fn accepts(&self, path: &str) -> bool {
        path.to_ascii_lowercase().ends_with(".py")
    }

    fn discover(&self, path: &str, text: &str) -> Option<TestFile> {
        let s = scan(path, text);
        Some(test_file(path, &s, s.suggested_runner())).filter(|f| !f.is_empty())
    }

    fn rerun_failed(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
        crate::rerun::rerun_failed(cfg, failed, |_| None)
    }

    fn supports_coverage(&self, type_id: &str) -> bool {
        type_id == PYTEST
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_testing::TestFrameworks;

    #[test]
    fn registers_like_any_framework() {
        let mut r = TestFrameworks::new();
        r.register(Box::new(PythonTestFrameworks));
        assert_eq!(r.for_type("tox").map(|p| p.id()), Some("python"));
        assert!(r.supports_coverage("pytest") && !r.supports_coverage("unittest"));
        let f = r.discover("tests/test_a.py", "def test_x():\n    pass\n", None);
        assert_eq!(f.framework.as_deref(), Some("pytest"));
        assert!(r
            .discover("src/app.py", "def test_x(): pass\n", None)
            .is_empty());
        assert!(r.discover("README.md", "", None).is_empty());
    }
}
