//! Remote Host tool window (Tools | Deployment | Browse Remote Host): the files of a
//! deployment server as a lazy tree, its context menu and Edit Remote File. Listing and
//! operations are `forge_deploy::remote_host` on a connection kept open between calls;
//! Download from Here / Upload to Here / Compare with Local Version reuse the deployment
//! commands with the node's mapped project path.
//!
//! Safe Mode: browsing and editing server files only copy files and never run project code,
//! so they work in untrusted projects (like uploads and downloads).
//!
//! Edit Remote File keeps a temporary copy in `<cache dir>/forge/remote-edit/<server>/…`,
//! never in the project; saving uploads it back unless the server copy changed since it
//! was downloaded (`conflict`, then the UI asks and saves again with `force`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use forge_core::{ForgeError, Result};
use forge_deploy::browse::{self, RemoteNode};
use forge_deploy::config::DeployServer;
use forge_deploy::connect;
use forge_deploy::remote_host::{self, Connections, EditedFile, SaveOutcome};
use serde::Serialize;
use tauri::State;

use crate::commands::deployment::{err, load, local_project};
use crate::commands::workspace::blocking;
use crate::state::AppState;

#[derive(Default)]
pub struct RemoteHostState {
    connections: Arc<Mutex<Connections>>,
    /// Files opened with Edit Remote File, by `server id \0 path`.
    edits: Arc<Mutex<HashMap<String, EditedFile>>>,
}

fn edit_key(server: &str, path: &str) -> String {
    format!("{server}\0{}", forge_deploy::mapping::normalize(path))
}

fn cache_dir() -> Result<PathBuf> {
    dirs::cache_dir()
        .map(|d| d.join("forge"))
        .ok_or_else(|| ForgeError::Other("no cache directory".into()))
}

fn server(state: &AppState, id: &str) -> Result<DeployServer> {
    load(state)?
        .get(id)
        .cloned()
        .ok_or_else(|| ForgeError::Other(format!("The deployment server {id} does not exist")))
}

/// Run `op` on the server's open connection (connecting first when needed).
async fn with_server<T: Send + 'static>(
    state: &AppState,
    hosts: &RemoteHostState,
    server_id: &str,
    op: impl FnOnce(&mut dyn forge_deploy::fs::ServerFs, &str, &DeployServer) -> forge_deploy::Result<T>
        + Send
        + 'static,
) -> Result<T> {
    let server = server(state, server_id)?;
    let project = local_project(state).ok();
    let connections = hosts.connections.clone();
    blocking(move || {
        let mut pool = connections.lock().expect("remote host connections");
        pool.with(
            &server,
            |s| connect::open(s, None, project.as_deref()),
            |fs, root| op(fs, root, &server),
        )
        .map_err(err)
    })
    .await
}

/// A folder of the tree.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteListing {
    /// The server's absolute root folder (Copy Path, the root node's tooltip).
    pub root: String,
    /// The root folder as a node (its mapping), named after `root`.
    pub root_node: RemoteNode,
    pub nodes: Vec<RemoteNode>,
}

/// The entries of `path` (relative to the root path, `""` for the root).
#[tauri::command]
pub async fn remote_host_list(
    server_id: String,
    path: String,
    state: State<'_, AppState>,
    hosts: State<'_, RemoteHostState>,
) -> Result<RemoteListing> {
    with_server(&state, &hosts, &server_id, move |fs, root, server| {
        let mut root_node = browse::node(server, &forge_deploy::compare::Entry::dir("", None));
        root_node.name = root.to_string();
        Ok(RemoteListing {
            root: root.to_string(),
            root_node,
            nodes: remote_host::list(fs, server, &path)?,
        })
    })
    .await
}

/// New File / New Directory in `dir`; returns the new node.
#[tauri::command]
pub async fn remote_host_create(
    server_id: String,
    dir: String,
    name: String,
    folder: bool,
    state: State<'_, AppState>,
    hosts: State<'_, RemoteHostState>,
) -> Result<RemoteNode> {
    with_server(&state, &hosts, &server_id, move |fs, _, server| {
        let path = remote_host::create(fs, &dir, &name, folder)?;
        let entry = fs
            .stat(&path)?
            .unwrap_or_else(|| forge_deploy::compare::Entry::file(path.clone(), 0, None));
        Ok(browse::node(server, &entry))
    })
    .await
}

