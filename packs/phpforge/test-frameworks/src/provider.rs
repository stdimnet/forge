//! The PHP test frameworks on the shared test runner's contribution point
//! (`forge_testing::TestFrameworkProvider`): PHPUnit, Pest, Codeception, Behat and PHPSpec
//! configurations are discovered, rerun and run with coverage through the same registry as
//! every other pack's frameworks; [`coverage_plan`] says how each measures coverage
//! (Xdebug or PCOV, path coverage, per-test data). Their command lines keep going through the PHP CLI
//! interpreter (`commands/test_frameworks.rs` and the PHPUnit run kind), so
//! [`TestFrameworkProvider::command`] is left to the backend.

use std::collections::BTreeMap;

use forge_runtools::Configuration;
use forge_testing::{CoverageFormat, FailedTest, TestFile, TestFrameworkProvider};

use crate::frameworks::Framework;

/// All PHP frameworks as one provider (`php`).
#[derive(Debug, Default, Clone, Copy)]
pub struct PhpTestFrameworks;

/// Configuration types that can run with coverage (all of them; Behat and PHPSpec through
/// their code coverage extensions).
pub const COVERAGE_TYPES: [&str; 5] = ["phpunit", "pest", "codeception", "behat", "phpspec"];

/// The coverage driver of a run (option `coverageDriver`, PHPStorm: the coverage runner).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CoverageDriver {
    /// Whatever the interpreter loads: Xdebug's coverage mode is turned on, PCOV is used
    /// when it is enabled.
    #[default]
    Auto,
    Xdebug,
    Pcov,
}

impl CoverageDriver {
    pub fn from_option(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "xdebug" => CoverageDriver::Xdebug,
            "pcov" => CoverageDriver::Pcov,
            _ => CoverageDriver::Auto,
        }
    }
}

/// How a Run with Coverage run of a PHP test configuration measures coverage and where
/// the report is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoveragePlan {
    /// Interpreter options before the framework's script (`-dpcov.enabled=1`).
    pub interpreter_args: Vec<String>,
    /// Arguments after the framework's (`--coverage-clover <file>`).
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    /// The Clover report: a name in the run's coverage folder, or (`in_dir` false) a path
    /// relative to the working directory that the framework's configuration names.
    pub file: String,
    pub in_dir: bool,
    pub format: CoverageFormat,
    /// The folder (in the coverage folder) of PHPUnit's XML report with the tests of each
    /// covered line (Track per-test coverage).
    pub tests_dir: Option<String>,
}

/// The report format of PHP coverage runs.
pub const COVERAGE_FORMAT: CoverageFormat = CoverageFormat::Clover;

/// Name of the Clover report in the run's coverage folder.
pub const CLOVER: &str = "clover.xml";
/// Folder of the per-test XML report in the run's coverage folder.
pub const TESTS_DIR: &str = "xml";

/// A value of a YAML line (`key: value`): unquoted, without a trailing comment.
fn yaml_value(v: &str) -> String {
    let v = v.trim();
    let v = if v.starts_with('"') || v.starts_with('\'') {
        let q = &v[..1];
        v[1..].split(q).next().unwrap_or_default()
    } else {
        v.split(" #").next().unwrap_or_default().trim()
    };
    v.to_string()
}

