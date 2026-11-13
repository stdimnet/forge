//! dotnet command lines: builds (Build / Rebuild / Clean / Restore of the solution or a
//! project), the ".NET Project" and "Launch Settings Profile" run configurations
//! (`dotnet run`, `dotnet watch`), NuGet (`dotnet add|remove package`), `dotnet new` with
//! `dotnet sln add`, and `dotnet format`. The backend and browser mode start them through
//! the Run tool window; nothing here runs a process.

use std::collections::BTreeMap;

use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use serde::{Deserialize, Serialize};

use crate::context::DotnetContext;
use crate::paths::{join, normalize};
use crate::solution::ProjectLanguage;

/// The ".NET Project" run configuration type (Rider's ".NET Project" and "Launch
/// Settings Profile" in one: a project, optionally with a launch profile).
pub const DOTNET_RUN: &str = "dotnet";
/// The hidden type the UI uses for build, NuGet and template command lines.
pub const DOTNET_COMMAND: &str = "dotnet-command";

/// A command line (relative `cwd`: from the project root).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DotnetCommand {
    pub program: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

impl DotnetCommand {
    pub(crate) fn new(ctx: &DotnetContext, args: Vec<String>) -> Self {
        DotnetCommand {
            program: ctx.dotnet(),
            args,
            cwd: String::new(),
            env: ctx.environment(),
        }
    }

