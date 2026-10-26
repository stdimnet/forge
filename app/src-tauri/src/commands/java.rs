//! JavaForge IPC: JDKs, Eclipse JDT LS, Maven and Gradle, Java run configurations and the
//! Java debugger. The logic lives in `packs/javaforge` (`javaforge-jdk`,
//! `javaforge-build`, `javaforge-lang-java`) and `forge_langservers::managed::{jdtls,
//! javadebug}`; this module wires them to the project, the settings and the LSP manager.
//!
//! - JDKs: `java_jdks`, `java_jdk_inspect`, `java_jdk_releases`, `java_jdk_download`
//!   (progress on `java://install`).
//! - JDT LS: started with a JDK 21+ when a Java project is open and JDT LS is installed
//!   ([`sync_lsp_server`]); `java_status` tells the UI what is missing, `java_jdtls_install`
//!   installs JDT LS and java-debug with the user's consent, `java_reload_project` re-imports
//!   the Maven / Gradle build.
//! - Builds: `java_build_info` (tool, wrapper, modules, goals and tasks).
//! - Runs (`run.rs`): [`prepare`] fills JUnit / TestNG configurations with the build tool,
//!   [`command_line`] builds Application, Maven and Gradle runs, [`resolve_launch`] asks
//!   JDT LS for an application's class path.
//! - Debugging: `java_debug_prepare` starts java-debug's DAP server inside JDT LS and picks
//!   the JDWP port the run listens on.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_langservers::managed::{javadebug, jdtls, Event, Http};
use forge_lsp::LspServerConfig;
use forge_process::{CommandLine, RunConfiguration};
use javaforge_build::command::ExecOptions;
use javaforge_build::{BuildProject, BuildTool, Goal};
use javaforge_jdk::adoptium::{self, AvailableReleases, Platform};
use javaforge_jdk::Jdk;
use javaforge_lang_java::jdtls::{commands as jdt, config_folder, JdtlsSetup, Runtime};
use javaforge_lang_java::launch::{self, AppLaunch, DEBUG_PORT_ENV, GRADLE_DEBUG_PORT};
use javaforge_lang_java::MainMethod;
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::lsp::LspState;
use crate::commands::workspace::blocking;
use crate::state::AppState;

pub const PLUGIN_ID: &str = "forge.java";
/// The language server's id (also `forge_trust::SERVERS_RUNNING_PROJECT_CODE`'s).
pub const SERVER_ID: &str = "jdtls";
/// `{ id: "jdtls" | "jdk", event: managed::Event }` while something is installed.
pub const INSTALL_EVENT: &str = "java://install";
/// JDT LS needs this Java release to run.
const SERVER_MIN_MAJOR: u32 = 21;

/// Run configuration types JavaForge builds.
pub const APPLICATION: &str = "java-application";
pub const MAVEN: &str = "maven";
pub const GRADLE: &str = "gradle";
pub const REMOTE: &str = "jvm-remote-debug";
const TEST_TYPES: [&str; 2] = ["junit", "testng"];

/// Detected JDKs (detection reads many folders: cached until a refresh).
#[derive(Default)]
pub struct JavaState {
    jdks: Mutex<Option<Vec<Jdk>>>,
}

static JAVA_STATE: std::sync::OnceLock<JavaState> = std::sync::OnceLock::new();

/// The JDK cache, shared by commands and run configurations.
fn java_state() -> &'static JavaState {
    JAVA_STATE.get_or_init(JavaState::default)
}

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

// ---- JDKs ----------------------------------------------------------------------------------

/// Detected JDKs plus the ones added by hand (`java.jdks`).
fn all_jdks(state: &AppState, java: &JavaState, refresh: bool) -> Vec<Jdk> {
    let mut cache = java.jdks.lock().expect("jdks lock");
    if refresh || cache.is_none() {
        *cache = Some(javaforge_jdk::detect(&javaforge_jdk::DetectEnv::system()));
    }
    let mut out = cache.clone().unwrap_or_default();
    drop(cache);
    let added: Vec<String> = state
        .read_settings()
        .get_as::<Vec<String>>("java.jdks")
        .unwrap_or_default();
    for home in added {
        if out.iter().any(|j| j.home == home) {
            continue;
        }
        if let Some(jdk) = javaforge_jdk::inspect(Path::new(&home), "Added") {
            if !out.iter().any(|j| j.home == jdk.home) {
                out.push(jdk);
            }
        }
    }
    out.sort_by(|a, b| b.major.cmp(&a.major).then_with(|| a.home.cmp(&b.home)));
    out
}

