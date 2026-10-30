//! Code With Me IPC (toolbar popup, Tools | Code With Me, Settings | Tools | Code With Me,
//! shared editor tabs). The model, protocol and sessions are in `forge-collab`; this module
//! only adapts them.
//!
//! Events:
//! - `collab://status` [`SessionView`] whenever the session changes;
//! - `collab://doc` [`DocEvent`] for remote edits, carets, saves, renames, closes and File
//!   Cache Conflicts (answered with `collab_doc_resolve_conflict`);
//! - `collab://notice` `{ text }` (someone joined or left, a disk conflict);
//! - `collab://presence` [`ParticipantPresence`] when another participant moved (file,
//!   caret, viewport, whom it follows);
//! - `collab://follow-host` `{ on }` when the host turned Make Everyone Follow You on or off
//!   (guest);
//! - `collab://ended` `{ reason }` when the session is over for this window.
//! - `collab://resumed` (guest) when the connection to the host was restored after it
//!   dropped: the UI brings its shared documents up to date with `collab_doc_sync`. While
//!   the connection is being restored, [`SessionView::connection`] says so.
//! - `collab://run-request` `{ request, configuration, guest }` (host): a guest with Run Full
//!   access asks to run a configuration; the host's UI starts it like Run and answers with
//!   `collab_run_answer`;
//! - `collab://terminal-opened` `{ id, title }` (host): a guest opened a terminal on this
//!   machine; the host's Terminal tool window shows it (`terminal_attach`).
//!
//! Shared terminals and runs: the host shares a terminal tab with `collab_share_terminal`
//! (every run is shared); their output reaches the session through the hooks below
//! (`on_terminal_*`, `on_run_*`). A guest attaches to a shared terminal with
//! `collab_terminal_attach` (it then works like any terminal session: `terminal_write`,
//! `terminal_close`) and to a run with `collab_run_attach` (events on
//! `run://cwm-run-<id>`).
//!
//! Updates and awareness cross the IPC as base64 strings; the initial state of a document is
//! sent as raw bytes ([`collab_doc_open`]).
//!
//! A guest with the link is admitted only when the host accepts: `collab://join-request`
//! shows the request with its verification code, `collab_answer_join` answers it and
//! `collab://join-closed` removes it when nobody answered. A joining guest gets
//! `collab://waiting` with the same code and can cancel with `collab_cancel_join`. No socket
//! is opened before Start Session.

use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use forge_backend::Backend;
use forge_collab::approval::{ApprovalEvent, Approvals};
use forge_collab::audit::{LogLine, SessionLog};
use forge_collab::docs::Resolution;
use forge_collab::excludes::Excludes;
use forge_collab::guest::{GuestEvent, GuestSession};
use forge_collab::guest_backend::lsp_host::{LspServices, ManagerSource};
use forge_collab::guest_backend::{CollabBackend, LspQuery};
use forge_collab::host::{HostConfig, HostEvent, HostServices, HostSession, LspRequest};
use forge_collab::link::{LinkSummary, SessionLink};
use forge_collab::permissions::{PermissionLevel, Permissions};
use forge_collab::presence::{Participant, ParticipantId, Presence};
use forge_collab::protocol::{DocId, SaveState, SharedFile};
use forge_collab::reconnect::ConnectionState;
use forge_collab::relay::{
    normalize_relay_token, normalize_relay_url, ConnectionPath, RelayOptions, RelayStatus,
};
use forge_collab::secrets::{KeyringStore, RelayToken, RelayTokenStatus};
use forge_collab::share::{RunId, SharedRun, SharedTerminal, TerminalId};
use forge_collab::transport::net::{ListenOptions, NetTransport};
use forge_collab::transport::relay::NetConfig;
use forge_collab::transport::Transport;
use forge_collab::verify::verification_code;
use forge_core::{ForgeError, FsChanges, Result};
use futures_util::future::BoxFuture;
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{oneshot, Notify};

use crate::commands::workspace::WorkspaceInfo;
use crate::state::AppState;

pub const STATUS_EVENT: &str = "collab://status";
pub const DOC_EVENT: &str = "collab://doc";
pub const NOTICE_EVENT: &str = "collab://notice";
pub const ENDED_EVENT: &str = "collab://ended";
pub const JOIN_REQUEST_EVENT: &str = "collab://join-request";
pub const JOIN_CLOSED_EVENT: &str = "collab://join-closed";
pub const WAITING_EVENT: &str = "collab://waiting";
pub const PRESENCE_EVENT: &str = "collab://presence";
pub const FOLLOW_HOST_EVENT: &str = "collab://follow-host";
pub const RUN_REQUEST_EVENT: &str = "collab://run-request";
pub const TERMINAL_OPENED_EVENT: &str = "collab://terminal-opened";
pub const RESUMED_EVENT: &str = "collab://resumed";

/// How long the host's UI may take to start a run a guest asked for.
const RUN_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

enum Active {
    Host(Arc<HostSession>),
    Guest(Arc<GuestSession>),
}

