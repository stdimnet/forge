//! Scratches and Consoles: scratch files in `<config dir>/forge/scratches/` (logic in
//! `forge-scratches`). PHP/JS/TS scratches run as the `scratch` run configuration kind; the
//! requests of `.http` scratches are sent here and their responses streamed to the Run tool
//! window as `run://<runId>` events. Changes to the list are emitted as `scratches://changed`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use forge_core::{ForgeError, Result};
use forge_process::output::{RunInput, RunOutput, RunOutputOptions};
use forge_process::{BuildContext, CommandLine, RunConfiguration, RunConfigurationKind, RunEvent};
use forge_scratches::http::{self, HttpRequest};
use forge_scratches::{
    languages, run, LanguageItem, LanguageRequest, Marker, RunContext, ScratchFile, ScratchStore,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;

fn err(e: forge_scratches::Error) -> ForgeError {
    ForgeError::Other(e.to_string())
}

/// The user's scratch store (`<config dir>/forge`).
fn store() -> Result<ScratchStore> {
    let config =
        dirs::config_dir().ok_or_else(|| ForgeError::Other("no user config directory".into()))?;
    Ok(ScratchStore::new(&ScratchStore::user_dir(&config)))
}

fn changed(app: &AppHandle) {
    if let Err(e) = app.emit("scratches://changed", ()) {
        tracing::warn!("cannot emit scratches change: {e}");
    }
}

#[tauri::command]
pub fn scratches_list() -> Result<Vec<ScratchFile>> {
    store()?.list().map_err(err)
}

/// Languages of the New Scratch File popup, ranked by `forge-scratches`.
#[tauri::command]
pub fn scratches_languages(
    request: LanguageRequest,
    state: State<'_, AppState>,
) -> Vec<LanguageItem> {
    let plugins = state.plugins.read().expect("plugins lock poisoned");
    languages::list(
        languages::languages(&plugins.file_type_registry()),
        &request,
    )
}

/// Create a scratch in `language` (a file type id) with the next free name.
#[tauri::command]
pub fn scratches_create(
    language: String,
    content: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ScratchFile> {
    let ext = {
        let plugins = state.plugins.read().expect("plugins lock poisoned");
        languages::extension_for(&plugins.file_type_registry(), &language)
    }
    .ok_or_else(|| ForgeError::Other(format!("unknown language '{language}'")))?;
    let file = store()?.create(&ext, content.as_deref()).map_err(err)?;
    changed(&app);
    Ok(file)
}

/// "New Scratch Buffer".
#[tauri::command]
pub fn scratches_create_buffer(app: AppHandle) -> Result<ScratchFile> {
    let file = store()?.create_buffer().map_err(err)?;
    changed(&app);
    Ok(file)
}

#[tauri::command]
pub fn scratches_read(name: String) -> Result<String> {
    store()?.read(&name).map_err(err)
}

#[tauri::command]
pub fn scratches_write(name: String, contents: String) -> Result<()> {
    store()?.write(&name, &contents).map_err(err)
}

#[tauri::command]
pub fn scratches_rename(name: String, new_name: String, app: AppHandle) -> Result<ScratchFile> {
    let file = store()?.rename(&name, &new_name).map_err(err)?;
    changed(&app);
    Ok(file)
}

#[tauri::command]
pub fn scratches_delete(name: String, app: AppHandle) -> Result<()> {
    store()?.delete(&name).map_err(err)?;
    changed(&app);
    Ok(())
}

/// Attach a data source to a SQL scratch (`None` detaches it).
#[tauri::command]
pub fn scratches_set_data_source(
    name: String,
    data_source_id: Option<String>,
    app: AppHandle,
) -> Result<ScratchFile> {
    let file = store()?
        .set_data_source(&name, data_source_id.as_deref())
        .map_err(err)?;
    changed(&app);
    Ok(file)
}

/// Gutter ▶ markers of a scratch's (unsaved) content.
#[tauri::command]
pub fn scratches_markers(name: String, content: String) -> Vec<Marker> {
    run::markers(&name, &content)
}

// ---- running ---------------------------------------------------------------------------

/// Run configuration kind `scratch` (option `scratch` = the scratch's name): PHP with the
/// project's Composer autoloader (unless option `useAutoloader` is `false`), Node, ….
pub struct ScratchKind;

impl RunConfigurationKind for ScratchKind {
    fn id(&self) -> &str {
        "scratch"
    }

    fn build(&self, cfg: &RunConfiguration, ctx: &BuildContext) -> Result<CommandLine> {
        let name = cfg.option("scratch");
        let path = store()?.path_of(name).map_err(err)?;
        if !path.is_file() {
            return Err(ForgeError::Other(format!(
                "scratch '{name}' does not exist"
            )));
        }
        let autoload = ctx.root.join("vendor").join("autoload.php");
        let context = RunContext {
            php_interpreter: match cfg.interpreter.trim() {
                "" => ctx.php_interpreter.clone(),
                p => p.to_string(),
            },
            project_root: Some(ctx.root.display().to_string()),
            autoloader: (cfg.option("useAutoloader") != "false" && autoload.is_file())
                .then(|| autoload.display().to_string()),
        };
        let cmd = run::run_command(&path.display().to_string(), &context).map_err(err)?;
        let mut line = CommandLine::new(cmd.program).args(cmd.args);
        line.cwd = PathBuf::from(cmd.cwd);
        line.env = cfg.env.clone();
        Ok(line)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpStarted {
    pub run_id: String,
    pub command_line: String,
}

/// Variables of the project's `http-client.env.json` (+ `.private.env.json`).
fn environment(root: Option<&Path>, env: Option<&str>) -> BTreeMap<String, String> {
    let read = |name: &str| root.and_then(|r| std::fs::read_to_string(r.join(name)).ok());
    http::environment(
        read("http-client.env.json").as_deref(),
        read("http-client.private.env.json").as_deref(),
        env,
    )
}

/// Send the request of an `.http` scratch at `line` (0-based) of `content` and stream the
/// console text of the response to `run://<runId>`, like a finished process.
#[tauri::command]
pub async fn scratches_http_run(
    run_id: String,
    content: String,
    line: usize,
    env: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<HttpStarted> {
    let root = state.with_workspace(|ws| Ok(ws.root().to_path_buf())).ok();
    let file = http::parse(&content);
    let request: HttpRequest = file
        .request_at(line)
        .map(|r| {
            http::resolve(
                r,
                &file.variables,
                &environment(root.as_deref(), env.as_deref()),
            )
        })
        .ok_or_else(|| ForgeError::Other("no request at the caret".into()))?;
    let command_line = format!("{} {}", request.method, request.url);
    let started = HttpStarted {
        run_id: run_id.clone(),
        command_line: command_line.clone(),
    };
    let root_text = root.map(|r| r.display().to_string());
    tauri::async_runtime::spawn(async move {
        let event = format!("run://{run_id}");
        let mut output = RunOutput::new(&RunOutputOptions {
            test_runner: false,
            root: root_text.clone(),
            cwd: root_text,
        });
        emit(
            &app,
            &event,
            &RunEvent::Started {
                pid: 0,
                command_line,
            },
        );
        let text = match forge_scratches::client::send(&request, Duration::from_secs(60)).await {
            Ok(resp) => http::render_request(&request) + &http::render_response(&resp),
            Err(e) => http::render_request(&request) + &http::render_error(&e),
        };
        for rendered in output.handle(&RunInput::Output {
            stream: "stdout".into(),
            text,
        }) {
            emit(&app, &event, &rendered);
        }
        emit(
            &app,
            &event,
            &RunEvent::Exited {
                code: Some(0),
                signal: None,
            },
        );
    });
    Ok(started)
}

fn emit<T: Serialize>(app: &AppHandle, event: &str, payload: &T) {
    if let Err(e) = app.emit(event, payload) {
        tracing::warn!("cannot emit {event}: {e}");
    }
}
