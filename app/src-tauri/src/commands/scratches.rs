//! Scratches and Consoles: scratch files in `<config dir>/forge/scratches/` (logic in
//! `forge-scratches`). PHP/JS/TS scratches run as the `scratch` run configuration kind; the
//! requests of `.http` files (scratches and project files) are sent here and their responses
//! streamed to the Run tool window as `run://<runId>` events, followed by the report of their
//! response handler. Changes to the list are emitted as `scratches://changed`. In a remote
//! project, requests are sent from the project's host through an SSH tunnel unless
//! `httpClient.sendFrom` is `local`, so `localhost` URLs reach the host's services.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_backend::Backend;

use forge_core::{ForgeError, Result};
use forge_process::output::{RunInput, RunOutput, RunOutputOptions};
use forge_process::{BuildContext, CommandLine, RunConfiguration, RunConfigurationKind, RunEvent};
use forge_scratches::handler;
use forge_scratches::http::{self, Dynamic, Resolver};
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

/// `client.global` variables of the HTTP Client, kept for the IDE session like PHPStorm's.
#[derive(Default)]
pub struct HttpClientState {
    globals: Arc<Mutex<BTreeMap<String, String>>>,
    /// OAuth 2.0 tokens of the session (memory only).
    tokens: forge_scratches::oauth_client::TokenCache,
}

const ENV_FILE: &str = "http-client.env.json";
const PRIVATE_ENV_FILE: &str = "http-client.private.env.json";

/// The environment files of the open project, read through its backend (remote projects
/// keep them on their host).
fn environment_files(backend: Option<&dyn Backend>) -> (Option<String>, Option<String>) {
    let read = |name: &str| backend.and_then(|b| b.read_file(name).ok());
    (read(ENV_FILE), read(PRIVATE_ENV_FILE))
}

/// Environments of `http-client.env.json` and `http-client.private.env.json` for the
/// selector in the toolbar of `.http` editors.
#[tauri::command]
pub async fn scratches_http_environments(state: State<'_, AppState>) -> Result<http::Environments> {
    let backend = state.backend().ok();
    crate::commands::workspace::blocking(move || {
        let (public, private) = environment_files(backend.as_deref());
        Ok(http::environments(public.as_deref(), private.as_deref()))
    })
    .await
}

/// Reads files referenced by a request (`< ./body.json`, `> ./handler.js`) relative to the
/// `.http` file: next to a scratch on this machine, or through the project's backend.
fn read_referenced(
    source: Option<&str>,
    backend: Option<&dyn Backend>,
    path: &str,
) -> std::result::Result<String, String> {
    let fail = |e: &dyn std::fmt::Display| format!("Cannot read {path}: {e}");
    match source.and_then(|s| s.strip_prefix("scratch://")) {
        Some(_) => {
            let dir = store().map_err(|e| fail(&e))?.dir().to_path_buf();
            let p = PathBuf::from(path);
            let p = if p.is_absolute() { p } else { dir.join(p) };
            std::fs::read_to_string(&p).map_err(|e| fail(&e))
        }
        None => {
            let backend = backend.ok_or_else(|| fail(&"no project is open"))?;
            let dir = source
                .and_then(|s| s.rsplit_once('/').map(|(d, _)| d))
                .unwrap_or("");
            let rel = http::join_relative(dir, path)
                .ok_or_else(|| fail(&"the file is outside the project"))?;
            backend.read_file(&rel).map_err(|e| fail(&e))
        }
    }
}

/// What a request needs before it is sent: the request, environment and OAuth settings.
struct Prepared {
    file: http::HttpFile,
    index: usize,
    vars: std::collections::BTreeMap<String, String>,
    env_name: String,
    auth: BTreeMap<String, forge_scratches::oauth::OAuthConfig>,
    refs: Vec<forge_scratches::oauth::TokenRef>,
}

fn prepare(
    content: &str,
    line: usize,
    env: Option<String>,
    backend: Option<&dyn Backend>,
) -> Result<Prepared> {
    let file = http::parse(content);
    let req = file
        .request_at(line)
        .ok_or_else(|| ForgeError::Other("no request at the caret".into()))?;
    let index = req.index;
    let (public, private) = environment_files(backend);
    let vars = http::environment(public.as_deref(), private.as_deref(), env.as_deref());
    let env_name = env
        .or_else(|| http::environments(public.as_deref(), private.as_deref()).default)
        .unwrap_or_default();
    let mut text = req.url.clone();
    for h in &req.headers {
        text.push_str(&h.value);
    }
    text.push_str(req.body.as_deref().unwrap_or(""));
    let refs = forge_scratches::oauth::token_refs(&text);
    let auth = if refs.is_empty() {
        BTreeMap::new()
    } else {
        let mut all = vars.clone();
        all.extend(file.variables.clone());
        forge_scratches::oauth::configs(public.as_deref(), private.as_deref(), &env_name, &|v| {
            http::substitute(v, &all)
        })
        .map_err(ForgeError::Other)?
    };
    Ok(Prepared {
        file,
        index,
        vars,
        env_name,
        auth,
        refs,
    })
}

