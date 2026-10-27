//! Profiler snapshots (`forge-profiler`) for the shared Profiler tool window: Xdebug,
//! SPX, V8 CPU and V8 heap.
//!
//! - `profiler_open` reads a snapshot file (Tools | Analyze Xdebug Profiler Snapshot…,
//!   Open Profiler Snapshot…): a cachegrind file, an SPX report (the metadata JSON or the
//!   `.txt.gz` trace; the other file is read from the same folder), a `.cpuprofile`,
//!   `.heapprofile` or `.heapsnapshot` (streamed from disk). V8 locations go through the
//!   scripts' source maps.
//! - Profiling runs: `RunRequest.profiler` makes the run write a snapshot into a folder of
//!   its own (`test_frameworks::report_dir`: the temp dir, or `.forge/test-reports/<run>`
//!   in the project when an interpreter or run target runs elsewhere, reached through its
//!   path mappings):
//!   - `xdebug`: PHP configurations get `XDEBUG_MODE=profile` and
//!     `-dxdebug.output_dir=<folder>`;
//!   - `spx`: `SPX_ENABLED=1 SPX_REPORT=full` and `-dspx.data_dir=<folder>` (or the reports
//!     of Settings | PHP | Debug | Profiler, "SPX data directory");
//!   - `v8-cpu` / `v8-heap`: Node.js gets `--cpu-prof --cpu-prof-dir=<folder>` /
//!     `--heap-prof --heap-prof-dir=<folder>` (npm scripts through `NODE_OPTIONS`).
//!
//!   When the run exits, the newest snapshot is copied to the user cache
//!   (`<cache>/forge/profiler/<time>/`), its paths are mapped back to local ones and it is
//!   emitted on `run://<runId>` as `{ type: "profiler", snapshot, path } | { error }`.
//! - `profiler_recent` lists the snapshots kept in the cache (the newest 20).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use forge_backend::Backend;
use forge_core::{ForgeError, Result};
use forge_process::output::OutputPathMapping;
use forge_process::CommandLine;
use forge_profiler::{OpenOptions, Snapshot, SnapshotKind, SourceMap};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::State;

use super::test_frameworks::{report_dir, ReportDir};
use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Snapshots kept in the cache.
const KEEP: usize = 20;
/// Source maps larger than this are not read.
const MAX_MAP: u64 = 64 * 1024 * 1024;

/// How a run is profiled (`RunRequest.profiler`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunProfiler {
    Xdebug,
    Spx,
    V8Cpu,
    V8Heap,
}

impl RunProfiler {
    fn label(self) -> &'static str {
        match self {
            RunProfiler::Xdebug => "Xdebug",
            RunProfiler::Spx => "SPX",
            RunProfiler::V8Cpu => "V8 CPU",
            RunProfiler::V8Heap => "V8 heap",
        }
    }

    /// Whether a file of the output folder is this profiler's snapshot.
    fn matches(self, name: &str) -> bool {
        match self {
            RunProfiler::Xdebug => name.starts_with("cachegrind.out"),
            RunProfiler::Spx => name.starts_with("spx-full-") && name.ends_with(".json"),
            RunProfiler::V8Cpu => name.ends_with(".cpuprofile"),
            RunProfiler::V8Heap => name.ends_with(".heapprofile"),
        }
    }
}

/// What a profiling run reads when it exits.
#[derive(Debug, Clone)]
pub(crate) struct ProfilerSetup {
    kind: RunProfiler,
    /// The run's own output folder.
    dir: ReportDir,
    /// SPX's data directory from the settings (local; left in place).
    data_dir: Option<PathBuf>,
    started: SystemTime,
}

