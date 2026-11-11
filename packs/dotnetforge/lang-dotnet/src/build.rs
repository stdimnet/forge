//! `dotnet build` / `msbuild` output: the canonical error format of the C#, F# and VB
//! compilers, NuGet and MSBuild (`path(line,col): error CS0103: message [project]`) as
//! problems for the Build tool window and the editor, the built outputs
//! (`Bookstore.Api -> /…/bin/Debug/net8.0/Bookstore.Api.dll`) and the summary.

use serde::{Deserialize, Serialize};

use crate::paths::{normalize, relative_to};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// One diagnostic of a build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildProblem {
    pub severity: Severity,
    /// `CS0103`, `NU1603`, `MSB3270`, `FS0039` (may be empty).
    pub code: String,
    pub message: String,
    /// The file relative to the project root when it is inside it, else as reported
    /// (empty for problems without a location, such as `MSBUILD : error`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    /// How the editor opens `path` when it is a file outside the project: a read-only
    /// `library://file/…` path, set by the app (empty otherwise).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub library: String,
    /// 0-based lines and columns (columns count characters, as the compilers do).
    #[serde(default)]
    pub line: u32,
    #[serde(default)]
    pub column: u32,
    #[serde(default)]
    pub end_line: u32,
    #[serde(default)]
    pub end_column: u32,
    /// The output named a line (and column): `line` 0 and `column` 0 are then `:1:1`, not
    /// "no position".
    #[serde(default)]
    pub located: bool,
    /// The project that reported it, relative to the project root.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project: String,
    /// What reported a problem without a location (`MSBUILD`, `CSC`, `NETSDK`, a project).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origin: String,
}

impl BuildProblem {
    /// A link to the documentation of the code (Roslyn, NuGet, MSBuild, .NET SDK).
    pub fn help_url(&self) -> Option<String> {
        let c = self.code.to_ascii_uppercase();
        let lower = c.to_ascii_lowercase();
        if c.starts_with("CS") {
            Some(format!(
                "https://learn.microsoft.com/dotnet/csharp/misc/{lower}"
            ))
        } else if c.starts_with("NU") {
            Some(format!(
                "https://learn.microsoft.com/nuget/reference/errors-and-warnings/{lower}"
            ))
        } else if c.starts_with("MSB") {
            Some(format!(
                "https://learn.microsoft.com/visualstudio/msbuild/errors/{lower}"
            ))
        } else if c.starts_with("NETSDK") {
            Some(format!(
                "https://learn.microsoft.com/dotnet/core/tools/sdk-errors/{lower}"
            ))
        } else if c.starts_with("CA") || c.starts_with("IDE") {
            Some(format!(
                "https://learn.microsoft.com/dotnet/fundamentals/code-analysis/{}/{lower}",
                if c.starts_with("CA") {
                    "quality-rules"
                } else {
                    "style-rules"
                }
            ))
        } else {
            None
        }
    }
}

/// A built output (`Project -> path`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildOutput {
    pub project: String,
    pub path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildReport {
    pub problems: Vec<BuildProblem>,
    pub outputs: Vec<BuildOutput>,
    /// `Build succeeded.` / `Build FAILED.` (`None`: no summary, e.g. cancelled).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub succeeded: Option<bool>,
    pub errors: u32,
    pub warnings: u32,
    /// `Time Elapsed 00:00:04.12`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub elapsed: String,
}

fn severity(word: &str) -> Option<Severity> {
    match word.to_ascii_lowercase().as_str() {
        "error" | "fatal error" => Some(Severity::Error),
        "warning" => Some(Severity::Warning),
        "info" | "message" => Some(Severity::Info),
        _ => None,
    }
}

