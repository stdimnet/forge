//! KotlinForge IPC: the Kotlin language server, Kotlin runs and scripts, ktlint and detekt,
//! and the Kotlin debugger. The logic lives in `packs/kotlinforge/lang-kotlin` and
//! `forge_langservers::managed::kotlin`; JDKs and builds come from JavaForge
//! (`commands/java.rs`).
//!
//! - Language server: [`sync_lsp_server`] registers kotlin-lsp (or fwcd's
//!   kotlin-language-server) for a local Kotlin project; `kotlin_status` tells the UI what
//!   is missing, `kotlin_lsp_install` downloads the pinned kotlin-lsp with the user's
//!   consent (progress on `kotlin://install`).
//! - Runs (`run.rs`): [`command_line`] builds Kotlin applications (Application
//!   configurations with `kotlin = true`: Gradle init script, Maven `exec:java`, `kotlinc`)
//!   and `kotlin-script` configurations. Kotest specs run through `KotlinTests` and
//!   JavaForge's build preparation.
//! - Linters: `kotlin_lint` (ktlint on the editor text), `kotlin_format` (ktlint format),
//!   `kotlin_detekt` (detekt CLI on files, or the project's Gradle tasks),
//!   `kotlin_linters_install` (pinned jars from Maven Central).
//! - Debugging: `kotlin_debug_prepare` starts kotlin-lsp's DAP server and picks the JDWP
//!   port; `kotlin_coroutines` reads the debugger's coroutine dump.

use std::path::{Path, PathBuf};
use std::time::Duration;

use forge_backend::process_io::capture;
use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_langservers::managed::kotlin::{self as managed, PinnedJar, DETEKT, KTLINT};
use forge_langservers::managed::{Event, Http};
use forge_lsp::LspServerConfig;
use forge_process::{CommandLine, RunConfiguration};
use javaforge_build::BuildTool;
use kotlinforge_lang_kotlin::coroutines::{self, CoroutineDump};
use kotlinforge_lang_kotlin::launch::{self, KotlinApp, Runner, ScriptRun};
use kotlinforge_lang_kotlin::lint::{self, LintProblem, Linter};
use kotlinforge_lang_kotlin::lsp::{KotlinServerSetup, Server};
use kotlinforge_lang_kotlin::MainFunction;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::java;
use crate::commands::lsp::LspState;
use crate::commands::workspace::blocking;
use crate::state::AppState;

pub const PLUGIN_ID: &str = "forge.kotlin";
/// `{ id: "kotlin-lsp" | "ktlint" | "detekt", event: managed::Event }` while installing.
pub const INSTALL_EVENT: &str = "kotlin://install";
/// The Kotlin script configuration type.
pub const SCRIPT: &str = "kotlin-script";
const LINT_TIMEOUT: Duration = Duration::from_secs(90);