fn setting(state: &AppState, key: &str) -> Option<String> {
    state
        .read_settings()
        .get_as::<String>(key)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn is_node(program: &str) -> bool {
    let name = forge_profiler::file_name(program).to_ascii_lowercase();
    name == "node" || name == "node.exe"
}

/// `NODE_OPTIONS` with `options` added (a quoted value when the path has spaces).
fn node_options(current: Option<&String>, options: &[String]) -> String {
    let mut out: Vec<String> = current
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .into_iter()
        .collect();
    for o in options {
        out.push(match o.split_once('=') {
            Some((k, v)) if v.contains(' ') => format!("{k}=\"{v}\""),
            _ => o.clone(),
        });
    }
    out.join(" ")
}

/// Make a run write a snapshot. `php` is set for PHP configurations (their arguments go
/// to PHP itself), `local` when the folder can be the system temp dir.
#[allow(clippy::too_many_arguments)]
pub(crate) fn wrap(
    state: &AppState,
    kind: RunProfiler,
    type_id: Option<&str>,
    mut cmd: CommandLine,
    run_id: &str,
    root: &Path,
    local: bool,
    php: bool,
) -> Result<(CommandLine, ProfilerSetup)> {
    let what = type_id.unwrap_or("this");
    let (dir_arg, dir) = report_dir(local, root, run_id, "profile");
    let mut data_dir = None;
    match kind {
        RunProfiler::Xdebug | RunProfiler::Spx if !php => {
            return Err(ForgeError::Other(format!(
                "'{what}' configurations cannot be profiled with {}: choose a PHP configuration",
                kind.label()
            )))
        }
        RunProfiler::Xdebug => {
            let args = [
                "-dxdebug.mode=profile".to_string(),
                "-dxdebug.start_with_request=yes".to_string(),
                format!("-dxdebug.output_dir={dir_arg}"),
            ];
            cmd.args.splice(0..0, args);
            cmd.env.insert("XDEBUG_MODE".into(), "profile".into());
        }
        RunProfiler::Spx => {
            cmd.env.insert("SPX_ENABLED".into(), "1".into());
            cmd.env.insert("SPX_REPORT".into(), "full".into());
            cmd.env.insert("SPX_AUTO_START".into(), "1".into());
            match setting(state, "php.profiler.spxDataDir") {
                Some(d) if local => data_dir = Some(PathBuf::from(d)),
                _ => cmd.args.insert(0, format!("-dspx.data_dir={dir_arg}")),
            }
        }
        RunProfiler::V8Cpu | RunProfiler::V8Heap => {
            let options = if kind == RunProfiler::V8Cpu {
                vec![
                    "--cpu-prof".to_string(),
                    format!("--cpu-prof-dir={dir_arg}"),
                ]
            } else {
                vec![
                    "--heap-prof".to_string(),
                    format!("--heap-prof-dir={dir_arg}"),
                ]
            };
            let launcher = forge_profiler::file_name(&cmd.program).to_ascii_lowercase();
            let launcher = launcher.trim_end_matches(".cmd").trim_end_matches(".exe");
            if is_node(&cmd.program) {
                cmd.args.splice(0..0, options);
            } else if matches!(type_id, Some("npm" | "node"))
                || matches!(
                    launcher,
                    "npm" | "npx" | "pnpm" | "yarn" | "bun" | "tsx" | "ts-node"
                )
            {
                let value = node_options(cmd.env.get("NODE_OPTIONS"), &options);
                cmd.env.insert("NODE_OPTIONS".into(), value);
            } else {
                return Err(ForgeError::Other(format!(
                    "'{what}' configurations cannot be profiled with V8: choose a Node.js or npm configuration"
                )));
            }
        }
    }
    Ok((
        cmd,
        ProfilerSetup {
            kind,
            dir,
            data_dir,
            started: SystemTime::now(),
        },
    ))
}

fn cache_root() -> Result<PathBuf> {
    Ok(dirs::cache_dir()
        .ok_or_else(|| ForgeError::Other("no cache folder".into()))?
        .join("forge")
        .join("profiler"))
}

/// A new cache folder for a snapshot; the oldest ones beyond [`KEEP`] are removed.
fn new_cache_dir() -> Result<PathBuf> {
    let root = cache_root()?;
    std::fs::create_dir_all(&root).map_err(|e| ForgeError::io(&root, e))?;
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .map(|r| {
            r.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    while dirs.len() >= KEEP {
        let _ = std::fs::remove_dir_all(dirs.remove(0));
    }
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let dir = root.join(format!("{stamp:015}"));
    std::fs::create_dir_all(&dir).map_err(|e| ForgeError::io(&dir, e))?;
    Ok(dir)
}

/// Where a snapshot's files are read from: a local folder or a project folder.
enum Files<'a> {
    Local,
    Project(&'a dyn Backend),
}

impl Files<'_> {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        match self {
            Files::Local => std::fs::read(path).ok(),
            Files::Project(b) => b.read_bytes(path).ok(),
        }
    }
}

/// The other file of an SPX report next to `path`.
fn spx_companion(files: &Files<'_>, path: &str, data: &[u8]) -> Option<Vec<u8>> {
    let (dir, name) = match path.rfind(['/', '\\']) {
        Some(i) => (&path[..=i], &path[i + 1..]),
        None => ("", path),
    };
    if let Some(stem) = name.strip_suffix(".json") {
        let key = forge_profiler::spx::metadata(data)
            .map(|m| m.key)
            .unwrap_or_else(|_| stem.to_string());
        files
            .read(&format!("{dir}{}", forge_profiler::spx::trace_name(&key)))
            .or_else(|| files.read(&format!("{dir}{stem}.txt.gz")))
    } else {
        let key = name.trim_end_matches(".gz").trim_end_matches(".txt");
        files.read(&format!("{dir}{key}.json"))
    }
}

