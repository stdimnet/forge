//! CForge v2 (`packs/cforge/analyzers`, `cforge_cmake::{sanitizers, packages}`): sanitizer
//! reports of C/C++ runs and tests, "Run with Valgrind Memcheck", "Profile with perf", and
//! the vcpkg / Conan package managers of CMake projects.
//!
//! - Sanitizers: runs of CMake Application, Google Test, Catch2 and CTest configurations get
//!   an observer (`run::run_start`) that collects the output; when the run exits, the reports
//!   it contains are emitted on `run://<runId>` as `{ type: "sanitizer", reports }` before
//!   `exited`.
//! - Valgrind and perf: `RunRequest.nativeTool` wraps the configuration's command line
//!   (`valgrind --xml=yes --xml-file=<temp>` / `perf record -g -o <temp>`); at the exit the
//!   XML is parsed (`{ type: "valgrind", report | error }`), or `perf script` turns the data
//!   into the Profiler's views (`{ type: "perf", profile: { title, view } | error }`). Both
//!   run local projects only and start through `run_start`, so Safe Mode refuses them.
//! - Packages: Conan's `conan_toolchain.cmake` (after "Conan Install") or vcpkg's toolchain
//!   file become the profile's `CMAKE_TOOLCHAIN_FILE` (`cpp::configure_command`); vcpkg
//!   port names complete in `vcpkg.json`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cforge_analyzers::{perf, project_path, sanitizer, valgrind, SanitizerReport};
use cforge_cmake::complete::Completion;
use cforge_cmake::packages::{self, Conan, PackageManagers, Vcpkg};
use cforge_cmake::Profile;
use forge_backend::process_io::capture;
use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_process::{CommandLine, RunEvent};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use super::cpp::{self, path_string, setting};
use crate::commands::spring::RunObserver;
use crate::commands::workspace::blocking;
use crate::state::AppState;

const PERF_SCRIPT_TIMEOUT: Duration = Duration::from_secs(300);
/// Output kept per run for the sanitizer reports.
const MAX_OUTPUT: usize = 32 * 1024 * 1024;

/// Configuration types whose output is searched for sanitizer reports.
const OBSERVED: [&str; 4] = [
    cforge_lang_cpp::CMAKE_APP,
    cforge_lang_cpp::GTEST,
    cforge_lang_cpp::CATCH2,
    cforge_lang_cpp::CTEST,
];

/// Configuration types that run under Valgrind or perf.
const NATIVE: [&str; 3] = [
    cforge_lang_cpp::CMAKE_APP,
    cforge_lang_cpp::GTEST,
    cforge_lang_cpp::CATCH2,
];

// ---- Sanitizers ----------------------------------------------------------------------------

pub(crate) fn observes(type_id: &str) -> bool {
    OBSERVED.contains(&type_id)
}

/// Maps a reported file onto a project file.
type Resolver = Box<dyn Fn(&str) -> Option<String>>;

/// Resolve reported files to project files: absolute paths below the root, or paths relative
/// to the project or the active profile's build folder.
fn resolver(state: &AppState) -> Resolver {
    let Ok(backend) = state.backend() else {
        return Box::new(|_| None);
    };
    let root = backend.info().root.clone();
    let build = cpp::build_dir(&root, &cpp::active_profile(state));
    let build_rel = cpp::project_relative(&root, &build).unwrap_or_default();
    let root_s = path_string(&root);
    Box::new(move |file: &str| {
        let exists = |p: &str| backend.read_optional(p).ok().flatten().is_some();
        project_path(file, &root_s, &build_rel, &exists)
    })
}

/// The sanitizer reports of a run's output, with project paths.
pub(crate) fn sanitizer_reports(state: &AppState, output: &str) -> Vec<SanitizerReport> {
    if !output.contains("Sanitizer") && !output.contains("runtime error:") {
        return Vec::new();
    }
    let mut reports = sanitizer::parse(output);
    if !reports.is_empty() {
        let resolve = resolver(state);
        sanitizer::resolve(&mut reports, &*resolve);
    }
    reports
}

/// Collect a run's output and emit its sanitizer reports when it exits.
pub(crate) fn sanitizer_observer(app: &AppHandle, run_id: &str) -> RunObserver {
    let app = app.clone();
    let event = format!("run://{run_id}");
    let output: Arc<Mutex<String>> = Arc::default();
    Arc::new(move |e: &RunEvent| match e {
        RunEvent::Output { text, .. } => {
            let mut out = output.lock().expect("sanitizer output");
            if out.len() + text.len() <= MAX_OUTPUT {
                out.push_str(text);
            }
        }
        RunEvent::Exited { .. } => {
            let text = std::mem::take(&mut *output.lock().expect("sanitizer output"));
            let reports = sanitizer_reports(&app.state::<AppState>(), &text);
            if !reports.is_empty() {
                let _ = app.emit(&event, json!({ "type": "sanitizer", "reports": reports }));
            }
        }
        RunEvent::Started { .. } => {}
    })
}

