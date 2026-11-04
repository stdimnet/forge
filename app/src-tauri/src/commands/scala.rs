//! JavaForge's Scala IPC: Metals, the sbt tool window, sbt runs and ScalaTest / MUnit. The
//! logic lives in `packs/javaforge/lang-scala`; JDKs come from JavaForge.
//!
//! - Language server: [`sync_lsp_server`] registers Metals (setting, Forge's install, PATH)
//!   for a local Scala project; `scala_status` tells the UI what is missing and
//!   `scala_metals_install` installs the pinned Metals with Coursier (`cs bootstrap`) into
//!   `<data dir>/forge/langservers/metals/<version>` with the user's consent (output on
//!   `scala://install`).
//! - sbt: `scala_sbt_model` (the build files, with the last import over it, cached in
//!   `<cache>/forge/javaforge/sbt`), `scala_sbt_import` (Reload: sbt prints its projects,
//!   Scala versions, folders and dependencies; output on `scala://sbt`; refused in Safe Mode
//!   and on remote projects), `scala_sbt_cancel`.
//! - Runs (`run.rs`): [`command_line`] builds `sbt-task` configurations (tasks or the sbt
//!   shell); [`prepare`] fills ScalaTest / MUnit configurations with sbt and the project.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_lsp::LspServerConfig;
use forge_process::{CommandLine, RunConfiguration};
use javaforge_lang_scala::metals;
use javaforge_lang_scala::sbt::{self, SbtBuild};
use javaforge_lang_scala::MainObject;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::java;
use crate::commands::lsp::LspState;
use crate::commands::workspace::blocking;
use crate::state::AppState;

pub const PLUGIN_ID: &str = "forge.scala";
/// The sbt Task configuration type.
pub const SBT_TASK: &str = "sbt-task";
/// `{ line, stderr }` while Metals installs.
pub const INSTALL_EVENT: &str = "scala://install";
/// `{ line, stderr }` while sbt imports the build.
pub const SBT_EVENT: &str = "scala://sbt";

fn windows() -> bool {
    cfg!(windows)
}

fn other(e: impl ToString) -> ForgeError {
    ForgeError::Other(e.to_string())
}

// ---- Tools -----------------------------------------------------------------------------

/// `<data dir>/forge/langservers/metals`.
fn metals_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("langservers").join("metals"))
}

fn launcher_name() -> &'static str {
    if windows() {
        "metals.bat"
    } else {
        "metals"
    }
}

/// The newest Metals Forge installed.
fn installed_metals() -> Option<PathBuf> {
    let dir = metals_dir()?;
    let mut versions: Vec<PathBuf> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path().join(launcher_name()))
        .filter(|p| p.is_file())
        .collect();
    versions.sort();
    versions.pop()
}

/// Metals: the setting, Forge's install, else the PATH.
fn find_metals(state: &AppState) -> Option<PathBuf> {
    let own = java::setting(state, "scala.metals.path");
    if !own.is_empty() {
        return Some(PathBuf::from(own)).filter(|p| p.is_file());
    }
    installed_metals().or_else(|| forge_lsp::find_on_path("metals"))
}

fn find_coursier(state: &AppState, backend: &dyn Backend) -> Option<PathBuf> {
    let own = java::setting(state, "scala.coursier.path");
    backend
        .find_executable(
            &["cs".to_string(), "coursier".to_string()],
            Some(own.as_str()).filter(|s| !s.is_empty()),
        )
        .ok()
        .flatten()
}

