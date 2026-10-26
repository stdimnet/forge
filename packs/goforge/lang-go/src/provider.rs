//! The Go Test provider of the shared test runner (`forge-testing`): it wraps GoForge's
//! `[[contributions.testFrameworks]]` entry (`go-test`: the run configuration form, its
//! test files and the `goTestJson` report) with what only Go needs:
//!
//! - discovery of tests, benchmarks, fuzz tests, examples and literal subtests
//!   ([`crate::discovery`]), keyed `<package folder>::<name>` like the test tree's results,
//! - `go test -json` command lines per scope and kind: tests (`-run '^TestA$/^sub$'`),
//!   benchmarks (`-bench`, `-benchmem`), fuzzing (`-fuzz`, `-fuzztime`), examples,
//! - coverage (`-coverprofile`, read as a Go coverprofile), CPU / memory / block / mutex
//!   profiles (`-cpuprofile`, …, opened in the Profiler afterwards),
//! - Rerun Failed Tests (`-run '^(TestA|TestB)$'` in the failed tests' packages),
//! - the project's SDK, GOPATH, GOFLAGS, build tags and modules from a [`GoContext`]
//!   (`-C <module>` when a module lives in a sub folder; import paths become project paths
//!   through location rules).
//!
//! Configuration options: `scope` (`all`, `directory`, `file`, `test`, `pattern`), `kind`
//! (`test`, `bench`, `fuzz`), `directory`, `file`, `test`, `pattern`, `tests` (the file's
//! top-level tests, `|` separated), `tags`, `fuzztime`, `benchtime`, `count`, `flags` (more
//! `go test` flags), `profile` (`cpu`, `mem`, `block`, `mutex`) and `profileFile`.

use forge_output::{JUnitFlavor, TestReport};
use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use forge_testing::{
    CommandContext, CoverageFormat, CoverageRequest, FailedTest, TargetKind, TestCommand, TestFile,
    TestFrameworkProvider, TestFrameworkSpec, TestTarget,
};

use crate::discovery::{discover, GoTestKind};
use crate::env::{dir_of, quote_meta, GoContext, CONTEXT_KEY};

/// The framework id and run configuration type.
pub const GO_TEST: &str = "go-test";

pub fn is_test_file(path: &str) -> bool {
    path.replace('\\', "/")
        .rsplit('/')
        .next()
        .is_some_and(|n| n.ends_with("_test.go"))
}

/// The Go Test provider.
pub struct GoTestProvider {
    spec: TestFrameworkSpec,
}

impl GoTestProvider {
    /// Wrap the manifest's `go-test` spec; `None` for other specs.
    pub fn new(spec: TestFrameworkSpec) -> Option<Self> {
        (spec.id == GO_TEST).then_some(Self { spec })
    }

    /// A provider without a manifest (tests, WebAssembly).
    pub fn standalone() -> Self {
        Self {
            spec: TestFrameworkSpec {
                id: GO_TEST.into(),
                name: "Go Test".into(),
                test_files: vec!["*_test.go".into()],
                report: TestReport::GoTestJson,
                ..TestFrameworkSpec::default()
            },
        }
    }
}

/// `-run` pattern of a full test name: each `/` level anchored on its own.
pub fn run_pattern(name: &str) -> String {
    name.split('/')
        .map(|p| format!("^{}$", quote_meta(p)))
        .collect::<Vec<_>>()
        .join("/")
}

fn kind_option(kind: GoTestKind) -> &'static str {
    match kind {
        GoTestKind::Benchmark => "bench",
        GoTestKind::Fuzz => "fuzz",
        GoTestKind::Test | GoTestKind::Example => "test",
    }
}

fn package_label(dir: &str, package: &str) -> String {
    let last = dir.rsplit('/').next().unwrap_or("");
    if !last.is_empty() {
        last.to_string()
    } else if !package.is_empty() {
        package.trim_end_matches("_test").to_string()
    } else {
        "root".to_string()
    }
}

