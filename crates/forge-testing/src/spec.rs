//! Test frameworks declared as data in a plugin manifest (`[[contributions.testFrameworks]]`):
//! everything a pack needs for the shared test runner without writing code.
//!
//! ```toml
//! [[contributions.testFrameworks]]
//! id = "busted"                       # also the run configuration type
//! name = "busted"
//! language = "lua"
//! detect = { files = [".busted", "spec/**/*_spec.lua"] }
//! testFiles = ["*_spec.lua"]
//! program = "busted"
//! programSetting = "lua.busted.path"
//! args = ["--output=TAP"]
//! report = "tap"
//! nameSeparator = " "
//! escape = "lua"
//! discovery = [
//!   { kind = "describe", pattern = '^\s*describe\(\s*["''](?P<name>[^"'']+)' },
//!   { kind = "test", pattern = '^\s*it\(\s*["''](?P<name>[^"'']+)' },
//! ]
//! rerun = { join = "|" }
//! [contributions.testFrameworks.scopes]
//! all = []
//! directory = ["${directory}"]
//! file = ["${file}"]
//! test = ["${file}", "--filter=${testPattern}"]
//! pattern = ["--filter=${pattern}"]
//! ```
//!
//! - `detect`: the project uses the framework when one of `files` (globs relative to the
//!   project root) exists or a `dependencies` file contains a text (`package.json` contains
//!   `"vitest"`).
//! - `testFiles`: globs of files with tests (a pattern without `/` matches the file name);
//!   the gutter shows icons in them.
//! - `discovery`: line patterns of suites (`describe`, `class`, `suite`, `file`) and tests
//!   (`test`, `method`, `example`); the `name` group (or group 1) is the name. Nesting
//!   follows the indentation. With `nameSeparator`, a test's name is its full name (the
//!   suites' names and its own, joined), as tools like busted and Mocha report it.
//! - `scopes`: the arguments for `all` tests, a `directory`, a `file`, one `test` and a name
//!   `pattern`; configurations of the framework's type pick one (`options.scope`).
//!   Placeholders: `${file}`, `${directory}`, `${test}` (full name), `${testName}` (own
//!   name), `${testPattern}` (the full name escaped with `escape`), `${pattern}`,
//!   `${reportDir}`, `${coverageFile}`. An argument whose placeholders are all empty is
//!   dropped.
//! - `report` / `junitFlavor`: how results arrive (see [`forge_output::TestReport`]).
//! - `locationRules`: rewrites of reported locations into project paths.
//! - `rerun`: Rerun Failed Tests runs the `pattern` scope with the failed tests' escaped
//!   names joined by `join`.
//! - `coverage`: Run with Coverage adds `args`, sets `env` and reads `file` (a path
//!   relative to the working directory, or with `${reportDir}` / `${coverageFile}`) in
//!   `format`.
//! - `fields`: extra fields of the run configuration form (`key`, `label`, `kind`,
//!   `placeholder`, `default`, `options`), stored in the configuration's options for a
//!   code-backed provider that wraps the spec (WebForge's Node.js test runners).

use std::collections::BTreeMap;

use forge_output::{JUnitFlavor, LocationRule, TestReport};
use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use regex_lite::Regex;
use serde::{Deserialize, Serialize};

use crate::coverage::CoverageFormat;
use crate::provider::{CommandContext, CoverageRequest, TestCommand, TestFrameworkProvider};
use crate::targets::{FailedTest, TargetKind, TestFile, TestTarget};

/// When a project uses a framework.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Detection {
    /// Globs relative to the project root.
    pub files: Vec<String>,
    pub dependencies: Vec<DependencyCheck>,
}

/// A project file that names the framework (`package.json` with `"jest"`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DependencyCheck {
    pub file: String,
    pub contains: String,
}

/// A line pattern of the gutter discovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryPattern {
    pub kind: TargetKind,
    pub pattern: String,
}

