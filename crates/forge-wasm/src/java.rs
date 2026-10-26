//! JavaForge (`javaforge-build`, `javaforge-lang-java`), mirroring the `java_*` IPC
//! commands and the backend's Java run building for the browser-mode mocks: main methods
//! for gutter icons, the demo project's Maven / Gradle build, and the command lines of
//! Application, Maven, Gradle, JUnit and TestNG configurations.

use std::collections::BTreeMap;

use forge_runtools::Configuration;
use forge_testing::{CommandContext, TestFrameworkProvider};
use javaforge_build::command::ExecOptions;
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