    /// How it reads in a tab title (`dotnet build Bookstore.sln`).
    pub fn display(&self) -> String {
        let program = crate::paths::file_name(&self.program)
            .trim_end_matches(".exe")
            .to_string();
        std::iter::once(program)
            .chain(self.args.iter().map(|a| {
                if a.contains(' ') {
                    format!("\"{a}\"")
                } else {
                    a.clone()
                }
            }))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn s(x: &str) -> String {
    x.to_string()
}

/// Build actions: `build`, `rebuild`, `clean`, `restore`, `test`, `pack`, `publish`,
/// `watch` (`dotnet watch build`). `target` is a solution or project (default: the active
/// solution, or the folder).
pub fn action_command(
    action: &str,
    target: Option<&str>,
    ctx: &DotnetContext,
) -> Result<DotnetCommand, String> {
    let target = target
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(normalize)
        .or_else(|| ctx.solution.clone());
    let mut args: Vec<String> = match action {
        "build" | "rebuild" => vec![s("build")],
        "clean" => vec![s("clean")],
        "restore" => vec![s("restore")],
        "test" => vec![s("test")],
        "pack" => vec![s("pack")],
        "publish" => vec![s("publish")],
        "watch" => vec![s("watch"), s("build")],
        other => return Err(format!("unknown dotnet action '{other}'")),
    };
    if action == "watch" {
        if let Some(t) = &target {
            args.insert(1, s("--project"));
            args.insert(2, t.clone());
        }
    } else if let Some(t) = &target {
        args.push(t.clone());
    }
    if action != "restore" {
        args.extend([s("-c"), ctx.configuration()]);
    }
    if action == "rebuild" {
        args.push(s("--no-incremental"));
    }
    if matches!(
        action,
        "build" | "rebuild" | "clean" | "restore" | "pack" | "publish"
    ) {
        args.push(s("-nologo"));
        let v = if ctx.verbosity.is_empty() {
            "minimal"
        } else {
            ctx.verbosity.as_str()
        };
        args.extend([s("-v"), v.to_string()]);
    }
    Ok(DotnetCommand::new(ctx, args))
}

/// The project of a run configuration: the `project` option, else the only runnable one.
fn run_project(cfg: &Configuration, ctx: &DotnetContext) -> Result<String, String> {
    let p = cfg.option("project").trim();
    if !p.is_empty() {
        return Ok(ctx
            .project(p)
            .map(|x| x.path.clone())
            .unwrap_or_else(|| normalize(p)));
    }
    let runnable: Vec<_> = ctx.projects.iter().filter(|p| p.runnable).collect();
    match runnable.as_slice() {
        [one] => Ok(one.path.clone()),
        [] => Err("Choose the project to run".into()),
        _ => Err("Choose the project to run (the solution has several)".into()),
    }
}

/// The command line of a ".NET Project" configuration. Options: `project`, `profile`
/// (launch profile; `none` for `--no-launch-profile`), `mode` (`run`, `watch`),
/// `framework`, `configuration`, `noBuild`, `dotnetArgs` (more `dotnet run` options);
/// the program arguments go after `--`.
pub fn run_command(cfg: &Configuration, ctx: &DotnetContext) -> Result<DotnetCommand, String> {
    let project = run_project(cfg, ctx)?;
    let opt = |k: &str| cfg.option(k).trim().to_string();
    let watch = opt("mode") == "watch";
    let mut args = if watch {
        vec![s("watch"), s("--project"), project.clone(), s("run")]
    } else {
        vec![s("run"), s("--project"), project.clone()]
    };
    if !opt("framework").is_empty() {
        args.extend([s("--framework"), opt("framework")]);
    }
    let configuration = match opt("configuration") {
        c if c.is_empty() => ctx.configuration(),
        c => c,
    };
    args.extend([s("-c"), configuration]);
    match opt("profile").as_str() {
        "" => {}
        "none" => args.push(s("--no-launch-profile")),
        p => args.extend([s("--launch-profile"), p.to_string()]),
    }
    if opt("noBuild") == "true" && !watch {
        args.push(s("--no-build"));
    }
    args.extend(split_args(&opt("dotnetArgs")));
    let program_args = split_args(&cfg.arguments);
    if !program_args.is_empty() {
        args.push(s("--"));
        args.extend(program_args);
    }
    let mut c = DotnetCommand::new(ctx, args);
    if watch {
        // Restart without asking on rude edits (the console cannot answer).
        c.env
            .insert("DOTNET_WATCH_RESTART_ON_RUDE_EDIT".into(), "1".into());
    }
    c.cwd = cfg.working_directory.trim().to_string();
    Ok(c)
}

/// The build before debugging a ".NET Project" configuration (`dotnet build <project>`),
/// whose output names the assembly to start.
pub fn debug_build_command(
    cfg: &Configuration,
    ctx: &DotnetContext,
) -> Result<DotnetCommand, String> {
    let project = run_project(cfg, ctx)?;
    let mut args = vec![s("build"), project, s("-c")];
    args.push(match cfg.option("configuration").trim() {
        "" => ctx.configuration(),
        c => c.to_string(),
    });
    if !cfg.option("framework").trim().is_empty() {
        args.extend([s("--framework"), cfg.option("framework").trim().to_string()]);
    }
    args.extend([s("-nologo"), s("-v"), s("minimal")]);
    Ok(DotnetCommand::new(ctx, args))
}

/// NuGet: `add` (install or change the version), `remove`.
pub fn package_command(
    action: &str,
    project: &str,
    package: &str,
    version: Option<&str>,
    prerelease: bool,
    ctx: &DotnetContext,
) -> Result<DotnetCommand, String> {
    let package = package.trim();
    if package.is_empty() || package.contains(char::is_whitespace) {
        return Err(format!("'{package}' is not a package id"));
    }
    let project = normalize(project);
    if project.is_empty() {
        return Err("Choose the project".into());
    }
    let mut args = match action {
        "add" | "update" => vec![s("add"), project, s("package"), package.to_string()],
        "remove" => vec![s("remove"), project, s("package"), package.to_string()],
        other => return Err(format!("unknown NuGet action '{other}'")),
    };
    if action != "remove" {
        match version.map(str::trim).filter(|v| !v.is_empty()) {
            Some(v) => args.extend([s("--version"), v.to_string()]),
            None if prerelease => args.push(s("--prerelease")),
            None => {}
        }
    }
    Ok(DotnetCommand::new(ctx, args))
}

/// What "Add Project from Template…" and New Project create.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewProject {
    /// Short name (`console`, `classlib`, `webapi`, `xunit`).
    pub template: String,
    pub name: String,
    /// Folder relative to the project root (default: the name, or `src/<name>` next to
    /// a solution with a `src` folder).
    pub directory: String,
    /// `C#`, `F#`, `VB` (empty: the template's default).
    pub language: String,
    /// `net8.0` (empty: the template's default).
    pub framework: String,
    /// Add it to this solution (`dotnet sln add`).
    pub solution: String,
}

/// `dotnet new <template> -n <name> -o <dir>` and `dotnet sln <sln> add <project>`.
pub fn new_project_commands(
    req: &NewProject,
    ctx: &DotnetContext,
) -> Result<Vec<DotnetCommand>, String> {
    let template = req.template.trim();
    if template.is_empty() || template.starts_with('-') || template.contains(char::is_whitespace) {
        return Err("Choose a template".into());
    }
    let name = req.name.trim();
    if name.is_empty() || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        return Err("Enter a valid project name".into());
    }
    let dir = match normalize(&req.directory) {
        d if d.is_empty() => name.to_string(),
        d => d,
    };
    let mut args = vec![
        s("new"),
        template.to_string(),
        s("-n"),
        name.to_string(),
        s("-o"),
        dir.clone(),
    ];
    if !req.language.trim().is_empty() {
        args.extend([s("-lang"), req.language.trim().to_string()]);
    }
    if !req.framework.trim().is_empty() {
        args.extend([s("--framework"), req.framework.trim().to_string()]);
    }
    let mut out = vec![DotnetCommand::new(ctx, args)];
    let solution = normalize(&req.solution);
    if !solution.is_empty()
        && !matches!(
            template,
            "sln"
                | "solution"
                | "gitignore"
                | "editorconfig"
                | "globaljson"
                | "nugetconfig"
                | "tool-manifest"
        )
    {
        let ext = match req.language.trim() {
            "F#" | "f#" | "fsharp" => ProjectLanguage::FSharp,
            "VB" | "vb" | "vbnet" => ProjectLanguage::VisualBasic,
            _ => ProjectLanguage::CSharp,
        };
        let proj = join(&dir, &format!("{name}.{}proj", ext.source_extension()));
        out.push(DotnetCommand::new(
            ctx,
            vec![s("sln"), solution, s("add"), proj],
        ));
    }
    Ok(out)
}