/// The project JDK: `java.projectJdk` (home or name), else the newest JDK.
fn project_jdk(state: &AppState, jdks: &[Jdk]) -> Option<Jdk> {
    let chosen = setting(state, "java.projectJdk");
    javaforge_jdk::pick(jdks, Some(chosen.as_str()).filter(|c| !c.is_empty()), 0).cloned()
}

/// The JDK that runs JDT LS: `java.jdtls.jdk`, else the project JDK when it is 21+, else
/// the newest 21+.
fn server_jdk(state: &AppState, jdks: &[Jdk]) -> Option<Jdk> {
    let chosen = setting(state, "java.jdtls.jdk");
    if !chosen.is_empty() {
        return javaforge_jdk::inspect(Path::new(&chosen), "Added")
            .filter(|j| j.major >= SERVER_MIN_MAJOR);
    }
    let project = project_jdk(state, jdks).map(|j| j.home);
    javaforge_jdk::pick(jdks, project.as_deref(), SERVER_MIN_MAJOR).cloned()
}

#[tauri::command]
pub async fn java_jdks(refresh: Option<bool>, app: AppHandle) -> Result<Vec<Jdk>> {
    blocking(move || {
        let state = app.state::<AppState>();
        Ok(all_jdks(&state, java_state(), refresh.unwrap_or(false)))
    })
    .await
}

/// A folder the user picked (Add JDK from Disk…): the JDK in it, or `None`.
#[tauri::command]
pub fn java_jdk_inspect(path: String) -> Option<Jdk> {
    javaforge_jdk::inspect(Path::new(&path), "Added")
}

#[tauri::command]
pub async fn java_jdk_releases(state: State<'_, AppState>) -> Result<AvailableReleases> {
    let api = setting(&state, "java.download.adoptiumApi");
    let api = if api.is_empty() {
        adoptium::DEFAULT_API.to_string()
    } else {
        api
    };
    let http = Http::new().map_err(ForgeError::Other)?;
    adoptium::available_releases(&http, &api)
        .await
        .map_err(ForgeError::Other)
}

fn emitter(app: &AppHandle, id: &'static str) -> impl FnMut(Event) + Send {
    let app = app.clone();
    move |event: Event| {
        let _ = app.emit(INSTALL_EVENT, json!({ "id": id, "event": event }));
    }
}

/// Download Eclipse Temurin `feature` into `<data dir>/forge/jdks` (SHA-256 checked).
#[tauri::command]
pub async fn java_jdk_download(feature: u32, app: AppHandle) -> Result<Jdk> {
    let state = app.state::<AppState>();
    let api = setting(&state, "java.download.adoptiumApi");
    let api = if api.is_empty() {
        adoptium::DEFAULT_API.to_string()
    } else {
        api
    };
    let platform = Platform::current()
        .ok_or_else(|| ForgeError::Other("Temurin has no JDK builds for this platform".into()))?;
    let dir = javaforge_jdk::default_jdks_dir()
        .ok_or_else(|| ForgeError::Other("no user data directory for JDKs".into()))?;
    let http = Http::new().map_err(ForgeError::Other)?;
    let mut on = emitter(&app, "jdk");
    let jdk = adoptium::download(&http, &api, feature, &platform, &dir, &mut on)
        .await
        .map_err(ForgeError::Other)?;
    // The new JDK shows up in the next detection.
    *java_state().jdks.lock().expect("jdks lock") = None;
    let h = app.clone();
    tauri::async_runtime::spawn_blocking(move || sync_lsp_server(&h, None));
    Ok(jdk)
}

// ---- Eclipse JDT LS --------------------------------------------------------------------