fn setting(state: &AppState, key: &str) -> String {
    state
        .read_settings()
        .get_as::<String>(key)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn flag(state: &AppState, key: &str, default: bool) -> bool {
    state.read_settings().get_as::<bool>(key).unwrap_or(default)
}

fn windows() -> bool {
    cfg!(windows)
}

fn other(e: impl ToString) -> ForgeError {
    ForgeError::Other(e.to_string())
}

// ---- Language server -------------------------------------------------------------------

/// The configured server and its launcher, or why there is none.
fn server_command(state: &AppState) -> (Server, std::result::Result<(String, String), String>) {
    let server = Server::parse(&setting(state, "kotlin.lsp.server"));
    let custom = setting(state, "kotlin.lsp.path");
    let found = match server {
        Server::KotlinLsp => {
            let install = if custom.is_empty() {
                managed::kotlin_lsp_dir().and_then(|d| managed::installed_kotlin_lsp(&d))
            } else {
                managed::inspect_kotlin_lsp(Path::new(&custom))
            };
            match install {
                Some(i) => Ok((i.launcher.display().to_string(), i.version)),
                None => forge_lsp::find_on_path("kotlin-lsp")
                    .or_else(|| forge_lsp::find_on_path("intellij-server"))
                    .map(|p| (p.display().to_string(), String::new()))
                    .ok_or_else(|| "kotlin-lsp is not installed".to_string()),
            }
        }
        Server::KotlinLanguageServer => {
            let p = if custom.is_empty() {
                forge_lsp::find_on_path("kotlin-language-server")
            } else {
                Some(PathBuf::from(&custom)).filter(|p| p.is_file())
            };
            p.map(|p| (p.display().to_string(), String::new()))
                .ok_or_else(|| "kotlin-language-server was not found on the PATH".to_string())
        }
    };
    (server, found)
}

/// JAVA_HOME of the server's build import: `kotlin.lsp.jdk`, else the project JDK.
fn import_jdk(state: &AppState) -> Option<String> {
    let own = setting(state, "kotlin.lsp.jdk");
    if !own.is_empty() {
        return Some(own);
    }
    let jdks = java::all_jdks(state, java::java_state(), false);
    java::project_jdk(state, &jdks).map(|j| j.home)
}

fn lsp_config(state: &AppState, server: Server, command: String) -> LspServerConfig {
    let setup = KotlinServerSetup {
        command,
        java_home: import_jdk(state),
        jvm_options: forge_process::split_args(&setting(state, "kotlin.lsp.jvmOptions")),
        arguments: Vec::new(),
    };
    let storage = dirs::cache_dir().map(|d| d.join("forge").join("kotlin-language-server"));
    LspServerConfig {
        id: server.id().into(),
        name: Some(server.label().into()),
        language_ids: vec!["kotlin".into()],
        // Gradle build scripts are left to the Gradle DSL support.
        file_extensions: vec!["kt".into(), "main.kts".into()],
        command: setup.command.clone(),
        args: setup.args(server),
        initialization_options: setup.initialization_options(
            server,
            storage.as_ref().map(|s| s.to_string_lossy()).as_deref(),
        ),
        settings: Value::Null,
        env: setup.env(server),
        pull_diagnostics: server == Server::KotlinLsp,
        after_initialized: Vec::new(),
    }
}

/// Register (or remove) the Kotlin language server for the open project: a local Kotlin
/// project with the plugin enabled and the server installed.
pub fn sync_lsp_server(app: &AppHandle, detected: Option<(&Path, &forge_project::ProjectInfo)>) {
    let state = app.state::<AppState>();
    let lsp = app.state::<LspState>();
    let remove_all = || {
        for s in [Server::KotlinLsp, Server::KotlinLanguageServer] {
            lsp.unregister_server(s.id());
        }
    };
    let Ok(backend) = state.backend() else {
        remove_all();
        return;
    };
    let root = backend.info().root.clone();
    let fresh;
    let info = match detected {
        Some((r, _)) if r != root => return,
        Some((_, info)) => info,
        None => {
            fresh = match backend.detect_project() {
                Ok(i) => i,
                Err(e) => {
                    tracing::warn!("project detection failed: {e}");
                    return;
                }
            };
            &fresh
        }
    };
    let enabled = state
        .plugins
        .read()
        .expect("plugins lock poisoned")
        .is_enabled(PLUGIN_ID);
    if !enabled || !info.uses("kotlin") || backend.info().remote.is_some() {
        remove_all();
        return;
    }
    let (server, found) = server_command(&state);
    for s in [Server::KotlinLsp, Server::KotlinLanguageServer] {
        if s != server {
            lsp.unregister_server(s.id());
        }
    }
    match found {
        Ok((command, _)) => {
            tracing::info!("{} with {command}", server.label());
            lsp.register_server(lsp_config(&state, server, command));
        }
        Err(why) => {
            tracing::info!("{} not started: {why}", server.label());
            lsp.unregister_server(server.id());
        }
    }
}

/// Whether a changed setting reconfigures the server.
pub fn affects_server(key: &str) -> bool {
    key.starts_with("kotlin.lsp.") || matches!(key, "java.projectJdk" | "java.jdks")
}

pub fn spawn_sync_lsp_server(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || sync_lsp_server(&app, None));
}