/// `message [/abs/Project.csproj]` → (message, project).
fn split_project(rest: &str) -> (String, String) {
    let t = rest.trim_end();
    if t.ends_with(']') {
        if let Some(open) = t.rfind(" [") {
            let proj = &t[open + 2..t.len() - 1];
            let lower = proj.to_ascii_lowercase();
            if lower.ends_with("proj")
                || lower.ends_with(".sln")
                || lower.ends_with(".slnx")
                || lower.contains("proj::")
            {
                let proj = proj.split("::").next().unwrap_or(proj);
                return (t[..open].trim().to_string(), proj.to_string());
            }
        }
    }
    (t.trim().to_string(), String::new())
}

/// `severity CODE: message` after the location.
fn split_kind(rest: &str) -> Option<(Severity, String, String)> {
    let r = rest.trim_start();
    let (head, message) = r.split_once(':')?;
    let mut words = head.split_whitespace();
    let first = words.next()?;
    let (sev, code) = if first.eq_ignore_ascii_case("fatal") {
        let second = words.next()?;
        if !second.eq_ignore_ascii_case("error") {
            return None;
        }
        (Severity::Error, words.next().unwrap_or(""))
    } else {
        (severity(first)?, words.next().unwrap_or(""))
    };
    if words.next().is_some() {
        return None;
    }
    if !code.is_empty() && !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some((sev, code.to_string(), message.trim_start().to_string()))
}

fn location(loc: &str) -> Option<(String, [u32; 4])> {
    // `path(line,col)`, `path(line,col,endLine,endCol)`, `path(line)`.
    let loc = loc.trim_end();
    if !loc.ends_with(')') {
        return None;
    }
    let open = loc.rfind('(')?;
    let nums: Vec<u32> = loc[open + 1..loc.len() - 1]
        .split(',')
        .map(|n| n.trim().parse::<u32>())
        .collect::<Result<_, _>>()
        .ok()?;
    if nums.is_empty() || nums.len() > 4 || loc[..open].trim().is_empty() {
        return None;
    }
    let line = nums[0].saturating_sub(1);
    let col = nums.get(1).copied().unwrap_or(1).saturating_sub(1);
    let (el, ec) = match nums.len() {
        4 => (nums[2].saturating_sub(1), nums[3].saturating_sub(1)),
        _ => (line, col),
    };
    Some((loc[..open].trim().to_string(), [line, col, el, ec]))
}

fn rel(root: &str, path: &str) -> String {
    let p = path.trim();
    if p.is_empty() {
        return String::new();
    }
    let n = normalize(p);
    if n.starts_with('/') || n.as_bytes().get(1) == Some(&b':') {
        relative_to(root, &n).unwrap_or(n)
    } else {
        n
    }
}

/// One line of build output as a problem.
pub fn parse_problem_line(line: &str, root: &str) -> Option<BuildProblem> {
    let l = line.trim();
    if l.is_empty() || l.len() > 8000 {
        return None;
    }
    // Strip a leading `  1>` (msbuild -m node prefix) or timestamps.
    let l = match l.find('>') {
        Some(i) if i <= 4 && l[..i].chars().all(|c| c.is_ascii_digit()) && i > 0 => {
            l[i + 1..].trim_start()
        }
        _ => l,
    };
    // Find `: <severity>` that follows the location / origin.
    let mut search = 0;
    while let Some(i) = l[search..].find(": ") {
        let at = search + i;
        let (head, rest) = (&l[..at], &l[at + 1..]);
        if let Some((sev, code, msg)) = split_kind(rest) {
            let (message, project) = split_project(&msg);
            let located = location(head).is_some();
            let (path, nums, origin) = match location(head) {
                Some((p, n)) => (p, n, String::new()),
                None => {
                    let h = head.trim();
                    // `MSBUILD`, `CSC`, `/abs/App.csproj` (problems of a project file),
                    // `EXEC`; never a sentence.
                    if h.is_empty()
                        || h.contains("  ")
                        || (h.contains(' ') && !h.contains('/') && !h.contains('\\'))
                    {
                        search = at + 2;
                        continue;
                    }
                    let lower = h.to_ascii_lowercase();
                    if lower.ends_with("proj")
                        || lower.ends_with(".props")
                        || lower.ends_with(".targets")
                        || lower.ends_with(".sln")
                        || lower.ends_with(".slnx")
                    {
                        (h.to_string(), [0; 4], String::new())
                    } else {
                        (String::new(), [0; 4], h.to_string())
                    }
                }
            };
            return Some(BuildProblem {
                severity: sev,
                code,
                message,
                path: rel(root, &path),
                library: String::new(),
                line: nums[0],
                column: nums[1],
                end_line: nums[2],
                end_column: nums[3],
                located,
                project: rel(root, &project),
                origin,
            });
        }
        search = at + 2;
    }
    None
}

