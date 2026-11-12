//! Jupyter notebooks (PyCharm's Jupyter support); the logic lives in
//! `packs/pyforge/jupyter`:
//!
//! - Notebooks: `.ipynb` into the editor's model with rendered outputs, round-trip stable
//!   serialization, outputs rendered as they arrive, DataFrame pages, notebook diffs.
//! - Kernels: `ipykernel` started in the project's (or a chosen) local interpreter with a
//!   connection file in Forge's cache folder, or a kernel on a Jupyter server (URL in the
//!   `jupyter.server.url` setting, token in the OS keychain). Events stream on
//!   `jupyter://<kernel id>`. Running a kernel runs project code: refused in Safe Mode.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_backend::Process;
use forge_core::{ForgeError, Result};
use forge_process::{CommandLine, RunEvent, RunSink};
use pyforge_jupyter::dataframe::{self, DataFrame, Page, Query};
use pyforge_jupyter::kernel::{free_ports, new_key, write_connection_file};
use pyforge_jupyter::protocol::ConnectionInfo;
use pyforge_jupyter::server::{kernel_specs, split_server_url, KernelSpecInfo, ServerAddress};
use pyforge_jupyter::session::{EventSink, Session};
use pyforge_jupyter::{secrets, Notebook, NotebookDiff, NotebookView, Output, Rendered, Variable};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::commands::python::{self, python_command, run_captured, setting_string};
use crate::commands::workspace::blocking;
use crate::state::AppState;

const SERVER_KEY: &str = "jupyter.server.url";
const START_TIMEOUT: Duration = Duration::from_secs(60);