/// How a test name becomes `${testPattern}`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PatternEscape {
    /// Regular expression metacharacters are escaped (Jest, Vitest, Go, pytest `-k` uses none).
    #[default]
    Regex,
    /// Lua pattern magic characters get `%` (busted).
    Lua,
    /// The name as it is.
    None,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RerunSpec {
    /// Separator between the failed tests' patterns (`|`).
    pub join: String,
    /// Put the joined names in this template (`^(${names})$`); empty: the names alone.
    pub template: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CoverageSpec {
    pub args: Vec<String>,
    /// Where the report is written: relative to the working directory, or using
    /// `${reportDir}` / `${coverageFile}`.
    pub file: String,
    pub format: CoverageFormat,
    pub env: BTreeMap<String, String>,
}

/// A choice of an `enum` field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FieldOption {
    pub value: String,
    pub label: String,
}

/// An extra field of the framework's run configuration form, stored in the
/// configuration's `options` (a Jest configuration's "Jest package" or "Node options").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpecField {
    pub key: String,
    pub label: String,
    /// `string` (default), `path`, `text`, `enum` or `bool`.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub placeholder: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub default: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<FieldOption>,
}

/// A test framework contribution.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TestFrameworkSpec {
    /// Framework id, also the id of its run configuration type.
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// File type of its test files (`lua`, `typescript`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub language: String,
    pub detect: Detection,
    pub test_files: Vec<String>,
    /// The executable (a configuration's interpreter field and `programSetting` win).
    pub program: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program_setting: Option<String>,
    /// Arguments every run gets, after the program.
    pub args: Vec<String>,
    /// Arguments per scope: `all`, `directory`, `file`, `test`, `pattern`.
    pub scopes: BTreeMap<String, Vec<String>>,
    pub report: TestReport,
    pub junit_flavor: JUnitFlavor,
    pub discovery: Vec<DiscoveryPattern>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_separator: Option<String>,
    pub escape: PatternEscape,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerun: Option<RerunSpec>,
    pub location_rules: Vec<LocationRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<CoverageSpec>,
    /// Whether configurations of the framework can be debugged.
    pub debuggable: bool,
    /// Extra fields of its run configuration form.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<SpecField>,
}

/// The order of scopes in forms and menus.
pub const SCOPES: [&str; 5] = ["all", "directory", "file", "test", "pattern"];

/// A glob (`**/`, `**`, `*`, `?`) as an anchored regular expression; a glob without `/`
/// matches the last path segment.
pub fn glob_regex(glob: &str) -> Option<Regex> {
    let g = glob.trim().trim_start_matches("./");
    if g.is_empty() {
        return None;
    }
    let mut re = String::from(if g.contains('/') { "^" } else { "^(?:.*/)?" });
    let mut chars = g.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if chars.peek() == Some(&'/') {
                    chars.next();
                    re.push_str("(?:.*/)?");
                } else {
                    re.push_str(".*");
                }
            }
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            c if "\\.+()|[]{}^$#&-~".contains(c) => {
                re.push('\\');
                re.push(c);
            }
            c => re.push(c),
        }
    }
    re.push('$');
    Regex::new(&re).ok()
}

pub fn glob_match(glob: &str, path: &str) -> bool {
    glob_regex(glob).is_some_and(|re| re.is_match(&path.replace('\\', "/")))
}

/// Escape a literal for a regular expression.
pub fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\.+*?()|[]{}^$/#".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Escape a literal for a Lua pattern.
pub fn lua_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "^$()%.[]*+-?".contains(c) {
            out.push('%');
        }
        out.push(c);
    }
    out
}

