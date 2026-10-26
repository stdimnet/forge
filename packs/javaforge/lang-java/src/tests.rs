//! JUnit (5, 4, 3) and TestNG for the shared test runner (`forge-testing`): gutter
//! discovery, Rerun Failed Tests and command lines through the build tool (Gradle
//! `cleanTest test --tests`, Maven Surefire `test -Dtest=`). Results come from the JUnit
//! XML reports the build writes (`build/test-results/test`, `target/surefire-reports`);
//! Run with Coverage adds JaCoCo's XML report.
//!
//! Configuration options (`junit` and `testng` types):
//!
//! - `scope`: `all`, `package`, `class`, `method` or `pattern` (Rerun Failed Tests);
//!   `package`, `class` (binary name), `method`, `tests` (`Class#method` per line);
//! - `file` (the test file), `sourceRoot` (its source root, `app/src/test/java/`), `module`
//!   (the module folder) and `testRunnerOptions` (extra build tool arguments);
//! - filled by the backend before the command is built: `buildTool` (`gradle` / `maven`),
//!   `buildProgram` (wrapper or executable), `buildDir` (absolute build folder), `buildRoot`
//!   (the build folder relative to the project, empty for the root), `gradlePath`
//!   (`:app`) and `offline`.

use forge_output::{JUnitFlavor, LocationRule, TestReport};
use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use forge_testing::{
    CommandContext, CoverageFormat, CoverageRequest, FailedTest, TargetKind, TestCommand, TestFile,
    TestFrameworkProvider, TestTarget,
};
use javaforge_build::command::ExecOptions;
use javaforge_build::{test_command, BuildProject, BuildTool, Module, TestFilter};

use crate::discovery::{discover_tests, source_root, Framework};

/// The module folder of a source root: `app/src/test/java/` → `app`.
pub fn module_of_source_root(root: &str) -> String {
    let root = root.trim_end_matches('/');
    match root.rfind("src/") {
        Some(i) => root[..i].trim_end_matches('/').to_string(),
        None => String::new(),
    }
}

/// JUnit or TestNG as a test framework provider.
pub struct JavaTests {
    junit: bool,
}

impl JavaTests {
    pub fn junit() -> Self {
        JavaTests { junit: true }
    }

    pub fn testng() -> Self {
        JavaTests { junit: false }
    }

    fn type_id(&self) -> &'static str {
        if self.junit {
            "junit"
        } else {
            "testng"
        }
    }

    fn accepts_framework(&self, f: Framework) -> bool {
        match f {
            Framework::TestNg => !self.junit,
            Framework::JUnit5 | Framework::JUnit4 => self.junit,
        }
    }
}

fn configuration(type_id: &str, name: &str, path: &str, root: &str) -> Configuration {
    Configuration::new(name, type_id)
        .with_option("file", path)
        .with_option("sourceRoot", root)
        .with_option("module", module_of_source_root(root))
}

/// The binary class name of a test file below its source root.
fn class_of(path: &str, root: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    let rel = if !root.is_empty() && path.starts_with(root) {
        &path[root.len()..]
    } else {
        // `…/java/com/acme/X.java`, else the file name.
        match path.rfind("/java/") {
            Some(i) => &path[i + 6..],
            None => path.rsplit('/').next().unwrap_or(&path),
        }
    };
    let rel = rel.strip_suffix(".java")?;
    Some(rel.replace('/', "."))
}

fn filter(cfg: &Configuration) -> Result<TestFilter, String> {
    let need = |key: &str, what: &str| {
        let v = cfg.option(key).trim();
        if v.is_empty() {
            Err(format!("{what} is not set"))
        } else {
            Ok(v.to_string())
        }
    };
    Ok(match cfg.option("scope") {
        "" | "all" => TestFilter::All,
        "package" => TestFilter::Package {
            name: need("package", "Package")?,
        },
        "class" => TestFilter::Class {
            name: need("class", "Class")?,
        },
        "method" => TestFilter::Method {
            class: need("class", "Class")?,
            method: need("method", "Method")?,
        },
        "pattern" => {
            let tests: Vec<(String, Option<String>)> = cfg
                .option("tests")
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(|l| match l.split_once('#') {
                    Some((c, m)) => (c.to_string(), Some(m.to_string())),
                    None => (l.to_string(), None),
                })
                .collect();
            if tests.is_empty() {
                return Err("No tests are selected".into());
            }
            TestFilter::Tests { tests }
        }
        other => return Err(format!("unknown test scope '{other}'")),
    })
}