/// Register the Go Test provider for the `go-test` spec among `specs`.
pub fn register(
    registry: &mut forge_testing::TestFrameworks,
    specs: impl IntoIterator<Item = TestFrameworkSpec>,
) {
    for spec in specs {
        if let Some(p) = GoTestProvider::new(spec) {
            registry.register(Box::new(p));
        }
    }
}

impl GoTestProvider {
    fn discover_file(&self, path: &str, text: &str) -> TestFile {
        let path = path.replace('\\', "/");
        let found = discover(text);
        let dir = dir_of(&path).to_string();
        let pkg = package_label(&dir, &found.package);
        let base = |name: String| {
            let mut c = Configuration::new(name, GO_TEST);
            c.options.insert("directory".into(), dir.clone());
            c.options.insert("file".into(), path.clone());
            c
        };
        let mut targets = Vec::new();
        if !found.tests.is_empty() {
            // ▶ on the package clause: every test of the package.
            if let Some(line) = text.lines().position(|l| l.trim_start().starts_with("package ")) {
                let mut c = base(format!("go test {pkg}"));
                c.options.insert("scope".into(), "directory".into());
                c.options.insert("recursive".into(), "false".into());
                targets.push(TestTarget {
                    line: line as u32,
                    kind: TargetKind::Suite,
                    label: format!("tests in '{pkg}'"),
                    key: dir.clone(),
                    configuration: c,
                });
            }
        }
        for t in &found.tests {
            let mut c = base(format!("{} in {pkg}", t.name));
            c.options.insert("scope".into(), "test".into());
            c.options.insert("test".into(), t.name.clone());
            c.options.insert("kind".into(), kind_option(t.kind).into());
            targets.push(TestTarget {
                line: t.line,
                kind: if t.kind == GoTestKind::Example {
                    TargetKind::Example
                } else {
                    TargetKind::Test
                },
                label: t.name.clone(),
                key: format!("{dir}::{}", t.name),
                configuration: c,
            });
        }
        let tests: Vec<&str> = found
            .tests
            .iter()
            .filter(|t| t.depth == 0 && matches!(t.kind, GoTestKind::Test | GoTestKind::Example))
            .map(|t| t.name.as_str())
            .collect();
        let file = (!tests.is_empty()).then(|| {
            let name = path.rsplit('/').next().unwrap_or(&path).to_string();
            let mut c = base(format!("go test {name}"));
            c.options.insert("scope".into(), "file".into());
            c.options.insert("tests".into(), tests.join("|"));
            c
        });
        TestFile {
            framework: Some(self.spec.id.clone()),
            targets,
            file,
        }
    }

