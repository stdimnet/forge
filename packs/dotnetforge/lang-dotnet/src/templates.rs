//! `dotnet new list`: the installed project and item templates (the SDK's and those added
//! with `dotnet new install`), read from the table the command prints.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub name: String,
    /// `console`; some templates have several (`webapp,razor`).
    pub short_names: Vec<String>,
    /// `C#`, `F#`, `VB` (the default first).
    pub languages: Vec<String>,
    pub tags: Vec<String>,
    /// A project template (not an item such as `gitignore` or `class`).
    pub project: bool,
}

impl Template {
    pub fn short_name(&self) -> &str {
        self.short_names.first().map(String::as_str).unwrap_or("")
    }
}

/// Item templates (files, not projects).
const ITEM_TAGS: &[&str] = &["Config", "Item", "Solution", "MSBuild"];

/// Parse the table of `dotnet new list` (column widths from the dashes line).
pub fn parse_template_list(text: &str) -> Vec<Template> {
    let lines: Vec<&str> = text.lines().collect();
    let Some(dash_index) = lines
        .iter()
        .position(|l| l.trim_start().starts_with("---") && l.contains("  "))
    else {
        return Vec::new();
    };
    let dashes = lines[dash_index];
    // Column start offsets (in chars).
    let chars: Vec<char> = dashes.chars().collect();
    let mut starts = Vec::new();
    let mut prev = ' ';
    for (i, c) in chars.iter().enumerate() {
        if *c == '-' && prev == ' ' {
            starts.push(i);
        }
        prev = *c;
    }
    let header: Vec<char> = lines
        .get(dash_index.wrapping_sub(1))
        .copied()
        .unwrap_or("")
        .chars()
        .collect();
    let column = |row: &[char], k: usize| -> String {
        let start = starts[k];
        let end = starts.get(k + 1).copied().unwrap_or(row.len());
        if start >= row.len() {
            return String::new();
        }
        row[start..end.min(row.len())].iter().collect::<String>().trim().to_string()
    };
    let names: Vec<String> = (0..starts.len()).map(|k| column(&header, k).to_ascii_lowercase()).collect();
    let find = |label: &str, fallback: usize| names.iter().position(|n| n.starts_with(label)).unwrap_or(fallback);
    let (name_col, short_col, lang_col, tags_col) = (
        find("template", 0),
        find("short", 1),
        find("lang", 2),
        find("tags", 3),
    );
    let mut out = Vec::new();
    for l in &lines[dash_index + 1..] {
        if l.trim().is_empty() {
            continue;
        }
        let row: Vec<char> = l.chars().collect();
        let name = column(&row, name_col);
        let short = column(&row, short_col);
        if name.is_empty() || short.is_empty() {
            continue;
        }
        let split = |s: String, sep: char| -> Vec<String> {
            s.split(sep)
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect()
        };
        let mut languages: Vec<String> = Vec::new();
        for lang in split(column(&row, lang_col), ',') {
            if let Some(d) = lang.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
                languages.insert(0, d.to_string());
            } else {
                languages.push(lang);
            }
        }
        let tags = split(column(&row, tags_col), '/');
        let project = !tags.iter().any(|t| ITEM_TAGS.contains(&t.as_str())) && !languages.is_empty();
        out.push(Template {
            name,
            short_names: split(short, ','),
            languages,
            tags,
            project,
        });
    }
    out
}

/// Templates offered when `dotnet new list` cannot run (no SDK in browser mode or tests).
pub fn common_templates() -> Vec<Template> {
    let t = |name: &str, short: &str, langs: &[&str], tags: &str| Template {
        name: name.into(),
        short_names: vec![short.into()],
        languages: langs.iter().map(|s| s.to_string()).collect(),
        tags: tags.split('/').map(str::to_string).collect(),
        project: true,
    };
    vec![
        t("Console App", "console", &["C#", "F#", "VB"], "Common/Console"),
        t("Class Library", "classlib", &["C#", "F#", "VB"], "Common/Library"),
        t("ASP.NET Core Web API", "webapi", &["C#", "F#"], "Web/Web API/API/Service"),
        t("ASP.NET Core Empty", "web", &["C#", "F#"], "Web/Empty"),
        t("ASP.NET Core Web App (Model-View-Controller)", "mvc", &["C#", "F#"], "Web/MVC"),
        t("ASP.NET Core Web App (Razor Pages)", "webapp", &["C#"], "Web/MVC/Razor Pages"),
        t("Blazor Web App", "blazor", &["C#"], "Web/Blazor/WebAssembly"),
        t("Worker Service", "worker", &["C#", "F#"], "Common/Worker/Web"),
        t("ASP.NET Core gRPC Service", "grpc", &["C#"], "Web/gRPC/API/Service"),
        t("xUnit Test Project", "xunit", &["C#", "F#", "VB"], "Test/xUnit/Desktop/Web"),
        t("NUnit 3 Test Project", "nunit", &["C#", "F#", "VB"], "Test/NUnit/Desktop/Web"),
        t("MSTest Test Project", "mstest", &["C#", "F#", "VB"], "Test/MSTest/Desktop/Web"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn reads_the_table() {
        let t = parse_template_list(&fixture("dotnet-new-list.txt"));
        assert_eq!(t.len(), 9);
        let console = t.iter().find(|x| x.short_name() == "console").unwrap();
        assert_eq!(console.name, "Console App");
        assert_eq!(console.languages, vec!["C#", "F#", "VB"]);
        assert_eq!(console.tags, vec!["Common", "Console"]);
        assert!(console.project);
        let webapp = t.iter().find(|x| x.name.starts_with("ASP.NET Core Web App (Razor")).unwrap();
        assert_eq!(webapp.short_names, vec!["webapp", "razor"]);
        assert_eq!(webapp.languages, vec!["C#"]);
        let gitignore = t.iter().find(|x| x.short_name() == "gitignore").unwrap();
        assert!(!gitignore.project);
        let sln = t.iter().find(|x| x.short_name() == "sln").unwrap();
        assert!(!sln.project);
        assert!(parse_template_list("No templates found.").is_empty());
        assert!(common_templates().iter().all(|t| t.project));
    }
}
