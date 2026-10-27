//! The project's .NET workspace: its solutions, the active one, the projects with their
//! implicit imports and launch profiles, and `global.json`. Built from the project's
//! solution and project file paths and a reader, so the desktop backend (files through
//! the project backend) and browser mode (WebAssembly over the demo files) share it.

use serde::{Deserialize, Serialize};

use crate::launch::{launch_settings_paths, parse_launch_settings};
use crate::msbuild::{evaluate_project, import_candidates, DotnetProject, Imports};
use crate::paths::{normalize, parent};
use crate::sdk::{parse_global_json, GlobalJson};
use crate::solution::{is_project_file, is_solution, parse_solution, Solution};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DotnetWorkspace {
    pub solutions: Vec<Solution>,
    /// The active solution's path (`None`: projects without a solution).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solution: Option<String>,
    /// The active solution's projects (or every project file without a solution).
    pub projects: Vec<DotnetProject>,
    /// Project files that are not part of the active solution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other_projects: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_json: Option<GlobalJson>,
    /// The build configuration the projects were evaluated with.
    pub configuration: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
}

impl DotnetWorkspace {
    pub fn active_solution(&self) -> Option<&Solution> {
        let p = self.solution.as_deref()?;
        self.solutions.iter().find(|s| s.path == p)
    }

    pub fn project(&self, path: &str) -> Option<&DotnetProject> {
        let p = normalize(path);
        self.projects
            .iter()
            .find(|x| x.path == p || x.name.eq_ignore_ascii_case(&p))
    }

    /// The project a source file belongs to: the deepest project folder above it.
    pub fn project_of_file(&self, file: &str) -> Option<&DotnetProject> {
        let f = normalize(file);
        self.projects
            .iter()
            .filter(|p| {
                let d = p.dir();
                d.is_empty() || f == p.path || f.starts_with(&format!("{d}/"))
            })
            .max_by_key(|p| p.dir().len())
    }

    pub fn test_projects(&self) -> impl Iterator<Item = &DotnetProject> {
        self.projects.iter().filter(|p| p.is_test_project)
    }

    pub fn runnable_projects(&self) -> impl Iterator<Item = &DotnetProject> {
        self.projects.iter().filter(|p| p.is_runnable())
    }

    /// What `dotnet build` / `test` builds without a target: the solution, else the only
    /// project (`None`: the project root, where dotnet finds one itself).
    pub fn build_target(&self) -> Option<String> {
        self.solution.clone().or_else(|| {
            (self.projects.len() == 1).then(|| self.projects[0].path.clone())
        })
    }
}

/// Pick the active solution: the preferred one, else the shallowest (`.sln` before
/// `.slnx` of the same name, then by name).
pub fn choose_solution(solutions: &[Solution], preferred: &str) -> Option<String> {
    let preferred = normalize(preferred);
    if !preferred.is_empty() {
        if let Some(s) = solutions.iter().find(|s| s.path == preferred) {
            return Some(s.path.clone());
        }
    }
    solutions
        .iter()
        .min_by_key(|s| {
            (
                s.path.matches('/').count(),
                s.path.to_ascii_lowercase().ends_with(".slnx"),
                s.path.to_ascii_lowercase(),
            )
        })
        .map(|s| s.path.clone())
}

/// Load the workspace. `files` are the project's solution and project files (relative,
/// any order); `read` returns a project file's text.
pub fn load_workspace(
    files: &[String],
    read: &dyn Fn(&str) -> Option<String>,
    preferred_solution: &str,
    configuration: &str,
) -> DotnetWorkspace {
    let configuration = if configuration.trim().is_empty() {
        "Debug".to_string()
    } else {
        configuration.trim().to_string()
    };
    let mut ws = DotnetWorkspace {
        configuration: configuration.clone(),
        ..DotnetWorkspace::default()
    };
    let mut files: Vec<String> = files.iter().map(|f| normalize(f)).collect();
    files.sort();
    files.dedup();
    for f in files.iter().filter(|f| is_solution(f)) {
        match read(f) {
            Some(text) => match parse_solution(f, &text) {
                Ok(s) => ws.solutions.push(s),
                Err(e) => ws.problems.push(format!("{f}: {e}")),
            },
            None => ws.problems.push(format!("{f}: cannot be read")),
        }
    }
    ws.solution = choose_solution(&ws.solutions, preferred_solution);
    let project_paths: Vec<String> = match ws.active_solution() {
        Some(s) => s
            .projects
            .iter()
            .filter(|p| is_project_file(&p.path))
            .map(|p| p.path.clone())
            .collect(),
        None => files.iter().filter(|f| is_project_file(f)).cloned().collect(),
    };
    ws.other_projects = files
        .iter()
        .filter(|f| is_project_file(f) && !project_paths.contains(f))
        .cloned()
        .collect();
    let nearest = |project: &str, name: &str| -> Option<(String, String)> {
        import_candidates(project, name)
            .into_iter()
            .find_map(|c| read(&c).map(|t| (c, t)))
    };
    for path in &project_paths {
        let Some(text) = read(path) else {
            ws.problems
                .push(format!("{path}: the project file of the solution does not exist"));
            continue;
        };
        let build = nearest(path, "Directory.Build.props");
        let packages = nearest(path, "Directory.Packages.props");
        let imports = Imports {
            build_props: build.as_ref().map(|(p, t)| (p.as_str(), t.as_str())),
            packages_props: packages.as_ref().map(|(p, t)| (p.as_str(), t.as_str())),
        };
        let mut project = evaluate_project(path, &text, &imports, &configuration);
        for settings in launch_settings_paths(&parent(path)) {
            if let Some(t) = read(&settings) {
                match parse_launch_settings(&t) {
                    Ok(p) => project.launch_profiles = p,
                    Err(e) => project.problems.push(format!("{settings}: {e}")),
                }
                break;
            }
        }
        ws.projects.push(project);
    }
    let global_dir = ws
        .solution
        .as_deref()
        .map(parent)
        .unwrap_or_default();
    for candidate in import_candidates(&format!("{global_dir}/x"), "global.json")
        .into_iter()
        .map(|c| c.trim_start_matches('/').to_string())
    {
        if let Some(t) = read(&candidate) {
            match parse_global_json(&t) {
                Ok(g) => ws.global_json = Some(g),
                Err(e) => ws.problems.push(e),
            }
            break;
        }
    }
    ws
}

