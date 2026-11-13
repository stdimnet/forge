//! CForge embedded development (`packs/cforge/embedded`): PlatformIO (the tool window's
//! environments and tasks), ESP-IDF (`idf.py`), Zephyr (`west`), the "OpenOCD Download &
//! Run" and "Embedded GDB Server" run configurations (OpenOCD flashing, the GDB server and a
//! GDB session through the shared DAP layer) and the Peripherals view (CMSIS-SVD registers
//! read with the debugger's `readMemory`).
//!
//! The commands only build command lines; the UI starts them through `run_start` (refused in
//! Safe Mode) or, for interactive ones (menuconfig, `west debug`), in the terminal after
//! `require_trusted`. Debug sessions start with `dap_start`, which refuses changed adapter
//! command lines in Safe Mode.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use cforge_embedded::espidf::{self, IdfAction, IdfSetup};
use cforge_embedded::openocd::{self, OpenOcdSetup};
use cforge_embedded::platformio::{self, PioProject, PioTask};
use cforge_embedded::svd::{self, Device, DeviceSummary, RegisterValue};
use cforge_embedded::zephyr::{self, WestAction};
use cforge_embedded::{EmbeddedKinds, ToolCommand};
use forge_core::{ForgeError, Result};
use forge_dap::{LaunchMode, LaunchRequest};
use forge_process::{CommandLine, RunConfiguration};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Manager, State};

use super::cpp::{self, path_string, setting};
use crate::commands::debug::DebugState;
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// "OpenOCD Download & Run".
pub const OPENOCD: &str = "openocd";
/// "Embedded GDB Server".
pub const GDB_SERVER: &str = "embedded-gdb-server";

fn to_command_line(c: ToolCommand) -> CommandLine {
    let mut cmd = CommandLine::new(c.program).args(c.args);
    cmd.cwd = PathBuf::from(c.cwd);
    cmd.env = c.env;
    cmd
}

fn exists(backend: &dyn forge_backend::Backend, rel: &str) -> bool {
    backend.read_optional(rel).ok().flatten().is_some()
}

// ---- Detection ------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EspIdfInfo {
    /// `CONFIG_IDF_TARGET` of `sdkconfig`.
    pub target: Option<String>,
    /// `espIdf.path`, else `IDF_PATH`.
    pub idf_path: Option<String>,
    pub port: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZephyrInfo {
    /// The board of the setting, else of the last build.
    pub board: Option<String>,
    /// Boards the application defines in `boards/`.
    pub local_boards: Vec<String>,
    pub build_dir: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedInfo {
    pub kinds: EmbeddedKinds,
    pub platformio: Option<PioProject>,
    pub esp_idf: Option<EspIdfInfo>,
    pub zephyr: Option<ZephyrInfo>,
}

fn zephyr_build_dir(state: &AppState) -> String {
    setting(state, "zephyr.buildDir").unwrap_or_else(|| "build".into())
}

/// PlatformIO, ESP-IDF and Zephyr in the open project.
#[tauri::command]
pub async fn embedded_info(app: AppHandle) -> Result<EmbeddedInfo> {
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let b = backend.as_ref();
        let cmake = b.read_optional("CMakeLists.txt").ok().flatten();
        let kinds = cforge_embedded::detect(&|p| exists(b, p), cmake.as_deref());
        let platformio = kinds
            .platformio
            .then(|| b.read_optional("platformio.ini").ok().flatten())
            .flatten()
            .map(|t| platformio::parse(&t));
        let esp_idf = kinds.esp_idf.then(|| EspIdfInfo {
            target: b
                .read_optional("sdkconfig")
                .ok()
                .flatten()
                .and_then(|t| espidf::target(&t)),
            idf_path: setting(&state, "espIdf.path").or_else(|| std::env::var("IDF_PATH").ok()),
            port: setting(&state, "espIdf.port"),
        });
        let zephyr = kinds.zephyr.then(|| {
            let build = zephyr_build_dir(&state);
            let cached = b
                .read_optional(&format!("{build}/CMakeCache.txt"))
                .ok()
                .flatten()
                .and_then(|t| zephyr::board_of_cache(&t));
            let boards: Vec<String> = b
                .list_dir("boards")
                .ok()
                .map(|_| {
                    let mut paths = Vec::new();
                    collect(b, "boards", 3, &mut paths);
                    paths
                })
                .unwrap_or_default();
            ZephyrInfo {
                board: setting(&state, "zephyr.board").or(cached),
                local_boards: zephyr::local_boards(&boards),
                build_dir: build,
            }
        });
        Ok(EmbeddedInfo {
            kinds,
            platformio,
            esp_idf,
            zephyr,
        })
    })
    .await
}