fn installation(state: &AppState) -> Option<jdtls::Installation> {
    let custom = setting(state, "java.jdtls.path");
    if !custom.is_empty() {
        return jdtls::inspect(Path::new(&custom));
    }
    jdtls::installed(&jdtls::default_dir()?)
}

fn java_debug_jar() -> Option<PathBuf> {
    javadebug::installed(&javadebug::default_dir()?)
}

/// FNV-1a: a stable short hash that keeps the data of projects with the same name apart.
fn short_hash(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:08x}", h as u32)
}

fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("forge").join("jdtls"))
}

/// How JDT LS starts for the project at `root`, or why it cannot.
fn jdtls_setup(
    state: &AppState,
    jdks: &[Jdk],
    root: &Path,
) -> std::result::Result<JdtlsSetup, String> {
    let install = installation(state).ok_or("Eclipse JDT Language Server is not installed")?;
    let server = server_jdk(state, jdks).ok_or(
        "Eclipse JDT LS needs a JDK 21 or newer: download one (Download JDK…) or choose it in Settings | Languages & Frameworks | Java",
    )?;
    let cache = cache_dir().ok_or("no user cache directory for JDT LS")?;
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".into());
    let project = project_jdk(state, jdks);
    let gradle_jvm = setting(state, "java.gradle.jvm");
    let vm_options = forge_process::split_args(&setting(state, "java.jdtls.vmOptions"));
    let hcr = setting(state, "java.debug.hotCodeReplace");
    Ok(JdtlsSetup {
        java: javaforge_jdk::java_executable(&server.home, windows()),
        launcher_jar: install.launcher_jar.display().to_string(),
        shared_config: install
            .dir
            .join(config_folder(std::env::consts::OS, std::env::consts::ARCH))
            .display()
            .to_string(),
        config_dir: cache
            .join(format!("config-{}", install.version))
            .display()
            .to_string(),
        data_dir: cache
            .join("workspaces")
            .join(format!(
                "{name}-{}",
                short_hash(&root.display().to_string())
            ))
            .display()
            .to_string(),
        vm_options,
        bundles: java_debug_jar()
            .map(|j| vec![j.display().to_string()])
            .unwrap_or_default(),
        runtimes: jdks
            .iter()
            .filter(|j| j.major > 0)
            .map(|j| Runtime {
                major: j.major,
                path: j.home.clone(),
                default: project.as_ref().is_some_and(|p| p.home == j.home),
            })
            .collect(),
        use_wrapper: flag(state, "java.gradle.useWrapper", true),
        gradle_java_home: Some(gradle_jvm).filter(|g| !g.is_empty()),
        hot_code_replace: if hcr.is_empty() { "auto".into() } else { hcr },
        download_sources: flag(state, "java.maven.downloadSources", true),
        formatter_url: None,
    })
}

fn to_lsp_config(setup: &JdtlsSetup) -> LspServerConfig {
    LspServerConfig {
        id: SERVER_ID.into(),
        name: Some("Eclipse JDT LS".into()),
        language_ids: vec!["java".into()],
        file_extensions: vec!["java".into()],
        command: setup.java.clone(),
        args: setup.args(),
        initialization_options: setup.initialization_options(),
        settings: setup.settings(),
    }
}

/// Register (or remove) JDT LS for the open project: a local Java project with the Java
/// plugin enabled, JDT LS installed and a JDK 21+.
pub fn sync_lsp_server(app: &AppHandle, detected: Option<(&Path, &forge_project::ProjectInfo)>) {
    let state = app.state::<AppState>();
    let lsp = app.state::<LspState>();
    let Ok(backend) = state.backend() else {
        lsp.unregister_server(SERVER_ID);
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
    if !enabled || !info.uses("java") || backend.info().remote.is_some() {
        lsp.unregister_server(SERVER_ID);
        return;
    }
    let jdks = all_jdks(&state, java_state(), false);
    match jdtls_setup(&state, &jdks, &root) {
        Ok(setup) => {
            tracing::info!("Eclipse JDT LS with {}", setup.java);
            lsp.register_server(to_lsp_config(&setup));
        }
        Err(why) => {
            tracing::info!("Eclipse JDT LS not started: {why}");
            lsp.unregister_server(SERVER_ID);
        }
    }
}

/// Whether a changed setting reconfigures JDT LS.
pub fn affects_server(key: &str) -> bool {
    key.starts_with("java.jdtls.")
        || matches!(
            key,
            "java.jdks"
                | "java.projectJdk"
                | "java.gradle.jvm"
                | "java.gradle.useWrapper"
                | "java.debug.hotCodeReplace"
                | "java.maven.downloadSources"
        )
}

/// [`sync_lsp_server`] on a blocking thread.
pub fn spawn_sync_lsp_server(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || sync_lsp_server(&app, None));
}

