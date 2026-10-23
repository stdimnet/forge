//! Command lines of the Pest, Codeception, Behat and PHPSpec run configurations, after the
//! PHP interpreter and its options (the backend puts `php` in front and runs the result
//! through the configuration's CLI interpreter), plus how the run reports results:
//!
//! | Type | Runs | Results |
//! | --- | --- | --- |
//! | Pest | `vendor/bin/pest --teamcity [--configuration f] [dir \| file] [--filter 'test\|method']` | TeamCity |
//! | Codeception | `vendor/bin/codecept run [suite] [file[:method]] [-g group] [--env env] [-c f] --xml <dir>/codeception.xml --no-interaction` | JUnit file |
//! | Behat | `vendor/bin/behat [--config f] [--profile p] --format=progress --out=std --format=junit --out=<dir> [path[:line]]` | JUnit files |
//! | PHPSpec | `vendor/bin/phpspec run --format=junit --no-interaction [--config f] [--bootstrap f] [file[:line]]` | JUnit on stdout |
//!
//! Options of the configurations (`options` of the run configuration):
//! - all: `scope`, `configFile` (empty: the framework's default), `testRunnerOptions`;
//! - Pest: `directory`, `file`, `test` (a `test()`/`it()`/`describe()` description),
//!   `dataset` (a row of its dataset);
//! - Codeception: `suite`, `directory`, `file`, `method`, `group`, `env`;
//! - Behat: `directory`, `file`, `line` (a scenario or example row), `profile`;
//! - PHPSpec: `directory`, `file`, `line` (an example), `method` (shown in names).

use forge_output::{JUnitFlavor, TestReport};
use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use serde::Serialize;

use crate::discover::pest_evaluable;
use crate::frameworks::{Framework, FrameworkConfig};

/// The test runner part of a command line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestCommand {
    /// Arguments after the PHP interpreter, starting with the framework's script.
    pub args: Vec<String>,
    pub report: TestReport,
    pub flavor: JUnitFlavor,
}

/// Run configuration types this module builds.
pub const TYPES: [&str; 4] = ["pest", "codeception", "behat", "phpspec"];

fn required<'a>(cfg: &'a Configuration, key: &str, label: &str) -> Result<&'a str, String> {
    match cfg.option(key).trim() {
        "" => Err(format!("{label} is not specified")),
        v => Ok(v),
    }
}

/// Escape a literal for a PCRE `--filter` pattern.
pub fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\.+*?()|[]{}^$/#".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The configuration file a run passes: the configuration's own, else the framework
/// setting's default.
fn config_file(cfg: &Configuration, fw: Option<&FrameworkConfig>) -> Option<String> {
    let own = cfg.option("configFile").trim();
    if !own.is_empty() {
        return Some(own.to_string());
    }
    fw.map(|f| f.config_file.trim().to_string())
        .filter(|c| !c.is_empty())
}

/// `file` or `file:line`.
fn with_line(file: &str, line: &str) -> String {
    match line.trim() {
        "" => file.to_string(),
        l => format!("{file}:{l}"),
    }
}

