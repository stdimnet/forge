//! GdUnit4 test suites (GDScript), run headless with GdUnit4's command line tool:
//! `godot --headless --path <root> -s -d res://addons/gdUnit4/bin/GdUnitCmdTool.gd
//! --ignoreHeadlessMode -a <res://suite or folder> -rd <report dir>`. GdUnit4 writes a JUnit
//! report (`<report dir>/report_1/results.xml`); suites are located through their files.
//!
//! GdUnit4's command line runs whole suites, so the gutter offers suites (and their
//! files); test functions show their results. C# tests (gdUnit4Net) run through `dotnet
//! test` like other .NET tests.
//!
//! Options: `scope` (`all`, `directory`, `file`), `directory` (default `test`), `file`,
//! `files` (failed suites, one per line), `flags` (more GdUnit4 options).

use std::collections::BTreeMap;

use forge_output::{JUnitFlavor, LocationRule, TestReport};
use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use forge_testing::{
    CommandContext, FailedTest, TargetKind, TestCommand, TestFile, TestFrameworkProvider,
    TestTarget,
};
use serde::{Deserialize, Serialize};

use super::path_to_res;
use crate::regex_escape;

pub const GDUNIT: &str = "gdunit4";
pub const CONTEXT_KEY: &str = "godot.context";
pub const CMD_TOOL: &str = "res://addons/gdUnit4/bin/GdUnitCmdTool.gd";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GodotTestContext {
    pub godot: String,
    pub project_path: String,
    /// Suite names → project files.
    #[serde(default)]
    pub suites: BTreeMap<String, String>,
}

/// A GdUnit4 suite of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Suite {
    /// `class_name`, else the file name without `.gd`.
    pub name: String,
    pub line: u32,
    /// `func test_…` with their 0-based lines.
    pub tests: Vec<(String, u32)>,
}

/// The suite of a GDScript file, if it extends `GdUnitTestSuite`.
pub fn scan_suite(path: &str, text: &str) -> Option<Suite> {
    let mut class_name = None;
    let mut extends_line = None;
    let mut tests = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let l = line.trim_start();
        if let Some(rest) = l.strip_prefix("class_name ") {
            class_name = rest
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .next()
                .map(str::to_string);
        } else if let Some(rest) = l.strip_prefix("extends ") {
            if rest.contains("GdUnitTestSuite") {
                extends_line = Some(i as u32);
            }
        } else if line.starts_with("func test_") || line.starts_with("func test(") {
            let name: String = l["func ".len()..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            tests.push((name, i as u32));
        }
    }
    let line = extends_line?;
    let stem = crate::util::file_name(path)
        .trim_end_matches(".gd")
        .to_string();
    Some(Suite {
        name: class_name.unwrap_or(stem),
        line,
        tests,
    })
}

pub fn is_gdscript(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".gd")
}

/// The gutter targets of a suite file.
pub fn discover_file(path: &str, text: &str) -> TestFile {
    let Some(suite) = scan_suite(path, text) else {
        return TestFile::default();
    };
    let mut c = Configuration::new(suite.name.clone(), GDUNIT);
    c.options.insert("scope".into(), "file".into());
    c.options.insert("file".into(), path.to_string());
    TestFile {
        framework: Some(GDUNIT.into()),
        targets: vec![TestTarget {
            line: suite.line,
            kind: TargetKind::Class,
            label: suite.name,
            key: path.to_string(),
            configuration: c.clone(),
        }],
        file: Some(c),
    }
}

/// GdUnit4's arguments of a configuration.
pub fn test_args(
    cfg: &Configuration,
    ctx: &GodotTestContext,
    report_dir: &str,
) -> Result<Vec<String>, String> {
    let opt = |k: &str| cfg.option(k).trim().to_string();
    let mut targets: Vec<String> = match opt("scope").as_str() {
        "" | "all" => vec![path_to_res(match opt("directory").as_str() {
            "" => "test",
            d => d,
        })],
        "directory" => {
            if opt("directory").is_empty() {
                return Err("Choose the test folder".into());
            }
            vec![path_to_res(&opt("directory"))]
        }
        "file" => {
            if opt("file").is_empty() {
                return Err("Choose the test suite".into());
            }
            vec![path_to_res(&opt("file"))]
        }
        other => return Err(format!("Unknown test scope '{other}'")),
    };
    let failed: Vec<String> = cfg
        .option("files")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(path_to_res)
        .collect();
    if !failed.is_empty() {
        targets = failed;
    }
    let mut args = vec![
        "--headless".to_string(),
        "--path".into(),
        ctx.project_path.clone(),
        "-s".into(),
        "-d".into(),
        CMD_TOOL.into(),
        "--ignoreHeadlessMode".into(),
    ];
    for t in targets {
        args.extend(["-a".to_string(), t]);
    }
    if !report_dir.is_empty() {
        args.extend(["-rd".to_string(), report_dir.to_string()]);
    }
    args.extend(split_args(&opt("flags")));
    Ok(args)
}

pub struct GdUnitTests;