/// Code With Me's state in this window.
#[derive(Default)]
pub struct CollabState {
    active: Mutex<Option<Active>>,
    /// The host's project, as a guest has it open.
    guest_workspace: Mutex<Option<WorkspaceInfo>>,
    /// The host's open join requests.
    approvals: Mutex<Option<Arc<Approvals>>>,
    /// A guest waiting for the host to accept; notified to cancel.
    joining: Mutex<Option<Arc<Notify>>>,
    /// The host's session log (kept after the session ends, until the next one starts).
    log: Mutex<Option<Arc<SessionLog>>>,
    /// Guests' run requests waiting for the host's UI.
    run_requests: Mutex<std::collections::HashMap<u64, RunAnswer>>,
    next_run_request: std::sync::atomic::AtomicU64,
}

type RunAnswer = oneshot::Sender<std::result::Result<(), String>>;

/// Clears the joining state however the join ends.
struct JoiningGuard<'a>(&'a CollabState);

impl Drop for JoiningGuard<'_> {
    fn drop(&mut self) {
        *self.0.joining.lock().expect("collab lock poisoned") = None;
    }
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
            || self.joining.lock().expect("collab lock poisoned").is_some()
    }

    /// Start joining; fails while a session runs or another join waits.
    fn begin_join(&self) -> Result<(Arc<Notify>, JoiningGuard<'_>)> {
        let mut joining = self.joining.lock().expect("collab lock poisoned");
        if joining.is_some() || self.active.lock().expect("collab lock poisoned").is_some() {
            return Err(other("A Code With Me session is already running."));
        }
        let cancel = Arc::new(Notify::new());
        *joining = Some(cancel.clone());
        Ok((cancel, JoiningGuard(self)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionRole {
    None,
    Host,
    Guest,
}

/// How one participant's connection travels, for the participants list and the status bar.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantConnection {
    pub participant: ParticipantId,
    #[serde(flatten)]
    pub path: ConnectionPath,
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
    /// Direct or relayed, per connected guest (host) or to the host (guest).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<ParticipantConnection>,
    /// The relay server of the session and whether it is connected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay: Option<RelayStatus>,
    /// The host's project, open in this window (guest).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceInfo>,
    /// Where the other participants are (a guest only learns files it may open).
    pub presence: Vec<ParticipantPresence>,
    /// Make Everyone Follow You is on.
    pub follow_everyone: bool,
    /// Shared terminals: the ones the host shares (with their ids in this window), or the
    /// host's terminals this guest sees.
    pub terminals: Vec<TerminalView>,
    /// The host's runs this guest sees (guest).
    pub runs: Vec<SharedRun>,
    /// Whether the connection to the host is up or being restored (guest).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionState>,
}

/// A shared terminal in the session view.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalView {
    #[serde(flatten)]
    pub terminal: SharedTerminal,
    /// The terminal's id in this window (host).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// One participant's presence (`collab://presence` and [`SessionView::presence`]).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantPresence {
    pub participant: ParticipantId,
    #[serde(flatten)]
    pub presence: Presence,
}

fn presence_list(
    list: Vec<(ParticipantId, Presence)>,
    me: ParticipantId,
) -> Vec<ParticipantPresence> {
    list.into_iter()
        .filter(|(id, _)| *id != me)
        .map(|(participant, presence)| ParticipantPresence {
            participant,
            presence,
        })
        .collect()
}

fn idle_view() -> SessionView {
    SessionView {
        role: SessionRole::None,
        can_host: true,
        host_unavailable: None,
        link: None,
        me: None,
        participants: Vec::new(),
        files: Vec::new(),
        permissions: None,
        host_label: None,
        notice: None,
        connections: Vec::new(),
        relay: None,
        workspace: None,
        presence: Vec::new(),
        follow_everyone: false,
        terminals: Vec::new(),
        runs: Vec::new(),
        connection: None,
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
            connections: h
                .connection_paths()
                .into_iter()
                .map(|(participant, path)| ParticipantConnection { participant, path })
                .collect(),
            relay: h.relay_status(),
            workspace: None,
            presence: presence_list(h.presence(), ParticipantId::HOST),
            follow_everyone: h.follow_everyone(),
            terminals: h
                .shared_terminals()
                .into_iter()
                .map(|(key, terminal)| TerminalView {
                    terminal,
                    key: Some(key),
                })
                .collect(),
            runs: Vec::new(),
            connection: None,
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
            connections: g
                .connection_path()
                .map(|path| ParticipantConnection {
                    participant: ParticipantId::HOST,
                    path,
                })
                .into_iter()
                .collect(),
            relay: g.relay_status(),
            workspace: state
                .guest_workspace
                .lock()
                .expect("collab lock poisoned")
                .clone(),
            presence: presence_list(g.presence(), g.me()),
            follow_everyone: g.follow_host(),
            terminals: g
                .terminals()
                .into_iter()
                .map(|terminal| TerminalView {
                    terminal,
                    key: None,
                })
                .collect(),
            runs: g.runs(),
            connection: Some(g.connection()),
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
    /// A File Cache Conflict opened or ended ([`DocConflict`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict: Option<DocConflict>,
    /// Host: the guest whose edit `update` is (absent for changes from disk); the editor
    /// marks the lines guests changed until the next save.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<ParticipantId>,
}

/// The shared file changed on the host's disk while it has unsaved changes. The host answers
/// with `collab_doc_resolve_conflict`; guests cannot save until then.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocConflict {
    /// Open (`true`) or over.
    pub open: bool,
    /// The file (project-relative); empty for guests, whose tab knows it.
    pub path: String,
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
        HostEvent::DocUpdate { doc, update, from } => doc_event(app, doc, |e| {
            e.update = Some(B64.encode(update));
            e.from = from;
        }),
        HostEvent::Awareness { doc, update } => {
            doc_event(app, doc, |e| e.awareness = Some(B64.encode(update)))
        }
        HostEvent::DocSaved { doc, saved } => doc_event(app, doc, |e| e.saved = Some(saved)),
        HostEvent::DocRenamed { doc, path } => doc_event(app, doc, |e| e.renamed = Some(path)),
        HostEvent::DocClosed { doc, reason } => doc_event(app, doc, |e| e.closed = Some(reason)),
        HostEvent::DocConflict {
            doc,
            path,
            conflict,
        } => doc_event(app, doc, |e| {
            e.conflict = Some(DocConflict {
                open: conflict,
                path,
            })
        }),
        HostEvent::Notice(text) => emit(app, NOTICE_EVENT, json!({ "text": text })),
        HostEvent::Presence {
            participant,
            presence,
        } => emit(
            app,
            PRESENCE_EVENT,
            ParticipantPresence {
                participant,
                presence,
            },
        ),
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
        GuestEvent::DocConflict { doc, conflict } => doc_event(app, doc, |e| {
            e.conflict = Some(DocConflict {
                open: conflict,
                path: String::new(),
            })
        }),
        GuestEvent::Denied(text) => emit(app, NOTICE_EVENT, json!({ "text": text })),
        GuestEvent::Presence {
            participant,
            presence,
        } => emit(
            app,
            PRESENCE_EVENT,
            ParticipantPresence {
                participant,
                presence,
            },
        ),
        GuestEvent::FollowHost(on) => emit(app, FOLLOW_HOST_EVENT, json!({ "on": on })),
        GuestEvent::Resumed => emit(app, RESUMED_EVENT, json!({})),
        GuestEvent::Ended(reason) => {
            end_guest_project(app);
            emit(app, ENDED_EVENT, json!({ "reason": reason }));
            emit_status(app);
        }
    }
}

