//! The Roslyn language server (`Microsoft.CodeAnalysis.LanguageServer`, the server of
//! VS Code's C# extension): how DotNetForge starts it, the solution or projects it opens
//! after `initialized` (`solution/open`, `project/open`), and its settings from Settings |
//! Languages & Frameworks | .NET (Roslyn asks for `csharp|<group>.<setting>` sections).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The language server's id in `forge-lsp` and Settings | Language Servers.
pub const SERVER_ID: &str = "roslyn";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RoslynSettings {
    /// `openFiles` (default), `fullSolution` or `none` for analyzer and compiler
    /// diagnostics.
    pub analysis_scope: String,
    pub inlay_hints: bool,
    /// Code lens with references (`3 references`).
    pub references_code_lens: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RoslynSetup {
    /// The `dotnet` host that runs the server.
    pub dotnet: String,
    /// `Microsoft.CodeAnalysis.LanguageServer.dll`.
    pub server_dll: String,
    /// Where the server writes its logs (Forge's cache folder).
    pub log_dir: String,
    /// The absolute solution path to open (else `projects`).
    pub solution: Option<String>,
    pub projects: Vec<String>,
    pub settings: RoslynSettings,
}

/// `file:///abs/path` with the characters URIs need escaped.
pub fn file_uri(path: &str) -> String {
    let p = path.replace('\\', "/");
    let p = if p.as_bytes().get(1) == Some(&b':') {
        format!("/{p}")
    } else {
        p
    };
    let mut out = String::from("file://");
    for b in p.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl RoslynSetup {
    pub fn command(&self) -> String {
        if self.dotnet.is_empty() {
            "dotnet".into()
        } else {
            self.dotnet.clone()
        }
    }

    pub fn args(&self) -> Vec<String> {
        let mut args = vec![
            self.server_dll.clone(),
            "--logLevel".into(),
            "Information".into(),
        ];
        if !self.log_dir.is_empty() {
            args.extend(["--extensionLogDirectory".into(), self.log_dir.clone()]);
        }
        args.push("--stdio".into());
        args
    }

    /// The notification that loads the code (`None`: nothing to open).
    pub fn open_notification(&self) -> Option<(String, Value)> {
        if let Some(s) = self.solution.as_deref().filter(|s| !s.is_empty()) {
            return Some(("solution/open".into(), json!({ "solution": file_uri(s) })));
        }
        if self.projects.is_empty() {
            return None;
        }
        Some((
            "project/open".into(),
            json!({ "projects": self.projects.iter().map(|p| file_uri(p)).collect::<Vec<_>>() }),
        ))
    }

    /// Answers to Roslyn's `workspace/configuration` requests.
    pub fn settings(&self) -> Value {
        let scope = match self.settings.analysis_scope.as_str() {
            "fullSolution" => "fullSolution",
            "none" => "none",
            _ => "openFiles",
        };
        let hints = self.settings.inlay_hints;
        json!({
            "csharp|background_analysis": {
                "dotnet_analyzer_diagnostics_scope": scope,
                "dotnet_compiler_diagnostics_scope": scope,
            },
            "csharp|inlay_hints": {
                "dotnet_enable_inlay_hints_for_parameters": hints,
                "dotnet_enable_inlay_hints_for_literal_parameters": hints,
                "dotnet_enable_inlay_hints_for_other_parameters": hints,
                "csharp_enable_inlay_hints_for_types": hints,
                "csharp_enable_inlay_hints_for_implicit_variable_types": hints,
                "csharp_enable_inlay_hints_for_lambda_parameter_types": hints,
                "csharp_enable_inlay_hints_for_implicit_object_creation": hints,
            },
            "csharp|code_lens": {
                "dotnet_enable_references_code_lens": self.settings.references_code_lens,
                "dotnet_enable_tests_code_lens": false,
            },
            "csharp|completion": {
                "dotnet_show_completion_items_from_unimported_namespaces": true,
                "dotnet_show_name_completion_suggestions": true,
            },
            "csharp|symbol_search": {
                "dotnet_search_reference_assemblies": true,
            },
            "csharp|formatting": {
                "dotnet_organize_imports_on_format": false,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup() {
        let s = RoslynSetup {
            dotnet: "/usr/bin/dotnet".into(),
            server_dll: "/d/roslyn/Microsoft.CodeAnalysis.LanguageServer.dll".into(),
            log_dir: "/c/roslyn".into(),
            solution: Some("/home/anna/My Projects/Bookstore/Bookstore.sln".into()),
            projects: Vec::new(),
            settings: RoslynSettings {
                analysis_scope: "fullSolution".into(),
                inlay_hints: true,
                references_code_lens: false,
            },
        };
        assert_eq!(s.command(), "/usr/bin/dotnet");
        assert_eq!(
            s.args(),
            vec![
                "/d/roslyn/Microsoft.CodeAnalysis.LanguageServer.dll",
                "--logLevel",
                "Information",
                "--extensionLogDirectory",
                "/c/roslyn",
                "--stdio"
            ]
        );
        let (method, params) = s.open_notification().unwrap();
        assert_eq!(method, "solution/open");
        assert_eq!(
            params["solution"],
            "file:///home/anna/My%20Projects/Bookstore/Bookstore.sln"
        );
        let settings = s.settings();
        assert_eq!(
            settings["csharp|background_analysis"]["dotnet_analyzer_diagnostics_scope"],
            "fullSolution"
        );
        assert_eq!(
            settings["csharp|inlay_hints"]["csharp_enable_inlay_hints_for_types"],
            true
        );
        let projects = RoslynSetup {
            projects: vec!["C:\\src\\A\\A.csproj".into()],
            ..RoslynSetup::default()
        };
        let (m, p) = projects.open_notification().unwrap();
        assert_eq!(m, "project/open");
        assert_eq!(p["projects"][0], "file:///C:/src/A/A.csproj");
        assert!(RoslynSetup::default().open_notification().is_none());
        assert_eq!(
            RoslynSetup::default().settings()["csharp|background_analysis"]
                ["dotnet_compiler_diagnostics_scope"],
            "openFiles"
        );
    }
}
