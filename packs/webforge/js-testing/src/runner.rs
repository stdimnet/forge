//! The Node.js test runners: Jest, Vitest, Mocha, Playwright Test, Cypress and the
//! built-in `node --test`.
//!
//! WebForge declares each runner as a `[[contributions.testFrameworks]]` entry (detection,
//! test file globs, the arguments per scope, the report format, Rerun Failed Tests and
//! coverage). [`JsTestRunner`] wraps that spec with what only Node.js runners need:
//!
//! - the runner's CLI script runs with Node.js (`node node_modules/jest/bin/jest.js …`), so
//!   no `node_modules/.bin` shim or `npx` is involved and the debugger can launch the same
//!   program; the options `executable` (Node interpreter), `nodeOptions` and `package`
//!   (the runner's package folder) change it,
//! - gutter discovery that follows the calls (`describe` / `it` / `test`, `test.describe`),
//! - Playwright's JSON report file (`PLAYWRIGHT_JSON_OUTPUT_NAME`), Mocha coverage through
//!   c8 or nyc ([`coverage_wrap`], also Run with Coverage of Node.js configurations), the
//!   coverage provider of Vitest (v8 / istanbul) and Jest (babel / v8), options before file
//!   arguments for `node --test`,
//! - Rerun Failed Tests also limits the run to the failed tests' files,
//! - the program and arguments of Debug ([`JsTestRunner::debug_program`]).

use std::collections::BTreeMap;

use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use forge_testing::{
    CommandContext, FailedTest, TargetKind, TestCommand, TestFile, TestFrameworkProvider,
    TestFrameworkSpec, TestTarget,
};
use serde::{Deserialize, Serialize};

use crate::discovery;

/// A Node.js test runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Runner {
    Jest,
    Vitest,
    Mocha,
    Playwright,
    Cypress,
    /// `node --test`.
    NodeTest,
}

impl Runner {
    pub const ALL: [Runner; 6] = [
        Runner::Jest,
        Runner::Vitest,
        Runner::Mocha,
        Runner::Playwright,
        Runner::Cypress,
        Runner::NodeTest,
    ];

    /// The test framework id (also the run configuration type).
    pub fn id(self) -> &'static str {
        match self {
            Runner::Jest => "jest",
            Runner::Vitest => "vitest",
            Runner::Mocha => "mocha",
            Runner::Playwright => "playwright",
            Runner::Cypress => "cypress",
            Runner::NodeTest => "node-test",
        }
    }

    pub fn from_id(id: &str) -> Option<Runner> {
        Runner::ALL.into_iter().find(|r| r.id() == id)
    }

    /// The npm package with the CLI; `None` for `node --test`.
    pub fn package(self) -> Option<&'static str> {
        match self {
            Runner::Jest => Some("jest"),
            Runner::Vitest => Some("vitest"),
            Runner::Mocha => Some("mocha"),
            Runner::Playwright => Some("@playwright/test"),
            Runner::Cypress => Some("cypress"),
            Runner::NodeTest => None,
        }
    }

    /// The CLI script inside the package.
    pub fn bin(self) -> &'static str {
        match self {
            Runner::Jest => "bin/jest.js",
            Runner::Vitest => "vitest.mjs",
            Runner::Mocha => "bin/mocha.js",
            Runner::Playwright => "cli.js",
            Runner::Cypress => "bin/cypress",
            Runner::NodeTest => "",
        }
    }

    /// Arguments that make a run debuggable: one process, no timeouts.
    pub fn debug_args(self) -> &'static [&'static str] {
        match self {
            Runner::Jest => &["--runInBand", "--testTimeout=999999"],
            Runner::Vitest => &["--no-file-parallelism", "--test-timeout=0"],
            Runner::Mocha => &["--no-timeouts"],
            Runner::Playwright => &["--workers=1", "--timeout=0"],
            Runner::Cypress | Runner::NodeTest => &[],
        }
    }

    /// Whether the tests run in Node.js (Cypress tests run in the browser it drives).
    pub fn debuggable(self) -> bool {
        self != Runner::Cypress
    }
}