/// The sanitizer reports in a text (a console, a log file).
#[tauri::command]
pub async fn cpp_sanitizer_reports(output: String, app: AppHandle) -> Result<Vec<SanitizerReport>> {
    blocking(move || Ok(sanitizer_reports(&app.state::<AppState>(), &output))).await
}

// ---- Valgrind and perf ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeTool {
    Valgrind,
    Perf,
}

/// What to read when a Valgrind or perf run exits.
#[derive(Debug, Clone)]
pub(crate) struct NativeSetup {
    pub tool: NativeTool,
    /// The XML file or the perf data (temp dir).
    pub file: PathBuf,
    pub perf: String,
    /// The profiled program (the Profiler's title).
    pub title: String,
}

fn valgrind_path(state: &AppState) -> String {
    setting(state, "cpp.valgrind.path").unwrap_or_else(|| "valgrind".into())
}

fn perf_path(state: &AppState) -> String {
    setting(state, "cpp.perf.path").unwrap_or_else(|| "perf".into())
}

/// Why perf cannot run here (other operating systems).
pub(crate) fn perf_unsupported() -> Option<&'static str> {
    if cfg!(target_os = "linux") {
        None
    } else if cfg!(target_os = "macos") {
        Some("Profiling with perf is available on Linux only. On macOS, profile with Instruments (Xcode) or DTrace outside Forge.")
    } else {
        Some("Profiling with perf is available on Linux only. On Windows, profile in WSL or with the Visual Studio profiler.")
    }
}

fn valgrind_unsupported() -> Option<&'static str> {
    cfg!(windows).then_some(
        "Valgrind does not run on Windows. Open the project in WSL to use Valgrind Memcheck.",
    )
}

fn file_name_part(run_id: &str) -> String {
    run_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Wrap a configuration's command line for Valgrind Memcheck or perf (`run::build`).
pub(crate) fn wrap(
    state: &AppState,
    tool: NativeTool,
    type_id: Option<&str>,
    cmd: CommandLine,
    run_id: &str,
    on_target: bool,
) -> Result<(CommandLine, NativeSetup)> {
    let type_id = type_id.unwrap_or("");
    let what = match tool {
        NativeTool::Valgrind => "run with Valgrind Memcheck",
        NativeTool::Perf => "be profiled with perf",
    };
    if !NATIVE.contains(&type_id) {
        return Err(ForgeError::Other(format!(
            "'{type_id}' configurations cannot {what}"
        )));
    }
    if on_target || !cpp::is_local(state) {
        return Err(ForgeError::Other(format!("Only local runs can {what}")));
    }
    if let Some(why) = match tool {
        NativeTool::Valgrind => valgrind_unsupported(),
        NativeTool::Perf => perf_unsupported(),
    } {
        return Err(ForgeError::Other(why.into()));
    }
    let tmp = std::env::temp_dir();
    let name = file_name_part(run_id);
    let title = Path::new(&cmd.program)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| cmd.program.clone());
    let (program, args, file) = match tool {
        NativeTool::Valgrind => {
            let file = tmp.join(format!("forge-valgrind-{name}.xml"));
            let options = setting(state, "cpp.valgrind.options")
                .unwrap_or_else(|| "--leak-check=full --track-origins=yes".into());
            let args = cforge_analyzers::valgrind_args(
                &path_string(&file),
                &options,
                &cmd.program,
                &cmd.args,
            );
            (valgrind_path(state), args, file)
        }
        NativeTool::Perf => {
            let file = tmp.join(format!("forge-perf-{name}.data"));
            let s = state.read_settings();
            let options = perf::RecordOptions {
                frequency: s.get_as::<u32>("cpp.perf.frequency").unwrap_or(999),
                call_graph: s
                    .get_as::<String>("cpp.perf.callGraph")
                    .unwrap_or_else(|| "dwarf".into()),
                extra: cforge_cmake::profile::split_args(
                    &s.get_as::<String>("cpp.perf.options").unwrap_or_default(),
                ),
            };
            drop(s);
            let args = perf::record_args(&path_string(&file), &options, &cmd.program, &cmd.args);
            (perf_path(state), args, file)
        }
    };
    let _ = std::fs::remove_file(&file);
    let wrapped = CommandLine {
        program,
        args,
        cwd: cmd.cwd,
        env: cmd.env,
        stdin_file: cmd.stdin_file,
    };
    Ok((
        wrapped,
        NativeSetup {
            tool,
            file,
            perf: perf_path(state),
            title,
        },
    ))
}