fn err(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

fn root_string(state: &AppState) -> Option<String> {
    state
        .project_root()
        .ok()
        .map(|r| r.to_string_lossy().replace('\\', "/"))
}

/// Read a notebook for the editor (empty text: a new notebook).
#[tauri::command]
pub async fn jupyter_open(text: String, state: State<'_, AppState>) -> Result<NotebookView> {
    let root = root_string(&state);
    blocking(move || pyforge_jupyter::open(&text, root.as_deref()).map_err(err)).await
}

/// The `.ipynb` text of the editor's notebook, keeping what did not change in `original`.
#[tauri::command]
pub async fn jupyter_serialize(notebook: Notebook, original: Option<String>) -> Result<String> {
    blocking(move || Ok(pyforge_jupyter::serialize(&notebook, original.as_deref()))).await
}

#[tauri::command]
pub async fn jupyter_render(
    outputs: Vec<Output>,
    state: State<'_, AppState>,
) -> Result<Vec<Rendered>> {
    let root = root_string(&state);
    Ok(pyforge_jupyter::render_outputs(&outputs, root.as_deref()))
}

/// Cell-by-cell diff of two `.ipynb` texts.
#[tauri::command]
pub async fn jupyter_diff(left: String, right: String) -> Result<NotebookDiff> {
    blocking(move || pyforge_jupyter::diff_texts(&left, &right).map_err(err)).await
}

#[tauri::command]
pub async fn jupyter_dataframe_query(frame: DataFrame, query: Query) -> Result<Page> {
    blocking(move || Ok(dataframe::query(&frame, &query))).await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpykernelStatus {
    pub installed: bool,
    pub version: Option<String>,
    /// `Python 3.12 (shop-api)`.
    pub interpreter: String,
    /// Kernels of this interpreter cannot be started by Forge (Docker, SSH, WSL, remote
    /// projects): use a Jupyter server.
    pub needs_server: bool,
}

/// Whether ipykernel is installed in the interpreter (the project's when empty).
#[tauri::command]
pub async fn jupyter_kernel_status(
    interpreter: Option<String>,
    state: State<'_, AppState>,
) -> Result<IpykernelStatus> {
    let (interp, _) = python::resolve(&state, interpreter.as_deref().unwrap_or(""))
        .ok_or_else(python::no_interpreter)?;
    let label = interp.label();
    if interp.kind.is_remote() || state.is_remote() {
        return Ok(IpykernelStatus {
            installed: false,
            version: None,
            interpreter: label,
            needs_server: true,
        });
    }
    let backend = state.backend()?;
    let cmd = python_command(
        &state,
        backend.as_ref(),
        &interp,
        &["-c", "import ipykernel; print(ipykernel.__version__)"],
    )?;
    blocking(move || {
        let out = run_captured(backend.as_ref(), &cmd, Duration::from_secs(60));
        Ok(IpykernelStatus {
            installed: out.is_ok(),
            version: out
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            interpreter: label,
            needs_server: false,
        })
    })
    .await
}

/// Where to start a kernel.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelRequest {
    /// The notebook (workspace-relative); the kernel starts in its folder.
    pub path: String,
    /// A local interpreter (path or configured name); empty = the project's.
    #[serde(default)]
    pub interpreter: Option<String>,
    /// A Jupyter server URL instead (empty = the `jupyter.server.url` setting, if any).
    #[serde(default)]
    pub server: Option<String>,
    /// Kernel spec on the server (empty = the server's default).
    #[serde(default)]
    pub kernel_name: Option<String>,
    /// The kernel's id, chosen by the caller so that it can listen on `jupyter://<id>` before
    /// the kernel starts and misses none of its first events (letters, digits and `-`);
    /// empty = Forge picks one.
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelStarted {
    pub id: String,
    /// `Python 3.12 (shop-api)` or `python3 on http://localhost:8888`.
    pub label: String,
    pub server: bool,
}

struct Kernel {
    session: Arc<Session>,
    process: Option<Arc<dyn Process>>,
    connection_file: Option<PathBuf>,
    request: KernelRequest,
    label: String,
}

impl Kernel {
    fn stop_process(&self) {
        if let Some(p) = &self.process {
            p.kill(Duration::from_secs(2));
        }
        if let Some(f) = &self.connection_file {
            let _ = std::fs::remove_file(f);
        }
    }
}

/// Running kernels by id.
#[derive(Default)]
pub struct JupyterState {
    kernels: Mutex<HashMap<String, Arc<Kernel>>>,
    next: AtomicU64,
}

impl JupyterState {
    fn get(&self, id: &str) -> Result<Arc<Kernel>> {
        self.kernels
            .lock()
            .expect("kernels")
            .get(id)
            .cloned()
            .ok_or_else(|| ForgeError::Other(format!("The kernel {id} is not running")))
    }
}

fn event_name(id: &str) -> String {
    format!("jupyter://{id}")
}

fn emit<T: Serialize>(app: &AppHandle, event: &str, payload: &T) {
    if let Err(e) = app.emit(event, payload) {
        tracing::warn!("cannot emit {event}: {e}");
    }
}

fn server_address(state: &AppState, request: &KernelRequest) -> Result<Option<ServerAddress>> {
    let url = request
        .server
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| setting_string(state, SERVER_KEY));
    if url.trim().is_empty() {
        return Ok(None);
    }
    let mut address = split_server_url(&url, "").map_err(ForgeError::Other)?;
    if address.token.is_empty() {
        address.token = secrets::token(&address.base).unwrap_or_default();
    }
    Ok(Some(address))
}

/// Start a kernel for `id` (a new one, or a restart of a local kernel).
async fn start(
    id: &str,
    request: KernelRequest,
    app: &AppHandle,
    state: &AppState,
) -> Result<Kernel> {
    state.require_trusted("Starting a Jupyter kernel")?;
    let event = event_name(id);
    let sink_app = app.clone();
    let sink_event = event.clone();
    let sink: EventSink = Arc::new(move |e| emit(&sink_app, &sink_event, &e));
    let root = root_string(state);
    if let Some(server) = server_address(state, &request)? {
        let kernel_name = request.kernel_name.clone().unwrap_or_default();
        let label = format!(
            "{} on {}",
            if kernel_name.is_empty() {
                "Kernel"
            } else {
                &kernel_name
            },
            server.base
        );
        let session = Session::connect_server(server, &kernel_name, root, sink, START_TIMEOUT)
            .await
            .map_err(err)?;
        return Ok(Kernel {
            session: Arc::new(session),
            process: None,
            connection_file: None,
            request,
            label,
        });
    }
    let (interp, _) = python::resolve(state, request.interpreter.as_deref().unwrap_or(""))
        .ok_or_else(python::no_interpreter)?;
    if interp.kind.is_remote() || state.is_remote() {
        return Err(ForgeError::Other(
            "Kernels of Docker, SSH and WSL interpreters and of remote projects run on a Jupyter server: set its URL in Settings | Languages & Frameworks | Jupyter".into(),
        ));
    }
    let backend = state.backend()?;
    let project = backend.info().root.clone();
    let folder = std::path::Path::new(&request.path)
        .parent()
        .map(|p| project.join(p))
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| project.clone());
    let key = new_key();
    let info = ConnectionInfo::local(free_ports().map_err(err)?, &key);
    let file = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("forge")
        .join("jupyter")
        .join(format!("kernel-{key}.json"));
    write_connection_file(&file, &info).map_err(err)?;
    let mut cmd = CommandLine::new(interp.python()).args([
        "-m".to_string(),
        "ipykernel_launcher".to_string(),
        "-f".to_string(),
        file.to_string_lossy().into_owned(),
    ]);
    cmd.cwd = folder;
    cmd.env.insert("PYTHONIOENCODING".into(), "utf-8".into());
    cmd.env
        .insert("PYDEVD_DISABLE_FILE_VALIDATION".into(), "1".into());
    let exit_app = app.clone();
    let exit_event = event.clone();
    // Set when the kernel process exits: a kernel that exited before Forge connected did
    // not answer on its ports, another program did.
    let (exit_tx, mut exit_rx) = tokio::sync::watch::channel(None::<Option<i32>>);
    let process_sink: RunSink = Arc::new(move |e: RunEvent| match e {
        RunEvent::Output { text, .. } => tracing::debug!("kernel: {}", text.trim_end()),
        RunEvent::Exited { code, .. } => {
            exit_tx.send_replace(Some(code));
            emit(
                &exit_app,
                &exit_event,
                &serde_json::json!({ "type": "exited", "code": code }),
            )
        }
        RunEvent::Started { .. } => {}
    });
    let start_backend = backend.clone();
    let process = blocking(move || start_backend.start_process(&cmd, process_sink)).await?;
    let exited = |code: Option<i32>| match code {
        Some(code) => format!("the kernel exited with code {code}"),
        None => "the kernel exited".to_string(),
    };
    let connected = tokio::select! {
        r = Session::connect_zmq(&info, root, sink, START_TIMEOUT) => r.map_err(|e| e.to_string()),
        code = async {
            let exit = exit_rx.wait_for(Option::is_some).await.map(|code| *code);
            match exit {
                Ok(code) => code.flatten(),
                // The process ended without reporting an exit: let the connection time out.
                Err(_) => std::future::pending().await,
            }
        } => Err(exited(code)),
    };
    // Connected, but the kernel exited meanwhile: whatever answered was not this kernel.
    let exit = *exit_rx.borrow();
    let connected = match (connected, exit) {
        (Ok(_), Some(code)) => Err(exited(code)),
        (connected, _) => connected,
    };
    match connected {
        Ok(session) => Ok(Kernel {
            session: Arc::new(session),
            process: Some(process),
            connection_file: Some(file),
            request,
            label: interp.label(),
        }),
        Err(e) => {
            process.kill(Duration::from_millis(500));
            let _ = std::fs::remove_file(&file);
            Err(ForgeError::Other(format!(
                "The kernel did not start: {e}. Is ipykernel installed in {}?",
                interp.label()
            )))
        }
    }
}

