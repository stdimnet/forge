//! Symfony IPC: project snapshot (routes, services, parameters, templates, console
//! commands, translations, event listeners), completion/navigation for Symfony references,
//! the missing-translation inspection with its fix, translation folds, gutter marks and running
//! console commands with streamed output. Logic lives in `phpforge-fw-symfony`; the
//! analysis runs where the project's files are (`forge_backend::frameworks`) and console
//! commands run through the project backend's process API.
//! Offsets crossing the boundary are UTF-16 (JS strings).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use forge_backend::frameworks::{symfony_console_command, SymfonyQuery, TranslationAnalysis};
use forge_backend::process_io::run_lines;
use forge_backend::{framework_call, FrameworkCall};
use forge_core::{ForgeError, Result};
use phpforge_fw_symfony::complete::CompletionList;
use phpforge_fw_symfony::events::EventListener;
use phpforge_fw_symfony::translations::TranslationTable;
use phpforge_fw_symfony::gutter::GutterMark;
use phpforge_fw_symfony::{
    Config, ConsoleCommand, Location, Parameter, Route, Service, SymfonyInfo, Template,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::commands::workspace::blocking;
use crate::state::AppState;

/// Event with [`ConsoleEvent`] payloads for `symfony_console_run`.
pub const CONSOLE_EVENT: &str = "symfony://console";

/// Running console commands (cancel flags by run id).
#[derive(Default)]
pub struct SymfonyState {
    runs: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
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

/// Ask the project's backend about the Symfony snapshot (cached there for 10 s).
async fn query<T: DeserializeOwned + Send + 'static>(
    state: &AppState,
    query: SymfonyQuery,
) -> Result<T> {
    let backend = state.backend()?;
    let call = FrameworkCall::Symfony {
        config: config(state),
        query,
    };
    blocking(move || framework_call(backend.as_ref(), call)).await
}

/// Symfony version and counts, or `None` when the project does not use Symfony.
#[tauri::command]
pub async fn symfony_detect(state: State<'_, AppState>) -> Result<Option<SymfonyInfo>> {
    query(&state, SymfonyQuery::Detect).await
}

/// Scan the project again (after configuration changes or a cache warmup).
#[tauri::command]
pub async fn symfony_refresh(state: State<'_, AppState>) -> Result<SymfonyInfo> {
    query(&state, SymfonyQuery::Refresh).await
}

#[tauri::command]
pub async fn symfony_routes(state: State<'_, AppState>) -> Result<Vec<Route>> {
    query(&state, SymfonyQuery::Routes).await
}

#[tauri::command]
pub async fn symfony_services(state: State<'_, AppState>) -> Result<Vec<Service>> {
    query(&state, SymfonyQuery::Services).await
}

#[tauri::command]
pub async fn symfony_parameters(state: State<'_, AppState>) -> Result<Vec<Parameter>> {
    query(&state, SymfonyQuery::Parameters).await
}

#[tauri::command]
pub async fn symfony_templates(state: State<'_, AppState>) -> Result<Vec<Template>> {
    query(&state, SymfonyQuery::Templates).await
}

/// Application commands first, then the framework/bundle commands.
#[tauri::command]
pub async fn symfony_console_commands(state: State<'_, AppState>) -> Result<Vec<ConsoleCommand>> {
    query(&state, SymfonyQuery::ConsoleCommands).await
}

/// Completion for route names, services, templates, parameters and Twig syntax.
#[tauri::command]
pub async fn symfony_complete(
    path: String,
    content: String,
    offset: usize,
    state: State<'_, AppState>,
) -> Result<Option<CompletionList>> {
    query(
        &state,
        SymfonyQuery::Complete {
            path,
            content,
            offset,
        },
    )
    .await
}

/// Declarations of the Symfony reference at the cursor.
#[tauri::command]
pub async fn symfony_goto(
    path: String,
    content: String,
    offset: usize,
    state: State<'_, AppState>,
) -> Result<Vec<Location>> {
    query(
        &state,
        SymfonyQuery::Goto {
            path,
            content,
            offset,
        },
    )
    .await
}

/// Line markers for a PHP controller/command or a Twig template.
#[tauri::command]
pub async fn symfony_gutter(
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<Vec<GutterMark>> {
    query(&state, SymfonyQuery::Gutter { path, content }).await
}

/// Translation keys per domain with their text per locale.
#[tauri::command]
pub async fn symfony_translations(state: State<'_, AppState>) -> Result<TranslationTable> {
    query(&state, SymfonyQuery::Translations).await
}

/// Event listeners (subscribers, `#[AsEventListener]`, `kernel.event_listener` tags).
#[tauri::command]
pub async fn symfony_events(state: State<'_, AppState>) -> Result<Vec<EventListener>> {
    query(&state, SymfonyQuery::Events).await
}

/// Missing translations and the translation folds of a PHP or Twig buffer.
#[tauri::command]
pub async fn symfony_translation_analysis(
    path: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<TranslationAnalysis> {
    query(&state, SymfonyQuery::TranslationAnalysis { path, content }).await
}

/// "Create translation key": add `key` to the catalogues of `locales`; returns where.
#[tauri::command]
pub async fn symfony_create_translation(
    domain: String,
    key: String,
    locales: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<Location>> {
    query(
        &state,
        SymfonyQuery::CreateTranslation {
            domain,
            key,
            locales,
        },
    )
    .await
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

/// Run `php bin/console <command> <args>` in the project root (on the project's host),
/// streaming its output as `symfony://console` events tagged with `runId`.
#[tauri::command]
pub fn symfony_console_run(
    command: String,
    args: Option<String>,
    run_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
    sf: State<'_, SymfonyState>,
) -> Result<()> {
    state.require_trusted("Running Symfony console commands")?;
    let backend = state.backend()?;
    let cfg = config(&state);
    let php = crate::commands::php_interpreters::local_php(&state);
    let cmd = symfony_console_command(
        &backend.info().root,
        &php,
        &cfg,
        &command,
        args.as_deref().unwrap_or(""),
    );
    let cancel = Arc::new(AtomicBool::new(false));
    sf.runs
        .lock()
        .map_err(err)?
        .insert(run_id.clone(), cancel.clone());
    let runs = sf.runs.clone();
    std::thread::spawn(move || {
        let mut command_line = vec![cmd.program.clone()];
        command_line.extend(cmd.args.iter().cloned());
        emit(
            &app,
            ConsoleEvent::Started {
                run_id: run_id.clone(),
                command_line: command_line.join(" "),
            },
        );
        let code = run_lines(backend.as_ref(), &cmd, &cancel, &mut |text, stderr| {
            emit(
                &app,
                ConsoleEvent::Output {
                    run_id: run_id.clone(),
                    text,
                    stderr,
                },
            )
        });
        let code = match code {
            Ok(code) => code,
            Err(e) => {
                emit(
                    &app,
                    ConsoleEvent::Output {
                        run_id: run_id.clone(),
                        text: format!("cannot run {}: {e}", cmd.program),
                        stderr: true,
                    },
                );
                None
            }
        };
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
    if let Some(flag) = sf.runs.lock().map_err(err)?.get(&run_id) {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}
