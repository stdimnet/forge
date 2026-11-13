//! DotNetForge for game engines (`packs/dotnetforge/gamedev`): Unity (the project's editor
//! and the Unity Hub's editors, the Forge editor bridge and its connection, project files,
//! Unity tests, Unity processes for debugging) and Godot (the project, the Godot
//! executable, the "Godot" run configuration, the editor's GDScript language server and
//! debug adapter, C# debugging with netcoredbg, GdUnit4).
//!
//! Reading projects runs nothing. Command lines (Start Unity, project files in batch mode,
//! Godot runs, tests) are started through `run_start`, which refuses them in Safe Mode;
//! `godot_debug_request` checks trust itself. Unity and Godot live on the user's machine,
//! so everything but reading the project needs a local project. Installing the bridge
//! package writes its source into `Packages/` after the user agreed in the UI.
//!
//! Events: `unity://bridge` (`BridgeEvent`: state, log, openFile with a project-relative
//! path, disconnected).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dotnetforge_gamedev::godot::{
    self, commands as godot_commands, dap as godot_dap, executable, gdunit, lsp as godot_lsp,
    GodotProject,
};
use dotnetforge_gamedev::unity::{
    self, bridge, commands as unity_commands, editors, package, processes, project, test_runner,
    UnityEditor, UnityProject,
};
use dotnetforge_gamedev::CommandSpec;
use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_dap::{LaunchMode, LaunchRequest};
use forge_process::{CommandLine, RunConfiguration};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use super::dotnet::{is_local, other, path_string, setting};
use crate::commands::debug::DebugState;
use crate::commands::lsp::LspState;
use crate::commands::workspace::blocking;
use crate::state::AppState;

pub const UNITY_PLUGIN: &str = "forge.unity";
pub const GODOT_PLUGIN: &str = "forge.godot";
pub const BRIDGE_EVENT: &str = "unity://bridge";
const BUILD_TIMEOUT: Duration = Duration::from_secs(1800);

/// The connection to the Unity editor of the open project.
#[derive(Default)]
pub struct GameDevState {
    bridge: Mutex<Option<(PathBuf, Arc<bridge::BridgeClient>)>>,
}

fn enabled(state: &AppState, plugin: &str) -> bool {
    state
        .plugins
        .read()
        .expect("plugins lock poisoned")
        .is_enabled(plugin)
}

fn int_setting(state: &AppState, key: &str, default: u16) -> u16 {
    state
        .read_settings()
        .get_as::<u64>(key)
        .and_then(|v| u16::try_from(v).ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn require_local(state: &AppState, what: &str) -> Result<()> {
    if is_local(state) {
        Ok(())
    } else {
        Err(other(format!(
            "{what} needs a local project: Unity and Godot run on this machine"
        )))
    }
}

fn to_command_line(c: CommandSpec, root: &Path) -> CommandLine {
    let mut cmd = CommandLine::new(c.program).args(c.args);
    cmd.cwd = if c.cwd.is_empty() {
        root.to_path_buf()
    } else {
        root.join(c.cwd)
    };
    cmd.env = c.env;
    cmd
}

fn root_files(backend: &dyn Backend) -> Vec<String> {
    backend
        .list_dir("")
        .map(|entries| {
            entries
                .into_iter()
                .filter(|e| e.kind != forge_core::workspace::FileKind::Dir)
                .map(|e| e.name)
                .collect()
        })
        .unwrap_or_default()
}

fn pid_alive(pid: u32) -> bool {
    forge_dap::list_processes().iter().any(|p| p.pid == pid)
}

fn os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    }
}

// ---- Unity -------------------------------------------------------------------------------------

pub(crate) fn unity_project(state: &AppState) -> Option<UnityProject> {
    let backend = state.backend().ok()?;
    let read = |rel: &str| backend.read_optional(rel).ok().flatten();
    unity::read_project(&read, &root_files(backend.as_ref()))
}

