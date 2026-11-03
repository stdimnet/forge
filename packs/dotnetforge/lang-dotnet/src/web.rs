//! The solution-wide models of the web and cloud features, from a loaded workspace and
//! the source files (path → text), shared by the desktop backend and browser mode:
//! endpoints of the web projects with their base URLs, Blazor components and the EF Core
//! model.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::aspnet::{self, Endpoint};
use crate::efcore::{self, EfModel};
use crate::msbuild::DotnetProject;
use crate::razor::{self, RazorComponent};
use crate::workspace::DotnetWorkspace;

/// Whether a source path is build output or a dependency folder.
fn generated(path: &str) -> bool {
    path.split('/')
        .any(|s| s == "bin" || s == "obj" || s == "node_modules")
}

/// The files below the folders of the projects `keep` accepts.
fn in_projects<'a>(
    files: &'a BTreeMap<String, String>,
    ws: &DotnetWorkspace,
    keep: impl Fn(&DotnetProject) -> bool,
) -> impl Iterator<Item = (&'a String, &'a String)> + 'a {
    let dirs: Vec<String> = ws
        .projects
        .iter()
        .filter(|p| keep(p))
        .map(|p| p.dir())
        .collect();
    files.iter().filter(move |(p, _)| {
        !generated(p)
            && dirs
                .iter()
                .any(|d| d.is_empty() || p.starts_with(&format!("{d}/")))
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointsModel {
    pub endpoints: Vec<Endpoint>,
    /// Project file → the URL it listens on (launch profiles).
    pub base_urls: BTreeMap<String, String>,
}

/// The endpoints of the web projects (`.cs`, `.razor` and `.cshtml` files).
pub fn endpoints(ws: &DotnetWorkspace, files: &BTreeMap<String, String>) -> EndpointsModel {
    let mut endpoints = Vec::new();
    for (path, text) in in_projects(files, ws, |p| p.web) {
        for mut e in aspnet::scan_file(path, text) {
            e.project = ws.project_of_file(path).map(|p| p.path.clone());
            endpoints.push(e);
        }
    }
    aspnet::sort(&mut endpoints);
    EndpointsModel {
        endpoints,
        base_urls: ws
            .projects
            .iter()
            .filter(|p| p.web)
            .map(|p| (p.path.clone(), aspnet::base_url(&p.launch_profiles)))
            .collect(),
    }
}

/// The `.http` request of an endpoint against its project's URL.
pub fn http_request(ws: &DotnetWorkspace, e: &Endpoint) -> String {
    let base = e
        .project
        .as_deref()
        .and_then(|p| ws.project(p))
        .map(|p| aspnet::base_url(&p.launch_profiles))
        .unwrap_or_else(|| aspnet::base_url(&[]));
    aspnet::http_request(e, &base)
}

/// Blazor components of `.razor` files and their `.razor.cs` code-behind.
pub fn components(ws: &DotnetWorkspace, files: &BTreeMap<String, String>) -> Vec<RazorComponent> {
    let razor_files: BTreeMap<String, String> = files
        .iter()
        .filter(|(p, _)| !generated(p) && (p.ends_with(".razor") || p.ends_with(".razor.cs")))
        .map(|(p, t)| (p.clone(), t.clone()))
        .collect();
    let mut out = razor::components(&razor_files);
    for c in &mut out {
        c.project = ws.project_of_file(&c.path).map(|p| p.path.clone());
    }
    out
}

/// The EF Core projects, DbContexts and migrations (`tools` is `.config/dotnet-tools.json`).
pub fn ef_model(
    ws: &DotnetWorkspace,
    files: &BTreeMap<String, String>,
    tools: Option<&str>,
) -> EfModel {
    let projects = efcore::ef_projects(&ws.projects);
    let paths: Vec<String> = projects.iter().map(|p| p.path.clone()).collect();
    let mut model = EfModel {
        local_tool: tools.is_some_and(efcore::has_local_tool),
        ..EfModel::default()
    };
    for (path, text) in in_projects(files, ws, |p| paths.contains(&p.path)) {
        if !path.ends_with(".cs") {
            continue;
        }
        let (contexts, migrations) = efcore::scan_source(path, text);
        let project = ws.project_of_file(path).map(|p| p.path.clone());
        model.contexts.extend(contexts.into_iter().map(|mut c| {
            c.project = project.clone();
            c
        }));
        model.migrations.extend(migrations.into_iter().map(|mut m| {
            m.project = project.clone();
            m
        }));
    }
    model.migrations.sort_by(|a, b| a.id.cmp(&b.id));
    model.projects = projects;
    model
}

/// What a chunk of run output says: the Aspire dashboard URL, Hot Reload events.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOutput {
    pub aspire: crate::aspire::AspireOutput,
    pub watch: Vec<crate::watch::WatchEvent>,
}

pub fn run_output(text: &str) -> RunOutput {
    RunOutput {
        aspire: crate::aspire::parse_output(text),
        watch: crate::watch::parse_watch_output(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{bookstore_files, fixture, fixture_reader};

    fn ws() -> DotnetWorkspace {
        crate::load_workspace(
            &bookstore_files(),
            &fixture_reader("bookstore"),
            "",
            "Debug",
        )
    }

    #[test]
    fn endpoints_of_web_projects_only() {
        let mut files = BTreeMap::new();
        files.insert(
            "src/Bookstore.Api/Program.cs".to_string(),
            fixture("bookstore/src/Bookstore.Api/Program.cs"),
        );
        files.insert(
            "src/Bookstore.Core/Catalog/Fake.cs".to_string(),
            "app.MapGet(\"/not-web\", () => 1);".to_string(),
        );
        files.insert(
            "src/Bookstore.Api/obj/Gen.cs".to_string(),
            "app.MapGet(\"/gen\", () => 1);".to_string(),
        );
        let m = endpoints(&ws(), &files);
        assert!(!m.endpoints.is_empty());
        assert!(m
            .endpoints
            .iter()
            .all(|e| e.file == "src/Bookstore.Api/Program.cs"));
        assert_eq!(
            m.endpoints[0].project.as_deref(),
            Some("src/Bookstore.Api/Bookstore.Api.csproj")
        );
        assert!(
            m.base_urls["src/Bookstore.Api/Bookstore.Api.csproj"].starts_with("http://localhost:")
        );
        let r = http_request(&ws(), &m.endpoints[0]);
        assert!(r.contains(&m.base_urls["src/Bookstore.Api/Bookstore.Api.csproj"]));
    }
}
