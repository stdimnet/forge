//! CForge (`packs/cforge/*`): toolchains, CMake profiles and presets, the CMake model of the
//! active profile (cmake-file-api), build commands and build problems, CMake Application runs,
//! the C++ test context (GoogleTest, Catch2, CTest) and their coverage, debug launch requests
//! with STL pretty printers, clangd (with the profile's compilation database) and
//! clang-format.
//!
//! Configuring runs CMake, which runs project code: `cmake_reload` calls `require_trusted`.
//! Builds, runs and tests start through `run_start`, which refuses them in Safe Mode; the
//! debug launch is refused by `dap_start`. A model that a trusted configure wrote earlier is
//! read in Safe Mode too (`cmake_state`), without running anything.
//!
//! Events: `cmake://log` (`{ line, stream }` while CMake configures), `cmake://state` (the
//! new [`CMakeState`]), `cpp://install` (`{ message }` or `{ label, done, total }` while
//! clangd downloads).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use cforge_cmake::complete::Completion;
use cforge_cmake::presets::{self, MacroContext, Presets};
use cforge_cmake::{
    BuildAction, CMakeModel, ConfigureContext, ConfigureMessage, Profile, ProfileEntry,
};
use cforge_lang_cpp::compdb;
use cforge_lang_cpp::diagnostics::{self, BuildProblem, Severity};
use cforge_lang_cpp::tools::{self, ClangdOptions};
use cforge_lang_cpp::{ContextTarget, CppContext};
use cforge_toolchains::{SystemProbe, Toolchain, ToolchainEntry};
use forge_backend::process_io::capture;
use forge_backend::{Backend, Process};
use forge_core::{ForgeError, Result};
use forge_dap::LaunchRequest;
use forge_langservers::managed::clangd as managed;
use forge_langservers::managed::{Event, Http};
use forge_lsp::LspServerConfig;
use forge_process::{CommandLine, OutputStream, RunConfiguration, RunEvent, RunSink};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::lsp::LspState;
use crate::commands::workspace::blocking;
use crate::state::AppState;

pub const PLUGIN: &str = "forge.cpp";
const SERVER_ID: &str = "clangd";
const FORMAT_TIMEOUT: Duration = Duration::from_secs(20);
const CONFIGURE_TIMEOUT: Duration = Duration::from_secs(1800);
const COVERAGE_TIMEOUT: Duration = Duration::from_secs(300);

pub(crate) fn setting(state: &AppState, key: &str) -> Option<String> {
    state
        .read_settings()
        .get_as::<String>(key)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub(crate) fn flag(state: &AppState, key: &str, default: bool) -> bool {
    state.read_settings().get_as::<bool>(key).unwrap_or(default)
}

pub(crate) fn is_local(state: &AppState) -> bool {
    state
        .backend()
        .map(|b| b.info().remote.is_none())
        .unwrap_or(true)
}

fn enabled(state: &AppState) -> bool {
    state
        .plugins
        .read()
        .expect("plugins lock poisoned")
        .is_enabled(PLUGIN)
}

pub(crate) fn path_string(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

// ---- Toolchains -----------------------------------------------------------------------------

/// When the toolchains were detected, the entries they were built from, and the toolchains.
type ToolchainCache = (Instant, Vec<ToolchainEntry>, Vec<Toolchain>);

/// Detected toolchains, looked up again after a minute.
static TOOLCHAINS: Mutex<Option<ToolchainCache>> = Mutex::new(None);

fn entries(state: &AppState) -> Vec<ToolchainEntry> {
    state
        .read_settings()
        .get_as::<Vec<ToolchainEntry>>("cpp.toolchains")
        .unwrap_or_default()
}

/// The toolchains of this machine: the user's (`cpp.toolchains`) first, then the detected
/// ones. Remote projects have none here (their compilers live on the remote host).
pub(crate) fn toolchains(state: &AppState, refresh: bool) -> Vec<Toolchain> {
    if !is_local(state) {
        return Vec::new();
    }
    let entries = entries(state);
    let mut cached = TOOLCHAINS.lock().expect("toolchain cache");
    if let Some((at, e, list)) = cached.as_ref() {
        if !refresh && *e == entries && at.elapsed() < Duration::from_secs(60) {
            return list.clone();
        }
    }
    let list = cforge_toolchains::all(&SystemProbe, &entries);
    *cached = Some((Instant::now(), entries, list.clone()));
    list
}

/// The toolchain of a profile (its id or name), else the default one.
pub(crate) fn toolchain_of(state: &AppState, profile: Option<&Profile>) -> Option<Toolchain> {
    let list = toolchains(state, false);
    let wanted = profile.map(|p| p.toolchain.as_str()).unwrap_or("");
    if !wanted.is_empty() {
        if let Some(t) = list.iter().find(|t| t.id == wanted || t.name == wanted) {
            return Some(t.clone());
        }
    }
    list.into_iter().next()
}

#[tauri::command]
pub async fn cpp_toolchains(refresh: Option<bool>, app: AppHandle) -> Result<Vec<Toolchain>> {
    let refresh = refresh.unwrap_or(false);
    blocking(move || Ok(toolchains(&app.state::<AppState>(), refresh))).await
}

/// Inspect a toolchain the user is editing (the Toolchains page's live check).
#[tauri::command]
pub async fn cpp_toolchain_inspect(entry: ToolchainEntry) -> Result<Toolchain> {
    blocking(move || Ok(cforge_toolchains::from_entry(&SystemProbe, &entry))).await
}

// ---- Profiles ---------------------------------------------------------------------------------

fn macro_context(root: &Path) -> MacroContext {
    MacroContext {
        source_dir: path_string(root),
        host_system_name: MacroContext::host().into(),
        env: std::env::vars().collect(),
    }
}

fn load_presets(backend: &dyn Backend) -> Presets {
    presets::load(&|p| backend.read_optional(p).ok().flatten())
}

/// The profiles of the project (settings and presets).
pub(crate) fn profiles(state: &AppState) -> Vec<Profile> {
    let Ok(backend) = state.backend() else {
        return vec![Profile::debug()];
    };
    let entries: Vec<ProfileEntry> = state
        .read_settings()
        .get_as::<Vec<ProfileEntry>>("cmake.profiles")
        .unwrap_or_default();
    let presets = load_presets(backend.as_ref());
    cforge_cmake::profiles(&entries, &presets, &macro_context(&backend.info().root))
}

/// The active profile: `cmake.activeProfile`, else the first enabled one.
pub(crate) fn active_profile(state: &AppState) -> Profile {
    let list = profiles(state);
    let wanted = setting(state, "cmake.activeProfile").unwrap_or_default();
    list.iter()
        .find(|p| p.name == wanted && p.enabled)
        .or_else(|| list.iter().find(|p| p.enabled))
        .cloned()
        .unwrap_or_else(Profile::debug)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilesInfo {
    pub profiles: Vec<Profile>,
    pub active: String,
    /// `CMakePresets.json` and the files it includes.
    pub preset_files: Vec<String>,
    pub preset_errors: Vec<String>,
}

#[tauri::command]
pub async fn cmake_profiles(app: AppHandle) -> Result<ProfilesInfo> {
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let presets = load_presets(backend.as_ref());
        Ok(ProfilesInfo {
            profiles: profiles(&state),
            active: active_profile(&state).name,
            preset_files: presets.files,
            preset_errors: presets.errors,
        })
    })
    .await
}

// ---- The CMake model ------------------------------------------------------------------------

/// What the CMake tool window shows.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CMakeState {
    /// The project root the state belongs to.
    pub root: String,
    pub profile: String,
    /// Absolute build folder of the profile.
    pub build_dir: String,
    /// `noCMake`, `notLoaded`, `loading`, `loaded` or `failed`.
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<CMakeModel>,
    pub messages: Vec<ConfigureMessage>,
    /// The configure output.
    pub log: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub command_line: String,
}

