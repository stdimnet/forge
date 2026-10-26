//! The project model of the Gradle and Maven tool windows: projects (the root, Gradle
//! subprojects, Maven modules), tasks and goals, Maven plugins and profiles, and resolved
//! dependency trees per configuration (Gradle) or with their scope (Maven).
//!
//! Before a sync the model comes from the build files ([`static_model`]). Reload (sync)
//! asks the build tool for the real data:
//!
//! - Gradle runs [`GRADLE_INIT_SCRIPT`] (`gradle -I forge-model.gradle forgeModel`), which
//!   writes every project's tasks and resolved dependencies as JSON ([`parse_gradle_model`]);
//! - Maven runs `dependency:tree -DoutputFile=… -DappendOutput=true`, whose text
//!   [`parse_maven_tree`] reads (also the `[INFO]` log form).
//!
//! [`merge`] puts the synced data over the static model.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::deps::{self, Declared};
use crate::detect::{BuildProject, BuildTool};
use crate::goals::{goals, Goal};

/// A dependency in a resolved tree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DependencyNode {
    /// `com.google.guava:guava:33.3.1-jre`, `project :core`.
    pub id: String,
    pub group: String,
    pub artifact: String,
    pub version: String,
    /// The requested version when conflict resolution picked another (`1.0 -> 1.2`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested: Option<String>,
    /// Maven scope (`compile`, `test`, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Already listed above (Gradle `(*)`, Maven "omitted for duplicate").
    pub omitted: bool,
    /// Could not be resolved.
    pub failed: bool,
    pub children: Vec<DependencyNode>,
}

impl DependencyNode {
    fn from_coordinates(id: &str) -> DependencyNode {
        let mut parts = id.split(':');
        let group = parts.next().unwrap_or_default().to_string();
        let artifact = parts.next().unwrap_or_default().to_string();
        let version = parts.next().unwrap_or_default().to_string();
        DependencyNode {
            id: id.to_string(),
            group,
            artifact,
            version,
            ..Default::default()
        }
    }
}

/// A Gradle configuration (`compileClasspath`) or Maven's dependencies.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DependencyScope {
    pub name: String,
    pub description: String,
    pub dependencies: Vec<DependencyNode>,
}

/// A Maven plugin and its goals (`compiler` → `compiler:compile`, …).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MavenPlugin {
    /// `org.apache.maven.plugins:maven-compiler-plugin`.
    pub id: String,
    /// `compiler`.
    pub prefix: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    pub goals: Vec<String>,
}

/// A Maven profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub active_by_default: bool,
}

/// A project of the build: the root, a Gradle subproject or a Maven module.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectModel {
    pub name: String,
    /// Gradle path (`:`, `:app`); the module folder for Maven.
    pub path: String,
    /// Folder relative to the build's root (empty for the root).
    pub dir: String,
    /// Gradle tasks or Maven lifecycle phases.
    pub tasks: Vec<Goal>,
    pub dependencies: Vec<DependencyScope>,
    pub plugins: Vec<MavenPlugin>,
    /// Dependencies the build file declares (before a sync, and for inspections).
    pub declared: Vec<Declared>,
}

/// What the tool window shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildModel {
    pub tool: BuildTool,
    pub projects: Vec<ProjectModel>,
    /// Maven profiles of all modules.
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// The Gradle version of the last sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradle_version: Option<String>,
    /// The data comes from the build tool (Reload), not only from the build files.
    #[serde(default)]
    pub synced: bool,
    /// Seconds since the Unix epoch of the sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synced_at: Option<u64>,
}

/// The init script of a Gradle sync: `forgeModel` writes the projects, their tasks and the
/// resolved dependencies of the usual classpath configurations to the file named by the
/// system property `forge.model.out`. Run with `--no-configuration-cache`.
pub const GRADLE_INIT_SCRIPT: &str = r#"// Forge: dumps the projects, tasks and resolved dependencies of the build as JSON.
import groovy.json.JsonOutput

def forgeConfigurations = ['compileClasspath', 'runtimeClasspath', 'testCompileClasspath', 'testRuntimeClasspath', 'annotationProcessor']

