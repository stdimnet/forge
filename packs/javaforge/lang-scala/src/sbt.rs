//! sbt builds, like IntelliJ IDEA's sbt import: the projects of a build with their Scala
//! version, dependencies and inter-project dependencies.
//!
//! - Before an import the build comes from its files ([`static_build`]): `build.sbt`
//!   (projects defined with `project`, `project.in(file(…))` or `project in file(…)`,
//!   `dependsOn`, `aggregate`, `name`, `scalaVersion` and `libraryDependencies`, string
//!   `val`s and setting sequences they use) and `project/build.properties` (`sbt.version`).
//! - An import (Reload) asks sbt itself ([`SYNC_COMMANDS`]): [`parse_sync`] reads the output
//!   of `projects` and `show scalaVersion baseDirectory libraryDependencies`, which [`merge`]
//!   puts over the static build.
//! - [`task_command`] builds `sbt` command lines for tasks (`core/test`, `~compile`,
//!   `api/runMain com.acme.Main 8080`) and the sbt shell.

use regex_lite::Regex;
use serde::{Deserialize, Serialize};

/// A library dependency (`"org.typelevel" %% "cats-core" % "2.12.0" % Test`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SbtDependency {
    pub group: String,
    pub artifact: String,
    pub version: String,
    /// `test`, `runtime`, `provided`, … (empty: compile).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub configuration: String,
    /// Cross-built for the Scala version (`%%`).
    pub cross: bool,
}

impl SbtDependency {
    pub fn id(&self) -> String {
        format!("{}:{}:{}", self.group, self.artifact, self.version)
    }
}

/// A project of an sbt build.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SbtProject {
    /// The project id (`core`), also the prefix of scoped tasks (`core/test`).
    pub id: String,
    /// `name := …`, else the id.
    pub name: String,
    /// Folder relative to the build root (empty for the root).
    pub dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scala_version: Option<String>,
    pub depends_on: Vec<String>,
    pub aggregate: Vec<String>,
    pub dependencies: Vec<SbtDependency>,
    /// The current (root) project sbt starts in.
    pub current: bool,
}

/// An sbt task the tool window lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SbtTask {
    pub name: &'static str,
    pub description: &'static str,
}

/// The tasks of every project, as IntelliJ IDEA's sbt tool window lists them.
pub const TASKS: [SbtTask; 10] = [
    SbtTask { name: "compile", description: "Compiles sources." },
    SbtTask { name: "test", description: "Executes all tests." },
    SbtTask { name: "run", description: "Runs a main class, passing along arguments provided on the command line." },
    SbtTask { name: "console", description: "Starts the Scala interpreter with the project classes on the classpath." },
    SbtTask { name: "package", description: "Produces the main artifact, such as a binary jar." },
    SbtTask { name: "clean", description: "Deletes files produced by the build, such as generated sources, compiled classes, and task caches." },
    SbtTask { name: "update", description: "Resolves and optionally retrieves dependencies, producing a report." },
    SbtTask { name: "doc", description: "Generates API documentation." },
    SbtTask { name: "publishLocal", description: "Publishes artifacts to the local Ivy repository." },
    SbtTask { name: "reload", description: "(Re)loads the current project or changes to another project." },
];

/// What the sbt tool window shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SbtBuild {
    /// `sbt.version` of `project/build.properties`, or the running sbt's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sbt_version: Option<String>,
    /// `ThisBuild / scalaVersion`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scala_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    pub projects: Vec<SbtProject>,
    /// sbt plugins of `project/plugins.sbt` (`addSbtPlugin("org.scalameta" % "sbt-scalafmt" % "2.5.2")`).
    pub plugins: Vec<SbtDependency>,
    /// The data comes from sbt (Reload), not only from the build files.
    pub synced: bool,
    /// Seconds since the Unix epoch of the import.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synced_at: Option<u64>,
}

impl SbtBuild {
    pub fn project(&self, id: &str) -> Option<&SbtProject> {
        self.projects.iter().find(|p| p.id == id)
    }

    /// The project whose folder holds a project-relative path (the deepest one).
    pub fn project_of_path(&self, path: &str) -> Option<&SbtProject> {
        let path = path.replace('\\', "/");
        self.projects
            .iter()
            .filter(|p| p.dir.is_empty() || path.starts_with(&format!("{}/", p.dir)))
            .max_by_key(|p| p.dir.len())
    }
}