/// Open a snapshot: an absolute local path, or a project-relative one.
fn open_path(
    backend: &dyn Backend,
    path: &str,
    metric: Option<usize>,
) -> std::result::Result<Snapshot, String> {
    let local = Path::new(path).is_absolute();
    let files = if local {
        Files::Local
    } else {
        Files::Project(backend)
    };
    let name = forge_profiler::file_name(path);
    // Heap snapshots can be large: stream them from disk.
    if local && name.to_ascii_lowercase().ends_with(".heapsnapshot") {
        let file = std::fs::File::open(path).map_err(|e| format!("cannot read {path}: {e}"))?;
        return forge_profiler::heapsnapshot::parse_reader(file, name);
    }
    let data = files
        .read(path)
        .ok_or_else(|| format!("cannot read {path}"))?;
    let companion = match forge_profiler::detect(path, &data) {
        Some(forge_profiler::Format::SpxMetadata | forge_profiler::Format::SpxTrace) => {
            spx_companion(&files, path, &data)
        }
        _ => None,
    };
    let root = backend.info().root.to_string_lossy().replace('\\', "/");
    let remote = backend.info().remote.is_some();
    let read_text = |p: &str| -> Option<String> {
        if let Some(rel) = p.strip_prefix(&format!("{root}/")) {
            if remote {
                return backend.read_optional(rel).ok().flatten();
            }
        }
        if remote {
            return None;
        }
        let meta = std::fs::metadata(p).ok()?;
        (meta.len() <= MAX_MAP)
            .then(|| std::fs::read_to_string(p).ok())
            .flatten()
    };
    let maps = |script: &str| -> Option<SourceMap> {
        forge_profiler::sourcemap::for_script(script, &read_text)
    };
    forge_profiler::open(
        path,
        &data,
        OpenOptions {
            metric,
            companion: companion.as_deref(),
            maps: Some(&maps),
        },
    )
}

/// Open a profiler snapshot file.
#[tauri::command]
pub async fn profiler_open(
    path: String,
    metric: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Snapshot> {
    let backend = state.backend()?;
    blocking(move || open_path(backend.as_ref(), &path, metric).map_err(ForgeError::Other)).await
}

/// A snapshot kept in the cache.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentSnapshot {
    pub path: String,
    pub name: String,
    /// Milliseconds since the epoch.
    pub at: u64,
}

/// The snapshot of a cache folder (the SPX metadata, not its trace).
fn main_file(dir: &Path) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    files
        .iter()
        .find(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            !(n.starts_with("spx-full-") && n.ends_with(".txt.gz"))
        })
        .cloned()
}

