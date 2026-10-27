//! `CMakePresets.json` and `CMakeUserPresets.json` (schema versions 1–10): configure, build
//! and test presets with `include` files, `inherits`, `condition`s and macro expansion
//! (`${sourceDir}`, `${presetName}`, `$env{NAME}`, …), as CLion loads them into CMake
//! profiles.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROJECT_FILE: &str = "CMakePresets.json";
pub const USER_FILE: &str = "CMakeUserPresets.json";

/// `"inherits": "base"` or `"inherits": ["a", "b"]`.
fn names(v: &Value) -> Vec<String> {
    match v {
        Value::String(s) => vec![s.clone()],
        Value::Array(items) => items
            .iter()
            .filter_map(|i| i.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// A cache variable's value: a string, a boolean (schema 6) or `{ type, value }`; `null`
/// unsets an inherited one.
fn cache_value(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(if *b { "TRUE" } else { "FALSE" }.into()),
        Value::Object(o) => match o.get("value") {
            Some(Value::Bool(b)) => Some(if *b { "TRUE" } else { "FALSE" }.into()),
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn string_map(v: Option<&Value>) -> BTreeMap<String, Option<String>> {
    v.and_then(Value::as_object)
        .map(|o| o.iter().map(|(k, v)| (k.clone(), cache_value(v))).collect())
        .unwrap_or_default()
}

fn opt_string(o: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    match o.get(key) {
        Some(Value::String(s)) => Some(s.clone()),
        // `architecture` / `toolset`: `{ "value": "x64", "strategy": "external" }`.
        Some(Value::Object(m)) => m.get("value").and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

/// A preset as written, before inheritance.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPreset {
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub hidden: bool,
    pub inherits: Vec<String>,
    /// Configure presets: `generator`, `binaryDir`, `installDir`, `toolchainFile`,
    /// `architecture`, `toolset`; build and test presets: `configurePreset`,
    /// `configuration`.
    pub fields: BTreeMap<String, String>,
    pub cache_variables: BTreeMap<String, Option<String>>,
    pub environment: BTreeMap<String, Option<String>>,
    /// Build presets: `targets`.
    pub targets: Vec<String>,
    pub jobs: Option<u32>,
    pub clean_first: Option<bool>,
    #[serde(skip)]
    pub condition: Option<Value>,
    /// The file that declares it (`CMakePresets.json`, `presets/base.json`).
    pub file: String,
}

impl RawPreset {
    fn parse(v: &Value, file: &str) -> Option<Self> {
        let o = v.as_object()?;
        let name = o.get("name")?.as_str()?.to_string();
        let mut fields = BTreeMap::new();
        for key in [
            "generator",
            "binaryDir",
            "installDir",
            "toolchainFile",
            "architecture",
            "toolset",
            "configurePreset",
            "configuration",
        ] {
            if let Some(s) = opt_string(o, key) {
                fields.insert(key.to_string(), s);
            }
        }
        let targets = o.get("targets").map(names).unwrap_or_default();
        Some(Self {
            name,
            display_name: o
                .get("displayName")
                .and_then(Value::as_str)
                .map(str::to_string),
            description: o
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string),
            hidden: o.get("hidden").and_then(Value::as_bool).unwrap_or(false),
            inherits: o.get("inherits").map(names).unwrap_or_default(),
            fields,
            cache_variables: string_map(o.get("cacheVariables")),
            environment: string_map(o.get("environment")),
            targets,
            jobs: o.get("jobs").and_then(Value::as_u64).map(|j| j as u32),
            clean_first: o.get("cleanFirst").and_then(Value::as_bool),
            condition: o.get("condition").cloned(),
            file: file.to_string(),
        })
    }
}

/// Every preset of a project.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Presets {
    pub configure: Vec<RawPreset>,
    pub build: Vec<RawPreset>,
    pub test: Vec<RawPreset>,
    /// Problems while reading (bad JSON, missing includes, include cycles).
    pub errors: Vec<String>,
    /// The files read, project-relative.
    pub files: Vec<String>,
}

fn parent_dir(path: &str) -> &str {
    path.rfind('/').map(|i| &path[..i]).unwrap_or("")
}

/// `a/b/../c.json` → `a/c.json`.
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

impl Presets {
    fn read_file(
        &mut self,
        path: &str,
        read: &dyn Fn(&str) -> Option<String>,
        stack: &mut Vec<String>,
    ) {
        if stack.iter().any(|p| p == path) {
            self.errors
                .push(format!("{path}: include cycle ({})", stack.join(" → ")));
            return;
        }
        if self.files.contains(&path.to_string()) {
            return;
        }
        let Some(text) = read(path) else {
            if !stack.is_empty() {
                self.errors.push(format!(
                    "{}: included file {path} not found",
                    stack.last().unwrap()
                ));
            }
            return;
        };
        let json: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                self.errors.push(format!("{path}: {e}"));
                return;
            }
        };
        self.files.push(path.to_string());
        stack.push(path.to_string());
        let dir = parent_dir(path).to_string();
        let mut includes: Vec<String> = json
            .get("include")
            .map(names)
            .unwrap_or_default()
            .into_iter()
            .map(|i| {
                if i.starts_with('/') || i.contains(':') {
                    i
                } else if dir.is_empty() {
                    normalize(&i)
                } else {
                    normalize(&format!("{dir}/{i}"))
                }
            })
            .collect();
        // CMakeUserPresets.json implicitly includes CMakePresets.json.
        if path == USER_FILE && !includes.iter().any(|i| i == PROJECT_FILE) {
            includes.insert(0, PROJECT_FILE.to_string());
        }
        for inc in includes {
            self.read_file(&inc, read, stack);
        }
        stack.pop();
        for (key, list) in [
            ("configurePresets", &mut self.configure),
            ("buildPresets", &mut self.build),
            ("testPresets", &mut self.test),
        ] {
            for p in json
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                match RawPreset::parse(p, path) {
                    Some(p) if list.iter().any(|o| o.name == p.name) => {
                        self.errors
                            .push(format!("{path}: duplicate preset '{}'", p.name));
                    }
                    Some(p) => list.push(p),
                    None => self.errors.push(format!("{path}: a preset without a name")),
                }
            }
        }
    }
}

/// Read the project's presets. `read` gets project-relative paths.
pub fn load(read: &dyn Fn(&str) -> Option<String>) -> Presets {
    let mut presets = Presets::default();
    let mut stack = Vec::new();
    presets.read_file(PROJECT_FILE, read, &mut stack);
    presets.read_file(USER_FILE, read, &mut stack);
    presets
}

/// What macros expand to.
#[derive(Debug, Clone, Default)]
pub struct MacroContext {
    /// Absolute project folder.
    pub source_dir: String,
    /// `Linux`, `Windows`, `Darwin`.
    pub host_system_name: String,
    /// The process environment (`$penv{}` and the base of `$env{}`).
    pub env: BTreeMap<String, String>,
}

impl MacroContext {
    pub fn host() -> &'static str {
        match std::env::consts::OS {
            "windows" => "Windows",
            "macos" => "Darwin",
            "linux" => "Linux",
            "freebsd" => "FreeBSD",
            _ => "Unknown",
        }
    }
}

