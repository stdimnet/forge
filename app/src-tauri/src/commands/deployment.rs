//! Deployment IPC (Tools | Deployment, Settings | Build, Execution, Deployment |
//! Deployment, the File Transfer tool window). Servers, mappings and transfers are in
//! `forge-deploy`; this module loads `.forge/deployment.toml` and the `deployment.*`
//! settings, opens the server and streams what happens to every file.
//!
//! Safe Mode: uploads, downloads and compares only copy files, they never run project code,
//! so the user's own actions work in untrusted projects. Automatic upload on save is off in
//! Safe Mode: it would act on servers the untrusted project configured itself, without the
//! user choosing to deploy.
//!
//! Events: `deploy://transfer` [`TransferEvent`] (`started`, one `file` per file or
//! folder, `finished`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use forge_core::{ForgeError, Result};
use forge_deploy::config::{DeployServer, DeploymentConfig, PROJECT_FILE};
use forge_deploy::connect::{self, TestResult};
use forge_deploy::local::LocalFolder;
use forge_deploy::options::{AutoUpload, CompareBy, DeployOptions, WarnNewer};
use forge_deploy::transfer::{Deployer, Direction, FileResult, Summary, SyncCompare};
use forge_deploy::{secrets, DeployError};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::commands::workspace::blocking;
use crate::state::AppState;

pub const TRANSFER_EVENT: &str = "deploy://transfer";

/// Running operations by the id the UI gave them (for Cancel).
#[derive(Default)]
pub struct DeploymentState {
    running: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// One automatic upload at a time per project.
    auto: tokio::sync::Mutex<()>,
}

fn err(e: DeployError) -> ForgeError {
    ForgeError::Other(e.to_string())
}

/// The project folder on this machine; transfers of remote projects are not supported.
fn local_project(state: &AppState) -> Result<PathBuf> {
    match state.local_workspace() {
        Some(ws) if !state.is_remote() => Ok(ws.root().to_path_buf()),
        _ => Err(ForgeError::Other(
            "Deployment works with projects on this machine; this project is on a remote host"
                .into(),
        )),
    }
}

fn load(state: &AppState) -> Result<DeploymentConfig> {
    let text = state
        .backend()?
        .read_optional(PROJECT_FILE)?
        .unwrap_or_default();
    DeploymentConfig::parse(&text).map_err(err)
}

fn save(state: &AppState, config: &DeploymentConfig) -> Result<()> {
    state.backend()?.write_file(PROJECT_FILE, &config.to_toml())
}

/// Deployment | Options from the settings.
fn options(state: &AppState) -> DeployOptions {
    let s = state.read_settings();
    let text = |key: &str| s.get_as::<String>(key).unwrap_or_default();
    let flag = |key: &str, default: bool| s.get_as::<bool>(key).unwrap_or(default);
    let defaults = DeployOptions::default();
    DeployOptions {
        auto_upload: AutoUpload::parse(&text("deployment.autoUpload")),
        exclude_names: s
            .get_as::<String>("deployment.excludeNames")
            .unwrap_or(defaults.exclude_names),
        overwrite_up_to_date: flag("deployment.overwriteUpToDate", false),
        preserve_timestamps: flag("deployment.preserveTimestamps", true),
        delete_target_items: flag("deployment.deleteTargetItems", false),
        create_empty_dirs: flag("deployment.createEmptyDirs", false),
        warn_newer: WarnNewer::parse(&text("deployment.warnOverNewer")),
        compare_by: CompareBy::parse(&text("deployment.compareBy")),
    }
}

/// A server for the UI.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerView {
    #[serde(flatten)]
    pub server: DeployServer,
    pub label: String,
    /// A password is stored in the keychain (SFTP: in the SSH configuration).
    pub has_secret: bool,
}