/// A Java source root's files as project paths: `com/acme/X.java` → `<root>com/acme/X.java`.
fn source_rule(root: &str) -> LocationRule {
    LocationRule {
        pattern: r"^([\w$/]+\.java)$".into(),
        replacement: format!("{root}$1"),
    }
}

impl TestFrameworkProvider for JavaTests {
    fn id(&self) -> &str {
        self.type_id()
    }

    fn name(&self) -> &str {
        if self.junit {
            "JUnit"
        } else {
            "TestNG"
        }
    }

    fn types(&self) -> Vec<String> {
        vec![self.type_id().to_string()]
    }

    fn accepts(&self, path: &str) -> bool {
        path.ends_with(".java")
    }

    fn discover(&self, path: &str, text: &str) -> Option<TestFile> {
        let d = discover_tests(text)?;
        if !self.accepts_framework(d.framework) {
            return None;
        }
        let path = path.replace('\\', "/");
        let root = source_root(&path, d.package.as_deref());
        let type_id = self.type_id();
        let mut targets = Vec::new();
        let mut file = None;
        for class in &d.classes {
            let cfg = configuration(type_id, &class.name, &path, &root)
                .with_option("scope", "class")
                .with_option("class", &class.class);
            if file.is_none() && !class.nested {
                file = Some(cfg.clone());
            }
            targets.push(TestTarget {
                line: class.line,
                kind: TargetKind::Class,
                label: class.name.clone(),
                key: if class.nested {
                    String::new()
                } else {
                    path.clone()
                },
                configuration: cfg,
            });
            for m in &class.methods {
                targets.push(TestTarget {
                    line: m.line,
                    kind: TargetKind::Method,
                    label: m.name.clone(),
                    key: format!("{path}::{}", m.name),
                    configuration: configuration(
                        type_id,
                        &format!("{}.{}", class.name, m.name),
                        &path,
                        &root,
                    )
                    .with_option("scope", "method")
                    .with_option("class", &class.class)
                    .with_option("method", &m.name),
                });
            }
        }
        Some(TestFile {
            framework: Some(type_id.to_string()),
            targets,
            file,
        })
    }

    fn rerun_failed(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
        let root = cfg.option("sourceRoot").to_string();
        let mut lines: Vec<String> = Vec::new();
        for f in failed {
            let class = f
                .path
                .as_deref()
                .and_then(|p| class_of(p, &root))
                .or_else(|| Some(cfg.option("class").to_string()).filter(|c| !c.is_empty()));
            let Some(class) = class else { continue };
            let method = f
                .member
                .clone()
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| forge_output::junit::java_method(&f.name).to_string());
            let line = if method.is_empty() {
                class
            } else {
                format!("{class}#{method}")
            };
            if !lines.contains(&line) {
                lines.push(line);
            }
        }
        if lines.is_empty() {
            return None;
        }
        let mut out = cfg.clone();
        out.options.insert("scope".into(), "pattern".into());
        out.options.insert("tests".into(), lines.join("\n"));
        Some(out)
    }

    fn command(
        &self,
        cfg: &Configuration,
        ctx: &CommandContext,
    ) -> Option<Result<TestCommand, String>> {
        Some(build_command(cfg, ctx))
    }

    fn supports_coverage(&self, _type_id: &str) -> bool {
        true
    }
}

