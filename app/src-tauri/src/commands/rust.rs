//! RustForge (`packs/rustforge/*`): the project's toolchain and Cargo workspace, the Cargo
//! tool window, Cargo run configurations, the Cargo Test provider's context, rustfmt,
//! Clippy, rust-analyzer's configuration and download, and debugging with LLDB / GDB and
//! rustc's pretty printers.
//!
//! Commands that build or run project code (`cargo metadata` may run nothing, but the
//! project's `.cargo/config.toml` can name programs; Clippy and debug builds run build
//! scripts and proc macros) call `require_trusted`; command lines the UI starts (Cargo
//! actions, rustup installs) go through `run_start`, which refuses them in Safe Mode.
//!
//! Events: `rust://install` (`{ message }` steps and `{ label, done, total }` progress) while
//! rust-analyzer downloads; `rust://workspace` (the new `CargoWorkspace`) after a reload.

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_backend::process_io::capture;
use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_dap::LaunchRequest;
use forge_langservers::managed::{Event, Http};
use forge_lsp::{LspManager, LspServerConfig};
use forge_process::{CommandLine, RunConfiguration};
use rustforge_lang_rust::{
    AnalyzerSettings, CargoCommand, CargoContext, CargoWorkspace, RunTarget, RustProblem,
    Selection, TargetKind,
};
use rustforge_toolchain::detect::{system_runner, ToolchainFileInfo};
use rustforge_toolchain::{RustEnv, RustStatus};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::debug::DebugState;
use crate::commands::workspace::blocking;
use crate::state::AppState;

pub const PLUGIN: &str = "forge.rust";
pub const ANALYZER_ID: &str = "rust-analyzer";
const METADATA_TIMEOUT: Duration = Duration::from_secs(300);
const BUILD_TIMEOUT: Duration = Duration::from_secs(1800);
const FORMAT_TIMEOUT: Duration = Duration::from_secs(20);

// ---- Settings ------------------------------------------------------------------------------

fn setting(state: &AppState, key: &str) -> String {
    state
        .read_settings()
        .get_as::<String>(key)
        .map(|v| v.trim().to_string())
        .unwrap_or_default()
}

fn flag(state: &AppState, key: &str, default: bool) -> bool {
    state.read_settings().get_bool(key).unwrap_or(default)
}