/// Acquire the OAuth tokens a request refers to (`{{$auth.token("id")}}`); the
/// Authorization Code grant opens the browser through the UI.
async fn tokens(
    prepared: &Prepared,
    cache: &forge_scratches::oauth_client::TokenCache,
    app: &AppHandle,
    log: &mut String,
) -> std::result::Result<BTreeMap<String, String>, String> {
    let mut values = BTreeMap::new();
    let open = {
        let app = app.clone();
        move |url: &str| {
            let _ = app.emit("http-client://open-browser", url.to_string());
        }
    };
    for r in &prepared.refs {
        let cfg = prepared.auth.get(&r.id).ok_or_else(|| {
            format!(
                "No OAuth configuration '{}' in Security.Auth of environment '{}'",
                r.id, prepared.env_name
            )
        })?;
        let (token, how) =
            forge_scratches::oauth_client::token(cfg, &prepared.env_name, cache, &open).await?;
        log.push_str(&format!("\x1b[90m{how}\x1b[0m\n"));
        let value = if r.id_token {
            token
                .id_token
                .clone()
                .ok_or_else(|| format!("The token '{}' has no id_token", r.id))?
        } else {
            token.access_token.clone()
        };
        values.insert(r.key.clone(), value);
    }
    Ok(values)
}

/// `.proto` files for gRPC requests: the `httpClient.grpc.protoFiles` setting, project
/// files or folders (folders are import roots).
fn proto_sources(state: &AppState, backend: Option<&dyn Backend>) -> BTreeMap<String, String> {
    let setting: Vec<String> = state
        .read_settings()
        .get_as(GRPC_PROTOS_KEY)
        .unwrap_or_default();
    let Some(backend) = backend else {
        return BTreeMap::new();
    };
    let mut out = BTreeMap::new();
    let all = || backend.list_files(20_000).unwrap_or_default();
    for entry in setting.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        let entry = entry.trim_end_matches('/');
        if entry.ends_with(".proto") {
            if let Ok(text) = backend.read_file(entry) {
                let name = entry.rsplit('/').next().unwrap_or(entry).to_string();
                out.insert(name, text);
            }
        } else {
            for p in all().into_iter().filter(|p| p.ends_with(".proto")) {
                if let Some(rel) = p.strip_prefix(&format!("{entry}/")) {
                    if let Ok(text) = backend.read_file(&p) {
                        out.insert(rel.to_string(), text);
                    }
                }
            }
        }
    }
    out
}

/// `httpClient.grpc.protoFiles`.
pub const GRPC_PROTOS_KEY: &str = "httpClient.grpc.protoFiles";

/// A WebSocket request as a run: console input is sent as a message, Stop closes it.
struct WebSocketRun {
    control: tokio::sync::mpsc::UnboundedSender<forge_scratches::ws::WsControl>,
}

impl forge_backend::Process for WebSocketRun {
    fn write_stdin(&self, data: &[u8]) -> Result<()> {
        self.control
            .send(forge_scratches::ws::WsControl::Send(
                String::from_utf8_lossy(data).into_owned(),
            ))
            .map_err(|_| ForgeError::Other("the connection is closed".into()))
    }

    fn close_stdin(&self) -> Result<()> {
        Ok(())
    }

    fn kill(&self, _grace: Duration) {
        let _ = self.control.send(forge_scratches::ws::WsControl::Close);
    }
}