/// A configure preset with its inheritance resolved and its macros expanded.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedConfigure {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub generator: Option<String>,
    /// Absolute, or relative to the project.
    pub binary_dir: Option<String>,
    pub toolchain_file: Option<String>,
    pub cache_variables: BTreeMap<String, String>,
    pub environment: BTreeMap<String, String>,
    pub file: String,
}

impl ResolvedConfigure {
    /// `CMAKE_BUILD_TYPE` of the preset.
    pub fn build_type(&self) -> Option<&str> {
        self.cache_variables
            .get("CMAKE_BUILD_TYPE")
            .map(String::as_str)
    }
}

/// A build preset, resolved.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedBuild {
    pub name: String,
    pub display_name: String,
    pub configure_preset: String,
    pub targets: Vec<String>,
    pub configuration: Option<String>,
    pub jobs: Option<u32>,
    pub clean_first: bool,
}

/// The chain of a preset and its parents, child first (depth-first, like CMake: the first
/// parent that sets a field wins).
fn lineage<'a>(
    list: &'a [RawPreset],
    name: &str,
    seen: &mut Vec<String>,
) -> Result<Vec<&'a RawPreset>, String> {
    if seen.iter().any(|s| s == name) {
        return Err(format!("preset '{name}' inherits from itself"));
    }
    let p = list
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| format!("unknown preset '{name}'"))?;
    seen.push(name.to_string());
    let mut out = vec![p];
    for parent in &p.inherits {
        out.extend(lineage(list, parent, seen)?);
    }
    seen.pop();
    Ok(out)
}