fn find_editors(state: &AppState) -> Vec<UnityEditor> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let env = editors::EditorEnv::system(os(), home, dirs::config_dir());
    let list = |p: &Path| -> Vec<String> {
        std::fs::read_dir(p)
            .map(|it| {
                it.flatten()
                    .filter(|e| e.path().is_dir())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    };
    let is_file = |p: &Path| p.is_file();
    let read = |p: &Path| std::fs::read_to_string(p).ok();
    let custom = setting(state, "unity.editorPath");
    editors::find_editors(
        &env,
        &editors::Fs {
            list_dirs: &list,
            is_file: &is_file,
            read: &read,
        },
        Some(custom.as_str()),
    )
}

/// The editor of a project: Settings', else the Hub's of its version, else the closest.
fn unity_editor(state: &AppState, project: &UnityProject) -> editors::EditorMatch {
    editors::match_editor(&find_editors(state), &project.version)
}

/// The running editor that has the project open (its pid).
fn editor_running(state: &AppState) -> Option<u32> {
    let backend = state.backend().ok()?;
    let text = backend
        .read_optional(project::EDITOR_INSTANCE)
        .ok()
        .flatten()?;
    let inst = project::parse_editor_instance(&text)?;
    pid_alive(inst.process_id).then_some(inst.process_id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnityStatus {
    project: Option<UnityProject>,
    editors: Vec<UnityEditor>,
    editor: Option<UnityEditor>,
    /// The editor is the project's exact version (or Settings' choice).
    exact: bool,
    /// The pid of the editor that has the project open.
    editor_running: Option<u32>,
    /// The bridge runs in that editor (`Library/ForgeBridge.json`).
    bridge_available: bool,
    connected: bool,
    welcome: Option<bridge::Welcome>,
    bundled_bridge: String,
    remote: bool,
}

/// The Unity project, its editors and the bridge.
#[tauri::command]
pub async fn unity_status(app: AppHandle) -> Result<UnityStatus> {
    blocking(move || {
        let state = app.state::<AppState>();
        let games = app.state::<GameDevState>();
        let remote = !is_local(&state);
        let project = unity_project(&state);
        let (editors, matched) = match (&project, remote) {
            (Some(p), false) => {
                let all = find_editors(&state);
                let m = editors::match_editor(&all, &p.version);
                (all, m)
            }
            _ => (Vec::new(), editors::EditorMatch::default()),
        };
        let root = state.project_root().ok();
        let client = games.bridge.lock().expect("bridge lock").clone();
        let connected = client
            .as_ref()
            .filter(|(r, c)| Some(r) == root.as_ref() && c.is_open());
        let bridge_available = !remote
            && state
                .backend()
                .ok()
                .and_then(|b| b.read_optional(project::BRIDGE_INSTANCE).ok().flatten())
                .and_then(|t| bridge::parse_instance(&t))
                .is_some_and(|i| i.pid == 0 || pid_alive(i.pid));
        Ok(UnityStatus {
            exact: matched.exact.is_some(),
            editor: matched.best().cloned(),
            editors,
            editor_running: if remote { None } else { editor_running(&state) },
            bridge_available,
            connected: connected.is_some(),
            welcome: connected.map(|(_, c)| c.welcome.clone()),
            bundled_bridge: package::PACKAGE_VERSION.into(),
            project,
            remote,
        })
    })
    .await
}

/// Copy the bridge package into `Packages/dev.forge.unity` (the UI asked the user first).
#[tauri::command]
pub async fn unity_install_bridge(app: AppHandle) -> Result<project::BridgeInstall> {
    blocking(move || {
        let state = app.state::<AppState>();
        if unity_project(&state).is_none() {
            return Err(other("The project is not a Unity project"));
        }
        let backend = state.backend()?;
        for f in package::install_plan() {
            backend.write_file(&f.path, &f.content)?;
        }
        Ok(unity_project(&state).map(|p| p.bridge).unwrap_or_default())
    })
    .await
}

/// Connect to the bridge of the editor that has the project open.
#[tauri::command]
pub async fn unity_bridge_connect(app: AppHandle) -> Result<bridge::Welcome> {
    let (root, instance) = {
        let state = app.state::<AppState>();
        require_local(&state, "Connecting to Unity")?;
        let backend = state.backend()?;
        let text = backend
            .read_optional(project::BRIDGE_INSTANCE)?
            .ok_or_else(|| other("The Forge editor bridge is not running: open the project in Unity with the bridge installed"))?;
        let instance = bridge::parse_instance(&text)
            .ok_or_else(|| other("Library/ForgeBridge.json is not a bridge instance file"))?;
        (backend.info().root.clone(), instance)
    };
    let (client, mut events) = bridge::BridgeClient::connect(
        &format!("127.0.0.1:{}", instance.port),
        &instance.token,
        Duration::from_secs(5),
    )
    .await
    .map_err(ForgeError::Other)?;
    let client = Arc::new(client);
    let welcome = client.welcome.clone();
    {
        let games = app.state::<GameDevState>();
        let mut slot = games.bridge.lock().expect("bridge lock");
        if let Some((_, old)) = slot.take() {
            old.close();
        }
        *slot = Some((root.clone(), client.clone()));
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            let event = match event {
                bridge::BridgeEvent::OpenFile { path, line, column } => {
                    bridge::BridgeEvent::OpenFile {
                        path: crate::commands::lsp::to_ui_path(&root, Path::new(&path)),
                        line,
                        column,
                    }
                }
                other => other,
            };
            let ended = matches!(event, bridge::BridgeEvent::Disconnected { .. });
            let _ = handle.emit(BRIDGE_EVENT, &event);
            if ended {
                let games = handle.state::<GameDevState>();
                let mut slot = games.bridge.lock().expect("bridge lock");
                if slot.as_ref().is_some_and(|(_, c)| Arc::ptr_eq(c, &client)) {
                    *slot = None;
                }
                break;
            }
        }
    });
    Ok(welcome)
}

#[tauri::command]
pub fn unity_bridge_disconnect(games: State<'_, GameDevState>) -> Result<()> {
    if let Some((_, c)) = games.bridge.lock().expect("bridge lock").take() {
        c.close();
    }
    Ok(())
}

/// Send a command (play, pause, resume, stop, step, refresh, syncSolution) to Unity.
#[tauri::command]
pub async fn unity_bridge_command(
    command: bridge::Command,
    games: State<'_, GameDevState>,
) -> Result<bridge::Ack> {
    let client = games
        .bridge
        .lock()
        .expect("bridge lock")
        .as_ref()
        .map(|(_, c)| c.clone())
        .filter(|c| c.is_open())
        .ok_or_else(|| other("Not connected to the Unity editor"))?;
    client.send(command).await.map_err(ForgeError::Other)
}

/// The command line of a Unity action (`openEditor`, `syncSolution`), run by the UI.
#[tauri::command]
pub async fn unity_command(action: String, app: AppHandle) -> Result<CommandLine> {
    blocking(move || {
        let state = app.state::<AppState>();
        require_local(&state, "Unity")?;
        let project = unity_project(&state).ok_or_else(|| other("The project is not a Unity project"))?;
        let editor = unity_editor(&state, &project).best().cloned().ok_or_else(|| {
            other(format!(
                "Unity {} is not installed: install it with the Unity Hub, or choose an editor in Settings | Languages & Frameworks | .NET | Unity",
                project.version
            ))
        })?;
        let root = state.project_root()?;
        let root_text = path_string(&root);
        let spec = match action.as_str() {
            "openEditor" => {
                if let Some(pid) = editor_running(&state) {
                    return Err(other(format!("Unity already has the project open (process {pid})")));
                }
                unity_commands::open_editor(&editor.path, &root_text)
            }
            "syncSolution" => {
                if editor_running(&state).is_some() {
                    return Err(other("Unity has the project open: connect to it (Tools | Unity | Connect to Unity Editor) to regenerate the project files there"));
                }
                unity_commands::sync_solution(&editor.path, &root_text, &project)
                    .map_err(ForgeError::Other)?
            }
            other_action => return Err(other(format!("Unknown Unity action '{other_action}'"))),
        };
        Ok(to_command_line(spec, &root))
    })
    .await
}

/// Running Unity editors with their debugger ports (Attach to Unity Process…).
#[tauri::command]
pub async fn unity_processes(app: AppHandle) -> Result<Vec<processes::UnityProcess>> {
    blocking(move || {
        let state = app.state::<AppState>();
        require_local(&state, "Attaching to Unity")?;
        let root = path_string(&state.project_root()?);
        let instance_pid = state
            .backend()?
            .read_optional(project::EDITOR_INSTANCE)?
            .and_then(|t| project::parse_editor_instance(&t))
            .map(|i| i.process_id);
        Ok(processes::unity_processes(
            &forge_dap::list_processes(),
            &root,
            instance_pid,
        ))
    })
    .await
}

/// Attach the configured Mono soft-debugger adapter to a Unity debugger port.
#[tauri::command]
pub fn unity_attach_request(
    port: u16,
    host: Option<String>,
    name: Option<String>,
    state: State<'_, AppState>,
) -> Result<LaunchRequest> {
    if setting(&state, "unity.debugger.adapterPath").is_empty() {
        return Err(other("Unity's debugger speaks Mono's soft debugger protocol, which Forge's debugger does not. Set a Mono debug adapter in Settings | Languages & Frameworks | .NET | Unity | Debugger to attach."));
    }
    let host = host
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "127.0.0.1".into());
    let mut r = LaunchRequest::launch("unity", "");
    r.program = None;
    r.name = name.or_else(|| Some(format!("Unity ({host}:{port})")));
    r.mode = LaunchMode::AttachPort { host, port };
    Ok(r)
}