/// The guest's session is over: the host's project is closed in this window.
fn end_guest_project(app: &AppHandle) {
    let collab = app.state::<CollabState>();
    collab.take();
    if collab
        .guest_workspace
        .lock()
        .expect("collab lock poisoned")
        .take()
        .is_some()
    {
        app.state::<AppState>().close_project();
        app.state::<crate::commands::lsp::LspState>().reset();
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

/// The relay settings and the token from the keychain.
fn host_relay_options(app: &AppHandle) -> Result<RelayOptions> {
    let (url, public, lan): (String, bool, bool) = {
        let state = app.state::<AppState>();
        let s = state.read_settings();
        (
            s.get_as("codeWithMe.relayUrl").unwrap_or_default(),
            s.get_as("codeWithMe.usePublicRelays").unwrap_or(false),
            s.get_as("codeWithMe.lanDiscovery").unwrap_or(true),
        )
    };
    let url = normalize_relay_url(&url).map_err(|e| {
        other(format!(
            "{e} (Settings | Tools | Code With Me | Relay server)"
        ))
    })?;
    let token = if url.is_some() {
        RelayToken(&KeyringStore).get().unwrap_or_else(|e| {
            tracing::warn!("{e}");
            None
        })
    } else {
        None
    };
    Ok(RelayOptions::host(url, token, public, lan))
}

/// How often the connection paths are checked: a relayed connection moves to a direct path
/// when hole punching succeeds, which iroh does not announce.
const PATH_POLL: std::time::Duration = std::time::Duration::from_secs(2);

/// Emit the status again whenever a connection path or the relay state changes.
fn watch_connections(app: &AppHandle, session: Active) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut last = None;
        loop {
            tokio::time::sleep(PATH_POLL).await;
            let state = app.state::<CollabState>();
            let current = match (
                &session,
                &*state.active.lock().expect("collab lock poisoned"),
            ) {
                (Active::Host(a), Some(Active::Host(b))) if Arc::ptr_eq(a, b) => true,
                (Active::Guest(a), Some(Active::Guest(b))) if Arc::ptr_eq(a, b) => true,
                _ => false,
            };
            if !current {
                return;
            }
            let v = view(&state);
            let key = serde_json::to_string(&(&v.connections, &v.relay)).ok();
            if key != last {
                if last.is_some() {
                    emit(&app, STATUS_EVENT, v);
                }
                last = key;
            }
        }
    });
}

/// Whether a relay token is stored (the token itself never reaches the UI).
#[tauri::command]
pub fn collab_relay_token_status() -> RelayTokenStatus {
    RelayToken(&KeyringStore).status()
}

/// Store the relay token in the keychain; empty removes it.
#[tauri::command]
pub fn collab_set_relay_token(token: Option<String>) -> Result<RelayTokenStatus> {
    let token = token.as_deref().map(str::trim).filter(|t| !t.is_empty());
    // Check before touching the keychain, with the same rules.
    normalize_relay_token(token.unwrap_or_default()).map_err(other)?;
    RelayToken(&KeyringStore).set(token).map_err(other)?;
    Ok(RelayToken(&KeyringStore).status())
}