/// A linter as the UI shows it: where it comes from.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    /// `setting`, `path`, `managed` or `gradle` (the project's Gradle plugin).
    pub source: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KotlinStatus {
    /// `kotlin-lsp` or `kotlin-language-server`.
    pub server: String,
    pub server_path: Option<String>,
    pub server_version: Option<String>,
    /// The pinned kotlin-lsp version Forge downloads (and its size in bytes).
    pub pinned_version: String,
    pub download_size: Option<u64>,
    pub ktlint: Option<ToolInfo>,
    pub detekt: Option<ToolInfo>,
    /// `kotlinc` for runs without a build and scripts.
    pub kotlinc: Option<String>,
    /// Why the server does not run (empty when it can).
    pub problems: Vec<String>,
    pub remote: bool,
}

/// How to run ktlint or detekt: a program and the arguments before the tool's own.
struct Tool {
    program: String,
    prefix: Vec<String>,
    info: ToolInfo,
}

fn java_program(state: &AppState) -> String {
    let jdks = java::all_jdks(state, java::java_state(), false);
    java::project_jdk(state, &jdks)
        .map(|j| javaforge_jdk::java_executable(&j.home, windows()))
        .unwrap_or_else(|| "java".into())
}

fn jar_tool(state: &AppState, jar: &Path, source: &str) -> Tool {
    Tool {
        program: java_program(state),
        prefix: vec!["-jar".into(), jar.display().to_string()],
        info: ToolInfo {
            source: source.into(),
            path: jar.display().to_string(),
        },
    }
}

/// ktlint / detekt: the setting, the PATH (on the project's host), Forge's download.
fn find_tool(state: &AppState, backend: &dyn Backend, linter: Linter) -> Option<Tool> {
    let (key, names, pinned): (&str, &[&str], &PinnedJar) = match linter {
        Linter::Ktlint => ("kotlin.ktlint.path", &["ktlint"], &KTLINT),
        Linter::Detekt => ("kotlin.detekt.path", &["detekt", "detekt-cli"], &DETEKT),
    };
    let custom = setting(state, key);
    if custom.ends_with(".jar") {
        return Some(jar_tool(state, Path::new(&custom), "setting"));
    }
    let names: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    let found = backend
        .find_executable(&names, Some(custom.as_str()).filter(|c| !c.is_empty()))
        .ok()
        .flatten();
    if let Some(p) = found {
        return Some(Tool {
            program: p.display().to_string(),
            prefix: Vec::new(),
            info: ToolInfo {
                source: if custom.is_empty() { "path" } else { "setting" }.into(),
                path: p.display().to_string(),
            },
        });
    }
    let jar = managed::jars_dir().and_then(|d| managed::installed_jar(&d, pinned))?;
    Some(jar_tool(state, &jar, "managed"))
}

/// The project's Gradle plugin for a linter (`gradle` source), when there is no CLI.
fn gradle_plugin(backend: &dyn Backend, linter: Linter) -> bool {
    ["build.gradle.kts", "build.gradle"]
        .iter()
        .filter_map(|f| backend.read_optional(f).ok().flatten())
        .any(|b| lint::gradle_plugin(&b, linter))
}

#[tauri::command]
pub async fn kotlin_status(app: AppHandle) -> Result<KotlinStatus> {
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let (server, found) = server_command(&state);
        let release = managed::current_kotlin_lsp_release();
        let tool_info = |l: Linter| {
            find_tool(&state, backend.as_ref(), l)
                .map(|t| t.info)
                .or_else(|| {
                    gradle_plugin(backend.as_ref(), l).then(|| ToolInfo {
                        source: "gradle".into(),
                        path: String::new(),
                    })
                })
        };
        let kotlinc = backend
            .find_executable(
                &["kotlinc".to_string()],
                Some(setting(&state, "kotlin.compiler.path").as_str()).filter(|c| !c.is_empty()),
            )
            .ok()
            .flatten()
            .map(|p| p.display().to_string());
        Ok(KotlinStatus {
            server: server.id().into(),
            server_path: found.as_ref().ok().map(|(p, _)| p.clone()),
            server_version: found
                .as_ref()
                .ok()
                .map(|(_, v)| v.clone())
                .filter(|v| !v.is_empty()),
            pinned_version: managed::KOTLIN_LSP_VERSION.into(),
            download_size: release.map(|(d, _)| d.size),
            ktlint: tool_info(Linter::Ktlint),
            detekt: tool_info(Linter::Detekt),
            kotlinc,
            problems: found.err().into_iter().collect(),
            remote: backend.info().remote.is_some(),
        })
    })
    .await
}