/// The Unity test context (`unity-test` runs).
fn unity_test_context(state: &AppState) -> Option<test_runner::UnityContext> {
    let project = unity_project(state)?;
    let editor = unity_editor(state, &project)
        .best()
        .map(|e| e.path.clone())
        .unwrap_or_default();
    let backend = state.backend().ok()?;
    let mut classes = BTreeMap::new();
    let req = forge_core::search::CollectRequest {
        extensions: vec!["cs".into()],
        max_files: 20_000,
        max_file_size: 1 << 20,
        max_total: 256 << 20,
    };
    for f in backend.collect_files(&req).unwrap_or_default() {
        if !test_runner::is_unity_source(&f.path)
            || !(f.text.contains("[Test") || f.text.contains("[UnityTest"))
        {
            continue;
        }
        let scan = dotnetforge_lang_dotnet::scan::scan_source(
            &f.path,
            &f.text,
            Some(dotnetforge_lang_dotnet::TestFramework::Nunit),
        );
        for c in scan.classes.iter().filter(|c| !c.tests.is_empty()) {
            classes.insert(c.full_name.clone(), f.path.clone());
        }
    }
    Some(test_runner::UnityContext {
        editor,
        project_path: path_string(&backend.info().root),
        platform: setting(state, "unity.tests.platform"),
        classes,
        editor_open: editor_running(state).is_some(),
    })
}