/// A runner's spec from the manifest with the Node.js specifics.
#[derive(Debug, Clone)]
pub struct JsTestRunner {
    pub runner: Runner,
    pub spec: TestFrameworkSpec,
}

/// What Debug starts: the runner's CLI script with Node.js under the debugger.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeProgram {
    /// The Node.js executable (`runtimeExecutable`).
    pub runtime: String,
    /// Node options (`runtimeArgs`).
    pub runtime_args: Vec<String>,
    /// The script, relative to the working directory unless absolute.
    pub program: String,
    pub args: Vec<String>,
    /// The configuration's working directory (empty: the project root).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cwd: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

fn join(dir: &str, file: &str) -> String {
    format!("{}/{file}", dir.trim_end_matches(['/', '\\']))
}

/// Options first, then file arguments (`node --test` reads everything after the first file
/// as files).
fn options_first(args: Vec<String>) -> Vec<String> {
    let (options, files): (Vec<String>, Vec<String>) =
        args.into_iter().partition(|a| a.starts_with('-'));
    options.into_iter().chain(files).collect()
}

impl JsTestRunner {
    /// The wrapper of a manifest spec whose id names a Node.js runner.
    pub fn new(spec: TestFrameworkSpec) -> Option<Self> {
        let runner = Runner::from_id(&spec.id)?;
        Some(Self { runner, spec })
    }

    /// The runner's CLI script: `<package option or node_modules/<package>>/<bin>`.
    pub fn script(&self, cfg: &Configuration) -> Option<String> {
        let package = self.runner.package()?;
        let dir = match cfg.option("package").trim() {
            "" => format!("node_modules/{package}"),
            p => p.to_string(),
        };
        // A package option may name the script itself.
        if dir.ends_with(".js") || dir.ends_with(".mjs") || dir.ends_with(".cjs") {
            return Some(dir);
        }
        Some(join(&dir, self.runner.bin()))
    }