/// Settings | Tools | Code With Me | Test Connection: connect to the relay as typed in the
/// page (not yet applied) and report the result.
#[tauri::command]
pub async fn collab_test_relay(url: String, public: bool) -> Result<RelayStatus> {
    let url = normalize_relay_url(&url).map_err(other)?;
    let token = if url.is_some() {
        RelayToken(&KeyringStore).get().ok().flatten()
    } else {
        None
    };
    let options = RelayOptions::host(url, token, public, false);
    if !options.contacts_servers() {
        return Err(other("Enter a relay server first."));
    }
    NetTransport::check_relay(NetConfig::with_relay(options))
        .await
        .map_err(|e| other(e.to_string()))
}

#[tauri::command]
pub fn collab_status(state: State<'_, CollabState>) -> SessionView {
    view(&state)
}

/// What the Join Session dialog shows about a pasted link: the host, its machine, the relay,
/// and the name this window joins with.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinPreview {
    #[serde(flatten)]
    pub summary: LinkSummary,
    pub user_name: String,
}

#[tauri::command]
pub fn collab_parse_link(link: String, app: AppHandle) -> Result<JoinPreview> {
    let summary = SessionLink::parse(&link)
        .map(|l| l.summary())
        .map_err(other)?;
    Ok(JoinPreview {
        summary,
        user_name: user_name(&app),
    })
}

/// Start Session: open the listening socket and share the files open in the editor.
#[tauri::command]
pub async fn collab_start(
    level: PermissionLevel,
    custom: Option<Permissions>,
    app: AppHandle,
) -> Result<SessionView> {
    let collab = app.state::<CollabState>();
    if collab.busy() {
        return Err(other("A Code With Me session is already running."));
    }
    let permissions = Permissions::preset(level)
        .or(custom)
        .ok_or_else(|| other("Choose the custom permissions first."))?;
    let (excludes, protected, max_kb, address, port, keep_log) = {
        let state = app.state::<AppState>();
        let s = state.read_settings();
        let patterns: Vec<String> = s.get_as("codeWithMe.excludedFiles").unwrap_or_default();
        let protected: Vec<String> = s.get_as("codeWithMe.protectedFiles").unwrap_or_else(|| {
            forge_collab::excludes::DEFAULT_PROTECTED
                .iter()
                .map(|p| p.to_string())
                .collect()
        });
        let max_kb: u64 = s.get_as("codeWithMe.maxSharedFileSize").unwrap_or(5120);
        let address: String = s.get_as("codeWithMe.listenAddress").unwrap_or_default();
        let port: u16 = s.get_as("codeWithMe.listenPort").unwrap_or(0);
        let keep_log: bool = s.get_as("codeWithMe.auditLog").unwrap_or(true);
        (patterns, protected, max_kb, address, port, keep_log)
    };
    let excludes = Excludes::new(&excludes).map_err(other)?;
    let protected = Excludes::new(&protected).map_err(other)?;
    let address = match address.trim() {
        "" => None,
        a => Some(a.parse().map_err(|_| {
            other(format!(
                "\"{a}\" is not an IP address (Settings | Tools | Code With Me)"
            ))
        })?),
    };
    let backend = app.state::<AppState>().backend()?;
    let relay = host_relay_options(&app)?;
    let transport = NetTransport::listen_with(
        ListenOptions { address, port },
        NetConfig::with_relay(relay),
    )
    .await
    .map_err(|e| other(format!("Cannot listen for guests: {e}")))?;
    let handle = app.clone();
    let approvals = Approvals::new(Arc::new(move |e| match e {
        ApprovalEvent::Request { id, name, code } => emit(
            &handle,
            JOIN_REQUEST_EVENT,
            json!({ "id": id, "name": name, "code": code, "level": level }),
        ),
        ApprovalEvent::Closed { id } => emit(&handle, JOIN_CLOSED_EVENT, json!({ "id": id })),
    }));
    let handle = app.clone();
    let host = HostSession::start(
        Arc::new(transport),
        backend,
        HostConfig {
            host_name: user_name(&app),
            machine: machine_name(),
            permissions,
            excludes,
            protected,
            max_file_size: (max_kb as usize).saturating_mul(1024),
            notice: None,
        },
        approvals.clone(),
        Arc::new(move |e| on_host_event(&handle, e)),
    );
    host.set_services(Arc::new(ShellServices {
        lsp: LspServices(Arc::new(ShellServers { app: app.clone() })),
        app: app.clone(),
    }));
    let log = session_log(keep_log, &host.session_hex());
    host.set_audit(log.clone());
    *collab.log.lock().expect("collab lock poisoned") = Some(log);
    *collab.approvals.lock().expect("collab lock poisoned") = Some(approvals);
    collab.set(Active::Host(host.clone()));
    watch_connections(&app, Active::Host(host));
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
    if let Some(approvals) = state.approvals.lock().expect("collab lock poisoned").take() {
        approvals.refuse_all();
    }
    state
        .run_requests
        .lock()
        .expect("collab lock poisoned")
        .clear();
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

/// Where the session logs go: `<data dir>/forge/code-with-me`, never the project.
fn log_dir() -> Option<std::path::PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("code-with-me"))
}