/// Project files below `dir`, `depth` levels deep.
fn collect(b: &dyn forge_backend::Backend, dir: &str, depth: u32, out: &mut Vec<String>) {
    let Ok(entries) = b.list_dir(dir) else {
        return;
    };
    for e in entries {
        let path = format!("{dir}/{}", e.name);
        if e.kind == forge_core::FileKind::Dir {
            if depth > 0 {
                collect(b, &path, depth - 1, out);
            }
        } else {
            out.push(path);
        }
    }
}

// ---- PlatformIO -----------------------------------------------------------------------------

fn pio_executable(state: &AppState) -> String {
    if let Some(p) = setting(state, "platformio.path") {
        return p;
    }
    for name in ["pio", "platformio"] {
        if let Some(p) = forge_lsp::find_on_path(name) {
            return path_string(&p);
        }
    }
    // The installer's virtual environment.
    if let Some(home) = dirs::home_dir() {
        let penv = home.join(".platformio").join("penv");
        let exe = if cfg!(windows) {
            penv.join("Scripts").join("pio.exe")
        } else {
            penv.join("bin").join("pio")
        };
        if exe.is_file() {
            return path_string(&exe);
        }
    }
    "pio".into()
}

/// The command of a PlatformIO task (`pio run -e <env> -t <target>`, …).
#[tauri::command]
pub async fn pio_command(env: String, task: PioTask, app: AppHandle) -> Result<CommandLine> {
    blocking(move || {
        let state = app.state::<AppState>();
        let root = state.project_root()?;
        Ok(to_command_line(platformio::task_command(
            &pio_executable(&state),
            task,
            env.trim(),
            &path_string(&root),
        )))
    })
    .await
}

// ---- ESP-IDF and Zephyr ---------------------------------------------------------------------

/// The `idf.py` command of an action; interactive ones (menuconfig) run in the terminal and
/// need a trusted project.
#[tauri::command]
pub async fn idf_command(action: IdfAction, app: AppHandle) -> Result<CommandLine> {
    let state = app.state::<AppState>();
    if action.interactive() {
        state.require_trusted("Running idf.py")?;
    }
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<AppState>();
        let root = state.project_root()?;
        let setup = IdfSetup {
            idf_path: setting(&state, "espIdf.path").unwrap_or_default(),
            python: setting(&state, "espIdf.python").unwrap_or_default(),
            port: setting(&state, "espIdf.port").unwrap_or_default(),
            baud: setting(&state, "espIdf.baud").unwrap_or_default(),
        };
        Ok(to_command_line(espidf::command(
            action,
            &setup,
            &path_string(&root),
        )))
    })
    .await
}

/// The `west` command of an action with the chosen board (`zephyr.board` when empty).
#[tauri::command]
pub async fn west_command(
    action: WestAction,
    board: Option<String>,
    app: AppHandle,
) -> Result<CommandLine> {
    let state = app.state::<AppState>();
    if action.interactive() {
        state.require_trusted("Running west")?;
    }
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<AppState>();
        let root = state.project_root()?;
        let board = board
            .filter(|b| !b.trim().is_empty())
            .or_else(|| setting(&state, "zephyr.board"))
            .unwrap_or_default();
        if matches!(action, WestAction::Build | WestAction::Rebuild) && board.is_empty() {
            return Err(ForgeError::Other(
                "Choose the board to build for (Zephyr › Board)".into(),
            ));
        }
        let west = setting(&state, "zephyr.west").unwrap_or_else(|| "west".into());
        let app_dir = setting(&state, "zephyr.appDir").unwrap_or_else(|| ".".into());
        Ok(to_command_line(zephyr::command(
            &west,
            action,
            &board,
            &app_dir,
            &zephyr_build_dir(&state),
            &path_string(&root),
        )))
    })
    .await
}

// ---- OpenOCD and embedded GDB servers -------------------------------------------------------