fn list(state: &AppState, key: &str) -> Vec<String> {
    state
        .read_settings()
        .get_as::<Vec<String>>(key)
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// `KEY=VALUE` entries of `rust.environment`.
fn environment(state: &AppState) -> BTreeMap<String, String> {
    list(state, "rust.environment")
        .iter()
        .filter_map(|e| e.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.to_string()))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

fn is_local(state: &AppState) -> bool {
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

// ---- Toolchain status -------------------------------------------------------------------------

/// Detected status per project root, looked up again after a minute.
static STATUS: Mutex<Option<(PathBuf, String, Instant, RustStatus)>> = Mutex::new(None);

/// The toolchain status of the project (local projects; remote ones get an empty status).
pub(crate) fn status(state: &AppState, refresh: bool) -> RustStatus {
    let Ok(root) = state.project_root() else {
        return RustStatus::default();
    };
    if !is_local(state) {
        return RustStatus::default();
    }
    let toolchain = setting(state, "rust.toolchain");
    {
        let cached = STATUS.lock().expect("rust status");
        if let Some((r, t, at, s)) = cached.as_ref() {
            if !refresh && *r == root && *t == toolchain && at.elapsed() < Duration::from_secs(60) {
                return s.clone();
            }
        }
    }
    let s = rustforge_toolchain::detect::detect(
        &RustEnv::system(),
        Some(&root),
        &toolchain,
        &system_runner,
    );
    *STATUS.lock().expect("rust status") = Some((root, toolchain, Instant::now(), s.clone()));
    s
}

fn cached_status() -> Option<RustStatus> {
    STATUS
        .lock()
        .expect("rust status")
        .as_ref()
        .map(|(_, _, _, s)| s.clone())
}

/// The project's `rust-toolchain.toml` / `rust-toolchain`.
fn toolchain_file(backend: &dyn Backend) -> Option<ToolchainFileInfo> {
    for name in ["rust-toolchain.toml", "rust-toolchain"] {
        if let Ok(Some(text)) = backend.read_optional(name) {
            return Some(
                match rustforge_lang_rust::toolchain::parse_toolchain_file(&text) {
                    Ok(file) => ToolchainFileInfo {
                        path: name.into(),
                        file: Some(file),
                        error: None,
                    },
                    Err(e) => ToolchainFileInfo {
                        path: name.into(),
                        file: None,
                        error: Some(e),
                    },
                },
            );
        }
    }
    None
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RustStatusPayload {
    #[serde(flatten)]
    pub status: RustStatus,
    /// The toolchain chosen in Settings | Rust (empty: rustup's choice).
    pub chosen: String,
}

/// Toolchains, the active one and why, the sysroot, components and tools (Settings | Rust,
/// the status bar).
#[tauri::command]
pub async fn rust_status(
    refresh: Option<bool>,
    state: State<'_, AppState>,
) -> Result<RustStatusPayload> {
    let app_state: &AppState = &state;
    let mut s = status(app_state, refresh.unwrap_or(false));
    if let Ok(b) = state.backend() {
        s.toolchain_file = toolchain_file(b.as_ref());
    }
    Ok(RustStatusPayload {
        status: s,
        chosen: setting(&state, "rust.toolchain"),
    })
}

fn rustup_program(state: &AppState) -> String {
    status(state, false)
        .rustup
        .unwrap_or_else(|| "rustup".into())
}

/// The command line that installs a toolchain, a rustup component or a cargo plugin (the
/// UI asks first and starts it with `run_start`).
#[tauri::command]
pub fn rust_install_command(
    kind: String,
    name: String,
    toolchain: Option<String>,
    state: State<'_, AppState>,
) -> Result<CommandLine> {
    let c = match kind.as_str() {
        "toolchain" => rustforge_toolchain::install::toolchain(
            &rustup_program(&state),
            &name,
            &["clippy".into(), "rustfmt".into(), "rust-src".into()],
        ),
        "component" => rustforge_toolchain::install::component(
            &rustup_program(&state),
            &name,
            toolchain.as_deref().unwrap_or(""),
        ),
        "cargoPlugin" => rustforge_toolchain::install::cargo_plugin(
            &status(&state, false)
                .cargo
                .unwrap_or_else(|| "cargo".into()),
            &name,
        ),
        other => Err(format!("unknown install kind '{other}'")),
    }
    .map_err(ForgeError::Other)?;
    let mut cmd = CommandLine::new(c.program).args(c.args);
    cmd.cwd = std::env::temp_dir();
    Ok(cmd)
}

/// Download rust-analyzer (the user agreed in the UI) from its GitHub release
/// (`rustforge_toolchain::analyzer::PINNED_RELEASE`, SHA-256 checked) into Forge's data
/// folder; returns the executable. Language servers are planned again afterwards.
#[tauri::command]
pub async fn rust_analyzer_download(app: AppHandle) -> Result<String> {
    let base = rustforge_toolchain::analyzer::default_dir()
        .ok_or_else(|| ForgeError::Other("no user data folder for rust-analyzer".into()))?;
    let asset =
        rustforge_toolchain::analyzer::asset_name(std::env::consts::OS, std::env::consts::ARCH)
            .ok_or_else(|| {
                ForgeError::Other("rust-analyzer has no release for this platform".into())
            })?;
    let http = Http::new().map_err(ForgeError::Other)?;
    let emitter = app.clone();
    let mut on = move |e: Event| {
        let payload = match e {
            Event::Step { message } => serde_json::json!({ "message": message }),
            Event::Progress { label, done, total } => {
                serde_json::json!({ "label": label, "done": done, "total": total })
            }
            Event::Output { line, .. } => serde_json::json!({ "message": line }),
        };
        let _ = emitter.emit("rust://install", payload);
    };
    let exe = rustforge_toolchain::analyzer::install(
        &http,
        rustforge_toolchain::analyzer::DEFAULT_API,
        rustforge_toolchain::analyzer::PINNED_RELEASE,
        &asset,
        &base,
        &mut on,
    )
    .await
    .map_err(ForgeError::Other)?;
    let state = app.state::<AppState>();
    status(&state, true);
    spawn_sync_analyzer(&app);
    Ok(exe.to_string_lossy().into_owned())
}

// ---- Cargo workspace ---------------------------------------------------------------------------

/// The loaded workspace of a project, the key of the manifests it came from and the
/// project's Rust files.
struct Loaded {
    root: PathBuf,
    key: u64,
    workspace: CargoWorkspace,
    files: Vec<String>,
}

static WORKSPACE: Mutex<Option<Loaded>> = Mutex::new(None);

/// A key of the manifests and the lock file: the cached metadata is reused while it holds.
fn manifests_key(backend: &dyn Backend, previous: Option<&CargoWorkspace>) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut files: Vec<String> = vec!["Cargo.toml".into(), "Cargo.lock".into()];
    if let Some(ws) = previous {
        files.extend(ws.members.iter().map(|m| m.manifest.clone()));
        if !ws.root.is_empty() {
            files.push(format!("{}/Cargo.toml", ws.root));
            files.push(format!("{}/Cargo.lock", ws.root));
        }
    }
    for f in files {
        f.hash(&mut h);
        backend.read_optional(&f).ok().flatten().hash(&mut h);
    }
    h.finish()
}

/// `<cache dir>/forge/rust/<project hash>.json`: the last `cargo metadata` of a project.
fn cache_file(root: &Path) -> Option<PathBuf> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut h);
    dirs::cache_dir().map(|d| {
        d.join("forge")
            .join("rust")
            .join(format!("{:016x}.json", h.finish()))
    })
}