def forgeDependencies(c) {
    def seen = new HashSet()
    def walk
    walk = { result, int depth ->
        if (result instanceof org.gradle.api.artifacts.result.ResolvedDependencyResult) {
            def id = result.selected.id
            def coords = id instanceof org.gradle.api.artifacts.component.ModuleComponentIdentifier ? "${id.group}:${id.module}:${id.version}".toString() : id.displayName
            def requested = result.requested.displayName
            def repeat = !seen.add(coords)
            [id: coords, requested: requested, omitted: repeat,
             children: (repeat || depth > 40) ? [] : result.selected.dependencies.collect { walk(it, depth + 1) }]
        } else {
            [id: result.requested.displayName, failed: true, children: []]
        }
    }
    try {
        return c.incoming.resolutionResult.root.dependencies.collect { walk(it, 0) }
    } catch (Exception e) {
        return [[id: 'Could not resolve: ' + e.message, failed: true, children: []]]
    }
}

gradle.rootProject { root ->
    root.tasks.register('forgeModel') { t ->
        if (t.metaClass.respondsTo(t, 'notCompatibleWithConfigurationCache', String)) {
            t.notCompatibleWithConfigurationCache('Forge reads the project model')
        }
        t.doLast {
            def projects = root.allprojects.collect { p ->
                [name: p.name,
                 path: p.path,
                 dir: root.projectDir.toPath().relativize(p.projectDir.toPath()).toString().replace('\\', '/'),
                 tasks: p.tasks.collect { task -> [name: task.name, group: task.group ?: 'other', description: task.description ?: ''] },
                 dependencies: forgeConfigurations.collect { n -> p.configurations.findByName(n) }
                     .findAll { it != null && it.canBeResolved }
                     .collect { c -> [name: c.name, description: c.description ?: '', dependencies: forgeDependencies(c)] }]
            }
            new File(System.getProperty('forge.model.out')).text =
                JsonOutput.toJson([gradleVersion: root.gradle.gradleVersion, projects: projects])
        }
    }
}
"#;

fn node_from_json(v: &Value) -> DependencyNode {
    let id = v.get("id").and_then(Value::as_str).unwrap_or_default();
    let mut node = if id.starts_with("project ") || id.matches(':').count() < 2 {
        DependencyNode {
            id: id.to_string(),
            ..Default::default()
        }
    } else {
        DependencyNode::from_coordinates(id)
    };
    let requested = v
        .get("requested")
        .and_then(Value::as_str)
        .unwrap_or_default();
    // `com.x:y` (BOM managed) or `com.x:y:1.0` while 1.2 won.
    if let Some(req_version) = requested.split(':').nth(2) {
        if !req_version.is_empty() && req_version != node.version {
            node.requested = Some(req_version.to_string());
        }
    }
    node.omitted = v.get("omitted").and_then(Value::as_bool).unwrap_or(false);
    node.failed = v.get("failed").and_then(Value::as_bool).unwrap_or(false);
    node.children = v
        .get("children")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(node_from_json).collect())
        .unwrap_or_default();
    node
}

/// Read the JSON the init script wrote.
pub fn parse_gradle_model(json: &str) -> Result<BuildModel, String> {
    let v: Value =
        serde_json::from_str(json).map_err(|e| format!("unexpected Gradle model: {e}"))?;
    let projects = v
        .get("projects")
        .and_then(Value::as_array)
        .ok_or("the Gradle model has no projects")?;
    let mut out = Vec::new();
    for p in projects {
        let s = |k: &str| {
            p.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let mut tasks: Vec<Goal> = p
            .get("tasks")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|t| Goal {
                        name: t
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .into(),
                        group: t
                            .get("group")
                            .and_then(Value::as_str)
                            .unwrap_or("other")
                            .into(),
                        description: t
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .into(),
                    })
                    .filter(|g| !g.name.is_empty() && g.name != "forgeModel")
                    .collect()
            })
            .unwrap_or_default();
        tasks.sort_by(|a, b| a.group.cmp(&b.group).then_with(|| a.name.cmp(&b.name)));
        let dependencies = p
            .get("dependencies")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|c| DependencyScope {
                        name: c
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .into(),
                        description: c
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .into(),
                        dependencies: c
                            .get("dependencies")
                            .and_then(Value::as_array)
                            .map(|d| d.iter().map(node_from_json).collect())
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.push(ProjectModel {
            name: s("name"),
            path: s("path"),
            dir: s("dir"),
            tasks,
            dependencies,
            ..Default::default()
        });
    }
    Ok(BuildModel {
        tool: BuildTool::Gradle,
        projects: out,
        profiles: Vec::new(),
        gradle_version: v
            .get("gradleVersion")
            .and_then(Value::as_str)
            .map(str::to_string),
        synced: true,
        synced_at: None,
    })
}