/// Expand `${name}` placeholders of one argument; `None` when it had placeholders and all
/// of them were empty.
pub fn expand(arg: &str, vars: &BTreeMap<&str, String>) -> Option<String> {
    let mut out = String::new();
    let mut rest = arg;
    let mut placeholders = 0;
    let mut filled = 0;
    while let Some(start) = rest.find("${") {
        let Some(end) = rest[start..].find('}') else {
            break;
        };
        out.push_str(&rest[..start]);
        let key = &rest[start + 2..start + end];
        placeholders += 1;
        let value = vars.get(key).cloned().unwrap_or_default();
        if !value.is_empty() {
            filled += 1;
        }
        out.push_str(&value);
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    (placeholders == 0 || filled > 0).then_some(out)
}

impl TestFrameworkSpec {
    /// Whether the project uses the framework: `files` are the project-relative paths
    /// known (a shallow listing), `read` returns a project file's text.
    pub fn detect(&self, files: &[String], read: &dyn Fn(&str) -> Option<String>) -> bool {
        let by_file = self.detect.files.iter().any(|g| {
            let re = glob_regex(g);
            files.iter().any(|f| {
                re.as_ref()
                    .is_some_and(|re| re.is_match(&f.replace('\\', "/")))
            })
        });
        by_file
            || self.detect.dependencies.iter().any(|d| {
                let names: Vec<&str> = if d.file.contains(['*', '?']) {
                    files
                        .iter()
                        .filter(|f| glob_match(&d.file, f))
                        .map(String::as_str)
                        .collect()
                } else {
                    vec![d.file.as_str()]
                };
                names
                    .iter()
                    .any(|f| read(f).is_some_and(|t| t.contains(&d.contains)))
            })
    }

    /// Whether a project file holds tests of this framework.
    pub fn is_test_file(&self, path: &str) -> bool {
        self.test_files.iter().any(|g| glob_match(g, path))
    }

    fn escape(&self, s: &str) -> String {
        match self.escape {
            PatternEscape::Regex => regex_escape(s),
            PatternEscape::Lua => lua_escape(s),
            PatternEscape::None => s.to_string(),
        }
    }

    fn config(&self, name: &str, options: &[(&str, &str)]) -> Configuration {
        let mut c = Configuration::new(name, self.id.as_str());
        for (k, v) in options {
            c = c.with_option(k, *v);
        }
        c
    }

    /// The tests of a file for the gutter: discovery patterns, nested by indentation.
    pub fn discover_tests(&self, path: &str, text: &str) -> TestFile {
        if !self.is_test_file(path) {
            return TestFile::default();
        }
        let patterns: Vec<(TargetKind, Regex)> = self
            .discovery
            .iter()
            .filter_map(|p| Regex::new(&p.pattern).ok().map(|re| (p.kind, re)))
            .collect();
        let file_name = path.rsplit('/').next().unwrap_or(path);
        let mut targets = Vec::new();
        // Open groups: (indentation, name).
        let mut stack: Vec<(usize, String)> = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let Some((kind, name)) = patterns.iter().find_map(|(kind, re)| {
                let c = re.captures(line)?;
                let name = c.name("name").or_else(|| c.get(1))?.as_str().to_string();
                Some((*kind, name))
            }) else {
                continue;
            };
            let indent = line.len() - line.trim_start().len();
            while stack.last().is_some_and(|(at, _)| *at >= indent) {
                stack.pop();
            }
            let full = match &self.name_separator {
                Some(sep) => stack
                    .iter()
                    .map(|(_, n)| n.as_str())
                    .chain([name.as_str()])
                    .collect::<Vec<_>>()
                    .join(sep),
                None => name.clone(),
            };
            let configuration = if kind == TargetKind::File {
                self.config(file_name, &[("scope", "file"), ("file", path)])
            } else {
                self.config(&name, &[("scope", "test"), ("file", path), ("test", &full)])
            };
            let key = if kind.is_group() {
                if kind == TargetKind::File || kind == TargetKind::Class {
                    path.to_string()
                } else {
                    String::new()
                }
            } else {
                format!("{path}::{full}")
            };
            targets.push(TestTarget {
                line: i as u32,
                kind,
                label: name.clone(),
                key,
                configuration,
            });
            if kind.is_group() {
                stack.push((indent, name));
            }
        }
        let file = (!targets.is_empty() || !self.discovery.is_empty())
            .then(|| self.config(file_name, &[("scope", "file"), ("file", path)]));
        TestFile {
            framework: Some(self.id.clone()),
            targets,
            file,
        }
    }

    /// The executable of a configuration.
    pub fn program(&self, cfg: &Configuration, setting: Option<String>) -> String {
        let own = cfg.option("executable").trim();
        if !own.is_empty() {
            return own.to_string();
        }
        setting
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| self.program.clone())
    }

    /// The command line of a configuration of this framework.
    pub fn build(&self, cfg: &Configuration, ctx: &CommandContext) -> Result<TestCommand, String> {
        let scope = match cfg.option("scope").trim() {
            "" => "all",
            s => s,
        };
        let template = self
            .scopes
            .get(scope)
            .ok_or_else(|| format!("{} cannot run the scope `{scope}`", self.name))?;
        let need = |key: &str, label: &str| -> Result<String, String> {
            match cfg.option(key).trim() {
                "" => Err(format!("{label} is not specified")),
                v => Ok(v.to_string()),
            }
        };
        match scope {
            "directory" => {
                need("directory", "Directory")?;
            }
            "file" => {
                need("file", "File")?;
            }
            "test" => {
                need("test", "Test")?;
            }
            "pattern" => {
                need("pattern", "Pattern")?;
            }
            _ => {}
        }
        let test = cfg.option("test").trim().to_string();
        let own_name = match &self.name_separator {
            Some(sep) if !sep.is_empty() => test
                .rsplit(sep.as_str())
                .next()
                .unwrap_or(&test)
                .to_string(),
            _ => test.clone(),
        };
        let mut vars: BTreeMap<&str, String> = BTreeMap::new();
        vars.insert("file", cfg.option("file").trim().to_string());
        vars.insert("directory", cfg.option("directory").trim().to_string());
        vars.insert(
            "testPattern",
            if test.is_empty() {
                String::new()
            } else {
                self.escape(&test)
            },
        );
        vars.insert("test", test);
        vars.insert("testName", own_name);
        vars.insert("pattern", cfg.option("pattern").trim().to_string());
        vars.insert("reportDir", ctx.report_dir.clone());
        vars.insert(
            "coverageFile",
            ctx.coverage_file.clone().unwrap_or_default(),
        );
        let mut args: Vec<String> = self.args.iter().filter_map(|a| expand(a, &vars)).collect();
        // A pattern of several lines (Rerun Failed Tests with `join = "\n"`) repeats the
        // arguments that use it, once per line (`--filter a --filter b`).
        let patterns: Vec<String> = vars["pattern"].lines().map(str::to_string).collect();
        for a in template {
            if a.contains("${pattern}") && patterns.len() > 1 {
                for p in &patterns {
                    let mut one = vars.clone();
                    one.insert("pattern", p.clone());
                    args.extend(expand(a, &one));
                }
            } else {
                args.extend(expand(a, &vars));
            }
        }
        args.extend(split_args(cfg.option("testRunnerOptions")));
        args.extend(split_args(&cfg.arguments));
        let mut env = BTreeMap::new();
        let mut coverage = None;
        if ctx.coverage_file.is_some() {
            let spec = self
                .coverage
                .as_ref()
                .ok_or_else(|| format!("{} cannot measure coverage", self.name))?;
            args.extend(spec.args.iter().filter_map(|a| expand(a, &vars)));
            env.extend(spec.env.clone());
            coverage = Some(CoverageRequest {
                file: expand(&spec.file, &vars).unwrap_or_default(),
                format: spec.format,
                location_rules: Vec::new(),
            });
        }
        Ok(TestCommand {
            program: self.program(cfg, ctx.setting(self.program_setting.as_deref())),
            args,
            env,
            report: self.report,
            flavor: self.junit_flavor,
            location_rules: self.location_rules.clone(),
            default_file: matches!(scope, "file" | "test")
                .then(|| cfg.option("file").trim().to_string()),
            coverage,
            report_dirs: Vec::new(),
        })
    }

    /// The configuration that reruns the failed tests (the `pattern` scope).
    pub fn rerun(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
        let spec = self.rerun.as_ref()?;
        if failed.is_empty() || !self.scopes.contains_key("pattern") {
            return None;
        }
        let mut names: Vec<String> = Vec::new();
        for f in failed {
            let n = self.escape(&f.name);
            if !names.contains(&n) {
                names.push(n);
            }
        }
        let joined = names.join(if spec.join.is_empty() {
            "|"
        } else {
            &spec.join
        });
        let pattern = if spec.template.is_empty() {
            joined
        } else {
            spec.template.replace("${names}", &joined)
        };
        let mut c = cfg.clone();
        c.options.insert("scope".into(), "pattern".into());
        c.options.insert("pattern".into(), pattern);
        Some(c)
    }
}