fn opt<'a>(cfg: &'a RunConfiguration, key: &str) -> &'a str {
    cfg.options.get(key).map(|s| s.trim()).unwrap_or("")
}

/// The ELF of an embedded configuration: its `executable`, else its CMake target's artifact.
fn elf_of(state: &AppState, cfg: &RunConfiguration) -> Result<String> {
    cpp::app_executable(state, cfg).map_err(|e| match e {
        ForgeError::Other(m) if m.starts_with("Choose the target") => ForgeError::Other(
            "Choose the CMake target or the executable (ELF) to flash and debug".into(),
        ),
        e => e,
    })
}

fn openocd_setup(state: &AppState, cfg: &RunConfiguration) -> OpenOcdSetup {
    let config = match opt(cfg, "boardConfig") {
        "" => setting(state, "openocd.boardConfig").unwrap_or_default(),
        c => c.to_string(),
    };
    OpenOcdSetup {
        openocd: setting(state, "openocd.path").unwrap_or_else(|| "openocd".into()),
        config,
        gdb_port: opt(cfg, "gdbPort").parse().unwrap_or(3333),
        extra: forge_process::split_args(opt(cfg, "openocdArguments")),
    }
}

/// The GDB for embedded targets: the configuration's, `cpp.embedded.gdb`, the toolchain's
/// cross GDB, `gdb-multiarch`, `arm-none-eabi-gdb`, else `gdb`.
fn embedded_gdb(state: &AppState, cfg: &RunConfiguration) -> String {
    if !opt(cfg, "gdb").is_empty() {
        return opt(cfg, "gdb").to_string();
    }
    if let Some(g) = setting(state, "cpp.embedded.gdb") {
        return g;
    }
    let profile = cpp::active_profile(state);
    if let Some(d) = cpp::toolchain_of(state, Some(&profile))
        .filter(|t| t.kind == cforge_toolchains::ToolchainKind::Cross)
        .and_then(|t| t.debugger)
    {
        return d.path;
    }
    for name in ["gdb-multiarch", "arm-none-eabi-gdb"] {
        if let Some(p) = forge_lsp::find_on_path(name) {
            return path_string(&p);
        }
    }
    "gdb".into()
}

/// What starting an embedded configuration needs: the GDB server, the flashing step and the
/// debug session.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedLaunch {
    /// The GDB server to start first (OpenOCD, or the user's server).
    pub server: Option<CommandLine>,
    /// Flash the ELF before the server starts (OpenOCD's `program`).
    pub download: Option<CommandLine>,
    /// The GDB session (`target remote host:port`).
    pub request: LaunchRequest,
    /// The SVD file for the Peripherals view (absolute).
    pub svd: Option<String>,
    pub elf: String,
}

fn absolute(root: &Path, p: &str) -> String {
    if Path::new(p).is_absolute() {
        p.to_string()
    } else {
        path_string(&root.join(p))
    }
}

fn launch(state: &AppState, cfg: &RunConfiguration) -> Result<EmbeddedLaunch> {
    let root = state.project_root()?;
    let elf = elf_of(state, cfg)?;
    let (server, download, host, port) = match cfg.type_id.as_str() {
        OPENOCD => {
            let setup = openocd_setup(state, cfg);
            let server =
                openocd::server_command(&setup, &path_string(&root)).map_err(ForgeError::Other)?;
            let download = (opt(cfg, "download") != "never")
                .then(|| openocd::program_command(&setup, &elf, &path_string(&root)))
                .transpose()
                .map_err(ForgeError::Other)?;
            (
                Some(to_command_line(server)),
                download.map(to_command_line),
                "localhost".to_string(),
                setup.gdb_port,
            )
        }
        GDB_SERVER => {
            let target = match opt(cfg, "targetRemote") {
                "" => "localhost:3333",
                t => t,
            };
            let t = target.strip_prefix("tcp:").unwrap_or(target);
            let (host, port) = t
                .rsplit_once(':')
                .and_then(|(h, p)| p.parse::<u16>().ok().map(|p| (h.to_string(), p)))
                .ok_or_else(|| ForgeError::Other(format!("'{target}' is not host:port")))?;
            let server = match opt(cfg, "gdbServer") {
                "" => None,
                program => {
                    let mut c = CommandLine::new(program)
                        .args(forge_process::split_args(opt(cfg, "gdbServerArguments")));
                    c.cwd = root.clone();
                    Some(c)
                }
            };
            (server, None, host, port)
        }
        other => {
            return Err(ForgeError::Other(format!(
                "'{other}' is not an embedded configuration"
            )))
        }
    };
    let mut request = LaunchRequest::launch("gdb", &elf);
    request.mode = LaunchMode::AttachPort {
        host: host.clone(),
        port,
    };
    request.name = Some(cfg.name.clone()).filter(|n| !n.is_empty());
    request.cwd = Some(path_string(&root));
    // GDB's DAP `attach` connects with `target remote <target>`.
    request.configuration = json!({ "target": openocd::gdb_target(&host, port) });
    request.adapter_command = Some(embedded_gdb(state, cfg));
    let svd = match opt(cfg, "svdFile") {
        "" => setting(state, "cpp.embedded.svdFile"),
        f => Some(f.to_string()),
    }
    .map(|f| absolute(&root, &f));
    Ok(EmbeddedLaunch {
        server,
        download,
        request,
        svd,
        elf,
    })
}

