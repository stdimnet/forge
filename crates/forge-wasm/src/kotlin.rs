//! KotlinForge (`kotlinforge-lang-kotlin`), mirroring the `kotlin_*` IPC commands and the
//! backend's Kotlin run building for the browser-mode mocks: main functions for gutter
//! icons, coroutine dumps, ktlint / detekt reports and the command lines of Kotlin
//! applications, scripts and Kotest configurations.

use std::collections::BTreeMap;

use forge_runtools::Configuration;
use forge_testing::{CommandContext, TestFrameworkProvider};
use javaforge_build::command::ExecOptions;
use javaforge_build::BuildTool;
use kotlinforge_lang_kotlin::launch::{self, KotlinApp, Runner, ScriptRun};
use kotlinforge_lang_kotlin::lint::{self, Linter};
use kotlinforge_lang_kotlin::{coroutines, KotlinTests};
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

/// `{ path, text }` → `MainFunction[]` (`kotlin_main_functions`).
#[wasm_bindgen]
pub fn kotlin_main_functions(request_json: &str) -> String {
    match parse::<FileRequest>(request_json) {
        Ok(r) => to_json(&kotlinforge_lang_kotlin::main_functions(&r.path, &r.text)),
        Err(e) => error(e),
    }
}

/// The debugger's answer to the coroutine dump expression → `CoroutineDump`
/// (`kotlin_coroutines`).
#[wasm_bindgen]
pub fn kotlin_coroutines(value: &str) -> String {
    to_json(&coroutines::parse_dump(value))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportRequest {
    /// `ktlintJson`, `sarif` or `checkstyle`.
    format: String,
    text: String,
    #[serde(default)]
    detekt: bool,
}

/// `{ format, text, detekt? }` → `LintProblem[]` (canned reports of the mocks).
#[wasm_bindgen]
pub fn kotlin_lint_report(request_json: &str) -> String {
    let r = match parse::<ReportRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let linter = if r.detekt {
        Linter::Detekt
    } else {
        Linter::Ktlint
    };
    let out = match r.format.as_str() {
        "ktlintJson" => lint::parse_ktlint_json(&r.text),
        "sarif" => lint::parse_sarif(&r.text),
        _ => lint::parse_checkstyle(&r.text, linter),
    };
    match out {
        Ok(p) => to_json(&p),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildRequest {
    folder: String,
    files: BTreeMap<String, String>,
    #[serde(default)]
    exec: ExecOptions,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunRequest {
    configuration: Configuration,
    build: BuildRequest,
    #[serde(default)]
    coverage: bool,
    #[serde(default)]
    debug_port: Option<u16>,
    #[serde(default)]
    runner: Option<String>,
}

/// `{ configuration, build, coverage?, debugPort?, runner? }` → `{ program, args, env?, test? }`:
/// a Kotlin application, script or Kotest configuration as the backend builds it.
#[wasm_bindgen]
pub fn kotlin_command_line(request_json: &str) -> String {
    let r = match parse::<RunRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let cfg = &r.configuration;
    let names: Vec<String> = r
        .build
        .files
        .keys()
        .filter(|k| !k.contains('/'))
        .cloned()
        .collect();
    let project = javaforge_build::detect(
        &r.build.folder,
        &names,
        &|p| r.build.files.get(p).cloned(),
        None,
    );
    let result = match cfg.type_id.as_str() {
        "kotest" => match &project {
            None => Err(
                "Tests run through Gradle or Maven: no build.gradle or pom.xml in the project root"
                    .to_string(),
            ),
            Some(project) => {
                let gradle_path = project
                    .module(cfg.option("module"))
                    .map(|m| m.path.clone())
                    .unwrap_or_default();
                let prepared = cfg
                    .clone()
                    .with_option("buildTool", project.tool.id())
                    .with_option(
                        "buildProgram",
                        javaforge_build::executable(project.tool, project.wrapper, &r.build.exec),
                    )
                    .with_option("buildDir", &r.build.exec.build_dir)
                    .with_option("gradlePath", gradle_path);
                let ctx = CommandContext {
                    coverage_file: r.coverage.then(|| "coverage".to_string()),
                    ..Default::default()
                };
                match KotlinTests.command(&prepared, &ctx) {
                    Some(Ok(t)) => {
                        Ok(json!({ "program": t.program, "args": t.args, "env": t.env, "test": t }))
                    }
                    Some(Err(e)) => Err(e),
                    None => Err("no command".into()),
                }
            }
        },
        "kotlin-script" => launch::script_command(&ScriptRun {
            script: cfg.option("script").to_string(),
            program: cfg.option("kotlin").to_string(),
            arguments: cfg.arguments.clone(),
            vm_options: cfg.interpreter_options.clone(),
        })
        .map(|c| json!({ "program": c.program, "args": c.args })),
        "java-application" => {
            let runner = Runner::parse(r.runner.as_deref().unwrap_or(cfg.option("runner")))
                .resolve(project.as_ref().map(|p| p.tool.id()));
            let mut app = KotlinApp {
                main_class: cfg.option("mainClass").to_string(),
                vm_options: cfg.interpreter_options.clone(),
                arguments: cfg.arguments.clone(),
                debug_port: r.debug_port,
                init_script: "~/.cache/forge/kotlin/run.init.gradle".into(),
                kotlinc: "kotlinc".into(),
                java: "java".into(),
                sources: vec![cfg.option("file").to_string()],
                output_jar: "/tmp/forge-kotlin/app.jar".into(),
                module: cfg.option("module").to_string(),
                ..Default::default()
            };
            if let Some(p) = &project {
                let tool = if runner == Runner::Maven {
                    BuildTool::Maven
                } else {
                    BuildTool::Gradle
                };
                app.build_program = javaforge_build::executable(tool, p.wrapper, &r.build.exec);
                app.gradle_path = p
                    .module(cfg.option("module"))
                    .map(|m| m.path.clone())
                    .unwrap_or_default();
            }
            launch::application_command(&app, runner).map(|c| {
                json!({ "program": c.program, "args": c.args, "env": c.env.into_iter().collect::<BTreeMap<_, _>>() })
            })
        }
        other => Err(format!("'{other}' is not a Kotlin configuration")),
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
    fn kotlin_commands_mains_and_reports() {
        let build = json!({ "folder": "orders", "files": { "settings.gradle.kts": "rootProject.name = \"orders\"", "build.gradle.kts": "plugins { kotlin(\"jvm\") }", "gradlew": "" }, "exec": { "useWrapper": true, "buildDir": "/p" } });
        let kotest = json!({
            "configuration": { "name": "CartSpec", "type": "kotest", "options": { "scope": "test", "class": "com.acme.CartSpec", "test": "adds items", "language": "kotlin" } },
            "build": build,
        });
        let out: serde_json::Value =
            serde_json::from_str(&kotlin_command_line(&kotest.to_string())).unwrap();
        assert_eq!(
            out["args"],
            json!(["cleanTest", "test", "--tests", "com.acme.CartSpec"])
        );
        assert_eq!(out["env"]["kotest_filter_tests"], "adds items");
        let app = json!({
            "configuration": { "name": "AppKt", "type": "java-application", "options": { "mainClass": "com.acme.AppKt", "kotlin": "true" } },
            "build": build,
        });
        let out: serde_json::Value =
            serde_json::from_str(&kotlin_command_line(&app.to_string())).unwrap();
        assert_eq!(out["program"], "/p/gradlew");
        assert_eq!(out["args"][4], "forgeRunKotlin");
        let mains: serde_json::Value = serde_json::from_str(&kotlin_main_functions(
            &json!({ "path": "src/main/kotlin/App.kt", "text": "fun main() {}" }).to_string(),
        ))
        .unwrap();
        assert_eq!(mains[0]["class"], "AppKt");
        let dump: serde_json::Value = serde_json::from_str(&kotlin_coroutines("\"[]\"")).unwrap();
        assert_eq!(dump["kind"], "coroutines");
        let report: serde_json::Value = serde_json::from_str(&kotlin_lint_report(&json!({ "format": "ktlintJson", "text": "[{\"file\":\"A.kt\",\"errors\":[{\"line\":1,\"column\":1,\"message\":\"m\",\"rule\":\"r\"}]}]" }).to_string())).unwrap();
        assert_eq!(report[0]["linter"], "ktlint");
    }
}
