---
title: DotNetForge (.NET)
description: The DotNetForge pack - .NET SDKs and global.json, solutions (.sln, .slnx) and MSBuild projects, the Build tool window with build problems, .NET Project run configurations with launch profiles and dotnet watch, xUnit / NUnit / MSTest through dotnet test, netcoredbg debugging, the NuGet tool window, dotnet new templates, Code Cleanup with dotnet format and EditorConfig, and the Roslyn language server.
order: 121
tags: [dotnet, csharp, fsharp, dotnetforge, msbuild, nuget, roslyn, netcoredbg, xunit, nunit, mstest, dotnet-format, editorconfig, aspnetcore]
---

# DotNetForge (.NET)

**DotNetForge** brings Rider's .NET support to Forge, inside Forge's PHPStorm-style layout.
Its plugin **.NET** (`forge.dotnet`) is active in [.NET projects](../getting-started/project-detection.md):
a solution (`.sln`, `.slnx`), a project file (`.csproj`, `.fsproj`, `.vbproj`) in the root or
one level below, `global.json`, a C# or F# file in the root, or two or more in a folder.

- C#, F# and Visual Basic files are highlighted out of the box. MSBuild files (`.csproj`,
  `.fsproj`, `.vbproj`, `.props`, `.targets`, `.slnx`, `.nuspec`, `.runsettings`, `.resx`)
  are XML, Razor (`.razor`, `.cshtml`) is HTML, and `.sln` solutions have their own
  highlighting (sections, GUIDs, configurations).
