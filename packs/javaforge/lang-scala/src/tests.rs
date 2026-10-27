//! ScalaTest and MUnit on the shared test runner (`forge-testing`), configuration types
//! `scalatest` and `munit`. Tests run through sbt (`core/testOnly com.acme.CartSpec -- -z
//! "adds items"`), whose JUnit XML reports (`target/test-reports`) feed the test tree.
//!
//! Options: `scope` (`all`, `package`, `class`, `test`, `pattern`), `class` (fully qualified
//! suite), `test` (the test name), `package`, `tests` (suites, one per line), `file`,
//! `project` (the sbt project id), `projectDir` (its folder), `sbtOptions`; filled by the
//! backend: `buildProgram` (sbt).

use forge_output::{JUnitFlavor, LocationRule, TestReport};
use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use forge_testing::{
    CommandContext, FailedTest, TargetKind, TestCommand, TestFile, TestFrameworkProvider,
    TestTarget,
};

use crate::sbt::task_command;
use crate::source::parse;
use crate::suites::{suites_of, Framework};

pub const SCALATEST: &str = "scalatest";
pub const MUNIT: &str = "munit";

/// ScalaTest (`ScalaTests::scalatest()`) or MUnit suites in Scala files.
pub struct ScalaTests {
    framework: Framework,
}

impl ScalaTests {
    pub fn scalatest() -> Self {
        ScalaTests {
            framework: Framework::ScalaTest,
        }
    }

    pub fn munit() -> Self {
        ScalaTests {
            framework: Framework::MUnit,
        }
    }
}

/// The folder of the project a source file belongs to: `modules/api/src/test/scala/…` →
/// `modules/api`.
pub fn project_dir_of(path: &str) -> String {
    let path = path.replace('\\', "/");
    match path.find("src/") {
        Some(i) => path[..i].trim_end_matches('/').to_string(),
        None => String::new(),
    }
}

fn configuration(type_id: &str, name: &str, path: &str) -> Configuration {
    Configuration::new(name, type_id)
        .with_option("file", path)
        .with_option("projectDir", project_dir_of(path))
}

impl TestFrameworkProvider for ScalaTests {
    fn id(&self) -> &str {
        self.framework.type_id()
    }

    fn name(&self) -> &str {
        match self.framework {
            Framework::ScalaTest => "ScalaTest",
            Framework::MUnit => "MUnit",
        }
    }

    fn types(&self) -> Vec<String> {
        vec![self.framework.type_id().to_string()]
    }

    fn accepts(&self, path: &str) -> bool {
        path.ends_with(".scala")
    }

    fn discover(&self, path: &str, text: &str) -> Option<TestFile> {
        let file = parse(text);
        let suites: Vec<_> = suites_of(&file)
            .into_iter()
            .filter(|s| s.framework == self.framework)
            .collect();
        if suites.is_empty() {
            return None;
        }
        let path = path.replace('\\', "/");
        let type_id = self.framework.type_id();
        let mut targets = Vec::new();
        let mut whole = None;
        for suite in &suites {
            let cfg = configuration(type_id, &suite.name, &path)
                .with_option("scope", "class")
                .with_option("class", &suite.class);
            whole.get_or_insert_with(|| cfg.clone());
            targets.push(TestTarget {
                line: suite.line,
                kind: TargetKind::Class,
                label: suite.name.clone(),
                key: path.clone(),
                configuration: cfg,
            });
            for t in &suite.tests {
                targets.push(TestTarget {
                    line: t.line,
                    kind: if t.group {
                        TargetKind::Class
                    } else {
                        TargetKind::Method
                    },
                    label: t.label.clone(),
                    key: format!("{path}::{}", t.name),
                    configuration: configuration(
                        type_id,
                        &format!("{}: {}", suite.name, t.name),
                        &path,
                    )
                    .with_option("scope", "test")
                    .with_option("class", &suite.class)
                    .with_option("test", &t.name),
                });
            }
        }
        Some(TestFile {
            framework: Some(type_id.into()),
            targets,
            file: whole,
        })
    }

    /// Test filters select one name at a time: the failed tests' suites run again.
    fn rerun_failed(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
        let mut classes: Vec<String> = Vec::new();
        for f in failed {
            // Reported locations are `com/acme/CartSpec.scala` below a test root.
            let class = f
                .path
                .as_deref()
                .and_then(|p| {
                    let p = p.replace('\\', "/");
                    let rel = p.rsplit_once("/scala/").map(|(_, r)| r.to_string())?;
                    Some(rel.strip_suffix(".scala")?.replace('/', "."))
                })
                .or_else(|| Some(cfg.option("class").to_string()).filter(|c| !c.is_empty()));
            if let Some(c) = class {
                if !classes.contains(&c) {
                    classes.push(c);
                }
            }
        }
        if classes.is_empty() {
            return None;
        }
        let mut out = cfg.clone();
        out.options.insert("scope".into(), "pattern".into());
        out.options.insert("tests".into(), classes.join("\n"));
        Some(out)
    }

    fn command(
        &self,
        cfg: &Configuration,
        _ctx: &CommandContext,
    ) -> Option<Result<TestCommand, String>> {
        Some(sbt_test_command(cfg, self.framework))
    }
}