/// Start a kernel for a notebook. Its events arrive on `jupyter://<id>`.
#[tauri::command]
pub async fn jupyter_kernel_start(
    request: KernelRequest,
    app: AppHandle,
    state: State<'_, AppState>,
    kernels: State<'_, JupyterState>,
) -> Result<KernelStarted> {
    let id = match request.id.as_deref().filter(|id| !id.is_empty()) {
        Some(id) => {
            let plain = id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
            if !plain || kernels.kernels.lock().expect("kernels").contains_key(id) {
                return Err(ForgeError::Other(format!("invalid kernel id: {id}")));
            }
            id.to_string()
        }
        None => format!("kernel-{}", kernels.next.fetch_add(1, Ordering::SeqCst) + 1),
    };
    let kernel = start(&id, request, &app, &state).await?;
    let started = KernelStarted {
        id: id.clone(),
        label: kernel.label.clone(),
        server: kernel.session.is_server(),
    };
    kernels
        .kernels
        .lock()
        .expect("kernels")
        .insert(id, Arc::new(kernel));
    Ok(started)
}

/// Run a cell; returns the request id.
#[tauri::command]
pub async fn jupyter_execute(
    id: String,
    cell_id: String,
    code: String,
    state: State<'_, AppState>,
    kernels: State<'_, JupyterState>,
) -> Result<String> {
    state.require_trusted("Running notebook cells")?;
    let k = kernels.get(&id)?;
    k.session.execute(&cell_id, &code).await.map_err(err)
}

