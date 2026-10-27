//! Spock on the shared test runner (`forge-testing`), configuration type `spock`. Spock 2
//! runs on the JUnit Platform, so a configuration is a JUnit configuration of JavaForge
//! (`JavaTests`' options and command lines) with `language = groovy`: the build tool's test
//! filter selects a specification (`--tests com.acme.CartSpec`) or one feature (`--tests
//! 'com.acme.CartSpec.adds items'`), results come from the JUnit XML reports and point at
//! `src/test/groovy/…`.
//!
//! `spock` options: `scope` (`all`, `package`, `class`, `method`, `pattern`), `class`
//! (binary name), `method` (the feature name), `package`, `tests` (`Class#feature` per
//! line), plus the options `JavaTests` documents (`file`, `sourceRoot`, `module`,
//! `buildTool`, …) that the backend fills.

use forge_runtools::Configuration;
use forge_testing::{
    CommandContext, FailedTest, TargetKind, TestCommand, TestFile, TestFrameworkProvider,
    TestTarget,
};
use javaforge_lang_java::discovery::source_root;
use javaforge_lang_java::tests::{build_command, class_of, module_of_source_root};

use crate::source::parse;
use crate::spock::specs_of;

/// The Spock configuration type.
pub const SPOCK: &str = "spock";

/// Spock specifications in Groovy files.
pub struct SpockTests;

fn configuration(name: &str, path: &str, root: &str) -> Configuration {
    Configuration::new(name, SPOCK)
        .with_option("file", path)
        .with_option("sourceRoot", root)
        .with_option("module", module_of_source_root(root))
        .with_option("language", "groovy")
}

impl TestFrameworkProvider for SpockTests {
    fn id(&self) -> &str {
        SPOCK
    }

    fn name(&self) -> &str {
        "Spock"
    }

    fn types(&self) -> Vec<String> {
        vec![SPOCK.to_string()]
    }

    fn accepts(&self, path: &str) -> bool {
        path.ends_with(".groovy")
    }

    fn discover(&self, path: &str, text: &str) -> Option<TestFile> {
        let file = parse(text);
        let specs = specs_of(&file);
        if specs.is_empty() {
            return None;
        }
        let path = path.replace('\\', "/");
        let root = source_root(&path, file.package.as_deref());
        let mut targets = Vec::new();
        let mut whole = None;
        for spec in &specs {
            let cfg = configuration(&spec.name, &path, &root)
                .with_option("scope", "class")
                .with_option("class", &spec.class);
            whole.get_or_insert_with(|| cfg.clone());
            targets.push(TestTarget {
                line: spec.line,
                kind: TargetKind::Class,
                label: spec.name.clone(),
                key: path.clone(),
                configuration: cfg,
            });
            for f in &spec.features {
                targets.push(TestTarget {
                    line: f.line,
                    kind: TargetKind::Method,
                    label: f.name.clone(),
                    key: format!("{path}::{}", f.name),
                    configuration: configuration(
                        &format!("{}.{}", spec.name, f.name),
                        &path,
                        &root,
                    )
                    .with_option("scope", "method")
                    .with_option("class", &spec.class)
                    .with_option("method", &f.name),
                });
            }
        }
        Some(TestFile {
            framework: Some(SPOCK.into()),
            targets,
            file: whole,
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
            // Unrolled iterations (`totals 1 items [items: 1, #0]`) rerun their feature
            // through the class: Spock names iterations, not methods.
            let feature = f
                .member
                .clone()
                .filter(|m| !m.is_empty() && !m.contains('['))
                .unwrap_or_default();
            let line = if feature.is_empty() {
                class
            } else {
                format!("{class}#{feature}")
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
        let mut junit = cfg.clone();
        junit.type_id = "junit".into();
        junit.options.insert("language".into(), "groovy".into());
        Some(build_command(&junit, ctx))
    }

    fn supports_coverage(&self, _type_id: &str) -> bool {
        true
    }
}

#[cfg(test)]
mod unit {
    use super::*;
    use forge_output::JUnitFlavor;

    const PATH: &str = "src/test/groovy/com/acme/cart/CartSpec.groovy";

    fn prepared(cfg: Configuration) -> Configuration {
        cfg.with_option("buildTool", "gradle")
            .with_option("buildProgram", "/p/gradlew")
            .with_option("buildDir", "/p")
    }

    #[test]
    fn discovers_specs_and_features() {
        let src = include_str!("../tests/fixtures/CartSpec.groovy");
        let file = SpockTests.discover(PATH, src).unwrap();
        assert_eq!(file.framework.as_deref(), Some("spock"));
        let spec = &file.targets[0];
        assert_eq!((spec.line, spec.kind), (6, TargetKind::Class));
        assert_eq!(spec.configuration.option("class"), "com.acme.cart.CartSpec");
        assert_eq!(spec.configuration.option("sourceRoot"), "src/test/groovy/");
        assert_eq!(spec.configuration.option("module"), "");
        let adds = &file.targets[1];
        assert_eq!(adds.key, format!("{PATH}::adds items"));
        assert_eq!(adds.configuration.option("method"), "adds items");
        assert_eq!(adds.configuration.type_id, "spock");
        assert!(SpockTests.accepts("A.groovy") && !SpockTests.accepts("build.gradle"));
        assert!(SpockTests
            .discover("src/main/groovy/A.groovy", "class A {}")
            .is_none());
    }

    #[test]
    fn commands_run_through_the_build_with_groovy_locations() {
        let src = include_str!("../tests/fixtures/CartSpec.groovy");
        let file = SpockTests.discover(PATH, src).unwrap();
        let cmd = SpockTests
            .command(
                &prepared(file.targets[1].configuration.clone()),
                &CommandContext::default(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(cmd.program, "/p/gradlew");
        assert_eq!(
            cmd.args,
            [
                "cleanTest",
                "test",
                "--tests",
                "com.acme.cart.CartSpec.adds items"
            ]
        );
        assert_eq!(cmd.flavor, JUnitFlavor::Kotlin);
        assert_eq!(
            cmd.location_rules[0].replacement,
            "src/test/groovy/$1.groovy"
        );
        assert_eq!(cmd.report_dirs, ["build/test-results/test"]);
        assert_eq!(cmd.default_file.as_deref(), Some(PATH));

        let failed = [FailedTest {
            name: "adds items".into(),
            member: Some("adds items".into()),
            path: Some(PATH.into()),
        }];
        let rerun = SpockTests
            .rerun_failed(&file.targets[0].configuration, &failed)
            .unwrap();
        assert_eq!(rerun.option("tests"), "com.acme.cart.CartSpec#adds items");
    }
}
