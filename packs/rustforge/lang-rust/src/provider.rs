//! The Cargo Test provider of the shared test runner (`forge-testing`). It wraps RustForge's
//! `[[contributions.testFrameworks]]` entry (`cargo-test`) with what only Rust needs:
//!
//! - discovery ([`crate::scan`]): tests, `#[tokio::test]` and friends, benchmarks, test
//!   modules and doctests, keyed `<file>::<libtest name>` like the test tree's results,
//! - command lines: `cargo test` (libtest JSON on nightly toolchains, plain output
//!   otherwise), `cargo bench`, `cargo test --doc`, `cargo nextest run` (libtest JSON) when
//!   it is installed and chosen, and `cargo llvm-cov` (LCOV) for Run with Coverage,
//! - Rerun Failed Tests (`-- --exact <name> <name>`),
//! - the location rules of the project's crates ([`CargoContext::location_rules`]),
//! - [`debug_test`]: the build of a test binary for the debugger.
//!
//! Configuration options: `scope` (`all`, `package`, `target`, `test`, `pattern`),
//! `package`, `target` (`lib`, `bin:x`, `test:x`, `bench:x`, `example:x`, `doc`), `test` (a
//! name filter), `exact`, `tests` (exact names, one per line), `kind` (`test`, `bench`),
//! `runner` (`auto`, `cargo`, `nextest`), `format` (`auto`, `json`, `text`), `features`,
//! `allFeatures`, `noDefaultFeatures`, `profile`, `targetTriple`, `toolchain`, `flags` (more
//! cargo options), `file` (the file the configuration came from); the program arguments go
//! to the test binaries.

use forge_output::{JUnitFlavor, TestReport};
use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use forge_testing::{
    CommandContext, CoverageFormat, CoverageRequest, FailedTest, TargetKind as Kind, TestCommand,
    TestFile, TestFrameworkProvider, TestFrameworkSpec, TestTarget,
};

use crate::command::{parse_target, selection_args, Selection};
use crate::context::{CargoContext, CONTEXT_KEY};
use crate::metadata::TargetKind;
use crate::scan::{scan, ItemKind};

/// The framework id and run configuration type.
pub const CARGO_TEST: &str = "cargo-test";

pub fn is_rust_file(path: &str) -> bool {
    path.ends_with(".rs")
}

pub struct CargoTestProvider {
    spec: TestFrameworkSpec,
}

impl CargoTestProvider {
    /// Wrap the manifest's `cargo-test` spec; `None` for other specs.
    pub fn new(spec: TestFrameworkSpec) -> Option<Self> {
        (spec.id == CARGO_TEST).then_some(Self { spec })
    }

    /// A provider without a manifest (tests, WebAssembly).
    pub fn standalone() -> Self {
        Self {
            spec: TestFrameworkSpec {
                id: CARGO_TEST.into(),
                name: "Cargo Test".into(),
                test_files: vec!["*.rs".into()],
                report: TestReport::LibtestJson,
                ..TestFrameworkSpec::default()
            },
        }
    }
}

/// Register the Cargo Test provider for the `cargo-test` spec among `specs`.
pub fn register(
    registry: &mut forge_testing::TestFrameworks,
    specs: impl IntoIterator<Item = TestFrameworkSpec>,
) {
    for spec in specs {
        if let Some(p) = CargoTestProvider::new(spec) {
            registry.register(Box::new(p));
        }
    }
}