/// `dotnet new list` (templates installed with the SDK and `dotnet new install`).
pub fn template_list_command(ctx: &DotnetContext) -> DotnetCommand {
    DotnetCommand::new(ctx, vec![s("new"), s("list")])
}

/// What Code Cleanup runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FormatRequest {
    /// `all` (whitespace, code style and analyzers), `whitespace`, `style`, `analyzers`.
    pub scope: String,
    /// A solution, project or folder (default: the active solution).
    pub target: String,
    /// Files or folders to limit it to (relative to the project root).
    pub include: Vec<String>,
    /// `error`, `warn` (default of dotnet format) or `info` for style and analyzers.
    pub severity: String,
    /// Report what would change without changing files.
    pub verify: bool,
    /// Folder for `format-report.json`.
    pub report: String,
    /// Treat `target` as a plain folder (no project evaluation; whitespace only).
    pub folder: bool,
}

/// `dotnet format …` (EditorConfig is the source of every rule).
pub fn format_command(req: &FormatRequest, ctx: &DotnetContext) -> Result<DotnetCommand, String> {
    let mut args = vec![s("format")];
    match req.scope.as_str() {
        "" | "all" => {}
        "whitespace" | "style" | "analyzers" => args.push(req.scope.clone()),
        other => return Err(format!("unknown dotnet format scope '{other}'")),
    }
    let target = match normalize(&req.target) {
        t if t.is_empty() => ctx.solution.clone().unwrap_or_default(),
        t => t,
    };
    if !target.is_empty() {
        args.push(target);
    }
    if req.folder {
        if req.scope != "whitespace" {
            return Err("Formatting a folder without a project only fixes whitespace".into());
        }
        args.push(s("--folder"));
    }
    let include: Vec<String> = req
        .include
        .iter()
        .map(|i| normalize(i))
        .filter(|i| !i.is_empty())
        .collect();
    if !include.is_empty() {
        args.push(s("--include"));
        args.extend(include);
    }
    if !req.severity.is_empty() && req.scope != "whitespace" {
        if !matches!(req.severity.as_str(), "error" | "warn" | "info") {
            return Err(format!("unknown severity '{}'", req.severity));
        }
        args.extend([s("--severity"), req.severity.clone()]);
    }
    if req.verify {
        args.push(s("--verify-no-changes"));
    }
    if !req.report.is_empty() {
        args.extend([s("--report"), req.report.clone()]);
    }
    args.extend([s("-v"), s("minimal")]);
    Ok(DotnetCommand::new(ctx, args))
}

