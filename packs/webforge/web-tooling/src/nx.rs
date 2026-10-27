//! Nx workspaces: projects from `project.json` files and `package.json` files of the
//! workspace (with their `nx` key and scripts), targets with their executors or commands,
//! `nx.json` `targetDefaults`, and the project graph (implicit dependencies and
//! dependencies between workspace packages). Targets run as `nx run <project>:<target>`.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use forge_jsonschema::tree::Node;
use forge_jsonschema::DocKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub name: String,
    /// `@nx/vite:build`, `nx:run-commands`, or `npm run <script>` for package scripts.
    pub executor: String,
    /// The file it is declared in and its 0-based line.
    pub path: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub name: String,
    /// Folder (workspace relative).
    pub root: String,
    /// `application` or `library`.
    pub kind: String,
    pub tags: Vec<String>,
    pub targets: Vec<Target>,
    /// Projects it depends on (sorted).
    pub dependencies: Vec<String>,
    /// `project.json` or `package.json`.
    pub config: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub projects: Vec<Project>,
    /// `nx.json` `targetDefaults` names, for completion and the tool window.
    pub target_defaults: Vec<String>,
    /// The workspace's default base branch (`affected`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_base: Option<String>,
}

fn parse(text: &str) -> Option<Node> {
    forge_jsonschema::parse(DocKind::Json, text)
        .into_iter()
        .next()
}

fn line_of(text: &str, byte: usize) -> u32 {
    text[..byte.min(text.len())].matches('\n').count() as u32
}

fn dir_of(path: &str) -> &str {
    path.rfind('/').map(|i| &path[..i]).unwrap_or("")
}

fn strings(n: Option<&Node>) -> Vec<String> {
    n.and_then(Node::as_array)
        .unwrap_or(&[])
        .iter()
        .filter_map(|x| x.as_str().map(str::to_string))
        .collect()
}

/// Whether the workspace is an Nx workspace (an `nx.json` in the root).
pub fn is_nx(paths: &[String]) -> bool {
    paths.iter().any(|p| p == "nx.json")
}

/// Read the workspace from its files: `nx.json`, `project.json` files and `package.json`
/// files (workspace relative paths; `node_modules` is ignored).
pub fn workspace(files: &BTreeMap<String, String>) -> Workspace {
    let mut ws = Workspace::default();
    if let Some(nx) = files.get("nx.json").and_then(|t| parse(t)) {
        ws.target_defaults = nx
            .get("targetDefaults")
            .and_then(Node::as_object)
            .unwrap_or(&[])
            .iter()
            .map(|p| p.key.clone())
            .collect();
        ws.default_base = nx
            .get("defaultBase")
            .or_else(|| nx.get("affected").and_then(|a| a.get("defaultBase")))
            .and_then(Node::as_str)
            .map(str::to_string);
    }
    // package name -> project name, for dependencies between packages.
    let mut package_names: BTreeMap<String, String> = BTreeMap::new();
    let mut package_deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut by_root: BTreeMap<String, Project> = BTreeMap::new();
    for (path, text) in files {
        if path.contains("node_modules/") {
            continue;
        }
        let name = path.rsplit('/').next().unwrap_or(path);
        if name != "project.json" && name != "package.json" {
            continue;
        }
        let Some(doc) = parse(text) else { continue };
        let root = dir_of(path).to_string();
        let folder = root.rsplit('/').next().unwrap_or(&root).to_string();
        let mut targets: Vec<Target> = Vec::new();
        let mut tags = Vec::new();
        let mut implicit = Vec::new();
        let mut project_name = None;
        let mut kind = None;
        if name == "project.json" {
            project_name = doc.get("name").and_then(Node::as_str).map(str::to_string);
            kind = doc
                .get("projectType")
                .and_then(Node::as_str)
                .map(str::to_string);
            tags = strings(doc.get("tags"));
            implicit = strings(doc.get("implicitDependencies"));
            for t in doc.get("targets").and_then(Node::as_object).unwrap_or(&[]) {
                let v = t.value.as_ref();
                let executor = v
                    .and_then(|v| v.get("executor"))
                    .and_then(Node::as_str)
                    .map(str::to_string)
                    .or_else(|| {
                        v.and_then(|v| v.get("command"))
                            .and_then(Node::as_str)
                            .map(|c| format!("nx:run-commands ({c})"))
                    })
                    .unwrap_or_default();
                targets.push(Target {
                    name: t.key.clone(),
                    executor,
                    path: path.clone(),
                    line: line_of(text, t.key_start),
                });
            }
        } else {
            // A workspace package: its scripts are targets (inferred by Nx).
            if root.is_empty() && !files.contains_key("project.json") && files.len() > 1 {
                // The root package.json is the workspace itself unless it is a project.
                let is_project = doc.get("nx").is_some();
                if !is_project {
                    continue;
                }
            }
            let pkg = doc.get("name").and_then(Node::as_str).map(str::to_string);
            if let Some(nx) = doc.get("nx") {
                project_name = nx.get("name").and_then(Node::as_str).map(str::to_string);
                tags = strings(nx.get("tags"));
                implicit = strings(nx.get("implicitDependencies"));
                for t in nx.get("targets").and_then(Node::as_object).unwrap_or(&[]) {
                    let executor = t
                        .value
                        .as_ref()
                        .and_then(|v| v.get("executor"))
                        .and_then(Node::as_str)
                        .unwrap_or("")
                        .to_string();
                    targets.push(Target {
                        name: t.key.clone(),
                        executor,
                        path: path.clone(),
                        line: line_of(text, t.key_start),
                    });
                }
            }
            for s in doc.get("scripts").and_then(Node::as_object).unwrap_or(&[]) {
                if !targets.iter().any(|t| t.name == s.key) {
                    targets.push(Target {
                        name: s.key.clone(),
                        executor: format!("npm run {}", s.key),
                        path: path.clone(),
                        line: line_of(text, s.key_start),
                    });
                }
            }
            let pname = project_name
                .clone()
                .or(pkg.clone())
                .unwrap_or(folder.clone());
            if let Some(pkg) = &pkg {
                package_names.insert(pkg.clone(), pname.clone());
            }
            let mut deps = Vec::new();
            for section in ["dependencies", "devDependencies", "peerDependencies"] {
                for d in doc.get(section).and_then(Node::as_object).unwrap_or(&[]) {
                    deps.push(d.key.clone());
                }
            }
            package_deps.insert(root.clone(), deps);
            project_name = Some(pname);
        }
        let kind = kind.unwrap_or_else(|| {
            if root.starts_with("apps/") || root.contains("/apps/") {
                "application".into()
            } else {
                "library".into()
            }
        });
        let project = Project {
            name: project_name.unwrap_or(folder),
            root: root.clone(),
            kind,
            tags,
            targets,
            dependencies: implicit,
            config: name.to_string(),
        };
        // `project.json` wins over the `package.json` of the same folder, but keeps its scripts.
        match by_root.get_mut(&root) {
            Some(existing) if existing.config == "project.json" => {
                for t in project.targets {
                    if !existing.targets.iter().any(|e| e.name == t.name) {
                        existing.targets.push(t);
                    }
                }
            }
            Some(existing) => {
                let mut p = project;
                for t in std::mem::take(&mut existing.targets) {
                    if !p.targets.iter().any(|e| e.name == t.name) {
                        p.targets.push(t);
                    }
                }
                *existing = p;
            }
            None => {
                by_root.insert(root, project);
            }
        }
    }
    let names: BTreeSet<String> = by_root.values().map(|p| p.name.clone()).collect();
    for (root, p) in by_root.iter_mut() {
        let mut deps: BTreeSet<String> = p
            .dependencies
            .iter()
            .filter(|d| !d.starts_with('!'))
            .cloned()
            .collect();
        for d in package_deps.get(root).into_iter().flatten() {
            if let Some(n) = package_names.get(d) {
                deps.insert(n.clone());
            }
        }
        deps.retain(|d| d != &p.name && names.contains(d));
        p.dependencies = deps.into_iter().collect();
    }
    ws.projects = by_root.into_values().collect();
    ws.projects.sort_by(|a, b| a.name.cmp(&b.name));
    ws
}