static CMAKE: Mutex<Option<CMakeState>> = Mutex::new(None);
static CONFIGURE: Mutex<Option<Arc<dyn Process>>> = Mutex::new(None);

pub(crate) fn build_dir(root: &Path, profile: &Profile) -> PathBuf {
    PathBuf::from(cforge_cmake::build_dir_of(profile, &path_string(root)))
}

/// A path below the project as a backend path, else `None`.
pub(crate) fn project_relative(root: &Path, abs: &Path) -> Option<String> {
    abs.strip_prefix(root)
        .ok()
        .map(|r| r.to_string_lossy().replace('\\', "/"))
}

pub(crate) fn read_text(backend: &dyn Backend, root: &Path, abs: &Path) -> Option<String> {
    match project_relative(root, abs) {
        Some(rel) => backend.read_optional(&rel).ok().flatten(),
        None if backend.info().remote.is_none() => std::fs::read_to_string(abs).ok(),
        None => None,
    }
}

fn list_names(backend: &dyn Backend, root: &Path, abs: &Path) -> Vec<String> {
    match project_relative(root, abs) {
        Some(rel) => backend
            .list_dir(&rel)
            .map(|e| e.into_iter().map(|e| e.name).collect())
            .unwrap_or_default(),
        None if backend.info().remote.is_none() => std::fs::read_dir(abs)
            .map(|it| {
                it.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default(),
        None => Vec::new(),
    }
}

fn write_text(backend: &dyn Backend, root: &Path, abs: &Path, text: &str) -> Result<()> {
    match project_relative(root, abs) {
        Some(rel) => {
            if let Some((dir, _)) = rel.rsplit_once('/') {
                let _ = create_dirs(backend, dir);
            }
            backend.write_file(&rel, text)
        }
        None => {
            if let Some(dir) = abs.parent() {
                std::fs::create_dir_all(dir).map_err(|e| ForgeError::io(dir, e))?;
            }
            std::fs::write(abs, text).map_err(|e| ForgeError::io(abs, e))
        }
    }
}

fn create_dirs(backend: &dyn Backend, rel: &str) -> Result<()> {
    let mut cur = String::new();
    for part in rel.split('/').filter(|p| !p.is_empty()) {
        if !cur.is_empty() {
            cur.push('/');
        }
        cur.push_str(part);
        let _ = backend.create_dir(&cur);
    }
    Ok(())
}

/// Read the file API reply of a build folder.
fn read_model(
    backend: &dyn Backend,
    root: &Path,
    build: &Path,
) -> std::result::Result<CMakeModel, String> {
    let reply = build.join(cforge_cmake::fileapi::REPLY_DIR);
    let files = list_names(backend, root, &reply);
    cforge_cmake::fileapi::parse_reply(&files, &|name| read_text(backend, root, &reply.join(name)))
}

pub(crate) fn has_cmake(backend: &dyn Backend) -> bool {
    backend
        .read_optional("CMakeLists.txt")
        .ok()
        .flatten()
        .is_some()
}

/// The current state; a model a configure wrote earlier is read without running CMake.
pub(crate) fn current_state(state: &AppState) -> Result<CMakeState> {
    let backend = state.backend()?;
    let root = backend.info().root.clone();
    let profile = active_profile(state);
    let build = build_dir(&root, &profile);
    {
        let cached = CMAKE.lock().expect("cmake state");
        if let Some(s) = cached.as_ref() {
            if s.root == path_string(&root)
                && s.profile == profile.name
                && s.build_dir == path_string(&build)
            {
                return Ok(s.clone());
            }
        }
    }
    let mut s = CMakeState {
        root: path_string(&root),
        profile: profile.name.clone(),
        build_dir: path_string(&build),
        status: "notLoaded".into(),
        ..CMakeState::default()
    };
    if !has_cmake(backend.as_ref()) {
        s.status = "noCMake".into();
    } else if let Ok(model) = read_model(backend.as_ref(), &root, &build) {
        s.model = Some(model);
        s.status = "loaded".into();
    }
    *CMAKE.lock().expect("cmake state") = Some(s.clone());
    Ok(s)
}

#[tauri::command]
pub async fn cmake_state(app: AppHandle) -> Result<CMakeState> {
    blocking(move || current_state(&app.state::<AppState>())).await
}

fn cmake_executable(state: &AppState, toolchain: Option<&Toolchain>) -> String {
    setting(state, "cmake.executable")
        .or_else(|| {
            toolchain
                .and_then(|t| t.cmake.as_ref())
                .map(|t| t.path.clone())
        })
        .unwrap_or_else(|| "cmake".into())
}

pub(crate) fn to_command_line(c: cforge_cmake::Command) -> CommandLine {
    let mut cmd = CommandLine::new(c.program).args(c.args);
    cmd.cwd = PathBuf::from(c.cwd);
    cmd.env = c.env;
    cmd
}

/// The configure command of the active profile.
fn configure_command(state: &AppState, root: &Path, profile: &Profile) -> CommandLine {
    let toolchain = toolchain_of(state, Some(profile));
    let generator = toolchain
        .as_ref()
        .map(|t| t.default_generator().to_string())
        .unwrap_or_default();
    let generator_used = if profile.generator.is_empty() {
        generator.clone()
    } else {
        profile.generator.clone()
    };
    let mut vars = toolchain
        .as_ref()
        .map(|t| cforge_toolchains::cmake_variables(t, &generator_used))
        .unwrap_or_default();
    vars.extend(profile.cache_variables.clone());
    if profile.preset.is_some() {
        // The preset decides compilers; Forge only adds what the preset has not set.
        vars.retain(|k, _| !profile.cache_variables.contains_key(k));
        vars.clear();
    }
    let ctx = ConfigureContext {
        cmake: cmake_executable(state, toolchain.as_ref()),
        source_dir: path_string(root),
        default_generator: generator,
        toolchain_variables: vars,
        package_toolchain: super::cpp_tools::package_toolchain(state, root, profile),
    };
    let mut cmd = to_command_line(cforge_cmake::configure_command(profile, &ctx));
    if profile.preset.is_some() {
        for (k, v) in &profile.cache_variables {
            if k != "CMAKE_BUILD_TYPE" {
                continue;
            }
            cmd.args.push(format!("-D{k}={v}"));
        }
        cmd.args.push("-DCMAKE_EXPORT_COMPILE_COMMANDS=ON".into());
    }
    cmd
}

fn emit_state(app: &AppHandle, s: &CMakeState) {
    let _ = app.emit("cmake://state", s);
}

/// Configure the active profile: write the file API query, run CMake (output on
/// `cmake://log`), read the reply. `resetCache` deletes `CMakeCache.txt` and `CMakeFiles`
/// first. Runs project code, so it needs a trusted project.
#[tauri::command]
pub async fn cmake_reload(
    reset_cache: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CMakeState> {
    state.require_trusted("Running CMake")?;
    let reset = reset_cache.unwrap_or(false);
    let handle = app.clone();
    let result = blocking(move || configure(&handle, &handle.state::<AppState>(), reset)).await;
    if result.is_ok() {
        // clangd reads the new compilation database.
        restart_lsp_server(&app);
    }
    result
}

fn configure(app: &AppHandle, state: &AppState, reset: bool) -> Result<CMakeState> {
    let backend = state.backend()?;
    let root = backend.info().root.clone();
    if !has_cmake(backend.as_ref()) {
        return Err(ForgeError::Other(
            "The project has no CMakeLists.txt".into(),
        ));
    }
    let profile = active_profile(state);
    let build = build_dir(&root, &profile);
    if reset {
        for name in ["CMakeCache.txt", "CMakeFiles"] {
            let p = build.join(name);
            match project_relative(&root, &p) {
                Some(rel) => {
                    let _ = backend.delete(&rel);
                }
                None => {
                    let _ = std::fs::remove_file(&p);
                    let _ = std::fs::remove_dir_all(&p);
                }
            }
        }
    }
    write_text(
        backend.as_ref(),
        &root,
        &build
            .join(cforge_cmake::fileapi::QUERY_DIR)
            .join("query.json"),
        &cforge_cmake::fileapi::query_json(),
    )?;
    let cmd = configure_command(state, &root, &profile);
    let mut s = CMakeState {
        root: path_string(&root),
        profile: profile.name.clone(),
        build_dir: path_string(&build),
        status: "loading".into(),
        command_line: cmd.display(),
        ..CMakeState::default()
    };
    emit_state(app, &s);
    let _ = app.emit(
        "cmake://log",
        json!({ "line": cmd.display(), "stream": "system" }),
    );

    let (tx, rx) = mpsc::channel::<Option<i32>>();
    let lines: Arc<Mutex<(Vec<String>, String, String)>> = Arc::default();
    let collected = lines.clone();
    let emitter = app.clone();
    let sink: RunSink = Arc::new(move |event| match event {
        RunEvent::Output { stream, text } => {
            let mut guard = collected.lock().expect("configure output");
            let (log, out, err) = &mut *guard;
            // Split each stream into lines as they arrive.
            let (buf, name) = match stream {
                OutputStream::Stderr => (err, "stderr"),
                _ => (out, "stdout"),
            };
            buf.push_str(&text);
            while let Some(i) = buf.find('\n') {
                let line: String = buf.drain(..=i).collect();
                let line = line.trim_end_matches(['\n', '\r']).to_string();
                let _ = emitter.emit("cmake://log", json!({ "line": line, "stream": name }));
                log.push(line);
            }
        }
        RunEvent::Exited { code, .. } => {
            let _ = tx.send(code);
        }
        RunEvent::Started { .. } => {}
    });
    let process = backend.start_process(&cmd, sink)?;
    *CONFIGURE.lock().expect("configure process") = Some(process.clone());
    let code = match rx.recv_timeout(CONFIGURE_TIMEOUT) {
        Ok(code) => code,
        Err(_) => {
            process.kill(Duration::from_millis(500));
            None
        }
    };
    *CONFIGURE.lock().expect("configure process") = None;
    let mut l = lines.lock().expect("configure output");
    let rest: Vec<String> = [std::mem::take(&mut l.1), std::mem::take(&mut l.2)]
        .into_iter()
        .filter(|r| !r.is_empty())
        .collect();
    l.0.extend(rest);
    s.log = l.0.clone();
    drop(l);
    let text = s.log.join("\n");
    s.messages = cforge_cmake::parse_log(&text);
    let ok = code == Some(0) && !cforge_cmake::log::failed(&text);
    match read_model(backend.as_ref(), &root, &build) {
        Ok(model) if ok => {
            s.model = Some(model);
            s.status = "loaded".into();
        }
        Ok(model) => {
            s.model = Some(model);
            s.status = "failed".into();
        }
        Err(e) => {
            if ok {
                s.log.push(e);
            }
            s.status = "failed".into();
        }
    }
    *CMAKE.lock().expect("cmake state") = Some(s.clone());
    emit_state(app, &s);
    Ok(s)
}

/// Stop a running configure.
#[tauri::command]
pub fn cmake_cancel() {
    if let Some(p) = CONFIGURE.lock().expect("configure process").take() {
        p.kill(Duration::from_secs(2));
    }
}

/// Forget the model (another profile became active, the settings changed).
pub(crate) fn invalidate() {
    *CMAKE.lock().expect("cmake state") = None;
}

/// Whether a changed setting changes the CMake model or the configure command.
pub fn affects_model(key: &str) -> bool {
    key.starts_with("cmake.")
        || key == "cpp.toolchains"
        || key.starts_with("cpp.vcpkg.")
        || key.starts_with("cpp.conan.")
}

// ---- Build --------------------------------------------------------------------------------

/// The `cmake --build` command of an action (started by the UI through `run_start`).
#[tauri::command]
pub async fn cmake_build_command(
    action: BuildAction,
    targets: Option<Vec<String>>,
    app: AppHandle,
) -> Result<CommandLine> {
    blocking(move || {
        let state = app.state::<AppState>();
        let root = state.project_root()?;
        let profile = active_profile(&state);
        let toolchain = toolchain_of(&state, Some(&profile));
        let multi = current_state(&state)
            .ok()
            .and_then(|s| s.model.map(|m| m.multi_config))
            .unwrap_or(false);
        Ok(to_command_line(cforge_cmake::build_command(
            &profile,
            &cmake_executable(&state, toolchain.as_ref()),
            &path_string(&root),
            &targets.unwrap_or_default(),
            action,
            multi,
        )))
    })
    .await
}

/// A build problem with its project path.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildProblemView {
    /// Project-relative (or absolute outside the project); `None` without a file.
    pub path: Option<String>,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub tool: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<BuildProblemView>,
}

fn view(p: BuildProblem, resolve: &dyn Fn(&str) -> Option<String>) -> BuildProblemView {
    BuildProblemView {
        path: resolve(&p.file),
        notes: p.notes.into_iter().map(|n| view(n, resolve)).collect(),
        file: p.file,
        line: p.line,
        column: p.column,
        severity: p.severity,
        message: p.message,
        code: p.code,
        tool: p.tool,
    }
}

/// The problems of a build's output, with project paths.
#[tauri::command]
pub async fn cpp_build_problems(output: String, app: AppHandle) -> Result<Vec<BuildProblemView>> {
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let root = backend.info().root.clone();
        let build = build_dir(&root, &active_profile(&state));
        let build_rel = project_relative(&root, &build).unwrap_or_default();
        let exists = |p: &str| backend.read_optional(p).ok().flatten().is_some();
        let root_s = path_string(&root);
        let resolve = |f: &str| diagnostics::project_path(f, &root_s, &build_rel, &exists);
        Ok(diagnostics::parse_build_output(&output)
            .into_iter()
            .map(|p| view(p, &resolve))
            .collect())
    })
    .await
}