/// The launch of an "OpenOCD Download & Run" or "Embedded GDB Server" configuration.
#[tauri::command]
pub async fn cpp_embedded_launch(
    configuration: RunConfiguration,
    app: AppHandle,
) -> Result<EmbeddedLaunch> {
    blocking(move || launch(&app.state::<AppState>(), &configuration)).await
}

// ---- SVD and the Peripherals view -----------------------------------------------------------

type SvdCache = HashMap<String, (Option<SystemTime>, Arc<Device>)>;
static SVD: Mutex<Option<SvdCache>> = Mutex::new(None);

fn load_svd(state: &AppState, path: &str) -> Result<Arc<Device>> {
    let backend = state.backend()?;
    let root = backend.info().root.clone();
    let abs = PathBuf::from(absolute(&root, path));
    let mtime = std::fs::metadata(&abs).and_then(|m| m.modified()).ok();
    let key = path_string(&abs);
    if let Some((t, d)) = SVD
        .lock()
        .expect("svd cache")
        .get_or_insert_with(HashMap::new)
        .get(&key)
    {
        if *t == mtime && mtime.is_some() {
            return Ok(d.clone());
        }
    }
    let text = match cpp::project_relative(&root, &abs) {
        Some(rel) => backend.read_file(&rel)?,
        None if backend.info().remote.is_none() => {
            std::fs::read_to_string(&abs).map_err(|e| ForgeError::io(&abs, e))?
        }
        None => return Err(ForgeError::Other(format!("Cannot read {key}"))),
    };
    let device = Arc::new(svd::parse(&text).map_err(|e| ForgeError::Other(format!("{key}: {e}")))?);
    SVD.lock()
        .expect("svd cache")
        .get_or_insert_with(HashMap::new)
        .insert(key, (mtime, device.clone()));
    Ok(device)
}

/// The peripherals of an SVD file.
#[tauri::command]
pub async fn cpp_svd_peripherals(path: String, app: AppHandle) -> Result<DeviceSummary> {
    blocking(move || Ok(load_svd(&app.state::<AppState>(), &path)?.summary())).await
}

/// Read a peripheral's registers through the debug session's `readMemory` and decode them.
#[tauri::command]
pub async fn cpp_svd_read(
    path: String,
    peripheral: String,
    session_id: String,
    app: AppHandle,
    debug: State<'_, DebugState>,
) -> Result<Vec<RegisterValue>> {
    let handle = app.clone();
    let device = blocking(move || load_svd(&handle.state::<AppState>(), &path)).await?;
    let p = device
        .peripheral(&peripheral)
        .cloned()
        .ok_or_else(|| ForgeError::Other(format!("No peripheral {peripheral} in the SVD file")))?;
    let session = crate::commands::debug::dap_session(&debug, &session_id)
        .ok_or_else(|| ForgeError::Other(format!("{session_id} is not a debug adapter session")))?;
    let mut blocks = Vec::new();
    for (offset, len) in svd::read_ranges(&p) {
        let address = format!("0x{:X}", p.base_address + offset);
        if let Ok((_, bytes, _)) = session.read_bytes(&address, 0, len).await {
            blocks.push((offset, bytes));
        }
    }
    Ok(svd::decode(&p, &blocks))
}