/// The session log: a file when the setting is on (and the directory can be written),
/// otherwise only in memory for the Session Log.
fn session_log(keep: bool, session: &str) -> Arc<SessionLog> {
    if !keep {
        return SessionLog::memory();
    }
    match log_dir().map(|dir| SessionLog::create(&dir, session)) {
        Some(Ok(log)) => log,
        Some(Err(e)) => {
            tracing::warn!("Code With Me: cannot write the session log: {e}");
            SessionLog::memory()
        }
        None => SessionLog::memory(),
    }
}

/// The Session Log of the current (or last) hosted session.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLogView {
    pub lines: Vec<LogLine>,
    /// The log file, when the session log is kept.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

#[tauri::command]
pub fn collab_session_log(state: State<'_, CollabState>) -> SessionLogView {
    match &*state.log.lock().expect("collab lock poisoned") {
        Some(log) => SessionLogView {
            lines: log.lines(),
            file: log.path().map(|p| p.to_string_lossy().into_owned()),
        },
        None => SessionLogView {
            lines: Vec::new(),
            file: None,
        },
    }
}

fn running_host(state: &CollabState) -> Result<Arc<HostSession>> {
    state.host().ok_or_else(|| other("No session is running."))
}

/// Permissions for the whole session (host): new guests and everyone in the session.
#[tauri::command]
pub fn collab_set_permissions(permissions: Permissions, app: AppHandle) -> Result<SessionView> {
    running_host(&app.state())?.set_permissions(permissions);
    let v = view(&app.state());
    emit(&app, STATUS_EVENT, v.clone());
    Ok(v)
}

/// Permissions of one guest (the participant's Permissions menu).
#[tauri::command]
pub fn collab_set_guest_permissions(
    participant: u32,
    permissions: Permissions,
    state: State<'_, CollabState>,
) -> Result<()> {
    if running_host(&state)?.set_guest_permissions(ParticipantId(participant), permissions) {
        Ok(())
    } else {
        Err(other("The guest is no longer in the session."))
    }
}

/// Remove Guest: disconnect a guest at once.
#[tauri::command]
pub fn collab_remove_guest(participant: u32, state: State<'_, CollabState>) -> Result<()> {
    if running_host(&state)?.remove(ParticipantId(participant)) {
        Ok(())
    } else {
        Err(other("The guest is no longer in the session."))
    }
}

/// Regenerate Link: the old link stops working; returns the new one.
#[tauri::command]
pub fn collab_regenerate_link(app: AppHandle) -> Result<String> {
    let link = running_host(&app.state())?.regenerate_link().to_url();
    emit_status(&app);
    Ok(link)
}

/// Accept or decline a guest's join request (host).
#[tauri::command]
pub fn collab_answer_join(id: u64, accept: bool, state: State<'_, CollabState>) -> Result<()> {
    let approvals = state
        .approvals
        .lock()
        .expect("collab lock poisoned")
        .clone();
    match approvals {
        Some(a) if a.answer(id, accept) => Ok(()),
        _ => Err(other("The request is no longer open.")),
    }
}

/// Stop waiting for the host to accept (guest).
#[tauri::command]
pub fn collab_cancel_join(state: State<'_, CollabState>) {
    if let Some(cancel) = state.joining.lock().expect("collab lock poisoned").clone() {
        cancel.notify_one();
    }
}

/// Join a session from a link.
#[tauri::command]
pub async fn collab_join(link: String, app: AppHandle) -> Result<SessionView> {
    let collab = app.state::<CollabState>();
    let link = SessionLink::parse(&link).map_err(other)?;
    let (cancel, _joining) = collab.begin_join()?;
    let own = host_relay_options(&app)?;
    let relay = RelayOptions::guest(link.relay.as_deref(), &own);
    let has_token = relay.token.is_some();
    let transport = NetTransport::dialer_with(NetConfig::with_relay(relay))
        .await
        .map_err(|e| other(format!("Cannot connect: {e}")))?;
    // The host sees the same code and compares it with this one before accepting.
    emit(
        &app,
        WAITING_EVENT,
        json!({
            "host": link.summary().host_label,
            "code": verification_code(&link.host_id, &transport.local_key(), &link.session_id),
        }),
    );
    let handle = app.clone();
    let name = user_name(&app);
    let joined = tokio::select! {
        r = GuestSession::join(
            Arc::new(transport),
            &link,
            &name,
            Arc::new(move |e| on_guest_event(&handle, e)),
        ) => r,
        _ = cancel.notified() => return Err(other("Cancelled.")),
    };
    let guest = joined.map_err(|e| match &link.relay {
        Some(relay) if !has_token => other(format!(
            "{e} The link goes through the relay server {relay}. If it needs a token, enter it \
             under Settings | Tools | Code With Me."
        )),
        _ => other(e),
    })?;
    // The host's project becomes this window's project.
    let session = guest.clone();
    let opened = tauri::async_runtime::spawn_blocking(move || {
        let backend: Arc<dyn Backend> = Arc::new(CollabBackend::open(session)?);
        Ok::<_, ForgeError>(backend)
    })
    .await
    .map_err(other)?;
    let backend = match opened {
        Ok(b) => b,
        Err(e) => {
            guest.leave().await;
            return Err(e);
        }
    };
    let info = WorkspaceInfo::from(backend.info());
    crate::commands::remote::close_project_connection(&app);
    let target = backend.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        handle.state::<AppState>().set_collab_project(target);
    })
    .await
    .map_err(other)?;
    app.state::<crate::commands::lsp::LspState>().reset();
    let handle = app.clone();
    guest.set_diagnostics_sink(Some(Arc::new(move |path, diagnostics| {
        // The host's diagnostics look like local ones to the editor.
        if let Ok(list) = serde_json::from_slice::<serde_json::Value>(&diagnostics) {
            emit(
                &handle,
                "lsp://diagnostics",
                json!({ "path": path, "diagnostics": list }),
            );
        }
    })));
    *collab.guest_workspace.lock().expect("collab lock poisoned") = Some(info);
    collab.set(Active::Guest(guest.clone()));
    watch_connections(&app, Active::Guest(guest));
    crate::commands::workspace::project_opened(&app);
    let v = view(&collab);
    emit(&app, STATUS_EVENT, v.clone());
    Ok(v)
}