// ---- Runs and tests ---------------------------------------------------------------------------

/// The project's C++ context for the test providers and runs.
pub(crate) fn context(state: &AppState) -> CppContext {
    let s = current_state(state).unwrap_or_default();
    let profile = active_profile(state);
    let toolchain = toolchain_of(state, Some(&profile));
    let cmake = cmake_executable(state, toolchain.as_ref());
    let mut tools = BTreeMap::new();
    if let Some(cxx) = toolchain.as_ref().and_then(|t| t.cxx_compiler.as_deref()) {
        tools.insert(
            "gcov".into(),
            cforge_lang_cpp::coverage::tool_next_to(cxx, "gcov"),
        );
        tools.insert(
            "llvm-profdata".into(),
            cforge_lang_cpp::coverage::tool_next_to(cxx, "llvm-profdata"),
        );
        tools.insert(
            "llvm-cov".into(),
            cforge_lang_cpp::coverage::tool_next_to(cxx, "llvm-cov"),
        );
    }
    let targets = s
        .model
        .as_ref()
        .map(|m| {
            m.executables(&profile.build_type)
                .into_iter()
                .filter_map(|t| {
                    Some(ContextTarget {
                        name: t.name.clone(),
                        executable: t.executable()?.to_string(),
                        sources: t.sources.iter().map(|s| s.path.clone()).collect(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    CppContext {
        build_dir: s.build_dir.clone(),
        build_type: profile.build_type.clone(),
        multi_config: s.model.as_ref().is_some_and(|m| m.multi_config),
        ctest: cforge_cmake::profile::ctest_next_to(&cmake),
        targets,
        coverage_tools: tools,
    }
}

/// Hand the C++ context to the GoogleTest, Catch2 and CTest providers (`run.rs`).
pub(crate) fn extend_test_context(
    state: &AppState,
    type_id: &str,
    ctx: &mut forge_testing::CommandContext,
) {
    if ![
        cforge_lang_cpp::GTEST,
        cforge_lang_cpp::CATCH2,
        cforge_lang_cpp::CTEST,
    ]
    .contains(&type_id)
    {
        return;
    }
    let cpp = context(state);
    if ctx.coverage_file.is_some() {
        // Fresh gcov data: counters of earlier runs would add up.
        remove_gcda(Path::new(&cpp.build_dir));
    }
    if let Ok(json) = serde_json::to_string(&cpp) {
        ctx.settings
            .insert(cforge_lang_cpp::CONTEXT_KEY.into(), json);
    }
}

fn to_runtools(cfg: &RunConfiguration) -> forge_runtools::Configuration {
    forge_runtools::Configuration {
        name: cfg.name.clone(),
        type_id: cfg.type_id.clone(),
        working_directory: cfg.working_directory.clone(),
        interpreter_options: cfg.interpreter_options.clone(),
        arguments: cfg.arguments.clone(),
        options: cfg.options.clone(),
        before_launch: Vec::new(),
    }
}

/// The executable of a CMake Application configuration.
pub(crate) fn app_executable(state: &AppState, cfg: &RunConfiguration) -> Result<String> {
    let exe = cfg
        .options
        .get("executable")
        .map(|s| s.trim())
        .unwrap_or("");
    if !exe.is_empty() {
        let root = state.project_root()?;
        return Ok(if Path::new(exe).is_absolute() {
            exe.to_string()
        } else {
            path_string(&root.join(exe))
        });
    }
    let target = cfg.options.get("target").map(|s| s.trim()).unwrap_or("");
    if target.is_empty() {
        return Err(ForgeError::Other("Choose the target to run".into()));
    }
    let s = current_state(state)?;
    let profile = active_profile(state);
    let model = s.model.ok_or_else(|| {
        ForgeError::Other(
            "The CMake project is not loaded: reload it in the CMake tool window".into(),
        )
    })?;
    model
        .target(&profile.build_type, target)
        .and_then(|t| t.executable().map(str::to_string))
        .ok_or_else(|| {
            ForgeError::Other(format!(
                "'{target}' is not an executable target of profile {}",
                profile.name
            ))
        })
}

pub(crate) fn working_dir(root: &Path, cfg: &RunConfiguration, exe: &str) -> PathBuf {
    match cfg.working_directory.trim() {
        "" => Path::new(exe)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.to_path_buf()),
        d if Path::new(d).is_absolute() => PathBuf::from(d),
        d => root.join(d),
    }
}

/// The command line of a CMake Application configuration (`run.rs`).
pub(crate) fn app_run(state: &AppState, cfg: &RunConfiguration) -> Result<CommandLine> {
    let root = state.project_root()?;
    let exe = app_executable(state, cfg)?;
    let mut cmd = CommandLine::new(exe.clone()).args(forge_process::split_args(&cfg.arguments));
    cmd.cwd = working_dir(&root, cfg, &exe);
    let profile = active_profile(state);
    cmd.env = profile.env();
    cmd.env.extend(cfg.env.clone());
    // Readable sanitizer reports (UBSan stacks, leak checks) unless the user set options.
    for (k, v) in cforge_cmake::sanitizers::run_env(&profile.sanitizers) {
        if std::env::var_os(&k).is_none() {
            cmd.env.entry(k).or_insert(v);
        }
    }
    Ok(cmd)
}

/// The target a configuration builds before it runs (the UI's Before launch step).
#[tauri::command]
pub async fn cpp_target_of(
    configuration: RunConfiguration,
    app: AppHandle,
) -> Result<Option<String>> {
    blocking(move || {
        let state = app.state::<AppState>();
        Ok(match configuration.type_id.as_str() {
            cforge_lang_cpp::CMAKE_APP => configuration
                .options
                .get("target")
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty()),
            cforge_lang_cpp::GTEST | cforge_lang_cpp::CATCH2 => {
                context(&state).target_name(&to_runtools(&configuration))
            }
            _ => None,
        })
    })
    .await
}

// ---- Debugging ------------------------------------------------------------------------------

/// libstdc++'s printers of a GCC toolchain (local machine).
fn printers_dir(toolchain: Option<&Toolchain>) -> Option<String> {
    let t = toolchain?;
    let cxx = t.cxx_compiler.as_deref()?;
    let version = t
        .compiler
        .as_ref()
        .map(|c| c.version.as_str())
        .unwrap_or("");
    tools::printer_dir_candidates(cxx, version)
        .into_iter()
        .find(|d| Path::new(d).join("libstdcxx").is_dir())
}

/// The launch request of a CMake Application, Google Test or Catch2 configuration: the
/// target's executable under the toolchain's debugger (LLDB or GDB, or the configuration's
/// `debugger`), with STL renderers.
#[tauri::command]
pub async fn cpp_debug_request(
    configuration: RunConfiguration,
    app: AppHandle,
) -> Result<LaunchRequest> {
    blocking(move || debug_request(&app.state::<AppState>(), &configuration)).await
}

fn debug_request(state: &AppState, cfg: &RunConfiguration) -> Result<LaunchRequest> {
    let root = state.project_root()?;
    let profile = active_profile(state);
    let toolchain = toolchain_of(state, Some(&profile));
    let (program, args) = match cfg.type_id.as_str() {
        cforge_lang_cpp::CMAKE_APP => (
            app_executable(state, cfg)?,
            forge_process::split_args(&cfg.arguments),
        ),
        _ => {
            let (p, mut a) = cforge_lang_cpp::debug_command(&to_runtools(cfg), &context(state))
                .map_err(ForgeError::Other)?;
            a.extend(forge_process::split_args(&cfg.arguments));
            (p, a)
        }
    };
    let chosen = cfg.options.get("debugger").map(|d| d.trim()).unwrap_or("");
    let debugger = toolchain.as_ref().and_then(|t| t.debugger.clone());
    let adapter = match chosen {
        "gdb" | "lldb" => chosen.to_string(),
        _ => debugger
            .as_ref()
            .map(|d| d.kind.adapter().to_string())
            .unwrap_or_else(|| "gdb".into()),
    };
    let setup = tools::debug_setup(
        &adapter,
        printers_dir(toolchain.as_ref()).as_deref(),
        flag(state, "cpp.debugger.stlRenderers", true),
    );
    let mut r = LaunchRequest::launch(&adapter, &program);
    r.name = Some(cfg.name.clone()).filter(|n| !n.is_empty());
    r.args = args;
    r.cwd = Some(path_string(&working_dir(&root, cfg, &program)));
    r.env = profile.env();
    r.env.extend(cfg.env.clone());
    r.configuration = setup.configuration;
    r.adapter_args = setup.adapter_args;
    // The toolchain's own GDB (cross toolchains: gdb-multiarch) unless the user set one.
    let user_gdb = setting(state, "debugger.gdb.path");
    if let Some(d) =
        debugger.filter(|d| d.kind.adapter() == adapter && adapter == "gdb" && user_gdb.is_none())
    {
        r.adapter_command = Some(d.path);
    }
    Ok(r)
}

// ---- Coverage -------------------------------------------------------------------------------

/// `.gcda` files below a folder (bounded).
fn gcda_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "gcda") {
                out.push(path_string(&p));
                if out.len() >= 20_000 {
                    return out;
                }
            }
        }
    }
    out.sort();
    out
}

fn remove_gcda(dir: &Path) {
    if dir.as_os_str().is_empty() {
        return;
    }
    for f in gcda_files(dir) {
        let _ = std::fs::remove_file(f);
    }
}

/// Turn a C++ test run's raw coverage into LCOV: Clang's `.profraw` files in the coverage
/// folder (llvm-profdata, llvm-cov export), else gcov's `.gcda` files in the build folder.
pub(crate) fn collect_coverage(
    backend: &dyn Backend,
    collect: &forge_testing::CoverageCollect,
    coverage_dir: &Path,
    root: &str,
) -> std::result::Result<String, String> {
    if backend.info().remote.is_some() {
        return Err("C++ coverage is collected for local projects only".into());
    }
    let tool = |name: &str| {
        collect
            .tools
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    };
    let run = |cmd: cforge_cmake::Command| -> std::result::Result<String, String> {
        let program = cmd.program.clone();
        let mut line = to_command_line(cmd);
        if line.cwd.as_os_str().is_empty() {
            line.cwd = coverage_dir.to_path_buf();
        }
        let out = capture(backend, &line, "", COVERAGE_TIMEOUT).map_err(|e| e.to_string())?;
        match out.code {
            Some(0) => Ok(out.stdout),
            _ => Err(format!("{program} failed: {}", out.stderr.trim())),
        }
    };
    let profraw: Vec<String> = std::fs::read_dir(coverage_dir)
        .map(|it| {
            it.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "profraw"))
                .map(|p| path_string(&p))
                .collect()
        })
        .unwrap_or_default();
    if !profraw.is_empty() {
        let profdata = path_string(&coverage_dir.join("forge.profdata"));
        run(cforge_lang_cpp::coverage::llvm_merge_command(
            &tool("llvm-profdata"),
            &profraw,
            &profdata,
        ))?;
        let export = cforge_lang_cpp::coverage::llvm_export_command(
            &tool("llvm-cov"),
            &profdata,
            &collect.binaries,
        )
        .ok_or("no test executables for llvm-cov")?;
        return run(export);
    }
    let build = collect.dirs.first().ok_or("no build folder")?;
    let gcda = gcda_files(Path::new(build));
    if gcda.is_empty() {
        return Err("No coverage data: build the profile with --coverage (GCC) or -fprofile-instr-generate -fcoverage-mapping (Clang)".into());
    }
    let json = run(cforge_lang_cpp::coverage::gcov_command(
        &tool("gcov"),
        &gcda,
        build,
    ))?;
    cforge_lang_cpp::coverage::gcov_json_to_lcov(&json, root)
}

