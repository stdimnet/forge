//! JavaForge's Android IPC: the SDK, the Device Manager, Logcat and Android App runs. The
//! logic lives in `packs/javaforge/android`; Gradle comes from JavaForge.
//!
//! adb and the emulator are the user's own SDK tools and devices are attached to this
//! machine, so they run here (local projects only), not through the project's backend.
//! Forge never downloads the SDK: without one the UI shows `sdk::NO_SDK_HINT`.
//!
//! - `android_status`: the SDK location and tools, and the project's Android modules.
//! - `android_devices`: `adb devices -l` (with the AVD of running emulators) and
//!   `emulator -list-avds`; `android_emulator_start` starts an AVD.
//! - Logcat: `android_logcat_start` streams `adb logcat -v threadtime` of a device into a
//!   ring buffer (`Logcat`), `android_logcat_view` sets the query and returns the matching
//!   entries, new matching entries arrive on `android://logcat`; `android_logcat_stop`,
//!   `android_logcat_clear`.
//! - Runs (`run.rs`): [`command_line`] builds `android-app` configurations (Gradle
//!   `assemble<Variant>`, `adb install -r`, `am start` of the launcher activity), which run
//!   project code and touch devices: `run::start_built` refuses them in Safe Mode.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_process::{CommandLine, RunConfiguration};
use javaforge_android::adb::{self, Device};
use javaforge_android::gradle::{self, AndroidModule, ModuleKind};
use javaforge_android::logcat::{self, LogEntry, Logcat, LogcatView, Query};
use javaforge_android::manifest;
use javaforge_android::run::{self as app_run, AppRun};
use javaforge_android::sdk::{self, SdkEnv, SdkLocation};
use javaforge_build::BuildTool;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::java;
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// The Android App configuration type.
pub const APP: &str = "android-app";
/// `{ entries, cleared?, ended? }`: new entries of the stream that match the query.
pub const LOGCAT_EVENT: &str = "android://logcat";

fn windows() -> bool {
    cfg!(windows)
}

fn other(e: impl ToString) -> ForgeError {
    ForgeError::Other(e.to_string())
}

fn os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn local_only(backend: &dyn Backend) -> Result<()> {
    if backend.info().remote.is_some() {
        return Err(other(
            "Android devices are used from local projects: open the project on this machine",
        ));
    }
    Ok(())
}

// ---- SDK and modules -----------------------------------------------------------------------

/// The SDK: `android.sdk.path` (an IDE setting, never the project's), `ANDROID_HOME`,
/// `local.properties` and the default folder. In Safe Mode `local.properties` is skipped: it
/// is a project file, and Forge runs the SDK's `adb` and `emulator`.
fn find_sdk(state: &AppState, backend: &dyn Backend) -> Option<SdkLocation> {
    let setting = java::setting(state, "android.sdk.path");
    let props = if state.safe_mode.is_on() {
        None
    } else {
        backend.read_optional("local.properties").ok().flatten()
    };
    let env = |k: &str| std::env::var(k).ok();
    let home = dirs::home_dir();
    let is_sdk = |p: &Path| {
        p.join("platform-tools").is_dir()
            || p.join("platforms").is_dir()
            || p.join("emulator").is_dir()
    };
    sdk::locate(&SdkEnv {
        setting: &setting,
        env: &env,
        local_properties: props.as_deref(),
        home: home.as_deref(),
        os: os(),
        is_sdk: &is_sdk,
    })
}

fn find_adb(sdk: Option<&SdkLocation>, backend: &dyn Backend) -> Option<PathBuf> {
    sdk.map(|s| sdk::adb_path(&s.path, windows()))
        .filter(|p| p.is_file())
        .or_else(|| {
            backend
                .find_executable(&["adb".to_string()], None)
                .ok()
                .flatten()
        })
}

fn find_emulator(sdk: Option<&SdkLocation>, backend: &dyn Backend) -> Option<PathBuf> {
    sdk.map(|s| sdk::emulator_path(&s.path, windows()))
        .filter(|p| p.is_file())
        .or_else(|| {
            backend
                .find_executable(&["emulator".to_string()], None)
                .ok()
                .flatten()
        })
}

/// An Android module of the project's Gradle build.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleInfo {
    /// Folder relative to the project (`app`, empty for the root).
    pub dir: String,
    /// Gradle path (`:app`).
    pub gradle_path: String,
    #[serde(flatten)]
    pub module: AndroidModule,
}

