//! JavaForge's JVM extras for the browser-mode mocks, mirroring the `groovy_*`, `scala_*`
//! and `android_*` IPC commands and the backend's run building: Groovy scripts and mains,
//! Groovy Script command lines, Scala main objects, sbt builds (`build.sbt` and recorded
//! sbt output) and command lines, ScalaTest / MUnit commands, adb output, Android modules
//! and manifests, Android App command lines and Logcat views.

use forge_runtools::Configuration;
use forge_testing::{CommandContext, TestFrameworkProvider};
use javaforge_android::logcat::{Logcat, Query};
use javaforge_android::run::AppRun;
use javaforge_android::{adb, gradle, manifest, run as app_run};
use javaforge_lang_groovy::launch::{self as groovy, Runner, ScriptRun};
use javaforge_lang_scala::sbt;
use javaforge_lang_scala::ScalaTests;
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
struct TextRequest {
    text: String,
}

/// `{ text }` (Groovy source) → `Runnable` (`groovy_runnable`).
#[wasm_bindgen]
pub fn groovy_runnable(request_json: &str) -> String {
    match parse::<TextRequest>(request_json) {
        Ok(r) => to_json(&javaforge_lang_groovy::runnable(&r.text)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct GroovyRequest {
    run: ScriptRun,
    runner: String,
}

/// `{ run: ScriptRun, runner }` → `{ program, args, env }` (a Groovy Script run).
#[wasm_bindgen]
pub fn groovy_script_command(request_json: &str) -> String {
    match parse::<GroovyRequest>(request_json) {
        Ok(r) => match groovy::script_command(&r.run, Runner::parse(&r.runner)) {
            Ok(c) => to_json(&c),
            Err(e) => error(e),
        },
        Err(e) => error(e),
    }
}

/// `{ text }` (Scala source) → `MainObject[]` (`scala_main_objects`).
#[wasm_bindgen]
pub fn scala_main_objects(request_json: &str) -> String {
    match parse::<TextRequest>(request_json) {
        Ok(r) => to_json(&javaforge_lang_scala::main_objects(&r.text)),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SbtRequest {
    build_sbt: String,
    #[serde(default)]
    build_properties: Option<String>,
    #[serde(default)]
    plugins_sbt: Option<String>,
    folder: String,
    /// Recorded output of an import (`SYNC_COMMANDS`): merged over the static build.
    #[serde(default)]
    sync_output: Option<String>,
    #[serde(default)]
    root: String,
}

/// `{ buildSbt, buildProperties?, pluginsSbt?, folder, syncOutput?, root }` → `SbtBuild`
/// (`scala_sbt_model`, `scala_sbt_import`).
#[wasm_bindgen]
pub fn scala_sbt_build(request_json: &str) -> String {
    let r = match parse::<SbtRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let base = sbt::static_build(
        &r.build_sbt,
        r.build_properties.as_deref(),
        r.plugins_sbt.as_deref(),
        &r.folder,
    );
    match r.sync_output {
        Some(out) => to_json(&sbt::merge(base, &sbt::parse_sync(&out, &r.root))),
        None => to_json(&base),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SbtCommandRequest {
    configuration: Configuration,
    #[serde(default)]
    program: String,
}

/// `{ configuration, program }` → `{ program, args, test? }`: an sbt Task, ScalaTest or MUnit
/// configuration as the backend builds it.
#[wasm_bindgen]
pub fn scala_command_line(request_json: &str) -> String {
    let r = match parse::<SbtCommandRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let cfg = &r.configuration;
    let program = if r.program.is_empty() { "sbt".to_string() } else { r.program.clone() };
    match cfg.type_id.as_str() {
        "scalatest" | "munit" => {
            let provider = if cfg.type_id == "munit" {
                ScalaTests::munit()
            } else {
                ScalaTests::scalatest()
            };
            let prepared = cfg.clone().with_option("buildProgram", program);
            match provider.command(&prepared, &CommandContext::default()) {
                Some(Ok(c)) => to_json(&json!({
                    "program": c.program,
                    "args": c.args,
                    "test": { "locationRules": c.location_rules, "defaultFile": c.default_file, "reportDirs": c.report_dirs },
                })),
                Some(Err(e)) => error(e),
                None => error("no command"),
            }
        }
        _ => {
            let tasks = sbt::split_tasks(cfg.option("tasks"));
            let vm = forge_runtools::args::split_args(&cfg.interpreter_options);
            let (program, args) = sbt::task_command(
                &program,
                cfg.option("project").trim(),
                &tasks,
                &vm,
                &forge_runtools::args::split_args(&cfg.arguments),
            );
            to_json(&json!({ "program": program, "args": args }))
        }
    }
}

#[derive(Deserialize)]
struct AdbRequest {
    #[serde(default)]
    devices: String,
    #[serde(default)]
    avds: String,
}

/// `{ devices, avds }` (outputs of `adb devices -l` and `emulator -list-avds`) →
/// `{ devices: Device[], avds: string[] }`.
#[wasm_bindgen]
pub fn android_adb(request_json: &str) -> String {
    match parse::<AdbRequest>(request_json) {
        Ok(r) => to_json(&json!({
            "devices": adb::parse_devices(&r.devices),
            "avds": adb::parse_avds(&r.avds),
        })),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModuleRequest {
    build: String,
    #[serde(default)]
    manifest: Option<String>,
}

/// `{ build, manifest? }` → `{ module: AndroidModule | null, launcher: string | null }`.
#[wasm_bindgen]
pub fn android_module(request_json: &str) -> String {
    match parse::<ModuleRequest>(request_json) {
        Ok(r) => {
            let module = gradle::android_module(&r.build);
            let launcher = r.manifest.as_deref().and_then(|xml| {
                manifest::parse(xml, module.as_ref().and_then(|m| m.namespace.as_deref()))
                    .launcher_activity()
                    .map(|a| a.name.clone())
            });
            to_json(&json!({ "module": module, "launcher": launcher }))
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct AppRequest {
    run: AppRun,
    #[serde(default)]
    windows: bool,
}

/// `{ run: AppRun, windows? }` → `{ program, args }` (an Android App run).
#[wasm_bindgen]
pub fn android_app_command(request_json: &str) -> String {
    let r = match parse::<AppRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    match app_run::steps(&r.run) {
        Ok(steps) => {
            let (program, args) = app_run::chain(&steps, r.windows);
            to_json(&json!({ "program": program, "args": args, "steps": steps }))
        }
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogcatRequest {
    /// `adb logcat -v threadtime` output so far.
    text: String,
    query: String,
    #[serde(default)]
    mine: Vec<String>,
    #[serde(default)]
    sources: Vec<String>,
    /// `adb shell ps -A -o PID,NAME` output.
    #[serde(default)]
    ps: String,
    #[serde(default)]
    limit: Option<usize>,
}

/// `{ text, query, mine?, sources?, ps?, limit? }` → `LogcatView` (`android_logcat_view`).
#[wasm_bindgen]
pub fn android_logcat_view(request_json: &str) -> String {
    let r = match parse::<LogcatRequest>(request_json) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let mut l = Logcat::new(100_000);
    l.set_sources(r.sources);
    l.set_processes(&adb::parse_ps(&r.ps));
    l.push(&r.text);
    to_json(&l.view(&Query::parse(&r.query), &r.mine, r.limit.unwrap_or(5000)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_the_commands() {
        let v: serde_json::Value = serde_json::from_str(&android_logcat_view(
            &json!({
                "text": "10-27 10:14:03.512  1432  1459 I ActivityManager: Start proc 12034:com.acme.notes/u0a190 for x\n10-27 10:14:03.640 12034 12034 E AndroidRuntime: \tat com.acme.notes.Main.run(Main.kt:3)\n",
                "query": "package:mine",
                "mine": ["com.acme.notes"],
                "sources": ["app/src/main/java/com/acme/notes/Main.kt"],
            })
            .to_string(),
        ))
        .unwrap();
        assert_eq!(v["matched"], 1);
        assert_eq!(v["entries"][0]["link"]["line"], 3);
        let r: serde_json::Value = serde_json::from_str(&groovy_runnable(&json!({ "text": "println 'x'" }).to_string())).unwrap();
        assert_eq!(r["script"], true);
        let c: serde_json::Value = serde_json::from_str(&scala_command_line(
            &json!({ "configuration": { "name": "t", "type": "sbt-task", "options": { "tasks": "test", "project": "core" } } }).to_string(),
        ))
        .unwrap();
        assert_eq!(c["args"][0], "core/test");
    }
}