/// The round trip to the host in milliseconds (the guest's status bar item).
#[tauri::command]
pub async fn collab_ping(app: AppHandle) -> Result<u64> {
    let guest = guest_session(&app).ok_or_else(|| other("No session is running."))?;
    guest.ping().await.map_err(other)
}

/// Leave Session (guest).
#[tauri::command]
pub async fn collab_leave(app: AppHandle) -> Result<()> {
    let state = app.state::<CollabState>();
    let Some(guest) = state.guest() else {
        return Ok(());
    };
    end_guest_project(&app);
    guest.leave().await;
    emit(
        &app,
        ENDED_EVENT,
        json!({ "reason": "You left the session." }),
    );
    emit_status(&app);
    Ok(())
}

/// Open a shared document: `content` is the host's editor text for a tab that is already
/// open. The reply is raw bytes: a big-endian `u32` length, a JSON header
/// `{ docId, path, saved, readOnly, conflict }` and the document as one Yjs update.
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
        "conflict": opened.conflict,
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

/// Where this participant is: the active editor's file, caret, viewport and whom it follows.
/// Sent by the UI on changes (throttled).
#[tauri::command]
pub fn collab_presence(presence: Presence, state: State<'_, CollabState>) {
    if let Some(host) = state.host() {
        host.host_presence(presence);
    } else if let Some(guest) = state.guest() {
        guest.set_presence(presence);
    }
}

/// Make Everyone Follow You (host).
#[tauri::command]
pub fn collab_follow_everyone(on: bool, app: AppHandle) -> Result<SessionView> {
    let host = running_host(&app.state::<CollabState>())?;
    host.set_follow_everyone(on);
    Ok(view(&app.state::<CollabState>()))
}