/// The arguments of `nx` that run a target (`run shop:build`), optionally with a
/// configuration (`run shop:build:production`).
pub fn run_args(project: &str, target: &str, configuration: Option<&str>) -> Vec<String> {
    let mut t = format!("{project}:{target}");
    if let Some(c) = configuration.filter(|c| !c.is_empty()) {
        t.push(':');
        t.push_str(c);
    }
    vec!["run".into(), t]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[(&str, &str)]) -> BTreeMap<String, String> {
        list.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn reads_projects_targets_and_graph() {
        let ws = workspace(&files(&[
            (
                "nx.json",
                r#"{ "targetDefaults": { "build": { "cache": true }, "test": {} }, "defaultBase": "main" }"#,
            ),
            (
                "package.json",
                r#"{ "name": "@acme/source", "private": true, "devDependencies": { "nx": "20" } }"#,
            ),
            (
                "apps/shop/project.json",
                r#"{
  "name": "shop",
  "projectType": "application",
  "tags": ["scope:shop"],
  "implicitDependencies": ["ui"],
  "targets": {
    "build": { "executor": "@nx/vite:build" },
    "serve": { "command": "vite serve" }
  }
}"#,
            ),
            (
                "libs/ui/package.json",
                r#"{ "name": "@acme/ui", "scripts": { "test": "vitest" } }"#,
            ),
            (
                "libs/util/package.json",
                r#"{ "name": "@acme/util", "nx": { "name": "util", "tags": ["type:util"] }, "dependencies": { "@acme/ui": "*" } }"#,
            ),
            ("node_modules/x/package.json", r#"{ "name": "x" }"#),
        ]));
        assert_eq!(ws.target_defaults, vec!["build", "test"]);
        assert_eq!(ws.default_base.as_deref(), Some("main"));
        let names: Vec<_> = ws.projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["@acme/ui", "shop", "util"]);
        let shop = &ws.projects[1];
        assert_eq!(shop.kind, "application");
        assert_eq!(
            shop.targets
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            vec!["build", "serve"]
        );
        assert_eq!(shop.targets[1].executor, "nx:run-commands (vite serve)");
        assert_eq!(shop.targets[0].line, 6);
        // `ui` is not a project name here (the package is `@acme/ui`), so it is dropped.
        assert!(shop.dependencies.is_empty());
        let util = &ws.projects[2];
        assert_eq!(util.dependencies, vec!["@acme/ui"]);
        assert_eq!(util.tags, vec!["type:util"]);
        assert_eq!(ws.projects[0].targets[0].executor, "npm run test");
        assert_eq!(
            run_args("shop", "build", Some("production")),
            vec!["run", "shop:build:production"]
        );
    }
}