/// Expand `${…}` and `$env{…}` macros.
pub fn expand(
    text: &str,
    preset: &str,
    generator: &str,
    file: &str,
    env: &BTreeMap<String, String>,
    ctx: &MacroContext,
) -> String {
    let source = ctx.source_dir.trim_end_matches(['/', '\\']);
    let parent = source
        .rfind(['/', '\\'])
        .map(|i| &source[..i])
        .unwrap_or("");
    let name = source.rsplit(['/', '\\']).next().unwrap_or("");
    let file_dir = {
        let d = parent_dir(file);
        if d.is_empty() {
            source.to_string()
        } else {
            format!("{source}/{d}")
        }
    };
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('$') {
        out.push_str(&rest[..i]);
        let after = &rest[i..];
        let (prefix, inner_start) = if after.starts_with("${") {
            ("", 2)
        } else if after.starts_with("$env{") {
            ("env", 5)
        } else if after.starts_with("$penv{") {
            ("penv", 6)
        } else {
            out.push('$');
            rest = &after[1..];
            continue;
        };
        let Some(end) = after[inner_start..].find('}') else {
            out.push_str(after);
            return out;
        };
        let key = &after[inner_start..inner_start + end];
        let value = match (prefix, key) {
            ("", "sourceDir") => Some(source.to_string()),
            ("", "sourceParentDir") => Some(parent.to_string()),
            ("", "sourceDirName") => Some(name.to_string()),
            ("", "presetName") => Some(preset.to_string()),
            ("", "generator") => Some(generator.to_string()),
            ("", "hostSystemName") => Some(ctx.host_system_name.clone()),
            ("", "fileDir") => Some(file_dir.clone()),
            ("", "dollar") => Some("$".into()),
            ("", "pathListSep") => Some(
                if ctx.host_system_name == "Windows" {
                    ";"
                } else {
                    ":"
                }
                .into(),
            ),
            ("env", k) => Some(
                env.get(k)
                    .or_else(|| ctx.env.get(k))
                    .cloned()
                    .unwrap_or_default(),
            ),
            ("penv", k) => Some(ctx.env.get(k).cloned().unwrap_or_default()),
            _ => None,
        };
        match value {
            Some(v) => out.push_str(&v),
            None => out.push_str(&after[..inner_start + end + 1]),
        }
        rest = &after[inner_start + end + 1..];
    }
    out.push_str(rest);
    out
}