/// Bring a shared document up to date after the connection to the host was restored
/// (guest). `state_vector` is the UI replica's (Yjs v1, base64). The reply is raw bytes like
/// [`collab_doc_open`]'s: a JSON header `{ docId, path, saved, readOnly, conflict,
/// stateVector }`
/// (the host's state vector, base64) and the update the replica is missing.
#[tauri::command]
pub async fn collab_doc_sync(
    doc_id: u64,
    state_vector: String,
    app: AppHandle,
) -> Result<tauri::ipc::Response> {
    let guest = guest_or_err(&app)?;
    let synced = guest
        .sync_doc(DocId(doc_id), decode(&state_vector)?)
        .await
        .map_err(other)?;
    let header = serde_json::to_vec(&json!({
        "docId": synced.doc.0,
        "path": synced.path,
        "saved": synced.saved,
        "readOnly": synced.read_only,
        "conflict": synced.conflict,
        "stateVector": B64.encode(&synced.state_vector),
    }))
    .map_err(other)?;
    let mut out = Vec::with_capacity(4 + header.len() + synced.update.len());
    out.extend_from_slice(&(header.len() as u32).to_be_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&synced.update);
    Ok(tauri::ipc::Response::new(out))
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

/// The host's answer to a File Cache Conflict (`collab://doc` with `conflict`): `load` takes
/// the file system's text (Load File System Changes), otherwise the shared document keeps its
/// text and stays unsaved (Keep Memory Changes).
#[tauri::command]
pub fn collab_doc_resolve_conflict(doc_id: u64, load: bool, app: AppHandle) -> Result<()> {
    let host = running_host(&app.state::<CollabState>())?;
    let how = if load {
        Resolution::LoadFileSystem
    } else {
        Resolution::KeepMemory
    };
    host.resolve_conflict(DocId(doc_id), how).map_err(other)
}

// ---- language servers for guests -------------------------------------------------------

/// The guest session of this window, if it is a guest.
pub fn guest_session(app: &AppHandle) -> Option<Arc<GuestSession>> {
    app.state::<CollabState>().guest()
}

/// Ask the host's language servers (a guest's `lsp_*` commands).
pub async fn guest_lsp<T: serde::de::DeserializeOwned>(
    guest: &GuestSession,
    query: LspQuery,
) -> Result<T> {
    let bytes = guest.lsp(query).await.map_err(other)?;
    serde_json::from_slice(&bytes).map_err(other)
}

/// The language servers of this window's project, for guests' requests.
struct ShellServers {
    app: AppHandle,
}

impl ManagerSource for ShellServers {
    fn manager(&self) -> std::result::Result<Arc<forge_lsp::LspManager>, String> {
        let lsp = self.app.state::<crate::commands::lsp::LspState>();
        let ws = self.app.state::<AppState>();
        lsp.manager(&self.app, &ws).map_err(|e| e.to_string())
    }
}

/// The host's servers reported diagnostics: guests that have the file open get them too.
pub fn on_diagnostics(
    app: &AppHandle,
    path: &str,
    diagnostics: &[forge_lsp::model::EditorDiagnostic],
) {
    if let Some(host) = app.state::<CollabState>().host() {
        if let Ok(bytes) = serde_json::to_vec(diagnostics) {
            host.publish_diagnostics(path, bytes);
        }
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

// ---- shared terminals and runs -----------------------------------------------------------

/// What this window does for its guests: language servers, terminals and runs. The session
/// has checked the guest's permissions before any of these is called.
struct ShellServices {
    lsp: LspServices<ShellServers>,
    app: AppHandle,
}

impl HostServices for ShellServices {
    fn lsp(&self, req: LspRequest) -> BoxFuture<'static, std::result::Result<Vec<u8>, String>> {
        self.lsp.lsp(req)
    }

    /// A shell in the project folder (the host's own shell and settings), shown in the host's
    /// Terminal tool window as `title`.
    fn terminal_open(
        &self,
        key: String,
        title: String,
        cols: u16,
        rows: u16,
    ) -> BoxFuture<'static, std::result::Result<(), String>> {
        let app = self.app.clone();
        Box::pin(async move {
            let options = crate::commands::terminal::TerminalOpenOptions {
                id: Some(key.clone()),
                cwd: Some(".".into()),
                cols,
                rows,
                ..Default::default()
            };
            crate::commands::terminal::open(options, app.clone())
                .await
                .map_err(|e| e.to_string())?;
            emit(
                &app,
                TERMINAL_OPENED_EVENT,
                json!({ "id": key, "title": title }),
            );
            Ok(())
        })
    }

    fn terminal_write(&self, key: &str, data: &str) -> std::result::Result<(), String> {
        let terminals = self.app.state::<crate::commands::terminal::TerminalState>();
        crate::commands::terminal::terminal_write(key.to_string(), data.to_string(), terminals)
            .map_err(|e| e.to_string())
    }

    fn terminal_close(&self, key: &str) {
        let terminals = self.app.state::<crate::commands::terminal::TerminalState>();
        if let Some(t) = terminals.remove(key) {
            if let Err(e) = t.kill() {
                tracing::warn!("Code With Me: cannot close a guest's terminal: {e}");
            }
        }
    }

    /// The host's UI starts the configuration as if the host clicked Run.
    fn run_start(
        &self,
        configuration: String,
        guest: String,
    ) -> BoxFuture<'static, std::result::Result<(), String>> {
        let app = self.app.clone();
        Box::pin(async move {
            let collab = app.state::<CollabState>();
            let request = collab
                .next_run_request
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                + 1;
            let (tx, rx) = oneshot::channel();
            collab
                .run_requests
                .lock()
                .expect("collab lock poisoned")
                .insert(request, tx);
            emit(
                &app,
                RUN_REQUEST_EVENT,
                json!({ "request": request, "configuration": configuration, "guest": guest }),
            );
            match tokio::time::timeout(RUN_REQUEST_TIMEOUT, rx).await {
                Ok(Ok(answer)) => answer,
                _ => {
                    collab
                        .run_requests
                        .lock()
                        .expect("collab lock poisoned")
                        .remove(&request);
                    Err("The host did not start the run.".into())
                }
            }
        })
    }

    fn run_stop(&self, key: &str) -> std::result::Result<(), String> {
        let runs = self.app.state::<crate::commands::run::RunState>();
        let (process, _) = runs.get(key).map_err(|e| e.to_string())?;
        std::thread::spawn(move || process.kill(std::time::Duration::from_secs(2)));
        Ok(())
    }

    fn run_write(&self, key: &str, data: &str) -> std::result::Result<(), String> {
        crate::commands::run::run_write(
            key.to_string(),
            data.to_string(),
            self.app.clone(),
            self.app.state(),
        )
        .map_err(|e| e.to_string())
    }
}

/// The host's UI started (or could not start) the run a guest asked for.
#[tauri::command]
pub fn collab_run_answer(request: u64, error: Option<String>, state: State<'_, CollabState>) {
    let waiting = state
        .run_requests
        .lock()
        .expect("collab lock poisoned")
        .remove(&request);
    if let Some(tx) = waiting {
        let _ = tx.send(error.map_or(Ok(()), Err));
    }
}

/// Share With Guests on a terminal tab (host), or stop sharing it.
#[tauri::command]
pub fn collab_share_terminal(
    id: String,
    title: String,
    shared: bool,
    app: AppHandle,
) -> Result<SessionView> {
    let host = running_host(&app.state())?;
    let (scrollback, cols, rows) = app
        .state::<crate::commands::terminal::TerminalState>()
        .snapshot(&id)
        .ok_or_else(|| other("The terminal is not running."))?;
    if !host.share_terminal(&id, &title, cols, rows, &scrollback, shared) {
        return Err(other("A guest's terminal is always shared."));
    }
    Ok(view(&app.state()))
}