fn android_modules(state: &AppState, backend: &dyn Backend) -> Vec<ModuleInfo> {
    let Some(build) = java::read_build(state, backend).filter(|b| b.tool == BuildTool::Gradle)
    else {
        return Vec::new();
    };
    let mut dirs: Vec<(String, String)> = vec![(String::new(), ":".to_string())];
    dirs.extend(
        build
            .modules
            .iter()
            .map(|m| (m.dir.clone(), m.path.clone())),
    );
    dirs.into_iter()
        .filter_map(|(dir, path)| {
            let text = ["build.gradle.kts", "build.gradle"].iter().find_map(|f| {
                let p = if dir.is_empty() {
                    f.to_string()
                } else {
                    format!("{dir}/{f}")
                };
                backend.read_optional(&p).ok().flatten()
            })?;
            // The root's `apply false` declarations are not a module.
            if dir.is_empty() && text.contains("apply false") {
                return None;
            }
            gradle::android_module(&text).map(|module| ModuleInfo {
                dir,
                gradle_path: path,
                module,
            })
        })
        .collect()
}

/// The project's application ids (`package:mine`).
fn application_ids(modules: &[ModuleInfo]) -> Vec<String> {
    modules
        .iter()
        .filter(|m| m.module.kind == ModuleKind::Application)
        .filter_map(|m| m.module.package().map(str::to_string))
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidStatus {
    pub sdk: Option<SdkLocation>,
    pub adb: Option<String>,
    pub emulator: Option<String>,
    /// Where to get the SDK when there is none.
    pub hint: Option<String>,
    pub modules: Vec<ModuleInfo>,
    pub remote: bool,
}

#[tauri::command]
pub async fn android_status(app: AppHandle) -> Result<AndroidStatus> {
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let sdk = find_sdk(&state, backend.as_ref());
        let adb = find_adb(sdk.as_ref(), backend.as_ref());
        let emulator = find_emulator(sdk.as_ref(), backend.as_ref());
        Ok(AndroidStatus {
            hint: sdk.is_none().then(|| sdk::NO_SDK_HINT.to_string()),
            adb: adb.map(|p| p.display().to_string()),
            emulator: emulator.map(|p| p.display().to_string()),
            sdk,
            modules: android_modules(&state, backend.as_ref()),
            remote: backend.info().remote.is_some(),
        })
    })
    .await
}

// ---- Devices -------------------------------------------------------------------------------