// ---- clangd and clang-format ------------------------------------------------------------------

/// Where clangd comes from.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClangdInfo {
    pub path: Option<String>,
    pub version: Option<String>,
    /// `setting`, `toolchain`, `path`, `managed`, or empty.
    pub source: String,
    /// The pinned version Forge downloads; `None` when there is no build for this platform.
    pub pinned_version: Option<String>,
}

fn clangd_version(path: &str) -> Option<String> {
    let out = std::process::Command::new(path)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    tools::clangd_version(&String::from_utf8_lossy(&out.stdout))
}

/// clangd: the setting, the toolchain's LLVM, the PATH, Forge's download.
fn find_clangd(state: &AppState) -> (Option<PathBuf>, String) {
    if let Some(p) = setting(state, "cpp.clangd.path") {
        return (Some(PathBuf::from(p)), "setting".into());
    }
    let toolchain = toolchain_of(state, Some(&active_profile(state)));
    if let Some(cxx) = toolchain.as_ref().and_then(|t| t.cxx_compiler.as_deref()) {
        let next = PathBuf::from(cforge_lang_cpp::coverage::tool_next_to(cxx, "clangd"));
        if next.is_absolute() && next.is_file() {
            return (Some(next), "toolchain".into());
        }
    }
    if let Some(p) = forge_lsp::find_on_path("clangd") {
        return (Some(p), "path".into());
    }
    if let Some(i) = managed::clangd_dir().and_then(|d| managed::installed_clangd(&d)) {
        return (Some(i.binary), "managed".into());
    }
    (None, String::new())
}