/// `dotnet --list-sdks`, `--list-runtimes`, `--version` probes.
pub fn probe_command(ctx: &DotnetContext, flag: &str) -> DotnetCommand {
    DotnetCommand {
        program: ctx.dotnet(),
        args: vec![flag.to_string()],
        cwd: String::new(),
        env: ctx.environment(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{bookstore_files, fixture_reader};
    use crate::workspace::load_workspace;

    fn ctx() -> DotnetContext {
        let ws = load_workspace(&bookstore_files(), &fixture_reader("bookstore"), "", "");
        DotnetContext::default().with_workspace(&ws)
    }

    #[test]
    fn builds() {
        let c = ctx();
        let b = action_command("build", None, &c).unwrap();
        assert_eq!(
            b.display(),
            "dotnet build Bookstore.sln -c Debug -nologo -v minimal"
        );
        assert_eq!(b.env["DOTNET_NOLOGO"], "1");
        let r = action_command(
            "rebuild",
            Some("src/Bookstore.Api/Bookstore.Api.csproj"),
            &c,
        )
        .unwrap();
        assert_eq!(
            r.args,
            vec![
                "build",
                "src/Bookstore.Api/Bookstore.Api.csproj",
                "-c",
                "Debug",
                "--no-incremental",
                "-nologo",
                "-v",
                "minimal"
            ]
        );
        assert_eq!(
            action_command("restore", None, &c).unwrap().args,
            vec!["restore", "Bookstore.sln", "-nologo", "-v", "minimal"]
        );
        assert_eq!(
            action_command("watch", None, &c).unwrap().args[..3],
            ["watch", "--project", "Bookstore.sln"]
        );
        assert!(action_command("explode", None, &c).is_err());
    }

    #[test]
    fn runs() {
        let c = ctx();
        let mut cfg = Configuration::new("Bookstore.Api: https", DOTNET_RUN);
        cfg.options.insert("project".into(), "Bookstore.Api".into());
        cfg.options.insert("profile".into(), "https".into());
        cfg.arguments = "--port 5000".into();
        let r = run_command(&cfg, &c).unwrap();
        assert_eq!(
            r.display(),
            "dotnet run --project src/Bookstore.Api/Bookstore.Api.csproj -c Debug --launch-profile https -- --port 5000"
        );
        cfg.options.insert("mode".into(), "watch".into());
        cfg.options.insert("profile".into(), "none".into());
        cfg.options.insert("noBuild".into(), "true".into());
        cfg.arguments.clear();
        let w = run_command(&cfg, &c).unwrap();
        assert_eq!(
            w.args,
            vec![
                "watch",
                "--project",
                "src/Bookstore.Api/Bookstore.Api.csproj",
                "run",
                "-c",
                "Debug",
                "--no-launch-profile"
            ]
        );
        assert_eq!(w.env["DOTNET_WATCH_RESTART_ON_RUDE_EDIT"], "1");
        // Two runnable projects: the project must be chosen.
        let none = Configuration::new("x", DOTNET_RUN);
        assert!(run_command(&none, &c).unwrap_err().contains("several"));
        let d = debug_build_command(&cfg, &c).unwrap();
        assert_eq!(
            d.args[..2],
            ["build", "src/Bookstore.Api/Bookstore.Api.csproj"]
        );
    }

    #[test]
    fn nuget_new_and_format() {
        let c = ctx();
        let add = package_command(
            "add",
            "src/Bookstore.Api/Bookstore.Api.csproj",
            "Serilog",
            Some("4.0.1"),
            false,
            &c,
        )
        .unwrap();
        assert_eq!(
            add.display(),
            "dotnet add src/Bookstore.Api/Bookstore.Api.csproj package Serilog --version 4.0.1"
        );
        let pre = package_command("add", "a.csproj", "X", None, true, &c).unwrap();
        assert_eq!(pre.args.last().unwrap(), "--prerelease");
        let rm = package_command("remove", "a.csproj", "X", Some("1"), false, &c).unwrap();
        assert_eq!(rm.args, vec!["remove", "a.csproj", "package", "X"]);
        assert!(package_command("add", "a.csproj", "two words", None, false, &c).is_err());
        assert!(package_command("add", "", "X", None, false, &c).is_err());

        let cmds = new_project_commands(
            &NewProject {
                template: "classlib".into(),
                name: "Bookstore.Shipping".into(),
                directory: "src/Bookstore.Shipping".into(),
                language: "F#".into(),
                framework: "net8.0".into(),
                solution: "Bookstore.sln".into(),
            },
            &c,
        )
        .unwrap();
        assert_eq!(cmds[0].display(), "dotnet new classlib -n Bookstore.Shipping -o src/Bookstore.Shipping -lang F# --framework net8.0");
        assert_eq!(
            cmds[1].display(),
            "dotnet sln Bookstore.sln add src/Bookstore.Shipping/Bookstore.Shipping.fsproj"
        );
        let cs = new_project_commands(
            &NewProject {
                template: "xunit".into(),
                name: "T".into(),
                solution: "S.sln".into(),
                ..NewProject::default()
            },
            &c,
        )
        .unwrap();
        assert_eq!(cs[1].args.last().unwrap(), "T/T.csproj");
        assert!(new_project_commands(
            &NewProject {
                template: "-x".into(),
                name: "a".into(),
                ..NewProject::default()
            },
            &c
        )
        .is_err());
        assert!(new_project_commands(
            &NewProject {
                template: "console".into(),
                name: "a/b".into(),
                ..NewProject::default()
            },
            &c
        )
        .is_err());
        assert_eq!(template_list_command(&c).args, vec!["new", "list"]);

        let f = format_command(
            &FormatRequest {
                scope: "style".into(),
                include: vec!["src/Bookstore.Api/Program.cs".into()],
                severity: "info".into(),
                report: "/tmp/r".into(),
                ..FormatRequest::default()
            },
            &c,
        )
        .unwrap();
        assert_eq!(f.display(), "dotnet format style Bookstore.sln --include src/Bookstore.Api/Program.cs --severity info --report /tmp/r -v minimal");
        let folder = format_command(
            &FormatRequest {
                scope: "whitespace".into(),
                target: "/tmp/x".into(),
                folder: true,
                include: vec!["a.cs".into()],
                ..FormatRequest::default()
            },
            &c,
        )
        .unwrap();
        assert_eq!(
            folder.args,
            vec![
                "format",
                "whitespace",
                "/tmp/x",
                "--folder",
                "--include",
                "a.cs",
                "-v",
                "minimal"
            ]
        );
        assert!(format_command(
            &FormatRequest {
                scope: "style".into(),
                folder: true,
                ..FormatRequest::default()
            },
            &c
        )
        .is_err());
        assert!(format_command(
            &FormatRequest {
                scope: "bogus".into(),
                ..FormatRequest::default()
            },
            &c
        )
        .is_err());
        assert_eq!(probe_command(&c, "--list-sdks").args, vec!["--list-sdks"]);
    }
}
