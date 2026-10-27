//! The C/C++ test frameworks of the shared test runner (`forge-testing`), as CLion runs
//! them:
//!
//! - **GoogleTest** (`gtest`): the test executable with `--gtest_filter` and
//!   `--gtest_output=xml:<report>` (JUnit XML with file and line of every test),
//! - **Catch2** (`catch2`): the test executable with a test spec, `-c <section>` and
//!   `--reporter junit --out <report>`,
//! - **CTest** (`ctest`): `ctest --test-dir <build> --output-junit <report>` with `-R`.
//!
//! The executable comes from the CMake model: the configuration's `target`, else the
//! executable target that compiles the test file. The backend passes the model, build folder
//! and tools as a [`CppContext`] (`CommandContext.settings["cpp.context"]`). Run with
//! Coverage adds Clang's `LLVM_PROFILE_FILE` and asks the backend's collector for an LCOV
//! report (gcov or llvm-cov, see [`crate::coverage`]).
//!
//! Configuration options: `scope` (`all`, `file`, `suite`, `test`, `pattern`), `file`,
//! `suite`, `test`, `kind` (`test`, `parameterized`, `typed`, `section`), `suites` (the
//! file's suites or test cases, `|` separated), `section` (Catch2 section path, `/`
//! separated), `filter` (a GoogleTest filter, Catch2 test spec or CTest regex), `target`,
//! `executable`, `label` and `jobs` (CTest).

use std::collections::BTreeMap;

use forge_output::{JUnitFlavor, TestReport};
use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use forge_testing::{
    CommandContext, CoverageCollect, CoverageFormat, CoverageRequest, FailedTest, TargetKind,
    TestCommand, TestFile, TestFrameworkProvider, TestTarget,
};
use serde::{Deserialize, Serialize};

use crate::discovery::{discover, CppTest, Framework, Kind};

pub const GTEST: &str = "gtest";
pub const CATCH2: &str = "catch2";
pub const CTEST: &str = "ctest";
/// The key of the [`CppContext`] in `CommandContext.settings`.
pub const CONTEXT_KEY: &str = "cpp.context";

/// An executable target as the test frameworks need it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextTarget {
    pub name: String,
    /// Absolute.
    pub executable: String,
    /// Project-relative sources.
    pub sources: Vec<String>,
}

/// What the backend knows about the project's build.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CppContext {
    /// Absolute build folder of the active profile.
    pub build_dir: String,
    pub build_type: String,
    pub multi_config: bool,
    pub ctest: String,
    pub targets: Vec<ContextTarget>,
    /// `gcov`, `llvm-profdata`, `llvm-cov` of the toolchain.
    pub coverage_tools: BTreeMap<String, String>,
}

impl CppContext {
    fn from(ctx: &CommandContext) -> Result<Self, String> {
        ctx.settings
            .get(CONTEXT_KEY)
            .map(|s| serde_json::from_str(s).map_err(|e| format!("bad C++ context: {e}")))
            .transpose()
            .map(Option::unwrap_or_default)
    }

    /// The executable of a configuration: its `executable`, its `target`'s, or the target
    /// that compiles its `file`.
    pub fn executable(&self, cfg: &Configuration) -> Result<String, String> {
        let exe = cfg.option("executable").trim();
        if !exe.is_empty() {
            return Ok(exe.to_string());
        }
        let target = cfg.option("target").trim();
        if !target.is_empty() {
            return self
                .targets
                .iter()
                .find(|t| t.name == target)
                .map(|t| t.executable.clone())
                .ok_or_else(|| {
                    format!("No executable target '{target}': reload the CMake project")
                });
        }
        let file = cfg.option("file").replace('\\', "/");
        if file.is_empty() {
            return Err("Choose the test target".into());
        }
        self.targets
            .iter()
            .find(|t| t.sources.contains(&file))
            .map(|t| t.executable.clone())
            .ok_or_else(|| {
                format!(
                    "No CMake target compiles {file}: reload the CMake project or choose a target"
                )
            })
    }