/// Evaluate a preset `condition` (`const`, `equals`, `notEquals`, `inList`, `notInList`,
/// `matches`, `notMatches`, `anyOf`, `allOf`, `not`); `true` without one.
pub fn condition_holds(cond: Option<&Value>, x: &dyn Fn(&str) -> String) -> bool {
    let Some(c) = cond else {
        return true;
    };
    match c {
        Value::Bool(b) => return *b,
        Value::Null => return true,
        _ => {}
    }
    let s = |k: &str| c.get(k).and_then(Value::as_str).map(x).unwrap_or_default();
    let list = |k: &str| -> Vec<String> {
        c.get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(x).collect())
            .unwrap_or_default()
    };
    let conds = |k: &str| -> Vec<Value> {
        c.get(k)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    match c.get("type").and_then(Value::as_str).unwrap_or("") {
        "const" => c.get("value").and_then(Value::as_bool).unwrap_or(true),
        "equals" => s("lhs") == s("rhs"),
        "notEquals" => s("lhs") != s("rhs"),
        "inList" => list("list").contains(&s("string")),
        "notInList" => !list("list").contains(&s("string")),
        // Regular expressions: a plain substring check covers the patterns presets use
        // (`"regex": "Linux"`).
        "matches" => s("string").contains(&s("regex")),
        "notMatches" => !s("string").contains(&s("regex")),
        "anyOf" => conds("conditions")
            .iter()
            .any(|c| condition_holds(Some(c), x)),
        "allOf" => conds("conditions")
            .iter()
            .all(|c| condition_holds(Some(c), x)),
        "not" => !condition_holds(c.get("condition"), x),
        _ => true,
    }
}

impl Presets {
    /// Resolve a configure preset: inheritance, environment, macros and its condition
    /// (`Ok(None)` when the condition does not hold on this machine).
    pub fn configure_preset(
        &self,
        name: &str,
        ctx: &MacroContext,
    ) -> Result<Option<ResolvedConfigure>, String> {
        let chain = lineage(&self.configure, name, &mut Vec::new())?;
        let first = |key: &str| chain.iter().find_map(|p| p.fields.get(key).cloned());
        let me = chain[0];
        // Parents first, so children override.
        let mut env_raw: BTreeMap<String, Option<String>> = BTreeMap::new();
        let mut cache_raw: BTreeMap<String, Option<String>> = BTreeMap::new();
        for p in chain.iter().rev() {
            env_raw.extend(p.environment.clone());
            cache_raw.extend(p.cache_variables.clone());
        }
        let generator = first("generator");
        let gen = generator.clone().unwrap_or_default();
        // Environment values may refer to each other and to the process environment.
        let mut env: BTreeMap<String, String> = BTreeMap::new();
        for _ in 0..3 {
            let mut next = BTreeMap::new();
            for (k, v) in &env_raw {
                if let Some(v) = v {
                    next.insert(k.clone(), expand(v, name, &gen, &me.file, &env, ctx));
                }
            }
            env = next;
        }
        let x = |s: &str| expand(s, name, &gen, &me.file, &env, ctx);
        let condition = chain.iter().find_map(|p| p.condition.as_ref());
        if !condition_holds(condition, &x) {
            return Ok(None);
        }
        Ok(Some(ResolvedConfigure {
            name: name.to_string(),
            display_name: me.display_name.clone().unwrap_or_else(|| name.to_string()),
            description: me.description.clone().unwrap_or_default(),
            generator,
            binary_dir: first("binaryDir").map(|d| x(&d)),
            toolchain_file: first("toolchainFile").map(|d| x(&d)),
            cache_variables: cache_raw
                .into_iter()
                .filter_map(|(k, v)| v.map(|v| (k, x(&v))))
                .collect(),
            environment: env,
            file: me.file.clone(),
        }))
    }

    /// The configure presets a user can pick: not hidden, condition true.
    pub fn visible_configure_presets(&self, ctx: &MacroContext) -> Vec<ResolvedConfigure> {
        self.configure
            .iter()
            .filter(|p| !p.hidden)
            .filter_map(|p| self.configure_preset(&p.name, ctx).ok().flatten())
            .collect()
    }