/// Send the request of an `.http` file at `line` (0-based) of `content` and stream the
/// console text of the response and of its response handler to `run://<runId>`, like a
/// finished process. `source` is the editor path of the file (`scratch://<name>` or a
/// project path) that referenced files are relative to. `debug` adds the Xdebug session
/// cookie ("Debug" in the gutter, PHP HTTP Request debug runs). `GRAPHQL` requests are
/// sent as JSON POSTs, `WEBSOCKET` requests stay connected (console input is sent, Stop
/// closes them) and `GRPC` requests are unary calls; `{{$auth.token("id")}}` acquires
/// OAuth 2.0 tokens first.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri passes every argument and state separately.
pub async fn scratches_http_run(
    run_id: String,
    content: String,
    line: usize,
    env: Option<String>,
    source: Option<String>,
    debug: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
    http_state: State<'_, HttpClientState>,
    runs: State<'_, crate::commands::run::RunState>,
) -> Result<HttpStarted> {
    let root = state.project_root().ok();
    let backend = state.backend().ok();
    let send_from_host = state
        .read_settings()
        .get_as::<String>(SEND_FROM_KEY)
        .is_none_or(|v| v != "local");
    let tunnels = backend
        .as_ref()
        .filter(|_| send_from_host)
        .and_then(|b| b.tunnels());
    let globals = http_state.globals.clone();
    let env = env.filter(|e| !e.is_empty());
    let prepared = {
        let backend = backend.clone();
        let content = content.clone();
        crate::commands::workspace::blocking(move || {
            prepare(&content, line, env, backend.as_deref())
        })
        .await?
    };
    let method = prepared.file.requests[prepared.index].method.clone();
    let protos = if method == "GRPC" {
        proto_sources(&state, backend.as_deref())
    } else {
        BTreeMap::new()
    };
    let raw = &prepared.file.requests[prepared.index];
    let command_line = format!("{} {}", raw.method, raw.url);
    let started = HttpStarted {
        run_id: run_id.clone(),
        command_line: command_line.clone(),
    };
    let tokens_cache = http_state.tokens.clone();
    let root_text = root.map(|r| r.display().to_string());
    let output = Arc::new(Mutex::new(RunOutput::new(&RunOutputOptions {
        test_runner: false,
        root: root_text.clone(),
        cwd: root_text,
        cycle_buffer_kb: None,
        ..Default::default()
    })));
    // A WebSocket run stays open: register it like a process so Stop and console input work.
    let ws_control = (method == "WEBSOCKET").then(|| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        runs.register(
            &run_id,
            Arc::new(WebSocketRun { control: tx }),
            output.clone(),
        );
        rx
    });
    tauri::async_runtime::spawn(async move {
        let event = format!("run://{run_id}");
        let print = |text: String| {
            let rendered = output
                .lock()
                .expect("run output")
                .handle(&RunInput::Output {
                    stream: "stdout".into(),
                    text,
                });
            for r in rendered {
                emit(&app, &event, &r);
            }
        };
        emit(
            &app,
            &event,
            &RunEvent::Started {
                pid: 0,
                command_line,
            },
        );
        let mut text = String::new();
        let values = match tokens(&prepared, &tokens_cache, &app, &mut text).await {
            Ok(v) => v,
            Err(e) => {
                text.push_str(&http::render_error(&e));
                print(text);
                emit(
                    &app,
                    &event,
                    &RunEvent::Exited {
                        code: Some(1),
                        signal: None,
                    },
                );
                return;
            }
        };
        let resolved = {
            let backend = backend.clone();
            let source = source.clone();
            let snapshot = globals.lock().expect("globals lock").clone();
            let load =
                move |path: &str| read_referenced(source.as_deref(), backend.as_deref(), path);
            let req = &prepared.file.requests[prepared.index];
            let mut resolver = Resolver::new(
                &prepared.vars,
                &snapshot,
                &prepared.file.variables,
                Dynamic::system(),
            )
            .with_loader(&load)
            .with_values(values);
            let resolved = resolver.resolve(req);
            let script = resolver.handler(req);
            (resolved, script)
        };
        let (resolved, handler_script) = resolved;
        let mut request = resolved.request;
        if debug.unwrap_or(false) {
            request.add_debug_cookie(forge_debug::IDE_KEY);
        }
        text.push_str(&http::render_request(&request));
        if !resolved.problems.is_empty() {
            for p in &resolved.problems {
                text.push_str(&http::render_error(p));
            }
            print(text);
            emit(
                &app,
                &event,
                &RunEvent::Exited {
                    code: Some(1),
                    signal: None,
                },
            );
            return;
        }
        let code = match request.method.as_str() {
            "WEBSOCKET" => {
                print(std::mem::take(&mut text));
                let rx = ws_control.expect("websocket control");
                let sink = |e: forge_scratches::ws::WsEvent| print(forge_scratches::ws::render(&e));
                forge_scratches::ws::run(&request, Duration::from_secs(30), rx, &sink).await;
                0
            }
            "GRPC" => {
                match forge_scratches::grpc::call(&request, &protos, TIMEOUT).await {
                    Ok(resp) => text.push_str(&forge_scratches::grpc::render(&resp)),
                    Err(e) => text.push_str(&http::render_error(&e)),
                }
                print(text);
                0
            }
            _ => {
                let request = if request.method == "GRAPHQL" {
                    match http::graphql_as_http(&request) {
                        Ok(r) => r,
                        Err(e) => {
                            text.push_str(&http::render_error(&e));
                            print(text);
                            emit(
                                &app,
                                &event,
                                &RunEvent::Exited {
                                    code: Some(1),
                                    signal: None,
                                },
                            );
                            return;
                        }
                    }
                } else {
                    request
                };
                let sent = match &tunnels {
                    Some(t) => send_from_host_of(t.as_ref(), &request, &mut text).await,
                    None => forge_scratches::client::send(&request, TIMEOUT).await,
                };
                match sent {
                    Ok(resp) => {
                        text.push_str(&http::render_response(&resp));
                        match handler_script {
                            Ok(Some(script)) => {
                                let mut globals = globals.lock().expect("globals lock");
                                let report = handler::run(&script, &resp, &mut globals);
                                text.push_str(&handler::render(&report));
                            }
                            Ok(None) => {}
                            Err(e) => text.push_str(&http::render_error(&e)),
                        }
                    }
                    Err(e) => text.push_str(&http::render_error(&e)),
                }
                print(text);
                0
            }
        };
        emit(
            &app,
            &event,
            &RunEvent::Exited {
                code: Some(code),
                signal: None,
            },
        );
    });
    Ok(started)
}