/// The sbt launcher: the setting, SDKMAN!'s current sbt, else the PATH.
pub(crate) fn find_sbt(state: &AppState, backend: &dyn Backend) -> Option<PathBuf> {
    let own = java::setting(state, "scala.sbt.path");
    if !own.is_empty() {
        return Some(PathBuf::from(own)).filter(|p| p.is_file());
    }
    if backend.info().remote.is_none() {
        let sdkman = std::env::var("SDKMAN_DIR")
            .ok()
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join(".sdkman")));
        if let Some(p) = sdkman
            .map(|d| {
                d.join("candidates/sbt/current/bin")
                    .join(if windows() { "sbt.bat" } else { "sbt" })
            })
            .filter(|p| p.is_file())
        {
            return Some(p);
        }
    }
    backend
        .find_executable(&["sbt".to_string()], None)
        .ok()
        .flatten()
}

fn project_java_home(state: &AppState) -> Option<String> {
    let jdks = java::all_jdks(state, java::java_state(), false);
    java::project_jdk(state, &jdks).map(|j| j.home)
}

// ---- Language server ---------------------------------------------------------------------

fn lsp_config(state: &AppState, command: String) -> LspServerConfig {
    let java_home = project_java_home(state);
    let sbt = state
        .backend()
        .ok()
        .and_then(|b| find_sbt(state, b.as_ref()))
        .map(|p| p.display().to_string());
    let mut env = std::collections::BTreeMap::new();
    if let Some(h) = &java_home {
        env.insert("JAVA_HOME".to_string(), h.clone());
    }
    LspServerConfig {
        id: metals::SERVER_ID.into(),
        name: Some("Metals".into()),
        language_ids: vec!["scala".into()],
        file_extensions: vec!["scala".into(), "sc".into(), "sbt".into()],
        command,
        args: Vec::new(),
        initialization_options: metals::initialization_options(),
        settings: metals::settings(java_home.as_deref(), sbt.as_deref()),
        env,
        pull_diagnostics: false,
        after_initialized: Vec::new(),
    }
}

/// Register (or remove) Metals for the open project: a local Scala project with the plugin
/// enabled and Metals installed.
pub fn sync_lsp_server(app: &AppHandle, detected: Option<(&Path, &forge_project::ProjectInfo)>) {
    let state = app.state::<AppState>();
    let lsp = app.state::<LspState>();
    let Ok(backend) = state.backend() else {
        lsp.unregister_server(metals::SERVER_ID);
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
    if !enabled || !info.uses("scala") || backend.info().remote.is_some() {
        lsp.unregister_server(metals::SERVER_ID);
        return;
    }
    match find_metals(&state) {
        Some(p) => {
            tracing::info!("Metals with {}", p.display());
            lsp.register_server(lsp_config(&state, p.display().to_string()));
        }
        None => lsp.unregister_server(metals::SERVER_ID),
    }
}

/// Whether a changed setting reconfigures Metals.
pub fn affects_server(key: &str) -> bool {
    key.starts_with("scala.metals.")
        || key == "scala.sbt.path"
        || matches!(key, "java.projectJdk" | "java.jdks")
}

pub fn spawn_sync_lsp_server(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || sync_lsp_server(&app, None));
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScalaStatus {
    pub metals: Option<String>,
    pub metals_version: Option<String>,
    /// The version `scala_metals_install` installs.
    pub pinned_version: String,
    pub coursier: Option<String>,
    pub sbt: Option<String>,
    /// Why Metals does not run (empty when it can).
    pub problems: Vec<String>,
    pub remote: bool,
}

#[tauri::command]
pub async fn scala_status(app: AppHandle) -> Result<ScalaStatus> {
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let remote = backend.info().remote.is_some();
        let metals = find_metals(&state);
        let coursier = find_coursier(&state, backend.as_ref());
        let mut problems = Vec::new();
        if remote {
            problems.push("Metals runs for local projects".to_string());
        } else if metals.is_none() {
            problems.push(if coursier.is_some() {
                "Metals is not installed".to_string()
            } else {
                "Metals is not installed, and Coursier (cs) was not found to install it: see get-coursier.io".to_string()
            });
        }
        if project_java_home(&state).is_none() {
            problems.push("No JDK was found for Metals and sbt".into());
        }
        Ok(ScalaStatus {
            metals_version: metals
                .as_ref()
                .and_then(|p| metals::version_of_install(&p.display().to_string())),
            metals: metals.map(|p| p.display().to_string()),
            pinned_version: metals::METALS_VERSION.into(),
            coursier: coursier.map(|p| p.display().to_string()),
            sbt: find_sbt(&state, backend.as_ref()).map(|p| p.display().to_string()),
            problems,
            remote,
        })
    })
    .await
}