fn view(server: DeployServer) -> ServerView {
    ServerView {
        label: server.label(),
        has_secret: secrets::has_secret(&server),
        server,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigView {
    pub default_server: Option<String>,
    pub servers: Vec<ServerView>,
    /// Transfers are possible (a local project).
    pub available: bool,
}

#[tauri::command]
pub async fn deploy_config(state: State<'_, AppState>) -> Result<ConfigView> {
    let config = load(&state)?;
    let available = local_project(&state).is_ok();
    blocking(move || {
        let default_server = config.default_server().map(|s| s.id.clone());
        Ok(ConfigView {
            default_server,
            servers: config.servers.into_iter().map(view).collect(),
            available,
        })
    })
    .await
}

/// Insert or update a server (a new id for an empty one). `secret` replaces the stored
/// FTP password; an empty string deletes it, `None` keeps it.
#[tauri::command]
pub async fn deploy_server_save(
    server: DeployServer,
    secret: Option<String>,
    state: State<'_, AppState>,
) -> Result<ServerView> {
    let mut config = load(&state)?;
    let id = config.upsert(server);
    save(&state, &config)?;
    let saved = config.get(&id).cloned().expect("just saved");
    blocking(move || {
        if let Some(secret) = secret {
            secrets::set(&saved, Some(&secret)).map_err(err)?;
        }
        Ok(view(saved))
    })
    .await
}

#[tauri::command]
pub fn deploy_server_delete(id: String, state: State<'_, AppState>) -> Result<()> {
    let mut config = load(&state)?;
    if config.remove(&id) {
        save(&state, &config)?;
    }
    Ok(())
}

/// "Use as Default"; `None` clears the default server.
#[tauri::command]
pub fn deploy_set_default(id: Option<String>, state: State<'_, AppState>) -> Result<()> {
    let mut config = load(&state)?;
    if let Some(id) = &id {
        if config.get(id).is_none() {
            return Err(ForgeError::Other(format!("no deployment server {id}")));
        }
    }
    config.default_server = id;
    save(&state, &config)
}

#[tauri::command]
pub fn deploy_validate(server: DeployServer) -> Vec<String> {
    server.validate()
}

/// Test Connection with the form's values (`secret`: a password typed but not saved yet).
#[tauri::command]
pub async fn deploy_test(
    server: DeployServer,
    secret: Option<String>,
    state: State<'_, AppState>,
) -> Result<TestResult> {
    let project = local_project(&state).ok();
    blocking(move || {
        Ok(connect::test(
            &server,
            secret.as_deref(),
            project.as_deref(),
        ))
    })
    .await
}

/// The server to use: the given one or the project's default.
fn pick(config: &DeploymentConfig, id: Option<&str>) -> Result<DeployServer> {
    match id {
        Some(id) => config.get(id).cloned().ok_or_else(|| {
            ForgeError::Other(format!("The deployment server {id} does not exist"))
        }),
        None => config.default_server().cloned().ok_or_else(|| {
            ForgeError::Other(
                "No default deployment server. Add one in Settings | Build, Execution, Deployment | Deployment."
                    .into(),
            )
        }),
    }
}

/// What the File Transfer tool window shows.
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TransferEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        id: String,
        server: String,
        operation: String,
        paths: Vec<String>,
        automatic: bool,
    },
    #[serde(rename_all = "camelCase")]
    File { id: String, file: FileResult },
    #[serde(rename_all = "camelCase")]
    Finished {
        id: String,
        summary: Summary,
        error: Option<String>,
    },
}

#[derive(Clone, Copy)]
enum Op {
    Transfer(Direction, bool),
    Delete,
}

struct Job {
    id: String,
    server: DeployServer,
    paths: Vec<String>,
    automatic: bool,
}

fn operation_label(op: Op, server: &DeployServer) -> String {
    match op {
        Op::Transfer(Direction::Upload, _) => format!("Upload to {}", server.label()),
        Op::Transfer(Direction::Download, _) => format!("Download from {}", server.label()),
        Op::Delete => format!("Delete from {}", server.label()),
    }
}

/// Run one operation on a blocking thread and stream it to the File Transfer window.
async fn run(
    app: AppHandle,
    project: PathBuf,
    job: Job,
    op: Op,
    options: DeployOptions,
    running: Arc<AtomicBool>,
) -> Result<Summary> {
    let _ = app.emit(
        TRANSFER_EVENT,
        TransferEvent::Started {
            id: job.id.clone(),
            server: job.server.label(),
            operation: operation_label(op, &job.server),
            paths: job.paths.clone(),
            automatic: job.automatic,
        },
    );
    let id = job.id.clone();
    let emitter = app.clone();
    let result = blocking(move || {
        let mut opened = connect::open(&job.server, None, Some(&project)).map_err(err)?;
        let mut local = LocalFolder::new(&project);
        let id = job.id.clone();
        let mut deployer = Deployer::new(&mut local, opened.fs.as_mut(), &job.server, &options)
            .skip_local(opened.inside_project.clone())
            .cancel(&running)
            .on_progress(move |file| {
                let _ = emitter.emit(
                    TRANSFER_EVENT,
                    TransferEvent::File {
                        id: id.clone(),
                        file: file.clone(),
                    },
                );
            });
        let summary = match op {
            Op::Transfer(direction, overwrite) => {
                deployer.transfer(direction, &job.paths, overwrite)
            }
            Op::Delete => deployer.delete_remote(&job.paths),
        };
        Ok(summary)
    })
    .await;
    let _ = app.emit(
        TRANSFER_EVENT,
        TransferEvent::Finished {
            id,
            summary: result.as_ref().cloned().unwrap_or_default(),
            error: result.as_ref().err().map(|e| e.to_string()),
        },
    );
    result
}

