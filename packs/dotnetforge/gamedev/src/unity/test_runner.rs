//! Unity Test Framework tests in batch mode, like Rider's Unity unit tests:
//!
//! - discovery: NUnit `[Test]`, `[TestCase]` and `[UnityTest]` methods of C# files in
//!   `Assets/` and `Packages/` (DotNetForge's C# scanner),
//! - `Unity -batchmode -projectPath <root> -runTests -testPlatform EditMode|PlayMode
//!   -testResults <report dir>/TestResults.xml [-testFilter <regex>] -logFile -`,
//! - results from the NUnit 3 XML file (`forge_output::formats::nunit`), located through the
//!   test classes' files,
//! - Rerun Failed Tests (a `-testFilter` of the failed methods).
//!
//! Unity refuses batch mode while an editor has the project open; the app checks that
//! first. Configuration options: `scope` (`all`, `class`, `method`, `filter`), `class`,
//! `method`, `filter` (a `-testFilter` expression), `tests` (failed tests,
//! `<class>.<method>` per line), `platform` (`EditMode`, `PlayMode` or a player target
//! such as `StandaloneLinux64`), `category`, `assemblies`, `flags` (more Unity options).

use std::collections::BTreeMap;

use dotnetforge_lang_dotnet::scan::scan_source;
use dotnetforge_lang_dotnet::TestFramework;
use forge_output::{JUnitFlavor, LocationRule, TestReport};
use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use forge_testing::{
    CommandContext, FailedTest, TargetKind, TestCommand, TestFile, TestFrameworkProvider,
    TestTarget,
};
use serde::{Deserialize, Serialize};

use crate::regex_escape;

/// The framework id and run configuration type.
pub const UNITY_TEST: &str = "unity-test";
/// `CommandContext.settings` key of the [`UnityContext`] (JSON).
pub const CONTEXT_KEY: &str = "unity.context";

/// What building a test command needs besides the configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnityContext {
    /// The Unity editor executable.
    pub editor: String,
    /// The project folder (absolute).
    pub project_path: String,
    /// Default test platform (`unity.tests.platform`).
    #[serde(default)]
    pub platform: String,
    /// Test class full names → project files (located test results).
    #[serde(default)]
    pub classes: BTreeMap<String, String>,
}

impl UnityContext {
    pub fn location_rules(&self) -> Vec<LocationRule> {
        self.classes
            .iter()
            .map(|(class, file)| LocationRule {
                pattern: format!("^{}$", regex_escape(class)),
                replacement: file.clone(),
            })
            .collect()
    }
}

/// Whether a file may hold Unity tests (C# under `Assets/` or `Packages/`).
pub fn is_unity_source(path: &str) -> bool {
    let p = path.replace('\\', "/");
    p.to_ascii_lowercase().ends_with(".cs")
        && (p.starts_with("Assets/") || p.starts_with("Packages/"))
}