#[derive(Clone, Serialize)]
struct Line {
    line: String,
    stderr: bool,
}

/// Install the pinned Metals with Coursier (the user asked for it).
#[tauri::command]
pub async fn scala_metals_install(app: AppHandle) -> Result<String> {
    let handle = app.clone();
    let version = blocking(move || {
        let state = handle.state::<AppState>();
        let backend = state.backend()?;
        let cs = find_coursier(&state, backend.as_ref()).ok_or_else(|| {
            other("Coursier (cs) was not found: install it from get-coursier.io, then try again")
        })?;
        let dir = metals_dir()
            .ok_or_else(|| other("no user data directory"))?
            .join(metals::METALS_VERSION);
        std::fs::create_dir_all(&dir).map_err(other)?;
        let out = dir.join(launcher_name());
        // Coursier is the user's own tool, not project code: it runs here, not through the
        // project's backend.
        let mut cmd = std::process::Command::new(&cs);
        cmd.args(metals::bootstrap_args(
            metals::METALS_VERSION,
            &out.display().to_string(),
        ))
        .current_dir(&dir);
        if let Some(h) = project_java_home(&state) {
            cmd.env("JAVA_HOME", h);
        }
        // In Flatpak Coursier runs on the host.
        let mut child = forge_process::host::host_command(cmd)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| other(format!("cannot run {}: {e}", cs.display())))?;
        let err_emit = handle.clone();
        let stderr = child.stderr.take().map(|e| {
            std::thread::spawn(move || {
                use std::io::BufRead;
                for line in std::io::BufReader::new(e).lines().map_while(|l| l.ok()) {
                    let _ = err_emit.emit(INSTALL_EVENT, Line { line, stderr: true });
                }
            })
        });
        if let Some(o) = child.stdout.take() {
            use std::io::BufRead;
            for line in std::io::BufReader::new(o).lines().map_while(|l| l.ok()) {
                let _ = handle.emit(
                    INSTALL_EVENT,
                    Line {
                        line,
                        stderr: false,
                    },
                );
            }
        }
        if let Some(t) = stderr {
            let _ = t.join();
        }
        let code = child.wait().map_err(other)?.code();
        if code != Some(0) || !out.is_file() {
            return Err(other(format!(
                "cs bootstrap failed{}",
                code.map(|c| format!(" (exit code {c})"))
                    .unwrap_or_default()
            )));
        }
        Ok(metals::METALS_VERSION.to_string())
    })
    .await?;
    spawn_sync_lsp_server(&app);
    Ok(version)
}

#[tauri::command]
pub fn scala_main_objects(text: String) -> Vec<MainObject> {
    javaforge_lang_scala::main_objects(&text)
}

// ---- sbt ---------------------------------------------------------------------------------

fn sbt_cache(root: &Path) -> Option<PathBuf> {
    dirs::cache_dir().map(|d| {
        d.join("forge").join("javaforge").join("sbt").join(format!(
            "{}.json",
            java::short_hash(&root.display().to_string())
        ))
    })
}

/// The build as its files describe it.
fn static_build(backend: &dyn Backend) -> Option<SbtBuild> {
    let build = backend.read_optional("build.sbt").ok().flatten();
    let props = backend
        .read_optional("project/build.properties")
        .ok()
        .flatten();
    if build.is_none()
        && props
            .as_deref()
            .is_none_or(|p| sbt::parse_build_properties(p).is_none())
    {
        return None;
    }
    let plugins = backend.read_optional("project/plugins.sbt").ok().flatten();
    let folder = backend
        .info()
        .root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    Some(sbt::static_build(
        build.as_deref().unwrap_or_default(),
        props.as_deref(),
        plugins.as_deref(),
        &folder,
    ))
}