async fn start(
    app: AppHandle,
    state: &AppState,
    deploy: &DeploymentState,
    id: String,
    server_id: Option<String>,
    paths: Vec<String>,
    op: Op,
) -> Result<Summary> {
    let project = local_project(state)?;
    let server = pick(&load(state)?, server_id.as_deref())?;
    let flag = Arc::new(AtomicBool::new(false));
    deploy
        .running
        .lock()
        .expect("deploy lock poisoned")
        .insert(id.clone(), flag.clone());
    let job = Job {
        id: id.clone(),
        server,
        paths,
        automatic: false,
    };
    let result = run(app, project, job, op, options(state), flag).await;
    deploy
        .running
        .lock()
        .expect("deploy lock poisoned")
        .remove(&id);
    result
}

/// Upload to / Download from a server (`serverId` empty: the default server). `id` names
/// the operation for its events and Cancel; `overwriteNewer` overwrites targets that are
/// newer (after the user confirmed).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn deploy_transfer(
    id: String,
    server_id: Option<String>,
    direction: Direction,
    paths: Vec<String>,
    overwrite_newer: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
    deploy: State<'_, DeploymentState>,
) -> Result<Summary> {
    let op = Op::Transfer(direction, overwrite_newer.unwrap_or(false));
    start(app, &state, &deploy, id, server_id, paths, op).await
}

/// Delete the deployed files of project paths from a server.
#[tauri::command]
pub async fn deploy_delete(
    id: String,
    server_id: Option<String>,
    paths: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
    deploy: State<'_, DeploymentState>,
) -> Result<Summary> {
    start(app, &state, &deploy, id, server_id, paths, Op::Delete).await
}

#[tauri::command]
pub fn deploy_cancel(id: String, deploy: State<'_, DeploymentState>) {
    if let Some(flag) = deploy
        .running
        .lock()
        .expect("deploy lock poisoned")
        .get(&id)
    {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Upload saved files to the default server when "Upload changed files automatically"
/// says so (`explicit`: Ctrl+S / Save All). `None` when nothing was uploaded.
#[tauri::command]
pub async fn deploy_auto_upload(
    id: String,
    paths: Vec<String>,
    explicit: bool,
    app: AppHandle,
    state: State<'_, AppState>,
    deploy: State<'_, DeploymentState>,
) -> Result<Option<Summary>> {
    let options = options(&state);
    if !options.auto_upload.uploads_on_save(explicit) || state.safe_mode.is_on() {
        return Ok(None);
    }
    let Ok(project) = local_project(&state) else {
        return Ok(None);
    };
    let Some(server) = load(&state)?.default_server().cloned() else {
        return Ok(None);
    };
    let paths: Vec<String> = paths
        .into_iter()
        .filter(|p| server.deployment_path(p).is_some())
        .collect();
    if paths.is_empty() {
        return Ok(None);
    }
    let _one_at_a_time = deploy.auto.lock().await;
    let job = Job {
        id,
        server,
        paths,
        automatic: true,
    };
    let op = Op::Transfer(Direction::Upload, false);
    let flag = Arc::new(AtomicBool::new(false));
    run(app, project, job, op, options, flag).await.map(Some)
}

/// Sync with Deployed: the folder diff of a project folder and its deployed folder.
#[tauri::command]
pub async fn deploy_sync_compare(
    server_id: Option<String>,
    path: String,
    state: State<'_, AppState>,
) -> Result<SyncCompare> {
    let project = local_project(&state)?;
    let server = pick(&load(&state)?, server_id.as_deref())?;
    let options = options(&state);
    blocking(move || {
        let mut opened = connect::open(&server, None, Some(&project)).map_err(err)?;
        let mut local = LocalFolder::new(&project);
        let result = Deployer::new(&mut local, opened.fs.as_mut(), &server, &options)
            .skip_local(opened.inside_project.clone())
            .sync_compare(&path);
        result.map_err(err)
    })
    .await
}

/// The deployed version of a project file for Compare with Deployed Version.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFile {
    /// The text, `None` when the file is not on the server.
    pub text: Option<String>,
    /// The file is not UTF-8 text.
    pub binary: bool,
    /// Where it is on the server.
    pub remote: String,
    pub server: String,
}

#[tauri::command]
pub async fn deploy_read_remote(
    server_id: Option<String>,
    path: String,
    state: State<'_, AppState>,
) -> Result<RemoteFile> {
    let project = local_project(&state)?;
    let server = pick(&load(&state)?, server_id.as_deref())?;
    let options = options(&state);
    blocking(move || {
        let mut opened = connect::open(&server, None, Some(&project)).map_err(err)?;
        let mut local = LocalFolder::new(&project);
        let read =
            Deployer::new(&mut local, opened.fs.as_mut(), &server, &options).read_remote(&path);
        let (bytes, remote) = read.map_err(err)?;
        let (text, binary) = match bytes {
            None => (None, false),
            Some(b) => match String::from_utf8(b) {
                Ok(t) => (Some(t), false),
                Err(_) => (None, true),
            },
        };
        Ok(RemoteFile {
            text,
            binary,
            remote,
            server: server.label(),
        })
    })
    .await
}