/// Whether a project file's change should reload the workspace.
pub fn affects_workspace(path: &str) -> bool {
    let name = crate::paths::file_name(path).to_ascii_lowercase();
    is_solution(path)
        || is_project_file(path)
        || matches!(
            name.as_str(),
            "directory.build.props" | "directory.packages.props" | "global.json" | "launchsettings.json"
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{bookstore_files, fixture_reader};

    #[test]
    fn loads_the_bookstore_solution() {
        let read = fixture_reader("bookstore");
        let ws = load_workspace(&bookstore_files(), &read, "", "");
        assert_eq!(ws.solution.as_deref(), Some("Bookstore.sln"));
        assert_eq!(ws.solutions.len(), 2);
        assert_eq!(ws.configuration, "Debug");
        let names: Vec<&str> = ws.projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["Bookstore.Api", "Bookstore.Core", "Bookstore.Tests", "Reports"]);
        let api = ws.project("src/Bookstore.Api/Bookstore.Api.csproj").unwrap();
        assert_eq!(api.launch_profiles.len(), 3);
        assert_eq!(api.package("Swashbuckle.AspNetCore").unwrap().version, "6.5.0");
        assert_eq!(ws.global_json.as_ref().unwrap().version.as_deref(), Some("8.0.400"));
        assert_eq!(ws.project_of_file("src/Bookstore.Core/Catalog/Cart.cs").unwrap().name, "Bookstore.Core");
        assert!(ws.project_of_file("README.md").is_none());
        assert_eq!(ws.test_projects().count(), 1);
        assert_eq!(ws.runnable_projects().count(), 2);
        assert_eq!(ws.build_target().as_deref(), Some("Bookstore.sln"));
        assert!(ws.problems.is_empty(), "{:?}", ws.problems);
        // The (missing) Services folder project and the preferred .slnx.
        let slnx = load_workspace(&bookstore_files(), &read, "Bookstore.slnx", "Release");
        assert_eq!(slnx.solution.as_deref(), Some("Bookstore.slnx"));
        assert_eq!(slnx.configuration, "Release");
    }

    #[test]
    fn projects_without_a_solution() {
        let files = vec!["App/App.csproj".to_string(), "Lib/Lib.fsproj".to_string()];
        let read = |p: &str| match p {
            "App/App.csproj" => Some("<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net9.0</TargetFramework></PropertyGroup></Project>".to_string()),
            "Lib/Lib.fsproj" => Some("<Project Sdk=\"Microsoft.NET.Sdk\"></Project>".to_string()),
            _ => None,
        };
        let ws = load_workspace(&files, &read, "", "Debug");
        assert!(ws.solution.is_none());
        assert_eq!(ws.projects.len(), 2);
        assert_eq!(ws.build_target(), None);
        let missing = load_workspace(
            &["S.sln".to_string()],
            &|p: &str| (p == "S.sln").then(|| "Microsoft Visual Studio Solution File, Format Version 12.00\nProject(\"{9A19103F-16F7-4668-BE54-9A1E7A4F7556}\") = \"Gone\", \"Gone\\Gone.csproj\", \"{1}\"\nEndProject\n".to_string()),
            "",
            "",
        );
        assert_eq!(missing.problems.len(), 1);
        assert!(missing.problems[0].contains("does not exist"));
    }

    #[test]
    fn reload_triggers() {
        assert!(affects_workspace("src/A/A.csproj"));
        assert!(affects_workspace("src/A/Properties/launchSettings.json"));
        assert!(affects_workspace("Directory.Packages.props"));
        assert!(!affects_workspace("src/A/Program.cs"));
    }
}
