//! The .NET test provider of the shared test runner (`forge-testing`): xUnit, NUnit and
//! MSTest through `dotnet test`. It wraps DotNetForge's `[[contributions.testFrameworks]]`
//! entry (`dotnet-test`):
//!
//! - discovery ([`crate::scan`]): test classes and methods of C# and F# files, keyed
//!   `<file>` and `<file>::<method>` like the test tree's results,
//! - command lines: `dotnet test <solution|project> --filter … --logger trx
//!   --results-directory <report dir>` (VSTest), or `--report-trx` with
//!   Microsoft.Testing.Platform; the TRX report becomes the test tree
//!   (`forge_output::formats::trx`),
//! - Rerun Failed Tests (a `FullyQualifiedName` filter of the failed methods),
//! - the location rules of the test classes ([`DotnetContext::location_rules`]).
//!
//! Configuration options: `scope` (`all`, `project`, `class`, `method`, `filter`),
//! `project`, `class` (full name), `method`, `filter` (a `dotnet test --filter`
//! expression), `tests` (failed tests, `<file>::<method>` per line), `framework`,
//! `configuration`, `noBuild`, `settings` (a `.runsettings` file), `flags` (more `dotnet
//! test` options), `file` (the file the configuration came from); the program arguments
//! go after `--` (RunSettings arguments).

use forge_output::{JUnitFlavor, TestReport};
use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use forge_testing::{
    CommandContext, FailedTest, TargetKind, TestCommand, TestFile, TestFrameworkProvider,
    TestFrameworkSpec, TestTarget,
};

use crate::context::{DotnetContext, CONTEXT_KEY};
use crate::paths::normalize;
use crate::scan::scan_source;

/// The framework id and run configuration type.
pub const DOTNET_TEST: &str = "dotnet-test";

/// The variable that makes the test host wait for a debugger (it prints its process id).
pub const DEBUG_ENV: (&str, &str) = ("VSTEST_HOST_DEBUG", "1");

pub fn is_test_source(path: &str) -> bool {
    let l = path.to_ascii_lowercase();
    l.ends_with(".cs") || l.ends_with(".fs")
}

pub struct DotnetTests {
    spec: TestFrameworkSpec,
}

impl DotnetTests {
    pub fn new(spec: TestFrameworkSpec) -> Option<Self> {
        (spec.id == DOTNET_TEST).then_some(Self { spec })
    }

    /// A provider without a manifest (tests, WebAssembly).
    pub fn standalone() -> Self {
        Self {
            spec: TestFrameworkSpec {
                id: DOTNET_TEST.into(),
                name: ".NET Tests".into(),
                test_files: vec!["*.cs".into(), "*.fs".into()],
                report: TestReport::JUnitFile,
                ..TestFrameworkSpec::default()
            },
        }
    }
}

/// Register the provider for the `dotnet-test` spec among `specs`.
pub fn register(
    registry: &mut forge_testing::TestFrameworks,
    specs: impl IntoIterator<Item = TestFrameworkSpec>,
) {
    for spec in specs {
        if let Some(p) = DotnetTests::new(spec) {
            registry.register(Box::new(p));
        }
    }
}

/// The tests of a source file for the gutter; `ctx` adds the project.
pub fn discover_file(path: &str, text: &str, ctx: Option<&DotnetContext>) -> TestFile {
    let path = normalize(path);
    let project = ctx.and_then(|c| c.project_of_file(&path));
    let hint = project.and_then(|p| p.test_framework);
    let scan = scan_source(&path, text, hint);
    let base = |name: String| {
        let mut c = Configuration::new(name, DOTNET_TEST);
        if let Some(p) = project {
            c.options.insert("project".into(), p.path.clone());
        }
        c.options.insert("file".into(), path.clone());
        c
    };
    let mut targets = Vec::new();
    for class in &scan.classes {
        if class.tests.is_empty() {
            continue;
        }
        let mut c = base(class.name.clone());
        c.options.insert("scope".into(), "class".into());
        c.options.insert("class".into(), class.full_name.clone());
        targets.push(TestTarget {
            line: class.line,
            kind: TargetKind::Class,
            label: class.name.clone(),
            key: path.clone(),
            configuration: c,
        });
        for t in &class.tests {
            let mut c = base(format!("{}.{}", class.name, t.name));
            c.options.insert("scope".into(), "method".into());
            c.options.insert("class".into(), class.full_name.clone());
            c.options.insert("method".into(), t.name.clone());
            targets.push(TestTarget {
                line: t.line,
                kind: TargetKind::Method,
                label: t.name.clone(),
                key: format!("{path}::{}", t.name),
                configuration: c,
            });
        }
    }
    targets.sort_by_key(|t| t.line);
    let classes: Vec<&str> = scan
        .classes
        .iter()
        .filter(|c| !c.tests.is_empty())
        .map(|c| c.full_name.as_str())
        .collect();
    let file = (!classes.is_empty()).then(|| {
        let name = crate::paths::file_name(&path).to_string();
        let mut c = base(format!("Tests in {name}"));
        c.options.insert("scope".into(), "filter".into());
        c.options.insert(
            "filter".into(),
            classes
                .iter()
                .map(|cl| class_filter(cl))
                .collect::<Vec<_>>()
                .join("|"),
        );
        c
    });
    TestFile {
        framework: Some(DOTNET_TEST.into()),
        targets,
        file,
    }
}

