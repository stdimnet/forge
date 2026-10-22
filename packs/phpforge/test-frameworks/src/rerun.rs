//! Rerun Failed Tests: the configuration that runs only the failed tests of a run.
//!
//! - PHPUnit: `--filter '/::(a|b)( .*)?$/'`;
//! - Pest: `--filter` with the failed tests' descriptions (and their PHPUnit method names);
//! - Codeception: `-g failed` (Codeception records the failed tests of the last run);
//! - Behat: `--rerun` (Behat remembers the failed scenarios);
//! - PHPSpec: the failed spec file (all specs when several files failed).

use forge_runtools::Configuration;
use serde::{Deserialize, Serialize};

use crate::command::regex_escape;
use crate::discover::pest_evaluable;

/// A failed test of the tree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FailedTest {
    pub name: String,
    /// Method or member of its location.
    pub member: Option<String>,
    /// Workspace-relative file.
    pub path: Option<String>,
}

fn append_options(cfg: &mut Configuration, extra: &str) {
    let joined = format!("{} {extra}", cfg.option("testRunnerOptions"));
    cfg.options
        .insert("testRunnerOptions".into(), joined.trim().to_string());
}

/// The configuration that reruns the failed tests; `None` when there is nothing to rerun.
pub fn rerun_failed(cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
    if failed.is_empty() {
        return None;
    }
    let mut c = cfg.clone();
    match cfg.type_id.as_str() {
        "phpunit" => {
            let names: Vec<String> = failed
                .iter()
                .map(|f| regex_escape(f.member.as_deref().unwrap_or(&f.name)))
                .collect();
            append_options(
                &mut c,
                &format!("--filter '/::({})( .*)?$/'", names.join("|")),
            );
        }
        "pest" => {
            let mut names: Vec<String> = Vec::new();
            for f in failed {
                let n = f.member.as_deref().unwrap_or(&f.name);
                names.push(regex_escape(n));
                names.push(regex_escape(&pest_evaluable(n)));
            }
            c.options.insert("scope".into(), "config".into());
            append_options(
                &mut c,
                &format!("--filter '{}'", names.join("|").replace('\'', "'\\''")),
            );
        }
        "codeception" => append_options(&mut c, "-g failed"),
        "behat" => append_options(&mut c, "--rerun"),
        "phpspec" => {
            let mut files: Vec<&str> = failed.iter().filter_map(|f| f.path.as_deref()).collect();
            files.sort_unstable();
            files.dedup();
            if let [file] = files[..] {
                c.options.insert("scope".into(), "file".into());
                c.options.insert("file".into(), file.into());
            }
        }
        _ => return None,
    }
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failed(name: &str, member: Option<&str>, path: Option<&str>) -> FailedTest {
        FailedTest {
            name: name.into(),
            member: member.map(Into::into),
            path: path.map(Into::into),
        }
    }

    #[test]
    fn builds_framework_specific_reruns() {
        let phpunit = Configuration::new("All", "phpunit")
            .with_option("testRunnerOptions", "--stop-on-failure");
        let r = rerun_failed(
            &phpunit,
            &[failed("a", Some("test_a"), None), failed("b.c", None, None)],
        )
        .unwrap();
        assert_eq!(
            r.option("testRunnerOptions"),
            "--stop-on-failure --filter '/::(test_a|b\\.c)( .*)?$/'"
        );

        let pest = Configuration::new("tests", "pest").with_option("scope", "file");
        let r = rerun_failed(&pest, &[failed("it works", Some("it works"), None)]).unwrap();
        assert_eq!(
            r.option("testRunnerOptions"),
            "--filter 'it works|__pest_evaluable_it_works'"
        );
        assert_eq!(r.option("scope"), "config");

        let codecept = Configuration::new("Unit", "codeception");
        assert_eq!(
            rerun_failed(&codecept, &[failed("x", None, None)])
                .unwrap()
                .option("testRunnerOptions"),
            "-g failed"
        );
        let behat = Configuration::new("features", "behat");
        assert_eq!(
            rerun_failed(&behat, &[failed("x", None, None)])
                .unwrap()
                .option("testRunnerOptions"),
            "--rerun"
        );

        let spec = Configuration::new("specs", "phpspec");
        let one = rerun_failed(
            &spec,
            &[
                failed("it a", None, Some("spec/ASpec.php")),
                failed("it b", None, Some("spec/ASpec.php")),
            ],
        )
        .unwrap();
        assert_eq!(
            (one.option("scope"), one.option("file")),
            ("file", "spec/ASpec.php")
        );
        let many = rerun_failed(
            &spec,
            &[
                failed("a", None, Some("spec/ASpec.php")),
                failed("b", None, Some("spec/BSpec.php")),
            ],
        )
        .unwrap();
        assert_eq!(many.option("scope"), "");

        assert!(rerun_failed(&phpunit, &[]).is_none());
        assert!(
            rerun_failed(&Configuration::new("x", "npm"), &[failed("a", None, None)]).is_none()
        );
    }
}