/// Quote an argument for sbt's command parser.
fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn sbt_test_command(cfg: &Configuration, framework: Framework) -> Result<TestCommand, String> {
    let program = match cfg.option("buildProgram").trim() {
        "" => "sbt".to_string(),
        p => p.to_string(),
    };
    let need = |key: &str, what: &str| {
        let v = cfg.option(key).trim();
        if v.is_empty() {
            Err(format!("{what} is not set"))
        } else {
            Ok(v.to_string())
        }
    };
    let task = match cfg.option("scope") {
        "" | "all" => "test".to_string(),
        "package" => format!("testOnly {}.*", need("package", "Package")?),
        "class" => format!("testOnly {}", need("class", "Suite")?),
        "test" => {
            let class = need("class", "Suite")?;
            let test = need("test", "Test")?;
            match framework {
                // ScalaTest: tests whose names contain the text.
                Framework::ScalaTest => format!("testOnly {class} -- -z {}", quoted(&test)),
                // MUnit: a glob over `Suite.test name`.
                Framework::MUnit => format!("testOnly {class} -- {}", quoted(&format!("*{test}"))),
            }
        }
        "pattern" => {
            let suites: Vec<&str> = cfg
                .option("tests")
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect();
            if suites.is_empty() {
                return Err("No suites are selected".into());
            }
            format!("testOnly {}", suites.join(" "))
        }
        other => return Err(format!("unknown test scope '{other}'")),
    };
    let project = cfg.option("project").trim().to_string();
    let dir = cfg
        .option("projectDir")
        .trim()
        .trim_end_matches('/')
        .to_string();
    let (program, args) = task_command(
        &program,
        &project,
        &[task],
        &[],
        &split_args(cfg.option("sbtOptions")),
    );
    let in_dir = |rel: &str| {
        if dir.is_empty() {
            rel.to_string()
        } else {
            format!("{dir}/{rel}")
        }
    };
    let scoped_file = matches!(cfg.option("scope"), "class" | "test");
    Ok(TestCommand {
        program,
        args,
        env: Default::default(),
        report: TestReport::JUnitFile,
        // Names keep their spaces (`A Cart should add items`).
        flavor: JUnitFlavor::Kotlin,
        location_rules: vec![LocationRule {
            pattern: r"^([\w$/]+)\.kt$".into(),
            replacement: format!("{}$1.scala", in_dir("src/test/scala/")),
        }],
        default_file: scoped_file
            .then(|| cfg.option("file").trim().to_string())
            .filter(|f| !f.is_empty()),
        coverage: None,
        report_dirs: vec![in_dir("target/test-reports")],
    })
}

#[cfg(test)]
mod unit {
    use super::*;

    const SPEC: &str = include_str!("../tests/fixtures/CartSpec.scala");
    const SUITE: &str = include_str!("../tests/fixtures/PriceSuite.scala");
    const PATH: &str = "modules/api/src/test/scala/com/acme/shop/CartSpec.scala";

    #[test]
    fn discovers_scalatest_and_munit() {
        let file = ScalaTests::scalatest().discover(PATH, SPEC).unwrap();
        assert_eq!(file.framework.as_deref(), Some("scalatest"));
        let suite = &file.targets[0];
        assert_eq!(
            suite.configuration.option("class"),
            "com.acme.shop.CartSpec"
        );
        assert_eq!(suite.configuration.option("projectDir"), "modules/api");
        let t = &file.targets[1];
        assert_eq!(t.label, "should add items");
        assert_eq!(t.key, format!("{PATH}::A Cart should add items"));
        assert_eq!(t.configuration.option("test"), "A Cart should add items");
        assert!(ScalaTests::munit().discover(PATH, SPEC).is_none());
        let m = ScalaTests::munit()
            .discover("src/test/scala/PriceSuite.scala", SUITE)
            .unwrap();
        assert_eq!(m.targets.len(), 3);
        assert_eq!(m.targets[2].configuration.type_id, "munit");
    }

    #[test]
    fn sbt_test_commands() {
        let file = ScalaTests::scalatest().discover(PATH, SPEC).unwrap();
        let cfg = file.targets[1]
            .configuration
            .clone()
            .with_option("project", "api")
            .with_option("buildProgram", "/usr/bin/sbt");
        let cmd = ScalaTests::scalatest()
            .command(&cfg, &CommandContext::default())
            .unwrap()
            .unwrap();
        assert_eq!(cmd.program, "/usr/bin/sbt");
        assert_eq!(
            cmd.args,
            ["api/testOnly com.acme.shop.CartSpec -- -z \"A Cart should add items\""]
        );
        assert_eq!(cmd.report_dirs, ["modules/api/target/test-reports"]);
        assert_eq!(
            cmd.location_rules[0].replacement,
            "modules/api/src/test/scala/$1.scala"
        );
        assert_eq!(cmd.default_file.as_deref(), Some(PATH));

        let m = ScalaTests::munit()
            .discover("src/test/scala/com/acme/shop/PriceSuite.scala", SUITE)
            .unwrap();
        let cmd = ScalaTests::munit()
            .command(&m.targets[1].configuration, &CommandContext::default())
            .unwrap()
            .unwrap();
        assert_eq!(cmd.program, "sbt");
        assert_eq!(
            cmd.args,
            ["testOnly com.acme.shop.PriceSuite -- \"*formats euros\""]
        );
        assert_eq!(cmd.report_dirs, ["target/test-reports"]);

        let all = Configuration::new("All", "munit").with_option("scope", "all");
        let cmd = ScalaTests::munit()
            .command(&all, &CommandContext::default())
            .unwrap()
            .unwrap();
        assert_eq!(cmd.args, ["test"]);

        let failed = [FailedTest {
            name: "A Cart should add items".into(),
            member: None,
            path: Some("modules/api/src/test/scala/com/acme/shop/CartSpec.scala".into()),
        }];
        let rerun = ScalaTests::scalatest().rerun_failed(&cfg, &failed).unwrap();
        assert_eq!(rerun.option("tests"), "com.acme.shop.CartSpec");
        let cmd = ScalaTests::scalatest()
            .command(&rerun, &CommandContext::default())
            .unwrap()
            .unwrap();
        assert_eq!(cmd.args, ["api/testOnly com.acme.shop.CartSpec"]);
    }
}