fn clangd_info(state: &AppState) -> ClangdInfo {
    let (path, source) = find_clangd(state);
    let path = path.map(|p| path_string(&p));
    ClangdInfo {
        version: path.as_deref().and_then(clangd_version),
        path,
        source,
        pinned_version: managed::current_clangd_release().map(|d| d.version),
    }
}

/// The compilation database folder clangd reads: the active profile's build folder, the
/// `cpp.compileCommands` setting, the project root or `build/`.
fn compile_commands_dir(state: &AppState, backend: &dyn Backend) -> Option<String> {
    let root = backend.info().root.clone();
    if let Some(p) = setting(state, "cpp.compileCommands") {
        let abs = if Path::new(&p).is_absolute() {
            PathBuf::from(&p)
        } else {
            root.join(&p)
        };
        let dir = if abs.extension().is_some_and(|e| e == "json") {
            abs.parent().map(Path::to_path_buf).unwrap_or(abs)
        } else {
            abs
        };
        return Some(path_string(&dir));
    }
    if has_cmake(backend) {
        return Some(path_string(&build_dir(&root, &active_profile(state))));
    }
    for rel in ["compile_commands.json", "build/compile_commands.json"] {
        if backend.read_optional(rel).ok().flatten().is_some() {
            let dir = Path::new(rel)
                .parent()
                .map(|d| root.join(d))
                .unwrap_or(root.clone());
            return Some(path_string(&dir));
        }
    }
    None
}

