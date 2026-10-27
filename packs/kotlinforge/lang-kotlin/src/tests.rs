//! Kotlin tests on the shared test runner (`forge-testing`): Kotest specs (configuration
//! type `kotest`) and JUnit 5 / 4 or TestNG tests in Kotlin files (types `junit` /
//! `testng`, built by JavaForge's `JavaTests` with `language = kotlin`). Both run through
//! Gradle or Maven and report JUnit XML; Kotest's single tests are selected with Kotest's
//! own filter (`kotest_filter_tests`), its specs with the build tool's class filter.
//!
//! `kotest` options: `scope` (`all`, `package`, `class`, `test`, `pattern`), `class`
//! (binary name), `test` (the test path, `totals -- applies the discount`), `package`,
//! `tests` (classes, one per line), plus the options `JavaTests` documents (`file`,
//! `sourceRoot`, `module`, `buildTool`, …) that the backend fills.

use forge_output::JUnitFlavor;
use forge_runtools::Configuration;
use forge_testing::{
    CommandContext, FailedTest, TargetKind, TestCommand, TestFile, TestFrameworkProvider,
    TestTarget,
};
use javaforge_lang_java::discovery::source_root;
use javaforge_lang_java::tests::{build_command, class_of, module_of_source_root};

use crate::discovery::tests_of;
use crate::kotest::specs_of;
use crate::source::parse;

/// The Kotest configuration type.
pub const KOTEST: &str = "kotest";

/// Environment variables Kotest reads its test filter from.
pub const FILTER_SPECS_ENV: &str = "kotest_filter_specs";
pub const FILTER_TESTS_ENV: &str = "kotest_filter_tests";

/// Kotest specs and JUnit / TestNG tests in Kotlin files.
pub struct KotlinTests;

fn configuration(type_id: &str, name: &str, path: &str, root: &str) -> Configuration {
    Configuration::new(name, type_id)
        .with_option("file", path)
        .with_option("sourceRoot", root)
        .with_option("module", module_of_source_root(root))
        .with_option("language", "kotlin")
}

impl TestFrameworkProvider for KotlinTests {
    fn id(&self) -> &str {
        KOTEST
    }

    fn name(&self) -> &str {
        "Kotest"
    }

    fn types(&self) -> Vec<String> {
        vec![KOTEST.to_string()]
    }

    fn accepts(&self, path: &str) -> bool {
        path.ends_with(".kt")
    }

    fn discover(&self, path: &str, text: &str) -> Option<TestFile> {
        let file = parse(text);
        let path = path.replace('\\', "/");
        let root = source_root(&path, file.package.as_deref());
        let specs = specs_of(&file);
        let mut targets = Vec::new();
        let mut whole = None;
        if !specs.is_empty() {
            for spec in &specs {
                let cfg = configuration(KOTEST, &spec.name, &path, &root)
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
                for t in &spec.tests {
                    targets.push(TestTarget {
                        line: t.line,
                        kind: if t.container {
                            TargetKind::Class
                        } else {
                            TargetKind::Method
                        },
                        label: t.name.clone(),
                        key: format!("{path}::{}", t.name),
                        configuration: configuration(
                            KOTEST,
                            &format!("{}: {}", spec.name, t.name),
                            &path,
                            &root,
                        )
                        .with_option("scope", "test")
                        .with_option("class", &spec.class)
                        .with_option("test", t.filter_path()),
                    });
                }
            }
            return Some(TestFile {
                framework: Some(KOTEST.into()),
                targets,
                file: whole,
            });
        }
        let d = tests_of(&file)?;
        let type_id = d.framework.type_id();
        for class in &d.classes {
            let cfg = configuration(type_id, &class.name, &path, &root)
                .with_option("scope", "class")
                .with_option("class", &class.class);
            if !class.nested {
                whole.get_or_insert_with(|| cfg.clone());
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
            file: whole,
        })
    }

    /// Kotest filters select one test at a time: the failed tests' specs run again.
    fn rerun_failed(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
        let root = cfg.option("sourceRoot").to_string();
        let mut classes: Vec<String> = Vec::new();
        for f in failed {
            let class = f
                .path
                .as_deref()
                .and_then(|p| class_of(p, &root))
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
        ctx: &CommandContext,
    ) -> Option<Result<TestCommand, String>> {
        Some(kotest_command(cfg, ctx))
    }

    fn supports_coverage(&self, _type_id: &str) -> bool {
        true
    }
}

/// A Kotest configuration as the build tool's test filter (`JavaTests`' options) plus
/// Kotest's own filter for a single test.
fn kotest_command(cfg: &Configuration, ctx: &CommandContext) -> Result<TestCommand, String> {
    let mut junit = cfg.clone();
    junit.type_id = "junit".into();
    junit.options.insert("language".into(), "kotlin".into());
    let mut env = Vec::new();
    // Other scopes (a `tests` list of spec classes from Rerun Failed Tests too) are
    // JavaTests' own.
    if cfg.option("scope") == "test" {
        let test = cfg.option("test").trim();
        if test.is_empty() {
            return Err("Test is not set".into());
        }
        junit.options.insert("scope".into(), "class".into());
        env.push((
            FILTER_SPECS_ENV.to_string(),
            cfg.option("class").trim().to_string(),
        ));
        env.push((FILTER_TESTS_ENV.to_string(), test.to_string()));
    }
    let mut cmd = build_command(&junit, ctx)?;
    cmd.flavor = JUnitFlavor::Kotlin;
    cmd.env.extend(env);
    Ok(cmd)
}

#[cfg(test)]
mod unit {
    use super::*;