/// Where a file sits in the workspace: package, target key and module path. Without a
/// context the standard layout is assumed (`<package>/src/lib.rs`, `tests/<name>.rs`, …).
pub fn locate(path: &str, ctx: Option<&CargoContext>) -> (String, String, String) {
    if let Some((p, t, m)) = ctx.and_then(|c| c.crate_of(path)) {
        let key = match t.kind {
            TargetKind::Lib | TargetKind::ProcMacro => "lib".to_string(),
            k => format!("{}:{}", k.id(), t.name),
        };
        return (p.name.clone(), key, m);
    }
    let rest = match path.rfind("src/") {
        Some(i) if i == 0 || path[..i].ends_with('/') => &path[i + 4..],
        _ => match ["tests/", "benches/", "examples/"]
            .iter()
            .find_map(|f| path.rfind(f).map(|i| (i, *f)))
        {
            Some((i, folder)) => {
                let name = path[i + folder.len()..]
                    .split('/')
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(".rs");
                let kind = match folder {
                    "tests/" => "test",
                    "benches/" => "bench",
                    _ => "example",
                };
                let inner = path[i + folder.len()..]
                    .strip_prefix(&format!("{name}/"))
                    .unwrap_or("");
                let module = inner
                    .trim_end_matches(".rs")
                    .trim_end_matches("/mod")
                    .trim_end_matches("main")
                    .trim_end_matches('/')
                    .replace('/', "::");
                return (String::new(), format!("{kind}:{name}"), module);
            }
            None => return (String::new(), String::new(), String::new()),
        },
    };
    let stem = rest.trim_end_matches(".rs");
    if stem == "main" {
        return (String::new(), String::new(), String::new());
    }
    if let Some(bin) = stem.strip_prefix("bin/") {
        let name = bin.split('/').next().unwrap_or(bin);
        let module = bin
            .strip_prefix(&format!("{name}/"))
            .unwrap_or("")
            .trim_end_matches("/mod")
            .replace('/', "::");
        return (String::new(), format!("bin:{name}"), module);
    }
    let module = if stem == "lib" {
        String::new()
    } else {
        stem.trim_end_matches("/mod").replace('/', "::")
    };
    (String::new(), "lib".into(), module)
}

fn join(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_string(),
        (_, true) => a.to_string(),
        _ => format!("{a}::{b}"),
    }
}

/// The tests of a Rust file for the gutter.
pub fn discover_file(path: &str, text: &str, ctx: Option<&CargoContext>) -> TestFile {
    let path = path.replace('\\', "/");
    let (package, target, module) = locate(&path, ctx);
    let workspace = ctx.map(|c| c.workspace.as_str()).unwrap_or("");
    let ws_rel = match workspace {
        "" => path.clone(),
        w => path
            .strip_prefix(&format!("{w}/"))
            .unwrap_or(&path)
            .to_string(),
    };
    let base = |name: String| {
        let mut c = Configuration::new(name, CARGO_TEST);
        if !package.is_empty() {
            c.options.insert("package".into(), package.clone());
        }
        if !target.is_empty() {
            c.options.insert("target".into(), target.clone());
        }
        c.options.insert("file".into(), path.clone());
        c
    };
    let mut targets = Vec::new();
    let items = scan(text, &module);
    for it in &items {
        match it.kind {
            ItemKind::Main => {}
            ItemKind::Test | ItemKind::Bench => {
                let mut c = base(it.path.clone());
                c.options.insert("scope".into(), "test".into());
                c.options.insert("test".into(), it.path.clone());
                c.options.insert("exact".into(), "true".into());
                if it.kind == ItemKind::Bench {
                    c.options.insert("kind".into(), "bench".into());
                }
                targets.push(TestTarget {
                    line: it.line,
                    kind: Kind::Test,
                    label: it.name.clone(),
                    key: format!("{path}::{}", it.path),
                    configuration: c,
                });
            }
            ItemKind::TestModule => {
                let mut c = base(format!("{}::", it.path));
                c.options.insert("scope".into(), "test".into());
                c.options.insert("test".into(), format!("{}::", it.path));
                targets.push(TestTarget {
                    line: it.line,
                    kind: Kind::Suite,
                    label: it.path.clone(),
                    key: path.clone(),
                    configuration: c,
                });
            }
            ItemKind::Doctest => {
                let name = if it.path.is_empty() {
                    format!("{ws_rel} - (line {})", it.line + 1)
                } else {
                    format!("{ws_rel} - {} (line {})", it.path, it.line + 1)
                };
                let shown = if it.path.is_empty() {
                    "crate documentation".to_string()
                } else {
                    it.path.clone()
                };
                let mut c = base(format!("Doctest {shown} (line {})", it.line + 1));
                c.options.insert("scope".into(), "test".into());
                c.options.insert("target".into(), "doc".into());
                c.options.insert("test".into(), name.clone());
                c.options.insert("exact".into(), "true".into());
                targets.push(TestTarget {
                    line: it.line,
                    kind: Kind::Example,
                    label: format!("doctest of {shown}"),
                    key: format!("{path}::{name}"),
                    configuration: c,
                });
            }
        }
    }
    let has_tests = items.iter().any(|i| {
        matches!(
            i.kind,
            ItemKind::Test | ItemKind::Bench | ItemKind::TestModule
        )
    });
    let file = has_tests.then(|| {
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        let mut c = base(format!("Tests in {name}"));
        if module.is_empty() {
            c.options.insert("scope".into(), "target".into());
        } else {
            c.options.insert("scope".into(), "test".into());
            c.options.insert("test".into(), join(&module, ""));
        }
        c
    });
    TestFile {
        framework: Some(CARGO_TEST.into()),
        targets,
        file,
    }
}