/// Run a tool and collect its stdout (killed after `timeout`).
fn run_tool(program: &Path, args: &[&str], timeout: Duration) -> Result<String> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    // In Flatpak the SDK tools run on the host.
    let mut child = forge_process::host::host_command(cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| other(format!("cannot run {}: {e}", program.display())))?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(other(format!("{} did not answer", program.display())));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(30)),
            Err(e) => return Err(other(e)),
        }
    }
    Ok(reader.join().unwrap_or_default())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Avd {
    pub name: String,
    /// The serial of the running emulator.
    pub serial: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicesSnapshot {
    pub devices: Vec<Device>,
    pub avds: Vec<Avd>,
    pub adb: Option<String>,
    pub emulator: Option<String>,
    pub hint: Option<String>,
    pub error: Option<String>,
}

fn devices_of(adb_path: &Path) -> Result<Vec<Device>> {
    let mut devices = adb::parse_devices(&run_tool(
        adb_path,
        &["devices", "-l"],
        Duration::from_secs(20),
    )?);
    for d in devices.iter_mut().filter(|d| d.emulator && d.online()) {
        d.avd = run_tool(
            adb_path,
            &["-s", &d.serial, "emu", "avd", "name"],
            Duration::from_secs(5),
        )
        .ok()
        .and_then(|o| adb::parse_avd_name(&o));
    }
    Ok(devices)
}

#[tauri::command]
pub async fn android_devices(app: AppHandle) -> Result<DevicesSnapshot> {
    blocking(move || devices_snapshot(&app.state::<AppState>())).await
}

const RUN_ADB: &str = "Running adb from the project";
const RUN_EMULATOR: &str = "Running the Android Emulator from the project";

fn devices_snapshot(state: &AppState) -> Result<DevicesSnapshot> {
    let backend = state.backend()?;
    local_only(backend.as_ref())?;
    let sdk = find_sdk(state, backend.as_ref());
    let adb_path = find_adb(sdk.as_ref(), backend.as_ref());
    let emulator = find_emulator(sdk.as_ref(), backend.as_ref());
    let mut error = None;
    let devices = match &adb_path {
        Some(a) => state
            .refuse_project_program(a, RUN_ADB)
            .and_then(|()| devices_of(a))
            .unwrap_or_else(|e| {
                error = Some(e.to_string());
                Vec::new()
            }),
        None => Vec::new(),
    };
    let avds = emulator
        .as_ref()
        .filter(|e| state.refuse_project_program(e, RUN_EMULATOR).is_ok())
        .and_then(|e| run_tool(e, &["-list-avds"], Duration::from_secs(20)).ok())
        .map(|o| adb::parse_avds(&o))
        .unwrap_or_default()
        .into_iter()
        .map(|name| Avd {
            serial: devices
                .iter()
                .find(|d| d.avd.as_deref() == Some(name.as_str()))
                .map(|d| d.serial.clone()),
            name,
        })
        .collect();
    let hint = if sdk.is_none() && adb_path.is_none() {
        Some(sdk::NO_SDK_HINT.to_string())
    } else {
        None
    };
    Ok(DevicesSnapshot {
        devices,
        avds,
        adb: adb_path.map(|p| p.display().to_string()),
        emulator: emulator.map(|p| p.display().to_string()),
        hint,
        error,
    })
}

/// Start an emulator (it keeps running after Forge exits, like Android Studio's).
#[tauri::command]
pub async fn android_emulator_start(avd: String, app: AppHandle) -> Result<()> {
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        local_only(backend.as_ref())?;
        let sdk = find_sdk(&state, backend.as_ref());
        let emulator = find_emulator(sdk.as_ref(), backend.as_ref()).ok_or_else(|| {
            other("The Android Emulator was not found in the SDK (emulator/emulator)")
        })?;
        state.refuse_project_program(&emulator, RUN_EMULATOR)?;
        let mut cmd = Command::new(&emulator);
        cmd.args(["-avd", avd.trim()]);
        if let Some(s) = &sdk {
            cmd.env("ANDROID_HOME", &s.path);
        }
        forge_process::host::host_command(cmd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| other(format!("cannot start the emulator: {e}")))?;
        Ok(())
    })
    .await
}

// ---- Logcat --------------------------------------------------------------------------------

struct Session {
    serial: String,
    adb: PathBuf,
    buffer: Logcat,
    query: Query,
    mine: Vec<String>,
    child: Option<Child>,
    generation: u64,
}

static LOGCAT: Mutex<Option<Session>> = Mutex::new(None);
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogcatBatch {
    serial: String,
    entries: Vec<LogEntry>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    cleared: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    ended: bool,
}