/// "Copy as cURL": the request at `line`, resolved with the environment (OAuth tokens
/// stay as their `{{$auth.token(…)}}` placeholders).
#[tauri::command]
pub async fn scratches_http_curl(
    content: String,
    line: usize,
    env: Option<String>,
    source: Option<String>,
    state: State<'_, AppState>,
    http_state: State<'_, HttpClientState>,
) -> Result<String> {
    let backend = state.backend().ok();
    let globals = http_state.globals.lock().expect("globals lock").clone();
    crate::commands::workspace::blocking(move || {
        let p = prepare(
            &content,
            line,
            env.filter(|e| !e.is_empty()),
            backend.as_deref(),
        )?;
        let load = |path: &str| read_referenced(source.as_deref(), backend.as_deref(), path);
        let req = &p.file.requests[p.index];
        let mut resolver = Resolver::new(&p.vars, &globals, &p.file.variables, Dynamic::system())
            .with_loader(&load);
        let mut r = resolver.resolve(req).request;
        if r.method == "GRAPHQL" {
            r = http::graphql_as_http(&r).map_err(ForgeError::Other)?;
        }
        Ok(forge_scratches::curl::to_curl(&r))
    })
    .await
}

/// "Import from cURL": the `.http` text of a pasted `curl` command.
#[tauri::command]
pub fn scratches_http_from_curl(command: String) -> Result<String> {
    forge_scratches::curl::parse_curl(&command)
        .map(|r| forge_scratches::curl::to_http(&r))
        .map_err(ForgeError::Other)
}

/// Forget the OAuth tokens of the session ("Clear OAuth Tokens").
#[tauri::command]
pub fn scratches_http_clear_tokens(http_state: State<'_, HttpClientState>) {
    http_state.tokens.clear();
}

/// `httpClient.sendFrom`: `remote` (default) or `local`.
pub const SEND_FROM_KEY: &str = "httpClient.sendFrom";

const TIMEOUT: Duration = Duration::from_secs(60);

/// Send `request` from the remote project's host: a local port forwarded through the
/// project's SSH connection to the URL's host and port as the host sees them.
pub(crate) async fn send_from_host_of(
    tunnels: &dyn forge_backend::Tunnels,
    request: &http::HttpRequest,
    text: &mut String,
) -> std::result::Result<http::HttpResponse, String> {
    let (host, port) = forge_scratches::client::target(request)?;
    let forward = tunnels
        .forward_local(host, port)
        .await
        .map_err(|e| format!("Cannot open an SSH tunnel: {e}"))?;
    text.push_str(&format!(
        "\x1b[90mSent from {} through an SSH tunnel\x1b[0m\n\n",
        forward.via
    ));
    let resp = forge_scratches::client::send_via(request, TIMEOUT, forward.port).await;
    drop(forward);
    resp
}

fn emit<T: Serialize>(app: &AppHandle, event: &str, payload: &T) {
    if let Err(e) = app.emit(event, payload) {
        tracing::warn!("cannot emit {event}: {e}");
    }
}