// ---- Godot -------------------------------------------------------------------------------------

pub(crate) fn godot_project(state: &AppState) -> Option<GodotProject> {
    let text = state
        .backend()
        .ok()?
        .read_optional(godot::project::PROJECT_FILE)
        .ok()
        .flatten()?;
    let mut p = godot::parse_project_godot(&text);
    if !p.csharp {
        let backend = state.backend().ok()?;
        p.csharp = root_files(backend.as_ref())
            .iter()
            .any(|f| f.ends_with(".csproj"));
    }
    Some(p)
}

fn find_godot(state: &AppState, csharp: bool) -> Option<executable::GodotExecutable> {
    let env = executable::GodotEnv {
        os: os().into(),
        home: dirs::home_dir().unwrap_or_default(),
        path_dirs: forge_process::host::sandbox_path_var()
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default(),
        env_vars: ["GODOT4", "GODOT"]
            .iter()
            .filter_map(|v| forge_process::host::env_var(v))
            .collect(),
    };
    let custom = setting(state, "godot.path");
    let list = |p: &Path| -> Vec<String> {
        std::fs::read_dir(p)
            .map(|it| {
                it.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    };
    executable::find_godot(
        &env,
        Some(custom.as_str()),
        csharp,
        &|p: &Path| p.is_file(),
        &list,
    )
}

static VERSIONS: Mutex<Option<BTreeMap<String, Option<executable::GodotVersion>>>> =
    Mutex::new(None);

fn godot_version(state: &AppState, path: &str) -> Option<executable::GodotVersion> {
    if let Some(v) = VERSIONS
        .lock()
        .expect("godot versions")
        .as_ref()
        .and_then(|m| m.get(path).cloned())
    {
        return v;
    }
    let backend = state.backend().ok()?;
    let mut cmd = CommandLine::new(path.to_string()).args(vec!["--version".to_string()]);
    cmd.cwd = std::env::temp_dir();
    let v = forge_backend::process_io::capture(backend.as_ref(), &cmd, "", Duration::from_secs(15))
        .ok()
        .and_then(|out| executable::parse_version(&out.stdout));
    VERSIONS
        .lock()
        .expect("godot versions")
        .get_or_insert_with(BTreeMap::new)
        .insert(path.to_string(), v.clone());
    v
}

fn godot_host(state: &AppState) -> String {
    match setting(state, "godot.host") {
        h if h.is_empty() => godot_lsp::DEFAULT_HOST.into(),
        h => h,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GodotStatus {
    project: Option<GodotProject>,
    scenes: Vec<String>,
    godot: Option<executable::GodotExecutable>,
    version: Option<executable::GodotVersion>,
    host: String,
    lsp_port: u16,
    dap_port: u16,
    /// The editor's servers answer (the editor has a project open).
    lsp_listening: bool,
    dap_listening: bool,
    remote: bool,
}

/// The Godot project, its scenes, the executable and the editor's servers.
#[tauri::command]
pub async fn godot_status(app: AppHandle) -> Result<GodotStatus> {
    let (project, scenes, godot, version, host, lsp_port, dap_port, remote) = {
        let app = app.clone();
        blocking(move || {
            let state = app.state::<AppState>();
            let remote = !is_local(&state);
            let project = godot_project(&state);
            let scenes = state
                .backend()?
                .list_files(50_000)
                .map(|f| godot::project::scenes(&f))
                .unwrap_or_default();
            let godot = if remote {
                None
            } else {
                find_godot(&state, project.as_ref().is_some_and(|p| p.csharp))
            };
            let version = godot.as_ref().and_then(|g| godot_version(&state, &g.path));
            Ok((
                project,
                scenes,
                godot,
                version,
                godot_host(&state),
                int_setting(&state, "godot.lsp.port", godot_lsp::DEFAULT_LSP_PORT),
                int_setting(&state, "godot.dap.port", godot_lsp::DEFAULT_DAP_PORT),
                remote,
            ))
        })
        .await?
    };
    let probe = Duration::from_millis(400);
    let (lsp_listening, dap_listening) = if remote || project.is_none() {
        (false, false)
    } else {
        tokio::join!(
            godot_lsp::listening(&host, lsp_port, probe),
            godot_lsp::listening(&host, dap_port, probe)
        )
    };
    Ok(GodotStatus {
        project,
        scenes,
        godot,
        version,
        host,
        lsp_port,
        dap_port,
        lsp_listening,
        dap_listening,
        remote,
    })
}

fn godot_path(state: &AppState) -> Result<(String, GodotProject)> {
    require_local(state, "Godot")?;
    let project = godot_project(state).ok_or_else(|| other("The project has no project.godot"))?;
    let godot = find_godot(state, project.csharp).ok_or_else(|| {
        other("Godot was not found: set its executable in Settings | Languages & Frameworks | .NET | Godot, or put it on the PATH")
    })?;
    Ok((godot.path, project))
}

/// The command line of a Godot action (`openEditor`), run by the UI.
#[tauri::command]
pub async fn godot_command(action: String, app: AppHandle) -> Result<CommandLine> {
    blocking(move || {
        let state = app.state::<AppState>();
        let (godot, _) = godot_path(&state)?;
        let root = state.project_root()?;
        match action.as_str() {
            "openEditor" => Ok(to_command_line(
                godot_commands::editor_command(&godot, &path_string(&root)),
                &root,
            )),
            other_action => Err(other(format!("Unknown Godot action '{other_action}'"))),
        }
    })
    .await
}

/// Whether `run.rs` builds a configuration type here.
pub(crate) fn handles(type_id: &str) -> bool {
    type_id == godot_commands::GODOT_RUN
}

/// The command line of a "Godot" configuration (`run.rs`).
pub(crate) fn run_command_line(state: &AppState, cfg: &RunConfiguration) -> Result<CommandLine> {
    let (godot, _) = godot_path(state)?;
    let root = state.project_root()?;
    let spec = godot_commands::run_command(
        &godot,
        &super::dotnet::to_runtools(cfg),
        &path_string(&root),
    )
    .map_err(ForgeError::Other)?;
    let mut cmd = to_command_line(spec, &root);
    cmd.env.extend(cfg.env.clone());
    Ok(cmd)
}

fn uses_csharp_debugger(state: &AppState, cfg: &RunConfiguration, project: &GodotProject) -> bool {
    let choice = cfg
        .options
        .get("debugger")
        .cloned()
        .filter(|d| !d.is_empty() && d != "auto")
        .unwrap_or_else(|| setting(state, "godot.debugger"));
    match choice.as_str() {
        "csharp" => true,
        "gdscript" => false,
        _ => project.csharp,
    }
}

/// Debug a "Godot" configuration: GDScript through the Godot editor's debug adapter (the
/// editor runs the game), C# with netcoredbg launching Godot (after `dotnet build`).
#[tauri::command]
pub async fn godot_debug_request(
    configuration: RunConfiguration,
    app: AppHandle,
) -> Result<LaunchRequest> {
    let (csharp, host, port) = {
        let state = app.state::<AppState>();
        state.require_trusted("Running the game to debug it")?;
        let project =
            godot_project(&state).ok_or_else(|| other("The project has no project.godot"))?;
        (
            uses_csharp_debugger(&state, &configuration, &project),
            godot_host(&state),
            int_setting(&state, "godot.dap.port", godot_lsp::DEFAULT_DAP_PORT),
        )
    };
    if !csharp {
        if !godot_lsp::listening(&host, port, Duration::from_secs(1)).await {
            return Err(other(format!(
                "The Godot editor's debug adapter does not answer on {host}:{port}: open the project in the Godot editor (Tools | Godot | Open in Godot Editor) and debug again"
            )));
        }
        let state = app.state::<AppState>();
        let root = path_string(&state.project_root()?);
        let cfg = super::dotnet::to_runtools(&configuration);
        let scene = (cfg.option("target") == "scene").then(|| cfg.option("scene").to_string());
        let mut r = LaunchRequest::launch(godot_dap::GODOT_ADAPTER, "");
        r.program = None;
        r.name = Some(configuration.name.clone()).filter(|n| !n.is_empty());
        r.mode = LaunchMode::AttachServer { host, port };
        r.cwd = Some(root.clone());
        r.configuration = godot_dap::launch_arguments(
            &root,
            scene.as_deref(),
            &forge_process::split_args(&configuration.arguments),
        );
        return Ok(r);
    }
    blocking(move || {
        let state = app.state::<AppState>();
        let (godot, _) = godot_path(&state)?;
        let adapter = super::dotnet::adapter_command(&state)?;
        let backend = state.backend()?;
        let root = backend.info().root.clone();
        let csproj = root_files(backend.as_ref())
            .into_iter()
            .find(|f| f.ends_with(".csproj"))
            .ok_or_else(|| other("The Godot project has no .csproj: create the C# solution in the Godot editor (Project | Tools | C#)"))?;
        let dotnet = super::dotnet::context(&state).dotnet();
        let out = forge_backend::process_io::capture(
            backend.as_ref(),
            &to_command_line(godot_commands::csharp_build(&dotnet, &csproj), &root),
            "",
            BUILD_TIMEOUT,
        )?;
        if out.code != Some(0) {
            let report = dotnetforge_lang_dotnet::parse_build_output(
                &format!("{}\n{}", out.stdout, out.stderr),
                &path_string(&root),
            );
            let first = report
                .problems
                .iter()
                .find(|p| p.severity == dotnetforge_lang_dotnet::Severity::Error)
                .map(|p| format!("{} {}: {}", p.path, p.code, p.message))
                .unwrap_or_else(|| "see the build output".into());
            return Err(other(format!("The build failed: {first}")));
        }
        let cfg = super::dotnet::to_runtools(&configuration);
        let args = godot_commands::run_args(&cfg, &path_string(&root)).map_err(ForgeError::Other)?;
        let mut r = LaunchRequest::launch("coreclr", &godot);
        r.name = Some(configuration.name.clone()).filter(|n| !n.is_empty());
        r.args = args;
        r.cwd = Some(path_string(&root));
        r.env = configuration.env.clone();
        r.configuration = json!({
            "justMyCode": state.read_settings().get_bool("dotnet.debugger.justMyCode").unwrap_or(true)
        });
        r.adapter_command = Some(adapter);
        Ok(r)
    })
    .await
}

/// The GdUnit4 test context (`gdunit4` runs).
fn godot_test_context(state: &AppState) -> Option<gdunit::GodotTestContext> {
    let project = godot_project(state)?;
    let godot = find_godot(state, project.csharp)
        .map(|g| g.path)
        .unwrap_or_default();
    let backend = state.backend().ok()?;
    let req = forge_core::search::CollectRequest {
        extensions: vec!["gd".into()],
        max_files: 20_000,
        max_file_size: 1 << 20,
        max_total: 128 << 20,
    };
    let suites = backend
        .collect_files(&req)
        .unwrap_or_default()
        .into_iter()
        .filter(|f| !f.path.starts_with("addons/"))
        .filter_map(|f| gdunit::scan_suite(&f.path, &f.text).map(|s| (s.name, f.path)))
        .collect();
    Some(gdunit::GodotTestContext {
        godot,
        project_path: path_string(&backend.info().root),
        suites,
    })
}

// ---- Shared ------------------------------------------------------------------------------------

/// Test runs: the Unity and GdUnit4 contexts.
pub(crate) fn extend_test_context(
    state: &AppState,
    type_id: &str,
    ctx: &mut forge_testing::CommandContext,
) {
    if type_id == test_runner::UNITY_TEST {
        if let Some(c) = unity_test_context(state) {
            if let Ok(json) = serde_json::to_string(&c) {
                ctx.settings.insert(test_runner::CONTEXT_KEY.into(), json);
            }
        }
    } else if type_id == gdunit::GDUNIT {
        if let Some(c) = godot_test_context(state) {
            if let Ok(json) = serde_json::to_string(&c) {
                ctx.settings.insert(gdunit::CONTEXT_KEY.into(), json);
            }
        }
    }
}

/// The tests of a file in a Unity (C#) or Godot (GDScript) project; `None` for other files.
pub(crate) fn discover_file(
    state: &AppState,
    path: &str,
    text: &str,
) -> Option<forge_testing::TestFile> {
    if gdunit::is_gdscript(path) && enabled(state, GODOT_PLUGIN) {
        return Some(gdunit::discover_file(path, text));
    }
    if test_runner::is_unity_source(path)
        && enabled(state, UNITY_PLUGIN)
        && unity_project(state).is_some()
    {
        return Some(test_runner::discover_file(path, text));
    }
    None
}

/// The Unity and GdUnit4 test providers of the shared runner.
pub(crate) fn register_tests(tests: &mut forge_testing::TestFrameworks) {
    tests.register(Box::new(test_runner::UnityTests));
    tests.register(Box::new(gdunit::GdUnitTests));
}

/// The Godot editor's debug adapter while the Godot plugin is enabled.
pub(crate) fn sync_debug(state: &AppState, debug: &DebugState) {
    if enabled(state, GODOT_PLUGIN) {
        debug
            .dap
            .register_adapter(Arc::new(godot_dap::GodotAdapter));
    } else {
        debug.dap.unregister_adapter(godot_dap::GODOT_ADAPTER);
    }
}

/// Register the Godot editor's GDScript language server (`tcp://`) for local Godot projects.
pub fn sync_lsp_server(app: &AppHandle, detected: Option<(&Path, &forge_project::ProjectInfo)>) {
    let state = app.state::<AppState>();
    let lsp = app.state::<LspState>();
    let Ok(backend) = state.backend() else {
        lsp.unregister_server(godot_lsp::SERVER_ID);
        return;
    };
    let root = backend.info().root.clone();
    let uses = match detected {
        Some((r, _)) if r != root => return,
        Some((_, info)) => info.uses("godot"),
        None => backend
            .detect_project()
            .map(|i| i.uses("godot"))
            .unwrap_or(false),
    };
    let on = state
        .read_settings()
        .get_bool("godot.lsp.enabled")
        .unwrap_or(true);
    if !enabled(&state, GODOT_PLUGIN) || !uses || !on || backend.info().remote.is_some() {
        lsp.unregister_server(godot_lsp::SERVER_ID);
        return;
    }
    lsp.unregister_server(godot_lsp::SERVER_ID);
    lsp.register_server(godot_lsp::server_config(
        &godot_host(&state),
        int_setting(&state, "godot.lsp.port", godot_lsp::DEFAULT_LSP_PORT),
    ));
}

/// Whether a changed setting reconfigures the GDScript server.
pub fn affects_server(key: &str) -> bool {
    matches!(key, "godot.lsp.enabled" | "godot.lsp.port" | "godot.host")
}

pub fn spawn_sync_lsp_server(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || sync_lsp_server(&app, None));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_that_reconnect_the_gdscript_server() {
        assert!(affects_server("godot.lsp.port"));
        assert!(!affects_server("godot.path"));
        assert_eq!(
            to_command_line(
                CommandSpec::new("godot", vec!["--path".into()]),
                Path::new("/g")
            )
            .cwd,
            PathBuf::from("/g")
        );
    }
}