/// A Maven coordinate of `dependency:tree`: `g:a:type[:classifier]:v[:scope]`.
fn maven_node(text: &str) -> DependencyNode {
    let mut text = text.trim();
    let mut omitted = false;
    let mut requested = None;
    // Verbose output: `(g:a:jar:1.0:compile - omitted for duplicate)`.
    if text.starts_with('(') && text.ends_with(')') {
        let inner = &text[1..text.len() - 1];
        omitted = inner.contains("omitted");
        text = inner.split(" - ").next().unwrap_or(inner);
    }
    let optional = text.ends_with("(optional)");
    let text = text.trim_end_matches("(optional)").trim();
    let coords = text.split_whitespace().next().unwrap_or(text);
    if let Some(i) = text.find("(version managed from ") {
        requested = text[i + 22..]
            .split([';', ')'])
            .next()
            .map(|s| s.trim().to_string());
    }
    let p: Vec<&str> = coords.split(':').collect();
    let (version, scope) = match p.len() {
        4 => (p[3], None),
        5 => (p[3], Some(p[4])),
        n if n >= 6 => (p[4], Some(p[5])),
        _ => ("", None),
    };
    DependencyNode {
        id: format!(
            "{}:{}:{version}",
            p.first().unwrap_or(&""),
            p.get(1).unwrap_or(&"")
        ),
        group: p.first().unwrap_or(&"").to_string(),
        artifact: p.get(1).unwrap_or(&"").to_string(),
        version: version.to_string(),
        requested,
        scope: scope.map(|s| {
            if optional {
                format!("{s}, optional")
            } else {
                s.to_string()
            }
        }),
        omitted,
        failed: false,
        children: Vec::new(),
    }
}

/// The trees of `mvn dependency:tree`, one per module: `(groupId:artifactId, tree)`. Reads
/// the plain `-DoutputFile` text and the `[INFO]` log.
pub fn parse_maven_tree(text: &str) -> Vec<(String, Vec<DependencyNode>)> {
    let mut out: Vec<(String, Vec<DependencyNode>)> = Vec::new();
    // Path of the nodes above the current line by depth.
    let mut stack: Vec<usize> = Vec::new();
    for raw in text.lines() {
        let line = raw.strip_prefix("[INFO] ").unwrap_or(raw).trim_end();
        if line.trim().is_empty()
            || line.starts_with('[')
            || line.starts_with("---")
            || line.contains("BUILD ")
        {
            continue;
        }
        let prefix_len = line
            .char_indices()
            .find(|&(_, c)| !matches!(c, '|' | ' ' | '+' | '-' | '\\'))
            .map_or(line.len(), |(i, _)| i);
        let (prefix, rest) = line.split_at(prefix_len);
        if prefix.is_empty() {
            // A module's root: `g:a:packaging:version`.
            if rest.split(':').count() >= 4 && !rest.contains(' ') {
                let mut p = rest.split(':');
                let id = format!("{}:{}", p.next().unwrap_or(""), p.next().unwrap_or(""));
                out.push((id, Vec::new()));
                stack.clear();
            }
            continue;
        }
        if !(prefix.ends_with("+- ") || prefix.ends_with("\\- ")) || out.is_empty() {
            continue;
        }
        let depth = prefix.len() / 3 - 1;
        let node = maven_node(rest);
        let Some((_, roots)) = out.last_mut() else {
            continue;
        };
        stack.truncate(depth);
        let mut list = &mut *roots;
        for &i in stack.iter() {
            list = &mut list[i].children;
        }
        if stack.len() < depth {
            // Malformed indentation: attach to the deepest node there is.
            continue;
        }
        list.push(node);
        let index = list.len() - 1;
        stack.push(index);
    }
    out
}