/// The command and how it reports (shared by runs and debugging).
struct Plan {
    args: Vec<String>,
    /// Arguments for the test binaries (after `--`).
    test_args: Vec<String>,
    report: TestReport,
    nextest: bool,
}

fn context_of(ctx: &CommandContext) -> Result<CargoContext, String> {
    Ok(ctx
        .settings
        .get(CONTEXT_KEY)
        .map(|s| serde_json::from_str(s).map_err(|e| format!("bad Cargo context: {e}")))
        .transpose()?
        .unwrap_or_default())
}

fn plan(cfg: &Configuration, cargo: &CargoContext, coverage: Option<&str>) -> Result<Plan, String> {
    let opt = |k: &str| cfg.option(k).trim().to_string();
    let scope = match opt("scope") {
        s if s.is_empty() => "all".to_string(),
        s => s,
    };
    let mut sel = Selection::from_options(cfg);
    let doc = sel.target == "doc";
    if doc {
        sel.target.clear();
    }
    match scope.as_str() {
        "all" => {
            sel.package.clear();
            sel.target.clear();
        }
        "package" => {
            if sel.package.is_empty() {
                return Err("Choose the package".into());
            }
            sel.target.clear();
        }
        "target" | "test" | "pattern" => {}
        other => return Err(format!("Unknown test scope '{other}'")),
    }
    let bench = opt("kind") == "bench";
    let runner = match opt("runner").as_str() {
        "" | "auto" => cargo.test_runner.clone(),
        r => r.to_string(),
    };
    let nextest = runner == "nextest" && !doc && !bench;
    if runner == "nextest" && !cargo.nextest && !doc && !bench {
        return Err("cargo-nextest is not installed (cargo install cargo-nextest --locked)".into());
    }
    let json = match opt("format").as_str() {
        "json" => true,
        "text" => false,
        _ => match cargo.test_format.as_str() {
            "json" => true,
            "text" => false,
            _ => cargo.nightly,
        },
    };
    let mut args = cargo.toolchain_args(cfg.option("toolchain"));
    let sub = if let Some(file) = coverage {
        if !cargo.llvm_cov {
            return Err(
                "Run with Coverage needs cargo-llvm-cov (cargo install cargo-llvm-cov --locked)"
                    .into(),
            );
        }
        if doc {
            return Err(
                "Coverage of doctests needs a nightly toolchain and is not supported".into(),
            );
        }
        args.push("llvm-cov".into());
        if nextest {
            args.push("nextest".into());
        }
        args.extend([
            "--lcov".to_string(),
            "--output-path".into(),
            file.to_string(),
        ]);
        "llvm-cov"
    } else if nextest {
        args.extend(["nextest".to_string(), "run".into()]);
        "nextest"
    } else if bench {
        args.push("bench".into());
        "bench"
    } else {
        args.push("test".into());
        "test"
    };
    args.extend(cargo.manifest_args());
    args.push("--no-fail-fast".into());
    if sel.package.is_empty() {
        args.push("--workspace".into());
    }
    if doc {
        args.push("--doc".into());
    }
    let flags = split_args(&opt("flags"));
    args.extend(selection_args(sub, &sel, cargo, &flags)?);
    args.extend(flags);
    if nextest {
        args.extend(["--message-format".to_string(), "libtest-json".into()]);
    }
    let mut test_args: Vec<String> = Vec::new();
    let tests: Vec<String> = opt("tests")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    match scope.as_str() {
        "test" => {
            let t = opt("test");
            if t.is_empty() {
                return Err("Enter the test".into());
            }
            test_args.push(t);
            if opt("exact") == "true" {
                test_args.push("--exact".into());
            }
        }
        "pattern" if !tests.is_empty() => {
            test_args.extend(tests);
            test_args.push("--exact".into());
        }
        "pattern" => {
            let t = opt("test");
            if t.is_empty() {
                return Err("Enter a test name filter".into());
            }
            test_args.push(t);
        }
        _ => {}
    }
    let report = if nextest || json {
        TestReport::LibtestJson
    } else {
        TestReport::LibtestText
    };
    if json && !nextest {
        test_args.extend([
            "-Z".to_string(),
            "unstable-options".into(),
            "--format".into(),
            "json".into(),
            "--report-time".into(),
        ]);
    }
    test_args.extend(split_args(&cfg.arguments));
    Ok(Plan {
        args,
        test_args,
        report,
        nextest,
    })
}

