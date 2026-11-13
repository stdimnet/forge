//! .NET Aspire, like Rider's Aspire plugin: AppHost projects (`IsAspireHost`, the
//! `Aspire.AppHost.Sdk` or the `Aspire.Hosting.AppHost` package), the resources their
//! `Program.cs` declares (`builder.AddProject<Projects.Api>("api")`, `AddRedis`,
//! `AddPostgres(…).AddDatabase(…)`, `AddContainer`, …) with their references, and what a
//! running AppHost prints: the dashboard's login URL and when the application started.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use forge_runtools::Configuration;

use crate::command::DotnetCommand;
use crate::context::DotnetContext;
use crate::csharp::{arguments, matching, tokenize, Token};
use crate::msbuild::DotnetProject;

/// The ".NET Aspire" run configuration type: `dotnet run` of an AppHost.
pub const DOTNET_ASPIRE: &str = "dotnet-aspire";

/// The command line of a ".NET Aspire" configuration (`project`, `profile`, `mode`
/// `run` / `watch`, `configuration`, `dotnetArgs` as for ".NET Project").
pub fn run_command(cfg: &Configuration, ctx: &DotnetContext) -> Result<DotnetCommand, String> {
    let mut c = cfg.clone();
    c.type_id = crate::command::DOTNET_RUN.into();
    crate::command::run_command(&c, ctx)
}

/// Whether a project is an Aspire AppHost.
pub fn is_app_host(p: &DotnetProject) -> bool {
    p.properties
        .get("IsAspireHost")
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
        || p.sdk
            .as_deref()
            .is_some_and(|s| s.to_ascii_lowercase().starts_with("aspire.apphost.sdk"))
        || p.packages
            .iter()
            .any(|r| r.id.eq_ignore_ascii_case("Aspire.Hosting.AppHost"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AspireResource {
    pub name: String,
    /// `project`, `container`, `redis`, `postgres`, `database`, `parameter`, … (from
    /// `Add<Kind>`, lowercased).
    pub kind: String,
    /// The project type (`Projects.Bookstore_Api`), container image or other detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The project file of a `project` resource (resolved by the caller).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// The resource it belongs to (`AddDatabase` on a server).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// `WithReference(…)` / `WaitFor(…)` targets by resource name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<String>,
    /// `WithExternalHttpEndpoints()`.
    #[serde(default)]
    pub external: bool,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppHost {
    pub name: String,
    pub path: String,
    /// The file the resources were read from.
    pub program: String,
    pub resources: Vec<AspireResource>,
}

/// The resources an AppHost's `Program.cs` declares.
pub fn scan_app_host(text: &str) -> Vec<AspireResource> {
    let tokens = tokenize(text);
    let mut out: Vec<AspireResource> = Vec::new();
    let mut vars: BTreeMap<String, usize> = BTreeMap::new();
    let mut target: Option<String> = None;
    // The resource the current chain is on.
    let mut current: Option<usize> = None;
    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        if t.is(';') || t.is('{') || t.is('}') {
            if let (Some(v), Some(c)) = (target.take(), current) {
                vars.insert(v, c);
            }
            target = None;
            current = None;
            i += 1;
            continue;
        }
        let Some(id) = t.ident() else {
            i += 1;
            continue;
        };
        let after_dot = i > 0 && tokens[i - 1].is('.');
        if !after_dot {
            if tokens.get(i + 1).is_some_and(|n| n.is('='))
                && !tokens.get(i + 2).is_some_and(|n| n.is('=') || n.is('>'))
            {
                target = Some(id.to_string());
                i += 2;
                continue;
            }
            current = vars.get(id).copied();
            i += 1;
            continue;
        }
        // A call: `.Name(` or `.Name<T>(`.
        let mut open = i + 1;
        let mut generic: Option<String> = None;
        if tokens.get(open).is_some_and(|n| n.is('<')) {
            let close = matching(&tokens, open);
            generic = Some(crate::csharp::text(
                &tokens[open + 1..close.saturating_sub(1)],
            ));
            open = close;
        }
        if !tokens.get(open).is_some_and(|n| n.is('(')) {
            i += 1;
            continue;
        }
        let args = arguments(&tokens, open);
        let first_string = args.first().and_then(|a| match a {
            [s] => s.string().map(str::to_string),
            _ => None,
        });
        let end = matching(&tokens, open);
        if let (Some(kind), Some(name)) = (id.strip_prefix("Add"), first_string.clone()) {
            if !kind.is_empty() && kind != "ServiceDefaults" {
                let kind_l = kind.to_ascii_lowercase();
                let detail = match kind_l.as_str() {
                    "project" => generic.clone(),
                    "container" => args
                        .get(1)
                        .and_then(|a| a.first())
                        .and_then(Token::string)
                        .map(|img| {
                            match args.get(2).and_then(|a| a.first()).and_then(Token::string) {
                                Some(tag) => format!("{img}:{tag}"),
                                None => img.to_string(),
                            }
                        }),
                    "npmapp" | "executable" | "nodeapp" => args
                        .get(1)
                        .and_then(|a| a.first())
                        .and_then(Token::string)
                        .map(str::to_string),
                    "connectionstring" | "parameter" => None,
                    _ => None,
                };
                let parent = current
                    .filter(|_| matches!(kind_l.as_str(), "database"))
                    .map(|p| out[p].name.clone());
                out.push(AspireResource {
                    name,
                    kind: kind_l,
                    detail,
                    project: None,
                    parent,
                    references: Vec::new(),
                    external: false,
                    line: t.line,
                });
                current = Some(out.len() - 1);
                i = end;
                continue;
            }
        }
        if let Some(c) = current {
            match id {
                "WithReference" | "WaitFor" | "WaitForCompletion" => {
                    let target_name = args
                        .first()
                        .and_then(|a| a.first())
                        .and_then(Token::ident)
                        .and_then(|v| vars.get(v))
                        .map(|&r| out[r].name.clone());
                    if let Some(n) = target_name {
                        if !out[c].references.contains(&n) {
                            out[c].references.push(n);
                        }
                    }
                }
                "WithExternalHttpEndpoints" => out[c].external = true,
                _ => {}
            }
        }
        i = end;
    }
    out
}

/// The project file of a `Projects.Bookstore_Api` type: the project whose name is the type
/// name with `_` for `.`.
pub fn project_of<'a>(detail: &str, projects: &'a [DotnetProject]) -> Option<&'a DotnetProject> {
    let t = detail.rsplit('.').next().unwrap_or(detail);
    projects
        .iter()
        .find(|p| p.name.replace(['.', '-', ' '], "_").eq_ignore_ascii_case(t))
}

/// The AppHosts of a workspace with their resources (`read` reads a project file's
/// sibling `Program.cs`).
pub fn app_hosts(
    projects: &[DotnetProject],
    read: &dyn Fn(&str) -> Option<String>,
) -> Vec<AppHost> {
    projects
        .iter()
        .filter(|p| is_app_host(p))
        .map(|p| {
            let program = crate::paths::join(&p.dir(), "Program.cs");
            let mut resources = read(&program)
                .or_else(|| read(&crate::paths::join(&p.dir(), "AppHost.cs")))
                .map(|t| scan_app_host(&t))
                .unwrap_or_default();
            for r in &mut resources {
                if r.kind == "project" {
                    r.project = r
                        .detail
                        .as_deref()
                        .and_then(|d| project_of(d, projects))
                        .map(|x| x.path.clone());
                }
            }
            AppHost {
                name: p.name.clone(),
                path: p.path.clone(),
                program,
                resources,
            }
        })
        .collect()
}

/// What a running AppHost printed so far.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AspireOutput {
    /// The dashboard login URL (`https://localhost:17037/login?t=…`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dashboard_url: Option<String>,
    /// `Distributed application started.`
    #[serde(default)]
    pub started: bool,
}