/// Escape a value of a `dotnet test --filter` expression.
pub fn filter_escape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for c in v.chars() {
        if "\\()&|=!~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The tests of a class (and its nested classes).
pub fn class_filter(class: &str) -> String {
    let c = filter_escape(class);
    format!("FullyQualifiedName~{c}.|FullyQualifiedName~{c}+")
}

/// A method and its data rows (NUnit names rows `Method(1,2)`).
pub fn method_filter(class: &str, method: &str) -> String {
    let m = filter_escape(&format!("{class}.{method}"));
    format!("FullyQualifiedName={m}|FullyQualifiedName~{m}\\(")
}

fn context_of(ctx: &CommandContext) -> Result<DotnetContext, String> {
    Ok(ctx
        .settings
        .get(CONTEXT_KEY)
        .map(|s| serde_json::from_str(s).map_err(|e| format!("bad .NET context: {e}")))
        .transpose()?
        .unwrap_or_default())
}

/// The `--filter` of a configuration (empty: every test of the target).
pub fn filter_of(cfg: &Configuration, dotnet: &DotnetContext) -> Result<String, String> {
    let opt = |k: &str| cfg.option(k).trim().to_string();
    let scope = match opt("scope") {
        s if s.is_empty() => "all".to_string(),
        s => s,
    };
    Ok(match scope.as_str() {
        "all" | "project" => String::new(),
        "class" => {
            if opt("class").is_empty() {
                return Err("Enter the test class".into());
            }
            class_filter(&opt("class"))
        }
        "method" => {
            if opt("class").is_empty() || opt("method").is_empty() {
                return Err("Enter the test class and method".into());
            }
            method_filter(&opt("class"), &opt("method"))
        }
        "filter" => {
            let tests: Vec<String> = cfg
                .option("tests")
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            if tests.is_empty() {
                if opt("filter").is_empty() {
                    return Err("Enter a test filter (FullyQualifiedName~Cart)".into());
                }
                opt("filter")
            } else {
                let mut parts: Vec<String> = Vec::new();
                for t in tests {
                    // `<file>::<method>` → the classes of the file; else a class-less name.
                    let (file, method) = match t.rsplit_once("::") {
                        Some((f, m)) => (f.to_string(), m.to_string()),
                        None => (String::new(), t.clone()),
                    };
                    let classes = dotnet.classes_in(&file);
                    if classes.is_empty() {
                        parts.push(format!("FullyQualifiedName~.{}", filter_escape(&method)));
                    } else {
                        for c in classes {
                            parts.push(method_filter(c, &method));
                        }
                    }
                }
                parts.dedup();
                parts.join("|")
            }
        }
        other => return Err(format!("Unknown test scope '{other}'")),
    })
}

/// The `dotnet test` arguments of a configuration.
pub fn test_args(cfg: &Configuration, dotnet: &DotnetContext, report_dir: &str) -> Result<Vec<String>, String> {
    let opt = |k: &str| cfg.option(k).trim().to_string();
    let scope = opt("scope");
    let mut args = vec!["test".to_string()];
    let target = match opt("project") {
        p if !p.is_empty() && scope != "all" => dotnet.project(&p).map(|x| x.path.clone()).unwrap_or(p),
        _ if scope == "project" => return Err("Choose the test project".into()),
        _ => dotnet.solution.clone().unwrap_or_default(),
    };
    if !target.is_empty() {
        args.push(target);
    }
    let configuration = match opt("configuration") {
        c if c.is_empty() => dotnet.configuration(),
        c => c,
    };
    args.extend(["-c".to_string(), configuration]);
    if !opt("framework").is_empty() {
        args.extend(["--framework".to_string(), opt("framework")]);
    }
    if opt("noBuild") == "true" {
        args.push("--no-build".into());
    }
    if !opt("settings").is_empty() {
        args.extend(["--settings".to_string(), opt("settings")]);
    }
    let filter = filter_of(cfg, dotnet)?;
    if dotnet.testing_platform {
        // Microsoft.Testing.Platform: TRX through its extension.
        if !filter.is_empty() {
            args.extend(["--filter".to_string(), filter]);
        }
        if !report_dir.is_empty() {
            args.extend(["--results-directory".to_string(), report_dir.to_string(), "--report-trx".into()]);
        }
    } else {
        if !filter.is_empty() {
            args.extend(["--filter".to_string(), filter]);
        }
        if !report_dir.is_empty() {
            args.extend([
                "--logger".to_string(),
                "trx;LogFilePrefix=forge".into(),
                "--results-directory".into(),
                report_dir.to_string(),
            ]);
        }
        args.extend(["--logger".to_string(), "console;verbosity=normal".into()]);
    }
    args.extend(split_args(&opt("flags")));
    let run_settings = split_args(&cfg.arguments);
    if !run_settings.is_empty() {
        args.push("--".into());
        args.extend(run_settings);
    }
    Ok(args)
}

impl DotnetTests {
    fn build(&self, cfg: &Configuration, ctx: &CommandContext) -> Result<TestCommand, String> {
        let dotnet = context_of(ctx)?;
        let args = test_args(cfg, &dotnet, &ctx.report_dir)?;
        Ok(TestCommand {
            program: dotnet.dotnet(),
            args,
            env: dotnet.environment(),
            report: TestReport::JUnitFile,
            flavor: JUnitFlavor::Generic,
            location_rules: dotnet.location_rules(),
            default_file: None,
            coverage: None,
            report_dirs: Vec::new(),
        })
    }

    fn rerun(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
        let mut tests: Vec<String> = Vec::new();
        for f in failed {
            let method = f
                .member
                .clone()
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| f.name.split('(').next().unwrap_or(&f.name).trim().to_string());
            if method.is_empty() {
                continue;
            }
            let key = match &f.path {
                Some(p) if !p.is_empty() => format!("{p}::{method}"),
                _ => method,
            };
            if !tests.contains(&key) {
                tests.push(key);
            }
        }
        if tests.is_empty() {
            return None;
        }
        let mut c = cfg.clone();
        c.options.insert("scope".into(), "filter".into());
        c.options.insert("tests".into(), tests.join("\n"));
        for k in ["class", "method", "filter"] {
            c.options.remove(k);
        }
        c.name = format!("Rerun failed: {}", cfg.name);
        Some(c)
    }
}

