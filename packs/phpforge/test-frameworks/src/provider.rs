//! The PHP test frameworks on the shared test runner's contribution point
//! (`forge_testing::TestFrameworkProvider`): PHPUnit, Pest, Codeception, Behat and PHPSpec
//! configurations are discovered, rerun and run with coverage through the same registry as
//! every other pack's frameworks. Their command lines keep going through the PHP CLI
//! interpreter (`commands/test_frameworks.rs` and the PHPUnit run kind), so
//! [`TestFrameworkProvider::command`] is left to the backend.

use std::collections::BTreeMap;

use forge_runtools::Configuration;
use forge_testing::{CoverageFormat, FailedTest, TestFile, TestFrameworkProvider};

use crate::frameworks::Framework;

/// All PHP frameworks as one provider (`php`).
#[derive(Debug, Default, Clone, Copy)]
pub struct PhpTestFrameworks;

/// Configuration types that can run with coverage: PHPUnit and Pest write Clover XML
/// (`--coverage-clover`) with Xdebug or PCOV.
pub const COVERAGE_TYPES: [&str; 2] = ["phpunit", "pest"];

/// Run with Coverage: the arguments that write a Clover report to `file` and the
/// environment that turns on Xdebug's coverage mode (PCOV needs none).
pub fn coverage_args(type_id: &str, file: &str) -> Option<(Vec<String>, BTreeMap<String, String>)> {
    COVERAGE_TYPES.contains(&type_id).then(|| {
        (
            vec!["--coverage-clover".to_string(), file.to_string()],
            BTreeMap::from([("XDEBUG_MODE".to_string(), "coverage".to_string())]),
        )
    })
}

/// The report format of [`coverage_args`].
pub const COVERAGE_FORMAT: CoverageFormat = CoverageFormat::Clover;

impl TestFrameworkProvider for PhpTestFrameworks {
    fn id(&self) -> &str {
        "php"
    }

    fn name(&self) -> &str {
        "PHP test frameworks"
    }

    fn types(&self) -> Vec<String> {
        Framework::ALL.iter().map(|f| f.id().to_string()).collect()
    }

    fn accepts(&self, path: &str) -> bool {
        let lower = path.to_ascii_lowercase();
        lower.ends_with(".feature") || (lower.ends_with(".php") && !lower.ends_with(".blade.php"))
    }

    fn discover(&self, path: &str, text: &str) -> Option<TestFile> {
        Some(crate::discover(path, text)).filter(|f| !f.is_empty())
    }

    fn rerun_failed(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
        crate::rerun_failed(cfg, failed)
    }

    fn supports_coverage(&self, type_id: &str) -> bool {
        COVERAGE_TYPES.contains(&type_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_testing::TestFrameworks;

    #[test]
    fn php_frameworks_register_like_any_other() {
        let mut r = TestFrameworks::new();
        r.register(Box::new(PhpTestFrameworks));
        assert_eq!(r.for_type("behat").map(|p| p.id()), Some("php"));
        let f = r.discover(
            "tests/Unit/CartTest.php",
            "<?php\nclass CartTest extends TestCase {\n    public function test_total() {}\n}\n",
            None,
        );
        assert_eq!(f.framework.as_deref(), Some("phpunit"));
        assert_eq!(f.targets.len(), 2);
        assert!(r
            .discover("app/Models/User.php", "<?php class User {}", None)
            .is_empty());
        assert!(r
            .discover("resources/views/a.blade.php", "", None)
            .is_empty());
        let cfg = Configuration::new("All", "phpunit");
        let failed = [FailedTest {
            name: "test_total".into(),
            ..FailedTest::default()
        }];
        assert!(r.rerun_failed(&cfg, &failed).is_some());
        assert!(r.supports_coverage("phpunit") && !r.supports_coverage("behat"));
        let (args, env) = coverage_args("pest", "/tmp/c.xml").unwrap();
        assert_eq!(args, ["--coverage-clover", "/tmp/c.xml"]);
        assert_eq!(env["XDEBUG_MODE"], "coverage");
    }
}
