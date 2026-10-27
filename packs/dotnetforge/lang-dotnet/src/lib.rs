//! DotNetForge's .NET support, in the style of Rider. Pure Rust (no I/O), shared by the
//! desktop backend and the WebAssembly build of browser mode.
//!
//! - [`solution`]: `.sln` and `.slnx` solutions (projects, solution folders,
//!   configurations).
//! - [`msbuild`]: project files with `Directory.Build.props` and central package
//!   management (target frameworks, output type, references, package references, test
//!   frameworks).
//! - [`workspace`]: the solution, its evaluated projects and their launch profiles.
//! - [`launch`]: `launchSettings.json` profiles.
//! - [`sdk`]: `dotnet --list-sdks` / `--list-runtimes`, `global.json` and the SDK the host
//!   picks, install hints.
//! - [`build`]: MSBuild / compiler output as problems and built outputs.
//! - [`scan`]: xUnit / NUnit / MSTest tests and `Main` methods of C# and F# files.
//! - [`context`] and [`provider`]: the .NET test provider (`dotnet test`, TRX reports,
//!   filters, Rerun Failed Tests).
//! - [`command`]: build, run / watch, NuGet, `dotnet new` and `dotnet format` command
//!   lines.
//! - [`templates`]: `dotnet new list`.
//! - [`nuget`] and [`version`]: NuGet v3 answers and NuGet versions.
//! - [`format`]: `dotnet format` reports and EditorConfig lookup.
//! - [`roslyn`]: the Roslyn language server's command line, `solution/open` and settings.

pub mod build;
pub mod command;
pub mod context;
pub mod format;
pub mod launch;
pub mod msbuild;
pub mod nuget;
pub mod paths;
pub mod provider;
pub mod roslyn;
pub mod scan;
pub mod sdk;
pub mod solution;
pub mod templates;
pub mod version;
pub mod workspace;
mod xml;

pub use build::{parse_build_output, BuildOutput, BuildProblem, BuildReport, Severity};
pub use command::{DotnetCommand, FormatRequest, NewProject, DOTNET_COMMAND, DOTNET_RUN};
pub use context::{DotnetContext, ProjectInfo, CONTEXT_KEY};
pub use launch::LaunchProfile;
pub use msbuild::{DotnetProject, PackageReference, TestFramework};
pub use provider::{discover_file, DotnetTests, DOTNET_TEST};
pub use solution::{ProjectLanguage, Solution, SolutionProject};
pub use workspace::{load_workspace, DotnetWorkspace};

/// Whether a path belongs to .NET (sources, projects, solutions, launch settings).
pub fn is_dotnet_path(path: &str) -> bool {
    let l = path.to_ascii_lowercase();
    [".cs", ".fs", ".fsi", ".fsx", ".vb", ".csx", ".razor", ".cshtml"]
        .iter()
        .any(|e| l.ends_with(e))
        || workspace::affects_workspace(path)
}

#[cfg(test)]
pub(crate) mod tests {
    pub fn fixture(name: &str) -> String {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// A reader of the files below a fixture folder.
    pub fn fixture_reader(dir: &str) -> impl Fn(&str) -> Option<String> {
        let base = format!("{}/tests/fixtures/{dir}", env!("CARGO_MANIFEST_DIR"));
        move |p: &str| std::fs::read_to_string(format!("{base}/{p}")).ok()
    }

    pub fn bookstore_files() -> Vec<String> {
        [
            "Bookstore.sln",
            "Bookstore.slnx",
            "src/Bookstore.Api/Bookstore.Api.csproj",
            "src/Bookstore.Core/Bookstore.Core.csproj",
            "tests/Bookstore.Tests/Bookstore.Tests.csproj",
            "tools/Reports/Reports.fsproj",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    #[test]
    fn dotnet_paths() {
        assert!(super::is_dotnet_path("src/A/Program.cs"));
        assert!(super::is_dotnet_path("A.slnx"));
        assert!(!super::is_dotnet_path("README.md"));
    }
}