/// The build with the last import over it.
pub(crate) fn current_build(backend: &dyn Backend) -> Option<SbtBuild> {
    let base = static_build(backend)?;
    let cached: Option<SbtBuild> = sbt_cache(&backend.info().root)
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|t| serde_json::from_str(&t).ok());
    Some(match cached {
        Some(s) => sbt::merge(base, &s),
        None => base,
    })
}

#[tauri::command]
pub async fn scala_sbt_model(state: State<'_, AppState>) -> Result<Option<SbtBuild>> {
    let backend = state.backend()?;
    blocking(move || Ok(current_build(backend.as_ref()))).await
}

static SBT_CANCEL: AtomicBool = AtomicBool::new(false);
static SBT_RUNNING: Mutex<()> = Mutex::new(());

/// Reload the sbt project: sbt prints its projects, Scala versions, folders and
/// dependencies (the build definition is project code).
#[tauri::command]
pub async fn scala_sbt_import(app: AppHandle) -> Result<SbtBuild> {
    let state = app.state::<AppState>();
    state.require_trusted("Reloading the sbt project")?;
    let backend = state.backend()?;
    if backend.info().remote.is_some() {
        return Err(other(
            "Reloading sbt projects on a remote host is not supported yet",
        ));
    }
    let Some(base) = static_build(backend.as_ref()) else {
        return Err(other("no build.sbt in the project root"));
    };
    let sbt_path = find_sbt(&state, backend.as_ref())
        .ok_or_else(|| other("sbt was not found: install it (sdk install sbt, or scala-sbt.org) or set it in Settings | Languages & Frameworks | Scala"))?;
    let java_home = project_java_home(&state);
    let vm = forge_process::split_args(&java::setting(&state, "scala.sbt.vmOptions"));
    let handle = app.clone();
    blocking(move || {
        let _guard = SBT_RUNNING
            .try_lock()
            .map_err(|_| other("an sbt import is already running"))?;
        SBT_CANCEL.store(false, Ordering::SeqCst);
        let root = backend.info().root.clone();
        let (program, mut args) =
            sbt::task_command(&sbt_path.display().to_string(), "", &[], &vm, &[]);
        args.extend(sbt::sync_args());
        let mut cmd = CommandLine::new(program).args(args);
        cmd.cwd = root.clone();
        if let Some(h) = java_home {
            cmd.env.insert("JAVA_HOME".into(), h);
        }
        let _ = handle.emit(
            SBT_EVENT,
            Line {
                line: format!("{} {}", cmd.program, cmd.args.join(" ")),
                stderr: false,
            },
        );
        let mut output = String::new();
        let code = forge_backend::process_io::run_lines(
            backend.as_ref(),
            &cmd,
            &SBT_CANCEL,
            &mut |line, stderr| {
                output.push_str(&line);
                output.push('\n');
                let _ = handle.emit(SBT_EVENT, Line { line, stderr });
            },
        )
        .map_err(|e| other(format!("cannot run sbt: {e}")))?;
        if code != Some(0) {
            let detail = output
                .lines()
                .rev()
                .find(|l| l.starts_with("[error]"))
                .unwrap_or_default()
                .to_string();
            return Err(other(format!(
                "sbt import failed{}{}",
                code.map(|c| format!(" (exit code {c})"))
                    .unwrap_or_default(),
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(": {detail}")
                }
            )));
        }
        let mut synced = sbt::parse_sync(&output, &root.display().to_string());
        synced.synced_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs());
        if let Some(cache) = sbt_cache(&root) {
            if let Some(dir) = cache.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(json) = serde_json::to_string(&synced) {
                let _ = std::fs::write(cache, json);
            }
        }
        Ok(sbt::merge(base, &synced))
    })
    .await
}