/// Read a finished Valgrind or perf run: the run event for the UI.
pub(crate) fn record(
    state: &AppState,
    backend: &dyn Backend,
    setup: &NativeSetup,
) -> serde_json::Value {
    let result = match setup.tool {
        NativeTool::Valgrind => std::fs::read_to_string(&setup.file)
            .map_err(|_| "Valgrind wrote no report: is valgrind installed (Settings | Build, Execution, Deployment | Valgrind)?".to_string())
            .and_then(|xml| valgrind::parse(&xml))
            .map(|mut report| {
                let resolve = resolver(state);
                valgrind::resolve(&mut report, &*resolve);
                json!({ "type": "valgrind", "report": report })
            }),
        NativeTool::Perf => {
            if !setup.file.is_file() {
                Err("perf recorded nothing: is perf installed, and does /proc/sys/kernel/perf_event_paranoid allow profiling (2 or lower)?".to_string())
            } else {
                let mut cmd = CommandLine::new(setup.perf.clone())
                    .args(perf::script_args(&path_string(&setup.file)));
                cmd.cwd = std::env::temp_dir();
                capture(backend, &cmd, "", PERF_SCRIPT_TIMEOUT)
                    .map_err(|e| e.to_string())
                    .and_then(|out| match out.code {
                        Some(0) => perf::open(&out.stdout, None),
                        _ => Err(format!("perf script failed: {}", out.stderr.trim())),
                    })
                    .map(|view| {
                        json!({ "type": "perf", "profile": { "title": setup.title, "view": view } })
                    })
            }
        }
    };
    let _ = std::fs::remove_file(&setup.file);
    let kind = match setup.tool {
        NativeTool::Valgrind => "valgrind",
        NativeTool::Perf => "perf",
    };
    result.unwrap_or_else(|error| json!({ "type": kind, "error": error }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeToolsInfo {
    pub valgrind: Option<String>,
    pub perf: Option<String>,
    /// Why Valgrind cannot run here.
    pub valgrind_unsupported: Option<String>,
    /// Why perf cannot run here (macOS, Windows).
    pub perf_unsupported: Option<String>,
    pub remote: bool,
}

/// Where Valgrind and perf are, and whether they can run here.
#[tauri::command]
pub async fn cpp_native_tools(app: AppHandle) -> Result<NativeToolsInfo> {
    blocking(move || {
        let state = app.state::<AppState>();
        let find = |p: String| {
            forge_lsp::find_on_path(&p)
                .map(|f| path_string(&f))
                .or_else(|| Path::new(&p).is_file().then_some(p))
        };
        Ok(NativeToolsInfo {
            valgrind: find(valgrind_path(&state)),
            perf: find(perf_path(&state)),
            valgrind_unsupported: valgrind_unsupported().map(str::to_string),
            perf_unsupported: perf_unsupported().map(str::to_string),
            remote: !cpp::is_local(&state),
        })
    })
    .await
}

/// Open a Valgrind XML file (absolute or a project file) in the Valgrind results.
#[tauri::command]
pub async fn cpp_valgrind_open(path: String, app: AppHandle) -> Result<valgrind::ValgrindReport> {
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let text = if Path::new(&path).is_absolute() && cpp::is_local(&state) {
            std::fs::read_to_string(&path).map_err(|e| ForgeError::io(Path::new(&path), e))?
        } else {
            backend.read_file(&path)?
        };
        let mut report = valgrind::parse(&text).map_err(ForgeError::Other)?;
        let resolve = resolver(&state);
        valgrind::resolve(&mut report, &*resolve);
        Ok(report)
    })
    .await
}

// ---- vcpkg and Conan -----------------------------------------------------------------------

fn exists(backend: &dyn Backend, rel: &str) -> bool {
    backend.read_optional(rel).ok().flatten().is_some()
}

fn conanfile(backend: &dyn Backend) -> Option<&'static str> {
    packages::CONANFILES
        .into_iter()
        .find(|f| exists(backend, f))
}

fn vcpkg_root(state: &AppState) -> Option<String> {
    let s = setting(state, "cpp.vcpkg.root");
    packages::vcpkg_root(s.as_deref(), &|k| std::env::var(k).ok(), &|p| {
        Path::new(p).is_file()
    })
}