/// Plugins Maven binds to the default lifecycle of a jar, and the goals of well-known
/// plugins (IntelliJ lists them from the effective POM).
const KNOWN_PLUGINS: [(&str, &str, &[&str]); 22] = [
    ("org.apache.maven.plugins", "clean", &["clean", "help"]),
    (
        "org.apache.maven.plugins",
        "resources",
        &["copy-resources", "help", "resources", "testResources"],
    ),
    (
        "org.apache.maven.plugins",
        "compiler",
        &["compile", "help", "testCompile"],
    ),
    ("org.apache.maven.plugins", "surefire", &["help", "test"]),
    (
        "org.apache.maven.plugins",
        "jar",
        &["help", "jar", "test-jar"],
    ),
    (
        "org.apache.maven.plugins",
        "install",
        &["help", "install", "install-file"],
    ),
    (
        "org.apache.maven.plugins",
        "deploy",
        &["deploy", "deploy-file", "help"],
    ),
    (
        "org.apache.maven.plugins",
        "site",
        &[
            "attach-descriptor",
            "deploy",
            "effective-site",
            "help",
            "jar",
            "run",
            "site",
            "stage",
            "stage-deploy",
        ],
    ),
    (
        "org.apache.maven.plugins",
        "failsafe",
        &["help", "integration-test", "verify"],
    ),
    (
        "org.apache.maven.plugins",
        "dependency",
        &[
            "analyze",
            "copy-dependencies",
            "go-offline",
            "list",
            "purge-local-repository",
            "resolve",
            "sources",
            "tree",
        ],
    ),
    (
        "org.apache.maven.plugins",
        "javadoc",
        &["aggregate", "fix", "help", "jar", "javadoc", "test-javadoc"],
    ),
    (
        "org.apache.maven.plugins",
        "source",
        &["aggregate", "help", "jar", "jar-no-fork", "test-jar"],
    ),
    ("org.apache.maven.plugins", "shade", &["help", "shade"]),
    ("org.apache.maven.plugins", "assembly", &["help", "single"]),
    (
        "org.apache.maven.plugins",
        "war",
        &["exploded", "help", "inplace", "war"],
    ),
    (
        "org.apache.maven.plugins",
        "enforcer",
        &["display-info", "enforce", "help"],
    ),
    (
        "org.apache.maven.plugins",
        "checkstyle",
        &["check", "checkstyle", "help"],
    ),
    (
        "org.jacoco",
        "jacoco",
        &["check", "dump", "help", "merge", "prepare-agent", "report"],
    ),
    (
        "org.springframework.boot",
        "spring-boot",
        &[
            "build-image",
            "build-info",
            "help",
            "repackage",
            "run",
            "start",
            "stop",
        ],
    ),
    ("org.codehaus.mojo", "exec", &["exec", "help", "java"]),
    (
        "org.codehaus.mojo",
        "versions",
        &[
            "display-dependency-updates",
            "display-plugin-updates",
            "set",
            "use-latest-releases",
        ],
    ),
    (
        "org.codehaus.mojo",
        "build-helper",
        &["add-source", "add-test-source", "help"],
    ),
];

const DEFAULT_PLUGINS: [&str; 8] = [
    "clean",
    "resources",
    "compiler",
    "surefire",
    "jar",
    "install",
    "deploy",
    "site",
];

/// `maven-compiler-plugin` → `compiler`, `spring-boot-maven-plugin` → `spring-boot`.
pub fn plugin_prefix(artifact: &str) -> String {
    artifact
        .strip_prefix("maven-")
        .and_then(|a| a.strip_suffix("-plugin"))
        .or_else(|| artifact.strip_suffix("-maven-plugin"))
        .or_else(|| artifact.strip_suffix("-plugin"))
        .unwrap_or(artifact)
        .to_string()
}