#[derive(serde::Deserialize, serde::Serialize)]
struct DiskCache {
    key: u64,
    workspace: CargoWorkspace,
}

/// Where the workspace's Cargo.toml is: the project root, else the first folder below it
/// with one (a Rust part of a larger project).
fn manifest_dir(backend: &dyn Backend) -> Option<String> {
    if backend.read_optional("Cargo.toml").ok().flatten().is_some() {
        return Some(String::new());
    }
    let entries = backend.list_dir("").ok()?;
    entries
        .iter()
        .filter(|e| e.kind == forge_core::FileKind::Dir && !e.name.starts_with('.') && !e.excluded)
        .find(|e| {
            backend
                .read_optional(&format!("{}/Cargo.toml", e.path))
                .ok()
                .flatten()
                .is_some()
        })
        .map(|e| e.path.clone())
}

fn rust_files(backend: &dyn Backend) -> Vec<String> {
    let req = forge_core::search::CollectRequest {
        extensions: vec!["rs".into()],
        max_files: 20_000,
        max_file_size: 4 << 20,
        max_total: 256 << 20,
    };
    backend
        .collect_files(&req)
        .map(|files| files.into_iter().map(|f| f.path).collect())
        .unwrap_or_default()
}

/// The project's Cargo workspace: from memory, the cache file, or `cargo metadata`.
fn load_workspace(state: &AppState, refresh: bool) -> Result<CargoWorkspace> {
    let backend = state.backend()?;
    let root = backend.info().root.clone();
    {
        let cached = WORKSPACE.lock().expect("cargo workspace");
        if let Some(l) = cached.as_ref().filter(|l| l.root == root && !refresh) {
            if manifests_key(backend.as_ref(), Some(&l.workspace)) == l.key {
                return Ok(l.workspace.clone());
            }
        }
    }
    let disk = cache_file(&root);
    if !refresh {
        if let Some(cached) = disk
            .as_ref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|t| serde_json::from_str::<DiskCache>(&t).ok())
        {
            if manifests_key(backend.as_ref(), Some(&cached.workspace)) == cached.key {
                let files = rust_files(backend.as_ref());
                *WORKSPACE.lock().expect("cargo workspace") = Some(Loaded {
                    root,
                    key: cached.key,
                    workspace: cached.workspace.clone(),
                    files,
                });
                return Ok(cached.workspace);
            }
        }
    }
    state.require_trusted("Loading the Cargo project (cargo metadata)")?;
    let dir = manifest_dir(backend.as_ref())
        .ok_or_else(|| ForgeError::Other("The project has no Cargo.toml".into()))?;
    let ctx = base_context(state);
    let mut cmd = CommandLine::new(ctx.cargo());
    cmd.args.extend(ctx.toolchain_args(""));
    cmd.args
        .extend(["metadata".into(), "--format-version".into(), "1".into()]);
    cmd.cwd = if dir.is_empty() {
        root.clone()
    } else {
        backend.resolve(&dir)?
    };
    cmd.env = ctx.environment();
    let out = capture(backend.as_ref(), &cmd, "", METADATA_TIMEOUT)?;
    if out.code != Some(0) {
        let err = out.stderr.trim();
        return Err(ForgeError::Other(if err.is_empty() {
            "cargo metadata failed".into()
        } else {
            err.lines()
                .rev()
                .find(|l| l.contains("error"))
                .unwrap_or(err)
                .to_string()
        }));
    }
    let workspace = rustforge_lang_rust::parse_metadata(&out.stdout, &root.to_string_lossy())
        .map_err(ForgeError::Other)?;
    let key = manifests_key(backend.as_ref(), Some(&workspace));
    if let Some(f) = disk {
        if let Some(parent) = f.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string(&DiskCache {
            key,
            workspace: workspace.clone(),
        }) {
            let _ = std::fs::write(f, text);
        }
    }
    let files = rust_files(backend.as_ref());
    *WORKSPACE.lock().expect("cargo workspace") = Some(Loaded {
        root,
        key,
        workspace: workspace.clone(),
        files,
    });
    Ok(workspace)
}