/// A manifest-declared framework as a provider of the registry.
#[derive(Debug, Clone)]
pub struct SpecProvider(pub TestFrameworkSpec);

impl TestFrameworkProvider for SpecProvider {
    fn id(&self) -> &str {
        &self.0.id
    }
    fn name(&self) -> &str {
        &self.0.name
    }
    fn types(&self) -> Vec<String> {
        vec![self.0.id.clone()]
    }
    fn accepts(&self, path: &str) -> bool {
        self.0.is_test_file(path)
    }
    fn discover(&self, path: &str, text: &str) -> Option<TestFile> {
        Some(self.0.discover_tests(path, text)).filter(|f| !f.is_empty())
    }
    fn rerun_failed(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
        self.0.rerun(cfg, failed)
    }
    fn command(
        &self,
        cfg: &Configuration,
        ctx: &CommandContext,
    ) -> Option<Result<TestCommand, String>> {
        Some(self.0.build(cfg, ctx))
    }
    fn supports_coverage(&self, _type_id: &str) -> bool {
        self.0.coverage.is_some()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// busted, as the LuaForge manifest declares it.
    pub(crate) fn busted() -> TestFrameworkSpec {
        toml_spec(include_str!("../fixtures/busted.toml"))
    }

    fn toml_spec(text: &str) -> TestFrameworkSpec {
        toml::from_str(text).expect("spec")
    }

    #[test]
    fn globs_match_paths_and_names() {
        assert!(glob_match("*_spec.lua", "spec/cart_spec.lua"));
        assert!(glob_match("spec/**/*.lua", "spec/a/b.lua"));
        assert!(glob_match("spec/**/*.lua", "spec/b.lua"));
        assert!(!glob_match("spec/*.lua", "spec/a/b.lua"));
        assert!(glob_match("**/*.test.ts", "src/cart.test.ts"));
        assert!(!glob_match("*.test.ts", "src/cart.test.tsx"));
        assert!(glob_match(".busted", ".busted"));
    }

    #[test]
    fn expands_placeholders_and_drops_empty_arguments() {
        let vars: BTreeMap<&str, String> =
            [("file", "a.lua".to_string()), ("pattern", String::new())].into();
        assert_eq!(expand("${file}", &vars).as_deref(), Some("a.lua"));
        assert_eq!(expand("--filter=${pattern}", &vars), None);
        assert_eq!(expand("--x", &vars).as_deref(), Some("--x"));
        assert_eq!(lua_escape("a.b-c (1)"), "a%.b%-c %(1%)");
    }

    #[test]
    fn detects_the_framework() {
        let spec = busted();
        let read = |p: &str| {
            (p == "shop-1.0-1.rockspec").then(|| "dependencies = { \"busted\" }".to_string())
        };
        assert!(spec.detect(&[".busted".into()], &|_| None));
        assert!(spec.detect(&["shop-1.0-1.rockspec".into()], &read));
        assert!(!spec.detect(&["main.lua".into()], &|_| None));
    }

    #[test]
    fn discovers_nested_tests_with_full_names() {
        let spec = busted();
        let text = r#"describe("Cart", function()
  it("adds items", function() end)
  describe("totals", function()
    it("rounds to cents", function() end)
  end)
end)
"#;
        let file = spec.discover_tests("spec/cart_spec.lua", text);
        let rows: Vec<_> = file
            .targets
            .iter()
            .map(|t| (t.line, t.kind, t.label.as_str(), t.key.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                (0, TargetKind::Describe, "Cart", ""),
                (
                    1,
                    TargetKind::Test,
                    "adds items",
                    "spec/cart_spec.lua::Cart adds items"
                ),
                (2, TargetKind::Describe, "totals", ""),
                (
                    3,
                    TargetKind::Test,
                    "rounds to cents",
                    "spec/cart_spec.lua::Cart totals rounds to cents"
                ),
            ]
        );
        assert_eq!(
            file.targets[3].configuration.option("test"),
            "Cart totals rounds to cents"
        );
        assert_eq!(file.file.unwrap().option("scope"), "file");
        assert!(spec.discover_tests("src/cart.lua", text).is_empty());
    }