    /// A build preset with inheritance resolved.
    pub fn build_preset(&self, name: &str) -> Result<ResolvedBuild, String> {
        let chain = lineage(&self.build, name, &mut Vec::new())?;
        let first = |key: &str| chain.iter().find_map(|p| p.fields.get(key).cloned());
        let me = chain[0];
        Ok(ResolvedBuild {
            name: name.to_string(),
            display_name: me.display_name.clone().unwrap_or_else(|| name.to_string()),
            configure_preset: first("configurePreset").unwrap_or_default(),
            targets: chain
                .iter()
                .find(|p| !p.targets.is_empty())
                .map(|p| p.targets.clone())
                .unwrap_or_default(),
            configuration: first("configuration"),
            jobs: chain.iter().find_map(|p| p.jobs),
            clean_first: chain.iter().find_map(|p| p.clean_first).unwrap_or(false),
        })
    }

    /// The visible build presets of a configure preset.
    pub fn build_presets_for(&self, configure: &str) -> Vec<ResolvedBuild> {
        self.build
            .iter()
            .filter(|p| !p.hidden)
            .filter_map(|p| self.build_preset(&p.name).ok())
            .filter(|b| b.configure_preset == configure)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT: &str = r#"{
      "version": 6,
      "include": ["cmake/presets/base.json"],
      "configurePresets": [
        {
          "name": "debug",
          "displayName": "Debug (Ninja)",
          "inherits": "base",
          "cacheVariables": { "CMAKE_BUILD_TYPE": "Debug", "SHAPES_TESTS": true }
        },
        {
          "name": "release",
          "inherits": ["base"],
          "binaryDir": "${sourceDir}/out/${presetName}",
          "cacheVariables": {
            "CMAKE_BUILD_TYPE": { "type": "STRING", "value": "Release" },
            "SHAPES_WARNINGS": null
          }
        },
        {
          "name": "windows-only",
          "inherits": "base",
          "condition": { "type": "equals", "lhs": "${hostSystemName}", "rhs": "Windows" }
        }
      ],
      "buildPresets": [
        { "name": "debug-all", "configurePreset": "debug", "jobs": 8 },
        { "name": "debug-app", "inherits": "debug-all", "targets": ["shapes_app"] }
      ],
      "testPresets": [ { "name": "debug", "configurePreset": "debug" } ]
    }"#;