/// The Cargo workspace for the Cargo tool window (`refresh`: run `cargo metadata` again).
#[tauri::command]
pub async fn cargo_workspace(refresh: Option<bool>, app: AppHandle) -> Result<CargoWorkspace> {
    let a = app.clone();
    let ws =
        blocking(move || load_workspace(&a.state::<AppState>(), refresh.unwrap_or(false))).await?;
    let _ = app.emit("rust://workspace", &ws);
    Ok(ws)
}

// ---- Context -----------------------------------------------------------------------------------

/// The context without the workspace (settings and tools).
fn base_context(state: &AppState) -> CargoContext {
    let s = cached_status().unwrap_or_default();
    CargoContext {
        cargo: s.cargo.clone().unwrap_or_default(),
        toolchain: setting(state, "rust.toolchain"),
        nightly: s.nightly(),
        test_format: setting(state, "rust.test.format"),
        test_runner: match setting(state, "rust.test.runner").as_str() {
            "" => "cargo".into(),
            r => r.into(),
        },
        nextest: s.tool("cargo-nextest").is_some(),
        llvm_cov: s.tool("cargo-llvm-cov").is_some(),
        env: environment(state),
        features: list(state, "rust.cargo.features"),
        all_features: flag(state, "rust.cargo.allFeatures", false),
        no_default_features: flag(state, "rust.cargo.noDefaultFeatures", false),
        windows: cfg!(windows) && is_local(state),
        ..CargoContext::default()
    }
}

/// The project's Cargo context with its workspace (loaded if needed and allowed).
pub(crate) fn context(state: &AppState) -> CargoContext {
    if is_local(state) {
        status(state, false);
    }
    let mut ctx = base_context(state);
    if load_workspace(state, false).is_ok() {
        if let Some(l) = WORKSPACE.lock().expect("cargo workspace").as_ref() {
            ctx = ctx.with_workspace(&l.workspace);
            ctx.files = l.files.clone();
        }
    }
    ctx
}

/// Hand the Cargo context to the Cargo Test provider of a test run (`run.rs`).
pub(crate) fn extend_test_context(
    state: &AppState,
    type_id: &str,
    ctx: &mut forge_testing::CommandContext,
) {
    if type_id != rustforge_lang_rust::CARGO_TEST {
        return;
    }
    if let Ok(json) = serde_json::to_string(&context(state)) {
        ctx.settings
            .insert(rustforge_lang_rust::CONTEXT_KEY.to_string(), json);
    }
}

/// The tests of a Rust file with the workspace's packages and targets (gutter ▶).
pub(crate) fn discover_file(state: &AppState, path: &str, text: &str) -> forge_testing::TestFile {
    let ctx = loaded_context(state);
    rustforge_lang_rust::discover_file(path, text, ctx.as_ref())
}

/// The context of an already loaded workspace (never runs cargo: discovery is frequent).
fn loaded_context(state: &AppState) -> Option<CargoContext> {
    let root = state.project_root().ok()?;
    let guard = WORKSPACE.lock().expect("cargo workspace");
    let l = guard.as_ref().filter(|l| l.root == root)?;
    let mut ctx = base_context(state).with_workspace(&l.workspace);
    ctx.files = l.files.clone();
    Some(ctx)
}