impl TestFrameworkProvider for DotnetTests {
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
        is_test_source(path)
    }

    fn discover(&self, path: &str, text: &str) -> Option<TestFile> {
        let found = discover_file(path, text, None);
        (!found.is_empty()).then_some(found)
    }

    fn rerun_failed(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
        self.rerun(cfg, failed)
    }

    fn command(&self, cfg: &Configuration, ctx: &CommandContext) -> Option<Result<TestCommand, String>> {
        Some(self.build(cfg, ctx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{bookstore_files, fixture, fixture_reader};
    use crate::workspace::load_workspace;

    fn dotnet() -> DotnetContext {
        let ws = load_workspace(&bookstore_files(), &fixture_reader("bookstore"), "", "");
        let mut c = DotnetContext::default().with_workspace(&ws);
        c.classes.insert("Bookstore.Tests.CartTests".into(), "tests/Bookstore.Tests/CartTests.cs".into());
        c.classes.insert("Bookstore.Tests.CartTests+WhenEmpty".into(), "tests/Bookstore.Tests/CartTests.cs".into());
        c
    }

    #[test]
    fn discovers_targets() {
        let ctx = dotnet();
        let path = "tests/Bookstore.Tests/CartTests.cs";
        let f = discover_file(path, &fixture(&format!("bookstore/{path}")), Some(&ctx));
        assert_eq!(f.framework.as_deref(), Some(DOTNET_TEST));
        let class = &f.targets[0];
        assert_eq!((class.kind, class.key.as_str(), class.line), (TargetKind::Class, path, 6));
        assert_eq!(class.configuration.option("project"), "tests/Bookstore.Tests/Bookstore.Tests.csproj");
        let m = f.targets.iter().find(|t| t.label == "Total_AddsLines").unwrap();
        assert_eq!(m.key, format!("{path}::Total_AddsLines"));
        assert_eq!(m.configuration.option("class"), "Bookstore.Tests.CartTests");
        assert_eq!(m.configuration.name, "CartTests.Total_AddsLines");
        let file = f.file.unwrap();
        assert_eq!(
            file.option("filter"),
            "FullyQualifiedName~Bookstore.Tests.CartTests.|FullyQualifiedName~Bookstore.Tests.CartTests+|FullyQualifiedName~Bookstore.Tests.CartTests+WhenEmpty.|FullyQualifiedName~Bookstore.Tests.CartTests+WhenEmpty+"
        );
        assert!(DotnetTests::standalone().discover("src/Bookstore.Core/Catalog/Cart.cs", &fixture("bookstore/src/Bookstore.Core/Catalog/Cart.cs")).is_none());
    }

    #[test]
    fn command_lines() {
        let dotnet = dotnet();
        let mut ctx = CommandContext {
            report_dir: "/tmp/r".into(),
            ..CommandContext::default()
        };
        ctx.settings.insert(CONTEXT_KEY.into(), serde_json::to_string(&dotnet).unwrap());
        let p = DotnetTests::standalone();
        let mut cfg = Configuration::new("All tests", DOTNET_TEST);
        let all = p.command(&cfg, &ctx).unwrap().unwrap();
        assert_eq!(all.program, "dotnet");
        assert_eq!(
            all.args,
            vec!["test", "Bookstore.sln", "-c", "Debug", "--logger", "trx;LogFilePrefix=forge", "--results-directory", "/tmp/r", "--logger", "console;verbosity=normal"]
        );
        assert_eq!(all.report, TestReport::JUnitFile);
        assert_eq!(all.location_rules.len(), 2);
        cfg.options.insert("scope".into(), "method".into());
        cfg.options.insert("project".into(), "tests/Bookstore.Tests/Bookstore.Tests.csproj".into());
        cfg.options.insert("class".into(), "Bookstore.Tests.CartTests".into());
        cfg.options.insert("method".into(), "Total_AddsLines".into());
        let m = p.command(&cfg, &ctx).unwrap().unwrap();
        assert_eq!(m.args[1], "tests/Bookstore.Tests/Bookstore.Tests.csproj");
        let f = m.args.iter().position(|a| a == "--filter").unwrap();
        assert_eq!(
            m.args[f + 1],
            "FullyQualifiedName=Bookstore.Tests.CartTests.Total_AddsLines|FullyQualifiedName~Bookstore.Tests.CartTests.Total_AddsLines\\("
        );
        cfg.options.insert("scope".into(), "class".into());
        cfg.options.remove("class");
        assert!(p.command(&cfg, &ctx).unwrap().is_err());

        // Rerun Failed Tests.
        let rerun = p
            .rerun_failed(
                &cfg,
                &[
                    FailedTest { name: "Total_AddsLines".into(), member: Some("Total_AddsLines".into()), path: Some("tests/Bookstore.Tests/CartTests.cs".into()) },
                    FailedTest { name: "Add_RejectsNonPositiveQuantities(quantity: 0)".into(), member: None, path: None },
                ],
            )
            .unwrap();
        assert_eq!(rerun.option("scope"), "filter");
        let args = test_args(&rerun, &dotnet, "").unwrap();
        let f = args.iter().position(|a| a == "--filter").unwrap();
        assert!(args[f + 1].starts_with("FullyQualifiedName=Bookstore.Tests.CartTests.Total_AddsLines|"));
        assert!(args[f + 1].contains("FullyQualifiedName=Bookstore.Tests.CartTests+WhenEmpty.Total_AddsLines"));
        assert!(args[f + 1].ends_with("FullyQualifiedName~.Add_RejectsNonPositiveQuantities"));
        assert!(!args.contains(&"--results-directory".to_string()));
        assert!(p.rerun_failed(&cfg, &[]).is_none());

        // Microsoft.Testing.Platform.
        let mut mtp = dotnet.clone();
        mtp.testing_platform = true;
        let a = test_args(&Configuration::new("x", DOTNET_TEST), &mtp, "/tmp/r").unwrap();
        assert!(a.ends_with(&["--results-directory".to_string(), "/tmp/r".into(), "--report-trx".into()]));
        assert_eq!(filter_escape("A(1)|B"), "A\\(1\\)\\|B");
    }
}
