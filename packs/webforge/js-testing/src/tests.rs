//! The runners as WebForge's manifest declares them: detection, discovery, command lines,
//! Rerun Failed Tests, coverage and debug programs.

use forge_output::TestReport;
use forge_runtools::Configuration;
use forge_testing::{
    CommandContext, CoverageFormat, FailedTest, TargetKind, TestFrameworkSpec, TestFrameworks,
};
use serde::Deserialize;

use crate::{JsTestRunner, Runner};

#[derive(Deserialize)]
struct Manifest {
    plugin: Vec<Plugin>,
}

#[derive(Deserialize)]
struct Plugin {
    #[serde(default)]
    contributions: Contributions,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Contributions {
    #[serde(default)]
    test_frameworks: Vec<TestFrameworkSpec>,
}

/// The test frameworks of WebForge's manifest.
fn specs() -> Vec<TestFrameworkSpec> {
    let text = include_str!("../../../../app/src-tauri/plugins/webforge.toml");
    let m: Manifest = toml::from_str(text).expect("webforge.toml");
    m.plugin
        .into_iter()
        .flat_map(|p| p.contributions.test_frameworks)
        .collect()
}

fn runner(id: &str) -> JsTestRunner {
    JsTestRunner::new(specs().into_iter().find(|s| s.id == id).expect(id)).expect(id)
}

fn registry() -> TestFrameworks {
    let mut r = TestFrameworks::new();
    let specs = specs();
    r.register_specs(&specs);
    crate::register(&mut r, specs);
    r
}

fn ctx() -> CommandContext {
    CommandContext {
        report_dir: "/tmp/r".into(),
        ..CommandContext::default()
    }
}

fn test_cfg(id: &str, file: &str, test: &str) -> Configuration {
    Configuration::new("t", id)
        .with_option("scope", "test")
        .with_option("file", file)
        .with_option("test", test)
}

#[test]
fn every_runner_is_declared() {
    let ids: Vec<String> = specs().into_iter().map(|s| s.id).collect();
    for r in Runner::ALL {
        assert!(ids.contains(&r.id().to_string()), "{}", r.id());
    }
}

#[test]
fn detects_runners_from_package_json_and_config_files() {
    let package = r#"{ "devDependencies": { "vitest": "^2.1.0", "@playwright/test": "^1.48.0" },
                       "scripts": { "test": "vitest" } }"#;
    let read = |p: &str| (p == "package.json").then(|| package.to_string());
    let files = vec!["package.json".to_string(), "src".into()];
    let detected: Vec<String> = specs()
        .iter()
        .filter(|s| s.detect(&files, &read))
        .map(|s| s.id.clone())
        .collect();
    assert_eq!(detected, ["playwright", "vitest"]);
    let jest_files = vec!["jest.config.ts".to_string()];
    let none = |_: &str| None;
    assert!(runner("jest").spec.detect(&jest_files, &none));
    assert!(runner("mocha").spec.detect(&[".mocharc.yml".into()], &none));
    assert!(runner("cypress")
        .spec
        .detect(&["cypress.config.ts".into()], &none));
    let node = |p: &str| {
        (p == "package.json").then(|| r#"{"scripts":{"test":"node --test"}}"#.to_string())
    };
    assert!(runner("node-test")
        .spec
        .detect(&["package.json".into()], &node));
}

const VITEST_FILE: &str = r#"import { describe, it, expect } from "vitest";
import { Cart } from "./cart";

describe("Cart", () => {
  it("adds items", () => {
    expect(new Cart().add("apple").count).toBe(1);
  });

  describe("totals", () => {
    it("rounds to cents", () => {
      expect(new Cart().total).toBe(0);
    });
  });
});
"#;

const PLAYWRIGHT_FILE: &str = r#"import { test, expect } from '@playwright/test';

test('shows the home page', async ({ page }) => {
  await page.goto('/');
});

test.describe('Checkout', () => {
  test('adds the voucher', async ({ page }) => {});
});
"#;

#[test]
fn discovers_tests_for_the_runner_the_project_uses() {
    let r = registry();
    let ids: Vec<String> = specs().into_iter().map(|s| s.id).collect();
    let file = r.discover_in_project("src/cart.test.ts", VITEST_FILE, &["jest".into()], &ids);
    // The file imports vitest: Jest leaves it alone even in a Jest project.
    assert_eq!(file.framework.as_deref(), Some("vitest"));
    let rows: Vec<_> = file
        .targets
        .iter()
        .map(|t| (t.line, t.kind, t.key.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            (3, TargetKind::Describe, ""),
            (4, TargetKind::Test, "src/cart.test.ts::Cart adds items"),
            (8, TargetKind::Describe, ""),
            (
                9,
                TargetKind::Test,
                "src/cart.test.ts::Cart totals rounds to cents"
            ),
        ]
    );
    let test = &file.targets[3].configuration;
    assert_eq!(
        (
            test.type_id.as_str(),
            test.option("scope"),
            test.option("test")
        ),
        ("vitest", "test", "Cart totals rounds to cents")
    );
    assert_eq!(file.file.unwrap().option("scope"), "file");

    // Without an import, project detection decides between Jest and Vitest.
    let globals = VITEST_FILE.replace("import { describe, it, expect } from \"vitest\";\n", "");
    let detected = |d: &[&str]| -> Option<String> {
        let d: Vec<String> = d.iter().map(|s| s.to_string()).collect();
        r.discover_in_project("src/cart.test.ts", &globals, &d, &ids)
            .framework
    };
    assert_eq!(detected(&["jest"]).as_deref(), Some("jest"));
    assert_eq!(
        detected(&["vitest", "playwright"]).as_deref(),
        Some("vitest")
    );

    let pw = r.discover_in_project(
        "tests/checkout.spec.ts",
        PLAYWRIGHT_FILE,
        &["vitest".into(), "playwright".into()],
        &ids,
    );
    assert_eq!(pw.framework.as_deref(), Some("playwright"));
    let labels: Vec<_> = pw.targets.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(
        labels,
        ["shows the home page", "Checkout", "adds the voucher"]
    );
    let cy = r.discover_in_project(
        "cypress/e2e/cart.cy.ts",
        "describe('Cart', () => { it('opens', () => { cy.visit('/') }) })",
        &["cypress".into(), "jest".into()],
        &ids,
    );
    assert_eq!(cy.framework.as_deref(), Some("cypress"));
    let node = r.discover_in_project(
        "test/math.test.mjs",
        "import { test } from 'node:test';\ntest('adds', () => {});\n",
        &[],
        &ids,
    );
    assert_eq!(node.framework.as_deref(), Some("node-test"));
}

#[test]
fn builds_node_commands_per_runner_and_scope() {
    let jest = runner("jest");
    let cmd = jest
        .build(
            &test_cfg("jest", "src/cart.test.ts", "Cart adds (1)"),
            &ctx(),
        )
        .unwrap();
    assert_eq!(cmd.program, "node");
    assert_eq!(
        cmd.args,
        [
            "node_modules/jest/bin/jest.js",
            "--json",
            "--outputFile=/tmp/r/jest.json",
            "--testLocationInResults",
            "src/cart.test.ts",
            "--testNamePattern=^Cart adds \\(1\\)"
        ]
    );
    assert_eq!(cmd.report, TestReport::JUnitFile);
    assert_eq!(cmd.default_file.as_deref(), Some("src/cart.test.ts"));

    let custom = test_cfg("jest", "a.test.js", "x")
        .with_option("executable", "/opt/node22/bin/node")
        .with_option("nodeOptions", "--max-old-space-size=4096")
        .with_option("package", "packages/web/node_modules/jest")
        .with_option("configFile", "jest.ci.config.ts");
    let cmd = jest.build(&custom, &ctx()).unwrap();
    assert_eq!(cmd.program, "/opt/node22/bin/node");
    assert_eq!(
        &cmd.args[..2],
        [
            "--max-old-space-size=4096",
            "packages/web/node_modules/jest/bin/jest.js"
        ]
    );
    assert_eq!(
        &cmd.args[cmd.args.len() - 2..],
        ["--config", "jest.ci.config.ts"]
    );

    let vitest = runner("vitest")
        .build(&Configuration::new("all", "vitest"), &ctx())
        .unwrap();
    assert_eq!(
        vitest.args,
        [
            "node_modules/vitest/vitest.mjs",
            "run",
            "--reporter=default",
            "--reporter=json",
            "--outputFile.json=/tmp/r/vitest.json"
        ]
    );

    let mocha = runner("mocha")
        .build(
            &Configuration::new("d", "mocha")
                .with_option("scope", "directory")
                .with_option("directory", "test"),
            &ctx(),
        )
        .unwrap();
    assert_eq!(
        mocha.args,
        [
            "node_modules/mocha/bin/mocha.js",
            "--reporter=tap",
            "test",
            "--recursive"
        ]
    );
    assert_eq!(mocha.report, TestReport::Tap);

    let pw = runner("playwright")
        .build(
            &test_cfg(
                "playwright",
                "tests/checkout.spec.ts",
                "Checkout adds the voucher",
            ),
            &ctx(),
        )
        .unwrap();
    assert_eq!(
        pw.args,
        [
            "node_modules/@playwright/test/cli.js",
            "test",
            "--reporter=list,json",
            "tests/checkout.spec.ts",
            "--grep=Checkout adds the voucher"
        ]
    );
    assert_eq!(
        pw.env["PLAYWRIGHT_JSON_OUTPUT_NAME"],
        "/tmp/r/playwright.json"
    );

    let cy = runner("cypress")
        .build(
            &Configuration::new("f", "cypress")
                .with_option("scope", "file")
                .with_option("file", "cypress/e2e/cart.cy.ts"),
            &ctx(),
        )
        .unwrap();
    assert_eq!(
        cy.args,
        [
            "node_modules/cypress/bin/cypress",
            "run",
            "--reporter",
            "junit",
            "--reporter-options",
            "mochaFile=/tmp/r/cypress-[hash].xml",
            "--spec",
            "cypress/e2e/cart.cy.ts"
        ]
    );

    let node = runner("node-test")
        .build(
            &test_cfg("node-test", "test/math.test.mjs", "math adds"),
            &ctx(),
        )
        .unwrap();
    assert_eq!(
        node.args,
        [
            "--test",
            "--test-reporter=tap",
            "--test-name-pattern=^math adds",
            "test/math.test.mjs"
        ]
    );
}

#[test]
fn coverage_writes_lcov() {
    let cov = CommandContext {
        coverage_file: Some("/tmp/c/coverage".into()),
        ..ctx()
    };
    let all = |id: &str| Configuration::new("all", id);
    let jest = runner("jest").build(&all("jest"), &cov).unwrap();
    assert!(jest.args.ends_with(&[
        "--coverage".to_string(),
        "--coverageReporters=lcov".into(),
        "--coverageDirectory=/tmp/c/coverage".into()
    ]));
    let c = jest.coverage.unwrap();
    assert_eq!(
        (c.file.as_str(), c.format),
        ("/tmp/c/coverage/lcov.info", CoverageFormat::Lcov)
    );
    let vitest = runner("vitest").build(&all("vitest"), &cov).unwrap();
    assert!(vitest
        .args
        .contains(&"--coverage.reportsDirectory=/tmp/c/coverage".to_string()));
    let mocha = runner("mocha").build(&all("mocha"), &cov).unwrap();
    assert_eq!(
        mocha.args,
        [
            "node_modules/c8/bin/c8.js",
            "--reporter=lcov",
            "--reports-dir=/tmp/c/coverage",
            "node",
            "node_modules/mocha/bin/mocha.js",
            "--reporter=tap"
        ]
    );
    assert_eq!(mocha.coverage.unwrap().file, "/tmp/c/coverage/lcov.info");
    let node = runner("node-test").build(&all("node-test"), &cov).unwrap();
    assert_eq!(
        node.args,
        [
            "--test",
            "--test-reporter=tap",
            "--experimental-test-coverage",
            "--test-reporter-destination=stdout",
            "--test-reporter=lcov",
            "--test-reporter-destination=/tmp/c/coverage.info"
        ]
    );
    let r = registry();
    assert!(r.supports_coverage("jest") && r.supports_coverage("mocha"));
    assert!(!r.supports_coverage("playwright") && !r.supports_coverage("cypress"));
}

#[test]
fn reruns_failed_tests_in_their_files() {
    let failed = [
        FailedTest {
            name: "rounds to cents".into(),
            path: Some("src/cart.test.ts".into()),
            ..FailedTest::default()
        },
        FailedTest {
            name: "applies (10%)".into(),
            path: Some("src/voucher.test.ts".into()),
            ..FailedTest::default()
        },
    ];
    let jest = runner("jest");
    let cfg = Configuration::new("All", "jest");
    let rerun = jest.rerun(&cfg, &failed).unwrap();
    assert_eq!(rerun.option("scope"), "pattern");
    let cmd = jest.build(&rerun, &ctx()).unwrap();
    assert_eq!(
        &cmd.args[4..],
        [
            "--testNamePattern=rounds to cents|applies \\(10%\\)",
            "src/cart.test.ts",
            "src/voucher.test.ts"
        ]
    );
    let mocha = runner("mocha");
    let tap = [FailedTest {
        name: "Api pings".into(),
        ..FailedTest::default()
    }];
    let rerun = mocha
        .rerun(&Configuration::new("All", "mocha"), &tap)
        .unwrap();
    assert_eq!(rerun.option("pattern"), "^(?:Api pings)$");
    // Cypress cannot filter names: the failed spec files run again.
    let cy = runner("cypress");
    let rerun = cy
        .rerun(&Configuration::new("All", "cypress"), &failed)
        .unwrap();
    let cmd = cy.build(&rerun, &ctx()).unwrap();
    assert_eq!(
        &cmd.args[cmd.args.len() - 2..],
        ["--spec", "src/cart.test.ts,src/voucher.test.ts"]
    );
    assert!(cy
        .rerun(&Configuration::new("All", "cypress"), &[])
        .is_none());
}

#[test]
fn debug_programs_run_in_one_process() {
    let jest = runner("jest");
    let p = jest
        .debug_program(
            &test_cfg("jest", "src/cart.test.ts", "Cart adds items")
                .with_option("nodeOptions", "--enable-source-maps"),
            &ctx(),
        )
        .unwrap();
    assert_eq!(p.runtime, "node");
    assert_eq!(p.runtime_args, ["--enable-source-maps"]);
    assert_eq!(p.program, "node_modules/jest/bin/jest.js");
    assert_eq!(&p.args[..2], ["--runInBand", "--testTimeout=999999"]);
    assert!(p
        .args
        .contains(&"--testNamePattern=^Cart adds items".to_string()));

    let vitest = runner("vitest")
        .debug_program(&test_cfg("vitest", "src/cart.test.ts", "Cart"), &ctx())
        .unwrap();
    assert_eq!(
        &vitest.args[..3],
        ["run", "--no-file-parallelism", "--test-timeout=0"]
    );
    let pw = runner("playwright")
        .debug_program(&test_cfg("playwright", "tests/a.spec.ts", "a"), &ctx())
        .unwrap();
    assert_eq!(&pw.args[..3], ["test", "--workers=1", "--timeout=0"]);
    assert!(pw.env.contains_key("PLAYWRIGHT_JSON_OUTPUT_NAME"));
    let node = runner("node-test")
        .debug_program(&test_cfg("node-test", "test/math.test.mjs", "adds"), &ctx())
        .unwrap();
    assert_eq!(node.program, "test/math.test.mjs");
    assert_eq!(
        node.runtime_args,
        ["--test", "--test-reporter=tap", "--test-name-pattern=^adds"]
    );
    assert!(runner("cypress")
        .debug_program(&Configuration::new("all", "cypress"), &ctx())
        .is_err());
}
