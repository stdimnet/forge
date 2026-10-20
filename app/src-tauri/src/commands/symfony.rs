//! Symfony IPC: project snapshot (routes, services, parameters, templates, console
//! commands), completion/navigation for Symfony references, gutter marks and running
//! console commands with streamed output. Logic lives in `phpforge-fw-symfony`.
//! Offsets crossing the boundary are UTF-16 (JS strings).

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_core::{ForgeError, Result};
use phpforge_fw_symfony::complete::{self, CompletionList};
use phpforge_fw_symfony::gutter::{self, GutterMark};
use phpforge_fw_symfony::{
    console, Config, ConsoleCommand, Location, Parameter, Route, Service, Snapshot, SymfonyInfo,
    Template,
};
use phpforge_lang_php::offsets::{byte_to_utf16, utf16_to_byte};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;

/// Event with [`ConsoleEvent`] payloads for `symfony_console_run`.
pub const CONSOLE_EVENT: &str = "symfony://console";

/// How long a project snapshot is reused before files are scanned again.
const SNAPSHOT_TTL: Duration = Duration::from_secs(10);

/// Cached project snapshot and running console commands.
#[derive(Default)]
pub struct SymfonyState {
    snapshot: Mutex<Option<(PathBuf, Instant, Arc<Snapshot>)>>,
    runs: Arc<Mutex<HashMap<String, Arc<Mutex<Child>>>>>,
}

