//! GoForge (`packs/goforge/*`): the project's Go context (SDK, GOPATH, GOFLAGS, build tags,
//! modules), SDK detection and downloads, tools, `go.mod` / `go.work` analysis and
//! completion, module commands, `go generate`, gofmt / goimports, go vet and golangci-lint,
//! Delve's launch requests and the pprof Profiler.
//!
//! Commands that start project programs (`go vet`, golangci-lint) call `require_trusted`;
//! command lines the UI starts (module commands, `go generate`, tool installs) go through
//! `run_start`, which refuses them in Safe Mode.
//!
//! Events: `go://install` (`{ message }` steps and `{ label, done, total }` progress) while
//! `go_sdk_download` runs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_backend::process_io::capture;
use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_dap::LaunchRequest;
use forge_langservers::managed::{Event, Http};
use forge_process::{CommandLine, RunConfiguration};
use goforge_lang_go::gomod::{GO_MOD_DIRECTIVES, GO_WORK_DIRECTIVES};
use goforge_lang_go::{
    GenerateDirective, GoCommand, GoContext, GoMod, GoProblem, GoWork, ModCompletion, ModuleRoot,
};
use goforge_pprof::ProfileView;
use goforge_sdk::download::{self, Downloadable};
use goforge_sdk::proxy::proxy_from_goproxy;
use goforge_sdk::{DetectEnv, GoSdk, GoTool, ModuleProxy, ToolDirs, ToolStatus};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::commands::debug::DebugState;
use crate::commands::workspace::blocking;
use crate::state::AppState;

pub const PLUGIN: &str = "forge.go";
const LINT_TIMEOUT: Duration = Duration::from_secs(180);
const FORMAT_TIMEOUT: Duration = Duration::from_secs(20);

// ---- Context ------------------------------------------------------------------------------

/// Detected SDKs, looked up again after a minute.
static SDKS: Mutex<Option<(Instant, Vec<GoSdk>)>> = Mutex::new(None);

fn sdks(refresh: bool) -> Vec<GoSdk> {
    let mut cached = SDKS.lock().expect("sdk cache");
    if let Some((at, list)) = cached.as_ref() {
        if !refresh && at.elapsed() < Duration::from_secs(60) {
            return list.clone();
        }
    }
    let list = goforge_sdk::detect(&DetectEnv::system());
    *cached = Some((Instant::now(), list.clone()));
    list
}