fn build_command(cfg: &Configuration, ctx: &CommandContext) -> Result<TestCommand, String> {
    let tool = BuildTool::parse(cfg.option("buildTool")).ok_or(
        "Tests run through Gradle or Maven: no build.gradle or pom.xml was found for this project",
    )?;
    let module_dir = cfg
        .option("module")
        .trim()
        .trim_end_matches('/')
        .to_string();
    let module = (!module_dir.is_empty()).then(|| Module {
        name: module_dir.clone(),
        path: match cfg.option("gradlePath").trim() {
            "" => format!(":{}", module_dir.replace('/', ":")),
            p => p.to_string(),
        },
        dir: module_dir.clone(),
    });
    let program = cfg.option("buildProgram").trim().to_string();
    let opts = ExecOptions {
        use_wrapper: false,
        executable: Some(program.clone()).filter(|p| !p.is_empty()),
        build_dir: cfg.option("buildDir").to_string(),
        windows: program.ends_with(".bat") || program.ends_with(".cmd"),
        offline: cfg.option("offline") == "true",
    };
    let project = BuildProject::minimal(tool, module.clone().into_iter().collect());
    let extra = split_args(cfg.option("testRunnerOptions"));
    let run = test_command(
        &project,
        &opts,
        module.as_ref(),
        &filter(cfg)?,
        ctx.coverage_file.is_some(),
        &extra,
    );
    let build_root = match cfg.option("buildRoot").trim().trim_end_matches('/') {
        "" => String::new(),
        r => format!("{r}/"),
    };
    let in_module = |set: &str| match &module {
        Some(m) => format!("{build_root}{}/src/{set}/java/", m.dir),
        None => format!("{build_root}src/{set}/java/"),
    };
    let test_root = match cfg.option("sourceRoot").trim() {
        "" => in_module("test"),
        r => format!("{}/", r.trim_end_matches('/')),
    };
    let scoped_file = matches!(cfg.option("scope"), "class" | "method");
    Ok(TestCommand {
        program: run.command.program,
        args: run.command.args,
        env: Default::default(),
        report: TestReport::JUnitFile,
        flavor: JUnitFlavor::Java,
        location_rules: vec![source_rule(&test_root)],
        default_file: scoped_file
            .then(|| cfg.option("file").trim().to_string())
            .filter(|f| !f.is_empty()),
        coverage: run.coverage_report.map(|file| CoverageRequest {
            file,
            format: CoverageFormat::Jacoco,
            location_rules: vec![source_rule(&in_module("main"))],
        }),
        report_dirs: run
            .report_dirs
            .iter()
            .map(|d| format!("{build_root}{d}"))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST: &str = "package com.acme.inventory;\nimport org.junit.jupiter.api.Test;\nclass InventoryTest {\n  @Test void addsStock() {}\n  @Test void removesStock() {}\n}\n";
    const PATH: &str = "app/src/test/java/com/acme/inventory/InventoryTest.java";

    fn prepared(cfg: Configuration, tool: &str) -> Configuration {
        cfg.with_option("buildTool", tool)
            .with_option("buildProgram", "/p/gradlew")
            .with_option("buildDir", "/p")
    }

    #[test]
    fn discovers_targets_with_configurations() {
        let file = JavaTests::junit().discover(PATH, TEST).unwrap();
        assert_eq!(file.framework.as_deref(), Some("junit"));
        assert_eq!(file.targets.len(), 3);
        let class = &file.targets[0];
        assert_eq!((class.line, class.kind), (2, TargetKind::Class));
        assert_eq!(class.key, PATH);
        assert_eq!(
            class.configuration.option("class"),
            "com.acme.inventory.InventoryTest"
        );
        assert_eq!(class.configuration.option("module"), "app");
        assert_eq!(
            class.configuration.option("sourceRoot"),
            "app/src/test/java/"
        );
        let m = &file.targets[1];
        assert_eq!(m.key, format!("{PATH}::addsStock"));
        assert_eq!(m.configuration.name, "InventoryTest.addsStock");
        assert_eq!(m.configuration.option("scope"), "method");
        assert_eq!(file.file.unwrap().option("scope"), "class");
        assert!(JavaTests::testng().discover(PATH, TEST).is_none());
        assert!(JavaTests::junit().accepts("A.java") && !JavaTests::junit().accepts("a.kt"));
    }

    #[test]
    fn gradle_method_command_with_reports_locations_and_coverage() {
        let file = JavaTests::junit().discover(PATH, TEST).unwrap();
        let cfg = prepared(file.targets[1].configuration.clone(), "gradle");
        let ctx = CommandContext {
            coverage_file: Some("/tmp/c".into()),
            ..Default::default()
        };
        let cmd = JavaTests::junit().command(&cfg, &ctx).unwrap().unwrap();
        assert_eq!(cmd.program, "/p/gradlew");
        assert_eq!(
            cmd.args,
            [
                ":app:cleanTest",
                ":app:test",
                ":app:jacocoTestReport",
                "--tests",
                "com.acme.inventory.InventoryTest.addsStock"
            ]
        );
        assert_eq!(cmd.report, TestReport::JUnitFile);
        assert_eq!(cmd.flavor, JUnitFlavor::Java);
        assert_eq!(cmd.report_dirs, ["app/build/test-results/test"]);
        assert_eq!(cmd.location_rules[0].replacement, "app/src/test/java/$1");
        assert_eq!(cmd.default_file.as_deref(), Some(PATH));
        let cov = cmd.coverage.unwrap();
        assert_eq!(
            cov.file,
            "app/build/reports/jacoco/test/jacocoTestReport.xml"
        );
        assert_eq!(cov.location_rules[0].replacement, "app/src/main/java/$1");
    }

    #[test]
    fn maven_rerun_failed_and_errors() {
        let file = JavaTests::junit().discover(PATH, TEST).unwrap();
        let class_cfg = file.targets[0].configuration.clone();
        let failed = [
            FailedTest {
                name: "addsStock()".into(),
                member: Some("addsStock".into()),
                path: Some(PATH.into()),
            },
            FailedTest {
                name: "removesStock()".into(),
                member: None,
                path: None,
            },
        ];
        let rerun = JavaTests::junit()
            .rerun_failed(&class_cfg, &failed)
            .unwrap();
        assert_eq!(rerun.option("scope"), "pattern");
        assert_eq!(
            rerun.option("tests"),
            "com.acme.inventory.InventoryTest#addsStock\ncom.acme.inventory.InventoryTest#removesStock"
        );
        let cfg = prepared(rerun, "maven").with_option("buildProgram", "/p/mvnw");
        let cmd = JavaTests::junit()
            .command(&cfg, &CommandContext::default())
            .unwrap()
            .unwrap();
        assert_eq!(
            cmd.args,
            [
                "-pl",
                "app",
                "-am",
                "test",
                "-Dtest=com.acme.inventory.InventoryTest#addsStock,com.acme.inventory.InventoryTest#removesStock",
                "-Dsurefire.failIfNoSpecifiedTests=false"
            ]
        );
        assert_eq!(cmd.report_dirs, ["app/target/surefire-reports"]);
        assert!(cmd.coverage.is_none());
        assert!(JavaTests::junit().rerun_failed(&class_cfg, &[]).is_none());
        let unprepared = JavaTests::junit()
            .command(&class_cfg, &CommandContext::default())
            .unwrap();
        assert!(unprepared.unwrap_err().contains("Gradle or Maven"));
        let no_class = prepared(
            Configuration::new("x", "junit").with_option("scope", "class"),
            "gradle",
        );
        assert_eq!(
            JavaTests::junit()
                .command(&no_class, &CommandContext::default())
                .unwrap()
                .unwrap_err(),
            "Class is not set"
        );
    }

    #[test]
    fn classes_of_paths() {
        assert_eq!(
            class_of("src/test/java/a/B.java", "src/test/java/").as_deref(),
            Some("a.B")
        );
        assert_eq!(class_of("x/java/a/B.java", "").as_deref(), Some("a.B"));
        assert_eq!(
            module_of_source_root("core/util/src/test/java/"),
            "core/util"
        );
        assert_eq!(module_of_source_root("src/test/java/"), "");
    }
}