    fn files(cfg: &Configuration) -> Vec<String> {
        cfg.option("files")
            .lines()
            .map(str::trim)
            .filter(|f| !f.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// The runner's own arguments (without Node.js and the script).
    fn runner_args(
        &self,
        cfg: &Configuration,
        ctx: &CommandContext,
    ) -> Result<TestCommand, String> {
        let mut built = self.spec.build(cfg, ctx)?;
        // Rerun Failed Tests: only the failed tests' files.
        let files = Self::files(cfg);
        if !files.is_empty() {
            match self.runner {
                Runner::Cypress => built.args.extend(["--spec".to_string(), files.join(",")]),
                _ => built.args.extend(files),
            }
        }
        let config = cfg.option("configFile").trim();
        if !config.is_empty() {
            let flag = match self.runner {
                Runner::Cypress => "--config-file",
                Runner::NodeTest => "",
                _ => "--config",
            };
            if !flag.is_empty() {
                built.args.extend([flag.to_string(), config.to_string()]);
            }
        }
        if self.runner == Runner::Playwright && !ctx.report_dir.is_empty() {
            built.env.insert(
                "PLAYWRIGHT_JSON_OUTPUT_NAME".into(),
                join(&ctx.report_dir, "playwright.json"),
            );
        }
        // The coverage provider: Vitest's v8 or istanbul, Jest's babel or v8.
        let provider = cfg.option("coverageProvider").trim();
        if ctx.coverage_file.is_some() && !provider.is_empty() {
            match self.runner {
                Runner::Vitest => built.args.push(format!("--coverage.provider={provider}")),
                Runner::Jest => built.args.push(format!("--coverageProvider={provider}")),
                _ => {}
            }
        }
        if self.runner == Runner::NodeTest {
            built.args = options_first(std::mem::take(&mut built.args));
        }
        Ok(built)
    }

    /// The command of a run: `node [node options] <script> <runner arguments>`.
    pub fn build(&self, cfg: &Configuration, ctx: &CommandContext) -> Result<TestCommand, String> {
        let mut built = self.runner_args(cfg, ctx)?;
        let node = built.program.clone();
        let mut args: Vec<String> = split_args(cfg.option("nodeOptions"));
        if let Some(script) = self.script(cfg) {
            args.push(script);
        }
        args.append(&mut built.args);
        // Mocha has no coverage of its own: c8 or nyc runs it (`c8 --reporter=lcov node mocha …`).
        if self.runner == Runner::Mocha {
            if let Some(dir) = ctx.coverage_file.as_deref() {
                args = coverage_wrap(cfg, &node, dir, args);
            }
        }
        built.program = node;
        built.args = args;
        Ok(built)
    }

    /// The program Debug starts under js-debug: the same command with the runner's
    /// single-process, no-timeout arguments.
    pub fn debug_program(
        &self,
        cfg: &Configuration,
        ctx: &CommandContext,
    ) -> Result<NodeProgram, String> {
        if !self.runner.debuggable() {
            return Err(
                "Cypress tests run in the browser: use Open Cypress and debug them in its browser's DevTools"
                    .into(),
            );
        }
        let ctx = CommandContext {
            coverage_file: None,
            ..ctx.clone()
        };
        let built = self.runner_args(cfg, &ctx)?;
        let mut args = built.args;
        let extra: Vec<String> = self
            .runner
            .debug_args()
            .iter()
            .map(|a| a.to_string())
            .collect();
        match self.runner {
            // `vitest run …`: options after the command.
            Runner::Vitest => {
                let at = usize::from(args.first().is_some_and(|a| a == "run"));
                args.splice(at..at, extra);
            }
            Runner::Playwright => {
                let at = usize::from(args.first().is_some_and(|a| a == "test"));
                args.splice(at..at, extra);
            }
            _ => args.splice(0..0, extra).for_each(drop),
        }
        let mut runtime_args = split_args(cfg.option("nodeOptions"));
        let program = match self.script(cfg) {
            Some(s) => s,
            None => {
                // `node --test file`: Node.js itself is the program; the first file is the
                // script js-debug launches, the options go to Node.js.
                let (options, files): (Vec<String>, Vec<String>) =
                    args.drain(..).partition(|a| a.starts_with('-'));
                runtime_args.extend(options);
                args = files;
                if args.is_empty() {
                    return Err("Debug a test file or a test of node --test".into());
                }
                args.remove(0)
            }
        };
        Ok(NodeProgram {
            runtime: built.program,
            runtime_args,
            program,
            args,
            cwd: cfg.working_directory.clone(),
            env: built.env,
        })
    }

    /// Whether a test file that the runner's globs match is the runner's: Playwright files
    /// import `@playwright/test` (or live in `e2e/`), `node --test` files import
    /// `node:test`, and unit test runners leave those, Cypress specs and files that import
    /// another runner alone.
    pub fn claims(&self, path: &str, text: &str) -> bool {
        let p = path.replace('\\', "/");
        let playwright = text.contains("@playwright/test");
        let cypress = p.starts_with("cypress/") || p.contains("/cypress/") || p.contains(".cy.");
        let imports = |module: &str| {
            text.contains(&format!("from \"{module}\""))
                || text.contains(&format!("from '{module}'"))
                || text.contains(&format!("require(\"{module}\")"))
                || text.contains(&format!("require('{module}')"))
        };
        let node_test = imports("node:test");
        let other_unit = |own: Runner| {
            [
                (Runner::Vitest, imports("vitest")),
                (Runner::Jest, imports("@jest/globals")),
                (Runner::Mocha, imports("mocha")),
            ]
            .into_iter()
            .any(|(r, used)| r != own && used)
        };
        match self.runner {
            Runner::Playwright => {
                playwright || ((p.starts_with("e2e/") || p.contains("/e2e/")) && !cypress)
            }
            Runner::Cypress => !playwright,
            Runner::NodeTest => node_test,
            unit => !playwright && !cypress && !node_test && !other_unit(unit),
        }
    }

    /// Gutter targets of a test file.
    pub fn discover_tests(&self, path: &str, text: &str) -> TestFile {
        if !self.spec.is_test_file(path) {
            return TestFile::default();
        }
        let id = self.spec.id.as_str();
        let file_name = path.rsplit('/').next().unwrap_or(path);
        let file_cfg = || {
            Configuration::new(file_name, id)
                .with_option("scope", "file")
                .with_option("file", path)
        };
        let targets: Vec<TestTarget> = discovery::discover(text)
            .into_iter()
            .map(|f| {
                let configuration = Configuration::new(f.name.as_str(), id)
                    .with_option("scope", "test")
                    .with_option("file", path)
                    .with_option("test", f.full_name.as_str());
                let key = if f.kind == TargetKind::Test {
                    format!("{path}::{}", f.full_name)
                } else {
                    String::new()
                };
                TestTarget {
                    line: f.line,
                    kind: f.kind,
                    label: f.name,
                    key,
                    configuration,
                }
            })
            .collect();
        TestFile {
            framework: Some(id.to_string()),
            targets,
            file: Some(file_cfg()),
        }
    }

    /// Rerun Failed Tests: the spec's name pattern, limited to the failed tests' files.
    pub fn rerun(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
        let mut files: Vec<String> = Vec::new();
        for f in failed {
            if let Some(p) = f.path.as_deref().filter(|p| !p.is_empty()) {
                if !files.iter().any(|x| x == p) {
                    files.push(p.to_string());
                }
            }
        }
        let mut c = match self.spec.rerun(cfg, failed) {
            Some(c) => c,
            // Cypress filters no test names: rerun the failed files.
            None if !files.is_empty() => {
                let mut c = cfg.clone();
                c.options.insert("scope".into(), "all".into());
                c
            }
            None => return None,
        };
        // The failed files replace a file or directory scope.
        if !files.is_empty() {
            c.options.remove("file");
            c.options.remove("directory");
            c.options.insert("files".into(), files.join("\n"));
        }
        Some(c)
    }
}

impl TestFrameworkProvider for JsTestRunner {
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
        self.spec.is_test_file(path)
    }
    fn discover(&self, path: &str, text: &str) -> Option<TestFile> {
        if !self.claims(path, text) {
            return None;
        }
        Some(self.discover_tests(path, text)).filter(|f| !f.targets.is_empty())
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
        self.spec.coverage.is_some()
    }
}

/// The coverage tool that runs a Node.js program (option `coverageTool`): c8 (V8's own
/// coverage, the default) or nyc (Istanbul instrumentation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageTool {
    C8,
    Nyc,
}

