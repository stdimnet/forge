//! The project's .NET context for command lines and the test provider: the `dotnet`
//! program, the build configuration, the active solution, the projects and where test
//! classes live (so TRX results, which name classes, reach their files).

use std::collections::BTreeMap;

use forge_output::LocationRule;
use serde::{Deserialize, Serialize};

use crate::msbuild::TestFramework;
use crate::workspace::DotnetWorkspace;

/// Key of the context in `forge_testing::CommandContext::settings`.
pub const CONTEXT_KEY: &str = "dotnet.context";

/// A project as command lines need it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub frameworks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_framework: Option<TestFramework>,
    #[serde(default)]
    pub test: bool,
    #[serde(default)]
    pub runnable: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DotnetContext {
    /// The `dotnet` executable (empty: `dotnet` on the PATH).
    pub dotnet: String,
    /// `Debug` (default) or `Release`.
    pub configuration: String,
    /// MSBuild verbosity of builds (`minimal`).
    pub verbosity: String,
    /// `KEY=VALUE` variables of every dotnet command (`dotnet.environment`).
    pub env: BTreeMap<String, String>,
    /// The active solution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solution: Option<String>,
    pub projects: Vec<ProjectInfo>,
    /// Test class full names (`Ns.Outer+Inner`) → their files.
    pub classes: BTreeMap<String, String>,
    /// `global.json`'s `test.runner` (`Microsoft.Testing.Platform`): `dotnet test` takes
    /// the new platform's options.
    pub testing_platform: bool,
    /// Windows paths.
    pub windows: bool,
}

impl DotnetContext {
    pub fn dotnet(&self) -> String {
        if self.dotnet.trim().is_empty() {
            "dotnet".into()
        } else {
            self.dotnet.clone()
        }
    }

    pub fn configuration(&self) -> String {
        if self.configuration.trim().is_empty() {
            "Debug".into()
        } else {
            self.configuration.trim().to_string()
        }
    }

    /// The context of a loaded workspace.
    pub fn with_workspace(mut self, ws: &DotnetWorkspace) -> Self {
        self.solution = ws.solution.clone();
        self.projects = ws
            .projects
            .iter()
            .map(|p| ProjectInfo {
                name: p.name.clone(),
                path: p.path.clone(),
                frameworks: p.target_frameworks.clone(),
                test_framework: p.test_framework,
                test: p.is_test_project,
                runnable: p.is_runnable(),
            })
            .collect();
        self.testing_platform = ws
            .global_json
            .as_ref()
            .and_then(|g| g.test_runner.as_deref())
            .is_some_and(|r| r.eq_ignore_ascii_case("Microsoft.Testing.Platform"));
        self
    }

    /// The project a source file belongs to (the deepest project folder above it).
    pub fn project_of_file(&self, file: &str) -> Option<&ProjectInfo> {
        let f = file.replace('\\', "/");
        self.projects
            .iter()
            .filter(|p| {
                let d = crate::paths::parent(&p.path);
                d.is_empty() || f.starts_with(&format!("{d}/"))
            })
            .max_by_key(|p| p.path.len())
    }

    pub fn project(&self, path_or_name: &str) -> Option<&ProjectInfo> {
        self.projects
            .iter()
            .find(|p| p.path == path_or_name || p.name.eq_ignore_ascii_case(path_or_name))
    }

    /// The environment of dotnet commands.
    pub fn environment(&self) -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        env.insert("DOTNET_NOLOGO".into(), "1".into());
        env.insert("DOTNET_CLI_TELEMETRY_OPTOUT".into(), "1".into());
        env.extend(self.env.clone());
        env
    }

    /// Test results name classes: map them to their files (nested classes to their
    /// outer class's file).
    pub fn location_rules(&self) -> Vec<LocationRule> {
        let mut classes: Vec<(&String, &String)> = self.classes.iter().collect();
        classes.sort_by_key(|(c, _)| std::cmp::Reverse(c.len()));
        classes
            .into_iter()
            .map(|(class, file)| LocationRule {
                pattern: format!("^{}(\\+.*)?$", regex_escape(class)),
                replacement: file.replace('$', "$$"),
            })
            .collect()
    }

    /// The test classes of a file.
    pub fn classes_in(&self, file: &str) -> Vec<&str> {
        self.classes
            .iter()
            .filter(|(_, f)| f.as_str() == file)
            .map(|(c, _)| c.as_str())
            .collect()
    }
}

/// Escape regular expression metacharacters.
pub fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\.+*?()|[]{}^$".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{bookstore_files, fixture_reader};
    use crate::workspace::load_workspace;

    #[test]
    fn context_of_the_workspace() {
        let ws = load_workspace(&bookstore_files(), &fixture_reader("bookstore"), "", "");
        let mut ctx = DotnetContext::default().with_workspace(&ws);
        assert_eq!(ctx.solution.as_deref(), Some("Bookstore.sln"));
        assert_eq!(ctx.projects.len(), 4);
        assert!(ctx.project("Bookstore.Tests").unwrap().test);
        assert_eq!(ctx.project_of_file("tests/Bookstore.Tests/CartTests.cs").unwrap().name, "Bookstore.Tests");
        assert_eq!(ctx.dotnet(), "dotnet");
        assert_eq!(ctx.configuration(), "Debug");
        assert_eq!(ctx.environment()["DOTNET_NOLOGO"], "1");
        ctx.classes.insert("Bookstore.Tests.CartTests".into(), "tests/Bookstore.Tests/CartTests.cs".into());
        let rules = ctx.location_rules();
        assert_eq!(rules[0].pattern, "^Bookstore\\.Tests\\.CartTests(\\+.*)?$");
        assert_eq!(ctx.classes_in("tests/Bookstore.Tests/CartTests.cs"), vec!["Bookstore.Tests.CartTests"]);
        assert!(!ctx.testing_platform);
    }
}