fn stop_session() {
    if let Some(mut s) = LOGCAT.lock().expect("logcat lock").take() {
        if let Some(mut c) = s.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// Stream the Logcat of a device (replaces a running stream).
#[tauri::command]
pub async fn android_logcat_start(
    serial: String,
    query: Option<String>,
    app: AppHandle,
) -> Result<()> {
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<AppState>();
        let backend = state.backend()?;
        local_only(backend.as_ref())?;
        let sdk = find_sdk(&state, backend.as_ref());
        let adb_path = find_adb(sdk.as_ref(), backend.as_ref())
            .ok_or_else(|| other("adb was not found: install the Android SDK platform-tools"))?;
        state.refuse_project_program(&adb_path, RUN_ADB)?;
        stop_session();
        let capacity = state
            .read_settings()
            .get_as::<i64>("android.logcat.bufferSize")
            .unwrap_or(20000)
            .max(100) as usize;
        let tail = state
            .read_settings()
            .get_as::<i64>("android.logcat.history")
            .unwrap_or(500)
            .clamp(1, 100_000) as u32;
        let mut buffer = Logcat::new(capacity);
        // Stack trace frames link to the project's Java and Kotlin sources.
        let sources: Vec<String> = backend
            .list_files(50_000)
            .unwrap_or_default()
            .into_iter()
            .filter(|f| f.ends_with(".kt") || f.ends_with(".java"))
            .collect();
        buffer.set_sources(sources);
        let ps = run_tool(
            &adb_path,
            &logcat::ps_args(&serial)
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            Duration::from_secs(10),
        )
        .map(|o| adb::parse_ps(&o))
        .unwrap_or_default();
        buffer.set_processes(&ps);
        let mut cmd = Command::new(&adb_path);
        cmd.args(logcat::stream_args(&serial, tail));
        let mut child = forge_process::host::host_command(cmd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| other(format!("cannot run adb logcat: {e}")))?;
        let mut stdout = child.stdout.take().expect("piped stdout");
        let generation = GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let query_text =
            query.unwrap_or_else(|| java::setting(&state, "android.logcat.defaultQuery"));
        *LOGCAT.lock().expect("logcat lock") = Some(Session {
            serial: serial.clone(),
            adb: adb_path,
            buffer,
            query: Query::parse(&query_text),
            mine: application_ids(&android_modules(&state, backend.as_ref())),
            child: Some(child),
            generation,
        });
        let emit = handle.clone();
        std::thread::spawn(move || {
            let mut chunk = vec![0u8; 64 * 1024];
            let mut pending: Vec<u8> = Vec::new();
            loop {
                let n = match stdout.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                pending.extend_from_slice(&chunk[..n]);
                let text = logcat::take_text(&mut pending);
                let mut guard = LOGCAT.lock().expect("logcat lock");
                let Some(s) = guard.as_mut().filter(|s| s.generation == generation) else {
                    return;
                };
                let new = s.buffer.push(&text);
                let entries: Vec<LogEntry> = new
                    .into_iter()
                    .filter(|e| s.query.matches(e, &s.mine))
                    .collect();
                if !entries.is_empty() {
                    let _ = emit.emit(
                        LOGCAT_EVENT,
                        LogcatBatch {
                            serial: s.serial.clone(),
                            entries,
                            cleared: false,
                            ended: false,
                        },
                    );
                }
            }
            let guard = LOGCAT.lock().expect("logcat lock");
            if let Some(s) = guard.as_ref().filter(|s| s.generation == generation) {
                let _ = emit.emit(
                    LOGCAT_EVENT,
                    LogcatBatch {
                        serial: s.serial.clone(),
                        entries: Vec::new(),
                        cleared: false,
                        ended: true,
                    },
                );
            }
        });
        Ok(())
    })
    .await
}

#[tauri::command]
pub fn android_logcat_stop() {
    stop_session();
}

/// Set the query and return the last `limit` matching entries.
#[tauri::command]
pub fn android_logcat_view(query: String, limit: Option<usize>) -> LogcatView {
    let mut guard = LOGCAT.lock().expect("logcat lock");
    let Some(s) = guard.as_mut() else {
        return LogcatView::default();
    };
    s.query = Query::parse(&query);
    s.buffer.view(&s.query, &s.mine, limit.unwrap_or(5000))
}

/// Clear the buffer here and on the device (`adb logcat -c`).
#[tauri::command]
pub async fn android_logcat_clear(app: AppHandle) -> Result<()> {
    blocking(move || {
        let (adb_path, serial) = {
            let mut guard = LOGCAT.lock().expect("logcat lock");
            let Some(s) = guard.as_mut() else {
                return Ok(());
            };
            s.buffer.clear();
            (s.adb.clone(), s.serial.clone())
        };
        let _ = run_tool(
            &adb_path,
            &["-s", &serial, "logcat", "-c"],
            Duration::from_secs(10),
        );
        let _ = app.emit(
            LOGCAT_EVENT,
            LogcatBatch {
                serial,
                entries: Vec::new(),
                cleared: true,
                ended: false,
            },
        );
        Ok(())
    })
    .await
}

// ---- Runs ----------------------------------------------------------------------------------

/// The launcher activity of a module: the merged manifest of the last build, else the
/// source manifest.
fn launcher_activity(backend: &dyn Backend, module: &ModuleInfo, variant: &str) -> Option<String> {
    let prefix = if module.dir.is_empty() {
        String::new()
    } else {
        format!("{}/", module.dir)
    };
    let mut paths: Vec<String> = manifest::merged_manifest_candidates(variant)
        .into_iter()
        .map(|p| format!("{prefix}{p}"))
        .collect();
    paths.push(format!("{prefix}src/main/AndroidManifest.xml"));
    paths.iter().find_map(|p| {
        let xml = backend.read_optional(p).ok().flatten()?;
        manifest::parse(&xml, module.module.namespace.as_deref())
            .launcher_activity()
            .map(|a| a.name.clone())
    })
}

