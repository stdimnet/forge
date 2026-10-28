//! RSpec and Minitest for the shared test runner (`forge-testing`). The provider wraps the
//! manifest's `rspec` / `minitest` entries (run configuration forms, test file globs) with
//! what only Ruby needs:
//!
//! - discovery ([`crate::discovery`]): ▶ on example groups, examples, test classes, test
//!   methods and Minitest::Spec blocks, keyed `<file>::<full description>` (RSpec) or
//!   `<file>::<test method>` (Minitest) like the test tree's results;
//! - RSpec: `rspec [file[:line] | dir] [-e example]… --format progress --format json --out
//!   <report dir>/rspec.json` (results read by `forge-output`'s RSpec parser);
//! - Minitest: `ruby -Ilib -Itest <file> -v [-n <filter>]`, or a loader (`-e`) that
//!   requires every `*_test.rb` / `test_*.rb` of a folder; results from the verbose output
//!   (`forge-output`'s Minitest parser);
//! - Rerun Failed Tests (`-e` per failed example, `-n /^(a|b)$/` for failed methods);
//! - `bundle exec` and the interpreter from a [`RubyContext`].
//!
//! Configuration options: `scope` (`all`, `directory`, `file`, `test`, `pattern`),
//! `directory`, `file`, `line` (RSpec: run the example or group at that line), `test` (RSpec:
//! full description; Minitest: method), `class` (Minitest: the test class), `pattern`,
//! `examples` / `files` (reruns, one per line), `executable`, `testRunnerOptions`.

use forge_output::{JUnitFlavor, TestReport};
use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use forge_testing::{
    CommandContext, FailedTest, TargetKind, TestCommand, TestFile, TestFrameworkProvider,
    TestFrameworkSpec, TestTarget,
};

use crate::context::{RubyCommand, RubyContext, CONTEXT_KEY};
use crate::discovery::{
    discover_minitest, discover_rspec, is_spec_file, is_test_file, RubyTestKind,
};

pub const RSPEC: &str = "rspec";
pub const MINITEST: &str = "minitest";
/// The report RSpec writes into the run's report folder.
pub const RSPEC_REPORT: &str = "rspec.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    RSpec,
    Minitest,
}

/// The RSpec or Minitest provider.
pub struct RubyTests {
    kind: Kind,
    spec: TestFrameworkSpec,
}

impl RubyTests {
    /// Wrap a manifest spec (`rspec` or `minitest`); `None` for other specs.
    pub fn new(spec: TestFrameworkSpec) -> Option<Self> {
        let kind = match spec.id.as_str() {
            RSPEC => Kind::RSpec,
            MINITEST => Kind::Minitest,
            _ => return None,
        };
        Some(Self { kind, spec })
    }

    /// RSpec without a manifest (tests, WebAssembly).
    pub fn rspec() -> Self {
        Self {
            kind: Kind::RSpec,
            spec: TestFrameworkSpec {
                id: RSPEC.into(),
                name: "RSpec".into(),
                test_files: vec!["*_spec.rb".into()],
                report: TestReport::JUnitFile,
                ..TestFrameworkSpec::default()
            },
        }
    }

    /// Minitest without a manifest (tests, WebAssembly).
    pub fn minitest() -> Self {
        Self {
            kind: Kind::Minitest,
            spec: TestFrameworkSpec {
                id: MINITEST.into(),
                name: "Minitest".into(),
                test_files: vec!["*_test.rb".into(), "test_*.rb".into()],
                report: TestReport::MinitestText,
                ..TestFrameworkSpec::default()
            },
        }
    }
}