/// The tests of a C# file for the gutter.
pub fn discover_file(path: &str, text: &str) -> TestFile {
    let scan = scan_source(path, text, Some(TestFramework::Nunit));
    let mut targets = Vec::new();
    let mut classes = Vec::new();
    for class in scan.classes.iter().filter(|c| !c.tests.is_empty()) {
        classes.push(class.full_name.clone());
        let mut c = Configuration::new(class.name.clone(), UNITY_TEST);
        c.options.insert("scope".into(), "class".into());
        c.options.insert("class".into(), class.full_name.clone());
        targets.push(TestTarget {
            line: class.line,
            kind: TargetKind::Class,
            label: class.name.clone(),
            key: path.to_string(),
            configuration: c,
        });
        for t in &class.tests {
            let mut c = Configuration::new(format!("{}.{}", class.name, t.name), UNITY_TEST);
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
    let file = (!classes.is_empty()).then(|| {
        let name = crate::util::file_name(path);
        let mut c = Configuration::new(format!("Tests in {name}"), UNITY_TEST);
        c.options.insert("scope".into(), "filter".into());
        c.options.insert(
            "filter".into(),
            format!(
                "^({})\\.",
                classes
                    .iter()
                    .map(|c| regex_escape(c))
                    .collect::<Vec<_>>()
                    .join("|")
            ),
        );
        c
    });
    TestFile {
        framework: Some(UNITY_TEST.into()),
        targets,
        file,
    }
}

/// The `-testFilter` of a configuration (empty: every test).
pub fn test_filter(cfg: &Configuration) -> Result<String, String> {
    let opt = |k: &str| cfg.option(k).trim().to_string();
    Ok(match opt("scope").as_str() {
        "" | "all" => String::new(),
        "class" => {
            if opt("class").is_empty() {
                return Err("Enter the test class".into());
            }
            format!("^{}\\.", regex_escape(&opt("class")))
        }
        "method" => {
            if opt("class").is_empty() || opt("method").is_empty() {
                return Err("Enter the test class and method".into());
            }
            format!(
                "^{}\\.{}(\\(.*\\))?$",
                regex_escape(&opt("class")),
                regex_escape(&opt("method"))
            )
        }
        "filter" => {
            let tests: Vec<String> = cfg
                .option("tests")
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(regex_escape)
                .collect();
            if !tests.is_empty() {
                format!("^({})(\\(.*\\))?$", tests.join("|"))
            } else if opt("filter").is_empty() {
                return Err("Enter a test filter (a test name or a regular expression)".into());
            } else {
                opt("filter")
            }
        }
        other => return Err(format!("Unknown test scope '{other}'")),
    })
}

/// The arguments of a test run.
pub fn test_args(
    cfg: &Configuration,
    ctx: &UnityContext,
    report_dir: &str,
) -> Result<Vec<String>, String> {
    if ctx.project_path.is_empty() {
        return Err("Not a Unity project".into());
    }
    let platform = match cfg.option("platform").trim() {
        "" if ctx.platform.is_empty() => "EditMode".to_string(),
        "" => ctx.platform.clone(),
        p => p.to_string(),
    };
    let mut args = vec![
        "-batchmode".to_string(),
        "-projectPath".into(),
        ctx.project_path.clone(),
        "-runTests".into(),
        "-testPlatform".into(),
        platform,
    ];
    if !report_dir.is_empty() {
        args.extend([
            "-testResults".to_string(),
            format!(
                "{}/TestResults.xml",
                report_dir.trim_end_matches(['/', '\\'])
            ),
        ]);
    }
    let filter = test_filter(cfg)?;
    if !filter.is_empty() {
        args.extend(["-testFilter".to_string(), filter]);
    }
    for (option, flag) in [
        ("category", "-testCategory"),
        ("assemblies", "-assemblyNames"),
    ] {
        let v = cfg.option(option).trim();
        if !v.is_empty() {
            args.extend([flag.to_string(), v.to_string()]);
        }
    }
    args.extend(["-logFile".to_string(), "-".into()]);
    args.extend(split_args(cfg.option("flags")));
    args.extend(split_args(&cfg.arguments));
    Ok(args)
}

pub struct UnityTests;

impl UnityTests {
    fn build(&self, cfg: &Configuration, ctx: &CommandContext) -> Result<TestCommand, String> {
        let unity: UnityContext = ctx
            .settings
            .get(CONTEXT_KEY)
            .map(|s| serde_json::from_str(s).map_err(|e| format!("bad Unity context: {e}")))
            .transpose()?
            .unwrap_or_default();
        if unity.editor.is_empty() {
            return Err(
                "No Unity editor for this project: install its version with the Unity Hub or choose one in Settings | Languages & Frameworks | .NET | Unity".into(),
            );
        }
        Ok(TestCommand {
            program: unity.editor.clone(),
            args: test_args(cfg, &unity, &ctx.report_dir)?,
            env: BTreeMap::new(),
            report: TestReport::JUnitFile,
            flavor: JUnitFlavor::Generic,
            location_rules: unity.location_rules(),
            default_file: None,
            coverage: None,
            report_dirs: Vec::new(),
        })
    }
}

/// Rerun Failed Tests: the failed methods by their class (the location's member and the
/// test class of its file).
pub fn rerun(
    cfg: &Configuration,
    failed: &[FailedTest],
    classes: &BTreeMap<String, String>,
) -> Option<Configuration> {
    let mut tests: Vec<String> = Vec::new();
    for f in failed {
        let method = f
            .member
            .clone()
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| {
                f.name
                    .split('(')
                    .next()
                    .unwrap_or(&f.name)
                    .trim()
                    .to_string()
            });
        if method.is_empty() {
            continue;
        }
        let in_file: Vec<&String> = classes
            .iter()
            .filter(|(_, file)| f.path.as_deref() == Some(file.as_str()))
            .map(|(c, _)| c)
            .collect();
        if in_file.is_empty() {
            tests.push(method);
        } else {
            tests.extend(in_file.into_iter().map(|c| format!("{c}.{method}")));
        }
    }
    tests.dedup();
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

impl TestFrameworkProvider for UnityTests {
    fn id(&self) -> &str {
        UNITY_TEST
    }

    fn name(&self) -> &str {
        "Unity Tests"
    }

    fn types(&self) -> Vec<String> {
        vec![UNITY_TEST.into()]
    }

    fn accepts(&self, path: &str) -> bool {
        is_unity_source(path)
    }

    fn discover(&self, path: &str, text: &str) -> Option<TestFile> {
        let f = discover_file(path, text);
        (!f.is_empty()).then_some(f)
    }

    fn rerun_failed(&self, cfg: &Configuration, failed: &[FailedTest]) -> Option<Configuration> {
        // Without the project's classes the failed tests run by method name.
        rerun(cfg, failed, &BTreeMap::new())
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

    const PLAYER_TESTS: &str = r#"using NUnit.Framework;
using UnityEngine.TestTools;
using System.Collections;

namespace Shooter.Tests
{
    public class PlayerTests
    {
        [Test]
        public void Jump_SetsVelocity() { }

        [TestCase(10)]
        [TestCase(-1)]
        public void Damage(int amount) { }

        [UnityTest]
        public IEnumerator FallsWithGravity() { yield return null; }
    }
}
"#;

    fn ctx() -> UnityContext {
        UnityContext {
            editor: "/u/Unity".into(),
            project_path: "/home/sam/Shooter".into(),
            platform: String::new(),
            classes: BTreeMap::from([(
                "Shooter.Tests.PlayerTests".to_string(),
                "Assets/Tests/EditMode/PlayerTests.cs".to_string(),
            )]),
        }
    }

    #[test]
    fn discovers_nunit_and_unity_tests() {
        let path = "Assets/Tests/EditMode/PlayerTests.cs";
        assert!(is_unity_source(path));
        assert!(!is_unity_source("src/App/Program.cs"));
        let f = discover_file(path, PLAYER_TESTS);
        let labels: Vec<&str> = f.targets.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "PlayerTests",
                "Jump_SetsVelocity",
                "Damage",
                "FallsWithGravity"
            ]
        );
        assert_eq!(f.targets[3].key, format!("{path}::FallsWithGravity"));
        assert_eq!(
            f.targets[0].configuration.option("class"),
            "Shooter.Tests.PlayerTests"
        );
        assert_eq!(
            f.file.unwrap().option("filter"),
            "^(Shooter\\.Tests\\.PlayerTests)\\."
        );
        assert!(UnityTests
            .discover("Assets/Scripts/Player.cs", "class Player {}")
            .is_none());
    }

    #[test]
    fn command_lines_and_reruns() {
        let mut command_ctx = CommandContext {
            report_dir: "/tmp/r".into(),
            ..CommandContext::default()
        };
        command_ctx
            .settings
            .insert(CONTEXT_KEY.into(), serde_json::to_string(&ctx()).unwrap());
        let mut cfg = Configuration::new("All", UNITY_TEST);
        let all = UnityTests.command(&cfg, &command_ctx).unwrap().unwrap();
        assert_eq!(all.program, "/u/Unity");
        assert_eq!(
            all.args,
            vec![
                "-batchmode",
                "-projectPath",
                "/home/sam/Shooter",
                "-runTests",
                "-testPlatform",
                "EditMode",
                "-testResults",
                "/tmp/r/TestResults.xml",
                "-logFile",
                "-"
            ]
        );
        assert_eq!(
            all.location_rules[0].pattern,
            "^Shooter\\.Tests\\.PlayerTests$"
        );

        cfg.options.insert("scope".into(), "method".into());
        cfg.options
            .insert("class".into(), "Shooter.Tests.PlayerTests".into());
        cfg.options.insert("method".into(), "Damage".into());
        cfg.options.insert("platform".into(), "PlayMode".into());
        cfg.options.insert("category".into(), "Fast".into());
        let m = test_args(&cfg, &ctx(), "").unwrap();
        let i = m.iter().position(|a| a == "-testFilter").unwrap();
        assert_eq!(
            m[i + 1],
            "^Shooter\\.Tests\\.PlayerTests\\.Damage(\\(.*\\))?$"
        );
        assert!(m.windows(2).any(|w| w == ["-testPlatform", "PlayMode"]));
        assert!(m.windows(2).any(|w| w == ["-testCategory", "Fast"]));
        assert!(!m.contains(&"-testResults".to_string()));

        // No editor: a clear error.
        let mut no_editor = ctx();
        no_editor.editor.clear();
        let mut c2 = CommandContext::default();
        c2.settings.insert(
            CONTEXT_KEY.into(),
            serde_json::to_string(&no_editor).unwrap(),
        );
        assert!(UnityTests
            .command(&cfg, &c2)
            .unwrap()
            .unwrap_err()
            .contains("Unity Hub"));

        let failed = [
            FailedTest {
                name: "Damage(-1)".into(),
                member: Some("Damage".into()),
                path: Some("Assets/Tests/EditMode/PlayerTests.cs".into()),
            },
            FailedTest {
                name: "Health_Decreases".into(),
                member: None,
                path: None,
            },
        ];
        let r = rerun(&cfg, &failed, &ctx().classes).unwrap();
        assert_eq!(
            test_filter(&r).unwrap(),
            "^(Shooter\\.Tests\\.PlayerTests\\.Damage|Health_Decreases)(\\(.*\\))?$"
        );
        assert!(rerun(&cfg, &[], &ctx().classes).is_none());
    }
}
