//! The cmake-file-api (CMake 3.14+): Forge writes a stateful client query
//! (`<build>/.cmake/api/v1/query/client-forge/query.json`) before configuring; CMake answers
//! in `<build>/.cmake/api/v1/reply/` with an index and one JSON file per object. This module
//! reads the `codemodel` v2 (configurations, projects, targets with their sources,
//! artifacts and where they are defined), `toolchains` v1 and `cmakeFiles` v1 replies into
//! the model of the CMake tool window.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The query folder of Forge's client, relative to the build folder.
pub const QUERY_DIR: &str = ".cmake/api/v1/query/client-forge";
/// The reply folder, relative to the build folder.
pub const REPLY_DIR: &str = ".cmake/api/v1/reply";

/// The query Forge writes to `<build>/<QUERY_DIR>/query.json`.
pub fn query_json() -> String {
    serde_json::json!({
        "requests": [
            { "kind": "codemodel", "version": 2 },
            { "kind": "toolchains", "version": 1 },
            { "kind": "cmakeFiles", "version": 1 }
        ],
        "client": { "name": "Forge" }
    })
    .to_string()
}

/// The kind of a target (CMake's `type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetKind {
    Executable,
    StaticLibrary,
    SharedLibrary,
    ModuleLibrary,
    ObjectLibrary,
    InterfaceLibrary,
    Utility,
}

impl TargetKind {
    fn parse(s: &str) -> Self {
        match s {
            "EXECUTABLE" => TargetKind::Executable,
            "STATIC_LIBRARY" => TargetKind::StaticLibrary,
            "SHARED_LIBRARY" => TargetKind::SharedLibrary,
            "MODULE_LIBRARY" => TargetKind::ModuleLibrary,
            "OBJECT_LIBRARY" => TargetKind::ObjectLibrary,
            "INTERFACE_LIBRARY" => TargetKind::InterfaceLibrary,
            _ => TargetKind::Utility,
        }
    }

    pub fn is_library(self) -> bool {
        matches!(
            self,
            TargetKind::StaticLibrary
                | TargetKind::SharedLibrary
                | TargetKind::ModuleLibrary
                | TargetKind::ObjectLibrary
                | TargetKind::InterfaceLibrary
        )
    }
}

/// A place in a CMake file (`CMakeLists.txt:11`), project-relative when inside the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub file: String,
    /// 1-based.
    pub line: u32,
    /// The command (`add_executable`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

/// A source of a target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    /// Project-relative (or absolute for files outside the project).
    pub path: String,
    /// `CXX`, `C`; empty for headers and other files that are not compiled.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub language: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub generated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub name: String,
    pub id: String,
    pub kind: TargetKind,
    /// Files the target produces, absolute (`<build>/shapes_app`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<String>,
    /// The folder of the `CMakeLists.txt` that defines it, project-relative.
    pub source_dir: String,
    /// Its build folder, absolute.
    pub build_dir: String,
    pub sources: Vec<Source>,
    /// Names of the targets it depends on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defined_at: Option<Location>,
    /// The project (`project()`) it belongs to.
    pub project: String,
    /// CMake-generated helpers (`ALL_BUILD`, `ZERO_CHECK`, `RUN_TESTS`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub generator_provided: bool,
}

impl Target {
    /// The executable to run (the first artifact without a debug-symbols extension).
    pub fn executable(&self) -> Option<&str> {
        if self.kind != TargetKind::Executable {
            return None;
        }
        self.artifacts
            .iter()
            .map(String::as_str)
            .find(|a| !a.ends_with(".pdb") && !a.ends_with(".ilk"))
    }

    /// Whether `path` (project-relative) is one of its sources.
    pub fn has_source(&self, path: &str) -> bool {
        self.sources.iter().any(|s| s.path == path)
    }
}

/// One configuration (`Debug`) of the code model.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Configuration {
    pub name: String,
    /// `project()` names, top-level first.
    pub projects: Vec<String>,
    pub targets: Vec<Target>,
}