impl CoverageTool {
    pub fn from_option(value: &str) -> Self {
        match value.trim() {
            "nyc" => CoverageTool::Nyc,
            _ => CoverageTool::C8,
        }
    }
}

/// Run with Coverage of a Node.js program that has no coverage of its own (Mocha, a Node.js
/// configuration): `<node> [options] <program …>` becomes
/// `<c8.js> --reporter=lcov --reports-dir=<dir> <node> [options] <program …>` (nyc:
/// `<nyc.js> --reporter=lcov --report-dir=<dir> …`), run with Node.js; the LCOV report is
/// `<dir>/lcov.info`. The tool comes from the project's `node_modules` or the
/// `c8Package` / `nycPackage` option.
pub fn coverage_wrap(cfg: &Configuration, node: &str, dir: &str, args: Vec<String>) -> Vec<String> {
    let tool = CoverageTool::from_option(cfg.option("coverageTool"));
    let (option, package, bin, dir_flag) = match tool {
        CoverageTool::C8 => ("c8Package", "node_modules/c8", "bin/c8.js", "--reports-dir"),
        CoverageTool::Nyc => (
            "nycPackage",
            "node_modules/nyc",
            "bin/nyc.js",
            "--report-dir",
        ),
    };
    let script = match cfg.option(option).trim() {
        "" => join(package, bin),
        p if p.ends_with(".js") => p.to_string(),
        p => join(p, bin),
    };
    let mut wrapped = vec![
        script,
        "--reporter=lcov".to_string(),
        format!("{dir_flag}={dir}"),
        node.to_string(),
    ];
    wrapped.extend(args);
    wrapped
}

/// Wrap the Node.js runners among manifest specs and register them (replacing the plain
/// spec providers of the same id).
pub fn register(
    registry: &mut forge_testing::TestFrameworks,
    specs: impl IntoIterator<Item = TestFrameworkSpec>,
) {
    for spec in specs {
        if let Some(r) = JsTestRunner::new(spec) {
            registry.register(Box::new(r));
        }
    }
}