/// Rename in place; returns the new path.
#[tauri::command]
pub async fn remote_host_rename(
    server_id: String,
    path: String,
    new_name: String,
    state: State<'_, AppState>,
    hosts: State<'_, RemoteHostState>,
) -> Result<String> {
    let edits = hosts.edits.clone();
    let id = server_id.clone();
    let renamed = with_server(&state, &hosts, &server_id, {
        let path = path.clone();
        move |fs, _, _| remote_host::rename(fs, &path, &new_name)
    })
    .await?;
    // An open Edit Remote File copy follows the file.
    let mut edits = edits.lock().expect("remote edits");
    if let Some(mut edited) = edits.remove(&edit_key(&id, &path)) {
        edited.path = renamed.clone();
        edits.insert(edit_key(&id, &renamed), edited);
    }
    Ok(renamed)
}

/// Delete files and folders (with everything in them); returns how many were deleted.
#[tauri::command]
pub async fn remote_host_delete(
    server_id: String,
    paths: Vec<String>,
    state: State<'_, AppState>,
    hosts: State<'_, RemoteHostState>,
) -> Result<usize> {
    with_server(&state, &hosts, &server_id, move |fs, _, _| {
        remote_host::delete(fs, &paths)
    })
    .await
}

/// A server file opened with Edit Remote File.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditOpened {
    pub text: String,
    /// The temporary copy on this machine.
    pub temp: String,
    /// Where it is on the server.
    pub remote: String,
}

/// Edit Remote File: download `path` into its temporary copy and return the text.
#[tauri::command]
pub async fn remote_host_edit(
    server_id: String,
    path: String,
    state: State<'_, AppState>,
    hosts: State<'_, RemoteHostState>,
) -> Result<EditOpened> {
    let cache = cache_dir()?;
    let id = server_id.clone();
    let (edited, remote) = with_server(&state, &hosts, &server_id, move |fs, root, _| {
        let edited = remote_host::start_edit(fs, &id, &path, &cache)?;
        Ok((edited.clone(), browse::absolute_path(root, &edited.path)))
    })
    .await?;
    let bytes = std::fs::read(&edited.temp).map_err(|e| ForgeError::io(&edited.temp, e))?;
    let text = String::from_utf8(bytes).map_err(|_| {
        ForgeError::Other(format!("{remote} is a binary file and cannot be edited"))
    })?;
    let opened = EditOpened {
        text,
        temp: edited.temp.display().to_string(),
        remote,
    };
    hosts
        .edits
        .lock()
        .expect("remote edits")
        .insert(edit_key(&server_id, &edited.path), edited);
    Ok(opened)
}

/// Save an Edit Remote File tab: write the temporary copy, then upload it unless the server
/// copy changed since it was downloaded (`force` overwrites it anyway).
#[tauri::command]
pub async fn remote_host_save_edit(
    server_id: String,
    path: String,
    text: String,
    force: Option<bool>,
    state: State<'_, AppState>,
    hosts: State<'_, RemoteHostState>,
) -> Result<SaveOutcome> {
    let key = edit_key(&server_id, &path);
    let mut edited = hosts
        .edits
        .lock()
        .expect("remote edits")
        .get(&key)
        .cloned()
        .ok_or_else(|| {
            ForgeError::Other(format!("{path} is not open with Edit Remote File any more"))
        })?;
    std::fs::write(&edited.temp, &text).map_err(|e| ForgeError::io(&edited.temp, e))?;
    let force = force.unwrap_or(false);
    let (outcome, edited) = with_server(&state, &hosts, &server_id, move |fs, _, _| {
        let outcome = remote_host::save_edit(fs, &mut edited, text.as_bytes(), force)?;
        Ok((outcome, edited))
    })
    .await?;
    hosts
        .edits
        .lock()
        .expect("remote edits")
        .insert(key, edited);
    Ok(outcome)
}

/// The Edit Remote File tab was closed: forget the file and remove its temporary copy.
#[tauri::command]
pub fn remote_host_close_edit(server_id: String, path: String, hosts: State<'_, RemoteHostState>) {
    let removed = hosts
        .edits
        .lock()
        .expect("remote edits")
        .remove(&edit_key(&server_id, &path));
    if let Some(edited) = removed {
        let _ = std::fs::remove_file(&edited.temp);
    }
}

/// Close the connection of one server (the selector switched away), or all of them.
#[tauri::command]
pub async fn remote_host_disconnect(
    server_id: Option<String>,
    hosts: State<'_, RemoteHostState>,
) -> Result<()> {
    let connections = hosts.connections.clone();
    blocking(move || {
        connections
            .lock()
            .expect("remote host connections")
            .close(server_id.as_deref());
        Ok(())
    })
    .await
}