    fn build(&self, cfg: &Configuration, ctx: &CommandContext) -> Result<TestCommand, String> {
        let go: GoContext = ctx
            .settings
            .get(CONTEXT_KEY)
            .map(|s| serde_json::from_str(s).map_err(|e| format!("bad Go context: {e}")))
            .transpose()?
            .unwrap_or_default();
        let opt = |k: &str| cfg.option(k).trim().to_string();
        let scope = match opt("scope") {
            s if s.is_empty() => "all".to_string(),
            s => s,
        };
        let kind = match opt("kind") {
            k if k.is_empty() => "test".to_string(),
            k => k,
        };
        let wd = cfg.working_directory.trim_matches('/').to_string();
        let file = opt("file").replace('\\', "/");
        let directory = match opt("directory") {
            d if d.is_empty() && !file.is_empty() => dir_of(&file).to_string(),
            d => d.trim_matches('/').to_string(),
        };

        // Packages and the filter of the scope.
        let mut change_dir: Option<String> = None;
        let mut packages: Vec<String> = Vec::new();
        let mut filter: Option<String> = None;
        let pkg_of = |dir: &str, recursive: bool, change_dir: &mut Option<String>| {
            let (c, p) = go.package_arg(dir, &wd);
            if change_dir.is_none() {
                *change_dir = c;
            }
            if recursive {
                if p == "." {
                    "./...".to_string()
                } else {
                    format!("{p}/...")
                }
            } else {
                p
            }
        };
        match scope.as_str() {
            "all" => {
                let (c, _) = go.package_arg(&wd, &wd);
                change_dir = c;
                packages.push("./...".into());
            }
            "directory" => {
                let recursive = opt("recursive") != "false";
                packages.push(pkg_of(&directory, recursive, &mut change_dir));
            }
            "file" => {
                if file.is_empty() {
                    return Err("Choose the test file".into());
                }
                packages.push(pkg_of(&directory, false, &mut change_dir));
                let tests = opt("tests");
                if !tests.is_empty() {
                    let names: Vec<String> = tests.split('|').map(quote_meta).collect();
                    filter = Some(format!("^({})$", names.join("|")));
                }
            }
            "test" => {
                let test = opt("test");
                if test.is_empty() {
                    return Err("Choose the test".into());
                }
                packages.push(pkg_of(&directory, false, &mut change_dir));
                filter = Some(run_pattern(&test));
            }
            "pattern" => {
                let pattern = opt("pattern");
                if pattern.is_empty() {
                    return Err("Enter a test name pattern".into());
                }
                let listed: Vec<String> = opt("packages")
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect();
                if listed.is_empty() && directory.is_empty() {
                    let (c, _) = go.package_arg(&wd, &wd);
                    change_dir = c;
                    packages.push("./...".into());
                } else if listed.is_empty() {
                    packages.push(pkg_of(&directory, true, &mut change_dir));
                } else {
                    for d in &listed {
                        packages.push(pkg_of(d, false, &mut change_dir));
                    }
                }
                filter = Some(pattern);
            }
            other => return Err(format!("Unknown test scope '{other}'")),
        }

        let mut args: Vec<String> = Vec::new();
        if let Some(c) = &change_dir {
            args.push("-C".into());
            args.push(c.clone());
        }
        args.push("test".into());
        args.push("-json".into());
        let tags = go.tags(&opt("tags"));
        if !tags.is_empty() {
            args.push(format!("-tags={}", tags.join(",")));
        }
        if !opt("count").is_empty() {
            args.push(format!("-count={}", opt("count")));
        }
        match kind.as_str() {
            "bench" => {
                args.push("-run=^$".into());
                args.push(format!("-bench={}", filter.as_deref().unwrap_or(".")));
                args.push("-benchmem".into());
                if !opt("benchtime").is_empty() {
                    args.push(format!("-benchtime={}", opt("benchtime")));
                }
            }
            "fuzz" => {
                if packages.len() != 1 || packages[0].ends_with("/...") {
                    return Err("Fuzzing runs one fuzz test of one package".into());
                }
                let Some(f) = &filter else {
                    return Err("Choose the fuzz test".into());
                };
                args.push("-run=^$".into());
                args.push(format!("-fuzz={f}"));
                let time = match opt("fuzztime") {
                    t if t.is_empty() => "10s".to_string(),
                    t => t,
                };
                args.push(format!("-fuzztime={time}"));
            }
            _ => {
                if let Some(f) = &filter {
                    args.push(format!("-run={f}"));
                }
            }
        }
        let coverage = ctx.coverage_file.as_ref().map(|file| {
            args.push(format!("-coverprofile={file}"));
            CoverageRequest {
                file: file.clone(),
                format: CoverageFormat::GoCover,
            }
        });
        let profile_file = opt("profileFile");
        match opt("profile").as_str() {
            "" | "none" => {}
            p @ ("cpu" | "mem" | "block" | "mutex") => {
                if profile_file.is_empty() {
                    return Err("No file for the profile".into());
                }
                if packages.len() != 1 || packages[0].ends_with("/...") {
                    return Err("Profiling runs the tests of one package".into());
                }
                args.push(format!("-{p}profile={profile_file}"));
            }
            other => return Err(format!("Unknown profile '{other}'")),
        }
        args.extend(split_args(&opt("flags")));
        args.extend(packages);
        let program_args = split_args(&cfg.arguments);
        if !program_args.is_empty() {
            args.push("-args".into());
            args.extend(program_args);
        }
        Ok(TestCommand {
            program: go.go(),
            args,
            env: go.environment(),
            report: TestReport::GoTestJson,
            flavor: JUnitFlavor::Generic,
            location_rules: go.location_rules(),
            default_file: (scope == "file").then_some(file),
            coverage,
        })
    }