fn guest_or_err(app: &AppHandle) -> Result<Arc<GuestSession>> {
    guest_session(app).ok_or_else(|| other("No session is running."))
}

/// What a guest's terminal tab starts with.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalAttach {
    pub scrollback: String,
    pub cols: u16,
    pub rows: u16,
    pub running: bool,
}

/// Show one of the host's shared terminals in the tab `id` (guest): it then works like any
/// terminal session of this window (`terminal_write`; `terminal_close` detaches).
#[tauri::command]
pub fn collab_terminal_attach(terminal: u64, id: String, app: AppHandle) -> Result<TerminalAttach> {
    let guest = guest_or_err(&app)?;
    let event = format!("terminal://{id}");
    let handle = app.clone();
    let (scrollback, info, proxy) = guest
        .attach_terminal(
            TerminalId(terminal),
            Arc::new(move |e| {
                if let Err(err) = handle.emit(&event, &e) {
                    tracing::warn!("cannot emit {event}: {err}");
                }
            }),
        )
        .map_err(other)?;
    app.state::<crate::commands::terminal::TerminalState>()
        .insert(id, proxy);
    Ok(TerminalAttach {
        scrollback,
        cols: info.cols,
        rows: info.rows,
        running: info.running,
    })
}

/// Show one of the host's runs (guest): what happened so far; live events follow on
/// `run://cwm-run-<run>`.
#[tauri::command]
pub fn collab_run_attach(run: u64, app: AppHandle) -> Result<Vec<serde_json::Value>> {
    let guest = guest_or_err(&app)?;
    let event = format!("run://cwm-run-{run}");
    let handle = app.clone();
    let events = guest
        .attach_run(
            RunId(run),
            Arc::new(move |e| {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&e) {
                    emit(&handle, &event, value);
                }
            }),
        )
        .map_err(other)?;
    Ok(events
        .iter()
        .filter_map(|e| serde_json::from_str(e).ok())
        .collect())
}

/// The run's tab closed (guest).
#[tauri::command]
pub fn collab_run_detach(run: u64, app: AppHandle) {
    if let Some(guest) = guest_session(&app) {
        guest.detach_run(RunId(run));
    }
}

/// Run one of the host's configurations by name (guest, Run Full access).
#[tauri::command]
pub async fn collab_run_start(configuration: String, app: AppHandle) -> Result<()> {
    guest_or_err(&app)?
        .start_run(&configuration)
        .await
        .map_err(other)
}

/// Stop one of the host's runs (guest, Run Full access).
#[tauri::command]
pub async fn collab_run_stop(run: u64, app: AppHandle) -> Result<()> {
    guest_or_err(&app)?
        .stop_run(RunId(run))
        .await
        .map_err(other)
}

/// Console input for one of the host's runs (guest, Run Full access).
#[tauri::command]
pub fn collab_run_input(run: u64, data: String, app: AppHandle) -> Result<()> {
    guest_or_err(&app)?.run_input(RunId(run), &data);
    Ok(())
}

// ---- hooks from the terminal and run commands --------------------------------------------

fn hosting(app: &AppHandle) -> Option<Arc<HostSession>> {
    app.try_state::<CollabState>()?.host()
}

/// Output of a terminal of this window; shared with guests if the terminal is shared.
pub fn on_terminal_output(app: &AppHandle, id: &str, data: &str) {
    if let Some(host) = hosting(app) {
        host.terminal_output(id, data);
    }
}

pub fn on_terminal_exited(app: &AppHandle, id: &str, code: Option<i32>) {
    if let Some(host) = hosting(app) {
        host.terminal_exited(id, code);
    }
}

pub fn on_terminal_resized(app: &AppHandle, id: &str, cols: u16, rows: u16) {
    if let Some(host) = hosting(app) {
        host.terminal_resized(id, cols, rows);
    }
}

pub fn on_terminal_closed(app: &AppHandle, id: &str) {
    if let Some(host) = hosting(app) {
        host.terminal_closed(id);
    }
}

/// A run of the Run tool window is about to start: guests who see runs see it.
pub fn on_run_starting(app: &AppHandle, run_id: &str, name: &str, kind: Option<&str>) {
    if let Some(host) = hosting(app) {
        host.run_started(run_id, name, kind);
    }
}

/// An event of a run (`run://<runId>`).
pub fn on_run_event<T: Serialize>(app: &AppHandle, run_id: &str, payload: &T) {
    if let Some(host) = hosting(app) {
        if let Ok(json) = serde_json::to_string(payload) {
            host.run_event(run_id, &json);
        }
    }
}

/// A run's tab closed (or the run did not start).
pub fn on_run_disposed(app: &AppHandle, run_id: &str) {
    if let Some(host) = hosting(app) {
        host.run_disposed(run_id);
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
    fn manifest_protected_files_match_the_crate_defaults() {
        let registry = crate::plugins::builtin_registry().unwrap();
        let spec = registry
            .all_settings()
            .find(|s| s.key == "codeWithMe.protectedFiles")
            .unwrap();
        let defaults: Vec<String> = serde_json::from_value(spec.default.clone()).unwrap();
        assert_eq!(defaults, forge_collab::excludes::DEFAULT_PROTECTED);
    }

    #[test]
    fn bad_links_are_reported() {
        assert!(SessionLink::parse("https://example.com").is_err());
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