impl CargoTestProvider {
    fn build(&self, cfg: &Configuration, ctx: &CommandContext) -> Result<TestCommand, String> {
        let cargo = context_of(ctx)?;
        let p = plan(cfg, &cargo, ctx.coverage_file.as_deref())?;
        let mut args = p.args;
        if !p.test_args.is_empty() {
            args.push("--".into());
            args.extend(p.test_args);
        }
        let mut env = cargo.environment();
        if p.nextest {
            env.insert("NEXTEST_EXPERIMENTAL_LIBTEST_JSON".into(), "1".into());
        }
        Ok(TestCommand {
            program: cargo.cargo(),
            args,
            env,
            report: p.report,
            flavor: JUnitFlavor::Generic,
            location_rules: cargo.location_rules(),
            default_file: None,
            coverage: ctx.coverage_file.as_ref().map(|f| CoverageRequest {
                file: f.clone(),
                format: CoverageFormat::Lcov,
                location_rules: Vec::new(),
            }),
            report_dirs: Vec::new(),
        })
    }

    fn rerun(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
        let mut names: Vec<String> = Vec::new();
        for f in failed {
            if !f.name.is_empty() && !names.contains(&f.name) {
                names.push(f.name.clone());
            }
        }
        if names.is_empty() {
            return None;
        }
        let mut c = cfg.clone();
        c.options.insert("scope".into(), "pattern".into());
        c.options.insert("tests".into(), names.join("\n"));
        c.options.remove("test");
        c.options.remove("exact");
        if c.option("target") != "doc" {
            c.options.remove("target");
        }
        if names.iter().any(|n| crate_doctest(n)) && names.iter().all(|n| crate_doctest(n)) {
            c.options.insert("target".into(), "doc".into());
        }
        c.name = format!("Rerun failed: {}", cfg.name);
        Some(c)
    }
}

fn crate_doctest(name: &str) -> bool {
    name.contains(".rs - ") && name.contains("(line ")
}

impl TestFrameworkProvider for CargoTestProvider {
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
        is_rust_file(path)
    }

    fn discover(&self, path: &str, text: &str) -> Option<TestFile> {
        let found = discover_file(path, text, None);
        (!found.is_empty()).then_some(found)
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

    fn supports_coverage(&self, type_id: &str) -> bool {
        type_id == self.spec.id
    }
}

/// What the debugger needs for a Cargo Test configuration: the build that finds the test
/// executable (`cargo test --no-run --message-format=json`), which of the built executables
/// to start and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugTest {
    pub build_args: Vec<String>,
    pub package: String,
    pub kind: Option<TargetKind>,
    pub target: String,
    pub args: Vec<String>,
}