/// What the UI needs to know about the Java setup.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaStatus {
    /// Version of the JDT LS installation in use.
    pub jdtls: Option<String>,
    pub jdtls_dir: Option<String>,
    /// The java-debug plugin jar.
    pub java_debug: Option<String>,
    pub server_jdk: Option<Jdk>,
    pub project_jdk: Option<Jdk>,
    /// The effective language level (`21`), from the setting, the build or the JDK.
    pub language_level: Option<String>,
    pub build: Option<BuildProject>,
    /// Why JDT LS does not run (empty when it can).
    pub problems: Vec<String>,
    /// JDT LS runs on remote projects only on the remote host (not yet supported).
    pub remote: bool,
}

/// The build in the project root (`None`: neither Maven nor Gradle).
fn read_build(state: &AppState, backend: &dyn Backend) -> Option<BuildProject> {
    let files: Vec<String> = backend
        .list_dir("")
        .ok()?
        .into_iter()
        .filter(|e| e.kind == forge_core::FileKind::File)
        .map(|e| e.name)
        .collect();
    let folder = backend
        .info()
        .root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let prefer = BuildTool::parse(&setting(state, "java.build.preferredTool"));
    javaforge_build::detect(
        &folder,
        &files,
        &|p| backend.read_optional(p).ok().flatten(),
        prefer,
    )
}

#[tauri::command]
pub async fn java_status(app: AppHandle) -> Result<JavaStatus> {
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let jdks = all_jdks(&state, java_state(), false);
        let install = installation(&state);
        let project = project_jdk(&state, &jdks);
        let build = read_build(&state, backend.as_ref());
        let level = Some(setting(&state, "java.languageLevel"))
            .filter(|l| !l.is_empty())
            .or_else(|| build.as_ref().and_then(|b| b.java_version.clone()))
            .or_else(|| project.as_ref().map(|p| p.major.to_string()));
        let problems = match jdtls_setup(&state, &jdks, &backend.info().root) {
            Ok(_) => Vec::new(),
            Err(e) => vec![e],
        };
        Ok(JavaStatus {
            jdtls: install.as_ref().map(|i| i.version.clone()),
            jdtls_dir: install.as_ref().map(|i| i.dir.display().to_string()),
            java_debug: java_debug_jar().map(|j| j.display().to_string()),
            server_jdk: server_jdk(&state, &jdks),
            project_jdk: project,
            language_level: level,
            build,
            problems,
            remote: backend.info().remote.is_some(),
        })
    })
    .await
}

/// Install Eclipse JDT LS (download.eclipse.org, SHA-256) and java-debug (Maven Central,
/// checksum), then start JDT LS. Returns the JDT LS version.
#[tauri::command]
pub async fn java_jdtls_install(app: AppHandle) -> Result<String> {
    let state = app.state::<AppState>();
    let site = match setting(&state, "java.jdtls.downloadSite") {
        s if s.is_empty() => jdtls::DEFAULT_MIRROR.to_string(),
        s => s,
    };
    let repository = match setting(&state, "java.debug.repository") {
        s if s.is_empty() => javadebug::DEFAULT_REPOSITORY.to_string(),
        s => s,
    };
    let channel = jdtls::Channel::parse(&setting(&state, "java.jdtls.channel"));
    let base = jdtls::default_dir()
        .ok_or_else(|| ForgeError::Other("no user data directory for JDT LS".into()))?;
    let debug_base = javadebug::default_dir()
        .ok_or_else(|| ForgeError::Other("no user data directory for java-debug".into()))?;
    let http = Http::new().map_err(ForgeError::Other)?;
    let mut on = emitter(&app, "jdtls");
    let install = jdtls::install(&http, &site, channel, &base, &mut on)
        .await
        .map_err(ForgeError::Other)?;
    if let Err(e) = javadebug::install(&http, &repository, &debug_base, &mut on).await {
        // Editing works without it; only debugging needs java-debug.
        on(Event::step(format!("java-debug was not installed: {e}")));
    }
    let h = app.clone();
    tauri::async_runtime::spawn_blocking(move || sync_lsp_server(&h, None))
        .await
        .map_err(|e| ForgeError::Other(e.to_string()))?;
    Ok(install.version)
}

