//! The shared test runner (`forge-testing`), coverage and run targets (`forge-runtargets`)
//! for the browser-mode mocks: discovery, Rerun Failed Tests and command lines of every
//! registered framework (the PHP frameworks plus the manifests' `testFrameworks`), coverage
//! reports with their summary and line marks, and "Run on" command lines, computed like
//! the desktop app.

use forge_output::links::LinkContext;
use forge_output::{LocationRule, OutputPathMapping};
use forge_runtargets::{Invocation, Mapper, RunTarget};
use forge_runtools::Configuration;
use std::collections::BTreeMap;

use forge_testing::coverage::{folders, project_hint, summary, CoverageReport};
use forge_testing::{CommandContext, FailedTest, TestFrameworkSpec, TestFrameworks};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

fn registry(specs: &[TestFrameworkSpec]) -> TestFrameworks {
    let mut r = TestFrameworks::new();
    r.register(Box::new(phpforge_test_frameworks::PhpTestFrameworks));
    r.register(Box::new(pyforge_testing::PythonTestFrameworks));
    r.register_specs(specs);
    webforge_js_testing::register(&mut r, specs.iter().cloned());
    goforge_lang_go::provider::register(&mut r, specs.iter().cloned());
    rustforge_lang_rust::provider::register(&mut r, specs.iter().cloned());
    rubyforge_lang_ruby::provider::register(&mut r, specs.iter().cloned());
    // JavaForge's JUnit and TestNG replace their manifest declarations, like in the app.
    r.register(Box::new(javaforge_lang_java::JavaTests::junit()));
    r.register(Box::new(javaforge_lang_java::JavaTests::testng()));
    r.register(Box::new(kotlinforge_lang_kotlin::KotlinTests));
    // CForge's GoogleTest, Catch2 and CTest replace their manifest declarations.
    cforge_lang_cpp::register(&mut r);
    r.register(Box::new(javaforge_lang_groovy::SpockTests));
    r.register(Box::new(javaforge_lang_scala::ScalaTests::scalatest()));
    r.register(Box::new(javaforge_lang_scala::ScalaTests::munit()));
    r
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscoverRequest {
    path: String,
    text: String,
    #[serde(default)]
    specs: Vec<TestFrameworkSpec>,
    /// Ids of the frameworks the project uses (`testing_detect`).
    #[serde(default)]
    detected: Vec<String>,
}

/// `{ path, text, specs, detected? }` → `TestFile` (`test_discover`, every framework,
/// those the project uses first).
#[wasm_bindgen]
pub fn testing_discover(request_json: &str) -> String {
    match parse::<DiscoverRequest>(request_json) {
        Ok(r) => {
            let ids: Vec<String> = r.specs.iter().map(|s| s.id.clone()).collect();
            to_json(&registry(&r.specs).discover_in_project(&r.path, &r.text, &r.detected, &ids))
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct RerunRequest {
    configuration: Configuration,
    failed: Vec<FailedTest>,
    #[serde(default)]
    specs: Vec<TestFrameworkSpec>,
}

/// `{ configuration, failed, specs }` → `Configuration | null` (`test_rerun_failed`).
#[wasm_bindgen]
pub fn testing_rerun_failed(request_json: &str) -> String {
    match parse::<RerunRequest>(request_json) {
        Ok(r) => to_json(&registry(&r.specs).rerun_failed(&r.configuration, &r.failed)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandRequest {
    configuration: Configuration,
    spec: TestFrameworkSpec,
    #[serde(default)]
    report_dir: String,
    #[serde(default)]
    coverage_file: Option<String>,
    /// The project's Go context for Go Test configurations.
    #[serde(default)]
    go_context: Option<serde_json::Value>,
    /// The project's C++ context (CMake targets, build folder) for GoogleTest, Catch2 and
    /// CTest configurations.
    #[serde(default)]
    cpp_context: Option<serde_json::Value>,
    /// The project's Cargo context for Cargo Test configurations.
    #[serde(default)]
    rust_context: Option<serde_json::Value>,
    /// The project's Ruby context for RSpec and Minitest configurations.
    #[serde(default)]
    ruby_context: Option<serde_json::Value>,
}

/// `{ configuration, spec, reportDir, coverageFile? }` → `TestCommand` of a manifest
/// framework.
#[wasm_bindgen]
pub fn testing_command(request_json: &str) -> String {
    match parse::<CommandRequest>(request_json) {
        Ok(r) => {
            let mut ctx = CommandContext {
                report_dir: r.report_dir,
                coverage_file: r.coverage_file,
                settings: Default::default(),
            };
            if let Some(go) = &r.go_context {
                ctx.settings
                    .insert(goforge_lang_go::CONTEXT_KEY.into(), go.to_string());
            }
            if let Some(cpp) = &r.cpp_context {
                ctx.settings
                    .insert(cforge_lang_cpp::CONTEXT_KEY.into(), cpp.to_string());
            }
            if let Some(rust) = &r.rust_context {
                ctx.settings
                    .insert(rustforge_lang_rust::CONTEXT_KEY.into(), rust.to_string());
            }
            if let Some(ruby) = &r.ruby_context {
                ctx.settings
                    .insert(rubyforge_lang_ruby::CONTEXT_KEY.into(), ruby.to_string());
            }
            let cpp = cforge_lang_cpp::CppTests::all()
                .into_iter()
                .find(|p| forge_testing::TestFrameworkProvider::id(p) == r.spec.id);
            // CForge's providers, Node.js test runners, Go Test and Cargo Test wrap their spec.
            let built = if let Some(cpp) = cpp {
                use forge_testing::TestFrameworkProvider;
                cpp.command(&r.configuration, &ctx)
                    .unwrap_or_else(|| Err("no command".into()))
            } else if let Some(js) = webforge_js_testing::JsTestRunner::new(r.spec.clone()) {
                js.build(&r.configuration, &ctx)
            } else if let Some(go) = goforge_lang_go::GoTestProvider::new(r.spec.clone()) {
                use forge_testing::TestFrameworkProvider;
                go.command(&r.configuration, &ctx)
                    .unwrap_or_else(|| Err("no command".into()))
            } else if let Some(rust) = rustforge_lang_rust::CargoTestProvider::new(r.spec.clone()) {
                use forge_testing::TestFrameworkProvider;
                rust.command(&r.configuration, &ctx)
                    .unwrap_or_else(|| Err("no command".into()))
            } else if let Some(ruby) = rubyforge_lang_ruby::RubyTests::new(r.spec.clone()) {
                use forge_testing::TestFrameworkProvider;
                ruby.command(&r.configuration, &ctx)
                    .unwrap_or_else(|| Err("no command".into()))
            } else {
                r.spec.build(&r.configuration, &ctx)
            };
            match built {
                Ok(c) => to_json(&c),
                Err(e) => error(e),
            }
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct DetectRequest {
    specs: Vec<TestFrameworkSpec>,
    files: Vec<String>,
    #[serde(default)]
    texts: std::collections::BTreeMap<String, String>,
}

/// `{ specs, files, texts }` → ids of the frameworks the project uses.
#[wasm_bindgen]
pub fn testing_detect(request_json: &str) -> String {
    match parse::<DetectRequest>(request_json) {
        Ok(r) => {
            let read = |p: &str| r.texts.get(p).cloned();
            let ids: Vec<&str> = r
                .specs
                .iter()
                .filter(|s| s.detect(&r.files, &read))
                .map(|s| s.id.as_str())
                .collect();
            to_json(&ids)
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoverageRequest {
    text: String,
    root: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    mappings: Vec<OutputPathMapping>,
    #[serde(default)]
    rules: Vec<LocationRule>,
    /// PHPUnit's XML coverage report (file name in the folder → text): the tests per line.
    #[serde(default)]
    tests_xml: BTreeMap<String, String>,
}

/// A report's suite fields: `{ report, format, lines, branches, methods, …Text, perTest }`.
fn suite_json(report: &CoverageReport, format: &str) -> serde_json::Value {
    let t = report.totals();
    json!({
        "report": report,
        "format": format,
        "lines": t.lines,
        "branches": t.branches,
        "methods": t.methods,
        "lineText": t.lines.text(),
        "branchText": t.branches.text(),
        "methodText": t.methods.text(),
        "perTest": report.has_tests(),
    })
}

/// `{ text, root, cwd?, mappings, rules, testsXml? }` → `{ report, format, lines, branches,
/// methods, … }`: a coverage report with project paths.
#[wasm_bindgen]
pub fn coverage_parse(request_json: &str) -> String {
    let r = match parse::<CoverageRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let (mut report, format) = match CoverageReport::parse_any(&r.text) {
        Ok(x) => x,
        Err(e) => return error(e),
    };
    if !r.tests_xml.is_empty() {
        let tests =
            forge_testing::coverage::phpunit_xml::collect(|name| r.tests_xml.get(name).cloned());
        for (path, lines) in tests.unwrap_or_default() {
            report.attach_tests(&path, &lines);
        }
    }
    let links = LinkContext::new(Some(&r.root), r.cwd.as_deref()).with_mappings(r.mappings);
    let rules: Vec<(regex_lite::Regex, String)> = r
        .rules
        .iter()
        .filter_map(|x| {
            regex_lite::Regex::new(&x.pattern)
                .ok()
                .map(|re| (re, x.replacement.clone()))
        })
        .collect();
    let report = report.resolve(|p| {
        let rewritten = rules
            .iter()
            .find(|(re, _)| re.is_match(p))
            .map(|(re, to)| re.replace(p, to.as_str()).into_owned());
        links.resolve(rewritten.as_deref().unwrap_or(p))
    });
    to_json(&suite_json(&report, format.label()))
}

#[derive(Deserialize)]
struct MergeRequest {
    reports: Vec<CoverageReport>,
}

/// `{ reports }` → the suite fields of their merged report (Show Coverage Data with several
/// suites).
#[wasm_bindgen]
pub fn coverage_merge(request_json: &str) -> String {
    match parse::<MergeRequest>(request_json) {
        Ok(r) => to_json(&suite_json(&CoverageReport::merged(r.reports), "Merged")),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct ReportRequest {
    report: CoverageReport,
}

/// `{ report }` → `{ path: hint }`: the Project view's hints of files and folders.
#[wasm_bindgen]
pub fn coverage_hints(request_json: &str) -> String {
    match parse::<ReportRequest>(request_json) {
        Ok(r) => {
            let mut out: BTreeMap<String, String> = folders(&r.report)
                .iter()
                .filter(|(p, _)| !p.is_empty())
                .filter_map(|(p, t)| project_hint(t, true).map(|h| (p.clone(), h)))
                .collect();
            for f in &r.report.files {
                if let Some(h) = project_hint(&f.totals(), false) {
                    out.insert(f.path.clone(), h);
                }
            }
            to_json(&out)
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct HtmlRequest {
    report: CoverageReport,
    project: String,
    title: String,
    generated: String,
    #[serde(default)]
    sources: BTreeMap<String, String>,
}

/// `{ report, project, title, generated, sources }` → the HTML coverage report.
#[wasm_bindgen]
pub fn coverage_html(request_json: &str) -> String {
    match parse::<HtmlRequest>(request_json) {
        Ok(r) => to_json(&forge_testing::coverage::html::render(
            &r.report,
            &r.project,
            &r.title,
            &r.generated,
            |p| r.sources.get(p).cloned(),
        )),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct NodeCoverageRequest {
    configuration: Configuration,
    node: String,
    dir: String,
    args: Vec<String>,
}

/// `{ configuration, node, dir, args }` → the arguments of a Node.js program under c8 or nyc
/// (Run with Coverage of a Node.js configuration).
#[wasm_bindgen]
pub fn node_coverage_wrap(request_json: &str) -> String {
    match parse::<NodeCoverageRequest>(request_json) {
        Ok(r) => to_json(&webforge_js_testing::coverage_wrap(
            &r.configuration,
            &r.node,
            &r.dir,
            r.args,
        )),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PhpCoverageRequest {
    configuration: Configuration,
    dir: String,
    /// Files relative to the working directory (behat.yml, phpspec.yml).
    #[serde(default)]
    files: BTreeMap<String, String>,
}

/// `{ configuration, dir, files }` → `{ interpreterArgs, args, env, file, inDir, testsDir }`:
/// how a PHP test configuration runs with coverage.
#[wasm_bindgen]
pub fn php_coverage_plan(request_json: &str) -> String {
    let r = match parse::<PhpCoverageRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    match phpforge_test_frameworks::provider::coverage_plan(&r.configuration, &r.dir, &|f| {
        r.files.get(f).cloned()
    }) {
        Ok(p) => to_json(&json!({
            "interpreterArgs": p.interpreter_args,
            "args": p.args,
            "env": p.env,
            "file": p.file,
            "inDir": p.in_dir,
            "testsDir": p.tests_dir,
        })),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct SummaryRequest {
    report: CoverageReport,
    project: String,
}

/// `{ report, project }` → `CoverageNode[]` (the Coverage tool window).
#[wasm_bindgen]
pub fn coverage_summary(request_json: &str) -> String {
    match parse::<SummaryRequest>(request_json) {
        Ok(r) => to_json(&summary(&r.report, &r.project)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct MarksRequest {
    report: CoverageReport,
    path: String,
}

/// `{ report, path }` → `LineMark[]` (the editor's coverage gutter).
#[wasm_bindgen]
pub fn coverage_marks(request_json: &str) -> String {
    match parse::<MarksRequest>(request_json) {
        Ok(r) => to_json(
            &r.report
                .file(&r.path)
                .map(|f| f.marks())
                .unwrap_or_default(),
        ),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WrapRequest {
    target: RunTarget,
    invocation: Invocation,
    root: String,
    /// A compose service's directory and volumes.
    #[serde(default)]
    compose: Option<(String, Vec<String>)>,
    #[serde(default)]
    docker: String,
}

/// `{ target, invocation, root, compose?, docker }` → `{ program, args, cwd, pathMappings,
/// debug }`: a command line on a run target.
#[wasm_bindgen]
pub fn run_target_wrap(request_json: &str) -> String {
    let r = match parse::<WrapRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let ctx = forge_runtargets::context(
        &r.target,
        &r.root,
        r.compose.as_ref().map(|(d, v)| (d.as_str(), v.as_slice())),
        &r.docker,
    );
    match forge_runtargets::wrap(&r.target, &r.invocation, &ctx) {
        Ok(w) => {
            let pairs: Vec<_> = Mapper::new(&ctx.root, &ctx.mappings)
                .pairs()
                .iter()
                .map(|(local, remote)| json!({ "local": local, "remote": remote }))
                .collect();
            to_json(&json!({
                "program": w.program,
                "args": w.args,
                "cwd": w.cwd,
                "pathMappings": pairs,
                "debug": forge_runtargets::debug_target(&r.target, &ctx.mappings),
            }))
        }
        Err(e) => error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const BUSTED: &str = r#"{"id":"busted","name":"busted","testFiles":["*_spec.lua"],"program":"busted","args":["--output=TAP"],"report":"tap","nameSeparator":" ","escape":"lua","discovery":[{"kind":"test","pattern":"^\\s*it\\(\"(?P<name>[^\"]+)\""}],"rerun":{"join":"\n"},"scopes":{"all":[],"file":["${file}"],"test":["${file}","--filter=${testPattern}"],"pattern":["--filter=${pattern}"]}}"#;

    #[test]
    fn discovers_runs_and_reruns_every_framework() {
        let out: Value = serde_json::from_str(&testing_discover(&format!(
            r#"{{"path":"spec/a_spec.lua","text":"it(\"works\", function() end)\n","specs":[{BUSTED}]}}"#
        )))
        .unwrap();
        assert_eq!(out["framework"], "busted");
        assert_eq!(out["targets"][0]["key"], "spec/a_spec.lua::works");
        let php: Value = serde_json::from_str(&testing_discover(
            r#"{"path":"features/a.feature","text":"Feature: A\n  Scenario: B\n"}"#,
        ))
        .unwrap();
        assert_eq!(php["framework"], "behat");
        let cmd: Value = serde_json::from_str(&testing_command(&format!(
            r#"{{"configuration":{{"name":"a","type":"busted","options":{{"scope":"file","file":"spec/a_spec.lua"}}}},"spec":{BUSTED}}}"#
        )))
        .unwrap();
        assert_eq!(cmd["args"], json!(["--output=TAP", "spec/a_spec.lua"]));
        assert_eq!(cmd["report"], "tap");
        let rerun: Value = serde_json::from_str(&testing_rerun_failed(&format!(
            r#"{{"configuration":{{"name":"a","type":"busted"}},"failed":[{{"name":"works"}}],"specs":[{BUSTED}]}}"#
        )))
        .unwrap();
        assert_eq!(rerun["options"]["pattern"], "works");
    }

    #[test]
    fn coverage_and_targets_through_json() {
        let parsed: Value = serde_json::from_str(&coverage_parse(
            r#"{"text":"SF:/p/src/a.js\nDA:1,1\nDA:2,0\nend_of_record\n","root":"/p"}"#,
        ))
        .unwrap();
        assert_eq!(parsed["format"], "LCOV");
        assert_eq!(parsed["lineText"], "50% (1/2)");
        let report = parsed["report"].to_string();
        let rows: Value = serde_json::from_str(&coverage_summary(&format!(
            r#"{{"report":{report},"project":"p"}}"#
        )))
        .unwrap();
        assert_eq!(rows[2]["path"], "src/a.js");
        let marks: Value = serde_json::from_str(&coverage_marks(&format!(
            r#"{{"report":{report},"path":"src/a.js"}}"#
        )))
        .unwrap();
        assert_eq!(marks[1]["status"], "uncovered");
        let merged: Value = serde_json::from_str(&coverage_merge(&format!(
            r#"{{"reports":[{report},{report}]}}"#
        )))
        .unwrap();
        assert_eq!(merged["format"], "Merged");
        assert_eq!(merged["lineText"], "50% (1/2)");
        let hints: Value =
            serde_json::from_str(&coverage_hints(&format!(r#"{{"report":{report}}}"#))).unwrap();
        assert_eq!(hints["src"], "100% files, 50% lines covered");
        let html: Value = serde_json::from_str(&coverage_html(&format!(
            r#"{{"report":{report},"project":"p","title":"All","generated":"now","sources":{{"src/a.js":"a();\nb();"}}}}"#
        )))
        .unwrap();
        assert!(html.as_str().unwrap().contains("<pre>b();</pre>"));
        // PHPUnit's per-test XML report next to the Clover report.
        let per_test: Value = serde_json::from_str(&coverage_parse(&serde_json::to_string(&json!({
            "text": "<coverage><project><file name=\"/p/src/A.php\"><line num=\"3\" type=\"stmt\" count=\"1\"/></file></project></coverage>",
            "root": "/p",
            "testsXml": {
                "index.xml": "<phpunit><project source=\"/p/src\"><directory name=\"/\"><file name=\"A.php\" href=\"A.php.xml\"/></directory></project></phpunit>",
                "A.php.xml": "<phpunit><file name=\"A.php\"><coverage><line nr=\"3\"><covered by=\"ATest::test_a\"/></line></coverage></file></phpunit>",
            },
        })).unwrap()))
        .unwrap();
        assert_eq!(per_test["perTest"], true);
        assert_eq!(
            per_test["report"]["files"][0]["lines"][0]["tests"][0],
            "ATest::test_a"
        );
        let plan: Value = serde_json::from_str(&php_coverage_plan(
            r#"{"configuration":{"name":"All","type":"phpunit","options":{"coverageDriver":"pcov"}},"dir":"/tmp/c"}"#,
        ))
        .unwrap();
        assert_eq!(plan["interpreterArgs"][0], "-dpcov.enabled=1");
        assert_eq!(plan["args"][1], "/tmp/c/clover.xml");
        let node: Value = serde_json::from_str(&node_coverage_wrap(
            r#"{"configuration":{"name":"server","type":"node"},"node":"node","dir":"/tmp/c","args":["src/a.js"]}"#,
        ))
        .unwrap();
        assert_eq!(node[0], "node_modules/c8/bin/c8.js");
        assert_eq!(node[4], "src/a.js");
        let w: Value = serde_json::from_str(&run_target_wrap(
            r#"{"target":{"kind":"dockerImage","image":"node:22"},"invocation":{"program":"node","args":["/p/a.js"],"cwd":"/p"},"root":"/p"}"#,
        ))
        .unwrap();
        assert_eq!(w["program"], "docker");
        assert_eq!(
            w["args"].as_array().unwrap().last().unwrap(),
            "/opt/project/a.js"
        );
        assert_eq!(w["pathMappings"][0]["remote"], "/opt/project");
        assert_eq!(w["debug"]["ideHost"], "host.docker.internal");
    }
}
