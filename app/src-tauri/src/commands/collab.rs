//! Code With Me IPC (toolbar popup, Tools | Code With Me, Settings | Tools | Code With Me,
//! shared editor tabs). The model, protocol and sessions are in `forge-collab`; this module
//! only adapts them.
//!
//! Events:
//! - `collab://status` [`SessionView`] whenever the session changes;
//! - `collab://doc` [`DocEvent`] for remote edits, carets, saves, renames and closes;
//! - `collab://notice` `{ text }` (someone joined or left, a disk conflict);
//! - `collab://ended` `{ reason }` when the session is over for this window.
//!
//! Updates and awareness cross the IPC as base64 strings; the initial state of a document is
//! sent as raw bytes ([`collab_doc_open`]).
//!
//! Until host approval exists, hosting needs the development flag `FORGE_CWM_DEV=1`: every
//! guest with the link joins without being asked. No socket is opened before Start Session.

use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use forge_collab::excludes::Excludes;
use forge_collab::guest::{GuestEvent, GuestSession};
use forge_collab::host::{DevAutoAccept, HostConfig, HostEvent, HostSession, DEV_PREVIEW_NOTICE};
use forge_collab::link::{LinkSummary, SessionLink};
use forge_collab::permissions::{PermissionLevel, Permissions};
use forge_collab::presence::{Participant, ParticipantId};
use forge_collab::protocol::{DocId, SaveState, SharedFile};
use forge_collab::transport::net::{ListenOptions, NetTransport};
use forge_core::{ForgeError, FsChanges, Result};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

pub const STATUS_EVENT: &str = "collab://status";
pub const DOC_EVENT: &str = "collab://doc";
pub const NOTICE_EVENT: &str = "collab://notice";
pub const ENDED_EVENT: &str = "collab://ended";

/// Environment variable that allows hosting while sessions have no host approval.
pub const DEV_FLAG: &str = "FORGE_CWM_DEV";

enum Active {
    Host(Arc<HostSession>),
    Guest(Arc<GuestSession>),
}

/// Code With Me's state in this window.
#[derive(Default)]
pub struct CollabState {
    active: Mutex<Option<Active>>,
}

impl CollabState {
    fn host(&self) -> Option<Arc<HostSession>> {
        match &*self.active.lock().expect("collab lock poisoned") {
            Some(Active::Host(h)) => Some(h.clone()),
            _ => None,
        }
    }

    fn guest(&self) -> Option<Arc<GuestSession>> {
        match &*self.active.lock().expect("collab lock poisoned") {
            Some(Active::Guest(g)) => Some(g.clone()),
            _ => None,
        }
    }

    fn take(&self) -> Option<Active> {
        self.active.lock().expect("collab lock poisoned").take()
    }

    fn set(&self, a: Active) {
        *self.active.lock().expect("collab lock poisoned") = Some(a);
    }