fn lsp_config(state: &AppState, backend: &dyn Backend, command: String) -> LspServerConfig {
    let toolchain = toolchain_of(state, Some(&active_profile(state)));
    let query_drivers = toolchain
        .iter()
        .flat_map(|t| [t.c_compiler.clone(), t.cxx_compiler.clone()])
        .flatten()
        .filter(|c| Path::new(c).is_absolute())
        .collect();
    let s = state.read_settings();
    let options = ClangdOptions {
        compile_commands_dir: compile_commands_dir(state, backend),
        clang_tidy: s.get_as::<bool>("cpp.clangd.clangTidy").unwrap_or(true),
        header_insertion: s
            .get_as::<String>("cpp.clangd.headerInsertion")
            .unwrap_or_else(|| "iwyu".into()),
        background_index: true,
        completion_style: s
            .get_as::<String>("cpp.clangd.completionStyle")
            .unwrap_or_else(|| "detailed".into()),
        query_drivers,
        extra_args: s
            .get_as::<String>("cpp.clangd.arguments")
            .unwrap_or_default(),
    };
    drop(s);
    LspServerConfig {
        id: SERVER_ID.into(),
        name: Some("clangd".into()),
        language_ids: vec!["c".into(), "cpp".into()],
        file_extensions: [
            "c", "h", "cc", "cpp", "cxx", "c++", "hh", "hpp", "hxx", "ipp", "inl", "cu", "m", "mm",
        ]
        .iter()
        .map(|e| e.to_string())
        .collect(),
        command,
        args: tools::clangd_args(&options),
        initialization_options: json!({ "clangdFileStatus": true }),
        settings: serde_json::Value::Null,
        env: BTreeMap::new(),
        pull_diagnostics: false,
    }
}