    /// The target name of a configuration (for building it first).
    pub fn target_name(&self, cfg: &Configuration) -> Option<String> {
        let target = cfg.option("target").trim();
        if !target.is_empty() {
            return Some(target.to_string());
        }
        let file = cfg.option("file").replace('\\', "/");
        self.targets
            .iter()
            .find(|t| t.sources.contains(&file))
            .map(|t| t.name.clone())
    }
}

/// One of the three frameworks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CppTests {
    framework: &'static str,
}

impl CppTests {
    pub fn gtest() -> Self {
        Self { framework: GTEST }
    }
    pub fn catch2() -> Self {
        Self { framework: CATCH2 }
    }
    pub fn ctest() -> Self {
        Self { framework: CTEST }
    }
    pub fn all() -> [Self; 3] {
        [Self::gtest(), Self::catch2(), Self::ctest()]
    }
}

/// Register the three providers (they replace the manifest's declarations of the same ids).
pub fn register(registry: &mut forge_testing::TestFrameworks) {
    for p in CppTests::all() {
        registry.register(Box::new(p));
    }
}

/// C and C++ sources and headers.
pub fn is_cpp_source(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "c" | "cc"
            | "cpp"
            | "cxx"
            | "c++"
            | "cu"
            | "h"
            | "hh"
            | "hpp"
            | "hxx"
            | "ipp"
            | "inl"
            | "ixx"
            | "cppm"
    )
}