/// The whole output of a build: problems (each once: MSBuild repeats them in its
/// summary), outputs and the summary.
pub fn parse_build_output(output: &str, root: &str) -> BuildReport {
    let mut report = BuildReport::default();
    for raw in output.lines() {
        let line = raw.trim_end_matches('\r');
        let t = line.trim();
        if t == "Build succeeded."
            || t.starts_with("Build succeeded with")
            || t.starts_with("Build succeeded in")
        {
            report.succeeded = Some(true);
            continue;
        }
        if t == "Build FAILED."
            || t.starts_with("Build failed with")
            || t.starts_with("Build failed in")
        {
            report.succeeded = Some(false);
            continue;
        }
        if let Some(rest) = t.strip_prefix("Time Elapsed ") {
            report.elapsed = rest.trim().to_string();
            continue;
        }
        if let Some((name, path)) = t.split_once(" -> ") {
            let name = name.trim();
            if !name.is_empty() && !name.contains(' ') && !path.contains(": ") {
                let p = rel(root, path.trim());
                if !report.outputs.iter().any(|o| o.path == p) {
                    report.outputs.push(BuildOutput {
                        project: name.to_string(),
                        path: p,
                    });
                }
                continue;
            }
        }
        if let Some(p) = parse_problem_line(line, root) {
            if !report.problems.contains(&p) {
                report.problems.push(p);
            }
        }
    }
    report.errors = report
        .problems
        .iter()
        .filter(|p| p.severity == Severity::Error)
        .count() as u32;
    report.warnings = report
        .problems
        .iter()
        .filter(|p| p.severity == Severity::Warning)
        .count() as u32;
    if report.succeeded.is_none() && report.errors > 0 {
        report.succeeded = Some(false);
    }
    report
}