/// Register (or remove) clangd for the open project: a local C/C++ project with the plugin
/// enabled and clangd installed.
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
    if !enabled(&state) || !info.uses("cpp") || backend.info().remote.is_some() {
        lsp.unregister_server(SERVER_ID);
        return;
    }
    match find_clangd(&state).0 {
        Some(p) => lsp.register_server(lsp_config(&state, backend.as_ref(), path_string(&p))),
        None => {
            tracing::info!("clangd not started: not installed");
            lsp.unregister_server(SERVER_ID);
        }
    }
}

pub(crate) fn restart_lsp_server(app: &AppHandle) {
    let lsp = app.state::<LspState>();
    lsp.unregister_server(SERVER_ID);
    spawn_sync_lsp_server(app);
}

/// Whether a changed setting reconfigures clangd.
pub fn affects_server(key: &str) -> bool {
    key.starts_with("cpp.clangd.")
        || matches!(
            key,
            "cpp.compileCommands" | "cpp.toolchains" | "cmake.activeProfile" | "cmake.profiles"
        )
}

pub fn spawn_sync_lsp_server(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || sync_lsp_server(&app, None));
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CppStatus {
    pub clangd: ClangdInfo,
    pub clang_format: Option<String>,
    /// The active profile's toolchain.
    pub toolchain: Option<Toolchain>,
    /// `cmake`, `compileCommands` or `sources`.
    pub project_kind: String,
    /// The compilation database clangd reads.
    pub compile_commands_dir: Option<String>,
    pub compile_commands: Option<compdb::Summary>,
    pub remote: bool,
}