pub fn debug_test(cfg: &Configuration, cargo: &CargoContext) -> Result<DebugTest, String> {
    if cfg.option("target") == "doctest" || cfg.option("target") == "doc" {
        return Err(
            "Doctests are compiled by rustdoc and cannot be debugged; run them instead".into(),
        );
    }
    let mut c = cfg.clone();
    c.options.insert("runner".into(), "cargo".into());
    c.options.insert("format".into(), "text".into());
    let mut p = plan(&c, &cargo.clone(), None)?;
    // `cargo test --no-run` with the same selection, messages as JSON.
    if let Some(i) = p.args.iter().position(|a| a == "test" || a == "bench") {
        p.args.insert(i + 1, "--no-run".into());
    }
    p.args.retain(|a| a != "--no-fail-fast");
    p.args
        .extend(["--message-format".to_string(), "json".into()]);
    let mut package = cfg.option("package").trim().to_string();
    if package.is_empty() {
        if let Some(pkg) = cargo.package_of(cfg.option("file")) {
            package = pkg.name.clone();
        }
    }
    let (kind, target) = match parse_target(cfg.option("target")) {
        Some((k, n)) => (Some(k), n),
        None => (None, String::new()),
    };
    let mut args = p.test_args;
    args.extend(["--nocapture".to_string(), "--test-threads=1".into()]);
    Ok(DebugTest {
        build_args: p.args,
        package,
        kind,
        target,
        args,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::tests::shop_context;

    fn ctx_with(cargo: &CargoContext, coverage: bool) -> CommandContext {
        let mut ctx = CommandContext {
            coverage_file: coverage.then(|| "/tmp/cov/coverage".into()),
            ..CommandContext::default()
        };
        ctx.settings
            .insert(CONTEXT_KEY.into(), serde_json::to_string(cargo).unwrap());
        ctx
    }

    fn cart() -> TestFile {
        discover_file(
            "crates/shop-core/src/cart.rs",
            include_str!("../tests/fixtures/shop/crates/shop-core/src/cart.rs"),
            Some(&shop_context()),
        )
    }

    #[test]
    fn discovery_keys_match_the_test_tree() {
        let found = cart();
        let rows: Vec<(u32, Kind, &str, &str)> = found
            .targets
            .iter()
            .map(|t| (t.line, t.kind, t.label.as_str(), t.key.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                (
                    30,
                    Kind::Example,
                    "doctest of cart::Cart::total",
                    "crates/shop-core/src/cart.rs::crates/shop-core/src/cart.rs - cart::Cart::total (line 31)"
                ),
                (54, Kind::Suite, "cart::tests", "crates/shop-core/src/cart.rs"),
                (58, Kind::Test, "adds_items", "crates/shop-core/src/cart.rs::cart::tests::adds_items"),
                (66, Kind::Test, "rounds_to_cents", "crates/shop-core/src/cart.rs::cart::tests::rounds_to_cents"),
                (73, Kind::Test, "vouchers", "crates/shop-core/src/cart.rs::cart::tests::vouchers"),
            ]
        );
        let cfg = &found.targets[2].configuration;
        assert_eq!(cfg.option("package"), "shop-core");
        assert_eq!(cfg.option("target"), "lib");
        assert_eq!(found.file.as_ref().unwrap().option("test"), "cart");
        // Without a context the standard layout is assumed.
        let plain = discover_file(
            "crates/shop-core/src/cart.rs",
            include_str!("../tests/fixtures/shop/crates/shop-core/src/cart.rs"),
            None,
        );
        assert_eq!(plain.targets[2].key, found.targets[2].key);
        assert_eq!(locate("tests/checkout.rs", None).1, "test:checkout");
        assert_eq!(
            locate("src/bin/tool/args.rs", None),
            ("".into(), "bin:tool".into(), "args".into())
        );
    }

    #[test]
    fn stable_toolchains_read_plain_output() {
        let cargo = shop_context();
        let provider = CargoTestProvider::standalone();
        let cfg = &cart().targets[3].configuration;
        let cmd = provider
            .command(cfg, &ctx_with(&cargo, false))
            .unwrap()
            .unwrap();
        assert_eq!(cmd.program, "cargo");
        assert_eq!(
            cmd.args,
            [
                "test",
                "--no-fail-fast",
                "-p",
                "shop-core",
                "--lib",
                "--",
                "cart::tests::rounds_to_cents",
                "--exact"
            ]
        );
        assert_eq!(cmd.report, TestReport::LibtestText);
        assert!(cmd
            .location_rules
            .iter()
            .any(|r| r.replacement == "crates/shop-core/src/cart.rs"));
    }

    #[test]
    fn nightly_json_doctests_benches_and_nextest() {
        let mut cargo = shop_context();
        cargo.nightly = true;
        cargo.toolchain = "nightly".into();
        let provider = CargoTestProvider::standalone();
        let doctest = &cart().targets[0].configuration;
        let cmd = provider
            .command(doctest, &ctx_with(&cargo, false))
            .unwrap()
            .unwrap();
        assert_eq!(
            cmd.args,
            [
                "+nightly",
                "test",
                "--no-fail-fast",
                "--doc",
                "-p",
                "shop-core",
                "--",
                "crates/shop-core/src/cart.rs - cart::Cart::total (line 31)",
                "--exact",
                "-Z",
                "unstable-options",
                "--format",
                "json",
                "--report-time"
            ]
        );
        assert_eq!(cmd.report, TestReport::LibtestJson);
        let mut all = Configuration::new("All", CARGO_TEST);
        all.options.insert("kind".into(), "bench".into());
        all.options.insert("features".into(), "discounts".into());
        let cmd = provider
            .command(&all, &ctx_with(&cargo, false))
            .unwrap()
            .unwrap();
        assert_eq!(
            &cmd.args[..5],
            [
                "+nightly",
                "bench",
                "--no-fail-fast",
                "--workspace",
                "--features"
            ]
        );
        cargo.nextest = true;
        cargo.test_runner = "nextest".into();
        all.options.remove("kind");
        let cmd = provider
            .command(&all, &ctx_with(&cargo, false))
            .unwrap()
            .unwrap();
        assert_eq!(
            cmd.args,
            [
                "+nightly",
                "nextest",
                "run",
                "--no-fail-fast",
                "--workspace",
                "--features",
                "discounts",
                "--message-format",
                "libtest-json"
            ]
        );
        assert_eq!(cmd.env["NEXTEST_EXPERIMENTAL_LIBTEST_JSON"], "1");
        // Doctests always go through cargo test.
        let cmd = provider
            .command(doctest, &ctx_with(&cargo, false))
            .unwrap()
            .unwrap();
        assert_eq!(cmd.args[1], "test");
    }

    #[test]
    fn coverage_with_cargo_llvm_cov() {
        let mut cargo = shop_context();
        let provider = CargoTestProvider::standalone();
        let all = Configuration::new("All", CARGO_TEST);
        let missing = provider.command(&all, &ctx_with(&cargo, true)).unwrap();
        assert!(missing.unwrap_err().contains("cargo-llvm-cov"));
        cargo.llvm_cov = true;
        let cmd = provider
            .command(&all, &ctx_with(&cargo, true))
            .unwrap()
            .unwrap();
        assert_eq!(
            cmd.args,
            [
                "llvm-cov",
                "--lcov",
                "--output-path",
                "/tmp/cov/coverage",
                "--no-fail-fast",
                "--workspace"
            ]
        );
        let cov = cmd.coverage.unwrap();
        assert_eq!(
            (cov.file.as_str(), cov.format),
            ("/tmp/cov/coverage", CoverageFormat::Lcov)
        );
        assert!(provider.supports_coverage(CARGO_TEST));
    }

    #[test]
    fn rerun_failed_tests_by_exact_name() {
        let provider = CargoTestProvider::standalone();
        let cfg = &cart().targets[3].configuration;
        let failed = [
            FailedTest {
                name: "cart::tests::rounds_to_cents".into(),
                ..FailedTest::default()
            },
            FailedTest {
                name: "money::tests::discounts_round_half_up".into(),
                ..FailedTest::default()
            },
        ];
        let rerun = provider.rerun_failed(cfg, &failed).unwrap();
        let cmd = provider
            .command(&rerun, &ctx_with(&shop_context(), false))
            .unwrap()
            .unwrap();
        assert_eq!(
            cmd.args,
            [
                "test",
                "--no-fail-fast",
                "-p",
                "shop-core",
                "--",
                "cart::tests::rounds_to_cents",
                "money::tests::discounts_round_half_up",
                "--exact"
            ]
        );
        assert!(provider.rerun_failed(cfg, &[]).is_none());
    }

    #[test]
    fn debugging_builds_the_test_binary() {
        let cargo = shop_context();
        let cfg = &cart().targets[2].configuration;
        let d = debug_test(cfg, &cargo).unwrap();
        assert_eq!(
            d.build_args,
            [
                "test",
                "--no-run",
                "-p",
                "shop-core",
                "--lib",
                "--message-format",
                "json"
            ]
        );
        assert_eq!(
            (d.package.as_str(), d.kind),
            ("shop-core", Some(TargetKind::Lib))
        );
        assert_eq!(
            d.args,
            [
                "cart::tests::adds_items",
                "--exact",
                "--nocapture",
                "--test-threads=1"
            ]
        );
        assert!(debug_test(&cart().targets[0].configuration, &cargo).is_err());
    }
}