    fn rerun(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
        let mut names: Vec<String> = Vec::new();
        let mut dirs: Vec<String> = Vec::new();
        for f in failed {
            let top = f.name.split('/').next().unwrap_or(&f.name).to_string();
            let valid = ["Test", "Example", "Fuzz", "Benchmark"]
                .iter()
                .any(|p| top.starts_with(p));
            if valid && !names.contains(&top) {
                names.push(top);
            }
            if let Some(p) = f.path.as_deref().filter(|p| !p.is_empty()) {
                let d = if p.ends_with(".go") { dir_of(p) } else { p };
                let d = d.to_string();
                if !dirs.contains(&d) {
                    dirs.push(d);
                }
            }
        }
        if names.is_empty() {
            return None;
        }
        let mut c = cfg.clone();
        let quoted: Vec<String> = names.iter().map(|n| quote_meta(n)).collect();
        c.options.insert("scope".into(), "pattern".into());
        c.options.insert("pattern".into(), format!("^({})$", quoted.join("|")));
        c.options.insert("kind".into(), "test".into());
        c.options.remove("test");
        c.options.remove("tests");
        if dirs.is_empty() {
            c.options.remove("packages");
        } else {
            c.options.insert("packages".into(), dirs.join("\n"));
        }
        c.name = format!("Rerun failed: {}", cfg.name);
        Some(c)
    }
}

/// What Delve needs to debug a Go Test configuration: the package folder (project
/// relative), the test binary's flags and the build flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugTest {
    pub package_dir: String,
    pub args: Vec<String>,
    pub build_flags: Vec<String>,
}

/// The Go Test configuration as a single-package test binary run (Delve's `test` mode).
/// Fuzz tests run their seed corpus; benchmarks run once per `-test.benchtime`.
pub fn debug_test(cfg: &Configuration, go: &GoContext) -> Result<DebugTest, String> {
    let opt = |k: &str| cfg.option(k).trim().to_string();
    let file = opt("file").replace('\\', "/");
    let directory = match opt("directory") {
        d if d.is_empty() && !file.is_empty() => dir_of(&file).to_string(),
        d => d.trim_matches('/').to_string(),
    };
    let scope = opt("scope");
    let mut args = Vec::new();
    let filter = match scope.as_str() {
        "test" => Some(run_pattern(&opt("test"))),
        "file" => Some(opt("tests"))
            .filter(|t| !t.is_empty())
            .map(|t| format!("^({})$", t.split('|').map(quote_meta).collect::<Vec<_>>().join("|"))),
        "pattern" => Some(opt("pattern")),
        "directory" if opt("recursive") == "false" => None,
        _ => {
            return Err(
                "Debugging runs the tests of one package: choose a file, a test or a package folder"
                    .into(),
            )
        }
    };
    match (opt("kind").as_str(), filter) {
        ("bench", f) => {
            args.push("-test.run=^$".to_string());
            args.push(format!("-test.bench={}", f.as_deref().unwrap_or(".")));
            args.push("-test.benchmem".into());
        }
        (_, Some(f)) => args.push(format!("-test.run={f}")),
        (_, None) => {}
    }
    args.push("-test.v".into());
    let mut build_flags = Vec::new();
    let tags = go.tags(&opt("tags"));
    if !tags.is_empty() {
        build_flags.push(format!("-tags={}", tags.join(",")));
    }
    Ok(DebugTest {
        package_dir: directory,
        args,
        build_flags,
    })
}