#[tauri::command]
pub fn scala_sbt_cancel() {
    SBT_CANCEL.store(true, Ordering::SeqCst);
}

// ---- Runs --------------------------------------------------------------------------------

/// ScalaTest and MUnit configurations.
pub(crate) fn handles_test(type_id: &str) -> bool {
    matches!(type_id, "scalatest" | "munit")
}

/// Fill a ScalaTest / MUnit configuration with sbt and the sbt project of its file.
pub(crate) fn prepare(state: &AppState, cfg: &mut RunConfiguration) -> Result<()> {
    let backend = state.backend()?;
    let sbt_path = find_sbt(state, backend.as_ref())
        .ok_or_else(|| other("ScalaTest and MUnit run through sbt, which was not found: install it or set it in Settings | Languages & Frameworks | Scala"))?;
    cfg.options
        .insert("buildProgram".into(), sbt_path.display().to_string());
    if cfg.option("project").trim().is_empty() {
        if let Some(build) = current_build(backend.as_ref()) {
            let dir = cfg.option("projectDir").trim().to_string();
            let file = cfg.option("file").trim().to_string();
            let found = build
                .projects
                .iter()
                .find(|p| !dir.is_empty() && p.dir == dir)
                .or_else(|| build.project_of_path(&file).filter(|_| !file.is_empty()))
                .filter(|p| !p.dir.is_empty());
            if let Some(p) = found {
                cfg.options.insert("project".into(), p.id.clone());
                cfg.options.insert("projectDir".into(), p.dir.clone());
            }
        }
    }
    let vm = java::setting(state, "scala.sbt.vmOptions");
    if !vm.is_empty() && cfg.option("sbtOptions").trim().is_empty() {
        let opts: Vec<String> = forge_process::split_args(&vm)
            .into_iter()
            .map(|o| {
                if o.starts_with("-J") {
                    o
                } else {
                    format!("-J{o}")
                }
            })
            .collect();
        cfg.options.insert("sbtOptions".into(), opts.join(" "));
    }
    if let Some(h) = project_java_home(state) {
        cfg.env.entry("JAVA_HOME".into()).or_insert(h);
    }
    Ok(())
}

/// The command line of an sbt Task configuration (`None` for other types): its tasks, or the
/// sbt shell without tasks.
pub(crate) fn command_line(
    state: &AppState,
    cfg: &RunConfiguration,
) -> Result<Option<CommandLine>> {
    if cfg.type_id != SBT_TASK {
        return Ok(None);
    }
    let backend = state.backend()?;
    let root = backend.info().root.clone();
    let program = match cfg.option("sbt").trim() {
        "" => find_sbt(state, backend.as_ref())
            .map(|p| p.display().to_string())
            .ok_or_else(|| other("sbt was not found: install it (sdk install sbt, or scala-sbt.org) or set it in Settings | Languages & Frameworks | Scala"))?,
        own => own.to_string(),
    };
    let mut vm = forge_process::split_args(&java::setting(state, "scala.sbt.vmOptions"));
    vm.extend(forge_process::split_args(&cfg.interpreter_options));
    let tasks = sbt::split_tasks(cfg.option("tasks"));
    let (program, args) = sbt::task_command(
        &program,
        cfg.option("project").trim(),
        &tasks,
        &vm,
        &forge_process::split_args(&cfg.arguments),
    );
    let mut cmd = CommandLine::new(program).args(args);
    cmd.cwd = match cfg.working_directory.trim() {
        "" => root,
        d if Path::new(d).is_absolute() => PathBuf::from(d),
        d => root.join(d),
    };
    if let Some(home) = java::jdk_home(state, cfg) {
        cmd.env.insert("JAVA_HOME".into(), home);
    }
    cmd.env.extend(cfg.env.clone());
    Ok(Some(cmd))
}