/// The command line of an Android App configuration (`None` for other types).
pub(crate) fn command_line(
    state: &AppState,
    cfg: &RunConfiguration,
) -> Result<Option<CommandLine>> {
    if cfg.type_id != APP {
        return Ok(None);
    }
    let backend = state.backend()?;
    local_only(backend.as_ref())?;
    let root = backend.info().root.clone();
    let modules = android_modules(state, backend.as_ref());
    let wanted = cfg
        .option("module")
        .trim()
        .trim_start_matches(':')
        .to_string();
    let module = modules
        .iter()
        .filter(|m| m.module.kind == ModuleKind::Application)
        .find(|m| {
            wanted.is_empty() || m.dir == wanted || m.gradle_path.trim_start_matches(':') == wanted
        })
        .cloned()
        .ok_or_else(|| {
            other(if wanted.is_empty() {
                "No Android application module (com.android.application) was found".to_string()
            } else {
                format!("'{wanted}' is not an Android application module")
            })
        })?;
    let build = java::read_build(state, backend.as_ref())
        .ok_or_else(|| other("Android apps build with Gradle: no Gradle build was found"))?;
    let opts = java::exec_options(state, BuildTool::Gradle, &root);
    let sdk = find_sdk(state, backend.as_ref());
    let adb_path = find_adb(sdk.as_ref(), backend.as_ref())
        .ok_or_else(|| other("adb was not found: install the Android SDK platform-tools"))?;
    // Listing the devices runs adb before the run itself is refused in Safe Mode.
    state.refuse_project_program(&adb_path, RUN_ADB)?;
    let serial = match cfg.option("device").trim() {
        "" => devices_of(&adb_path)?
            .into_iter()
            .find(|d| d.online())
            .map(|d| d.serial)
            .unwrap_or_default(),
        s => s.to_string(),
    };
    let variant = match cfg.option("variant").trim() {
        "" => "debug".to_string(),
        v => v.to_string(),
    };
    let package = module.module.package().unwrap_or_default().to_string();
    let activity = match cfg.option("activity").trim() {
        "" => launcher_activity(backend.as_ref(), &module, &variant).unwrap_or_default(),
        a => manifest::qualify(a, module.module.namespace.as_deref()),
    };
    let mut gradle_args = Vec::new();
    if opts.offline {
        gradle_args.push("--offline".to_string());
    }
    gradle_args.extend(forge_process::split_args(&cfg.arguments));
    let run = AppRun {
        gradle: javaforge_build::executable(BuildTool::Gradle, build.wrapper, &opts),
        gradle_args,
        module_path: module.gradle_path.clone(),
        module_dir: module.dir.clone(),
        variant,
        apk: cfg.option("apk").trim().to_string(),
        adb: adb_path.display().to_string(),
        serial,
        package,
        activity,
        skip_build: cfg.option("skipBuild") == "true",
    };
    let steps = app_run::steps(&run).map_err(ForgeError::Other)?;
    let (program, args) = app_run::chain(&steps, windows());
    let mut cmd = CommandLine::new(program).args(args);
    cmd.cwd = root;
    if let Some(home) = java::gradle_java_home(state) {
        cmd.env.insert("JAVA_HOME".into(), home);
    }
    if let Some(s) = sdk {
        cmd.env
            .insert("ANDROID_HOME".into(), s.path.display().to_string());
    }
    cmd.env.extend(cfg.env.clone());
    Ok(Some(cmd))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn safe_mode_never_runs_the_project_sdk() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = forge_core::paths::canonicalize(dir.path())
            .unwrap()
            .join("app");
        let marker = dir.path().join("ran");
        let tools = root.join("tools/sdk");
        for (sub, name) in [("platform-tools", "adb"), ("emulator", "emulator")] {
            std::fs::create_dir_all(tools.join(sub)).unwrap();
            let tool = tools.join(sub).join(name);
            std::fs::write(&tool, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
            std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(
            root.join("local.properties"),
            format!("sdk.dir={}\n", tools.display()),
        )
        .unwrap();
        let state = AppState::for_tests(dir.path().join("trusted-projects.toml"));
        state.set_workspace(forge_core::Workspace::open(&root).unwrap());
        assert!(state.safe_mode.is_on());
        let backend = state.backend().unwrap();
        // The project's `local.properties` does not choose the SDK in Safe Mode.
        assert!(find_sdk(&state, backend.as_ref()).is_none_or(|s| !s.path.starts_with(&root)));
        let _ = devices_snapshot(&state);
        assert!(
            !marker.exists(),
            "the project's adb or emulator ran in Safe Mode"
        );
        // A tool inside the project is refused wherever it was found.
        assert!(state
            .refuse_project_program(&tools.join("platform-tools/adb"), RUN_ADB)
            .is_err());
    }
}