/// The main assembly a build produced for a project (`.dll`, or the apphost for
/// self-contained outputs).
pub fn output_of<'a>(report: &'a BuildReport, project_name: &str) -> Option<&'a BuildOutput> {
    report
        .outputs
        .iter()
        .filter(|o| o.project.eq_ignore_ascii_case(project_name))
        .find(|o| o.path.to_ascii_lowercase().ends_with(".dll"))
        .or_else(|| {
            report
                .outputs
                .iter()
                .find(|o| o.project.eq_ignore_ascii_case(project_name))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    const ROOT: &str = "/home/anna/Projects/Bookstore";

    #[test]
    fn reads_the_failed_build() {
        let r = parse_build_output(&fixture("dotnet-build-failed.txt"), ROOT);
        assert_eq!(r.succeeded, Some(false));
        assert_eq!((r.errors, r.warnings), (2, 2), "{:#?}", r.problems);
        let cs = r.problems.iter().find(|p| p.code == "CS8618").unwrap();
        assert_eq!(cs.code, "CS8618");
        assert_eq!(cs.severity, Severity::Warning);
        assert_eq!(cs.path, "src/Bookstore.Core/Catalog/Book.cs");
        assert_eq!((cs.line, cs.column), (9, 19));
        assert_eq!(cs.project, "src/Bookstore.Core/Bookstore.Core.csproj");
        assert!(cs.message.starts_with("Non-nullable property 'Title'"));
        let err = r.problems.iter().find(|p| p.code == "CS0103").unwrap();
        assert_eq!(err.path, "src/Bookstore.Api/Program.cs");
        assert_eq!((err.line, err.column), (11, 29));
        assert_eq!(
            err.message,
            "The name 'inventry' does not exist in the current context"
        );
        assert!(cs.located);
        let nu = r.problems.iter().find(|p| p.code == "NU1603").unwrap();
        assert_eq!(nu.path, "tests/Bookstore.Tests/Bookstore.Tests.csproj");
        assert!(!nu.located);
        // The first character of a file is a position too.
        let first = parse_problem_line(
            "/home/anna/Projects/Bookstore/src/A.cs(1,1): error CS1001: Identifier expected [/home/anna/Projects/Bookstore/src/A.csproj]",
            ROOT,
        )
        .unwrap();
        assert_eq!((first.line, first.column, first.located), (0, 0, true));
        let fs = r.problems.iter().find(|p| p.code == "FS0039").unwrap();
        assert_eq!(
            (
                fs.path.as_str(),
                fs.line,
                fs.column,
                fs.end_line,
                fs.end_column
            ),
            ("tools/Reports/Sales.fs", 4, 10, 4, 17)
        );
        assert_eq!(r.elapsed, "00:00:03.71");
        assert_eq!(r.outputs.len(), 1);
        assert_eq!(
            r.outputs[0].path,
            "src/Bookstore.Core/bin/Debug/net8.0/Acme.Bookstore.Core.dll"
        );
        assert_eq!(
            cs.help_url().unwrap(),
            "https://learn.microsoft.com/dotnet/csharp/misc/cs8618"
        );
    }

    #[test]
    fn reads_a_successful_build_and_odd_lines() {
        let r = parse_build_output(&fixture("dotnet-build-ok.txt"), ROOT);
        assert_eq!(r.succeeded, Some(true));
        assert!(r.problems.is_empty());
        assert_eq!(r.outputs.len(), 4);
        assert_eq!(
            output_of(&r, "Bookstore.Api").unwrap().path,
            "src/Bookstore.Api/bin/Debug/net8.0/Bookstore.Api.dll"
        );
        assert!(output_of(&r, "Nope").is_none());
        let msb = parse_problem_line(
            "MSBUILD : error MSB1009: Project file does not exist.",
            ROOT,
        )
        .unwrap();
        assert_eq!(
            (msb.origin.as_str(), msb.code.as_str(), msb.path.as_str()),
            ("MSBUILD", "MSB1009", "")
        );
        let fatal = parse_problem_line(
            "CSC : fatal error CS2001: Source file 'x.cs' could not be found.",
            ROOT,
        )
        .unwrap();
        assert_eq!(fatal.severity, Severity::Error);
        let node = parse_problem_line(
            "  3>/home/anna/Projects/Bookstore/a.cs(1,2): error CS1002: ; expected",
            ROOT,
        )
        .unwrap();
        assert_eq!(node.path, "a.cs");
        assert!(parse_problem_line("Determining projects to restore...", ROOT).is_none());
        assert!(
            parse_problem_line("  Warning: this sentence is not a diagnostic: really", ROOT)
                .is_none()
        );
        assert!(parse_problem_line("Restore complete (0.4s)", ROOT).is_none());
        let outside = parse_problem_line(
            "/opt/sdk/Microsoft.Common.targets(2413,5): warning MSB3277: Conflicts",
            ROOT,
        )
        .unwrap();
        assert_eq!(outside.path, "/opt/sdk/Microsoft.Common.targets");
        let win = parse_problem_line("C:\\src\\Shop\\Program.cs(3,1): error CS0246: Type 'X' not found [C:\\src\\Shop\\Shop.csproj]", "C:/src/Shop").unwrap();
        assert_eq!(
            (win.path.as_str(), win.project.as_str()),
            ("Program.cs", "Shop.csproj")
        );
    }
}