fn err(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

fn config(state: &AppState) -> Config {
    let s = state.read_settings();
    let d = Config::default();
    Config {
        app_directory: s.get_as("symfony.appDirectory").unwrap_or(d.app_directory),
        templates_directory: s
            .get_as("symfony.templatesDirectory")
            .unwrap_or(d.templates_directory),
        console_path: s.get_as("symfony.consolePath").unwrap_or(d.console_path),
        environment: s.get_as("symfony.environment").unwrap_or(d.environment),
        container_xml: s.get_as("symfony.containerXml").unwrap_or(d.container_xml),
    }
}

/// The snapshot of the open project, rescanned when stale or when `force` is set.
async fn snapshot(state: &AppState, sf: &SymfonyState, force: bool) -> Result<Arc<Snapshot>> {
    let root = state.with_workspace(|ws| Ok(ws.root().to_path_buf()))?;
    let cfg = config(state);
    if !force {
        let guard = sf.snapshot.lock().map_err(err)?;
        if let Some((r, at, snap)) = guard.as_ref() {
            if *r == root
                && at.elapsed() < SNAPSHOT_TTL
                && snap.config.environment == cfg.environment
            {
                return Ok(snap.clone());
            }
        }
    }
    let scan_root = root.clone();
    let snap =
        tauri::async_runtime::spawn_blocking(move || phpforge_fw_symfony::scan(&scan_root, &cfg))
            .await
            .map_err(err)?;
    let snap = Arc::new(snap);
    *sf.snapshot.lock().map_err(err)? = Some((root, Instant::now(), snap.clone()));
    Ok(snap)
}

/// Symfony version and counts, or `None` when the project does not use Symfony.
#[tauri::command]
pub async fn symfony_detect(
    state: State<'_, AppState>,
    sf: State<'_, SymfonyState>,
) -> Result<Option<SymfonyInfo>> {
    let root = state.with_workspace(|ws| Ok(ws.root().to_path_buf()))?;
    if !phpforge_fw_symfony::is_symfony_project(&root) {
        return Ok(None);
    }
    Ok(Some(snapshot(&state, &sf, false).await?.info.clone()))
}

/// Scan the project again (after configuration changes or a cache warmup).
#[tauri::command]
pub async fn symfony_refresh(
    state: State<'_, AppState>,
    sf: State<'_, SymfonyState>,
) -> Result<SymfonyInfo> {
    Ok(snapshot(&state, &sf, true).await?.info.clone())
}

#[tauri::command]
pub async fn symfony_routes(
    state: State<'_, AppState>,
    sf: State<'_, SymfonyState>,
) -> Result<Vec<Route>> {
    Ok(snapshot(&state, &sf, false).await?.routes.clone())
}

#[tauri::command]
pub async fn symfony_services(
    state: State<'_, AppState>,
    sf: State<'_, SymfonyState>,
) -> Result<Vec<Service>> {
    Ok(snapshot(&state, &sf, false).await?.services.clone())
}

#[tauri::command]
pub async fn symfony_parameters(
    state: State<'_, AppState>,
    sf: State<'_, SymfonyState>,
) -> Result<Vec<Parameter>> {
    Ok(snapshot(&state, &sf, false).await?.parameters.clone())
}

#[tauri::command]
pub async fn symfony_templates(
    state: State<'_, AppState>,
    sf: State<'_, SymfonyState>,
) -> Result<Vec<Template>> {
    Ok(snapshot(&state, &sf, false).await?.templates.clone())
}

/// Application commands first, then the framework/bundle commands.
#[tauri::command]
pub async fn symfony_console_commands(
    state: State<'_, AppState>,
    sf: State<'_, SymfonyState>,
) -> Result<Vec<ConsoleCommand>> {
    Ok(console::all_commands(
        &snapshot(&state, &sf, false).await?.commands,
    ))
}

/// Completion for route names, services, templates, parameters and Twig syntax.
#[tauri::command]
pub async fn symfony_complete(
    path: String,
    content: String,
    offset: usize,
    state: State<'_, AppState>,
    sf: State<'_, SymfonyState>,
) -> Result<Option<CompletionList>> {
    let snap = snapshot(&state, &sf, false).await?;
    let byte = utf16_to_byte(&content, offset);
    Ok(
        complete::complete(&snap, &path, &content, byte).map(|mut list| {
            list.from = byte_to_utf16(&content, list.from);
            list
        }),
    )
}

/// Declarations of the Symfony reference at the cursor.
#[tauri::command]
pub async fn symfony_goto(
    path: String,
    content: String,
    offset: usize,
    state: State<'_, AppState>,
    sf: State<'_, SymfonyState>,
) -> Result<Vec<Location>> {
    let snap = snapshot(&state, &sf, false).await?;
    Ok(complete::goto(
        &snap,
        &path,
        &content,
        utf16_to_byte(&content, offset),
    ))
}

/// Line markers for a PHP controller/command or a Twig template.
#[tauri::command]
pub async fn symfony_gutter(
    path: String,
    content: String,
    state: State<'_, AppState>,
    sf: State<'_, SymfonyState>,
) -> Result<Vec<GutterMark>> {
    let snap = snapshot(&state, &sf, false).await?;
    Ok(gutter::marks(&snap, &path, &content))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ConsoleEvent {
    Started {
        run_id: String,
        command_line: String,
    },
    Output {
        run_id: String,
        text: String,
        stderr: bool,
    },
    Exit {
        run_id: String,
        code: Option<i32>,
    },
}

fn emit(app: &AppHandle, event: ConsoleEvent) {
    if let Err(e) = app.emit(CONSOLE_EVENT, event) {
        tracing::warn!("could not emit {CONSOLE_EVENT}: {e}");
    }
}

/// Run `php bin/console <command> <args>` in the project root, streaming its output as
/// `symfony://console` events tagged with `runId`.
#[tauri::command]
pub fn symfony_console_run(
    command: String,
    args: Option<String>,
    run_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
    sf: State<'_, SymfonyState>,
) -> Result<()> {
    let root = state.with_workspace(|ws| Ok(ws.root().to_path_buf()))?;
    let cfg = config(&state);
    let php: String = state
        .read_settings()
        .get_as("php.interpreter")
        .unwrap_or_else(|| "php".into());
    let argv = console::command_line(
        &php,
        &cfg.console_path,
        &command,
        args.as_deref().unwrap_or(""),
    );
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .current_dir(&root)
        .env("APP_ENV", &cfg.environment)
        .env("COLUMNS", "120")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| err(format!("cannot run {}: {e}", argv[0])))?;
    emit(
        &app,
        ConsoleEvent::Started {
            run_id: run_id.clone(),
            command_line: argv.join(" "),
        },
    );
    let streams: Vec<(Box<dyn std::io::Read + Send>, bool)> = [
        child
            .stdout
            .take()
            .map(|s| (Box::new(s) as Box<dyn std::io::Read + Send>, false)),
        child
            .stderr
            .take()
            .map(|s| (Box::new(s) as Box<dyn std::io::Read + Send>, true)),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut readers = Vec::new();
    for (stream, stderr) in streams {
        let app = app.clone();
        let run_id = run_id.clone();
        readers.push(std::thread::spawn(move || {
            for line in BufReader::new(stream)
                .lines()
                .map_while(std::io::Result::ok)
            {
                emit(
                    &app,
                    ConsoleEvent::Output {
                        run_id: run_id.clone(),
                        text: line,
                        stderr,
                    },
                );
            }
        }));
    }
    let child = Arc::new(Mutex::new(child));
    sf.runs
        .lock()
        .map_err(err)?
        .insert(run_id.clone(), child.clone());
    let runs = sf.runs.clone();
    std::thread::spawn(move || {
        let code = loop {
            let status = child
                .lock()
                .ok()
                .and_then(|mut c| c.try_wait().ok().flatten());
            if let Some(s) = status {
                break s.code();
            }
            std::thread::sleep(Duration::from_millis(80));
        };
        for r in readers {
            let _ = r.join();
        }
        if let Ok(mut r) = runs.lock() {
            r.remove(&run_id);
        }
        emit(&app, ConsoleEvent::Exit { run_id, code });
    });
    Ok(())
}

/// Stop a running console command.
#[tauri::command]
pub fn symfony_console_cancel(run_id: String, sf: State<'_, SymfonyState>) -> Result<()> {
    if let Some(child) = sf.runs.lock().map_err(err)?.remove(&run_id) {
        child.lock().map_err(err)?.kill().map_err(err)?;
    }
    Ok(())
}