fn known(prefix: &str) -> Option<(&'static str, &'static [&'static str])> {
    KNOWN_PLUGINS
        .iter()
        .find(|(_, p, _)| *p == prefix)
        .map(|(g, _, goals)| (*g, *goals))
}

fn tag_text<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&format!("</{name}>"))? + start;
    Some(xml[start..end].trim())
}

/// The `<tag>…</tag>` blocks of an XML text (not nested in each other).
fn blocks<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(s) = rest.find(&open) {
        let body = &rest[s + open.len()..];
        let Some(e) = body.find(&close) else { break };
        out.push(&body[..e]);
        rest = &body[e + close.len()..];
    }
    out
}

/// Plugins of a pom: the default lifecycle ones and those in `<build><plugins>`.
pub fn maven_plugins(pom: &str) -> Vec<MavenPlugin> {
    let pom = strip_comments(pom);
    let mut out: Vec<MavenPlugin> = DEFAULT_PLUGINS
        .iter()
        .map(|p| {
            let (group, goals) = known(p).expect("default plugins are known");
            MavenPlugin {
                id: format!("{group}:maven-{p}-plugin"),
                prefix: p.to_string(),
                version: String::new(),
                goals: goals.iter().map(|g| g.to_string()).collect(),
            }
        })
        .collect();
    let build = tag_text(&pom, "build").unwrap_or("");
    // `<pluginManagement>` only configures plugins.
    let build = match (
        build.find("<pluginManagement>"),
        build.find("</pluginManagement>"),
    ) {
        (Some(s), Some(e)) if e > s => format!("{}{}", &build[..s], &build[e..]),
        _ => build.to_string(),
    };
    for plugin in blocks(&build, "plugin") {
        let Some(artifact) = tag_text(plugin, "artifactId") else {
            continue;
        };
        let prefix = plugin_prefix(artifact);
        let group = tag_text(plugin, "groupId").unwrap_or("org.apache.maven.plugins");
        let version = tag_text(plugin, "version").unwrap_or("").to_string();
        let mut goals: Vec<String> = known(&prefix)
            .map(|(_, g)| g.iter().map(|g| g.to_string()).collect())
            .unwrap_or_default();
        for g in blocks(plugin, "goal") {
            if !goals.iter().any(|x| x == g.trim()) {
                goals.push(g.trim().to_string());
            }
        }
        goals.sort();
        let id = format!("{group}:{artifact}");
        match out.iter_mut().find(|p| p.prefix == prefix) {
            Some(p) => {
                p.id = id;
                p.version = version;
                for g in goals {
                    if !p.goals.contains(&g) {
                        p.goals.push(g);
                    }
                }
            }
            None => out.push(MavenPlugin {
                id,
                prefix,
                version,
                goals,
            }),
        }
    }
    out
}