    const BASE: &str = r#"{
      "version": 6,
      "configurePresets": [
        {
          "name": "base",
          "hidden": true,
          "generator": "Ninja",
          "binaryDir": "${sourceDir}/build/${presetName}",
          "environment": { "CCACHE_DIR": "$env{HOME}/.ccache", "HOME": "/home/dev" },
          "cacheVariables": { "SHAPES_WARNINGS": "ON", "CMAKE_EXPORT_COMPILE_COMMANDS": "ON" }
        }
      ]
    }"#;

    const USER: &str = r#"{
      "version": 6,
      "configurePresets": [
        { "name": "my-clang", "inherits": "debug", "cacheVariables": { "CMAKE_CXX_COMPILER": "clang++" }, "toolchainFile": "$penv{VCPKG_ROOT}/scripts/buildsystems/vcpkg.cmake" }
      ]
    }"#;

    fn files(name: &str) -> Option<String> {
        match name {
            PROJECT_FILE => Some(PROJECT.into()),
            USER_FILE => Some(USER.into()),
            "cmake/presets/base.json" => Some(BASE.into()),
            _ => None,
        }
    }

    fn ctx() -> MacroContext {
        MacroContext {
            source_dir: "/home/dev/shapes".into(),
            host_system_name: "Linux".into(),
            env: [("VCPKG_ROOT".to_string(), "/opt/vcpkg".to_string())].into(),
        }
    }

    #[test]
    fn loads_includes_and_user_presets() {
        let p = load(&files);
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        assert_eq!(
            p.files,
            [PROJECT_FILE, "cmake/presets/base.json", USER_FILE]
        );
        let names: Vec<&str> = p.configure.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            ["base", "debug", "release", "windows-only", "my-clang"]
        );
        assert_eq!(p.configure[0].file, "cmake/presets/base.json");
    }

    #[test]
    fn resolves_inheritance_and_macros() {
        let p = load(&files);
        let debug = p.configure_preset("debug", &ctx()).unwrap().unwrap();
        assert_eq!(debug.display_name, "Debug (Ninja)");
        assert_eq!(debug.generator.as_deref(), Some("Ninja"));
        assert_eq!(
            debug.binary_dir.as_deref(),
            Some("/home/dev/shapes/build/debug")
        );
        assert_eq!(debug.build_type(), Some("Debug"));
        assert_eq!(debug.cache_variables["SHAPES_TESTS"], "TRUE");
        assert_eq!(debug.cache_variables["SHAPES_WARNINGS"], "ON");
        assert_eq!(debug.environment["CCACHE_DIR"], "/home/dev/.ccache");
        let release = p.configure_preset("release", &ctx()).unwrap().unwrap();
        assert_eq!(
            release.binary_dir.as_deref(),
            Some("/home/dev/shapes/out/release")
        );
        assert_eq!(release.build_type(), Some("Release"));
        assert!(!release.cache_variables.contains_key("SHAPES_WARNINGS"));
        let mine = p.configure_preset("my-clang", &ctx()).unwrap().unwrap();
        assert_eq!(mine.build_type(), Some("Debug"));
        assert_eq!(
            mine.toolchain_file.as_deref(),
            Some("/opt/vcpkg/scripts/buildsystems/vcpkg.cmake")
        );
        assert_eq!(
            mine.binary_dir.as_deref(),
            Some("/home/dev/shapes/build/my-clang")
        );
    }

    #[test]
    fn conditions_and_visibility() {
        let p = load(&files);
        assert_eq!(p.configure_preset("windows-only", &ctx()).unwrap(), None);
        let mut win = ctx();
        win.host_system_name = "Windows".into();
        assert!(p.configure_preset("windows-only", &win).unwrap().is_some());
        let visible: Vec<String> = p
            .visible_configure_presets(&ctx())
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(visible, ["debug", "release", "my-clang"]);
        assert!(p.configure_preset("nope", &ctx()).is_err());
        let x = |s: &str| s.to_string();
        let any = serde_json::json!({"type": "anyOf", "conditions": [
            {"type": "inList", "string": "a", "list": ["b", "a"]},
            {"type": "const", "value": false}
        ]});
        assert!(condition_holds(Some(&any), &x));
        let not = serde_json::json!({"type": "not", "condition": {"type": "notEquals", "lhs": "1", "rhs": "1"}});
        assert!(condition_holds(Some(&not), &x));
    }

    #[test]
    fn build_presets_inherit() {
        let p = load(&files);
        let app = p.build_preset("debug-app").unwrap();
        assert_eq!(app.configure_preset, "debug");
        assert_eq!(app.targets, ["shapes_app"]);
        assert_eq!(app.jobs, Some(8));
        assert_eq!(p.build_presets_for("debug").len(), 2);
    }

    #[test]
    fn bad_files_and_cycles_are_reported() {
        let cyclic = |name: &str| match name {
            PROJECT_FILE => Some(
                r#"{"include": ["a.json"], "configurePresets": [{"name": "x", "inherits": "x"}]}"#
                    .to_string(),
            ),
            "a.json" => Some(r#"{"include": ["CMakePresets.json"]}"#.to_string()),
            USER_FILE => Some("{ not json".to_string()),
            _ => None,
        };
        let p = load(&cyclic);
        assert_eq!(p.errors.len(), 2, "{:?}", p.errors);
        assert!(p.errors[0].contains("include cycle"));
        assert!(p.errors[1].starts_with("CMakeUserPresets.json:"));
        assert!(p
            .configure_preset("x", &ctx())
            .unwrap_err()
            .contains("itself"));
    }
}