/// A compiler of a language (`toolchains` reply).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageToolchain {
    pub language: String,
    /// `GNU`, `Clang`, `MSVC`.
    pub compiler_id: String,
    pub path: String,
    pub version: String,
}

/// What a configured build folder contains.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CMakeModel {
    pub cmake_version: String,
    pub generator: String,
    pub multi_config: bool,
    /// Absolute.
    pub source_dir: String,
    /// Absolute.
    pub build_dir: String,
    pub configurations: Vec<Configuration>,
    pub toolchains: Vec<LanguageToolchain>,
    /// CMake files of the project (`CMakeLists.txt`, `cmake/*.cmake`), project-relative:
    /// a change to one of them makes the model outdated.
    pub cmake_files: Vec<String>,
}

impl CMakeModel {
    /// The configuration of a build type (multi-config generators have several).
    pub fn configuration(&self, build_type: &str) -> Option<&Configuration> {
        self.configurations
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(build_type))
            .or_else(|| self.configurations.first())
    }

    /// Every executable target of a configuration.
    pub fn executables(&self, build_type: &str) -> Vec<&Target> {
        self.configuration(build_type)
            .map(|c| {
                c.targets
                    .iter()
                    .filter(|t| t.kind == TargetKind::Executable)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The executable target that compiles `path` (project-relative).
    pub fn target_of_source(&self, build_type: &str, path: &str) -> Option<&Target> {
        self.executables(build_type)
            .into_iter()
            .find(|t| t.has_source(path))
    }

    pub fn target(&self, build_type: &str, name: &str) -> Option<&Target> {
        self.configuration(build_type)?
            .targets
            .iter()
            .find(|t| t.name == name)
    }
}

fn join(base: &str, rel: &str) -> String {
    if rel.is_empty() || rel == "." {
        return base.to_string();
    }
    if rel.starts_with('/') || rel.get(1..2) == Some(":") {
        return rel.to_string();
    }
    format!("{}/{}", base.trim_end_matches('/'), rel)
}

/// `/home/dev/shapes/src/a.cpp` → `src/a.cpp` when below `root`.
fn relative(root: &str, path: &str) -> String {
    let root = root.trim_end_matches('/');
    path.strip_prefix(root)
        .and_then(|r| r.strip_prefix('/'))
        .map(str::to_string)
        .unwrap_or_else(|| path.to_string())
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn location_of(target: &Value, node: Option<u64>, source_dir: &str) -> Option<Location> {
    let graph = target.get("backtraceGraph")?;
    let node = graph.get("nodes")?.get(node? as usize)?;
    let file = graph
        .get("files")?
        .get(node.get("file")?.as_u64()? as usize)?
        .as_str()?;
    let command = node
        .get("command")
        .and_then(Value::as_u64)
        .and_then(|c| graph.get("commands")?.get(c as usize)?.as_str())
        .map(str::to_string);
    Some(Location {
        file: relative(source_dir, file),
        line: node.get("line").and_then(Value::as_u64).unwrap_or(1) as u32,
        command,
    })
}

fn parse_target(
    v: &Value,
    project: &str,
    source_root: &str,
    build_root: &str,
    names: &BTreeMap<String, String>,
) -> Target {
    let paths = v.get("paths");
    let src_rel = paths.map(|p| s(p, "source")).unwrap_or(".");
    let build_rel = paths.map(|p| s(p, "build")).unwrap_or(".");
    let groups: Vec<String> = v
        .get("compileGroups")
        .and_then(Value::as_array)
        .map(|g| g.iter().map(|g| s(g, "language").to_string()).collect())
        .unwrap_or_default();
    let sources = v
        .get("sources")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|src| {
                    let path = s(src, "path");
                    let abs = join(source_root, path);
                    Source {
                        path: relative(source_root, &abs),
                        language: src
                            .get("compileGroupIndex")
                            .and_then(Value::as_u64)
                            .and_then(|i| groups.get(i as usize).cloned())
                            .unwrap_or_default(),
                        generated: src
                            .get("isGenerated")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    Target {
        name: s(v, "name").to_string(),
        id: s(v, "id").to_string(),
        kind: TargetKind::parse(s(v, "type")),
        artifacts: v
            .get("artifacts")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|a| join(build_root, s(a, "path"))).collect())
            .unwrap_or_default(),
        source_dir: if src_rel == "." {
            String::new()
        } else {
            src_rel.to_string()
        },
        build_dir: join(build_root, build_rel),
        sources,
        dependencies: v
            .get("dependencies")
            .and_then(Value::as_array)
            .map(|d| {
                d.iter()
                    .filter_map(|d| names.get(s(d, "id")).cloned())
                    .collect()
            })
            .unwrap_or_default(),
        defined_at: location_of(v, v.get("backtrace").and_then(Value::as_u64), source_root),
        project: project.to_string(),
        generator_provided: v
            .get("isGeneratorProvided")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

/// The newest index file among the reply folder's file names.
pub fn index_file<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    names
        .into_iter()
        .filter(|n| n.starts_with("index-") && n.ends_with(".json"))
        .max()
}

/// Read a reply. `files` are the reply folder's file names, `read` returns one of them.
pub fn parse_reply(
    files: &[String],
    read: &dyn Fn(&str) -> Option<String>,
) -> Result<CMakeModel, String> {
    let index_name = index_file(files.iter().map(String::as_str))
        .ok_or("CMake wrote no file API reply (CMake 3.14 or newer is needed)")?;
    let index: Value =
        serde_json::from_str(&read(index_name).ok_or("cannot read the reply index")?)
            .map_err(|e| format!("bad reply index: {e}"))?;
    let load = |file: &str| -> Result<Value, String> {
        let text = read(file).ok_or_else(|| format!("cannot read {file}"))?;
        serde_json::from_str(&text).map_err(|e| format!("bad {file}: {e}"))
    };
    // Our client's answers, else the shared objects of the index.
    let mut objects: Vec<Value> = index
        .pointer("/reply/client-forge/query.json/responses")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if objects.iter().all(|o| o.get("jsonFile").is_none()) {
        objects = index
            .get("objects")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
    }
    let file_of = |kind: &str| {
        objects
            .iter()
            .find(|o| s(o, "kind") == kind)
            .map(|o| s(o, "jsonFile").to_string())
            .filter(|f| !f.is_empty())
    };
    let cmake = index.get("cmake").cloned().unwrap_or(Value::Null);
    let mut model = CMakeModel {
        cmake_version: cmake
            .pointer("/version/string")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        generator: cmake
            .pointer("/generator/name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        multi_config: cmake
            .pointer("/generator/multiConfig")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        ..CMakeModel::default()
    };
    let codemodel_file = file_of("codemodel").ok_or("the reply has no codemodel")?;
    let codemodel = load(&codemodel_file)?;
    model.source_dir = codemodel
        .pointer("/paths/source")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    model.build_dir = codemodel
        .pointer("/paths/build")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    for cfg in codemodel
        .get("configurations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let projects: Vec<String> = cfg
            .get("projects")
            .and_then(Value::as_array)
            .map(|p| p.iter().map(|p| s(p, "name").to_string()).collect())
            .unwrap_or_default();
        let refs = cfg
            .get("targets")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let names: BTreeMap<String, String> = refs
            .iter()
            .map(|t| (s(t, "id").to_string(), s(t, "name").to_string()))
            .collect();
        let mut targets = Vec::new();
        for r in &refs {
            let project = r
                .get("projectIndex")
                .and_then(Value::as_u64)
                .and_then(|i| projects.get(i as usize).cloned())
                .unwrap_or_default();
            let t = load(s(r, "jsonFile"))?;
            targets.push(parse_target(
                &t,
                &project,
                &model.source_dir,
                &model.build_dir,
                &names,
            ));
        }
        model.configurations.push(Configuration {
            name: s(cfg, "name").to_string(),
            projects,
            targets,
        });
    }
    if let Some(f) = file_of("toolchains") {
        let t = load(&f)?;
        for tc in t
            .get("toolchains")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let compiler = tc.get("compiler").cloned().unwrap_or(Value::Null);
            model.toolchains.push(LanguageToolchain {
                language: s(tc, "language").to_string(),
                compiler_id: s(&compiler, "id").to_string(),
                path: s(&compiler, "path").to_string(),
                version: s(&compiler, "version").to_string(),
            });
        }
    }
    if let Some(f) = file_of("cmakeFiles") {
        let c = load(&f)?;
        model.cmake_files = c
            .get("inputs")
            .and_then(Value::as_array)
            .map(|inputs| {
                inputs
                    .iter()
                    .filter(|i| {
                        !i.get("isExternal")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                            && !i
                                .get("isGenerated")
                                .and_then(Value::as_bool)
                                .unwrap_or(false)
                            && !i.get("isCMake").and_then(Value::as_bool).unwrap_or(false)
                    })
                    .map(|i| relative(&model.source_dir, s(i, "path")))
                    .collect()
            })
            .unwrap_or_default();
    }
    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn reply_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reply")
    }

    fn recorded() -> CMakeModel {
        let dir = reply_dir();
        let files: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        parse_reply(&files, &|name| std::fs::read_to_string(dir.join(name)).ok()).unwrap()
    }

    #[test]
    fn reads_the_recorded_codemodel() {
        let m = recorded();
        assert_eq!(m.cmake_version, "3.28.3");
        assert_eq!(m.generator, "Ninja");
        assert!(!m.multi_config);
        assert_eq!(m.source_dir, "/home/dev/shapes");
        assert_eq!(m.build_dir, "/home/dev/shapes/build/debug");
        let cfg = m.configuration("debug").unwrap();
        assert_eq!(cfg.name, "Debug");
        assert_eq!(cfg.projects, ["shapes"]);
        let names: Vec<&str> = cfg.targets.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["shapes", "shapes_app", "shapes_tests"]);
        let lib = &cfg.targets[0];
        assert_eq!(lib.kind, TargetKind::StaticLibrary);
        assert!(lib.kind.is_library());
        assert_eq!(lib.artifacts, ["/home/dev/shapes/build/debug/libshapes.a"]);
        assert_eq!(lib.executable(), None);
        let app = &cfg.targets[1];
        assert_eq!(app.kind, TargetKind::Executable);
        assert_eq!(
            app.executable(),
            Some("/home/dev/shapes/build/debug/shapes_app")
        );
        assert_eq!(app.dependencies, ["shapes"]);
        assert_eq!(
            app.defined_at,
            Some(Location {
                file: "CMakeLists.txt".into(),
                line: 11,
                command: Some("add_executable".into())
            })
        );
        assert_eq!(
            app.sources,
            [Source {
                path: "src/main.cpp".into(),
                language: "CXX".into(),
                generated: false
            }]
        );
        assert_eq!(m.cmake_files, ["CMakeLists.txt"]);
        assert_eq!(m.toolchains[0].compiler_id, "GNU");
        assert_eq!(m.toolchains[0].version, "13.3.0");
        let tests = m
            .target_of_source("Debug", "tests/shapes_test.cpp")
            .unwrap();
        assert_eq!(tests.name, "shapes_tests");
        assert_eq!(m.executables("Debug").len(), 2);
        assert_eq!(m.target("Debug", "shapes").unwrap().source_dir, "");
    }

    #[test]
    fn missing_or_broken_replies() {
        assert!(parse_reply(&[], &|_| None)
            .unwrap_err()
            .contains("no file API reply"));
        let files = vec!["index-1.json".to_string()];
        assert!(parse_reply(&files, &|_| Some("{}".into()))
            .unwrap_err()
            .contains("no codemodel"));
        assert_eq!(
            index_file(["index-2024-01.json", "index-2025-02.json", "codemodel.json"]),
            Some("index-2025-02.json")
        );
        let q: Value = serde_json::from_str(&query_json()).unwrap();
        assert_eq!(q["requests"][0]["kind"], "codemodel");
    }
}