/// `\`, `,`, `[`, `]`, `"` and `*` are special in Catch2 test specs.
pub fn catch2_escape(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if matches!(c, '\\' | ',' | '[' | ']' | '"' | '*') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A CTest name as an anchored regular expression.
pub fn regex_escape(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if "\\^$.|?*+()[]{}".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The `--gtest_filter` of one test.
pub fn gtest_test_filter(suite: &str, test: &str, kind: &str) -> String {
    match kind {
        "parameterized" => format!("*/{suite}.{test}/*"),
        "typed" => format!("{suite}/*.{test}"),
        _ => format!("{suite}.{test}"),
    }
}

/// The filter of a whole suite (plain, parameterized and typed instantiations).
pub fn gtest_suite_filter(suite: &str) -> String {
    format!("{suite}.*:*/{suite}.*:{suite}/*.*")
}

fn kind_option(k: Kind) -> &'static str {
    match k {
        Kind::Parameterized => "parameterized",
        Kind::Typed => "typed",
        Kind::Section => "section",
        Kind::Test => "test",
    }
}

fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

impl CppTests {
    fn discover_file(&self, path: &str, text: &str) -> Option<TestFile> {
        let found = discover(text);
        let want = match self.framework {
            GTEST => Framework::GoogleTest,
            CATCH2 => Framework::Catch2,
            _ => return None,
        };
        if found.framework != Some(want) || found.tests.is_empty() {
            return None;
        }
        let path = path.replace('\\', "/");
        let base = |name: String| {
            let mut c = Configuration::new(name, self.framework);
            c.options.insert("file".into(), path.clone());
            c
        };
        let mut targets = Vec::new();
        let mut groups: Vec<String> = Vec::new();
        let tests: Vec<&CppTest> = found.tests.iter().filter(|t| t.framework == want).collect();
        for t in &tests {
            let mut c;
            let (label, key);
            match want {
                Framework::GoogleTest => {
                    if !groups.contains(&t.suite) {
                        groups.push(t.suite.clone());
                    }
                    c = base(format!("{}.{}", t.suite, t.name));
                    c.options.insert("scope".into(), "test".into());
                    c.options.insert("suite".into(), t.suite.clone());
                    c.options.insert("test".into(), t.name.clone());
                    c.options.insert("kind".into(), kind_option(t.kind).into());
                    label = format!("{}.{}", t.suite, t.name);
                    key = format!("{path}::{}", t.name);
                }
                Framework::Catch2 => {
                    let case = t.path.first().cloned().unwrap_or_default();
                    if t.kind != Kind::Section && !groups.contains(&case) {
                        groups.push(case.clone());
                    }
                    c = base(t.path.join(" / "));
                    c.options.insert("scope".into(), "test".into());
                    c.options.insert("test".into(), case);
                    c.options.insert("kind".into(), kind_option(t.kind).into());
                    if t.path.len() > 1 {
                        c.options.insert("section".into(), t.path[1..].join("/"));
                    }
                    label = t.path.join(" / ");
                    key = format!("{path}::{}", t.path.join("/"));
                }
            }
            targets.push(TestTarget {
                line: t.line,
                kind: if t.kind == Kind::Section {
                    TargetKind::Describe
                } else {
                    TargetKind::Test
                },
                label,
                key,
                configuration: c,
            });
        }
        let mut file = base(format!("{} tests in {}", self.name(), base_name(&path)));
        file.options.insert("scope".into(), "file".into());
        file.options.insert("suites".into(), groups.join("|"));
        Some(TestFile {
            framework: Some(self.framework.to_string()),
            targets,
            file: Some(file),
        })
    }

    fn build(&self, cfg: &Configuration, ctx: &CommandContext) -> Result<TestCommand, String> {
        let cpp = CppContext::from(ctx)?;
        let opt = |k: &str| cfg.option(k).trim().to_string();
        let scope = match opt("scope") {
            s if s.is_empty() => "all".to_string(),
            s => s,
        };
        // The form's pattern field, or the filter field of the framework.
        let pattern = || match opt("pattern") {
            p if p.is_empty() => opt("filter"),
            p => p,
        };
        let report_dir = ctx.report_dir.trim_end_matches(['/', '\\']).to_string();
        let mut env = BTreeMap::new();
        let mut args: Vec<String> = Vec::new();
        let (program, default_file, binaries) = match self.framework {
            GTEST => {
                let exe = cpp.executable(cfg)?;
                let filter = match scope.as_str() {
                    "all" => Some(opt("filter")),
                    "file" => {
                        let suites: Vec<String> = opt("suites")
                            .split('|')
                            .filter(|s| !s.is_empty())
                            .map(gtest_suite_filter)
                            .collect();
                        (!suites.is_empty()).then(|| suites.join(":"))
                    }
                    "suite" => Some(gtest_suite_filter(&opt("suite"))),
                    // A test typed into the form is `Suite.Test`.
                    "test" if opt("suite").is_empty() => Some(opt("test")),
                    "test" => Some(gtest_test_filter(&opt("suite"), &opt("test"), &opt("kind"))),
                    "pattern" => Some(pattern()),
                    other => return Err(format!("Unknown test scope '{other}'")),
                };
                let file = format!("{report_dir}/gtest-report.xml");
                args.push(format!("--gtest_output=xml:{file}"));
                if let Some(f) = filter.filter(|f| !f.is_empty()) {
                    args.push(format!("--gtest_filter={f}"));
                }
                (exe.clone(), None, vec![exe])
            }
            CATCH2 => {
                let exe = cpp.executable(cfg)?;
                match scope.as_str() {
                    "all" if !opt("filter").is_empty() => args.push(opt("filter")),
                    "all" => {}
                    "file" => {
                        let cases: Vec<String> = opt("suites")
                            .split('|')
                            .filter(|s| !s.is_empty())
                            .map(catch2_escape)
                            .collect();
                        if !cases.is_empty() {
                            args.push(cases.join(","));
                        }
                    }
                    "suite" | "test" => {
                        args.push(catch2_escape(&opt("test")));
                        for s in opt("section").split('/').filter(|s| !s.is_empty()) {
                            args.push("-c".into());
                            args.push(s.to_string());
                        }
                    }
                    "pattern" => args.push(pattern()),
                    other => return Err(format!("Unknown test scope '{other}'")),
                }
                let file = format!("{report_dir}/catch2-report.xml");
                args.extend([
                    "--reporter".into(),
                    "junit".into(),
                    "--out".into(),
                    file.clone(),
                ]);
                let default = Some(opt("file")).filter(|f| !f.is_empty());
                (exe.clone(), default, vec![exe])
            }
            _ => {
                if cpp.build_dir.is_empty() {
                    return Err(
                        "The project has no CMake build folder: reload the CMake project".into(),
                    );
                }
                let ctest = if cpp.ctest.is_empty() {
                    "ctest".to_string()
                } else {
                    cpp.ctest.clone()
                };
                let file = format!("{report_dir}/ctest-report.xml");
                args.extend([
                    "--test-dir".into(),
                    cpp.build_dir.clone(),
                    "--output-on-failure".into(),
                    "--output-junit".into(),
                    file.clone(),
                ]);
                if cpp.multi_config && !cpp.build_type.is_empty() {
                    args.extend(["-C".into(), cpp.build_type.clone()]);
                }
                match scope.as_str() {
                    "all" | "file" if !opt("filter").is_empty() => {
                        args.extend(["-R".into(), opt("filter")])
                    }
                    "all" | "file" => {}
                    "test" => {
                        args.extend(["-R".into(), format!("^{}$", regex_escape(&opt("test")))])
                    }
                    "pattern" => args.extend(["-R".into(), pattern()]),
                    other => return Err(format!("Unknown test scope '{other}'")),
                }
                if !opt("label").is_empty() {
                    args.extend(["-L".into(), opt("label")]);
                }
                if !opt("jobs").is_empty() {
                    args.extend(["-j".into(), opt("jobs")]);
                }
                let binaries = cpp.targets.iter().map(|t| t.executable.clone()).collect();
                (ctest, None, binaries)
            }
        };
        args.extend(split_args(&cfg.arguments));
        let coverage = ctx.coverage_file.as_ref().map(|file| {
            let dir = file
                .rsplit_once(['/', '\\'])
                .map(|(d, _)| d.to_string())
                .unwrap_or_default();
            let (k, v) = crate::coverage::llvm_profile_env(&dir);
            env.insert(k, v);
            CoverageRequest {
                file: file.clone(),
                format: CoverageFormat::Lcov,
                location_rules: Vec::new(),
                collect: Some(CoverageCollect {
                    kind: "cpp".into(),
                    dirs: vec![cpp.build_dir.clone()]
                        .into_iter()
                        .filter(|d| !d.is_empty())
                        .collect(),
                    binaries,
                    tools: cpp.coverage_tools.clone(),
                }),
            }
        });
        Ok(TestCommand {
            program,
            args,
            env,
            report: TestReport::JUnitFile,
            flavor: JUnitFlavor::Generic,
            location_rules: Vec::new(),
            default_file,
            coverage,
            report_dirs: Vec::new(),
        })
    }
}

impl TestFrameworkProvider for CppTests {
    fn id(&self) -> &str {
        self.framework
    }

    fn name(&self) -> &str {
        match self.framework {
            GTEST => "Google Test",
            CATCH2 => "Catch2",
            _ => "CTest",
        }
    }

    fn types(&self) -> Vec<String> {
        vec![self.framework.to_string()]
    }

    fn accepts(&self, path: &str) -> bool {
        self.framework != CTEST && is_cpp_source(path)
    }

    fn discover(&self, path: &str, text: &str) -> Option<TestFile> {
        self.discover_file(path, text)
    }

    fn rerun_failed(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
        if failed.is_empty() {
            return None;
        }
        let mut c = cfg.clone();
        c.name = format!("Failed tests of {}", cfg.name);
        let names: Vec<&str> = failed.iter().map(|f| f.name.as_str()).collect();
        let filter = match self.framework {
            // Test case names of the XML report (`AreaOfSquare`, `KeepsRatio/0`).
            GTEST => names
                .iter()
                .map(|n| format!("*.{n}"))
                .collect::<Vec<_>>()
                .join(":"),
            // Sections are reported as `case/section`: rerun their test cases.
            CATCH2 => {
                let mut cases: Vec<String> = Vec::new();
                for n in &names {
                    let case = catch2_escape(n.split('/').next().unwrap_or(n));
                    if !cases.contains(&case) {
                        cases.push(case);
                    }
                }
                cases.join(",")
            }
            _ => format!(
                "^({})$",
                names
                    .iter()
                    .map(|n| regex_escape(n))
                    .collect::<Vec<_>>()
                    .join("|")
            ),
        };
        c.options.insert("scope".into(), "pattern".into());
        c.options.insert("pattern".into(), filter);
        Some(c)
    }

    fn command(
        &self,
        cfg: &Configuration,
        ctx: &CommandContext,
    ) -> Option<Result<TestCommand, String>> {
        Some(self.build(cfg, ctx))
    }

    fn supports_coverage(&self, _type_id: &str) -> bool {
        true
    }
}

/// The program and arguments that debug a test configuration (the test executable with the
/// filter, without report files).
pub fn debug_command(
    cfg: &Configuration,
    ctx: &CppContext,
) -> Result<(String, Vec<String>), String> {
    let mut command_ctx = CommandContext {
        report_dir: String::new(),
        ..CommandContext::default()
    };
    command_ctx.settings.insert(
        CONTEXT_KEY.into(),
        serde_json::to_string(ctx).map_err(|e| e.to_string())?,
    );
    let provider = match cfg.type_id.as_str() {
        GTEST => CppTests::gtest(),
        CATCH2 => CppTests::catch2(),
        other => {
            return Err(format!(
                "'{other}' configurations are debugged through their executable"
            ))
        }
    };
    let cmd = provider.build(cfg, &command_ctx)?;
    let args = cmd
        .args
        .into_iter()
        .filter(|a| !a.starts_with("--gtest_output="))
        .collect::<Vec<_>>();
    // Catch2's reporter arguments come last: drop them.
    let args = match args.iter().position(|a| a == "--reporter") {
        Some(i) if provider.framework == CATCH2 => {
            let mut a = args.clone();
            a.drain(i..(i + 4).min(a.len()));
            a
        }
        _ => args,
    };
    Ok((cmd.program, args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_testing::TestFrameworks;

    fn ctx(coverage: bool) -> CommandContext {
        let cpp = CppContext {
            build_dir: "/home/dev/shapes/cmake-build-debug".into(),
            build_type: "Debug".into(),
            multi_config: false,
            ctest: "/usr/bin/ctest".into(),
            targets: vec![
                ContextTarget {
                    name: "shapes_tests".into(),
                    executable: "/home/dev/shapes/cmake-build-debug/shapes_tests".into(),
                    sources: vec!["tests/shapes_test.cpp".into()],
                },
                ContextTarget {
                    name: "stats_tests".into(),
                    executable: "/home/dev/shapes/cmake-build-debug/stats_tests".into(),
                    sources: vec!["tests/stats_test.cpp".into()],
                },
            ],
            coverage_tools: [("gcov".to_string(), "/usr/bin/gcov".to_string())].into(),
        };
        let mut c = CommandContext {
            report_dir: "/tmp/forge-run-1".into(),
            coverage_file: coverage.then(|| "/tmp/forge-run-1/coverage/coverage".into()),
            ..CommandContext::default()
        };
        c.settings
            .insert(CONTEXT_KEY.into(), serde_json::to_string(&cpp).unwrap());
        c
    }

    const GTEST_FILE: &str = "#include <gtest/gtest.h>\nTEST_F(PolygonTest, AreaOfSquare) {}\nTEST_P(ScaleTest, KeepsRatio) {}\n";
    const CATCH_FILE: &str = "#include <catch2/catch_test_macros.hpp>\nTEST_CASE(\"median\") {\n  SECTION(\"even, count\") {}\n}\n";

    fn registry() -> TestFrameworks {
        let mut r = TestFrameworks::new();
        register(&mut r);
        r
    }

    #[test]
    fn googletest_gutter_and_commands() {
        let r = registry();
        let file = r.discover("tests/shapes_test.cpp", GTEST_FILE, None);
        assert_eq!(file.framework.as_deref(), Some("gtest"));
        let rows: Vec<(u32, &str, &str)> = file
            .targets
            .iter()
            .map(|t| (t.line, t.label.as_str(), t.key.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                (
                    1,
                    "PolygonTest.AreaOfSquare",
                    "tests/shapes_test.cpp::AreaOfSquare"
                ),
                (
                    2,
                    "ScaleTest.KeepsRatio",
                    "tests/shapes_test.cpp::KeepsRatio"
                ),
            ]
        );
        let test = r
            .command(&file.targets[1].configuration, &ctx(false))
            .unwrap()
            .unwrap();
        assert_eq!(
            test.program,
            "/home/dev/shapes/cmake-build-debug/shapes_tests"
        );
        assert_eq!(
            test.args,
            [
                "--gtest_output=xml:/tmp/forge-run-1/gtest-report.xml",
                "--gtest_filter=*/ScaleTest.KeepsRatio/*"
            ]
        );
        assert_eq!(test.report, TestReport::JUnitFile);
        let whole = r
            .command(file.file.as_ref().unwrap(), &ctx(false))
            .unwrap()
            .unwrap();
        assert_eq!(
            whole.args[1],
            "--gtest_filter=PolygonTest.*:*/PolygonTest.*:PolygonTest/*.*:ScaleTest.*:*/ScaleTest.*:ScaleTest/*.*"
        );
        let rerun = r
            .rerun_failed(
                &file.targets[0].configuration,
                &[FailedTest {
                    name: "PerimeterOfSquare".into(),
                    ..FailedTest::default()
                }],
            )
            .unwrap();
        assert_eq!(rerun.options["pattern"], "*.PerimeterOfSquare");
        let again = r.command(&rerun, &ctx(false)).unwrap().unwrap();
        assert_eq!(again.args[1], "--gtest_filter=*.PerimeterOfSquare");
        let (program, args) = debug_command(
            &file.targets[0].configuration,
            &CppContext::from(&ctx(false)).unwrap(),
        )
        .unwrap();
        assert!(program.ends_with("shapes_tests"));
        assert_eq!(args, ["--gtest_filter=PolygonTest.AreaOfSquare"]);
    }

    #[test]
    fn catch2_gutter_and_commands() {
        let r = registry();
        let file = r.discover("tests/stats_test.cpp", CATCH_FILE, None);
        assert_eq!(file.framework.as_deref(), Some("catch2"));
        assert_eq!(file.targets.len(), 2);
        assert_eq!(
            file.targets[1].key,
            "tests/stats_test.cpp::median/even, count"
        );
        assert_eq!(file.targets[1].kind, TargetKind::Describe);
        let cmd = r
            .command(&file.targets[1].configuration, &ctx(false))
            .unwrap()
            .unwrap();
        assert_eq!(
            cmd.program,
            "/home/dev/shapes/cmake-build-debug/stats_tests"
        );
        assert_eq!(
            cmd.args,
            [
                "median",
                "-c",
                "even, count",
                "--reporter",
                "junit",
                "--out",
                "/tmp/forge-run-1/catch2-report.xml"
            ]
        );
        assert_eq!(cmd.default_file.as_deref(), Some("tests/stats_test.cpp"));
        let rerun = r
            .rerun_failed(
                &file.targets[0].configuration,
                &[
                    FailedTest {
                        name: "median/even, count".into(),
                        ..FailedTest::default()
                    },
                    FailedTest {
                        name: "mean [fast]".into(),
                        ..FailedTest::default()
                    },
                ],
            )
            .unwrap();
        assert_eq!(rerun.options["pattern"], "median,mean \\[fast\\]");
        let (_, args) = debug_command(
            &file.targets[1].configuration,
            &CppContext::from(&ctx(false)).unwrap(),
        )
        .unwrap();
        assert_eq!(args, ["median", "-c", "even, count"]);
        // A file of another framework is not Catch2's.
        assert!(CppTests::catch2().discover("a.cpp", GTEST_FILE).is_none());
    }

    #[test]
    fn ctest_and_coverage() {
        let r = registry();
        let mut cfg = Configuration::new("All CTest", "ctest");
        cfg.options.insert("jobs".into(), "4".into());
        let cmd = r.command(&cfg, &ctx(true)).unwrap().unwrap();
        assert_eq!(cmd.program, "/usr/bin/ctest");
        assert_eq!(
            cmd.args,
            [
                "--test-dir",
                "/home/dev/shapes/cmake-build-debug",
                "--output-on-failure",
                "--output-junit",
                "/tmp/forge-run-1/ctest-report.xml",
                "-j",
                "4"
            ]
        );
        assert_eq!(
            cmd.env["LLVM_PROFILE_FILE"],
            "/tmp/forge-run-1/coverage/forge-%p.profraw"
        );
        let cov = cmd.coverage.unwrap();
        assert_eq!(cov.format, CoverageFormat::Lcov);
        let collect = cov.collect.unwrap();
        assert_eq!(collect.kind, "cpp");
        assert_eq!(collect.dirs, ["/home/dev/shapes/cmake-build-debug"]);
        assert_eq!(collect.binaries.len(), 2);
        let rerun = r
            .rerun_failed(
                &cfg,
                &[FailedTest {
                    name: "perimeter.square".into(),
                    ..FailedTest::default()
                }],
            )
            .unwrap();
        let again = r.command(&rerun, &ctx(false)).unwrap().unwrap();
        assert_eq!(
            &again.args[5..],
            ["-R", "^(perimeter\\.square)$", "-j", "4"]
        );
        assert!(r.supports_coverage("ctest"));
        assert!(!CppTests::ctest().accepts("a.cpp"));
        assert!(CppTests::gtest().accepts("tests/a_test.cc"));
    }

    /// The recorded and documented reports become the test tree's events.
    #[test]
    fn reports_become_test_events() {
        use forge_output::testevents::TestEvent;
        let read = |name: &str| {
            std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures")
                    .join(name),
            )
            .unwrap()
        };
        let summary = |xml: &str| -> Vec<String> {
            forge_output::formats::document_events(xml, JUnitFlavor::Generic)
                .unwrap()
                .into_iter()
                .filter_map(|e| match e {
                    TestEvent::TestFailed { name, .. } => Some(format!("failed {name}")),
                    TestEvent::TestIgnored { name, .. } => Some(format!("ignored {name}")),
                    TestEvent::TestStarted { name, location } => Some(format!(
                        "started {name} {}",
                        location
                            .map(|l| format!("{}:{}", l.path, l.line.unwrap_or(0)))
                            .unwrap_or_default()
                    )),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            summary(&read("gtest-report.xml")),
            [
                "started AreaOfSquare /home/dev/shapes/tests/shapes_test.cpp:8",
                "started PerimeterOfSquare /home/dev/shapes/tests/shapes_test.cpp:13",
                "failed PerimeterOfSquare",
                "ignored DISABLED_Triangle",
                "started KeepsRatio/0 /home/dev/shapes/tests/shapes_test.cpp:25",
                "started KeepsRatio/1 /home/dev/shapes/tests/shapes_test.cpp:25",
            ]
        );
        assert_eq!(
            summary(&read("catch2-junit.xml")),
            [
                "started mean of a vector ",
                "started median/odd count ",
                "started median/even count ",
                "failed median/even count",
                "started empty input throws ",
            ]
        );
        assert_eq!(
            summary(&read("ctest-junit.xml")),
            ["started area ", "started perimeter ", "failed perimeter"]
        );
    }

    #[test]
    fn unknown_targets_explain_themselves() {
        let r = registry();
        let mut cfg = Configuration::new("x", "gtest");
        cfg.options.insert("file".into(), "tests/other.cpp".into());
        let err = r.command(&cfg, &ctx(false)).unwrap().unwrap_err();
        assert!(
            err.contains("No CMake target compiles tests/other.cpp"),
            "{err}"
        );
        cfg.options.insert("target".into(), "nope".into());
        assert!(r
            .command(&cfg, &ctx(false))
            .unwrap()
            .unwrap_err()
            .contains("'nope'"));
        cfg.options.insert("executable".into(), "/opt/t".into());
        assert_eq!(
            r.command(&cfg, &ctx(false)).unwrap().unwrap().program,
            "/opt/t"
        );
        let c = CppContext::from(&ctx(false)).unwrap();
        let mut by_file = Configuration::new("y", "gtest");
        by_file
            .options
            .insert("file".into(), "tests/stats_test.cpp".into());
        assert_eq!(c.target_name(&by_file).as_deref(), Some("stats_tests"));
    }
}