/// The Clover report a Behat or PHPSpec configuration file asks its code coverage extension
/// for: `clover: <path>` (phpspec-code-coverage's `output`) or `clover:` with a `target:`
/// below it (behat-code-coverage's `reports`).
pub fn clover_target(yaml: &str) -> Option<String> {
    let lines: Vec<&str> = yaml.lines().collect();
    let indent = |l: &str| l.len() - l.trim_start().len();
    for (i, l) in lines.iter().enumerate() {
        let t = l.trim_start();
        let Some(rest) = t.strip_prefix("clover:") else {
            continue;
        };
        let v = yaml_value(rest);
        if !v.is_empty() {
            return Some(v);
        }
        let own = indent(l);
        for below in &lines[i + 1..] {
            if below.trim().is_empty() || below.trim_start().starts_with('#') {
                continue;
            }
            if indent(below) <= own {
                break;
            }
            if let Some(target) = below.trim_start().strip_prefix("target:") {
                let v = yaml_value(target);
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
    }
    None
}

fn on(cfg: &Configuration, key: &str) -> bool {
    cfg.option(key).trim() == "true"
}

/// Run with Coverage of a PHP test configuration: the driver's interpreter options, the
/// framework's report arguments and where the Clover report is. `dir` is the run's coverage
/// folder as the command line names it; `read` reads a file relative to the working
/// directory (the Behat and PHPSpec configuration files).
///
/// | Type | Report |
/// | --- | --- |
/// | PHPUnit, Pest | `--coverage-clover <dir>/clover.xml` [`--path-coverage`] [`--coverage-xml <dir>/xml`] |
/// | Codeception | `--coverage-xml=<dir>/clover.xml` [`--coverage-phpunit=<dir>/xml`] (`coverage: enabled: true` in codeception.yml) |
/// | Behat | the `clover` report of behat-code-coverage (`DVDoug\Behat\CodeCoverage\Extension`) in behat.yml |
/// | PHPSpec | the `clover` output of phpspec-code-coverage in phpspec.yml |
///
/// Drivers: Auto sets `XDEBUG_MODE=coverage` (PCOV, when enabled, is picked by
/// php-code-coverage itself); Xdebug adds `-dxdebug.mode=coverage -dpcov.enabled=0`; PCOV
/// adds `-dpcov.enabled=1` and turns Xdebug off (`XDEBUG_MODE=off`). Path coverage (branch
/// and path coverage of PHPUnit 10+ / Pest) needs Xdebug.
pub fn coverage_plan(
    cfg: &Configuration,
    dir: &str,
    read: &dyn Fn(&str) -> Option<String>,
) -> Result<CoveragePlan, String> {
    let type_id = cfg.type_id.as_str();
    if !COVERAGE_TYPES.contains(&type_id) {
        return Err(format!(
            "'{type_id}' configurations cannot run with coverage"
        ));
    }
    let driver = CoverageDriver::from_option(cfg.option("coverageDriver"));
    let mut plan = CoveragePlan {
        format: COVERAGE_FORMAT,
        file: CLOVER.into(),
        in_dir: true,
        ..CoveragePlan::default()
    };
    match driver {
        CoverageDriver::Auto => {
            plan.env.insert("XDEBUG_MODE".into(), "coverage".into());
        }
        CoverageDriver::Xdebug => {
            plan.interpreter_args =
                vec!["-dxdebug.mode=coverage".into(), "-dpcov.enabled=0".into()];
            plan.env.insert("XDEBUG_MODE".into(), "coverage".into());
        }
        CoverageDriver::Pcov => {
            plan.interpreter_args = vec!["-dpcov.enabled=1".into()];
            plan.env.insert("XDEBUG_MODE".into(), "off".into());
        }
    }
    let per_test = on(cfg, "perTestCoverage");
    let clover = format!("{dir}/{CLOVER}");
    let tests = format!("{dir}/{TESTS_DIR}");
    match type_id {
        "phpunit" | "pest" => {
            plan.args = vec!["--coverage-clover".into(), clover];
            if on(cfg, "pathCoverage") {
                if driver == CoverageDriver::Pcov {
                    return Err("Path coverage needs Xdebug: PCOV measures lines only".into());
                }
                plan.args.push("--path-coverage".into());
            }
            if per_test {
                plan.args.extend(["--coverage-xml".into(), tests]);
                plan.tests_dir = Some(TESTS_DIR.into());
            }
        }
        "codeception" => {
            // Optional-value options: `=` keeps the value with its option.
            plan.args = vec![format!("--coverage-xml={clover}")];
            if per_test {
                plan.args.push(format!("--coverage-phpunit={tests}"));
                plan.tests_dir = Some(TESTS_DIR.into());
            }
        }
        "behat" | "phpspec" => {
            let framework = Framework::from_id(type_id).expect("PHP framework");
            let own = cfg.option("configFile").trim();
            let files: Vec<&str> = if own.is_empty() {
                framework.config_files().to_vec()
            } else {
                vec![own]
            };
            let (extension, package) = if type_id == "behat" {
                ("behat-code-coverage", "dvdoug/behat-code-coverage")
            } else {
                (
                    "phpspec-code-coverage",
                    "friends-of-phpspec/phpspec-code-coverage",
                )
            };
            let target = files
                .iter()
                .find_map(|f| read(f).and_then(|t| clover_target(&t)))
                .ok_or_else(|| {
                    format!(
                        "{} coverage needs the {extension} extension ({package}) with a Clover report in {}",
                        framework.label(),
                        files.first().copied().unwrap_or_default()
                    )
                })?;
            plan.file = target;
            plan.in_dir = false;
        }
        _ => unreachable!("checked above"),
    }
    Ok(plan)
}

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
        assert!(r.supports_coverage("phpunit") && r.supports_coverage("behat"));
        assert!(!r.supports_coverage("artisan"));
    }

    fn cfg(type_id: &str, options: &[(&str, &str)]) -> Configuration {
        let mut c = Configuration::new("All", type_id);
        for (k, v) in options {
            c = c.with_option(*k, *v);
        }
        c
    }

    fn no_files(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn phpunit_and_pest_coverage_by_driver() {
        let auto = coverage_plan(&cfg("pest", &[]), "/tmp/r", &no_files).unwrap();
        assert_eq!(auto.args, ["--coverage-clover", "/tmp/r/clover.xml"]);
        assert!(auto.interpreter_args.is_empty());
        assert_eq!(auto.env["XDEBUG_MODE"], "coverage");
        assert!(auto.in_dir && auto.tests_dir.is_none());
        let pcov = coverage_plan(
            &cfg(
                "phpunit",
                &[("coverageDriver", "pcov"), ("perTestCoverage", "true")],
            ),
            "/tmp/r",
            &no_files,
        )
        .unwrap();
        assert_eq!(pcov.interpreter_args, ["-dpcov.enabled=1"]);
        assert_eq!(pcov.env["XDEBUG_MODE"], "off");
        assert_eq!(
            pcov.args,
            [
                "--coverage-clover",
                "/tmp/r/clover.xml",
                "--coverage-xml",
                "/tmp/r/xml"
            ]
        );
        assert_eq!(pcov.tests_dir.as_deref(), Some("xml"));
        let xdebug = coverage_plan(
            &cfg(
                "phpunit",
                &[("coverageDriver", "Xdebug"), ("pathCoverage", "true")],
            ),
            "d",
            &no_files,
        )
        .unwrap();
        assert_eq!(
            xdebug.interpreter_args,
            ["-dxdebug.mode=coverage", "-dpcov.enabled=0"]
        );
        assert_eq!(
            xdebug.args,
            ["--coverage-clover", "d/clover.xml", "--path-coverage"]
        );
        let err = coverage_plan(
            &cfg(
                "phpunit",
                &[("coverageDriver", "pcov"), ("pathCoverage", "true")],
            ),
            "d",
            &no_files,
        )
        .unwrap_err();
        assert!(err.contains("Xdebug"));
        assert!(coverage_plan(&cfg("artisan", &[]), "d", &no_files).is_err());
    }

    #[test]
    fn codeception_behat_and_phpspec_coverage() {
        let c = coverage_plan(
            &cfg("codeception", &[("perTestCoverage", "true")]),
            "d",
            &no_files,
        )
        .unwrap();
        assert_eq!(
            c.args,
            ["--coverage-xml=d/clover.xml", "--coverage-phpunit=d/xml"]
        );
        let behat_yml = "default:\n  suites: ~\n  extensions:\n    DVDoug\\Behat\\CodeCoverage\\Extension:\n      filter:\n        include:\n          directories:\n            'src': ~\n      reports:\n        html:\n          target: build/coverage-behat\n        clover:\n          name: Shop\n          # the IDE reads this one\n          target: 'build/coverage-behat/clover.xml'\n";
        let read = |p: &str| (p == "behat.yml.dist").then(|| behat_yml.to_string());
        let b = coverage_plan(&cfg("behat", &[]), "d", &read).unwrap();
        assert_eq!(b.file, "build/coverage-behat/clover.xml");
        assert!(!b.in_dir && b.args.is_empty());
        assert_eq!(b.env["XDEBUG_MODE"], "coverage");
        let err = coverage_plan(&cfg("behat", &[("configFile", "ci/behat.yml")]), "d", &read)
            .unwrap_err();
        assert!(err.contains("dvdoug/behat-code-coverage") && err.contains("ci/behat.yml"));
        let spec_yml = "extensions:\n  FriendsOfPhpSpec\\PhpSpec\\CodeCoverage\\CodeCoverageExtension:\n    format:\n      - html\n      - clover\n    output:\n      html: build/coverage\n      clover: build/coverage.xml # Forge\n";
        let read = |p: &str| (p == ".phpspec.yml").then(|| spec_yml.to_string());
        let s = coverage_plan(&cfg("phpspec", &[("coverageDriver", "pcov")]), "d", &read).unwrap();
        assert_eq!(s.file, "build/coverage.xml");
        assert_eq!(s.interpreter_args, ["-dpcov.enabled=1"]);
        assert_eq!(clover_target("clover: \"a b.xml\""), Some("a b.xml".into()));
        assert_eq!(
            clover_target("reports:\n  clover:\n    name: x\nother: 1\n"),
            None
        );
    }
}