/// `sbt.version` of `project/build.properties`.
pub fn parse_build_properties(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "sbt.version").then(|| v.trim().to_string())
    })
}

/// Remove `//` and `/* */` comments, keeping string literals and line breaks.
fn strip_comments(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            out.push(c);
            i += 1;
            while i < chars.len() && chars[i] != '"' && chars[i] != '\n' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    out.push(chars[i]);
                    i += 1;
                }
                out.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                out.push(chars[i]);
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                if chars[i] == '\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i += 2;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid sbt pattern")
}

/// A value: a string literal or a `val` name.
fn value(literal: Option<&str>, name: Option<&str>, vals: &[(String, String)]) -> Option<String> {
    if let Some(l) = literal {
        return Some(l.to_string());
    }
    let n = name?;
    vals.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone())
}

/// `key := "x"` or `key := someVal` in `text` (optionally scoped `ThisBuild / key`).
fn setting(text: &str, key: &str, this_build: bool, vals: &[(String, String)]) -> Option<String> {
    let scope = if this_build {
        r"ThisBuild\s*/\s*"
    } else {
        r"(?:^|[^/\w])\s*"
    };
    let r = re(&format!(r#"{scope}{key}\s*:=\s*(?:"([^"]*)"|(\w+))"#));
    let found = r.captures_iter(text).find_map(|c| {
        value(
            c.get(1).map(|m| m.as_str()),
            c.get(2).map(|m| m.as_str()),
            vals,
        )
    });
    found
}

/// The library dependencies written in `text`.
fn dependencies(text: &str, vals: &[(String, String)]) -> Vec<SbtDependency> {
    let r = re(
        r#""([^"]+)"\s*(%%%|%%|%)\s*"([^"]+)"\s*%\s*(?:"([^"]+)"|(\w+))(?:\s*%\s*(?:"([^"]+)"|([A-Z]\w*)))?"#,
    );
    r.captures_iter(text)
        .map(|c| SbtDependency {
            group: c[1].to_string(),
            artifact: c[3].to_string(),
            version: value(
                c.get(4).map(|m| m.as_str()),
                c.get(5).map(|m| m.as_str()),
                vals,
            )
            .unwrap_or_default(),
            configuration: c
                .get(6)
                .or(c.get(7))
                .map(|m| {
                    let s = m.as_str();
                    // `"compile->compile;test->test"` is a mapping, not one configuration.
                    s.split(&[';', '-'][..])
                        .next()
                        .unwrap_or(s)
                        .to_ascii_lowercase()
                })
                .filter(|s| s != "compile")
                .unwrap_or_default(),
            cross: &c[2] != "%",
        })
        .collect()
}

/// Project references of `.dependsOn(…)` or `.aggregate(…)`.
fn references(text: &str, call: &str) -> Vec<String> {
    let r = re(&format!(r"\.{call}\s*\(([^)]*)\)"));
    let mut out = Vec::new();
    for c in r.captures_iter(text) {
        for part in c[1].split(',') {
            let id = part.split('%').next().unwrap_or_default().trim();
            if !id.is_empty()
                && id.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
                && !out.iter().any(|x| x == id)
            {
                out.push(id.to_string());
            }
        }
    }
    out
}

/// The build as written in `build.sbt` (and `project/build.properties`,
/// `project/plugins.sbt`). `folder` names the root project when the build defines none.
pub fn static_build(
    build_sbt: &str,
    build_properties: Option<&str>,
    plugins_sbt: Option<&str>,
    folder: &str,
) -> SbtBuild {
    let text = strip_comments(build_sbt);
    // Top-level definitions split the file into sections.
    let def = re(r"(?m)^\s*(?:lazy\s+)?val\s+(\w+)\s*(?::\s*\w+\s*)?=");
    let starts: Vec<(usize, String)> = def
        .captures_iter(&text)
        .map(|c| (c.get(0).map_or(0, |m| m.start()), c[1].to_string()))
        .collect();
    let preamble_end = starts.first().map_or(text.len(), |(s, _)| *s);
    let mut sections: Vec<(String, &str)> = Vec::new();
    for (k, (start, name)) in starts.iter().enumerate() {
        let end = starts.get(k + 1).map_or(text.len(), |(s, _)| *s);
        sections.push((name.clone(), &text[*start..end]));
    }
    let string_val = re(r#"(?m)^\s*(?:lazy\s+)?val\s+(\w+)\s*(?::\s*String\s*)?=\s*"([^"]*)"\s*$"#);
    let vals: Vec<(String, String)> = string_val
        .captures_iter(&text)
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .collect();
    let project_def = re(
        r#"^\s*(?:lazy\s+)?val\s+\w+\s*(?::\s*Project\s*)?=\s*\(?\s*(?:project|Project\s*\(\s*"\w+"\s*,\s*file\s*\(\s*"([^"]*)"\s*\)\s*\))(?:\s*\.\s*in\s*\(\s*file\s*\(\s*"([^"]*)"\s*\)\s*\)|\s+in\s+file\s*\(\s*"([^"]*)"\s*\))?"#,
    );
    let mut projects = Vec::new();
    // Settings outside of projects (the preamble, `ThisBuild` settings) apply to the root.
    let global = &text[..preamble_end];
    for (id, body) in &sections {
        let Some(c) = project_def.captures(body) else {
            continue;
        };
        let dir = c
            .get(1)
            .or(c.get(2))
            .or(c.get(3))
            .map(|m| {
                m.as_str()
                    .trim_start_matches("./")
                    .trim_end_matches('/')
                    .to_string()
            })
            .unwrap_or_else(|| id.clone());
        let dir = if dir == "." { String::new() } else { dir };
        // Setting sequences the project uses (`.settings(commonSettings, …)`).
        let mut text_of_project = body.to_string();
        for (name, other) in &sections {
            if name != id
                && re(&format!(r"\b{name}\b")).is_match(body)
                && project_def.captures(other).is_none()
            {
                text_of_project.push('\n');
                text_of_project.push_str(other);
            }
        }
        let mut deps = dependencies(&text_of_project, &vals);
        if dir.is_empty() {
            deps.extend(dependencies(global, &vals));
        }
        projects.push(SbtProject {
            id: id.clone(),
            name: setting(body, "name", false, &vals).unwrap_or_else(|| id.clone()),
            dir,
            scala_version: setting(body, "scalaVersion", false, &vals),
            depends_on: references(body, "dependsOn"),
            aggregate: references(body, "aggregate"),
            dependencies: deps,
            current: false,
        });
    }
    let scala_version = setting(&text, "scalaVersion", true, &vals)
        .or_else(|| setting(global, "scalaVersion", false, &vals));
    if !projects.iter().any(|p| p.dir.is_empty()) {
        // sbt's default root project (named after the folder) aggregates the others.
        projects.insert(
            0,
            SbtProject {
                id: "root".into(),
                name: setting(global, "name", false, &vals).unwrap_or_else(|| folder.to_string()),
                dir: String::new(),
                scala_version: None,
                depends_on: Vec::new(),
                aggregate: if projects.is_empty() {
                    Vec::new()
                } else {
                    projects.iter().map(|p| p.id.clone()).collect()
                },
                dependencies: dependencies(global, &vals),
                current: true,
            },
        );
    } else if let Some(root) = projects.iter_mut().find(|p| p.dir.is_empty()) {
        root.current = true;
    }
    for p in &mut projects {
        if p.scala_version.is_none() {
            p.scala_version = scala_version.clone();
        }
    }
    let plugins = plugins_sbt
        .map(|t| {
            let t = strip_comments(t);
            dependencies(&t, &vals)
        })
        .unwrap_or_default();
    SbtBuild {
        sbt_version: build_properties.and_then(parse_build_properties),
        scala_version,
        organization: setting(&text, "organization", true, &vals),
        projects,
        plugins,
        synced: false,
        synced_at: None,
    }
}

/// The commands of an import, in one sbt run.
pub const SYNC_COMMANDS: [&str; 4] = [
    "projects",
    "show scalaVersion",
    "show baseDirectory",
    "show libraryDependencies",
];

/// sbt's arguments for an import (batch mode, no colors, no server).
pub fn sync_args() -> Vec<String> {
    let mut a = vec![
        "-batch".to_string(),
        "-no-colors".into(),
        "-Dsbt.server.forcestart=false".into(),
    ];
    a.extend(SYNC_COMMANDS.iter().map(|c| c.to_string()));
    a
}

/// A `ModuleID` as `show libraryDependencies` prints it: `org:name:rev[:configurations]`.
fn module_id(text: &str) -> Option<SbtDependency> {
    let mut p = text.trim().split(':');
    let group = p.next()?.trim().to_string();
    let artifact = p.next()?.trim().to_string();
    let version = p.next()?.trim().to_string();
    let configuration = p.next().unwrap_or_default().trim().to_string();
    // `org.scalameta:munit:1.0.2:test(-> default)` style configuration mappings.
    let configuration = configuration
        .split(['(', '-'])
        .next()
        .unwrap_or_default()
        .to_string();
    Some(SbtDependency {
        group,
        artifact,
        version,
        configuration: if configuration == "compile" {
            String::new()
        } else {
            configuration
        },
        cross: false,
    })
}

/// `[info] …` lines without the log level (other lines as they are).
fn message(line: &str) -> &str {
    let l = line.trim_end_matches('\r');
    for level in ["[info] ", "[warn] ", "[success] ", "[error] "] {
        if let Some(rest) = l.strip_prefix(level) {
            return rest;
        }
    }
    l
}

/// What an import found: projects with their folder, Scala version and dependencies.
pub fn parse_sync(output: &str, root: &str) -> SbtBuild {
    let mut projects: Vec<SbtProject> = Vec::new();
    let mut sbt_version = None;
    let mut current: Option<String> = None;
    let mut in_projects = false;
    // The key a `show` header named, and its project.
    let mut showing: Option<(String, String)> = None;
    let root = root.replace('\\', "/").trim_end_matches('/').to_string();
    let header = re(r"^(?:([\w-]+) / )?(\w+)$");
    let ensure = |projects: &mut Vec<SbtProject>, id: &str| -> usize {
        if let Some(i) = projects.iter().position(|p| p.id == id) {
            return i;
        }
        projects.push(SbtProject {
            id: id.to_string(),
            name: id.to_string(),
            ..Default::default()
        });
        projects.len() - 1
    };
    for line in output.lines() {
        let m = message(line);
        if let Some(rest) = m.strip_prefix("welcome to sbt ") {
            sbt_version = rest.split_whitespace().next().map(str::to_string);
            continue;
        }
        if m.starts_with("In file:") {
            in_projects = true;
            continue;
        }
        let value_line = m.starts_with('\t') || m.starts_with("  ");
        if in_projects {
            if value_line {
                let t = m.trim();
                let (star, id) = match t.strip_prefix('*') {
                    Some(id) => (true, id.trim()),
                    None => (false, t),
                };
                let i = ensure(&mut projects, id);
                projects[i].current = star;
                if star {
                    current = Some(id.to_string());
                }
                continue;
            }
            in_projects = false;
        }
        if !value_line {
            showing = header.captures(m.trim()).and_then(|c| {
                let key = c[2].to_string();
                if !matches!(
                    key.as_str(),
                    "scalaVersion" | "baseDirectory" | "libraryDependencies"
                ) {
                    return None;
                }
                let project = c
                    .get(1)
                    .map(|p| p.as_str().to_string())
                    .or_else(|| current.clone())?;
                Some((project, key))
            });
            continue;
        }
        let Some((project, key)) = &showing else {
            continue;
        };
        let i = ensure(&mut projects, project);
        let v = m.trim();
        match key.as_str() {
            "scalaVersion" => projects[i].scala_version = Some(v.to_string()),
            "baseDirectory" => {
                let dir = v.replace('\\', "/");
                projects[i].dir = dir
                    .strip_prefix(&root)
                    .map(|d| d.trim_start_matches('/').to_string())
                    .unwrap_or(dir);
            }
            _ => {
                if let Some(d) = v.strip_prefix("* ").and_then(module_id) {
                    // The Scala library sbt adds itself.
                    projects[i].dependencies.push(d);
                }
            }
        }
    }
    let scala_version = current
        .as_deref()
        .and_then(|c| projects.iter().find(|p| p.id == c))
        .and_then(|p| p.scala_version.clone());
    SbtBuild {
        sbt_version,
        scala_version,
        organization: None,
        projects,
        plugins: Vec::new(),
        synced: true,
        synced_at: None,
    }
}

/// The static build with what an import found (matched by project id). Dependencies sbt
/// resolved replace the written ones, but keep the `%%` flag of the build file.
pub fn merge(mut base: SbtBuild, synced: &SbtBuild) -> SbtBuild {
    for s in &synced.projects {
        match base.projects.iter_mut().find(|p| p.id == s.id) {
            Some(p) => {
                if s.scala_version.is_some() {
                    p.scala_version = s.scala_version.clone();
                }
                if !s.dir.is_empty() || s.current {
                    p.dir = s.dir.clone();
                }
                let written = std::mem::take(&mut p.dependencies);
                p.dependencies = s
                    .dependencies
                    .iter()
                    .map(|d| {
                        let mut d = d.clone();
                        // `cats-core` was written `%%`: the resolved name has no suffix.
                        d.cross = written
                            .iter()
                            .any(|w| w.group == d.group && w.artifact == d.artifact && w.cross);
                        d
                    })
                    .collect();
                p.current = s.current;
            }
            None => base.projects.push(s.clone()),
        }
    }
    if synced.sbt_version.is_some() {
        base.sbt_version = synced.sbt_version.clone();
    }
    if synced.scala_version.is_some() {
        base.scala_version = synced.scala_version.clone();
    }
    base.synced = true;
    base.synced_at = synced.synced_at;
    base
}

/// A task scoped to a project: `test` in `core` → `core/test`, `~compile` → `~core/compile`.
/// Tasks that name a project or scope already (`api/run`, `Test/compile`) stay as they are.
pub fn scoped_task(project: &str, task: &str) -> String {
    let task = task.trim();
    let (watch, rest) = match task.strip_prefix('~') {
        Some(r) => ("~", r.trim_start()),
        None => ("", task),
    };
    let first = rest.split_whitespace().next().unwrap_or_default();
    if project.is_empty() || first.contains('/') || rest.is_empty() {
        return format!("{watch}{rest}");
    }
    format!("{watch}{project}/{rest}")
}

/// An `sbt` command line: tasks (each one sbt command, scoped to `project`) after the JVM
/// options (`-J-Xmx2g`) and extra sbt options; no tasks starts the sbt shell.
pub fn task_command(
    program: &str,
    project: &str,
    tasks: &[String],
    vm_options: &[String],
    options: &[String],
) -> (String, Vec<String>) {
    let mut args: Vec<String> = vm_options
        .iter()
        .map(|o| {
            if o.starts_with("-J") || o.starts_with("-D") {
                o.clone()
            } else {
                format!("-J{o}")
            }
        })
        .collect();
    args.extend(options.iter().cloned());
    args.extend(
        tasks
            .iter()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .map(|t| scoped_task(project, t)),
    );
    (program.to_string(), args)
}

/// Split a task list: one sbt command per line or `;`.
pub fn split_tasks(text: &str) -> Vec<String> {
    text.split(['\n', ';'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUILD: &str = include_str!("../tests/fixtures/build.sbt");
    const PROPERTIES: &str = include_str!("../tests/fixtures/build.properties");
    const SHOW: &str = include_str!("../tests/fixtures/sbt-show.txt");
    const PROJECTS: &str = include_str!("../tests/fixtures/sbt-projects.txt");

    #[test]
    fn reads_build_sbt() {
        let plugins = "addSbtPlugin(\"org.scalameta\" % \"sbt-scalafmt\" % \"2.5.2\")\n// addSbtPlugin(\"x\" % \"y\" % \"1\")\n";
        let b = static_build(BUILD, Some(PROPERTIES), Some(plugins), "shop");
        assert_eq!(b.sbt_version.as_deref(), Some("1.10.5"));
        assert_eq!(b.scala_version.as_deref(), Some("3.3.4"));
        assert_eq!(b.organization.as_deref(), Some("com.acme"));
        assert_eq!(b.plugins.len(), 1);
        assert_eq!(b.plugins[0].artifact, "sbt-scalafmt");
        let ids: Vec<(&str, &str, &str)> = b
            .projects
            .iter()
            .map(|p| (p.id.as_str(), p.name.as_str(), p.dir.as_str()))
            .collect();
        assert_eq!(
            ids,
            [
                ("root", "shop", ""),
                ("core", "shop-core", "core"),
                ("api", "shop-api", "modules/api")
            ]
        );
        let root = &b.projects[0];
        assert!(root.current);
        assert_eq!(root.aggregate, ["core", "api"]);
        let core = &b.projects[1];
        assert_eq!(core.scala_version.as_deref(), Some("3.3.4"));
        let deps: Vec<(String, &str, bool)> = core
            .dependencies
            .iter()
            .map(|d| (d.id(), d.configuration.as_str(), d.cross))
            .collect();
        assert_eq!(
            deps,
            [
                ("org.typelevel:cats-core:2.12.0".to_string(), "", true),
                ("org.scalatest:scalatest:3.2.19".to_string(), "test", true),
                ("org.scalameta:munit:1.0.2".to_string(), "test", true),
            ]
        );
        let api = &b.projects[2];
        assert_eq!(api.scala_version.as_deref(), Some("2.13.15"));
        assert_eq!(api.depends_on, ["core"]);
        let logback = api
            .dependencies
            .iter()
            .find(|d| d.artifact == "logback-classic")
            .unwrap();
        assert_eq!(
            (logback.configuration.as_str(), logback.cross),
            ("runtime", false)
        );
        assert_eq!(
            b.project_of_path("modules/api/src/main/scala/A.scala")
                .unwrap()
                .id,
            "api"
        );
        assert_eq!(b.project_of_path("build.sbt").unwrap().id, "root");
    }

    #[test]
    fn single_project_builds_get_a_root() {
        let b = static_build(
            "scalaVersion := \"3.5.2\"\nname := \"hello\"\nlibraryDependencies += \"org.scalameta\" %% \"munit\" % \"1.0.2\" % Test\n",
            None,
            None,
            "hello-world",
        );
        assert_eq!(b.projects.len(), 1);
        let p = &b.projects[0];
        assert_eq!(
            (p.id.as_str(), p.name.as_str(), p.dir.as_str()),
            ("root", "hello", "")
        );
        assert_eq!(p.scala_version.as_deref(), Some("3.5.2"));
        assert_eq!(p.dependencies[0].artifact, "munit");
        assert!(p.current);
    }

    #[test]
    fn reads_sbt_output_and_merges_it() {
        let projects = parse_sync(PROJECTS, "/home/me/shop");
        let ids: Vec<(&str, bool)> = projects
            .projects
            .iter()
            .map(|p| (p.id.as_str(), p.current))
            .collect();
        assert_eq!(ids, [("api", false), ("core", false), ("root", true)]);

        let s = parse_sync(SHOW, "/home/me/shop/");
        assert_eq!(s.sbt_version.as_deref(), Some("1.10.5"));
        assert_eq!(s.scala_version.as_deref(), Some("3.3.4"));
        let api = s.projects.iter().find(|p| p.id == "api").unwrap();
        assert_eq!(api.dir, "modules/api");
        assert_eq!(api.scala_version.as_deref(), Some("2.13.15"));
        assert_eq!(api.dependencies.len(), 4);
        assert_eq!(api.dependencies[3].configuration, "runtime");
        let root = s.projects.iter().find(|p| p.id == "root").unwrap();
        assert_eq!(root.dir, "");
        assert_eq!(root.dependencies[0].artifact, "scala3-library");

        let merged = merge(static_build(BUILD, Some(PROPERTIES), None, "shop"), &s);
        assert!(merged.synced);
        let core = merged.project("core").unwrap();
        assert_eq!(core.dependencies.len(), 4);
        let cats = core
            .dependencies
            .iter()
            .find(|d| d.artifact == "cats-core")
            .unwrap();
        assert!(cats.cross);
        assert!(merged.project("root").unwrap().current);
        assert_eq!(merged.projects.len(), 3);
    }

    #[test]
    fn task_command_lines() {
        assert_eq!(scoped_task("core", "test"), "core/test");
        assert_eq!(scoped_task("core", "~compile"), "~core/compile");
        assert_eq!(scoped_task("core", "api/run 8080"), "api/run 8080");
        assert_eq!(scoped_task("", "test"), "test");
        let (p, a) = task_command(
            "/usr/bin/sbt",
            "api",
            &split_tasks("clean; runMain com.acme.Main 8080"),
            &["-Xmx2g".to_string()],
            &["-no-colors".to_string()],
        );
        assert_eq!(p, "/usr/bin/sbt");
        assert_eq!(
            a,
            [
                "-J-Xmx2g",
                "-no-colors",
                "api/clean",
                "api/runMain com.acme.Main 8080"
            ]
        );
        assert_eq!(sync_args()[3], "projects");
    }
}