/// Register RSpec and Minitest for their manifest specs among `specs`.
pub fn register(
    registry: &mut forge_testing::TestFrameworks,
    specs: impl IntoIterator<Item = TestFrameworkSpec>,
) {
    for spec in specs {
        if let Some(p) = RubyTests::new(spec) {
            registry.register(Box::new(p));
        }
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Escape text for a Ruby regular expression literal (`/…/`).
pub fn ruby_regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\^$.|?*+()[]{}/-".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn context_of(ctx: &CommandContext) -> Result<RubyContext, String> {
    ctx.settings
        .get(CONTEXT_KEY)
        .map(|s| serde_json::from_str(s).map_err(|e| format!("bad Ruby context: {e}")))
        .transpose()
        .map(Option::unwrap_or_default)
}

fn lines(s: &str) -> Vec<String> {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// The `-e` code that requires the test files of a folder (Minitest's autorun runs them).
pub fn minitest_loader(dir: &str) -> String {
    let dir = dir.trim_matches('/');
    let base = if dir.is_empty() {
        "test".to_string()
    } else {
        dir.to_string()
    };
    let base = base.replace('\'', "\\'");
    format!(
        "['{base}/**/*_test.rb', '{base}/**/test_*.rb'].flat_map {{ |g| Dir.glob(g) }}.uniq.sort.each {{ |f| require File.expand_path(f) }}"
    )
}

/// The command of an RSpec configuration; `report_dir` receives `rspec.json`.
pub fn rspec_command(
    cfg: &Configuration,
    ruby: &RubyContext,
    report_dir: &str,
) -> Result<RubyCommand, String> {
    let opt = |k: &str| cfg.option(k).trim().to_string();
    let scope = match opt("scope") {
        s if s.is_empty() => "all".to_string(),
        s => s,
    };
    let file = opt("file").replace('\\', "/");
    let mut args: Vec<String> = Vec::new();
    match scope.as_str() {
        "all" => {}
        "directory" => {
            let d = opt("directory");
            if d.is_empty() {
                return Err("Choose the spec folder".into());
            }
            args.push(d);
        }
        "file" => {
            if file.is_empty() {
                return Err("Choose the spec file".into());
            }
            args.push(file.clone());
        }
        "test" => {
            if file.is_empty() {
                return Err("Choose the spec file".into());
            }
            let line = opt("line");
            if !line.is_empty() {
                args.push(format!("{file}:{line}"));
            } else {
                let test = opt("test");
                if test.is_empty() {
                    return Err("Enter the example's description".into());
                }
                args.push(file.clone());
                args.push("--example".into());
                args.push(test);
            }
        }
        "pattern" => {
            let examples = lines(&opt("examples"));
            if examples.is_empty() {
                let pattern = opt("pattern");
                if pattern.is_empty() {
                    return Err("Enter a part of the examples' descriptions".into());
                }
                args.push("--example".into());
                args.push(pattern);
            } else {
                for e in examples {
                    args.push("--example".into());
                    args.push(e);
                }
            }
            args.extend(lines(&opt("files")));
            if !opt("directory").is_empty() {
                args.push(opt("directory"));
            }
        }
        other => return Err(format!("Unknown test scope '{other}'")),
    }
    args.extend(split_args(&opt("testRunnerOptions")));
    args.extend([
        "--format".into(),
        "progress".into(),
        "--format".into(),
        "json".into(),
        "--out".into(),
        format!(
            "{}/{RSPEC_REPORT}",
            report_dir.trim_end_matches(['/', '\\'])
        ),
    ]);
    Ok(ruby.gem_command("rspec", &opt("executable"), args))
}

/// The command of a Minitest configuration.
pub fn minitest_command(cfg: &Configuration, ruby: &RubyContext) -> Result<RubyCommand, String> {
    let opt = |k: &str| cfg.option(k).trim().to_string();
    let scope = match opt("scope") {
        s if s.is_empty() => "all".to_string(),
        s => s,
    };
    let file = opt("file").replace('\\', "/");
    let mut args: Vec<String> = vec!["-Ilib".into(), "-Itest".into()];
    let mut filter: Option<String> = None;
    let loader = |dir: &str, args: &mut Vec<String>| {
        args.push("-e".into());
        args.push(minitest_loader(dir));
        args.push("--".into());
    };
    match scope.as_str() {
        "all" => loader("test", &mut args),
        "directory" => {
            let d = opt("directory");
            if d.is_empty() {
                return Err("Choose the test folder".into());
            }
            loader(&d, &mut args);
        }
        "file" => {
            if file.is_empty() {
                return Err("Choose the test file".into());
            }
            args.push(file.clone());
        }
        "test" => {
            if file.is_empty() {
                return Err("Choose the test file".into());
            }
            args.push(file.clone());
            let class = opt("class");
            let test = opt("test");
            filter = Some(match (class.is_empty(), test.is_empty()) {
                (true, true) => return Err("Enter the test method".into()),
                (false, true) => format!("/^{}#/", ruby_regex_escape(&class)),
                (true, false) => test,
                (false, false) => format!("{class}#{test}"),
            });
        }
        "pattern" => {
            let pattern = opt("pattern");
            if pattern.is_empty() {
                return Err("Enter a test name pattern".into());
            }
            let files = lines(&opt("files"));
            match files.as_slice() {
                [] => loader(&opt("directory"), &mut args),
                [one] => args.push(one.clone()),
                many => {
                    let list = many
                        .iter()
                        .map(|f| format!("'{}'", f.replace('\'', "\\'")))
                        .collect::<Vec<_>>()
                        .join(", ");
                    args.push("-e".into());
                    args.push(format!(
                        "[{list}].each {{ |f| require File.expand_path(f) }}"
                    ));
                    args.push("--".into());
                }
            }
            filter = Some(if pattern.starts_with('/') {
                pattern
            } else {
                format!("/{pattern}/")
            });
        }
        other => return Err(format!("Unknown test scope '{other}'")),
    }
    args.push("-v".into());
    if let Some(f) = filter {
        args.push("-n".into());
        args.push(f);
    }
    args.extend(split_args(&opt("testRunnerOptions")));
    let executable = opt("executable");
    if !executable.is_empty() {
        let mut c = ruby.ruby_command(args, false);
        c.program = executable;
        return Ok(c);
    }
    Ok(ruby.ruby_command(args, true))
}

impl RubyTests {
    fn base(&self, name: String, path: &str) -> Configuration {
        let mut c = Configuration::new(name, self.spec.id.clone());
        c.options.insert("file".into(), path.to_string());
        c
    }

    fn discover_rspec(&self, path: &str, text: &str) -> TestFile {
        let found = discover_rspec(text);
        let mut targets = Vec::new();
        for t in &found {
            let mut c = self.base(
                if t.full_name.is_empty() {
                    format!("{}:{}", file_name(path), t.line + 1)
                } else {
                    t.full_name.clone()
                },
                path,
            );
            c.options.insert("scope".into(), "test".into());
            c.options.insert("line".into(), (t.line + 1).to_string());
            c.options.insert("test".into(), t.full_name.clone());
            let group = t.kind == RubyTestKind::Group;
            targets.push(TestTarget {
                line: t.line,
                kind: if group {
                    TargetKind::Describe
                } else {
                    TargetKind::Test
                },
                label: if t.name.is_empty() {
                    t.full_name.clone()
                } else {
                    t.name.clone()
                },
                key: if group && t.depth == 0 {
                    path.to_string()
                } else {
                    format!("{path}::{}", t.full_name)
                },
                configuration: c,
            });
        }
        let file = (!found.is_empty()).then(|| {
            let mut c = self.base(file_name(path).trim_end_matches(".rb").to_string(), path);
            c.options.insert("scope".into(), "file".into());
            c
        });
        TestFile {
            framework: Some(self.spec.id.clone()),
            targets,
            file,
        }
    }

    fn discover_minitest(&self, path: &str, text: &str) -> TestFile {
        let found = discover_minitest(text);
        let mut targets = Vec::new();
        for t in &found {
            let group = t.kind == RubyTestKind::Group;
            let mut c = self.base(
                if group {
                    t.class.clone()
                } else {
                    format!("{}.{}", t.class, t.name)
                },
                path,
            );
            c.options.insert("scope".into(), "test".into());
            c.options.insert("class".into(), t.class.clone());
            if !group {
                c.options.insert("test".into(), t.full_name.clone());
            }
            targets.push(TestTarget {
                line: t.line,
                kind: if group {
                    TargetKind::Class
                } else {
                    TargetKind::Method
                },
                label: if group {
                    t.class.clone()
                } else {
                    t.name.clone()
                },
                key: if group {
                    path.to_string()
                } else {
                    format!("{path}::{}", t.full_name)
                },
                configuration: c,
            });
        }
        let file = (!found.is_empty()).then(|| {
            let mut c = self.base(file_name(path).trim_end_matches(".rb").to_string(), path);
            c.options.insert("scope".into(), "file".into());
            c
        });
        TestFile {
            framework: Some(self.spec.id.clone()),
            targets,
            file,
        }
    }

    fn rerun(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
        let mut c = cfg.clone();
        let mut files: Vec<String> = Vec::new();
        for f in failed {
            if let Some(p) = f.path.as_deref().filter(|p| p.ends_with(".rb")) {
                if !files.iter().any(|x| x == p) {
                    files.push(p.to_string());
                }
            }
        }
        match self.kind {
            Kind::RSpec => {
                let mut examples: Vec<String> = Vec::new();
                for f in failed {
                    let e = f
                        .member
                        .clone()
                        .filter(|m| !m.is_empty())
                        .unwrap_or_else(|| f.name.clone());
                    if !e.is_empty() && !examples.contains(&e) {
                        examples.push(e);
                    }
                }
                if examples.is_empty() {
                    return None;
                }
                c.options.insert("examples".into(), examples.join("\n"));
            }
            Kind::Minitest => {
                let mut names: Vec<String> = Vec::new();
                for f in failed {
                    let m = f
                        .member
                        .clone()
                        .filter(|m| m.starts_with("test_"))
                        .unwrap_or_else(|| f.name.clone());
                    if !m.is_empty() && !names.contains(&m) {
                        names.push(m);
                    }
                }
                if names.is_empty() {
                    return None;
                }
                let quoted: Vec<String> = names.iter().map(|n| ruby_regex_escape(n)).collect();
                c.options
                    .insert("pattern".into(), format!("/^({})$/", quoted.join("|")));
            }
        }
        c.options.insert("scope".into(), "pattern".into());
        c.options.insert("files".into(), files.join("\n"));
        for k in ["test", "line", "class"] {
            c.options.remove(k);
        }
        c.name = format!("Rerun failed: {}", cfg.name);
        Some(c)
    }

    fn build(&self, cfg: &Configuration, ctx: &CommandContext) -> Result<TestCommand, String> {
        let ruby = context_of(ctx)?;
        let scope = cfg.option("scope").trim().to_string();
        let file = cfg.option("file").trim().replace('\\', "/");
        let (command, report) = match self.kind {
            Kind::RSpec => (
                rspec_command(cfg, &ruby, &ctx.report_dir)?,
                TestReport::JUnitFile,
            ),
            Kind::Minitest => (minitest_command(cfg, &ruby)?, TestReport::MinitestText),
        };
        Ok(TestCommand {
            program: command.program,
            args: command.args,
            env: command.env,
            report,
            flavor: JUnitFlavor::Generic,
            location_rules: Vec::new(),
            default_file: (matches!(scope.as_str(), "file" | "test") && !file.is_empty())
                .then_some(file),
            coverage: None,
            report_dirs: Vec::new(),
        })
    }
}

impl TestFrameworkProvider for RubyTests {
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
        match self.kind {
            Kind::RSpec => is_spec_file(path),
            Kind::Minitest => is_test_file(path),
        }
    }
    fn discover(&self, path: &str, text: &str) -> Option<TestFile> {
        let path = path.replace('\\', "/");
        let f = match self.kind {
            Kind::RSpec => self.discover_rspec(&path, text),
            Kind::Minitest => self.discover_minitest(&path, text),
        };
        (!f.is_empty()).then_some(f)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(ruby: &RubyContext) -> CommandContext {
        let mut c = CommandContext {
            report_dir: "/tmp/forge-test-reports/run-1".into(),
            ..CommandContext::default()
        };
        c.settings
            .insert(CONTEXT_KEY.into(), serde_json::to_string(ruby).unwrap());
        c
    }

    fn bundled() -> RubyContext {
        RubyContext {
            ruby: "/opt/rubies/3.3.6/bin/ruby".into(),
            bundle: "/opt/rubies/3.3.6/bin/bundle".into(),
            use_bundler: true,
            ..RubyContext::default()
        }
    }

    const SPEC: &str = "RSpec.describe Cart do\n  it \"adds items\" do\n  end\n\n  describe \"#total\" do\n    it \"sums\" do\n    end\n  end\nend\n";

    #[test]
    fn rspec_targets_and_commands() {
        let p = RubyTests::rspec();
        assert!(p.accepts("spec/cart_spec.rb") && !p.accepts("test/cart_test.rb"));
        let f = p.discover("spec/cart_spec.rb", SPEC).unwrap();
        let keys: Vec<_> = f.targets.iter().map(|t| (t.line, t.key.as_str())).collect();
        assert_eq!(
            keys,
            [
                (0, "spec/cart_spec.rb"),
                (1, "spec/cart_spec.rb::Cart adds items"),
                (4, "spec/cart_spec.rb::Cart #total"),
                (5, "spec/cart_spec.rb::Cart #total sums"),
            ]
        );
        let c = p
            .command(&f.targets[3].configuration, &ctx(&bundled()))
            .unwrap()
            .unwrap();
        assert_eq!(c.program, "/opt/rubies/3.3.6/bin/bundle");
        assert_eq!(
            c.args.join(" "),
            "exec rspec spec/cart_spec.rb:6 --format progress --format json --out /tmp/forge-test-reports/run-1/rspec.json"
        );
        assert_eq!(c.report, TestReport::JUnitFile);
        assert_eq!(c.default_file.as_deref(), Some("spec/cart_spec.rb"));
        let all = Configuration::new("All specs", RSPEC);
        let c = p
            .command(&all, &ctx(&RubyContext::default()))
            .unwrap()
            .unwrap();
        assert_eq!(c.program, "rspec");
        assert_eq!(c.args[0], "--format");
        // Rerun: one --example per failed example (their full descriptions).
        let failed = [FailedTest {
            name: "sums".into(),
            member: Some("Cart #total sums".into()),
            path: Some("spec/cart_spec.rb".into()),
        }];
        let rerun = p.rerun_failed(&all, &failed).unwrap();
        let c = p
            .command(&rerun, &ctx(&RubyContext::default()))
            .unwrap()
            .unwrap();
        assert_eq!(
            c.args[..3].join(" "),
            "--example Cart #total sums spec/cart_spec.rb"
        );
    }

    const TEST: &str =
        "class CartTest < Minitest::Test\n  def test_total\n    assert true\n  end\nend\n";

    #[test]
    fn minitest_targets_and_commands() {
        let p = RubyTests::minitest();
        let f = p.discover("test/cart_test.rb", TEST).unwrap();
        assert_eq!(f.targets[0].key, "test/cart_test.rb");
        assert_eq!(f.targets[1].key, "test/cart_test.rb::test_total");
        let c = p
            .command(&f.targets[1].configuration, &ctx(&bundled()))
            .unwrap()
            .unwrap();
        assert_eq!(
            c.args.join(" "),
            "exec ruby -Ilib -Itest test/cart_test.rb -v -n CartTest#test_total"
        );
        assert_eq!(c.report, TestReport::MinitestText);
        let c = p
            .command(&f.targets[0].configuration, &ctx(&RubyContext::default()))
            .unwrap()
            .unwrap();
        assert_eq!(c.program, "ruby");
        assert_eq!(c.args.last().unwrap(), "/^CartTest#/");
        let all = Configuration::new("All tests", MINITEST);
        let c = p
            .command(&all, &ctx(&RubyContext::default()))
            .unwrap()
            .unwrap();
        assert_eq!(c.args[2], "-e");
        assert!(c.args[3].contains("'test/**/*_test.rb'"));
        assert_eq!(&c.args[4..], ["--", "-v"]);
        let failed = [
            FailedTest {
                name: "test_total".into(),
                member: Some("test_total".into()),
                path: Some("test/cart_test.rb".into()),
            },
            FailedTest {
                name: "adds items".into(),
                member: Some("test_0001_adds items".into()),
                path: Some("test/spec_test.rb".into()),
            },
        ];
        let rerun = p.rerun_failed(&all, &failed).unwrap();
        let c = p
            .command(&rerun, &ctx(&RubyContext::default()))
            .unwrap()
            .unwrap();
        assert!(c.args[3].starts_with("['test/cart_test.rb', 'test/spec_test.rb'].each"));
        assert_eq!(
            &c.args[5..],
            ["-v", "-n", "/^(test_total|test_0001_adds items)$/"]
        );
    }

    #[test]
    fn errors_name_the_missing_field() {
        let p = RubyTests::rspec();
        let cfg = Configuration::new("x", RSPEC).with_option("scope", "file");
        let err = p
            .command(&cfg, &CommandContext::default())
            .unwrap()
            .unwrap_err();
        assert_eq!(err, "Choose the spec file");
        assert_eq!(ruby_regex_escape("a.b/c"), "a\\.b\\/c");
    }
}
