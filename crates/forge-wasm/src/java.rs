//! JavaForge (`javaforge-build`, `javaforge-lang-java`), mirroring the `java_*` IPC
//! commands and the backend's Java run building for the browser-mode mocks: main methods
//! for gutter icons, the demo project's Maven / Gradle build, and the command lines of
//! Application, Maven, Gradle, JUnit and TestNG configurations.

use std::collections::BTreeMap;

use forge_runtools::Configuration;
use forge_testing::{CommandContext, TestFrameworkProvider};
use javaforge_build::command::{gradle_application_command, ExecOptions, GradleAppRun};
use javaforge_build::dsl::{self, CentralData, Script};
use javaforge_build::{deps, model, wrapper};
use javaforge_build::{BuildProject, BuildTool};
use javaforge_lang_java::launch::{self, AppLaunch};
use javaforge_lang_java::JavaTests;
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

fn parse<T: for<'de> Deserialize<'de>>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct FileRequest {
    path: String,
    text: String,
}

/// `{ path, text }` → `MainMethod[]` (`java_main_methods`).
#[wasm_bindgen]
pub fn java_main_methods(request_json: &str) -> String {
    match parse::<FileRequest>(request_json) {
        Ok(r) => to_json(&javaforge_lang_java::main_methods(&r.path, &r.text)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildRequest {
    folder: String,
    /// Project files by path (the root's and the modules' build files are read).
    files: BTreeMap<String, String>,
    #[serde(default)]
    prefer: Option<String>,
    #[serde(default)]
    exec: ExecOptions,
}

fn build(r: &BuildRequest) -> Option<BuildProject> {
    let names: Vec<String> = r
        .files
        .keys()
        .filter(|k| !k.contains('/'))
        .cloned()
        .collect();
    javaforge_build::detect(
        &r.folder,
        &names,
        &|p| r.files.get(p).cloned(),
        r.prefer.as_deref().and_then(BuildTool::parse),
    )
}

/// `{ folder, files, prefer?, exec }` → `{ project, goals, program } | null` (`java_build_info`).
#[wasm_bindgen]
pub fn java_build_info(request_json: &str) -> String {
    match parse::<BuildRequest>(request_json) {
        Ok(r) => match build(&r) {
            Some(project) => to_json(&json!({
                "goals": javaforge_build::goals(&project),
                "program": javaforge_build::executable(project.tool, project.wrapper, &r.exec),
                "project": project,
            })),
            None => "null".into(),
        },
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunRequest {
    configuration: Configuration,
    build: BuildRequest,
    #[serde(default)]
    coverage: bool,
    /// `java` of the configuration's JDK (Application).
    #[serde(default)]
    java: String,
    #[serde(default)]
    debug_port: Option<u16>,
    /// "Build and run using: Gradle": Application runs go through `forgeRun`.
    #[serde(default)]
    gradle_run: bool,
    /// Where the backend writes `forge-run.gradle`.
    #[serde(default)]
    init_script: String,
}

/// `{ configuration, build, coverage?, java?, debugPort? }` → `{ program, args, test? }`: the
/// command line of a Java configuration as the backend builds it (`run.rs`, `java.rs`);
/// JUnit / TestNG add their `TestCommand` (`test`).
#[wasm_bindgen]
pub fn java_command_line(request_json: &str) -> String {
    let r = match parse::<RunRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let cfg = &r.configuration;
    let project = build(&r.build);
    let result = match cfg.type_id.as_str() {
        "junit" | "testng" => {
            let Some(project) = project else {
                return error("Tests run through Gradle or Maven: no build.gradle or pom.xml in the project root");
            };
            let mut prepared = cfg.clone();
            let gradle_path = project
                .module(cfg.option("module"))
                .map(|m| m.path.clone())
                .unwrap_or_default();
            let mut extra = cfg.option("testRunnerOptions").to_string();
            if let Some(port) = r.debug_port {
                extra = format!(
                    "{extra} {}",
                    launch::build_tool_debug_args(project.tool.id(), port).join(" ")
                );
            }
            prepared = prepared
                .with_option("buildTool", project.tool.id())
                .with_option(
                    "buildProgram",
                    javaforge_build::executable(project.tool, project.wrapper, &r.build.exec),
                )
                .with_option("buildDir", &r.build.exec.build_dir)
                .with_option("gradlePath", gradle_path)
                .with_option("testRunnerOptions", extra.trim());
            let provider = if cfg.type_id == "junit" {
                JavaTests::junit()
            } else {
                JavaTests::testng()
            };
            let ctx = CommandContext {
                coverage_file: r.coverage.then(|| "coverage".to_string()),
                ..Default::default()
            };
            match provider.command(&prepared, &ctx) {
                Some(Ok(t)) => Ok(json!({ "program": t.program, "args": t.args, "test": t })),
                Some(Err(e)) => Err(e),
                None => Err("no command".into()),
            }
        }
        "java-application"
            if r.gradle_run
                && cfg.option("classpath").trim().is_empty()
                && project
                    .as_ref()
                    .is_some_and(|p| p.tool == BuildTool::Gradle) =>
        {
            let project = project.expect("a Gradle build");
            let mut jvm_args = forge_runtools::args::split_args(&cfg.interpreter_options);
            if let Some(port) = r.debug_port {
                jvm_args.push(launch::jdwp_listen(port, true));
            }
            let module = project.module(cfg.option("module")).cloned();
            let run = GradleAppRun {
                main_class: cfg.option("mainClass").to_string(),
                args: forge_runtools::args::split_args(&cfg.arguments),
                jvm_args,
                init_script: r.init_script.clone(),
                test_classpath: false,
                working_dir: String::new(),
            };
            let c = gradle_application_command(&project, &r.build.exec, module.as_ref(), &run);
            Ok(json!({ "program": c.program, "args": c.args }))
        }
        "java-application" => {
            let classpath = match cfg.option("classpath").trim() {
                "" => launch::build_output_classpath(
                    project.as_ref().map(|p| p.tool.id()).unwrap_or("gradle"),
                    cfg.option("module"),
                ),
                own => own
                    .split(['\n', ':'])
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
            };
            let l = AppLaunch {
                java: r.java.clone(),
                vm_options: cfg.interpreter_options.clone(),
                main_class: cfg.option("mainClass").to_string(),
                module: Some(cfg.option("javaModule").to_string()).filter(|m| !m.trim().is_empty()),
                classpath,
                modulepath: Vec::new(),
                arguments: cfg.arguments.clone(),
                windows: false,
                debug_port: r.debug_port,
            };
            launch::application_command(&l)
                .map(|(program, args)| json!({ "program": program, "args": args }))
        }
        "maven" | "gradle" => {
            let tool = if cfg.type_id == "maven" {
                BuildTool::Maven
            } else {
                BuildTool::Gradle
            };
            let project = project
                .filter(|p| p.tool == tool)
                .unwrap_or_else(|| BuildProject::minimal(tool, Vec::new()));
            let key = if tool == BuildTool::Maven {
                "goals"
            } else {
                "tasks"
            };
            let goals = forge_runtools::args::split_args(cfg.option(key));
            if goals.is_empty() {
                Err(format!("No {key} to run"))
            } else {
                let mut extra = forge_runtools::args::split_args(&cfg.arguments);
                if let Some(port) = r.debug_port {
                    extra.extend(launch::build_tool_debug_args(tool.id(), port));
                }
                let module = project.module(cfg.option("module")).cloned();
                let c = javaforge_build::goals_command(
                    &project,
                    &r.build.exec,
                    module.as_ref(),
                    &goals,
                    &extra,
                );
                Ok(json!({ "program": c.program, "args": c.args }))
            }
        }
        other => Err(format!("'{other}' is not a Java configuration")),
    };
    match result {
        Ok(v) => to_json(&v),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelRequest {
    #[serde(flatten)]
    build: BuildRequest,
    /// What a Gradle sync's init script wrote (`forge-model.gradle`).
    #[serde(default)]
    gradle_model: Option<String>,
    /// What a Maven sync's `dependency:tree` wrote.
    #[serde(default)]
    maven_tree: Option<String>,
}

/// `{ folder, files, prefer?, gradleModel?, mavenTree? }` → `BuildModel | null`
/// (`java_build_model`, `java_sync_project`).
#[wasm_bindgen]
pub fn java_build_model(request_json: &str) -> String {
    let r = match parse::<ModelRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let Some(project) = build(&r.build) else {
        return "null".into();
    };
    let base = model::static_model(&project, &|p| r.build.files.get(p).cloned());
    let synced = match (project.tool, &r.gradle_model, &r.maven_tree) {
        (BuildTool::Gradle, Some(json), _) => match model::parse_gradle_model(json) {
            Ok(m) => Some(m),
            Err(e) => return error(e),
        },
        (BuildTool::Maven, _, Some(tree)) => Some(model::maven_sync_model(tree)),
        _ => None,
    };
    to_json(&match synced {
        Some(s) => model::merge(base, &s),
        None => base,
    })
}

#[derive(Deserialize)]
struct CompleteRequest {
    path: String,
    text: String,
    offset: usize,
    #[serde(default)]
    data: CentralData,
}

/// `{ path, text, offset }` → `Request | null`: what Maven Central must answer.
#[wasm_bindgen]
pub fn java_build_request(request_json: &str) -> String {
    match parse::<CompleteRequest>(request_json) {
        Ok(r) => match Script::of_path(&r.path) {
            Some(s) => to_json(&dsl::request(s, &r.text, r.offset)),
            None => "null".into(),
        },
        Err(e) => error(e),
    }
}

/// `{ path, text, offset, data }` → `CompletionOut | null` (`java_build_complete`).
#[wasm_bindgen]
pub fn java_build_complete(request_json: &str) -> String {
    match parse::<CompleteRequest>(request_json) {
        Ok(r) => match Script::of_path(&r.path) {
            Some(s) => to_json(&dsl::complete(s, &r.text, r.offset, &r.data)),
            None => "null".into(),
        },
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct OutdatedRequest {
    path: String,
    text: String,
    /// `group:artifact` → versions.
    #[serde(default)]
    versions: std::collections::HashMap<String, Vec<String>>,
}

/// `{ path, text, versions }` → `OutdatedMarker[]` (`java_build_outdated`); `{ path, text }`
/// alone → the `group:artifact`s to look up (`coordinates`).
#[wasm_bindgen]
pub fn java_build_outdated(request_json: &str) -> String {
    match parse::<OutdatedRequest>(request_json) {
        Ok(r) => match deps::FileKind::of_path(&r.path) {
            Some(kind) => to_json(&json!({
                "coordinates": deps::versioned_coordinates(&r.text, kind),
                "markers": deps::outdated(&r.text, kind, &r.versions),
            })),
            None => to_json(&json!({ "coordinates": [], "markers": [] })),
        },
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WrapperRequest {
    text: String,
    /// Update Wrapper: the new version and its checksum.
    #[serde(default)]
    update: Option<(String, String)>,
}

/// `{ text, update? }` → `{ wrapper, text }`: `gradle-wrapper.properties` read (and updated).
#[wasm_bindgen]
pub fn java_wrapper(request_json: &str) -> String {
    match parse::<WrapperRequest>(request_json) {
        Ok(r) => {
            let text = match &r.update {
                Some((version, sum)) => {
                    let kind = wrapper::parse_properties(&r.text)
                        .map(|w| w.distribution_type)
                        .unwrap_or_else(|| "bin".into());
                    wrapper::update_properties(
                        &r.text,
                        &wrapper::distribution_url(version, &kind),
                        Some(sum),
                    )
                }
                None => r.text.clone(),
            };
            to_json(&json!({ "wrapper": wrapper::parse_properties(&text), "text": text }))
        }
        Err(e) => error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_and_command_lines() {
        let files = json!({
            "settings.gradle.kts": "rootProject.name = \"inventory\"",
            "build.gradle.kts": "plugins { application\n jacoco }\napplication { mainClass = \"com.acme.App\" }",
            "gradlew": "",
        });
        let build = json!({ "folder": "inventory", "files": files, "exec": { "useWrapper": true, "buildDir": "/p" } });
        let model: serde_json::Value =
            serde_json::from_str(&java_build_model(&build.to_string())).unwrap();
        assert_eq!(model["tool"], "gradle");
        assert_eq!(model["projects"][0]["path"], ":");
        let gradle_run = json!({
            "configuration": { "name": "App", "type": "java-application", "options": { "mainClass": "com.acme.App" } },
            "build": build, "gradleRun": true, "initScript": "/c/forge-run.gradle",
        });
        let out: serde_json::Value =
            serde_json::from_str(&java_command_line(&gradle_run.to_string())).unwrap();
        assert_eq!(out["program"], "/p/gradlew");
        assert_eq!(out["args"][2], "forgeRun");
        let complete =
            json!({ "path": "build.gradle.kts", "text": "dependencies {\n  imp\n}", "offset": 20 });
        let c: serde_json::Value =
            serde_json::from_str(&java_build_complete(&complete.to_string())).unwrap();
        assert!(c["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["label"] == "implementation"));
        let w: serde_json::Value = serde_json::from_str(&java_wrapper(&json!({ "text": "distributionUrl=https\\://services.gradle.org/distributions/gradle-8.9-bin.zip\n", "update": ["8.11", "ab"] }).to_string())).unwrap();
        assert_eq!(w["wrapper"]["version"], "8.11");
        let info: serde_json::Value =
            serde_json::from_str(&java_build_info(&build.to_string())).unwrap();
        assert_eq!(info["program"], "/p/gradlew");
        assert_eq!(info["project"]["mainClass"], "com.acme.App");
        let test = json!({
            "configuration": { "name": "T", "type": "junit", "options": { "scope": "class", "class": "com.acme.AppTest" } },
            "build": build,
            "coverage": true,
        });
        let out: serde_json::Value =
            serde_json::from_str(&java_command_line(&test.to_string())).unwrap();
        assert_eq!(
            out["args"],
            json!([
                "cleanTest",
                "test",
                "jacocoTestReport",
                "--tests",
                "com.acme.AppTest"
            ])
        );
        assert_eq!(out["test"]["flavor"], "java");
        let app = json!({
            "configuration": { "name": "App", "type": "java-application", "options": { "mainClass": "com.acme.App" } },
            "build": build,
            "java": "/jdk/bin/java",
        });
        let out: serde_json::Value =
            serde_json::from_str(&java_command_line(&app.to_string())).unwrap();
        assert_eq!(out["program"], "/jdk/bin/java");
        assert_eq!(out["args"][0], "-cp");
        let main: serde_json::Value = serde_json::from_str(&java_main_methods(
            &json!({ "path": "App.java", "text": "class App { public static void main(String[] a) {} }" }).to_string(),
        ))
        .unwrap();
        assert_eq!(main[0]["class"], "App");
    }
}