// ---- Builds --------------------------------------------------------------------------------

/// A build as the Maven / Gradle tool window shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildView {
    pub project: BuildProject,
    pub goals: Vec<Goal>,
    /// The program runs use (`/p/gradlew`, `mvn`).
    pub program: String,
}

fn exec_options(state: &AppState, tool: BuildTool, root: &Path) -> ExecOptions {
    let (wrapper, exe, offline) = match tool {
        BuildTool::Gradle => (
            "java.gradle.useWrapper",
            "java.gradle.executable",
            "java.gradle.offline",
        ),
        BuildTool::Maven => (
            "java.maven.useWrapper",
            "java.maven.executable",
            "java.maven.offline",
        ),
    };
    ExecOptions {
        use_wrapper: flag(state, wrapper, true),
        executable: Some(setting(state, exe)).filter(|e| !e.is_empty()),
        build_dir: root.display().to_string(),
        windows: windows(),
        offline: flag(state, offline, false),
    }
}

#[tauri::command]
pub async fn java_build_info(state: State<'_, AppState>) -> Result<Option<BuildView>> {
    let backend = state.backend()?;
    let Some(project) = read_build(&state, backend.as_ref()) else {
        return Ok(None);
    };
    let opts = exec_options(&state, project.tool, &backend.info().root);
    Ok(Some(BuildView {
        goals: javaforge_build::goals(&project),
        program: javaforge_build::executable(project.tool, project.wrapper, &opts),
        project,
    }))
}

/// Reload All Maven / Gradle Projects: JDT LS re-imports the build files.
#[tauri::command]
pub async fn java_reload_project(app: AppHandle) -> Result<()> {
    let state = app.state::<AppState>();
    let backend = state.backend()?;
    let Some(build) = read_build(&state, backend.as_ref()) else {
        return Err(ForgeError::Other(
            "no Maven or Gradle build in the project root".into(),
        ));
    };
    let root = backend.info().root.clone();
    let manager = app.state::<LspState>().manager(&app, &state)?;
    let Some(client) = manager
        .running_clients()
        .into_iter()
        .find(|c| c.id() == SERVER_ID)
    else {
        return Err(ForgeError::Other(
            "Eclipse JDT LS is not running: open a Java file first".into(),
        ));
    };
    let mut files = vec![build.build_file.clone()];
    for m in &build.modules {
        let name = match build.tool {
            BuildTool::Maven => "pom.xml",
            BuildTool::Gradle if build.kotlin_dsl => "build.gradle.kts",
            BuildTool::Gradle => "build.gradle",
        };
        files.push(format!("{}/{name}", m.dir));
    }
    let identifiers: Vec<Value> = files
        .iter()
        .map(|f| root.join(f))
        .filter(|p| p.is_file())
        .filter_map(|p| forge_lsp::path_to_uri(&p).ok())
        .map(|u| json!({ "uri": u.to_string() }))
        .collect();
    client.notify_raw(
        "java/projectConfigurationsUpdate",
        json!({ "identifiers": identifiers }),
    )?;
    Ok(())
}

// ---- Run configurations ----------------------------------------------------------------

/// `main` methods of a file for gutter icons (pure; also in WebAssembly).
#[tauri::command]
pub fn java_main_methods(path: String, text: String) -> Vec<MainMethod> {
    javaforge_lang_java::main_methods(&path, &text)
}