fn emitter(app: &AppHandle, id: &'static str) -> impl FnMut(Event) + Send {
    let app = app.clone();
    move |event: Event| {
        let _ = app.emit(INSTALL_EVENT, json!({ "id": id, "event": event }));
    }
}

/// Download the pinned kotlin-lsp (download.jetbrains.com, SHA-256), then start it.
/// Returns its version.
#[tauri::command]
pub async fn kotlin_lsp_install(app: AppHandle) -> Result<String> {
    let base =
        managed::kotlin_lsp_dir().ok_or_else(|| other("no user data directory for kotlin-lsp"))?;
    let http = Http::new().map_err(ForgeError::Other)?;
    let mut on = emitter(&app, "kotlin-lsp");
    let inst = managed::install_kotlin_lsp(&http, &base, &mut on)
        .await
        .map_err(ForgeError::Other)?;
    let h = app.clone();
    tauri::async_runtime::spawn_blocking(move || sync_lsp_server(&h, None))
        .await
        .map_err(other)?;
    Ok(inst.version)
}

/// Download the pinned ktlint and detekt jars (Maven Central, SHA-256).
#[tauri::command]
pub async fn kotlin_linters_install(app: AppHandle) -> Result<Vec<String>> {
    let state = app.state::<AppState>();
    let repository = match setting(&state, "kotlin.download.repository") {
        r if r.is_empty() => managed::DEFAULT_REPOSITORY.to_string(),
        r => r,
    };
    let base =
        managed::jars_dir().ok_or_else(|| other("no user data directory for ktlint and detekt"))?;
    let http = Http::new().map_err(ForgeError::Other)?;
    let mut out = Vec::new();
    for (id, tool) in [("ktlint", &KTLINT), ("detekt", &DETEKT)] {
        let mut on = emitter(&app, id);
        let p = managed::install_jar(&http, &repository, &base, tool, &mut on)
            .await
            .map_err(ForgeError::Other)?;
        out.push(p.display().to_string());
    }
    Ok(out)
}

// ---- Run configurations ----------------------------------------------------------------

/// `main` functions of a file for gutter icons (pure; also in WebAssembly).
#[tauri::command]
pub fn kotlin_main_functions(path: String, text: String) -> Vec<MainFunction> {
    kotlinforge_lang_kotlin::main_functions(&path, &text)
}

/// The Gradle init script in Forge's cache directory (written when it changed).
fn init_script() -> Result<PathBuf> {
    let dir = dirs::cache_dir()
        .ok_or_else(|| other("no user cache directory"))?
        .join("forge")
        .join("kotlin");
    let file = dir.join("run.init.gradle");
    if std::fs::read_to_string(&file).ok().as_deref() != Some(launch::GRADLE_INIT_SCRIPT) {
        std::fs::create_dir_all(&dir).map_err(other)?;
        std::fs::write(&file, launch::GRADLE_INIT_SCRIPT).map_err(other)?;
    }
    Ok(file)
}

fn kotlin_program(state: &AppState, backend: &dyn Backend, names: &[&str]) -> Option<String> {
    let custom = setting(state, "kotlin.compiler.path");
    // A compiler home or its bin folder: the program inside it.
    if !custom.is_empty() {
        let p = Path::new(&custom);
        let dir = if p.is_file() {
            p.parent().map(Path::to_path_buf)
        } else {
            Some(p.join("bin"))
                .filter(|b| b.is_dir())
                .or(Some(p.to_path_buf()))
        };
        for n in names {
            if let Some(d) = &dir {
                for candidate in [d.join(n), d.join(format!("{n}.bat"))] {
                    if candidate.is_file() {
                        return Some(candidate.display().to_string());
                    }
                }
            }
        }
    }
    let names: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    backend
        .find_executable(&names, None)
        .ok()
        .flatten()
        .map(|p| p.display().to_string())
}