#[tauri::command]
pub async fn jupyter_interrupt(id: String, kernels: State<'_, JupyterState>) -> Result<()> {
    let k = kernels.get(&id)?;
    k.session.interrupt().await.map_err(err)
}

/// Restart the kernel (same id; the notebook keeps listening).
#[tauri::command]
pub async fn jupyter_restart(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
    kernels: State<'_, JupyterState>,
) -> Result<KernelStarted> {
    let k = kernels.get(&id)?;
    emit(
        &app,
        &event_name(&id),
        &serde_json::json!({ "type": "status", "state": "restarting" }),
    );
    if k.session.restart().await.map_err(err)? {
        return Ok(KernelStarted {
            id,
            label: k.label.clone(),
            server: true,
        });
    }
    let _ = k.session.shutdown().await;
    k.stop_process();
    let kernel = start(&id, k.request.clone(), &app, &state).await?;
    let started = KernelStarted {
        id: id.clone(),
        label: kernel.label.clone(),
        server: false,
    };
    kernels
        .kernels
        .lock()
        .expect("kernels")
        .insert(id, Arc::new(kernel));
    Ok(started)
}

#[tauri::command]
pub async fn jupyter_shutdown(id: String, kernels: State<'_, JupyterState>) -> Result<()> {
    let k = kernels.kernels.lock().expect("kernels").remove(&id);
    if let Some(k) = k {
        let _ = tokio::time::timeout(Duration::from_secs(3), k.session.shutdown()).await;
        k.stop_process();
    }
    Ok(())
}

#[tauri::command]
pub async fn jupyter_variables(
    id: String,
    kernels: State<'_, JupyterState>,
) -> Result<Vec<Variable>> {
    let k = kernels.get(&id)?;
    k.session.variables().await.map_err(err)
}

#[tauri::command]
pub async fn jupyter_variable_children(
    id: String,
    expr: String,
    kernels: State<'_, JupyterState>,
) -> Result<Vec<Variable>> {
    let k = kernels.get(&id)?;
    k.session.children(&expr).await.map_err(err)
}

/// View as DataFrame: up to `limit` rows of a DataFrame, Series or array variable.
#[tauri::command]
pub async fn jupyter_variable_frame(
    id: String,
    expr: String,
    limit: Option<usize>,
    kernels: State<'_, JupyterState>,
) -> Result<DataFrame> {
    let k = kernels.get(&id)?;
    k.session
        .frame(&expr, limit.unwrap_or(10_000))
        .await
        .map_err(err)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCheck {
    pub base: String,
    pub default_kernel: String,
    pub kernels: Vec<KernelSpecInfo>,
}

/// Test a Jupyter server: the kernels it offers. A given token is stored in the keychain.
#[tauri::command]
pub async fn jupyter_server_test(url: String, token: Option<String>) -> Result<ServerCheck> {
    let mut address =
        split_server_url(&url, token.as_deref().unwrap_or("")).map_err(ForgeError::Other)?;
    if address.token.is_empty() {
        address.token = secrets::token(&address.base).unwrap_or_default();
    }
    let (default_kernel, kernels) = kernel_specs(&address).await.map_err(err)?;
    if !address.token.is_empty() {
        secrets::store_token(&address.base, &address.token).map_err(ForgeError::Other)?;
    }
    Ok(ServerCheck {
        base: address.base,
        default_kernel,
        kernels,
    })
}

/// Whether a token for the server is in the keychain.
#[tauri::command]
pub async fn jupyter_server_has_token(url: String) -> Result<bool> {
    let address = split_server_url(&url, "").map_err(ForgeError::Other)?;
    Ok(!address.token.is_empty() || secrets::token(&address.base).is_some())
}

#[tauri::command]
pub async fn jupyter_server_forget_token(url: String) -> Result<()> {
    let address = split_server_url(&url, "").map_err(ForgeError::Other)?;
    secrets::delete_token(&address.base).map_err(ForgeError::Other)
}
