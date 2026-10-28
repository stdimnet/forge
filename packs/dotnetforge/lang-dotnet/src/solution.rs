//! Solutions: the classic `.sln` text format (Visual Studio 2005+) and the XML `.slnx`
//! format (Visual Studio 17.13+, `dotnet sln migrate`). Both become a [`Solution`] with its
//! projects, solution folders and configurations; paths are relative to the project root
//! and use `/`.

use serde::{Deserialize, Serialize};

use crate::paths::{join, normalize, parent};
use crate::xml;

/// The language of a project file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectLanguage {
    CSharp,
    FSharp,
    VisualBasic,
    /// `.sqlproj`, `.vcxproj`, `.njsproj`, ... (listed, not loaded).
    Other,
}

impl ProjectLanguage {
    /// By the project file's extension.
    pub fn of_path(path: &str) -> ProjectLanguage {
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".csproj") {
            ProjectLanguage::CSharp
        } else if lower.ends_with(".fsproj") {
            ProjectLanguage::FSharp
        } else if lower.ends_with(".vbproj") {
            ProjectLanguage::VisualBasic
        } else {
            ProjectLanguage::Other
        }
    }

    /// The language id of the editor and `forge-project` (`csharp`, `fsharp`, `vbnet`).
    pub fn id(self) -> &'static str {
        match self {
            ProjectLanguage::CSharp => "csharp",
            ProjectLanguage::FSharp => "fsharp",
            ProjectLanguage::VisualBasic => "vbnet",
            ProjectLanguage::Other => "",
        }
    }

    /// `dotnet new -lang` value.
    pub fn template_language(self) -> &'static str {
        match self {
            ProjectLanguage::FSharp => "F#",
            ProjectLanguage::VisualBasic => "VB",
            _ => "C#",
        }
    }

    /// Source file extension.
    pub fn source_extension(self) -> &'static str {
        match self {
            ProjectLanguage::FSharp => "fs",
            ProjectLanguage::VisualBasic => "vb",
            _ => "cs",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SolutionFormat {
    Sln,
    Slnx,
}

/// A project of a solution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolutionProject {
    pub name: String,
    /// The project file relative to the project root.
    pub path: String,
    pub language: ProjectLanguage,
    /// The project GUID (`.sln`), empty for `.slnx`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// The solution folder path (`src/Services`), empty at the top level.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
}

/// A parsed solution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Solution {
    /// The solution file relative to the project root.
    pub path: String,
    pub name: String,
    pub format: SolutionFormat,
    pub projects: Vec<SolutionProject>,
    /// Solution folders (`src`, `src/Services`), parents first.
    pub folders: Vec<String>,
    /// `Debug|Any CPU`, `Release|x64`.
    pub configurations: Vec<String>,
    /// Lines that could not be read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
}

impl Solution {
    /// The build configurations without platforms (`Debug`, `Release`).
    pub fn build_types(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for c in &self.configurations {
            let t = c.split('|').next().unwrap_or(c).trim().to_string();
            if !t.is_empty() && !out.contains(&t) {
                out.push(t);
            }
        }
        out
    }
}

const FOLDER_GUID: &str = "2150E333-8FDC-42A3-9474-1A3956D46DE8";

fn file_stem(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_string(),
        _ => name.to_string(),
    }
}

fn quoted(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('"') else { break };
        out.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    out
}

fn guid(s: &str) -> String {
    s.trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .to_ascii_uppercase()
}