/// Settings | C/C++ and the status bar.
#[tauri::command]
pub async fn cpp_status(refresh: Option<bool>, app: AppHandle) -> Result<CppStatus> {
    let refresh = refresh.unwrap_or(false);
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let remote = backend.info().remote.is_some();
        let toolchain = (!remote)
            .then(|| {
                toolchains(&state, refresh);
                toolchain_of(&state, Some(&active_profile(&state)))
            })
            .flatten();
        let dir = compile_commands_dir(&state, backend.as_ref());
        let root = backend.info().root.clone();
        let summary = dir.as_ref().and_then(|d| {
            let text = read_text(
                backend.as_ref(),
                &root,
                &Path::new(d).join(compdb::FILE_NAME),
            )?;
            compdb::parse(&text)
                .ok()
                .map(|e| compdb::summary(&e, &path_string(&root)))
        });
        let kind = if has_cmake(backend.as_ref()) {
            "cmake"
        } else if summary.is_some() {
            "compileCommands"
        } else {
            "sources"
        };
        Ok(CppStatus {
            clangd: clangd_info(&state),
            clang_format: (!remote)
                .then(|| clang_format(&state))
                .flatten()
                .map(|p| path_string(&p)),
            toolchain,
            project_kind: kind.into(),
            compile_commands_dir: dir,
            compile_commands: summary,
            remote,
        })
    })
    .await
}

/// Download the pinned clangd (the user agreed in the UI) and start it for the project.
#[tauri::command]
pub async fn cpp_clangd_install(app: AppHandle) -> Result<ClangdInfo> {
    let base = managed::clangd_dir()
        .ok_or_else(|| ForgeError::Other("no user data folder for clangd".into()))?;
    let d = managed::current_clangd_release().ok_or_else(|| {
        ForgeError::Other("The clangd releases have no build for this platform".into())
    })?;
    let http = Http::new().map_err(ForgeError::Other)?;
    let emitter = app.clone();
    let mut on = move |e: Event| {
        let payload = match e {
            Event::Step { message } => json!({ "message": message }),
            Event::Progress { label, done, total } => {
                json!({ "label": label, "done": done, "total": total })
            }
            Event::Output { line, .. } => json!({ "message": line }),
        };
        let _ = emitter.emit("cpp://install", payload);
    };
    managed::install_clangd(&http, &base, &d, &mut on)
        .await
        .map_err(ForgeError::Other)?;
    restart_lsp_server(&app);
    blocking(move || Ok(clangd_info(&app.state::<AppState>()))).await
}

fn clang_format(state: &AppState) -> Option<PathBuf> {
    if let Some(p) = setting(state, "cpp.clangFormat.path") {
        return Some(PathBuf::from(p));
    }
    let toolchain = toolchain_of(state, Some(&active_profile(state)));
    if let Some(cxx) = toolchain.as_ref().and_then(|t| t.cxx_compiler.as_deref()) {
        let next = PathBuf::from(cforge_lang_cpp::coverage::tool_next_to(cxx, "clang-format"));
        if next.is_absolute() && next.is_file() {
            return Some(next);
        }
    }
    forge_lsp::find_on_path("clang-format")
}

/// Format a C/C++ file with clang-format (`.clang-format` of its folder); `None` when the
/// project formats with clangd or not at all.
#[tauri::command]
pub async fn cpp_format(
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<Option<String>> {
    let formatter = setting(&state, "cpp.formatter").unwrap_or_else(|| "clangFormat".into());
    if formatter != "clangFormat" {
        return Ok(None);
    }
    let fallback =
        setting(&state, "cpp.clangFormat.fallbackStyle").unwrap_or_else(|| "LLVM".into());
    let Some(program) = clang_format(&state) else {
        return Err(ForgeError::Other(
            "clang-format is not installed: install LLVM or choose it in Settings | C/C++".into(),
        ));
    };
    let backend = state.backend()?;
    let safe_mode = state.safe_mode.is_on();
    blocking(move || {
        if safe_mode
            && forge_trust::runs_project_program(&backend.info().root, &path_string(&program), &[])
        {
            return Err(ForgeError::Other(forge_trust::refusal(
                "Running the project's clang-format",
            )));
        }
        let abs = backend.resolve(&path)?;
        let mut cmd = CommandLine::new(path_string(&program))
            .args(tools::clang_format_args(&path_string(&abs), &fallback));
        cmd.cwd = backend.info().root.clone();
        let out = capture(backend.as_ref(), &cmd, &content, FORMAT_TIMEOUT)?;
        match out.code {
            Some(0) => Ok(Some(out.stdout)),
            None => Err(ForgeError::Other("clang-format did not finish".into())),
            Some(_) => Err(ForgeError::Other(out.stderr.trim().to_string())),
        }
    })
    .await
}

/// Completion in CMake files.
#[tauri::command]
pub fn cmake_complete(text: String, offset: usize) -> Option<Completion> {
    cforge_cmake::complete::complete(&text, offset)
}