fn to_command_line(c: CargoCommand, root: &Path) -> CommandLine {
    let mut cmd = CommandLine::new(c.program).args(c.args);
    cmd.cwd = if c.cwd.is_empty() {
        root.to_path_buf()
    } else if Path::new(&c.cwd).is_absolute() {
        PathBuf::from(&c.cwd)
    } else {
        root.join(&c.cwd)
    };
    cmd.env = c.env;
    cmd
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

/// The command line of a "Cargo" configuration (`run.rs`).
pub(crate) fn cargo_run(state: &AppState, cfg: &RunConfiguration) -> Result<CommandLine> {
    let ctx = context(state);
    let built =
        rustforge_lang_rust::command_line(&to_runtools(cfg), &ctx).map_err(ForgeError::Other)?;
    let mut cmd = to_command_line(built, &state.project_root()?);
    cmd.env.extend(cfg.env.clone());
    Ok(cmd)
}

/// A Cargo tool window action (`build`, `check`, `clean`, `update`, `test`, `doc`,
/// `clippy`, `run`, `bench`) for a package and target (started by the UI with `run_start`).
#[tauri::command]
pub fn cargo_command(
    action: String,
    package: Option<String>,
    target: Option<String>,
    state: State<'_, AppState>,
) -> Result<CommandLine> {
    let ctx = context(&state);
    let sel = Selection {
        package: package.unwrap_or_default(),
        target: target.unwrap_or_default(),
        ..Selection::default()
    };
    let c = rustforge_lang_rust::action_command(&action, &sel, &ctx).map_err(ForgeError::Other)?;
    Ok(to_command_line(c, &state.project_root()?))
}

/// `fn main` of binaries and examples in a file (gutter ▶ Run / Debug).
#[tauri::command]
pub fn rust_file_targets(
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Vec<RunTarget> {
    let ctx = loaded_context(&state);
    rustforge_lang_rust::run_targets(&path, &content, ctx.as_ref())
}

// ---- rustfmt and Clippy -------------------------------------------------------------------------

/// Reformat a Rust file with rustfmt (`rust.formatter`): the package's edition and the
/// nearest `rustfmt.toml`. `None` when formatting is off, left to rust-analyzer or rustfmt
/// is missing.
#[tauri::command]
pub async fn rust_format(
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<Option<String>> {
    let formatter = match setting(&state, "rust.formatter") {
        f if f.is_empty() => "rustfmt".to_string(),
        f => f,
    };
    if formatter != "rustfmt" {
        return Ok(None);
    }
    let program = if is_local(&state) {
        match status(&state, false).tool("rustfmt") {
            Some(p) => p.to_string(),
            None => return Ok(None),
        }
    } else {
        "rustfmt".to_string()
    };
    let edition = loaded_context(&state)
        .and_then(|_| {
            WORKSPACE
                .lock()
                .expect("cargo workspace")
                .as_ref()
                .and_then(|l| l.workspace.package_of(&path).map(|p| p.edition.clone()))
        })
        .unwrap_or_default();
    let extra = forge_process::split_args(&setting(&state, "rust.rustfmt.extraArgs"));
    let backend = state.backend()?;
    let safe_mode = state.safe_mode.is_on();
    blocking(move || {
        let root = backend.info().root.clone();
        if safe_mode && forge_trust::runs_project_program(&root, &program, &[]) {
            return Err(ForgeError::Other(forge_trust::refusal(
                "Running the project's rustfmt",
            )));
        }
        let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let config = rustforge_lang_rust::format::config_candidates(dir)
            .into_iter()
            .find(|c| backend.read_optional(c).ok().flatten().is_some())
            .and_then(|c| backend.resolve(&c).ok())
            .map(|p| p.to_string_lossy().into_owned());
        let mut cmd = CommandLine::new(&program).args(rustforge_lang_rust::format::rustfmt_args(
            &edition,
            config.as_deref(),
            &extra,
        ));
        cmd.cwd = root;
        cmd.env.insert("RUSTUP_AUTO_INSTALL".into(), "0".into());
        let out = capture(backend.as_ref(), &cmd, &content, FORMAT_TIMEOUT)?;
        match out.code {
            Some(0) => Ok(Some(out.stdout)),
            None => Err(ForgeError::Other("rustfmt did not finish".into())),
            Some(_) => Err(ForgeError::Other(
                out.stderr.trim().replace("<stdin>", &path).to_string(),
            )),
        }
    })
    .await
}

/// Clippy's problems (`cargo clippy --message-format=json`) for the package of `path` (the
/// whole workspace without it), with suggestions as quick fixes. Clippy builds the code,
/// so it runs only in trusted projects.
#[tauri::command]
pub async fn rust_clippy(path: Option<String>, app: AppHandle) -> Result<Vec<RustProblem>> {
    let a = app.clone();
    blocking(move || {
        let state = a.state::<AppState>();
        state.require_trusted("Running Clippy")?;
        let ctx = context(&state);
        let backend = state.backend()?;
        let package = path
            .as_deref()
            .and_then(|p| ctx.package_of(p))
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let mut args = ctx.toolchain_args("");
        args.push("clippy".into());
        args.extend(ctx.manifest_args());
        if package.is_empty() {
            args.push("--workspace".into());
        }
        args.push("--all-targets".into());
        let sel = Selection {
            package,
            ..Selection::default()
        };
        args.extend(
            rustforge_lang_rust::command::selection_args("clippy", &sel, &ctx, &[])
                .map_err(ForgeError::Other)?,
        );
        args.extend(["--message-format".into(), "json".into()]);
        let extra = forge_process::split_args(&setting(&state, "rust.clippy.extraArgs"));
        if !extra.is_empty() {
            args.push("--".into());
            args.extend(extra);
        }
        let mut cmd = CommandLine::new(ctx.cargo()).args(args);
        cmd.cwd = backend.info().root.clone();
        cmd.env = ctx.environment();
        let out = capture(backend.as_ref(), &cmd, "", BUILD_TIMEOUT)?;
        if out.code.is_none() {
            return Err(ForgeError::Other("Clippy did not finish".into()));
        }
        let mut problems = rustforge_lang_rust::problems(&out.stdout, &ctx.workspace);
        if problems.is_empty() && out.code != Some(0) && !out.stdout.contains("compiler-message") {
            return Err(ForgeError::Other(
                out.stderr
                    .lines()
                    .rev()
                    .find(|l| l.contains("error"))
                    .unwrap_or("cargo clippy failed")
                    .to_string(),
            ));
        }
        // LSP positions count UTF-16 code units.
        let mut texts: BTreeMap<String, Option<String>> = BTreeMap::new();
        for p in &mut problems {
            let text = texts
                .entry(p.path.clone())
                .or_insert_with(|| backend.read_optional(&p.path).ok().flatten());
            if let Some(t) = text {
                let line = |n: u32| t.lines().nth(n as usize).unwrap_or("");
                p.column = rustforge_lang_rust::messages::utf16_column(line(p.line), p.column);
                p.end_column =
                    rustforge_lang_rust::messages::utf16_column(line(p.end_line), p.end_column);
                for e in p
                    .fixes
                    .iter_mut()
                    .flat_map(|f| f.edits.iter_mut())
                    .filter(|e| e.path == p.path)
                {
                    e.column = rustforge_lang_rust::messages::utf16_column(line(e.line), e.column);
                    e.end_column =
                        rustforge_lang_rust::messages::utf16_column(line(e.end_line), e.end_column);
                }
            }
        }
        Ok(problems)
    })
    .await
}

// ---- Debugging -------------------------------------------------------------------------------------

fn gdb_contribution(state: &AppState) -> Option<forge_plugin_api::DebugAdapterContribution> {
    state
        .plugins
        .read()
        .expect("plugins lock poisoned")
        .debug_adapters()
        .find(|a| a.r#type == "gdb")
        .cloned()
}

fn rust_gdb(
    state: &AppState,
    printers: Option<rustforge_lang_rust::PrettyPrinters>,
) -> Option<rustforge_toolchain::gdb::RustGdbAdapter> {
    let gdb = gdb_contribution(state)?;
    let command = Some(setting(state, "debugger.gdb.path")).filter(|c| !c.is_empty());
    Some(rustforge_toolchain::gdb::RustGdbAdapter::new(
        gdb, command, printers,
    ))
}

/// Register the `rust-gdb` adapter with the DAP manager while the Rust plugin is enabled.
pub(crate) fn sync_debug(state: &AppState, debug: &DebugState) {
    if !enabled(state) {
        debug
            .dap
            .unregister_adapter(rustforge_toolchain::gdb::ADAPTER);
        return;
    }
    let printers = cached_status().and_then(|s| s.pretty_printers);
    if let Some(a) = rust_gdb(state, printers) {
        debug.dap.register_adapter(Arc::new(a));
    }
}

/// The adapter for Rust: `rust.debugger`, `auto` picks LLDB when lldb-dap is installed.
fn debugger(state: &AppState) -> &'static str {
    match setting(state, "rust.debugger").as_str() {
        "lldb" => "lldb",
        "gdb" => "gdb",
        _ => {
            let custom = setting(state, "debugger.lldb.path");
            let found = if custom.is_empty() {
                forge_lsp::find_on_path("lldb-dap").is_some()
                    || forge_lsp::find_on_path("lldb-vscode").is_some()
            } else {
                Path::new(&custom).is_file()
            };
            if found {
                "lldb"
            } else {
                "gdb"
            }
        }
    }
}

/// Build a Cargo or Cargo Test configuration (`--message-format=json`) and return the
/// launch request for its executable: LLDB with rustc's pretty printers as
/// `initCommands`, or the `rust-gdb` adapter.
#[tauri::command]
pub async fn rust_debug_request(
    configuration: RunConfiguration,
    app: AppHandle,
) -> Result<LaunchRequest> {
    let a = app.clone();
    let (request, printers) = blocking(move || {
        let state = a.state::<AppState>();
        state.require_trusted("Building the program to debug")?;
        let ctx = context(&state);
        let backend = state.backend()?;
        let root = backend.info().root.clone();
        let cfg = to_runtools(&configuration);
        let (build_args, package, kind, target, args) = if cfg.type_id == rustforge_lang_rust::CARGO_TEST {
            let d = rustforge_lang_rust::debug_test(&cfg, &ctx).map_err(ForgeError::Other)?;
            (d.build_args, d.package, d.kind, d.target, d.args)
        } else {
            let command = cfg.option("command").trim().to_string();
            let sub = command.split_whitespace().next().unwrap_or("run").to_string();
            if sub != "run" && !command.is_empty() {
                return Err(ForgeError::Other(format!(
                    "Debug builds and runs a binary; 'cargo {sub}' cannot be debugged. Use a Cargo Test configuration for tests."
                )));
            }
            let sel = Selection::from_options(&cfg);
            let mut b = ctx.toolchain_args(cfg.option("toolchain"));
            b.push("build".into());
            b.extend(ctx.manifest_args());
            b.extend(
                rustforge_lang_rust::command::selection_args("build", &sel, &ctx, &[])
                    .map_err(ForgeError::Other)?,
            );
            b.extend(["--message-format".into(), "json".into()]);
            let (kind, name) = match rustforge_lang_rust::command::parse_target(&sel.target) {
                Some((k, n)) => (Some(k), n),
                None => (Some(TargetKind::Bin), String::new()),
            };
            let args = forge_process::split_args(&cfg.arguments);
            (b, sel.package, kind, name, args)
        };
        let mut cmd = CommandLine::new(ctx.cargo()).args(build_args);
        cmd.cwd = root.clone();
        cmd.env = ctx.environment();
        let out = capture(backend.as_ref(), &cmd, "", BUILD_TIMEOUT)?;
        if let Some(err) = rustforge_lang_rust::build_failure(&out.stdout) {
            return Err(ForgeError::Other(err));
        }
        if out.code != Some(0) && out.stdout.trim().is_empty() {
            return Err(ForgeError::Other(out.stderr.trim().to_string()));
        }
        let found = rustforge_lang_rust::artifacts(&out.stdout);
        let exe = rustforge_lang_rust::pick_executable(&found, &package, kind, &target)
            .map_err(ForgeError::Other)?;
        let adapter = debugger(&state);
        let printers = flag(&state, "rust.debugger.prettyPrinters", true)
            .then(|| status(&state, false).pretty_printers)
            .flatten();
        let cwd = match configuration.working_directory.trim() {
            "" => root.to_string_lossy().into_owned(),
            w if Path::new(w).is_absolute() => w.to_string(),
            w => root.join(w).to_string_lossy().into_owned(),
        };
        let mut env = ctx.env.clone();
        env.extend(configuration.env.clone());
        let mut r = LaunchRequest::launch(
            if adapter == "lldb" { "lldb" } else { rustforge_toolchain::gdb::ADAPTER },
            &exe.executable,
        );
        r.name = Some(configuration.name.clone());
        r.args = args;
        r.cwd = Some(cwd);
        r.env = env;
        if adapter == "lldb" {
            if let Some(p) = printers.as_ref() {
                r.configuration = serde_json::json!({ "initCommands": p.lldb_init_commands() });
            }
        }
        Ok((r, printers))
    })
    .await?;
    // GDB gets the printers as adapter arguments: register it with this toolchain's.
    if request.adapter == rustforge_toolchain::gdb::ADAPTER {
        let state = app.state::<AppState>();
        let debug = app.state::<DebugState>();
        if let Some(a) = rust_gdb(&state, printers) {
            debug.dap.register_adapter(Arc::new(a));
        }
    }
    Ok(request)
}

// ---- rust-analyzer -------------------------------------------------------------------------------

fn analyzer_settings(state: &AppState) -> AnalyzerSettings {
    AnalyzerSettings {
        check_on_save: flag(state, "rust.analyzer.checkOnSave", true),
        check_command: setting(state, "rust.analyzer.checkCommand"),
        features: list(state, "rust.cargo.features"),
        all_features: flag(state, "rust.cargo.allFeatures", false),
        no_default_features: flag(state, "rust.cargo.noDefaultFeatures", false),
        target: setting(state, "rust.analyzer.target"),
        build_scripts: flag(state, "rust.analyzer.buildScripts", true),
        proc_macros: flag(state, "rust.analyzer.procMacros", true),
        env: environment(state),
        trusted: !state.safe_mode.is_on(),
    }
}

/// The rust-analyzer executable: the setting, the toolchain's component or Forge's
/// download (`None`: leave the catalog's choice).
fn analyzer_command(state: &AppState) -> Option<String> {
    let custom = setting(state, "rust.analyzer.path");
    if !custom.is_empty() {
        return Some(custom);
    }
    if !is_local(state) {
        return None;
    }
    status(state, false)
        .tool("rust-analyzer")
        .map(str::to_string)
}

/// Configure the planned rust-analyzer from Settings | Rust, or register it when only the
/// toolchain or Forge's download has one (called after the catalog servers are planned).
pub(crate) fn configure_analyzer(manager: &LspManager, state: &AppState) {
    if !enabled(state) {
        return;
    }
    let settings = analyzer_settings(state);
    let existing = manager.configs().into_iter().find(|c| c.id == ANALYZER_ID);
    let uses_rust = existing.is_some()
        || state
            .backend()
            .ok()
            .and_then(|b| manifest_dir(b.as_ref()))
            .is_some();
    if !uses_rust {
        return;
    }
    let command = analyzer_command(state);
    let mut config = match existing {
        Some(c) => c,
        None => match &command {
            Some(_) => LspServerConfig {
                id: ANALYZER_ID.into(),
                name: Some("rust-analyzer".into()),
                language_ids: vec!["rust".into()],
                file_extensions: vec!["rs".into()],
                command: String::new(),
                args: Vec::new(),
                initialization_options: serde_json::Value::Null,
                settings: serde_json::Value::Null,
                ..LspServerConfig::default()
            },
            None => return,
        },
    };
    let before = config.clone();
    if let Some(c) = command {
        config.command = c;
    }
    config.initialization_options = settings.config();
    config.settings = settings.settings();
    if config != before {
        manager.register_server(config);
    }
}

/// Whether a changed setting reconfigures rust-analyzer.
pub fn affects_analyzer(key: &str) -> bool {
    key.starts_with("rust.analyzer.")
        || key.starts_with("rust.cargo.")
        || matches!(key, "rust.environment" | "rust.toolchain")
}

/// [`configure_analyzer`] for the open project, on a blocking thread.
pub fn spawn_sync_analyzer(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let lsp = app.state::<crate::commands::lsp::LspState>();
        if let Ok(m) = lsp.manager(&app, &state) {
            configure_analyzer(&m, &state);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_that_restart_rust_analyzer() {
        assert!(affects_analyzer("rust.analyzer.procMacros"));
        assert!(affects_analyzer("rust.cargo.features"));
        assert!(!affects_analyzer("rust.formatter"));
    }
}