/// Whether JavaForge builds configurations of `type_id`.
pub(crate) fn handles(type_id: &str) -> bool {
    matches!(type_id, APPLICATION | MAVEN | GRADLE | REMOTE) || TEST_TYPES.contains(&type_id)
}

/// The JDK home a configuration runs with: its own (`interpreter` / option `jdk`), else
/// the project JDK.
fn jdk_home(state: &AppState, cfg: &RunConfiguration) -> Option<String> {
    let own = [cfg.interpreter.trim(), cfg.option("jdk").trim()]
        .into_iter()
        .find(|s| !s.is_empty())
        .map(str::to_string);
    let jdks = all_jdks(state, java_state(), false);
    match own {
        Some(o) => javaforge_jdk::pick(&jdks, Some(&o), 0)
            .map(|j| j.home.clone())
            .or(Some(o)),
        None => project_jdk(state, &jdks).map(|j| j.home),
    }
}

/// Fill a JUnit / TestNG configuration with its build (tool, program, folders, module), and
/// add the JDWP arguments of a debug run (`FORGE_JAVA_DEBUG_PORT`).
pub(crate) fn prepare(
    state: &AppState,
    cfg: &mut RunConfiguration,
    debug_port: Option<u16>,
) -> Result<()> {
    if !TEST_TYPES.contains(&cfg.type_id.as_str()) {
        return Ok(());
    }
    let backend = state.backend()?;
    let root = backend.info().root.clone();
    let build = read_build(state, backend.as_ref()).ok_or_else(|| {
        ForgeError::Other(
            "Tests run through Gradle or Maven: no build.gradle or pom.xml in the project root"
                .into(),
        )
    })?;
    let opts = exec_options(state, build.tool, &root);
    let program = javaforge_build::executable(build.tool, build.wrapper, &opts);
    let module = cfg.option("module").trim().to_string();
    let gradle_path = build
        .module(&module)
        .map(|m| m.path.clone())
        .unwrap_or_default();
    let set = |cfg: &mut RunConfiguration, k: &str, v: String| {
        cfg.options.insert(k.into(), v);
    };
    set(cfg, "buildTool", build.tool.id().into());
    set(cfg, "buildProgram", program);
    set(cfg, "buildDir", root.display().to_string());
    set(cfg, "buildRoot", String::new());
    set(cfg, "gradlePath", gradle_path);
    set(cfg, "offline", opts.offline.to_string());
    if let Some(port) = debug_port {
        let extra = launch::build_tool_debug_args(build.tool.id(), port).join(" ");
        let joined = format!("{} {extra}", cfg.option("testRunnerOptions"))
            .trim()
            .to_string();
        set(cfg, "testRunnerOptions", joined);
    }
    // Gradle and Maven run with the project JDK.
    if let Some(home) = jdk_home(state, cfg) {
        cfg.env.entry("JAVA_HOME".into()).or_insert(home);
    }
    Ok(())
}