- Completion, navigation, hover, rename, code actions and diagnostics for C# come from the
  **Roslyn language server** (see [below](#roslyn-language-server)), F# from FsAutoComplete
  when it is installed (`dotnet tool install --global fsautocomplete`).
- Everything else is computed in Rust (`packs/dotnetforge/*`): solutions and project files,
  launch profiles, SDK selection, build output, test discovery, TRX reports, command lines
  and NuGet answers.

![CartTests.cs with run icons on the xUnit test class and its facts and theories](../../screenshots/dotnet-editor.png)

## .NET SDK

**Settings | Languages & Frameworks | .NET** shows the `dotnet` host, its SDKs and runtimes,
and the SDK the solution builds with.

![Settings | Languages & Frameworks | .NET](../../screenshots/dotnet-settings.png)

- The host is the **dotnet executable** of the page, else `DOTNET_ROOT`, the `PATH`, the
  default install folders (`/usr/share/dotnet`, `/usr/lib/dotnet`, `/usr/local/share/dotnet`,
  Homebrew, `C:\Program Files\dotnet`) or `~/.dotnet`.
- SDKs and runtimes come from `dotnet --list-sdks` and `dotnet --list-runtimes`, run outside
  the project, or from the host's `sdk/` and `shared/` folders.
- **SDK in use** follows the dotnet host's rules: `global.json`'s `version`,
  `rollForward` (`patch`, `feature`, `minor`, `major`, `latestPatch`, `latestFeature`,
  `latestMinor`, `latestMajor`, `disable`) and `allowPrerelease`. When `global.json` asks
  for an SDK that is not installed, the page says so.
- Without an SDK, a balloon offers **Install .NET SDK…**: the platform's package manager
  (`apt`, `dnf`, `pacman`, Homebrew, winget) in the Terminal, or the download page. Forge
  never installs an SDK on its own.

## Solutions and projects

Forge reads the solution and its projects from the files, the way Rider shows them before a
design-time build:

- `.sln` (Visual Studio 2005+) and `.slnx` solutions: projects, solution folders and
  configurations (`Debug|Any CPU`). With several solutions, the Build tool window picks the
  active one (stored per project, `dotnet.solution`).
- Project files with the nearest `Directory.Build.props` and `Directory.Packages.props`:
  the SDK (`Microsoft.NET.Sdk.Web` makes a web project), `OutputType`, `TargetFramework(s)`,
  `AssemblyName`, `RootNamespace`, project, framework and package references, F# compile
  order, and properties such as `Nullable`, `LangVersion` and `Version`.
- `$(Property)` references are expanded, and simple conditions
  (`'$(Configuration)' == 'Release'`) are evaluated with the active configuration;
  conditions that depend on the target framework keep their references listed.
- **Central package management**: versions from `PackageVersion` items,
  `VersionOverride`, and implicit references (`FSharp.Core`).
- Launch profiles from `Properties/launchSettings.json` (comments and trailing commas are
  fine).
- A test project is one with `IsTestProject`, the `MSTest.Sdk`, or `Microsoft.NET.Test.Sdk`
  with xUnit, NUnit or MSTest.

The solution reloads when you save a solution, project, `Directory.*.props`,
`launchSettings.json` or `global.json` file, and with **Reload Solution**.

## Build tool window

The **Build** tool window (bottom) shows the solution on the left and the last build on the
right, like Rider's.

![The Build tool window with the solution and the problems of a failed build](../../screenshots/dotnet-build.png)

- **Solution tree**: solution folders, projects with their language and target frameworks,
  launch profiles and package references. Double-click runs a project or a launch profile.
- Toolbar: **Build**, **Rebuild**, **Clean** and **Restore NuGet Packages** of the solution
  (or the selected project), **Run** and **Debug** of the selected project or profile,
  **Reload Solution**, **Show Build Output**, the active solution and the build
  configuration (**Debug** / **Release**).
- Builds run `dotnet build` in the Run tool window. Their output is parsed in Rust: the
  compiler (C#, F#, VB), NuGet and MSBuild problems, each once, grouped by file, with the
  summary (errors, warnings, time). Click a problem to open its location; the editor shows
  the same problems (inspection *Build errors and warnings*, source `dotnet-build`).
- **Build | Build Project** (`Ctrl+F9`) builds the solution in .NET projects; the Build menu
  also has **Build Solution**, **Rebuild Solution**, **Clean Solution**, **Restore NuGet
  Packages** and **Reload Solution**.

## Run configurations

The **.NET Project** configuration type runs a project with `dotnet run`:

![Run/Debug Configurations with a .NET Project configuration and its launch profile](../../screenshots/dotnet-run-config.png)

- **Project**: the runnable projects (`Exe`, `WinExe` and web projects).
- **Launch profile**: a profile of `launchSettings.json` (`--launch-profile`), the default
  (the first one) or **None** (`--no-launch-profile`). Profiles of other kinds (IIS
  Express, Docker) are listed in the Build tool window but `dotnet run` cannot start them.
- **Run with**: `dotnet run`, or `dotnet watch` with hot reload (rude edits restart the
  program).
- **Target framework**, **Configuration** (else Settings | .NET), **Do not build** and more
  `dotnet run` options; program arguments go after `--`.
- **Build before running tests and debugging** (Settings | .NET) builds the solution through
  the Build tool window first, so its problems show up there.

## Testing

The **.NET Tests** configuration type runs xUnit, NUnit and MSTest tests with `dotnet test`
on the [shared test runner](testing.md):

![A dotnet test run with a failed FluentAssertions comparison](../../screenshots/dotnet-tests.png)

- Gutter icons on test classes and methods: xUnit `[Fact]` / `[Theory]`, NUnit `[Test]` /
  `[TestCase]` / `[TestCaseSource]`, MSTest `[TestMethod]` / `[DataTestMethod]` in C#, and
  `[<Fact>]` / `[<Test>]` functions and members in F#. Nested classes count (`Outer+Inner`);
  skipped tests (`Skip =`, `[Ignore]`) are marked.
- Scopes: the solution, a test project, a class, a method or a `--filter` expression. Class
  and method runs use `FullyQualifiedName` filters (data rows included).
- Results come from the **TRX report** (`--logger trx`) that `dotnet test` writes into
  Forge's report folder: suites per class, tests per data row, durations, output, failure
  messages with stack traces, and **Click to see difference** for xUnit, NUnit, MSTest and
  FluentAssertions comparisons. The gutter icons show the results.
- **Rerun Failed Tests** runs only the failed methods.
- `global.json` with `"test": { "runner": "Microsoft.Testing.Platform" }` switches to the new
  platform's options (`--report-trx`).
- **Debug** runs the tests with `VSTEST_HOST_DEBUG=1` and attaches netcoredbg to the test
  host when it waits for a debugger.

## Debugging

Debugging uses **netcoredbg** (MIT, Samsung), a Debug Adapter Protocol server for .NET:

- **Debug** on a .NET Project configuration builds the project, finds the built assembly in
  the build output and starts `dotnet <assembly>` under netcoredbg with the launch profile's
  arguments, working folder and environment (including `ASPNETCORE_URLS` from
  `applicationUrl`). **Step through my code only** (Just My Code) is on by default.
- netcoredbg comes from Settings | .NET's path, Forge's download, or the `PATH`. **Download…**
  fetches the pinned release from GitHub after you agree and checks the SHA-256 GitHub
  publishes for it; without one, the download is refused.

## NuGet

The **NuGet** tool window (bottom) searches the feed and manages the solution's packages.

![The NuGet tool window with a search and a package's projects](../../screenshots/dotnet-nuget.png)

- Without a search term, the list shows the packages installed in the solution, with their
  upgrades. A search asks the feed (nuget.org by default, Settings | .NET | NuGet feed)
  through Forge's Rust client; **Prerelease** includes prerelease versions.
- The details of a package show its versions and every project with the version it uses:
  **Install**, **Upgrade** and **Remove** run `dotnet add <project> package <id> --version
  <v>` / `dotnet remove <project> package <id>` in the Run tool window. With central package
  management, `dotnet` updates `Directory.Packages.props`.
- **Updates** lists the installed packages with a newer version (`Upgrade All`).
- Versions are cached in the user cache folder for six hours and used when the feed cannot
  be reached.

![NuGet upgrades of the solution's packages](../../screenshots/dotnet-nuget-updates.png)

## Templates

- **File | New Project | .NET Project** creates a solution with a project from a `dotnet
  new` template (console, class library, ASP.NET Core, Blazor, worker, xUnit, NUnit, MSTest,
  or any other template's short name), in C#, F# or Visual Basic, for a chosen framework.
- **Tools | Add Project from Template…** lists the templates `dotnet new list` reports
  (including those added with `dotnet new install`), creates the project in `src/` or
  `tests/` and adds it to the solution (`dotnet sln add`).

![Add Project from Template](../../screenshots/dotnet-add-project.png)

## Code Cleanup and formatting

- **Code | Code Cleanup…** in a .NET project runs `dotnet format` on the current file, its
  folder or the whole solution. Every rule comes from the project's `.editorconfig` files:
  whitespace, code style (`IDE*`) and analyzers (`CA*`). Settings | .NET chooses what it fixes
  (all, whitespace, style, analyzers) and the minimum severity; a balloon reports how many
  fixes `dotnet format` made.
- **Code | Reformat Code** (`Ctrl+Alt+L`) uses the Roslyn server by default. With *Reformat
  Code with: dotnet format*, Forge formats the editor text with `dotnet format whitespace`
  in a temporary copy that has the same `.editorconfig` files, so the project is not touched.

## Roslyn language server

C# code insight comes from **Microsoft.CodeAnalysis.LanguageServer** (MIT), the server of
VS Code's C# extension. It is never bundled:

- When it is missing, a balloon offers **Download…**. Forge downloads the pinned version for
  your platform from Microsoft's public `vs-impl` NuGet feed, checks it against the SHA-256
  recorded for that version, and keeps it in Forge's data folder. It runs on your .NET
  runtime (9 or newer); **Download the Roslyn Language Server…** is also in Find Action.
- Forge starts it for local projects and opens the active solution (`solution/open`), or the
  project files without one. It replaces csharp-ls while it runs.
- Settings: **Analyze** open files, the whole solution or nothing; parameter and type hints.

## Project trust

Building, running, testing, restoring, formatting and debugging run project code (MSBuild
targets, source generators, analyzers), so [Safe Mode](project-trust.md) refuses them, and
the Roslyn server and FsAutoComplete do not start. Reading the solution, project files and
launch profiles and searching NuGet stay available.

## Browser mode

`?project=dotnet` opens the `Bookstore` demo solution (an ASP.NET Core API, a class library
with two target frameworks, xUnit tests and an F# tool, with central package management);
`&build=broken` makes its build fail with compiler errors, `&roslyn=missing` starts without
the Roslyn server.