/// The command of a Pest, Codeception, Behat or PHPSpec configuration. `fw` is the
/// framework entry for the configuration's interpreter (Settings | PHP | Test Frameworks),
/// `report_dir` the directory JUnit files are written to (Codeception, Behat).
pub fn build(
    cfg: &Configuration,
    fw: Option<&FrameworkConfig>,
    report_dir: &str,
) -> Result<TestCommand, String> {
    let framework = Framework::from_id(&cfg.type_id)
        .filter(|f| *f != Framework::Phpunit)
        .ok_or_else(|| format!("`{}` is not a test framework configuration", cfg.type_id))?;
    let exe = fw
        .filter(|f| f.framework == framework)
        .map(FrameworkConfig::executable)
        .unwrap_or_else(|| FrameworkConfig::new(framework).executable());
    let scope = cfg.option("scope");
    let extra = split_args(cfg.option("testRunnerOptions"));
    let mut args = vec![exe];
    let (report, flavor) = match framework {
        Framework::Pest => {
            args.push("--teamcity".into());
            if let Some(c) = config_file(cfg, fw) {
                args.extend(["--configuration".into(), c]);
            }
            args.extend(extra);
            match scope {
                "directory" => args.push(required(cfg, "directory", "Directory")?.into()),
                "file" => args.push(required(cfg, "file", "File")?.into()),
                "test" => {
                    let test = required(cfg, "test", "Test")?;
                    if let Some(file) = Some(cfg.option("file").trim()).filter(|f| !f.is_empty()) {
                        args.push(file.into());
                    }
                    // Pest matches descriptions and its PHPUnit method names.
                    let mut filter = format!(
                        "{}|{}",
                        regex_escape(test),
                        regex_escape(&pest_evaluable(test))
                    );
                    if let Some(set) = Some(cfg.option("dataset").trim()).filter(|d| !d.is_empty())
                    {
                        filter = format!("(?:{filter}).*{}", regex_escape(set));
                    }
                    args.extend(["--filter".into(), filter]);
                }
                _ => {}
            }
            (TestReport::TeamCity, JUnitFlavor::Generic)
        }
        Framework::Codeception => {
            args.push("run".into());
            match scope {
                "suite" => args.push(required(cfg, "suite", "Suite")?.into()),
                "directory" => args.push(required(cfg, "directory", "Directory")?.into()),
                "file" => args.push(required(cfg, "file", "File")?.into()),
                "method" => {
                    let file = required(cfg, "file", "File")?;
                    let method = required(cfg, "method", "Method")?;
                    args.push(format!("{file}:{method}"));
                }
                _ => {}
            }
            for group in cfg
                .option("group")
                .split(',')
                .map(str::trim)
                .filter(|g| !g.is_empty())
            {
                args.extend(["-g".into(), group.into()]);
            }
            for env in cfg
                .option("env")
                .split(',')
                .map(str::trim)
                .filter(|e| !e.is_empty())
            {
                args.extend(["--env".into(), env.into()]);
            }
            if let Some(c) = config_file(cfg, fw) {
                args.extend(["-c".into(), c]);
            }
            args.extend([
                "--xml".into(),
                format!("{}/codeception.xml", report_dir.trim_end_matches('/')),
                "--no-interaction".into(),
            ]);
            args.extend(extra);
            (TestReport::JUnitFile, JUnitFlavor::Generic)
        }
        Framework::Behat => {
            if let Some(c) = config_file(cfg, fw) {
                args.extend(["--config".into(), c]);
            }
            if let Some(p) = Some(cfg.option("profile").trim()).filter(|p| !p.is_empty()) {
                args.extend(["--profile".into(), p.into()]);
            }
            args.extend([
                "--format=progress".into(),
                "--out=std".into(),
                "--format=junit".into(),
                format!("--out={}", report_dir.trim_end_matches('/')),
            ]);
            args.extend(extra);
            match scope {
                "directory" => args.push(required(cfg, "directory", "Directory")?.into()),
                "file" => args.push(required(cfg, "file", "Feature file")?.into()),
                "scenario" => {
                    let file = required(cfg, "file", "Feature file")?;
                    args.push(with_line(file, required(cfg, "line", "Line")?));
                }
                _ => {}
            }
            (TestReport::JUnitFile, JUnitFlavor::Behat)
        }
        Framework::Phpspec => {
            args.extend([
                "run".into(),
                "--format=junit".into(),
                "--no-interaction".into(),
            ]);
            if let Some(c) = config_file(cfg, fw) {
                args.extend(["--config".into(), c]);
            }
            if let Some(b) = fw
                .map(|f| f.bootstrap_file.trim())
                .filter(|b| !b.is_empty())
            {
                args.extend(["--bootstrap".into(), b.into()]);
            }
            args.extend(extra);
            match scope {
                "directory" => args.push(required(cfg, "directory", "Directory")?.into()),
                "file" => args.push(required(cfg, "file", "Spec file")?.into()),
                "method" => {
                    let file = required(cfg, "file", "Spec file")?;
                    args.push(with_line(file, required(cfg, "line", "Line")?));
                }
                _ => {}
            }
            (TestReport::JUnitStdout, JUnitFlavor::PhpSpec)
        }
        Framework::Phpunit => unreachable!("filtered above"),
    };
    Ok(TestCommand {
        args,
        report,
        flavor,
    })
}