fn split_lines(s: &str) -> Vec<String> {
    s.split(['\n', if windows() { ';' } else { ':' }])
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// The command line of an Application, Maven or Gradle configuration (`None` for other
/// types).
pub(crate) fn command_line(
    state: &AppState,
    cfg: &RunConfiguration,
    debug_port: Option<u16>,
) -> Result<Option<CommandLine>> {
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
    let mut cmd = match cfg.type_id.as_str() {
        APPLICATION => {
            let home = jdk_home(state, cfg);
            let classpath = match cfg.option("classpath").trim() {
                "" => match cfg.option("resolvedClasspath").trim() {
                    "" => {
                        let tool = read_build(state, backend.as_ref())
                            .map(|b| b.tool.id())
                            .unwrap_or("gradle");
                        launch::build_output_classpath(tool, cfg.option("module"))
                    }
                    resolved => split_lines(resolved),
                },
                own => split_lines(own),
            };
            let l = AppLaunch {
                java: home
                    .map(|h| javaforge_jdk::java_executable(&h, windows()))
                    .unwrap_or_else(|| "java".into()),
                vm_options: cfg.interpreter_options.clone(),
                main_class: cfg.option("mainClass").to_string(),
                module: Some(cfg.option("javaModule").to_string()).filter(|m| !m.trim().is_empty()),
                classpath: classpath.iter().map(|p| abs(p)).collect(),
                modulepath: split_lines(cfg.option("resolvedModulepath"))
                    .iter()
                    .map(|p| abs(p))
                    .collect(),
                arguments: cfg.arguments.clone(),
                windows: windows(),
                debug_port,
            };
            let (program, args) = launch::application_command(&l).map_err(ForgeError::Other)?;
            CommandLine::new(program).args(args)
        }
        MAVEN | GRADLE => {
            let build = read_build(state, backend.as_ref());
            let tool = if cfg.type_id == MAVEN {
                BuildTool::Maven
            } else {
                BuildTool::Gradle
            };
            let mut project = build
                .filter(|b| b.tool == tool)
                .unwrap_or_else(|| BuildProject::minimal(tool, Vec::new()));
            let opts = exec_options(state, tool, &root);
            if !opts.use_wrapper {
                project.wrapper = false;
            }
            let goals = forge_process::split_args(cfg.option(if tool == BuildTool::Maven {
                "goals"
            } else {
                "tasks"
            }));
            if goals.is_empty() {
                return Err(ForgeError::Other(format!(
                    "No {} to run",
                    if tool == BuildTool::Maven {
                        "goals"
                    } else {
                        "tasks"
                    }
                )));
            }
            let module = project.module(cfg.option("module")).cloned();
            let mut extra = forge_process::split_args(&cfg.arguments);
            if let Some(port) = debug_port {
                extra.extend(launch::build_tool_debug_args(tool.id(), port));
            }
            let built =
                javaforge_build::goals_command(&project, &opts, module.as_ref(), &goals, &extra);
            CommandLine::new(built.program).args(built.args)
        }
        REMOTE => {
            return Err(ForgeError::Other(
                "Remote JVM Debug configurations attach the debugger; start them with Debug".into(),
            ))
        }
        _ => return Ok(None),
    };
    cmd.cwd = cwd;
    if cfg.type_id != APPLICATION {
        if let Some(home) = jdk_home(state, cfg) {
            cmd.env.insert("JAVA_HOME".into(), home);
        }
    }
    cmd.env.extend(cfg.env.clone());
    Ok(Some(cmd))
}

/// The debug port a run request asks for (`FORGE_JAVA_DEBUG_PORT`), removed from its
/// environment.
pub(crate) fn take_debug_port(env: &mut std::collections::BTreeMap<String, String>) -> Option<u16> {
    env.remove(DEBUG_PORT_ENV)
        .and_then(|p| p.trim().parse().ok())
}

/// A JDT LS command on the project's server (started when needed).
async fn execute(app: &AppHandle, command: &str, arguments: Vec<Value>) -> Result<Option<Value>> {
    let state = app.state::<AppState>();
    let root = state.project_root()?;
    let manager = app.state::<LspState>().manager(app, &state)?;
    // Any Java path routes to JDT LS.
    manager
        .execute_command(&root.join("Main.java"), SERVER_ID, command, arguments)
        .await
}

/// Application runs without an explicit class path: compile with JDT LS and ask java-debug
/// for the class and module path of the main class.
pub(crate) async fn resolve_launch(app: &AppHandle, cfg: &mut RunConfiguration) {
    if cfg.type_id != APPLICATION || !cfg.option("classpath").trim().is_empty() {
        return;
    }
    let state = app.state::<AppState>();
    if state
        .backend()
        .map(|b| b.info().remote.is_some())
        .unwrap_or(true)
    {
        return;
    }
    let running = app
        .state::<LspState>()
        .manager(app, &state)
        .map(|m| m.running_clients().iter().any(|c| c.id() == SERVER_ID))
        .unwrap_or(false);
    if !running || java_debug_jar().is_none() {
        return;
    }
    let main = cfg.option("mainClass").to_string();
    if let Ok(manager) = app.state::<LspState>().manager(app, &state) {
        if let Some(client) = manager
            .running_clients()
            .into_iter()
            .find(|c| c.id() == SERVER_ID)
        {
            let _ = client
                .request_raw(
                    jdt::BUILD_WORKSPACE,
                    json!(false),
                    std::time::Duration::from_secs(120),
                )
                .await;
        }
    }
    match execute(app, jdt::RESOLVE_CLASSPATH, vec![json!(main), Value::Null]).await {
        Ok(Some(Value::Array(paths))) => {
            let join = |v: Option<&Value>| -> String {
                v.and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default()
            };
            cfg.options
                .insert("resolvedModulepath".into(), join(paths.first()));
            cfg.options
                .insert("resolvedClasspath".into(), join(paths.get(1)));
        }
        Ok(_) => {}
        Err(e) => tracing::info!("java-debug could not resolve the class path of {main}: {e}"),
    }
}

// ---- Debugging -----------------------------------------------------------------------------

/// What a Java debug session needs: java-debug's DAP port and the JVM's JDWP address.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugPlan {
    pub dap_port: u16,
    pub host_name: String,
    pub jdwp_port: u16,
    /// The run must be started with `FORGE_JAVA_DEBUG_PORT` (not for Remote JVM Debug).
    pub run: bool,
}