fn setting(state: &AppState, key: &str) -> Option<String> {
    state
        .read_settings()
        .get_as::<String>(key)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn is_local(state: &AppState) -> bool {
    state
        .backend()
        .map(|b| b.info().remote.is_none())
        .unwrap_or(true)
}

/// The SDK the project uses: `go.goroot`, else the newest one found (local projects).
fn current_sdk(state: &AppState) -> Option<GoSdk> {
    if let Some(root) = setting(state, "go.goroot") {
        return goforge_sdk::sdk_from_folder(Path::new(&root))
            .ok()
            .or(Some(GoSdk {
                root: PathBuf::from(root),
                version: String::new(),
                source: "custom".into(),
            }));
    }
    if !is_local(state) {
        return None;
    }
    sdks(false).into_iter().next()
}

/// The module folders of a project: `go.mod` in the root and one or two levels below, plus
/// the `use` folders of `go.work`.
fn modules(backend: &dyn Backend) -> Vec<ModuleRoot> {
    let mut files: Vec<(String, String)> = Vec::new();
    let read = |path: String, files: &mut Vec<(String, String)>| {
        if files.iter().any(|(p, _)| *p == path) {
            return;
        }
        if let Ok(Some(text)) = backend.read_optional(&path) {
            files.push((path, text));
        }
    };
    read("go.mod".into(), &mut files);
    if let Ok(Some(work)) = backend.read_optional("go.work") {
        for u in goforge_lang_go::parse_go_work(&work).uses {
            let dir = u
                .text
                .trim_start_matches("./")
                .trim_matches('/')
                .to_string();
            if !dir.contains("..") {
                read(goforge_lang_go::env::join(&dir, "go.mod"), &mut files);
            }
        }
    }
    let skip = |name: &str| {
        name.starts_with('.') || matches!(name, "vendor" | "node_modules" | "testdata" | "target")
    };
    let mut dirs: Vec<(String, u32)> = vec![(String::new(), 0)];
    while let Some((dir, depth)) = dirs.pop() {
        let Ok(entries) = backend.list_dir(&dir) else {
            continue;
        };
        for e in entries {
            if e.kind != forge_core::FileKind::Dir || skip(&e.name) || e.excluded {
                continue;
            }
            read(format!("{}/go.mod", e.path), &mut files);
            if depth < 1 {
                dirs.push((e.path.clone(), depth + 1));
            }
        }
    }
    goforge_lang_go::modules_of(files.iter().map(|(p, t)| (p.as_str(), t.as_str())))
}

/// `KEY=VALUE` entries of `go.environment`.
fn environment(state: &AppState) -> BTreeMap<String, String> {
    state
        .read_settings()
        .get_as::<Vec<String>>("go.environment")
        .unwrap_or_default()
        .iter()
        .filter_map(|e| e.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.to_string()))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

/// The project's Go context (see `goforge_lang_go::GoContext`).
pub(crate) fn context(state: &AppState) -> GoContext {
    let modules = state
        .backend()
        .map(|b| modules(b.as_ref()))
        .unwrap_or_default();
    let s = state.read_settings();
    let tags: Vec<String> = s.get_as("go.buildTags").unwrap_or_default();
    let gopath = s.get_as::<String>("go.gopath").unwrap_or_default();
    let goflags = s.get_as::<String>("go.goflags").unwrap_or_default();
    drop(s);
    GoContext {
        goroot: current_sdk(state)
            .map(|s| s.root.to_string_lossy().into_owned())
            .unwrap_or_default(),
        gopath,
        goflags,
        build_tags: tags,
        env: environment(state),
        modules,
        windows: cfg!(windows) && is_local(state),
    }
}

/// Hand the Go context to the Go Test provider of a test run (`run.rs`).
pub(crate) fn extend_test_context(
    state: &AppState,
    type_id: &str,
    ctx: &mut forge_testing::CommandContext,
) {
    if type_id != goforge_lang_go::GO_TEST {
        return;
    }
    let go = context(state);
    if let Ok(json) = serde_json::to_string(&go) {
        ctx.settings
            .insert(goforge_lang_go::CONTEXT_KEY.to_string(), json);
    }
}

fn to_command_line(c: GoCommand, root: &Path) -> CommandLine {
    let mut cmd = CommandLine::new(c.program).args(c.args);
    cmd.cwd = if c.cwd.is_empty() {
        root.to_path_buf()
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

/// The command line of a Go Build configuration (`run.rs`).
pub(crate) fn build_run(state: &AppState, cfg: &RunConfiguration) -> Result<CommandLine> {
    let go = context(state);
    let built =
        goforge_lang_go::build_command(&to_runtools(cfg), &go).map_err(ForgeError::Other)?;
    let root = state.project_root()?;
    let mut cmd = to_command_line(built, &root);
    if !cfg.working_directory.trim().is_empty() {
        let wd = cfg.working_directory.trim();
        cmd.cwd = if Path::new(wd).is_absolute() {
            PathBuf::from(wd)
        } else {
            root.join(wd)
        };
    }
    cmd.env.extend(cfg.env.clone());
    Ok(cmd)
}

// ---- Status, SDKs and tools --------------------------------------------------------------

fn tool_dirs(go: &GoContext) -> ToolDirs {
    let goroot = (!go.goroot.is_empty()).then(|| PathBuf::from(&go.goroot));
    ToolDirs::system(Some(&go.gopath), goroot.as_deref())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoStatus {
    /// The SDK in use (`None`: no Go found).
    pub sdk: Option<GoSdk>,
    /// Whether the user chose it (`go.goroot`).
    pub custom: bool,
    pub sdks: Vec<GoSdk>,
    pub tools: Vec<ToolStatus>,
    pub modules: Vec<ModuleRoot>,
    /// GOPATH as the project uses it (setting or Go's default).
    pub gopath: String,
}

/// The project's SDK, the detected SDKs, the tools and modules (Settings | Go, status bar).
#[tauri::command]
pub async fn go_status(refresh: Option<bool>, state: State<'_, AppState>) -> Result<GoStatus> {
    let refresh = refresh.unwrap_or(false);
    let list = blocking(move || Ok(sdks(refresh))).await?;
    let go = context(&state);
    let sdk = current_sdk(&state);
    let dirs = tool_dirs(&go);
    let version = sdk.as_ref().map(|s| s.version.clone()).unwrap_or_default();
    let user = |t: GoTool| setting(&state, t.setting());
    let tools = goforge_sdk::tools::status(&dirs, &version, user);
    let gopath = if go.gopath.is_empty() {
        forge_process::host::env_var("GOPATH").unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join("go").to_string_lossy().into_owned())
                .unwrap_or_default()
        })
    } else {
        go.gopath.clone()
    };
    Ok(GoStatus {
        custom: setting(&state, "go.goroot").is_some(),
        sdk,
        sdks: list,
        tools,
        modules: go.modules,
        gopath,
    })
}

/// The Go SDKs on go.dev/dl for this platform (stable releases, newest first).
#[tauri::command]
pub async fn go_sdk_releases(state: State<'_, AppState>) -> Result<Vec<Downloadable>> {
    let source =
        setting(&state, "go.sdk.downloadSource").unwrap_or_else(|| download::DEFAULT_SOURCE.into());
    let http = Http::new().map_err(ForgeError::Other)?;
    let releases = download::releases(&http, &source)
        .await
        .map_err(ForgeError::Other)?;
    let (os, arch) = download::platform();
    Ok(goforge_sdk::downloadable(&releases, &os, &arch))
}

/// Download a Go SDK (the user agreed in the UI): the archive of `version` for this
/// platform, SHA-256 checked, into Forge's data folder; it becomes the project's GOROOT.
#[tauri::command]
pub async fn go_sdk_download(
    version: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<GoSdk> {
    let source =
        setting(&state, "go.sdk.downloadSource").unwrap_or_else(|| download::DEFAULT_SOURCE.into());
    let dir = goforge_sdk::detect::default_sdks_dir()
        .ok_or_else(|| ForgeError::Other("no user data folder for Go SDKs".into()))?;
    let http = Http::new().map_err(ForgeError::Other)?;
    let releases = download::releases(&http, &source)
        .await
        .map_err(ForgeError::Other)?;
    let (os, arch) = download::platform();
    let wanted = version.trim().trim_start_matches("go").to_string();
    let file = goforge_sdk::downloadable(&releases, &os, &arch)
        .into_iter()
        .find(|d| d.version == wanted)
        .ok_or_else(|| ForgeError::Other(format!("Go {wanted} has no download for {os}/{arch}")))?
        .file;
    let emitter = app.clone();
    let mut on = move |e: Event| {
        let payload = match e {
            Event::Step { message } => serde_json::json!({ "message": message }),
            Event::Progress { label, done, total } => {
                serde_json::json!({ "label": label, "done": done, "total": total })
            }
            Event::Output { line, .. } => serde_json::json!({ "message": line }),
        };
        let _ = emitter.emit("go://install", payload);
    };
    let root = download::install(&http, &source, &file, &dir, &mut on)
        .await
        .map_err(ForgeError::Other)?;
    sdks(true);
    goforge_sdk::sdk_from_folder(&root).map_err(ForgeError::Other)
}

/// The `go install` command line of a tool (started by the UI through `run_start`).
#[tauri::command]
pub fn go_tool_install_command(tool: GoTool, state: State<'_, AppState>) -> Result<CommandLine> {
    let go = context(&state);
    let version = current_sdk(&state).map(|s| s.version).unwrap_or_default();
    let gobin = goforge_sdk::tools::default_tools_dir()
        .ok_or_else(|| ForgeError::Other("no user data folder for Go tools".into()))?;
    let c = goforge_sdk::install_command(tool, &go.go(), &version, &gobin).ok_or_else(|| {
        ForgeError::Other(format!(
            "Forge does not install {}; see its website",
            tool.label()
        ))
    })?;
    let mut cmd = CommandLine::new(c.program).args(c.args);
    cmd.cwd = std::env::temp_dir();
    cmd.env = go.environment();
    cmd.env.extend(c.env);
    Ok(cmd)
}

// ---- go.mod / go.work ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ModFile {
    Mod(GoMod),
    Work(GoWork),
}

/// `go.mod` / `go.work` with positions and problems.
#[tauri::command]
pub fn go_mod_parse(path: String, content: String) -> ModFile {
    if path.ends_with("go.work") {
        ModFile::Work(goforge_lang_go::parse_go_work(&content))
    } else {
        ModFile::Mod(goforge_lang_go::parse_go_mod(&content))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModCompletionItem {
    pub label: String,
    pub detail: Option<String>,
    /// `directive`, `module`, `version`.
    pub kind: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModCompletionResult {
    pub from: u32,
    pub items: Vec<ModCompletionItem>,
}

fn gomodcache(go: &GoContext) -> Option<PathBuf> {
    if let Some(c) = forge_process::host::env_var("GOMODCACHE") {
        if !c.is_empty() {
            return Some(PathBuf::from(c));
        }
    }
    let gopath = if go.gopath.is_empty() {
        forge_process::host::env_var("GOPATH")
            .filter(|g| !g.is_empty())
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join("go")))?
    } else {
        PathBuf::from(
            go.gopath
                .split(if cfg!(windows) { ';' } else { ':' })
                .next()?,
        )
    };
    Some(gopath.join("pkg").join("mod"))
}

async fn module_versions(state: &AppState, module: &str) -> Result<Vec<String>> {
    let lookups = state
        .read_settings()
        .get_bool("go.modules.proxyLookups")
        .unwrap_or(true);
    if !lookups {
        return Ok(Vec::new());
    }
    let goproxy = environment(state)
        .get("GOPROXY")
        .cloned()
        .or_else(|| std::env::var("GOPROXY").ok())
        .unwrap_or_default();
    let Some(base) = proxy_from_goproxy(&goproxy) else {
        return Ok(Vec::new());
    };
    let proxy =
        ModuleProxy::new(&base, ModuleProxy::default_cache_dir()).map_err(ForgeError::Other)?;
    proxy.versions(module).await.map_err(ForgeError::Other)
}

/// Completion in `go.mod` / `go.work`: directives, module paths (the project's
/// requirements and the module cache) and versions (module proxy, cached).
#[tauri::command]
pub async fn go_mod_complete(
    path: String,
    content: String,
    offset: u32,
    state: State<'_, AppState>,
) -> Result<Option<ModCompletionResult>> {
    let Some(ctx) = goforge_lang_go::gomod::completion_context(&content, offset) else {
        return Ok(None);
    };
    let item = |label: &str, detail: Option<String>, kind: &str| ModCompletionItem {
        label: label.to_string(),
        detail,
        kind: kind.into(),
    };
    Ok(Some(match ctx {
        ModCompletion::Directive { from, .. } => ModCompletionResult {
            from,
            items: if path.ends_with("go.work") {
                GO_WORK_DIRECTIVES
            } else {
                GO_MOD_DIRECTIVES
            }
            .iter()
            .map(|d| item(d, None, "directive"))
            .collect(),
        },
        ModCompletion::GoVersion { from, .. } => {
            let mut versions: Vec<String> = sdks(false).iter().map(|s| s.version.clone()).collect();
            versions.dedup();
            ModCompletionResult {
                from,
                items: versions
                    .iter()
                    .map(|v| item(v, Some("installed SDK".into()), "version"))
                    .collect(),
            }
        }
        ModCompletion::ModulePath { from, prefix } => {
            let go = context(&state);
            let mut items: Vec<ModCompletionItem> = Vec::new();
            for r in goforge_lang_go::parse_go_mod(&content).requires {
                items.push(item(&r.path.text, Some("required".into()), "module"));
            }
            if let Some(cache) = gomodcache(&go) {
                let cached =
                    blocking(move || Ok(goforge_sdk::cached_modules(&cache, 2000))).await?;
                for m in cached {
                    if m.starts_with(&prefix) && !items.iter().any(|i| i.label == m) {
                        items.push(item(&m, Some("module cache".into()), "module"));
                    }
                }
            }
            items.truncate(500);
            ModCompletionResult { from, items }
        }
        ModCompletion::Version { from, module, .. } => {
            let versions = module_versions(&state, &module).await.unwrap_or_default();
            let latest = goforge_lang_go::version::latest(&versions);
            ModCompletionResult {
                from,
                items: versions
                    .iter()
                    .map(|v| {
                        let detail = (Some(v) == latest.as_ref()).then(|| "latest".to_string());
                        item(v, detail, "version")
                    })
                    .collect(),
            }
        }
    }))
}

/// Versions of a module (Add Dependency).
#[tauri::command]
pub async fn go_module_versions(module: String, state: State<'_, AppState>) -> Result<Vec<String>> {
    module_versions(&state, &module).await
}

/// `go mod tidy | download | verify`, `go work sync`, `go get <module>[@version]` in the
/// module folder `dir` (project relative).
#[tauri::command]
pub fn go_module_command(
    action: String,
    arg: Option<String>,
    dir: Option<String>,
    state: State<'_, AppState>,
) -> Result<CommandLine> {
    let go = context(&state);
    let dir = dir.unwrap_or_else(|| {
        go.modules
            .first()
            .map(|m| m.dir.clone())
            .unwrap_or_default()
    });
    let c = goforge_lang_go::module_command(&action, arg.as_deref().unwrap_or(""), &dir, &go)
        .map_err(ForgeError::Other)?;
    Ok(to_command_line(c, &state.project_root()?))
}

// ---- go generate ----------------------------------------------------------------------------

/// `//go:generate` directives of a file (gutter ▶).
#[tauri::command]
pub fn go_generate_directives(content: String) -> Vec<GenerateDirective> {
    goforge_lang_go::generate_directives(&content)
}

/// `go generate` for a file (all directives, or the one on `line`), in the file's module.
#[tauri::command]
pub fn go_generate_command(
    path: String,
    content: String,
    line: Option<u32>,
    state: State<'_, AppState>,
) -> Result<CommandLine> {
    let go = context(&state);
    let dir = goforge_lang_go::env::dir_of(&path).to_string();
    let module_dir = go
        .module_of(&dir)
        .map(|m| m.dir.clone())
        .unwrap_or_default();
    let rel = goforge_lang_go::env::strip_dir(&path, &module_dir).unwrap_or(&path);
    let args = goforge_lang_go::generate_args(&format!("./{rel}"), &content, line)
        .map_err(ForgeError::Other)?;
    let c = GoCommand {
        program: go.go(),
        args,
        cwd: module_dir,
        env: go.environment(),
    };
    Ok(to_command_line(c, &state.project_root()?))
}

// ---- Formatting and external annotators -------------------------------------------------

/// Reformat a Go file with gofmt or goimports (`go.formatter`); `None` when formatting is
/// off, left to gopls or the tool is missing.
#[tauri::command]
pub async fn go_format(
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<Option<String>> {
    let formatter = setting(&state, "go.formatter").unwrap_or_else(|| "gofmt".into());
    if formatter != "gofmt" && formatter != "goimports" {
        return Ok(None);
    }
    let go = context(&state);
    let dirs = tool_dirs(&go);
    let program = if formatter == "goimports" {
        dirs.locate(
            GoTool::Goimports,
            setting(&state, "go.goimports.path").as_deref(),
        )
    } else {
        dirs.locate_binary("gofmt", None)
    };
    let Some(program) = program else {
        return Ok(None);
    };
    let backend = state.backend()?;
    let safe_mode = state.safe_mode.is_on();
    blocking(move || {
        if safe_mode
            && forge_trust::runs_project_program(
                &backend.info().root,
                &program.to_string_lossy(),
                &[],
            )
        {
            return Err(ForgeError::Other(forge_trust::refusal(
                "Running the project's gofmt",
            )));
        }
        let mut cmd = CommandLine::new(program.to_string_lossy());
        if formatter == "goimports" {
            // goimports needs the file's path to find its module.
            cmd = cmd
                .arg("-srcdir")
                .arg(backend.resolve(&path)?.to_string_lossy());
        }
        cmd.cwd = backend.info().root.clone();
        let out = capture(backend.as_ref(), &cmd, &content, FORMAT_TIMEOUT)?;
        match out.code {
            Some(0) => Ok(Some(out.stdout)),
            None => Err(ForgeError::Other(format!("{formatter} did not finish"))),
            Some(_) => Err(ForgeError::Other(
                out.stderr
                    .trim()
                    .replace("<standard input>", &path)
                    .to_string(),
            )),
        }
    })
    .await
}

/// Problems from `go vet -json` or golangci-lint for the package of `path` (all packages
/// without it). Both build the code, so they run only in trusted projects.
#[tauri::command]
pub async fn go_lint(
    tool: String,
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<GoProblem>> {
    state.require_trusted(if tool == "vet" {
        "Running go vet"
    } else {
        "Running golangci-lint"
    })?;
    let go = context(&state);
    let backend = state.backend()?;
    let root_abs = backend.info().root.to_string_lossy().into_owned();
    let dir = path
        .as_deref()
        .map(|p| goforge_lang_go::env::dir_of(p).to_string());
    let (module_dir, target) = match &dir {
        Some(d) => {
            let (_, pkg) = go.package_arg(d, "");
            let m = go.module_of(d).map(|m| m.dir.clone()).unwrap_or_default();
            (m, pkg)
        }
        None => (
            go.modules
                .first()
                .map(|m| m.dir.clone())
                .unwrap_or_default(),
            "./...".to_string(),
        ),
    };
    let lint_path = setting(&state, "go.golangciLint.path");
    let dirs = tool_dirs(&go);
    blocking(move || {
        let cwd = if module_dir.is_empty() {
            backend.info().root.clone()
        } else {
            backend.resolve(&module_dir)?
        };
        let cwd_abs = cwd.to_string_lossy().into_owned();
        if tool == "vet" {
            let mut cmd = CommandLine::new(go.go()).args(["vet".to_string(), "-json".to_string()]);
            let tags = go.tags("");
            if !tags.is_empty() {
                cmd = cmd.arg(format!("-tags={}", tags.join(",")));
            }
            cmd = cmd.arg(target);
            cmd.cwd = cwd;
            cmd.env = go.environment();
            let out = capture(backend.as_ref(), &cmd, "", LINT_TIMEOUT)?;
            if out.code.is_none() {
                return Err(ForgeError::Other("go vet did not finish".into()));
            }
            let problems = goforge_lang_go::parse_vet(&out.stderr, &root_abs);
            if problems.is_empty() && out.code != Some(0) && !out.stderr.contains('{') {
                return Err(ForgeError::Other(out.stderr.trim().to_string()));
            }
            Ok(problems)
        } else {
            let Some(program) = dirs.locate(GoTool::GolangciLint, lint_path.as_deref()) else {
                return Err(ForgeError::Other(
                    "golangci-lint is not installed. Install it from golangci-lint.run, or set its path in Settings | Go."
                        .into(),
                ));
            };
            let mut version = CommandLine::new(program.to_string_lossy()).arg("--version");
            version.cwd = cwd.clone();
            let major = capture(backend.as_ref(), &version, "", FORMAT_TIMEOUT)
                .ok()
                .and_then(|o| goforge_lang_go::lint::golangci_major(&o.stdout))
                .unwrap_or(2);
            let mut cmd = CommandLine::new(program.to_string_lossy()).args(
                goforge_lang_go::lint::golangci_args(major, &target),
            );
            cmd.cwd = cwd;
            cmd.env = go.environment();
            let out = capture(backend.as_ref(), &cmd, "", LINT_TIMEOUT)?;
            if out.code.is_none() {
                return Err(ForgeError::Other("golangci-lint did not finish".into()));
            }
            let rel_cwd = goforge_lang_go::lint::relative(&cwd_abs, &root_abs);
            let rel_cwd = if rel_cwd == cwd_abs { String::new() } else { rel_cwd };
            goforge_lang_go::parse_golangci(&out.stdout, &root_abs, &rel_cwd).map_err(|e| {
                let err = out.stderr.trim();
                ForgeError::Other(if err.is_empty() { e } else { err.to_string() })
            })
        }
    })
    .await
}

// ---- Debugging ------------------------------------------------------------------------------

fn delve_config(state: &AppState) -> goforge_delve::DelveConfig {
    let go = context(state);
    let dirs = tool_dirs(&go);
    let dlv = dirs
        .locate(GoTool::Dlv, setting(state, "go.delve.path").as_deref())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut env: Vec<(String, String)> = go.environment().into_iter().collect();
    if !go.goroot.is_empty() {
        // Delve runs `go build`: the project's SDK first on the PATH.
        let bin = Path::new(&go.goroot).join("bin");
        let mut paths = vec![bin];
        if let Some(p) = forge_process::host::path_var() {
            paths.extend(std::env::split_paths(&p));
        }
        if let Ok(joined) = std::env::join_paths(paths) {
            env.push(("PATH".into(), joined.to_string_lossy().into_owned()));
        }
    }
    goforge_delve::DelveConfig { dlv, env }
}

/// Register Delve with the DAP manager while the Go plugin is enabled.
pub(crate) fn sync_debug(state: &AppState, debug: &DebugState) {
    let enabled = state
        .plugins
        .read()
        .expect("plugins lock poisoned")
        .is_enabled(PLUGIN);
    if enabled {
        debug
            .dap
            .register_adapter(Arc::new(goforge_delve::DelveAdapter::new(delve_config(
                state,
            ))));
    } else {
        debug.dap.unregister_adapter(goforge_delve::ADAPTER);
    }
}

/// Whether Delve is installed (`path`), and what "Install" would run.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DelveStatus {
    pub path: Option<String>,
    pub install: Option<String>,
}

#[tauri::command]
pub fn go_delve_status(state: State<'_, AppState>) -> DelveStatus {
    let c = delve_config(&state);
    let version = current_sdk(&state).map(|s| s.version).unwrap_or_default();
    DelveStatus {
        path: Some(c.dlv).filter(|p| !p.is_empty()),
        install: GoTool::Dlv.install_target(&version),
    }
}

/// The Delve launch request of a Go Build, Go Test or Go Remote configuration.
#[tauri::command]
pub fn go_debug_request(
    configuration: RunConfiguration,
    state: State<'_, AppState>,
) -> Result<LaunchRequest> {
    let go = context(&state);
    let root = state.project_root()?;
    goforge_delve::request(
        &to_runtools(&configuration),
        &configuration.env,
        &go,
        &root.to_string_lossy(),
    )
    .map_err(ForgeError::Other)
}

// ---- Profiler -------------------------------------------------------------------------------

/// `<cache dir>/forge/go-profiles/<name>-<time>.pprof`: where a profiled test run writes
/// its profile.
#[tauri::command]
pub fn go_profile_path(name: String) -> Result<String> {
    let dir = dirs::cache_dir()
        .ok_or_else(|| ForgeError::Other("no cache folder".into()))?
        .join("forge")
        .join("go-profiles");
    std::fs::create_dir_all(&dir).map_err(|e| ForgeError::Other(e.to_string()))?;
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok(dir
        .join(format!("{safe}-{stamp}.pprof"))
        .to_string_lossy()
        .into_owned())
}

/// Open a pprof profile: an absolute local path (a profiled run's output) or a project
/// file.
#[tauri::command]
pub async fn pprof_open(
    path: String,
    sample_index: Option<usize>,
    state: State<'_, AppState>,
) -> Result<ProfileView> {
    let backend = state.backend()?;
    blocking(move || {
        let data = if Path::new(&path).is_absolute() {
            std::fs::read(&path)
                .map_err(|e| ForgeError::Other(format!("cannot read {path}: {e}")))?
        } else {
            backend.read_bytes(&path)?
        };
        goforge_pprof::open(&data, sample_index).map_err(ForgeError::Other)
    })
    .await
}