/// Parse a `.sln` file; `path` is its location relative to the project root.
pub fn parse_sln(path: &str, text: &str) -> Result<Solution, String> {
    let path = normalize(path);
    let dir = parent(&path);
    let mut lines = text.lines().map(|l| l.trim_start_matches('\u{feff}'));
    let header = lines
        .by_ref()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if !header.starts_with("Microsoft Visual Studio Solution File") {
        return Err("not a Visual Studio solution file".into());
    }
    struct Entry {
        name: String,
        path: String,
        folder: bool,
        id: String,
    }
    let mut entries: Vec<Entry> = Vec::new();
    let mut nested: Vec<(String, String)> = Vec::new();
    let mut configurations = Vec::new();
    let mut problems = Vec::new();
    let mut section = String::new();
    for raw in lines {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("Project(") {
            let parts = quoted(rest);
            if parts.len() < 4 {
                problems.push(format!("cannot read the project line: {line}"));
                continue;
            }
            let folder = guid(&parts[0]) == FOLDER_GUID;
            entries.push(Entry {
                name: parts[1].clone(),
                path: if folder {
                    String::new()
                } else {
                    join(&dir, &normalize(&parts[2]))
                },
                folder,
                id: guid(&parts[3]),
            });
        } else if let Some(rest) = line.strip_prefix("GlobalSection(") {
            section = rest.split(')').next().unwrap_or("").to_string();
        } else if line == "EndGlobalSection" {
            section.clear();
        } else if let Some((k, v)) = line.split_once('=') {
            match section.as_str() {
                "SolutionConfigurationPlatforms" => {
                    let c = k.trim().to_string();
                    if !c.is_empty() && !configurations.contains(&c) {
                        configurations.push(c);
                    }
                }
                "NestedProjects" => nested.push((guid(k), guid(v))),
                _ => {}
            }
        }
    }
    // Solution folder paths through NestedProjects.
    let parent_of = |id: &str| nested.iter().find(|(c, _)| c == id).map(|(_, p)| p.clone());
    let folder_path = |id: &str| -> String {
        let mut names = Vec::new();
        let mut cur = Some(id.to_string());
        let mut guard = 0;
        while let Some(c) = cur {
            guard += 1;
            if guard > 64 {
                break;
            }
            match entries.iter().find(|e| e.id == c && e.folder) {
                Some(e) => names.push(e.name.clone()),
                None => break,
            }
            cur = parent_of(&c);
        }
        names.reverse();
        names.join("/")
    };
    let mut folders: Vec<String> = entries
        .iter()
        .filter(|e| e.folder)
        .map(|e| folder_path(&e.id))
        .collect();
    folders.sort_by_key(|f| (f.matches('/').count(), f.clone()));
    let projects = entries
        .iter()
        .filter(|e| !e.folder)
        .map(|e| SolutionProject {
            name: e.name.clone(),
            language: ProjectLanguage::of_path(&e.path),
            path: e.path.clone(),
            id: e.id.clone(),
            folder: parent_of(&e.id)
                .map(|p| folder_path(&p))
                .unwrap_or_default(),
        })
        .collect();
    Ok(Solution {
        name: file_stem(&path),
        path,
        format: SolutionFormat::Sln,
        projects,
        folders,
        configurations,
        problems,
    })
}

/// Parse a `.slnx` file; `path` is its location relative to the project root.
pub fn parse_slnx(path: &str, text: &str) -> Result<Solution, String> {
    let path = normalize(path);
    let dir = parent(&path);
    let root = xml::parse(text)?;
    if !root.name.eq_ignore_ascii_case("Solution") {
        return Err("not a .slnx solution (the root element is not <Solution>)".into());
    }
    let mut projects = Vec::new();
    let mut folders = Vec::new();
    let mut configurations = Vec::new();
    fn walk(
        el: &xml::Element,
        folder: &str,
        dir: &str,
        projects: &mut Vec<SolutionProject>,
        folders: &mut Vec<String>,
    ) {
        for c in &el.children {
            if c.name.eq_ignore_ascii_case("Project") {
                if let Some(p) = c.attr_nonempty("Path") {
                    let p = join(dir, &normalize(p));
                    projects.push(SolutionProject {
                        name: file_stem(&p),
                        language: ProjectLanguage::of_path(&p),
                        path: p,
                        id: c.attr("Id").map(guid).unwrap_or_default(),
                        folder: folder.to_string(),
                    });
                }
            } else if c.name.eq_ignore_ascii_case("Folder") {
                let name = c.attr("Name").unwrap_or("").trim_matches('/').to_string();
                if !name.is_empty() && !folders.contains(&name) {
                    folders.push(name.clone());
                }
                walk(c, &name, dir, projects, folders);
            }
        }
    }
    walk(&root, "", &dir, &mut projects, &mut folders);
    if let Some(c) = root.child("Configurations") {
        let types: Vec<&str> = c
            .children_named("BuildType")
            .filter_map(|b| b.attr_nonempty("Name"))
            .collect();
        let platforms: Vec<&str> = c
            .children_named("Platform")
            .filter_map(|b| b.attr_nonempty("Name"))
            .collect();
        let types = if types.is_empty() {
            vec!["Debug", "Release"]
        } else {
            types
        };
        let platforms = if platforms.is_empty() {
            vec!["Any CPU"]
        } else {
            platforms
        };
        for t in &types {
            for p in &platforms {
                configurations.push(format!("{t}|{p}"));
            }
        }
    } else {
        configurations = vec!["Debug|Any CPU".into(), "Release|Any CPU".into()];
    }
    folders.sort_by_key(|f| (f.matches('/').count(), f.clone()));
    Ok(Solution {
        name: file_stem(&path),
        path,
        format: SolutionFormat::Slnx,
        projects,
        folders,
        configurations,
        problems: Vec::new(),
    })
}