impl GdUnitTests {
    fn build(&self, cfg: &Configuration, ctx: &CommandContext) -> Result<TestCommand, String> {
        let g: GodotTestContext = ctx
            .settings
            .get(CONTEXT_KEY)
            .map(|s| serde_json::from_str(s).map_err(|e| format!("bad Godot context: {e}")))
            .transpose()?
            .unwrap_or_default();
        if g.godot.is_empty() {
            return Err("Godot was not found: set its executable in Settings | Languages & Frameworks | .NET | Godot".into());
        }
        Ok(TestCommand {
            program: g.godot.clone(),
            args: test_args(cfg, &g, &ctx.report_dir)?,
            env: BTreeMap::new(),
            report: TestReport::JUnitFile,
            flavor: JUnitFlavor::Generic,
            location_rules: g
                .suites
                .iter()
                .map(|(name, file)| LocationRule {
                    pattern: format!("^{}$", regex_escape(name)),
                    replacement: file.clone(),
                })
                .collect(),
            default_file: None,
            coverage: None,
            report_dirs: Vec::new(),
        })
    }
}

impl TestFrameworkProvider for GdUnitTests {
    fn id(&self) -> &str {
        GDUNIT
    }

    fn name(&self) -> &str {
        "GdUnit4"
    }

    fn types(&self) -> Vec<String> {
        vec![GDUNIT.into()]
    }

    fn accepts(&self, path: &str) -> bool {
        is_gdscript(path)
    }

    fn discover(&self, path: &str, text: &str) -> Option<TestFile> {
        let f = discover_file(path, text);
        (!f.is_empty()).then_some(f)
    }

    fn rerun_failed(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
        let mut files: Vec<String> = failed
            .iter()
            .filter_map(|f| f.path.clone())
            .filter(|p| is_gdscript(p))
            .collect();
        files.sort();
        files.dedup();
        if files.is_empty() {
            return None;
        }
        let mut c = cfg.clone();
        c.options.insert("files".into(), files.join("\n"));
        c.name = format!("Rerun failed: {}", cfg.name);
        Some(c)
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

    const SUITE: &str = "# GdUnit generated TestSuite\nclass_name PlayerTest\nextends GdUnitTestSuite\n@warning_ignore('unused_parameter')\n\nconst __source = 'res://player.gd'\n\nfunc test_jump() -> void:\n\tassert_int(1).is_equal(1)\n\nfunc test_take_damage() -> void:\n\tpass\n\nfunc helper():\n\tpass\n";

    #[test]
    fn discovers_suites() {
        let s = scan_suite("test/player_test.gd", SUITE).unwrap();
        assert_eq!(s.name, "PlayerTest");
        assert_eq!(s.line, 2);
        assert_eq!(
            s.tests,
            vec![
                ("test_jump".to_string(), 7),
                ("test_take_damage".to_string(), 10)
            ]
        );
        assert!(scan_suite(
            "player.gd",
            "extends CharacterBody2D\nfunc test_x():\n\tpass\n"
        )
        .is_none());
        let unnamed = scan_suite("test/enemy_test.gd", "extends GdUnitTestSuite\n").unwrap();
        assert_eq!(unnamed.name, "enemy_test");
        let f = GdUnitTests.discover("test/player_test.gd", SUITE).unwrap();
        assert_eq!(f.targets.len(), 1);
        assert_eq!(
            f.targets[0].configuration.option("file"),
            "test/player_test.gd"
        );
    }

    #[test]
    fn command_lines() {
        let g = GodotTestContext {
            godot: "/usr/bin/godot".into(),
            project_path: "/g".into(),
            suites: BTreeMap::from([("PlayerTest".to_string(), "test/player_test.gd".to_string())]),
        };
        let mut ctx = CommandContext {
            report_dir: "/tmp/r".into(),
            ..CommandContext::default()
        };
        ctx.settings
            .insert(CONTEXT_KEY.into(), serde_json::to_string(&g).unwrap());
        let cfg = Configuration::new("All", GDUNIT);
        let c = GdUnitTests.command(&cfg, &ctx).unwrap().unwrap();
        assert_eq!(
            c.args,
            vec![
                "--headless",
                "--path",
                "/g",
                "-s",
                "-d",
                CMD_TOOL,
                "--ignoreHeadlessMode",
                "-a",
                "res://test",
                "-rd",
                "/tmp/r"
            ]
        );
        assert_eq!(c.location_rules[0].replacement, "test/player_test.gd");
        let file = GdUnitTests
            .discover("test/player_test.gd", SUITE)
            .unwrap()
            .file
            .unwrap();
        let a = test_args(&file, &g, "").unwrap();
        assert!(a.ends_with(&["-a".to_string(), "res://test/player_test.gd".into()]));
        let rerun = GdUnitTests
            .rerun_failed(
                &cfg,
                &[FailedTest {
                    name: "test_jump".into(),
                    member: None,
                    path: Some("test/player_test.gd".into()),
                }],
            )
            .unwrap();
        assert_eq!(rerun.option("files"), "test/player_test.gd");
        assert!(GdUnitTests.rerun_failed(&cfg, &[]).is_none());
        let mut none = CommandContext::default();
        none.settings.insert(
            CONTEXT_KEY.into(),
            "{\"godot\":\"\",\"projectPath\":\"/g\"}".into(),
        );
        assert!(GdUnitTests.command(&cfg, &none).unwrap().is_err());
    }
}