    const SPEC: &str = "package com.acme.cart\nimport io.kotest.core.spec.style.FunSpec\nclass CartSpec : FunSpec({\n  test(\"adds items\") { }\n  context(\"totals\") {\n    test(\"applies the discount\") { }\n  }\n})\n";
    const PATH: &str = "app/src/test/kotlin/com/acme/cart/CartSpec.kt";

    fn prepared(cfg: Configuration) -> Configuration {
        cfg.with_option("buildTool", "gradle")
            .with_option("buildProgram", "/p/gradlew")
            .with_option("buildDir", "/p")
    }

    #[test]
    fn discovers_kotest_targets() {
        let file = KotlinTests.discover(PATH, SPEC).unwrap();
        assert_eq!(file.framework.as_deref(), Some("kotest"));
        let labels: Vec<(&str, u32, &str)> = file
            .targets
            .iter()
            .map(|t| (t.label.as_str(), t.line, t.key.as_str()))
            .collect();
        assert_eq!(labels[0], ("CartSpec", 2, PATH));
        assert_eq!(labels[1].0, "adds items");
        assert_eq!(
            labels[3],
            (
                "applies the discount",
                5,
                "app/src/test/kotlin/com/acme/cart/CartSpec.kt::applies the discount"
            )
        );
        let leaf = &file.targets[3].configuration;
        assert_eq!(leaf.type_id, "kotest");
        assert_eq!(leaf.option("test"), "totals -- applies the discount");
        assert_eq!(leaf.option("module"), "app");
        assert_eq!(leaf.option("sourceRoot"), "app/src/test/kotlin/");
        assert_eq!(file.file.unwrap().option("scope"), "class");
        assert!(KotlinTests.accepts("A.kt") && !KotlinTests.accepts("build.gradle.kts"));
    }

    #[test]
    fn kotest_commands_filter_specs_and_tests() {
        let file = KotlinTests.discover(PATH, SPEC).unwrap();
        let cmd = KotlinTests
            .command(
                &prepared(file.targets[3].configuration.clone()),
                &CommandContext::default(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(cmd.program, "/p/gradlew");
        assert_eq!(
            cmd.args,
            [
                ":app:cleanTest",
                ":app:test",
                "--tests",
                "com.acme.cart.CartSpec"
            ]
        );
        assert_eq!(cmd.env[FILTER_TESTS_ENV], "totals -- applies the discount");
        assert_eq!(cmd.env[FILTER_SPECS_ENV], "com.acme.cart.CartSpec");
        assert_eq!(cmd.flavor, JUnitFlavor::Kotlin);
        assert_eq!(cmd.location_rules[0].replacement, "app/src/test/kotlin/$1");
        assert_eq!(cmd.report_dirs, ["app/build/test-results/test"]);

        let failed = [FailedTest {
            name: "applies the discount".into(),
            member: Some("applies the discount".into()),
            path: Some(PATH.into()),
        }];
        let rerun = KotlinTests
            .rerun_failed(&file.targets[0].configuration, &failed)
            .unwrap();
        assert_eq!(rerun.option("tests"), "com.acme.cart.CartSpec");
        let cmd = KotlinTests
            .command(&prepared(rerun), &CommandContext::default())
            .unwrap()
            .unwrap();
        assert!(cmd.args.contains(&"com.acme.cart.CartSpec".to_string()));
        assert!(cmd.env.is_empty());
    }

    #[test]
    fn junit_tests_in_kotlin_files_use_the_junit_type() {
        let src = "package com.acme\nimport org.junit.jupiter.api.Test\nclass CartTest {\n  @Test fun `adds stock`() {}\n}\n";
        let file = KotlinTests
            .discover("app/src/test/kotlin/com/acme/CartTest.kt", src)
            .unwrap();
        assert_eq!(file.framework.as_deref(), Some("junit"));
        let m = &file.targets[1];
        assert_eq!(
            m.key,
            "app/src/test/kotlin/com/acme/CartTest.kt::adds stock"
        );
        assert_eq!(m.configuration.type_id, "junit");
        assert_eq!(m.configuration.option("language"), "kotlin");
        assert_eq!(m.configuration.option("method"), "adds stock");
        assert!(KotlinTests.discover("A.kt", "class A").is_none());
    }
}