/// Parse a solution by its extension.
pub fn parse_solution(path: &str, text: &str) -> Result<Solution, String> {
    if path.to_ascii_lowercase().ends_with(".slnx") {
        parse_slnx(path, text)
    } else {
        parse_sln(path, text)
    }
}

/// Whether a path is a solution file.
pub fn is_solution(path: &str) -> bool {
    let l = path.to_ascii_lowercase();
    l.ends_with(".sln") || l.ends_with(".slnx")
}

/// Whether a path is a project file DotNetForge loads.
pub fn is_project_file(path: &str) -> bool {
    ProjectLanguage::of_path(path) != ProjectLanguage::Other
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn reads_sln_projects_folders_and_configurations() {
        let s = parse_sln("Bookstore.sln", &fixture("bookstore/Bookstore.sln")).unwrap();
        assert_eq!(s.name, "Bookstore");
        assert_eq!(s.format, SolutionFormat::Sln);
        let paths: Vec<&str> = s.projects.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "src/Bookstore.Api/Bookstore.Api.csproj",
                "src/Bookstore.Core/Bookstore.Core.csproj",
                "tests/Bookstore.Tests/Bookstore.Tests.csproj",
                "tools/Reports/Reports.fsproj",
            ]
        );
        assert_eq!(s.projects[0].folder, "src");
        assert_eq!(s.projects[2].folder, "tests");
        assert_eq!(s.projects[3].language, ProjectLanguage::FSharp);
        assert_eq!(s.projects[3].folder, "");
        assert_eq!(s.folders, vec!["src", "tests", "src/Services"]);
        assert_eq!(s.configurations, vec!["Debug|Any CPU", "Release|Any CPU"]);
        assert_eq!(s.build_types(), vec!["Debug", "Release"]);
        assert!(s.problems.is_empty());
    }

    #[test]
    fn sln_in_a_subfolder_and_errors() {
        let text = "Microsoft Visual Studio Solution File, Format Version 12.00\nProject(\"{9A19103F-16F7-4668-BE54-9A1E7A4F7556}\") = \"App\", \"App\\App.csproj\", \"{11111111-1111-1111-1111-111111111111}\"\nEndProject\nProject(\"broken\nGlobal\nEndGlobal\n";
        let s = parse_sln("dotnet/App.sln", text).unwrap();
        assert_eq!(s.projects[0].path, "dotnet/App/App.csproj");
        assert_eq!(s.problems.len(), 1);
        assert!(parse_sln("x.sln", "hello").is_err());
    }

    #[test]
    fn reads_slnx() {
        let s = parse_slnx("Bookstore.slnx", &fixture("bookstore/Bookstore.slnx")).unwrap();
        assert_eq!(s.format, SolutionFormat::Slnx);
        assert_eq!(s.projects.len(), 4);
        assert_eq!(s.projects[0].name, "Bookstore.Api");
        assert_eq!(s.projects[0].folder, "src");
        assert_eq!(s.projects[3].path, "tools/Reports/Reports.fsproj");
        assert_eq!(s.folders, vec!["src", "tests"]);
        assert_eq!(
            s.configurations,
            vec![
                "Debug|Any CPU",
                "Debug|x64",
                "Release|Any CPU",
                "Release|x64"
            ]
        );
        assert!(parse_slnx("x.slnx", "<Project/>").is_err());
        let minimal = parse_solution(
            "a/b.slnx",
            "<Solution><Project Path=\"c/c.vbproj\"/></Solution>",
        )
        .unwrap();
        assert_eq!(minimal.projects[0].path, "a/c/c.vbproj");
        assert_eq!(minimal.projects[0].language, ProjectLanguage::VisualBasic);
        assert_eq!(minimal.configurations.len(), 2);
    }

    #[test]
    fn kinds() {
        assert!(is_solution("A.SLN") && is_solution("b.slnx") && !is_solution("c.csproj"));
        assert!(is_project_file("x.fsproj") && !is_project_file("x.sqlproj"));
        assert_eq!(ProjectLanguage::of_path("a.vbproj").id(), "vbnet");
    }
}