/// FNV-1a, to keep the jars of different mains apart.
fn short_hash(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:08x}", h as u32)
}

/// The command line of a Kotlin application or script (`None` for other configurations).
pub(crate) fn command_line(
    state: &AppState,
    cfg: &RunConfiguration,
    debug_port: Option<u16>,
) -> Result<Option<CommandLine>> {
    let kotlin_app = cfg.type_id == java::APPLICATION && cfg.option("kotlin") == "true";
    if !kotlin_app && cfg.type_id != SCRIPT {
        return Ok(None);
    }
    let backend = state.backend()?;
    let root = backend.info().root.clone();
    let cwd = match cfg.working_directory.trim() {
        "" => root.clone(),
        d if Path::new(d).is_absolute() => PathBuf::from(d),
        d => root.join(d),
    };
    let abs = |p: &str| -> String {
        if Path::new(p).is_absolute() {
            p.to_string()
        } else {
            root.join(p).display().to_string()
        }
    };
    let built = if cfg.type_id == SCRIPT {
        let program = match cfg.option("kotlin").trim() {
            "" => kotlin_program(state, backend.as_ref(), &["kotlin", "kotlinc"])
                .unwrap_or_else(|| "kotlin".into()),
            own => own.to_string(),
        };
        launch::script_command(&ScriptRun {
            script: abs(cfg.option("script").trim()),
            program,
            arguments: cfg.arguments.clone(),
            vm_options: cfg.interpreter_options.clone(),
        })
        .map_err(ForgeError::Other)?
    } else {
        let build = java::read_build(state, backend.as_ref());
        let runner = Runner::parse(&match cfg.option("runner").trim() {
            "" => setting(state, "kotlin.run.runner"),
            r => r.to_string(),
        })
        .resolve(build.as_ref().map(|b| b.tool.id()));
        let mut app = KotlinApp {
            main_class: cfg.option("mainClass").to_string(),
            vm_options: cfg.interpreter_options.clone(),
            arguments: cfg.arguments.clone(),
            working_dir: cwd.display().to_string(),
            windows: windows(),
            debug_port,
            ..Default::default()
        };
        match runner {
            Runner::Gradle | Runner::Maven | Runner::Auto => {
                let tool = if runner == Runner::Maven {
                    BuildTool::Maven
                } else {
                    BuildTool::Gradle
                };
                let project = build.filter(|b| b.tool == tool).ok_or_else(|| {
                    other(format!(
                        "Kotlin runs through {}: the project root has no {} build (choose kotlinc in the configuration)",
                        if tool == BuildTool::Maven { "Maven" } else { "Gradle" },
                        if tool == BuildTool::Maven { "Maven" } else { "Gradle" },
                    ))
                })?;
                let opts = java::exec_options(state, tool, &root);
                app.build_program = javaforge_build::executable(tool, project.wrapper, &opts);
                app.offline = opts.offline;
                let module = cfg.option("module").trim().to_string();
                app.module = module.clone();
                app.gradle_path = project
                    .module(&module)
                    .map(|m| m.path.clone())
                    .unwrap_or_default();
                if tool == BuildTool::Gradle {
                    if backend.info().remote.is_some() {
                        return Err(other("Kotlin applications of remote projects run through Gradle tasks: choose kotlinc or a Gradle configuration"));
                    }
                    app.init_script = init_script()?.display().to_string();
                }
            }
            Runner::Kotlinc => {
                app.kotlinc = kotlin_program(state, backend.as_ref(), &["kotlinc"])
                    .ok_or_else(|| other("kotlinc was not found: install the Kotlin compiler or set it in Settings | Languages & Frameworks | Kotlin"))?;
                app.java = java_program(state);
                let file = cfg.option("file").trim();
                app.sources = if file.is_empty() {
                    let module = cfg.option("module").trim();
                    vec![abs(&if module.is_empty() {
                        "src/main/kotlin".to_string()
                    } else {
                        format!("{module}/src/main/kotlin")
                    })]
                } else {
                    vec![abs(file)]
                };
                app.output_jar = std::env::temp_dir()
                    .join("forge-kotlin")
                    .join(format!(
                        "{}.jar",
                        short_hash(&format!("{}{}", root.display(), app.main_class))
                    ))
                    .display()
                    .to_string();
                if let Some(parent) = Path::new(&app.output_jar).parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
            }
        }
        launch::application_command(&app, runner).map_err(ForgeError::Other)?
    };
    let mut cmd = CommandLine::new(built.program).args(built.args);
    cmd.cwd = cwd;
    for (k, v) in built.env {
        cmd.env.insert(k, v);
    }
    if let Some(home) = java::jdk_home(state, cfg) {
        cmd.env.entry("JAVA_HOME".into()).or_insert(home);
    }
    cmd.env.extend(cfg.env.clone());
    Ok(Some(cmd))
}