/// The package managers of the project and the toolchain file of `profile`.
fn package_managers(state: &AppState, root: &Path, profile: &Profile) -> PackageManagers {
    let Ok(backend) = state.backend() else {
        return PackageManagers::default();
    };
    let local = backend.info().remote.is_none();
    let mut out = PackageManagers::default();
    if let Some(conanfile) = conanfile(backend.as_ref()) {
        let build = cpp::build_dir(root, profile);
        let toolchain_file = local
            .then(|| {
                packages::conan_toolchain_candidates(&path_string(&build), &profile.build_type)
                    .into_iter()
                    .find(|c| Path::new(c).is_file())
            })
            .flatten();
        out.conan = Some(Conan {
            conanfile: conanfile.into(),
            toolchain_file,
        });
    }
    if let Some(manifest) = backend
        .read_optional(packages::VCPKG_MANIFEST)
        .ok()
        .flatten()
    {
        let root = if local { vcpkg_root(state) } else { None };
        out.vcpkg = Some(Vcpkg {
            toolchain_file: root.as_deref().map(packages::vcpkg_toolchain),
            root,
            dependencies: packages::vcpkg_dependencies(&manifest),
        });
    }
    let conan_on = cpp::flag(state, "cpp.conan.useToolchain", true);
    let vcpkg_on = cpp::flag(state, "cpp.vcpkg.useToolchain", true);
    out.toolchain_file = out
        .conan
        .as_ref()
        .filter(|_| conan_on)
        .and_then(|c| c.toolchain_file.clone())
        .or_else(|| {
            out.vcpkg
                .as_ref()
                .filter(|_| vcpkg_on)
                .and_then(|v| v.toolchain_file.clone())
        });
    out
}

/// The toolchain file a package manager adds to the profile's configure.
pub(crate) fn package_toolchain(
    state: &AppState,
    root: &Path,
    profile: &Profile,
) -> Option<String> {
    package_managers(state, root, profile).toolchain_file
}

#[tauri::command]
pub async fn cpp_packages(app: AppHandle) -> Result<PackageManagers> {
    blocking(move || {
        let state = app.state::<AppState>();
        let root = state.project_root()?;
        Ok(package_managers(
            &state,
            &root,
            &cpp::active_profile(&state),
        ))
    })
    .await
}

/// Port names of the local vcpkg's `ports/` folder (cached per root).
static PORTS: Mutex<Option<(String, Vec<String>)>> = Mutex::new(None);

fn ports(root: &str) -> Vec<String> {
    let mut cached = PORTS.lock().expect("vcpkg ports");
    if let Some((r, list)) = cached.as_ref() {
        if r == root {
            return list.clone();
        }
    }
    let mut list: Vec<String> = std::fs::read_dir(Path::new(root).join("ports"))
        .map(|it| {
            it.flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    list.sort();
    *cached = Some((root.to_string(), list.clone()));
    list
}

/// Port name completion in `vcpkg.json` (from the local vcpkg's `ports/`, if present).
#[tauri::command]
pub async fn cpp_vcpkg_complete(
    text: String,
    offset: usize,
    app: AppHandle,
) -> Result<Option<Completion>> {
    blocking(move || {
        let state = app.state::<AppState>();
        let Some(root) = vcpkg_root(&state).filter(|_| cpp::is_local(&state)) else {
            return Ok(None);
        };
        Ok(packages::complete_vcpkg(&text, offset, &ports(&root)))
    })
    .await
}

/// `conan install . -of <build> -s build_type=<type> --build=missing` for the active profile
/// (started by the UI through `run_start`).
#[tauri::command]
pub async fn cpp_conan_install_command(app: AppHandle) -> Result<CommandLine> {
    blocking(move || {
        let state = app.state::<AppState>();
        let backend = state.backend()?;
        let root = backend.info().root.clone();
        if conanfile(backend.as_ref()).is_none() {
            return Err(ForgeError::Other(
                "The project has no conanfile.txt or conanfile.py".into(),
            ));
        }
        let profile = cpp::active_profile(&state);
        let build = cpp::build_dir(&root, &profile);
        let conan = setting(&state, "cpp.conan.path").unwrap_or_else(|| "conan".into());
        let extra = cforge_cmake::profile::split_args(
            &setting(&state, "cpp.conan.arguments").unwrap_or_default(),
        );
        let mut cmd = cpp::to_command_line(packages::conan_install_command(
            &conan,
            &path_string(&root),
            &path_string(&build),
            &profile.build_type,
            &setting(&state, "cpp.conan.profile").unwrap_or_default(),
            &extra,
        ));
        cmd.env = profile.env();
        Ok(cmd)
    })
    .await
}

/// Restart clangd (a new compilation database, e.g. after PlatformIO's compiledb).
#[tauri::command]
pub fn cpp_clangd_restart(app: AppHandle) {
    cpp::restart_lsp_server(&app);
}