fn strip_comments(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(s) = rest.find("<!--") {
        out.push_str(&rest[..s]);
        match rest[s..].find("-->") {
            Some(e) => rest = &rest[s + e + 3..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// The project's own `<artifactId>` (not its parent's).
fn pom_artifact_id(pom: &str) -> Option<String> {
    let mut pom = strip_comments(pom);
    if let (Some(s), Some(e)) = (pom.find("<parent>"), pom.find("</parent>")) {
        if e > s {
            pom.replace_range(s..e + 9, "");
        }
    }
    for section in ["dependencies", "dependencyManagement", "build", "profiles"] {
        if let Some(s) = pom.find(&format!("<{section}>")) {
            pom.truncate(s);
        }
    }
    tag_text(&pom, "artifactId").map(str::to_string)
}

/// The profiles of a pom.
pub fn maven_profiles(pom: &str) -> Vec<Profile> {
    let pom = strip_comments(pom);
    blocks(&pom, "profile")
        .into_iter()
        .filter_map(|p| {
            let id = tag_text(p, "id")?.to_string();
            let active_by_default = tag_text(p, "activeByDefault") == Some("true");
            Some(Profile {
                id,
                active_by_default,
            })
        })
        .collect()
}

/// The `-P` argument of the selected profiles: `dev` activates, `!prod` deactivates.
pub fn profiles_argument(profiles: &[String]) -> Option<String> {
    let list: Vec<&str> = profiles
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    (!list.is_empty()).then(|| format!("-P{}", list.join(",")))
}

/// The model of a build from its files alone (before a sync, or without one).
pub fn static_model(project: &BuildProject, read: &dyn Fn(&str) -> Option<String>) -> BuildModel {
    let tasks = goals(project);
    let mut projects = Vec::new();
    let mut profiles: Vec<Profile> = Vec::new();
    let root_path = match project.tool {
        BuildTool::Gradle => ":".to_string(),
        BuildTool::Maven => String::new(),
    };
    let mut dirs: Vec<(String, String, String)> =
        vec![(project.name.clone(), root_path, String::new())];
    for m in &project.modules {
        let path = match project.tool {
            BuildTool::Gradle => m.path.clone(),
            BuildTool::Maven => m.dir.clone(),
        };
        dirs.push((
            m.name.rsplit(':').next().unwrap_or(&m.name).to_string(),
            path,
            m.dir.clone(),
        ));
    }
    for (name, path, dir) in dirs {
        let file = |f: &str| {
            if dir.is_empty() {
                f.to_string()
            } else {
                format!("{dir}/{f}")
            }
        };
        let mut model = ProjectModel {
            name,
            path,
            dir: dir.clone(),
            tasks: tasks
                .iter()
                .filter(|g| g.group != "Plugins")
                .cloned()
                .collect(),
            ..Default::default()
        };
        match project.tool {
            BuildTool::Gradle => {
                let text = ["build.gradle.kts", "build.gradle"]
                    .iter()
                    .find_map(|f| read(&file(f)).map(|t| (t, f.ends_with(".kts"))));
                if let Some((t, kts)) = text {
                    model.declared = deps::declarations(
                        &t,
                        if kts {
                            deps::FileKind::GradleKotlin
                        } else {
                            deps::FileKind::GradleGroovy
                        },
                    );
                }
            }
            BuildTool::Maven => {
                if let Some(pom) = read(&file("pom.xml")) {
                    if let Some(id) = pom_artifact_id(&pom) {
                        model.name = id;
                    }
                    model.plugins = maven_plugins(&pom);
                    model.declared = deps::declarations(&pom, deps::FileKind::Pom);
                    for p in maven_profiles(&pom) {
                        if !profiles.iter().any(|x| x.id == p.id) {
                            profiles.push(p);
                        }
                    }
                }
            }
        }
        projects.push(model);
    }
    BuildModel {
        tool: project.tool,
        projects,
        profiles,
        gradle_version: project.gradle_version.clone(),
        synced: false,
        synced_at: None,
    }
}

/// The static model with what a sync found: Gradle's tasks and dependencies, Maven's
/// dependency trees (matched by artifactId).
pub fn merge(mut base: BuildModel, synced: &BuildModel) -> BuildModel {
    if base.tool != synced.tool {
        return base;
    }
    for p in base.projects.iter_mut() {
        let found = synced.projects.iter().find(|s| match base.tool {
            BuildTool::Gradle => s.path == p.path,
            BuildTool::Maven => s.name == p.name,
        });
        if let Some(s) = found {
            if base.tool == BuildTool::Gradle && !s.tasks.is_empty() {
                p.tasks = s.tasks.clone();
            }
            p.dependencies = s.dependencies.clone();
        }
    }
    if base.tool == BuildTool::Gradle {
        // Subprojects the settings file did not show (applied by plugins, `includeBuild`).
        for s in &synced.projects {
            if !base.projects.iter().any(|p| p.path == s.path) {
                base.projects.push(s.clone());
            }
        }
        if synced.gradle_version.is_some() {
            base.gradle_version = synced.gradle_version.clone();
        }
    }
    base.synced = true;
    base.synced_at = synced.synced_at;
    base
}

/// A Maven sync result: the trees of `dependency:tree` as projects (matched to modules by
/// artifactId, `name`).
pub fn maven_sync_model(tree: &str) -> BuildModel {
    BuildModel {
        tool: BuildTool::Maven,
        projects: parse_maven_tree(tree)
            .into_iter()
            .map(|(id, deps)| ProjectModel {
                name: id.split(':').nth(1).unwrap_or(&id).to_string(),
                path: id.clone(),
                dependencies: vec![DependencyScope {
                    name: "Dependencies".into(),
                    description: String::new(),
                    dependencies: deps,
                }],
                ..Default::default()
            })
            .collect(),
        profiles: Vec::new(),
        gradle_version: None,
        synced: true,
        synced_at: None,
    }
}

/// Every task of the model as `:path:task` (and the root's plain names) for Execute Gradle
/// Task… completion, without duplicates.
pub fn task_names(model: &BuildModel) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in &model.projects {
        for t in &p.tasks {
            let name = match model.tool {
                BuildTool::Gradle if !p.path.is_empty() && p.path != ":" => {
                    format!("{}:{}", p.path, t.name)
                }
                _ => t.name.clone(),
            };
            if !out.contains(&name) {
                out.push(name);
            }
        }
        for plugin in &p.plugins {
            for g in &plugin.goals {
                let name = format!("{}:{g}", plugin.prefix);
                if !out.contains(&name) {
                    out.push(name);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::detect;
    use std::collections::HashMap;

    pub(crate) const GRADLE_MODEL: &str = include_str!("../tests/fixtures/gradle-model.json");
    const MAVEN_TREE: &str = include_str!("../tests/fixtures/maven-dependency-tree.txt");
    const MAVEN_LOG: &str = include_str!("../tests/fixtures/maven-dependency-tree-log.txt");

    #[test]
    fn reads_the_gradle_model() {
        let m = parse_gradle_model(GRADLE_MODEL).unwrap();
        assert_eq!(m.gradle_version.as_deref(), Some("8.10.2"));
        let paths: Vec<&str> = m.projects.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, [":", ":app", ":core"]);
        let app = &m.projects[1];
        assert_eq!(app.dir, "app");
        assert!(app
            .tasks
            .iter()
            .any(|t| t.name == "run" && t.group == "application"));
        assert!(!app.tasks.iter().any(|t| t.name == "forgeModel"));
        let cc = app
            .dependencies
            .iter()
            .find(|c| c.name == "compileClasspath")
            .unwrap();
        let guava = &cc.dependencies[1];
        assert_eq!(
            (
                guava.group.as_str(),
                guava.artifact.as_str(),
                guava.version.as_str()
            ),
            ("com.google.guava", "guava", "33.3.1-jre")
        );
        assert_eq!(guava.children[0].artifact, "failureaccess");
        assert_eq!(cc.dependencies[0].id, "project :core");
        let trc = app
            .dependencies
            .iter()
            .find(|c| c.name == "testRuntimeClasspath")
            .unwrap();
        let jupiter = &trc.dependencies[2];
        assert_eq!(jupiter.version, "5.11.3");
        assert_eq!(jupiter.requested, None);
        let bumped = trc
            .dependencies
            .iter()
            .find(|d| d.artifact == "slf4j-api")
            .unwrap();
        assert_eq!(bumped.requested.as_deref(), Some("2.0.9"));
        assert!(trc.dependencies.iter().any(|d| d.omitted));
        assert!(trc.dependencies.iter().any(|d| d.failed));
        assert!(task_names(&m).contains(&":app:run".to_string()));
        assert!(parse_gradle_model("{}").is_err());
    }

    #[test]
    fn reads_maven_dependency_trees() {
        let trees = parse_maven_tree(MAVEN_TREE);
        assert_eq!(trees.len(), 2);
        assert_eq!(trees[0].0, "com.acme:shop-domain");
        let (_, web) = &trees[1];
        assert_eq!(web.len(), 3);
        let starter = &web[0];
        assert_eq!(starter.artifact, "spring-boot-starter-web");
        assert_eq!(starter.scope.as_deref(), Some("compile"));
        assert_eq!(starter.children.len(), 2);
        assert_eq!(starter.children[0].children[0].artifact, "snakeyaml");
        assert_eq!(web[1].id, "com.acme:shop-domain:1.0.0");
        let junit = &web[2];
        assert_eq!(junit.scope.as_deref(), Some("test"));
        // `[INFO]` log output reads the same.
        let log = parse_maven_tree(MAVEN_LOG);
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].1[0].children.len(), 2);
        assert_eq!(log[0].1[1].version, "1.2.3");
        let model = maven_sync_model(MAVEN_TREE);
        assert_eq!(model.projects[1].name, "shop-web");
    }

    #[test]
    fn maven_plugins_and_profiles() {
        let pom = r#"<project><build>
          <pluginManagement><plugins><plugin><artifactId>maven-war-plugin</artifactId></plugin></plugins></pluginManagement>
          <plugins>
            <plugin><groupId>org.jacoco</groupId><artifactId>jacoco-maven-plugin</artifactId><version>0.8.12</version>
              <executions><execution><goals><goal>prepare-agent</goal></goals></execution></executions></plugin>
            <plugin><artifactId>maven-compiler-plugin</artifactId><version>3.13.0</version></plugin>
            <plugin><groupId>com.acme</groupId><artifactId>acme-maven-plugin</artifactId><executions><execution><goals><goal>generate</goal></goals></execution></executions></plugin>
          </plugins></build>
          <profiles><profile><id>dev</id><activation><activeByDefault>true</activeByDefault></activation></profile><profile><id>prod</id></profile><!-- <profile><id>old</id></profile> --></profiles>
        </project>"#;
        let plugins = maven_plugins(pom);
        let prefixes: Vec<&str> = plugins.iter().map(|p| p.prefix.as_str()).collect();
        assert_eq!(
            prefixes,
            [
                "clean",
                "resources",
                "compiler",
                "surefire",
                "jar",
                "install",
                "deploy",
                "site",
                "jacoco",
                "acme"
            ]
        );
        assert_eq!(plugins[2].version, "3.13.0");
        assert!(plugins[8].goals.contains(&"report".to_string()));
        assert_eq!(plugins[9].goals, ["generate"]);
        assert!(!prefixes.contains(&"war"));
        let profiles = maven_profiles(pom);
        assert_eq!(
            profiles,
            [
                Profile {
                    id: "dev".into(),
                    active_by_default: true
                },
                Profile {
                    id: "prod".into(),
                    active_by_default: false
                }
            ]
        );
        assert_eq!(
            profiles_argument(&["dev".into(), "!prod".into()]).as_deref(),
            Some("-Pdev,!prod")
        );
        assert_eq!(profiles_argument(&[]), None);
        assert_eq!(plugin_prefix("spring-boot-maven-plugin"), "spring-boot");
    }

    #[test]
    fn static_and_merged_models() {
        let files: HashMap<String, String> = [
            (
                "settings.gradle.kts",
                "rootProject.name = \"inv\"\ninclude(\"app\", \"core\")\n",
            ),
            ("build.gradle.kts", "plugins { java }\n"),
            (
                "app/build.gradle.kts",
                "dependencies {\n    implementation(\"com.google.guava:guava:32.0.0-jre\")\n}\n",
            ),
            (
                "gradle/wrapper/gradle-wrapper.properties",
                "distributionUrl=https\\://services.gradle.org/distributions/gradle-8.9-bin.zip\n",
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let names: Vec<String> = files.keys().filter(|k| !k.contains('/')).cloned().collect();
        let read = |p: &str| files.get(p).cloned();
        let project = detect("inv", &names, &read, None).unwrap();
        let base = static_model(&project, &read);
        assert_eq!(base.gradle_version.as_deref(), Some("8.9"));
        let paths: Vec<&str> = base.projects.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, [":", ":app", ":core"]);
        assert_eq!(base.projects[1].declared[0].artifact, "guava");
        assert!(!base.synced);
        let synced = parse_gradle_model(GRADLE_MODEL).unwrap();
        let merged = merge(base, &synced);
        assert!(merged.synced);
        assert_eq!(merged.gradle_version.as_deref(), Some("8.10.2"));
        assert!(!merged.projects[1].dependencies.is_empty());
        assert_eq!(merged.projects[1].declared.len(), 1);
    }
}