// ---- ktlint and detekt -----------------------------------------------------------------

fn refuse_project_program(
    state: &AppState,
    backend: &dyn Backend,
    tool: &Tool,
    label: &str,
) -> Result<()> {
    let program = tool.prefix.get(1).unwrap_or(&tool.program);
    if state.safe_mode.is_on()
        && forge_trust::runs_project_program(&backend.info().root, program, &[])
    {
        return Err(other(forge_trust::refusal(&format!(
            "Running the project's {label}"
        ))));
    }
    Ok(())
}

/// The problems ktlint finds in the editor text of `path` (empty without ktlint).
#[tauri::command]
pub async fn kotlin_lint(
    path: String,
    content: String,
    app: AppHandle,
) -> Result<Vec<LintProblem>> {
    blocking(move || {
        let state = app.state::<AppState>();
        if !flag(&state, "kotlin.ktlint.enabled", true) {
            return Ok(Vec::new());
        }
        let backend = state.backend()?;
        let Some(tool) = find_tool(&state, backend.as_ref(), Linter::Ktlint) else {
            return Ok(Vec::new());
        };
        refuse_project_program(&state, backend.as_ref(), &tool, "ktlint")?;
        let mut cmd = CommandLine::new(&tool.program).args(tool.prefix.clone());
        cmd = cmd.args(lint::ktlint_args(&path, false));
        cmd.cwd = backend.info().root.clone();
        let out = capture(backend.as_ref(), &cmd, &content, LINT_TIMEOUT)?;
        if out.code.is_none() {
            return Err(other("ktlint did not finish"));
        }
        let mut problems = lint::parse_ktlint_json(&out.stdout).map_err(|e| {
            let err = out
                .stderr
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or_default();
            other(if err.is_empty() {
                e
            } else {
                format!("ktlint: {err}")
            })
        })?;
        for p in &mut problems {
            p.file = path.clone();
        }
        Ok(problems)
    })
    .await
}

/// ktlint format of the editor text (`None` without ktlint).
#[tauri::command]
pub async fn kotlin_format(
    path: String,
    content: String,
    app: AppHandle,
) -> Result<Option<String>> {
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let Some(tool) = find_tool(&state, backend.as_ref(), Linter::Ktlint) else {
            return Ok(None);
        };
        refuse_project_program(&state, backend.as_ref(), &tool, "ktlint")?;
        let mut cmd = CommandLine::new(&tool.program).args(tool.prefix.clone());
        cmd = cmd.args(lint::ktlint_args(&path, true));
        cmd.cwd = backend.info().root.clone();
        let out = capture(backend.as_ref(), &cmd, &content, LINT_TIMEOUT)?;
        match out.code {
            // ktlint exits with 1 when it could not fix everything; stdout is the text.
            Some(0) | Some(1) if !out.stdout.is_empty() => Ok(Some(out.stdout)),
            None => Err(other("ktlint did not finish")),
            _ => Err(other(
                out.stderr
                    .lines()
                    .find(|l| !l.trim().is_empty())
                    .unwrap_or("ktlint failed")
                    .to_string(),
            )),
        }
    })
    .await
}

fn relative(root: &Path, file: &str) -> String {
    let p = Path::new(file);
    match p.strip_prefix(root) {
        Ok(r) => r.to_string_lossy().replace('\\', "/"),
        Err(_) => file.replace('\\', "/"),
    }
}