/// PHPUnit configurations keep their own command builder; this fills in what Settings | PHP
/// | Test Frameworks configures for them: the executable (`phpunitPath`), the default
/// configuration file and the default bootstrap file.
pub fn phpunit_defaults(cfg: &mut Configuration, fw: Option<&FrameworkConfig>) {
    let Some(fw) = fw.filter(|f| f.framework == Framework::Phpunit) else {
        return;
    };
    if cfg.option("phpunitPath").trim().is_empty() {
        cfg.options.insert("phpunitPath".into(), fw.executable());
    }
    if cfg.option("configFile").trim().is_empty() && !fw.config_file.trim().is_empty() {
        cfg.options
            .insert("configFile".into(), fw.config_file.trim().into());
    }
    let bootstrap = fw.bootstrap_file.trim();
    let options = cfg.option("testRunnerOptions").to_string();
    if !bootstrap.is_empty() && !split_args(&options).iter().any(|a| a == "--bootstrap") {
        let joined = format!(
            "--bootstrap {} {options}",
            forge_runtools::args::quote_arg(bootstrap)
        );
        cfg.options
            .insert("testRunnerOptions".into(), joined.trim().to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frameworks::Mode;

    fn cfg(type_id: &str, options: &[(&str, &str)]) -> Configuration {
        let mut c = Configuration::new("t", type_id);
        for (k, v) in options {
            c = c.with_option(k, *v);
        }
        c
    }

    fn args(c: &Configuration, fw: Option<&FrameworkConfig>) -> String {
        build(c, fw, "/r").unwrap().args.join(" ")
    }

    #[test]
    fn pest_runs_files_directories_and_single_tests() {
        assert_eq!(args(&cfg("pest", &[]), None), "vendor/bin/pest --teamcity");
        assert_eq!(
            args(
                &cfg(
                    "pest",
                    &[("scope", "directory"), ("directory", "tests/Unit")]
                ),
                None
            ),
            "vendor/bin/pest --teamcity tests/Unit"
        );
        let single = cfg(
            "pest",
            &[
                ("scope", "test"),
                ("file", "tests/Unit/CartTest.php"),
                ("test", "`Cart` → it adds items (twice)"),
                ("testRunnerOptions", "--bail"),
            ],
        );
        let mut fw = FrameworkConfig::new(Framework::Pest);
        fw.config_file = "phpunit.xml".into();
        assert_eq!(
            build(&single, Some(&fw), "/r").unwrap().args,
            [
                "vendor/bin/pest",
                "--teamcity",
                "--configuration",
                "phpunit.xml",
                "--bail",
                "tests/Unit/CartTest.php",
                "--filter",
                "`Cart` → it adds items \\(twice\\)|__pest_evaluable__Cart__→_it_adds_items__twice_"
            ]
        );
        let set = cfg(
            "pest",
            &[("scope", "test"), ("test", "it a"), ("dataset", "one")],
        );
        assert_eq!(
            build(&set, None, "/r").unwrap().args[3],
            "(?:it a|__pest_evaluable_it_a).*one"
        );
        assert_eq!(
            build(&single, None, "/r").unwrap().report,
            TestReport::TeamCity
        );
        assert_eq!(
            build(&cfg("pest", &[("scope", "file")]), None, "/r").unwrap_err(),
            "File is not specified"
        );
    }

    #[test]
    fn codeception_runs_suites_tests_groups_and_writes_a_junit_report() {
        let c = cfg(
            "codeception",
            &[
                ("scope", "method"),
                ("file", "tests/Acceptance/LoginCest.php"),
                ("method", "tryToLogin"),
                ("group", "smoke, admin"),
                ("env", "chrome"),
            ],
        );
        let built = build(&c, None, "/tmp/forge/run-1/").unwrap();
        assert_eq!(
            built.args.join(" "),
            "vendor/bin/codecept run tests/Acceptance/LoginCest.php:tryToLogin -g smoke -g admin --env chrome --xml /tmp/forge/run-1/codeception.xml --no-interaction"
        );
        assert_eq!(built.report, TestReport::JUnitFile);
        let mut fw = FrameworkConfig::new(Framework::Codeception);
        fw.mode = Mode::Path;
        fw.path = "codecept.phar".into();
        fw.config_file = "codeception.dist.yml".into();
        assert_eq!(
            args(&cfg("codeception", &[("scope", "suite"), ("suite", "Unit")]), Some(&fw)),
            "codecept.phar run Unit -c codeception.dist.yml --xml /r/codeception.xml --no-interaction"
        );
    }

    #[test]
    fn behat_runs_features_and_scenarios_with_progress_and_junit() {
        let c = cfg(
            "behat",
            &[
                ("scope", "scenario"),
                ("file", "features/checkout.feature"),
                ("line", "12"),
                ("profile", "ci"),
            ],
        );
        let built = build(&c, None, "/r").unwrap();
        assert_eq!(
            built.args.join(" "),
            "vendor/bin/behat --profile ci --format=progress --out=std --format=junit --out=/r features/checkout.feature:12"
        );
        assert_eq!(
            (built.report, built.flavor),
            (TestReport::JUnitFile, JUnitFlavor::Behat)
        );
    }

    #[test]
    fn phpspec_reports_junit_on_stdout() {
        let mut fw = FrameworkConfig::new(Framework::Phpspec);
        fw.bootstrap_file = "spec/bootstrap.php".into();
        let c = cfg(
            "phpspec",
            &[
                ("scope", "method"),
                ("file", "spec/Acme/CartSpec.php"),
                ("line", "14"),
            ],
        );
        let built = build(&c, Some(&fw), "/r").unwrap();
        assert_eq!(
            built.args.join(" "),
            "vendor/bin/phpspec run --format=junit --no-interaction --bootstrap spec/bootstrap.php spec/Acme/CartSpec.php:14"
        );
        assert_eq!(
            (built.report, built.flavor),
            (TestReport::JUnitStdout, JUnitFlavor::PhpSpec)
        );
        assert!(build(&cfg("phpunit", &[]), None, "/r").is_err());
    }

    #[test]
    fn phpunit_takes_executable_config_and_bootstrap_from_the_settings() {
        let mut fw = FrameworkConfig::new(Framework::Phpunit);
        fw.mode = Mode::Path;
        fw.path = "tools/phpunit.phar".into();
        fw.config_file = "phpunit.dist.xml".into();
        fw.bootstrap_file = "tests/boot strap.php".into();
        let mut c = cfg("phpunit", &[("testRunnerOptions", "--stop-on-failure")]);
        phpunit_defaults(&mut c, Some(&fw));
        assert_eq!(c.option("phpunitPath"), "tools/phpunit.phar");
        assert_eq!(c.option("configFile"), "phpunit.dist.xml");
        assert_eq!(
            c.option("testRunnerOptions"),
            "--bootstrap 'tests/boot strap.php' --stop-on-failure"
        );
        let mut own = cfg(
            "phpunit",
            &[("phpunitPath", "bin/phpunit"), ("configFile", "x.xml")],
        );
        phpunit_defaults(&mut own, Some(&fw));
        assert_eq!(
            (own.option("phpunitPath"), own.option("configFile")),
            ("bin/phpunit", "x.xml")
        );
    }
}