/// Scan AppHost output for the dashboard URL and the start.
pub fn parse_output(text: &str) -> AspireOutput {
    let mut out = AspireOutput::default();
    for line in text.lines() {
        let l = strip_ansi(line);
        if let Some(i) = l.find("Login to the dashboard at ") {
            let url = l[i + "Login to the dashboard at ".len()..]
                .split_whitespace()
                .next()
                .unwrap_or("");
            if url.starts_with("http") {
                out.dashboard_url = Some(url.trim_end_matches('.').to_string());
            }
        } else if out.dashboard_url.is_none()
            && l.contains("Now listening on:")
            && l.contains("Aspire.Hosting.DistributedApplication")
        {
            if let Some(url) = l.split("Now listening on:").nth(1).map(str::trim) {
                out.dashboard_url = Some(url.to_string());
            }
        }
        if l.contains("Distributed application started") {
            out.started = true;
        }
    }
    out
}

pub(crate) fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn scans_resources() {
        let r = scan_app_host(&fixture("aspire/Program.cs"));
        let rows: Vec<String> = r
            .iter()
            .map(|r| {
                format!(
                    "{} {} {:?} {:?} {:?}",
                    r.kind, r.name, r.detail, r.parent, r.references
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                "redis cache None None []",
                "postgres pg None None []",
                "database bookstore None Some(\"pg\") []",
                "container mail Some(\"axllent/mailpit:v1.20\") None []",
                "project api Some(\"Projects.Bookstore_Api\") None [\"bookstore\", \"cache\", \"pg\"]",
                "npmapp web Some(\"../web\") None [\"api\"]",
            ]
        );
        assert!(r[4].external);
        assert_eq!(r[4].line, 9);
    }

    #[test]
    fn parses_output() {
        let o = parse_output("info: Aspire.Hosting.DistributedApplication[0]\n      Aspire version: 9.0.0\n\u{1b}[40minfo\u{1b}[0m: Aspire.Hosting.DistributedApplication[0]\n      Login to the dashboard at https://localhost:17037/login?t=4a1b2c\ninfo: Aspire.Hosting.DistributedApplication[0]\n      Distributed application started. Press Ctrl+C to shut down.\n");
        assert_eq!(
            o.dashboard_url.as_deref(),
            Some("https://localhost:17037/login?t=4a1b2c")
        );
        assert!(o.started);
        assert_eq!(parse_output("hello"), AspireOutput::default());
    }
}