/// detekt's findings: in `paths` (project-relative files) or the whole project. Runs the
/// detekt CLI, or without one the project's `detekt` Gradle task (a project program: only
/// in trusted projects).
#[tauri::command]
pub async fn kotlin_detekt(paths: Option<Vec<String>>, app: AppHandle) -> Result<Vec<LintProblem>> {
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let root = backend.info().root.clone();
        if backend.info().remote.is_some() {
            return Err(other("detekt runs on local projects"));
        }
        let mut problems = match find_tool(&state, backend.as_ref(), Linter::Detekt) {
            Some(tool) => {
                refuse_project_program(&state, backend.as_ref(), &tool, "detekt")?;
                let input: Vec<String> = match &paths {
                    Some(p) if !p.is_empty() => p
                        .iter()
                        .map(|f| root.join(f).display().to_string())
                        .collect(),
                    _ => vec![root.display().to_string()],
                };
                let mut config = setting(&state, "kotlin.detekt.config");
                let candidates: Vec<String> = [
                    "config/detekt/detekt.yml",
                    "config/detekt.yml",
                    "detekt.yml",
                    ".detekt.yml",
                    "config/detekt/baseline.xml",
                    "detekt-baseline.xml",
                ]
                .iter()
                .filter(|f| root.join(f).is_file())
                .map(|f| f.to_string())
                .collect();
                let (auto_config, baseline) = lint::detekt_config_files(&candidates);
                if config.is_empty() {
                    config = auto_config.unwrap_or_default();
                }
                let report = std::env::temp_dir().join(format!(
                    "forge-detekt-{}.sarif",
                    short_hash(&root.display().to_string())
                ));
                let _ = std::fs::remove_file(&report);
                let mut cmd = CommandLine::new(&tool.program).args(tool.prefix.clone());
                cmd = cmd.args(lint::detekt_args(
                    &input,
                    Some(&config),
                    baseline.as_deref(),
                    &report.display().to_string(),
                ));
                cmd.cwd = root.clone();
                let out = capture(backend.as_ref(), &cmd, "", Duration::from_secs(600))?;
                let text = std::fs::read_to_string(&report).map_err(|_| {
                    let err = out
                        .stderr
                        .lines()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or("detekt wrote no report");
                    other(format!("detekt: {err}"))
                })?;
                let _ = std::fs::remove_file(&report);
                lint::parse_sarif(&text).map_err(ForgeError::Other)?
            }
            None if gradle_plugin(backend.as_ref(), Linter::Detekt) => {
                state.require_trusted("Running the project's detekt Gradle task")?;
                let build = java::read_build(&state, backend.as_ref())
                    .ok_or_else(|| other("no Gradle build in the project root"))?;
                let opts = java::exec_options(&state, BuildTool::Gradle, &root);
                let program = javaforge_build::executable(BuildTool::Gradle, build.wrapper, &opts);
                let mut cmd = CommandLine::new(program).args([
                    "--console=plain",
                    "-q",
                    "detekt",
                    "--continue",
                ]);
                cmd.cwd = root.clone();
                let _ = capture(backend.as_ref(), &cmd, "", Duration::from_secs(900))?;
                let mut out = Vec::new();
                for dir in std::iter::once(String::new())
                    .chain(build.modules.iter().map(|m| format!("{}/", m.dir)))
                {
                    let sarif = root.join(format!("{dir}build/reports/detekt/detekt.sarif"));
                    let xml = root.join(format!("{dir}build/reports/detekt/detekt.xml"));
                    if let Ok(t) = std::fs::read_to_string(&sarif) {
                        out.extend(lint::parse_sarif(&t).unwrap_or_default());
                    } else if let Ok(t) = std::fs::read_to_string(&xml) {
                        out.extend(lint::parse_checkstyle(&t, Linter::Detekt).unwrap_or_default());
                    }
                }
                out
            }
            None => return Ok(Vec::new()),
        };
        for p in &mut problems {
            p.file = relative(&root, &p.file);
        }
        if let Some(only) = paths.filter(|p| !p.is_empty()) {
            problems.retain(|p| only.iter().any(|f| lint::same_file(&p.file, f)));
        }
        Ok(problems)
    })
    .await
}