    #[test]
    fn builds_commands_per_scope() {
        let spec = busted();
        let ctx = CommandContext::default();
        let cfg = Configuration::new("t", "busted")
            .with_option("scope", "test")
            .with_option("file", "spec/cart_spec.lua")
            .with_option("test", "Cart totals rounds (1.5)")
            .with_option("testRunnerOptions", "--verbose");
        let cmd = spec.build(&cfg, &ctx).unwrap();
        assert_eq!(cmd.program, "busted");
        assert_eq!(
            cmd.args,
            [
                "--output=TAP",
                "spec/cart_spec.lua",
                "--filter=Cart totals rounds %(1%.5%)",
                "--verbose"
            ]
        );
        assert_eq!(cmd.report, TestReport::Tap);
        assert_eq!(cmd.default_file.as_deref(), Some("spec/cart_spec.lua"));
        let all = spec
            .build(&Configuration::new("t", "busted"), &ctx)
            .unwrap();
        assert_eq!(all.args, ["--output=TAP"]);
        let mut with_exe = Configuration::new("t", "busted").with_option("scope", "directory");
        assert_eq!(
            spec.build(&with_exe, &ctx).unwrap_err(),
            "Directory is not specified"
        );
        with_exe = with_exe
            .with_option("directory", "spec")
            .with_option("executable", "/opt/busted");
        assert_eq!(spec.build(&with_exe, &ctx).unwrap().program, "/opt/busted");
        let setting = CommandContext {
            settings: [(
                "lua.busted.path".to_string(),
                "/usr/local/bin/busted".to_string(),
            )]
            .into(),
            ..CommandContext::default()
        };
        assert_eq!(
            spec.build(&Configuration::new("t", "busted"), &setting)
                .unwrap()
                .program,
            "/usr/local/bin/busted"
        );
        let coverage = CommandContext {
            coverage_file: Some("/tmp/r/lcov.info".into()),
            ..CommandContext::default()
        };
        let cmd = spec
            .build(&Configuration::new("t", "busted"), &coverage)
            .unwrap();
        assert_eq!(cmd.args, ["--output=TAP", "--coverage"]);
        let c = cmd.coverage.unwrap();
        assert_eq!(
            (c.file.as_str(), c.format),
            ("luacov.report.out", CoverageFormat::Lcov)
        );
    }

    #[test]
    fn reruns_failed_tests_by_pattern() {
        let spec = busted();
        let cfg = Configuration::new("All", "busted");
        let failed = [
            FailedTest {
                name: "Cart totals rounds".into(),
                ..FailedTest::default()
            },
            FailedTest {
                name: "Cart a-b".into(),
                ..FailedTest::default()
            },
        ];
        let c = spec.rerun(&cfg, &failed).unwrap();
        assert_eq!(c.option("scope"), "pattern");
        assert_eq!(c.option("pattern"), "Cart totals rounds\nCart a%-b");
        let cmd = spec.build(&c, &CommandContext::default()).unwrap();
        assert_eq!(
            cmd.args,
            [
                "--output=TAP",
                "--filter=Cart totals rounds",
                "--filter=Cart a%-b"
            ]
        );
        assert!(spec.rerun(&cfg, &[]).is_none());
    }
}