impl TestFrameworkProvider for GoTestProvider {
    fn id(&self) -> &str {
        &self.spec.id
    }
    fn name(&self) -> &str {
        &self.spec.name
    }
    fn types(&self) -> Vec<String> {
        vec![self.spec.id.clone()]
    }
    fn accepts(&self, path: &str) -> bool {
        is_test_file(path)
    }
    fn discover(&self, path: &str, text: &str) -> Option<TestFile> {
        Some(self.discover_file(path, text)).filter(|f| !f.targets.is_empty())
    }
    fn rerun_failed(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
        self.rerun(cfg, failed)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::ModuleRoot;
    use std::collections::BTreeMap;

    const MONEY_TEST: &str = "package money\n\nimport \"testing\"\n\nfunc TestRound(t *testing.T) {\n\tt.Run(\"half up\", func(t *testing.T) {})\n}\n\nfunc BenchmarkString(b *testing.B) {}\n\nfunc FuzzParse(f *testing.F) {}\n\nfunc ExampleAmount_String() {}\n";

    fn ctx(coverage: bool) -> CommandContext {
        let go = GoContext {
            goroot: "/sdk/go1.24".into(),
            build_tags: vec!["integration".into()],
            modules: vec![
                ModuleRoot {
                    path: "github.com/acme/ledger".into(),
                    dir: "".into(),
                },
                ModuleRoot {
                    path: "github.com/acme/tools".into(),
                    dir: "tools".into(),
                },
            ],
            ..GoContext::default()
        };
        CommandContext {
            report_dir: "/tmp/r".into(),
            coverage_file: coverage.then(|| "/tmp/r/coverage/coverage".into()),
            settings: BTreeMap::from([(CONTEXT_KEY.into(), serde_json::to_string(&go).unwrap())]),
        }
    }

    fn args(cfg: &Configuration, coverage: bool) -> Vec<String> {
        GoTestProvider::standalone()
            .build(cfg, &ctx(coverage))
            .unwrap()
            .args
    }

    #[test]
    fn discovers_targets_with_package_keys() {
        let f = GoTestProvider::standalone()
            .discover("internal/money/money_test.go", MONEY_TEST)
            .unwrap();
        let rows: Vec<_> = f
            .targets
            .iter()
            .map(|t| (t.line, t.kind, t.key.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                (0, TargetKind::Suite, "internal/money"),
                (4, TargetKind::Test, "internal/money::TestRound"),
                (5, TargetKind::Test, "internal/money::TestRound/half_up"),
                (8, TargetKind::Test, "internal/money::BenchmarkString"),
                (10, TargetKind::Test, "internal/money::FuzzParse"),
                (12, TargetKind::Example, "internal/money::ExampleAmount_String"),
            ]
        );
        assert_eq!(f.targets[1].configuration.name, "TestRound in money");
        assert_eq!(f.targets[3].configuration.option("kind"), "bench");
        assert_eq!(
            f.file.unwrap().option("tests"),
            "TestRound|ExampleAmount_String"
        );
        assert!(GoTestProvider::standalone().discover("a_test.go", "package a").is_none());
        assert!(!GoTestProvider::standalone().accepts("a.go"));
    }

    #[test]
    fn command_lines_per_scope_and_kind() {
        let test = Configuration::new("t", GO_TEST)
            .with_option("scope", "test")
            .with_option("file", "internal/money/money_test.go")
            .with_option("test", "TestRound/half_up");
        let built = GoTestProvider::standalone().build(&test, &ctx(false)).unwrap();
        assert_eq!(built.program, "/sdk/go1.24/bin/go");
        assert_eq!(
            built.args,
            [
                "test",
                "-json",
                "-tags=integration",
                "-run=^TestRound$/^half_up$",
                "./internal/money"
            ]
        );
        assert_eq!(built.report, TestReport::GoTestJson);
        assert!(built.location_rules.iter().any(|r| r.replacement == "tools/"));

        let all = Configuration::new("all", GO_TEST);
        assert_eq!(args(&all, true)[3..], ["-coverprofile=/tmp/r/coverage/coverage", "./..."]);
        let cov = GoTestProvider::standalone().build(&all, &ctx(true)).unwrap();
        assert_eq!(cov.coverage.unwrap().format, CoverageFormat::GoCover);

        let bench = test
            .clone()
            .with_option("test", "BenchmarkString")
            .with_option("kind", "bench")
            .with_option("benchtime", "2s");
        assert_eq!(
            args(&bench, false)[3..],
            ["-run=^$", "-bench=^BenchmarkString$", "-benchmem", "-benchtime=2s", "./internal/money"]
        );
        let fuzz = test
            .clone()
            .with_option("test", "FuzzParse")
            .with_option("kind", "fuzz")
            .with_option("fuzztime", "30s");
        assert_eq!(
            args(&fuzz, false)[3..],
            ["-run=^$", "-fuzz=^FuzzParse$", "-fuzztime=30s", "./internal/money"]
        );
        let fuzz_all = all.clone().with_option("kind", "fuzz");
        assert!(GoTestProvider::standalone().build(&fuzz_all, &ctx(false)).is_err());

        let file = Configuration::new("f", GO_TEST)
            .with_option("scope", "file")
            .with_option("file", "tools/gen/gen_test.go")
            .with_option("tests", "TestA|TestB");
        let built = GoTestProvider::standalone().build(&file, &ctx(false)).unwrap();
        assert_eq!(built.args[..2], ["-C", "tools"]);
        assert_eq!(built.args[5..], ["-run=^(TestA|TestB)$", "./gen"]);
        assert_eq!(built.default_file.as_deref(), Some("tools/gen/gen_test.go"));

        let dir = Configuration::new("d", GO_TEST)
            .with_option("scope", "directory")
            .with_option("directory", "internal");
        assert_eq!(args(&dir, false).last().unwrap(), "./internal/...");

        let mut prof = test.clone().with_option("profile", "cpu");
        assert!(GoTestProvider::standalone().build(&prof, &ctx(false)).is_err());
        prof = prof
            .with_option("profileFile", "/cache/cpu.pprof")
            .with_option("flags", "-v -race");
        prof.arguments = "-update".into();
        assert_eq!(
            args(&prof, false)[3..],
            [
                "-run=^TestRound$/^half_up$",
                "-cpuprofile=/cache/cpu.pprof",
                "-v",
                "-race",
                "./internal/money",
                "-args",
                "-update"
            ]
        );
    }

    #[test]
    fn debug_runs_one_package() {
        let go = GoContext {
            build_tags: vec!["integration".into()],
            ..GoContext::default()
        };
        let cfg = Configuration::new("t", GO_TEST)
            .with_option("scope", "test")
            .with_option("file", "internal/money/money_test.go")
            .with_option("test", "TestRound/half_up");
        let d = debug_test(&cfg, &go).unwrap();
        assert_eq!(d.package_dir, "internal/money");
        assert_eq!(d.args, ["-test.run=^TestRound$/^half_up$", "-test.v"]);
        assert_eq!(d.build_flags, ["-tags=integration"]);
        let bench = cfg.clone().with_option("kind", "bench").with_option("test", "BenchmarkString");
        assert_eq!(debug_test(&bench, &go).unwrap().args[1], "-test.bench=^BenchmarkString$");
        assert!(debug_test(&Configuration::new("all", GO_TEST), &go).is_err());
    }

    #[test]
    fn reruns_failed_tests_by_top_level_name() {
        let cfg = Configuration::new("all", GO_TEST);
        let failed = [
            FailedTest {
                name: "TestRound/negative".into(),
                member: Some("TestRound".into()),
                path: Some("internal/money/money_test.go".into()),
            },
            FailedTest {
                name: "TestRound".into(),
                member: None,
                path: Some("internal/money".into()),
            },
            FailedTest {
                name: "TestParse".into(),
                member: None,
                path: Some("internal/money".into()),
            },
        ];
        let p = GoTestProvider::standalone();
        let r = p.rerun(&cfg, &failed).unwrap();
        assert_eq!(r.option("pattern"), "^(TestRound|TestParse)$");
        assert_eq!(r.option("packages"), "internal/money");
        assert_eq!(
            args(&r, false)[3..],
            ["-run=^(TestRound|TestParse)$", "./internal/money"]
        );
        assert!(p.rerun(&cfg, &[]).is_none());
    }
}