// ---- Debugging -------------------------------------------------------------------------

/// What a Kotlin debug session needs: the adapter (`kotlin`: kotlin-lsp's DAP server,
/// `java`: java-debug), its port and the JVM's JDWP address.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KotlinDebugPlan {
    pub adapter: String,
    pub dap_port: u16,
    pub host_name: String,
    pub jdwp_port: u16,
    /// The run must be started with `FORGE_JAVA_DEBUG_PORT` (not for Remote JVM Debug).
    pub run: bool,
}

#[tauri::command]
pub async fn kotlin_debug_prepare(
    configuration: RunConfiguration,
    app: AppHandle,
) -> Result<KotlinDebugPlan> {
    let state = app.state::<AppState>();
    if setting(&state, "kotlin.debug.debugger") == "java-debug" {
        let plan = java::java_debug_prepare(configuration, app.clone()).await?;
        return Ok(KotlinDebugPlan {
            adapter: "java".into(),
            dap_port: plan.dap_port,
            host_name: plan.host_name,
            jdwp_port: plan.jdwp_port,
            run: plan.run,
        });
    }
    let root = state.project_root()?;
    let manager = app.state::<LspState>().manager(&app, &state)?;
    let running = manager
        .running_clients()
        .into_iter()
        .any(|c| c.id() == Server::KotlinLsp.id());
    if !running {
        return Err(other(
            "The Kotlin debugger runs in kotlin-lsp: open a Kotlin file once kotlin-lsp is installed, or choose java-debug in Settings | Languages & Frameworks | Kotlin",
        ));
    }
    let folder = forge_lsp::path_to_uri(&root).map_err(other)?.to_string();
    let port = manager
        .execute_command(
            &root.join("Main.kt"),
            Server::KotlinLsp.id(),
            "start_debug_server",
            vec![json!(folder)],
        )
        .await?
        .and_then(|v| v.as_u64())
        .ok_or_else(|| other("kotlin-lsp did not start its debug server"))? as u16;
    let (host, jdwp, run) = match configuration.type_id.as_str() {
        java::REMOTE => {
            let host = match configuration.option("host").trim() {
                "" => "localhost".to_string(),
                h => h.to_string(),
            };
            let port = configuration
                .option("port")
                .trim()
                .parse::<u16>()
                .map_err(|_| other("Port is not a number"))?;
            (host, port, false)
        }
        java::GRADLE => (
            "127.0.0.1".to_string(),
            javaforge_lang_java::launch::GRADLE_DEBUG_PORT,
            true,
        ),
        "junit" | "testng" | "kotest" => {
            let backend = state.backend()?;
            let gradle = java::read_build(&state, backend.as_ref())
                .is_none_or(|b| b.tool == BuildTool::Gradle);
            let port = if gradle {
                javaforge_lang_java::launch::GRADLE_DEBUG_PORT
            } else {
                java::free_port()?
            };
            ("127.0.0.1".to_string(), port, true)
        }
        _ => ("127.0.0.1".to_string(), java::free_port()?, true),
    };
    Ok(KotlinDebugPlan {
        adapter: "kotlin".into(),
        dap_port: port,
        host_name: host,
        jdwp_port: jdwp,
        run,
    })
}

/// The debugger's answer to the coroutine dump expression (`value`), or its error.
#[tauri::command]
pub fn kotlin_coroutines(value: String) -> CoroutineDump {
    coroutines::parse_dump(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_and_names() {
        assert!(affects_server("kotlin.lsp.jvmOptions"));
        assert!(affects_server("java.projectJdk"));
        assert!(!affects_server("kotlin.ktlint.enabled"));
        assert_eq!(relative(Path::new("/p"), "/p/src/A.kt"), "src/A.kt");
        assert_eq!(relative(Path::new("/p"), "src/A.kt"), "src/A.kt");
        assert_eq!(short_hash("a"), short_hash("a"));
        assert!(matches!(
            kotlin_coroutines("\"[]\"".into()),
            CoroutineDump::Coroutines { .. }
        ));
    }
}