/// The snapshots of earlier profiling runs, newest first.
#[tauri::command]
pub fn profiler_recent() -> Result<Vec<RecentSnapshot>> {
    let Ok(root) = cache_root() else {
        return Ok(Vec::new());
    };
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .map(|r| {
            r.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    dirs.reverse();
    Ok(dirs
        .iter()
        .filter_map(|d| {
            let file = main_file(d)?;
            let at = d.file_name()?.to_str()?.parse::<u64>().unwrap_or(0);
            Some(RecentSnapshot {
                path: file.to_string_lossy().into_owned(),
                name: file.file_name()?.to_string_lossy().into_owned(),
                at,
            })
        })
        .collect())
}

/// The snapshot files a run wrote: `(name, bytes)` of the newest one (and the SPX trace).
fn take_files(setup: &ProfilerSetup, backend: &dyn Backend) -> Vec<(String, Vec<u8>)> {
    let kind = setup.kind;
    let mut out = Vec::new();
    let local_pick = |dir: &Path, since: Option<SystemTime>| -> Option<PathBuf> {
        std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .filter(|e| kind.matches(&e.file_name().to_string_lossy()))
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .filter(|(m, _)| since.is_none_or(|s| *m >= s))
            .max_by_key(|(m, _)| *m)
            .map(|(_, p)| p)
    };
    let local_dir = match (&setup.data_dir, &setup.dir) {
        (Some(d), _) => Some((d.clone(), Some(setup.started))),
        (None, ReportDir::Local(d)) => Some((d.clone(), None)),
        _ => None,
    };
    if let Some((dir, since)) = local_dir {
        if let Some(file) = local_pick(&dir, since) {
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if let Ok(data) = std::fs::read(&file) {
                if kind == RunProfiler::Spx {
                    let path = file.to_string_lossy().into_owned();
                    if let Some(trace) = spx_companion(&Files::Local, &path, &data) {
                        let key = forge_profiler::spx::metadata(&data)
                            .map(|m| m.key)
                            .unwrap_or_default();
                        out.push((forge_profiler::spx::trace_name(&key), trace));
                    }
                }
                out.insert(0, (name, data));
            }
        }
    } else if let ReportDir::Project(rel) = &setup.dir {
        let mut best: Option<(String, Vec<u8>)> = None;
        for e in backend.list_dir(rel).unwrap_or_default() {
            if !kind.matches(&e.name) {
                continue;
            }
            if let Ok(data) = backend.read_bytes(&e.path) {
                if best.as_ref().is_none_or(|(_, b)| data.len() > b.len()) {
                    best = Some((e.path.clone(), data));
                }
            }
        }
        if let Some((path, data)) = best {
            if kind == RunProfiler::Spx {
                if let Some(trace) = spx_companion(&Files::Project(backend), &path, &data) {
                    let key = forge_profiler::spx::metadata(&data)
                        .map(|m| m.key)
                        .unwrap_or_default();
                    out.push((forge_profiler::spx::trace_name(&key), trace));
                }
            }
            out.insert(0, (forge_profiler::file_name(&path).to_string(), data));
        }
    }
    if setup.data_dir.is_none() {
        setup.dir.remove(backend);
    }
    out
}

/// Read a finished profiling run: the run event for the UI.
pub(crate) fn record(
    backend: &dyn Backend,
    setup: &ProfilerSetup,
    mappings: &[OutputPathMapping],
) -> serde_json::Value {
    let result = (|| -> std::result::Result<(Snapshot, String), String> {
        let files = take_files(setup, backend);
        let Some((name, _)) = files.first().cloned() else {
            return Err(match setup.kind {
                RunProfiler::Xdebug => "Xdebug wrote no snapshot: is Xdebug 3 installed and loaded by the interpreter?".to_string(),
                RunProfiler::Spx => "SPX wrote no report: is the SPX extension installed and loaded by the interpreter?".to_string(),
                _ => "Node.js wrote no profile: did the program exit normally (Node.js 12 or newer)?".to_string(),
            });
        };
        let dir = new_cache_dir().map_err(|e| e.to_string())?;
        for (n, data) in &files {
            std::fs::write(dir.join(n), data).map_err(|e| e.to_string())?;
        }
        let path = dir.join(&name).to_string_lossy().into_owned();
        let mut snapshot = open_path(backend, &path, None)?;
        map_back(&mut snapshot, mappings);
        Ok((snapshot, path))
    })();
    match result {
        Ok((snapshot, path)) => json!({ "type": "profiler", "snapshot": snapshot, "path": path }),
        Err(error) => json!({ "type": "profiler", "profiler": setup.kind, "error": error }),
    }
}

/// Container / remote paths of a snapshot to local ones.
fn map_back(snapshot: &mut Snapshot, mappings: &[OutputPathMapping]) {
    if mappings.is_empty() || snapshot.kind == SnapshotKind::V8Heap {
        return;
    }
    snapshot.map_files(&|f: &str| {
        mappings.iter().find_map(|m| {
            let remote = m.remote.trim_end_matches('/');
            f.strip_prefix(remote)
                .filter(|rest| rest.is_empty() || rest.starts_with('/'))
                .map(|rest| format!("{}{rest}", m.local.trim_end_matches('/')))
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_options_quote_paths_with_spaces() {
        let o = node_options(
            Some(&"--max-old-space-size=4096".to_string()),
            &["--cpu-prof".into(), "--cpu-prof-dir=/tmp/a b".into()],
        );
        assert_eq!(
            o,
            "--max-old-space-size=4096 --cpu-prof --cpu-prof-dir=\"/tmp/a b\""
        );
        assert!(is_node("/usr/bin/node") && is_node("C:\\node\\node.exe") && !is_node("npm"));
        assert!(RunProfiler::Xdebug.matches("cachegrind.out.123.gz"));
        assert!(
            RunProfiler::Spx.matches("spx-full-1.json")
                && !RunProfiler::Spx.matches("spx-full-1.txt.gz")
        );
    }

    #[test]
    fn maps_container_paths_back() {
        let mut s = forge_profiler::demo::snapshot(SnapshotKind::Xdebug, "/opt/project", None);
        map_back(
            &mut s,
            &[OutputPathMapping {
                remote: "/opt/project".into(),
                local: "/home/me/shop".into(),
            }],
        );
        assert!(s
            .functions
            .iter()
            .any(|f| f.file == "/home/me/shop/app/Services/PriceCalculator.php"));
        assert!(s.functions.iter().any(|f| f.file == "php:internal"));
    }
}