fn free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| ForgeError::Other(format!("no free port: {e}")))?;
    Ok(listener
        .local_addr()
        .map_err(|e| ForgeError::Other(e.to_string()))?
        .port())
}

#[tauri::command]
pub async fn java_debug_prepare(
    configuration: RunConfiguration,
    app: AppHandle,
) -> Result<DebugPlan> {
    if java_debug_jar().is_none() {
        return Err(ForgeError::Other(
            "The Java debugger (java-debug) is not installed: run Install Eclipse JDT Language Server".into(),
        ));
    }
    let state = app.state::<AppState>();
    let hcr = match setting(&state, "java.debug.hotCodeReplace") {
        s if s.is_empty() => "auto".to_string(),
        s => s,
    };
    let settings =
        json!({ "hotCodeReplace": hcr, "showStaticVariables": false, "showQualifiedNames": false })
            .to_string();
    let _ = execute(&app, jdt::UPDATE_DEBUG_SETTINGS, vec![json!(settings)]).await;
    let dap = execute(&app, jdt::START_DEBUG_SESSION, vec![])
        .await?
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            ForgeError::Other(
                "Eclipse JDT LS did not start a debug session: is java-debug loaded?".into(),
            )
        })? as u16;
    let (host, port, run) = match configuration.type_id.as_str() {
        REMOTE => {
            let host = match configuration.option("host").trim() {
                "" => "localhost".to_string(),
                h => h.to_string(),
            };
            let port = configuration
                .option("port")
                .trim()
                .parse::<u16>()
                .map_err(|_| ForgeError::Other("Port is not a number".into()))?;
            (host, port, false)
        }
        GRADLE => ("127.0.0.1".to_string(), GRADLE_DEBUG_PORT, true),
        t if TEST_TYPES.contains(&t) => {
            let backend = state.backend()?;
            let gradle =
                read_build(&state, backend.as_ref()).is_none_or(|b| b.tool == BuildTool::Gradle);
            let port = if gradle {
                GRADLE_DEBUG_PORT
            } else {
                free_port()?
            };
            ("127.0.0.1".to_string(), port, true)
        }
        _ => ("127.0.0.1".to_string(), free_port()?, true),
    };
    Ok(DebugPlan {
        dap_port: dap,
        host_name: host,
        jdwp_port: port,
        run,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handled_types_and_settings() {
        for t in [APPLICATION, MAVEN, GRADLE, REMOTE, "junit", "testng"] {
            assert!(handles(t), "{t}");
        }
        assert!(!handles("phpunit"));
        assert!(affects_server("java.jdtls.vmOptions"));
        assert!(affects_server("java.projectJdk"));
        assert!(!affects_server("java.maven.offline"));
        assert_eq!(short_hash("/p/shop"), short_hash("/p/shop"));
        assert_ne!(short_hash("/p/shop"), short_hash("/q/shop"));
        let mut env =
            std::collections::BTreeMap::from([(DEBUG_PORT_ENV.to_string(), "5005".to_string())]);
        assert_eq!(take_debug_port(&mut env), Some(5005));
        assert!(env.is_empty());
        assert!(free_port().unwrap() > 0);
    }
}