    fn busy(&self) -> bool {
        self.active.lock().expect("collab lock poisoned").is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionRole {
    None,
    Host,
    Guest,
}

/// The session as the UI shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub role: SessionRole,
    /// Whether Start Session can be used; otherwise `host_unavailable` says why.
    pub can_host: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_unavailable: Option<String>,
    /// The invitation link (host).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub me: Option<ParticipantId>,
    pub participants: Vec<Participant>,
    /// Files guests can open.
    pub files: Vec<SharedFile>,
    /// The session's guest permissions (host) or this guest's permissions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Permissions>,
    /// `name (machine)` of the host (guest).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_label: Option<String>,
    /// A notice for every participant, e.g. that the session is a development preview.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

fn dev_flag() -> bool {
    std::env::var(DEV_FLAG).is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

fn idle_view() -> SessionView {
    let can_host = dev_flag();
    SessionView {
        role: SessionRole::None,
        can_host,
        host_unavailable: (!can_host).then(|| {
            format!(
                "Hosting is a development preview until host approval is available: guests with \
                 the link join without being asked. Start Forge with {DEV_FLAG}=1 to try it."
            )
        }),
        link: None,
        me: None,
        participants: Vec::new(),
        files: Vec::new(),
        permissions: None,
        host_label: None,
        notice: None,
    }
}

fn view(state: &CollabState) -> SessionView {
    if let Some(h) = state.host() {
        return SessionView {
            role: SessionRole::Host,
            can_host: true,
            host_unavailable: None,
            link: Some(h.link().to_url()),
            me: Some(ParticipantId::HOST),
            participants: h.participants(),
            files: h.shared_files(),
            permissions: Some(h.permissions()),
            host_label: None,
            notice: h.notice(),
        };
    }
    if let Some(g) = state.guest() {
        return SessionView {
            role: SessionRole::Guest,
            can_host: false,
            host_unavailable: None,
            link: None,
            me: Some(g.me()),
            participants: g.participants(),
            files: g.files(),
            permissions: Some(g.permissions()),
            host_label: Some(g.host_label().to_string()),
            notice: g.notice(),
        };
    }
    idle_view()
}

fn emit(app: &AppHandle, event: &str, payload: impl Serialize + Clone) {
    if let Err(e) = app.emit(event, payload) {
        tracing::warn!("cannot emit {event}: {e}");
    }
}

fn emit_status(app: &AppHandle) {
    emit(app, STATUS_EVENT, view(&app.state::<CollabState>()));
}

/// A change to one shared document.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocEvent {
    pub doc_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub awareness: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved: Option<SaveState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renamed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed: Option<String>,
}

fn doc_event(app: &AppHandle, doc: DocId, fill: impl FnOnce(&mut DocEvent)) {
    let mut e = DocEvent {
        doc_id: doc.0,
        ..DocEvent::default()
    };
    fill(&mut e);
    emit(app, DOC_EVENT, e);
}

fn on_host_event(app: &AppHandle, e: HostEvent) {
    match e {
        HostEvent::Changed => emit_status(app),
        HostEvent::DocUpdate { doc, update } => {
            doc_event(app, doc, |e| e.update = Some(B64.encode(update)))
        }
        HostEvent::Awareness { doc, update } => {
            doc_event(app, doc, |e| e.awareness = Some(B64.encode(update)))
        }
        HostEvent::DocSaved { doc, saved } => doc_event(app, doc, |e| e.saved = Some(saved)),
        HostEvent::DocRenamed { doc, path } => doc_event(app, doc, |e| e.renamed = Some(path)),
        HostEvent::DocClosed { doc, reason } => doc_event(app, doc, |e| e.closed = Some(reason)),
        HostEvent::Notice(text) => emit(app, NOTICE_EVENT, json!({ "text": text })),
    }
}

fn on_guest_event(app: &AppHandle, e: GuestEvent) {
    match e {
        GuestEvent::Changed => emit_status(app),
        GuestEvent::DocUpdate { doc, update } => {
            doc_event(app, doc, |e| e.update = Some(B64.encode(update)))
        }
        GuestEvent::Awareness { doc, update } => {
            doc_event(app, doc, |e| e.awareness = Some(B64.encode(update)))
        }
        GuestEvent::DocSaved { doc, saved } => doc_event(app, doc, |e| e.saved = Some(saved)),
        GuestEvent::DocRenamed { doc, path } => doc_event(app, doc, |e| e.renamed = Some(path)),
        GuestEvent::DocClosed { doc, reason } => doc_event(app, doc, |e| e.closed = Some(reason)),
        GuestEvent::Denied(text) => emit(app, NOTICE_EVENT, json!({ "text": text })),
        GuestEvent::Ended(reason) => {
            app.state::<CollabState>().take();
            emit(app, ENDED_EVENT, json!({ "reason": reason }));
            emit_status(app);
        }
    }
}

fn other(e: impl std::fmt::Display) -> ForgeError {
    ForgeError::Other(e.to_string())
}

/// `codeWithMe.userName`, else the system user name.
fn user_name(app: &AppHandle) -> String {
    let configured: String = app
        .state::<AppState>()
        .read_settings()
        .get_as("codeWithMe.userName")
        .unwrap_or_default();
    if !configured.trim().is_empty() {
        return configured.trim().to_string();
    }
    ["USER", "USERNAME", "LOGNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .unwrap_or_else(|| "Forge user".into())
}

fn machine_name() -> Option<String> {
    let name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())?;
    let name = name.trim().to_string();
    (!name.is_empty() && name.chars().count() <= 64).then_some(name)
}

#[tauri::command]
pub fn collab_status(state: State<'_, CollabState>) -> SessionView {
    view(&state)
}

/// What the Join Session dialog shows about a pasted link.
#[tauri::command]
pub fn collab_parse_link(link: String) -> Result<LinkSummary> {
    SessionLink::parse(&link)
        .map(|l| l.summary())
        .map_err(other)
}

/// Start Session: open the listening socket and share the files open in the editor.
#[tauri::command]
pub async fn collab_start(level: PermissionLevel, app: AppHandle) -> Result<SessionView> {
    if !dev_flag() {
        return Err(other(idle_view().host_unavailable.unwrap_or_default()));
    }
    let collab = app.state::<CollabState>();
    if collab.busy() {
        return Err(other("A Code With Me session is already running."));
    }
    let permissions = Permissions::preset(level)
        .ok_or_else(|| other("Custom permissions are not available yet."))?;
    let (excludes, max_kb, address, port) = {
        let state = app.state::<AppState>();
        let s = state.read_settings();
        let patterns: Vec<String> = s.get_as("codeWithMe.excludedFiles").unwrap_or_default();
        let max_kb: u64 = s.get_as("codeWithMe.maxSharedFileSize").unwrap_or(5120);
        let address: String = s.get_as("codeWithMe.listenAddress").unwrap_or_default();
        let port: u16 = s.get_as("codeWithMe.listenPort").unwrap_or(0);
        (patterns, max_kb, address, port)
    };
    let excludes = Excludes::new(&excludes).map_err(other)?;
    let address = match address.trim() {
        "" => None,
        a => Some(a.parse().map_err(|_| {
            other(format!(
                "\"{a}\" is not an IP address (Settings | Tools | Code With Me)"
            ))
        })?),
    };
    let backend = app.state::<AppState>().backend()?;
    let transport = NetTransport::listen(ListenOptions { address, port })
        .await
        .map_err(|e| other(format!("Cannot listen for guests: {e}")))?;
    let handle = app.clone();
    let host = HostSession::start(
        Arc::new(transport),
        backend,
        HostConfig {
            host_name: user_name(&app),
            machine: machine_name(),
            permissions,
            excludes,
            max_file_size: (max_kb as usize).saturating_mul(1024),
            notice: Some(DEV_PREVIEW_NOTICE.into()),
        },
        Arc::new(DevAutoAccept),
        Arc::new(move |e| on_host_event(&handle, e)),
    );
    collab.set(Active::Host(host));
    let v = view(&collab);
    emit(&app, STATUS_EVENT, v.clone());
    Ok(v)
}

/// End Session (host).
#[tauri::command]
pub async fn collab_end(app: AppHandle) -> Result<()> {
    let state = app.state::<CollabState>();
    let Some(host) = state.host() else {
        return Ok(());
    };
    state.take();
    host.end("The host ended the session.").await;
    emit_status(&app);
    Ok(())
}

/// Copy Session Link.
#[tauri::command]
pub fn collab_link(state: State<'_, CollabState>) -> Result<String> {
    state
        .host()
        .map(|h| h.link().to_url())
        .ok_or_else(|| other("No session is running."))
}

/// Join a session from a link.
#[tauri::command]
pub async fn collab_join(link: String, app: AppHandle) -> Result<SessionView> {
    let collab = app.state::<CollabState>();
    if collab.busy() {
        return Err(other("A Code With Me session is already running."));
    }
    let link = SessionLink::parse(&link).map_err(other)?;
    let transport = NetTransport::dialer()
        .await
        .map_err(|e| other(format!("Cannot connect: {e}")))?;
    let handle = app.clone();
    let guest = GuestSession::join(
        Arc::new(transport),
        &link,
        &user_name(&app),
        Arc::new(move |e| on_guest_event(&handle, e)),
    )
    .await
    .map_err(other)?;
    collab.set(Active::Guest(guest));
    let v = view(&collab);
    emit(&app, STATUS_EVENT, v.clone());
    Ok(v)
}

/// Leave Session (guest).
#[tauri::command]
pub async fn collab_leave(app: AppHandle) -> Result<()> {
    let state = app.state::<CollabState>();
    let Some(guest) = state.guest() else {
        return Ok(());
    };
    state.take();
    guest.leave().await;
    emit_status(&app);
    Ok(())
}

/// Open a shared document: `content` is the host's editor text for a tab that is already
/// open. The reply is raw bytes: a big-endian `u32` length, a JSON header
/// `{ docId, path, saved, readOnly }` and the document as one Yjs update.
#[tauri::command]
pub async fn collab_doc_open(
    path: String,
    content: Option<String>,
    app: AppHandle,
) -> Result<tauri::ipc::Response> {
    let state = app.state::<CollabState>();
    let opened = if let Some(host) = state.host() {
        host.open_doc(&path, content).await.map_err(other)?
    } else if let Some(guest) = state.guest() {
        guest.open_doc(&path).await.map_err(other)?
    } else {
        return Err(other("No session is running."));
    };
    let header = serde_json::to_vec(&json!({
        "docId": opened.doc.0,
        "path": opened.path,
        "saved": opened.saved,
        "readOnly": opened.read_only,
    }))
    .map_err(other)?;
    let mut out = Vec::with_capacity(4 + header.len() + opened.update.len());
    out.extend_from_slice(&(header.len() as u32).to_be_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&opened.update);
    Ok(tauri::ipc::Response::new(out))
}

fn decode(b64: &str) -> Result<Vec<u8>> {
    B64.decode(b64).map_err(|_| other("invalid update"))
}

/// A local edit (Yjs update, base64).
#[tauri::command]
pub fn collab_doc_update(doc_id: u64, update: String, state: State<'_, CollabState>) -> Result<()> {
    let bytes = decode(&update)?;
    if let Some(host) = state.host() {
        host.host_update(DocId(doc_id), &bytes).map_err(other)
    } else if let Some(guest) = state.guest() {
        guest.update(DocId(doc_id), bytes);
        Ok(())
    } else {
        Err(other("No session is running."))
    }
}

/// The local caret and selection (awareness update, base64).
#[tauri::command]
pub fn collab_awareness(doc_id: u64, update: String, state: State<'_, CollabState>) -> Result<()> {
    let bytes = decode(&update)?;
    if let Some(host) = state.host() {
        host.host_awareness(DocId(doc_id), &bytes);
    } else if let Some(guest) = state.guest() {
        guest.awareness(DocId(doc_id), bytes);
    }
    Ok(())
}

#[tauri::command]
pub fn collab_doc_close(doc_id: u64, state: State<'_, CollabState>) {
    if let Some(host) = state.host() {
        host.host_close(DocId(doc_id));
    } else if let Some(guest) = state.guest() {
        guest.close_doc(DocId(doc_id));
    }
}

/// Save a shared document on the host.
#[tauri::command]
pub async fn collab_doc_save(doc_id: u64, app: AppHandle) -> Result<()> {
    let state = app.state::<CollabState>();
    if let Some(host) = state.host() {
        host.save(DocId(doc_id)).await.map(|_| ()).map_err(other)
    } else if let Some(guest) = state.guest() {
        guest.save(DocId(doc_id)).await.map_err(other)
    } else {
        Err(other("No session is running."))
    }
}

// ---- hooks from the project's file operations ------------------------------------------

/// The project watcher reported changes: shared documents follow the disk.
pub fn on_fs_changed(app: &AppHandle, changes: &FsChanges) {
    if let Some(host) = app.state::<CollabState>().host() {
        let changes = changes.clone();
        tauri::async_runtime::spawn(async move { host.on_fs_changed(&changes).await });
    }
}

/// The host renamed a file or folder from the Project view.
pub fn on_renamed(app: &AppHandle, from: &str, to: &str) {
    if let Some(host) = app.state::<CollabState>().host() {
        host.on_renamed(from, to);
    }
}

/// The host deleted a file or folder from the Project view.
pub fn on_deleted(app: &AppHandle, path: &str) {
    if let Some(host) = app.state::<CollabState>().host() {
        host.on_deleted(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_excludes_match_the_crate_defaults() {
        let registry = crate::plugins::builtin_registry().unwrap();
        let spec = registry
            .all_settings()
            .find(|s| s.key == "codeWithMe.excludedFiles")
            .unwrap();
        let defaults: Vec<String> = serde_json::from_value(spec.default.clone()).unwrap();
        assert_eq!(defaults, forge_collab::excludes::DEFAULT_PATTERNS);
    }

    #[test]
    fn bad_links_are_reported() {
        assert!(collab_parse_link("https://example.com".into()).is_err());
    }

    #[test]
    fn idle_sessions_explain_why_hosting_is_unavailable() {
        let v = idle_view();
        assert_eq!(v.role, SessionRole::None);
        assert_eq!(v.can_host, v.host_unavailable.is_none());
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json["role"], "none");
        assert!(json.get("link").is_none());
    }
}
