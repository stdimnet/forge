//! The host's side of a session: accepting guests, the shared documents, writing them to
//! disk and applying disk changes.
//!
//! The host's own editor takes part like a guest, as [`ParticipantId::HOST`], through the
//! methods of [`HostSession`]; guests send [`GuestMessage`]s over their connection. Every
//! guest message is checked by the guest's [`Policy`] before the host acts on it.
//!
//! What is shared: the files the host has open in the editor (excluded files, `.git/` and
//! files over the size limit are never shared). Guests open them from the list the host
//! sends ([`HostMessage::SharedFiles`]); the project tree for guests comes later.
//!
//! Admission goes through an [`Admission`]; the host approval with verification codes plugs
//! in there. Until it exists, [`DevAutoAccept`] admits every guest that proves it has the
//! link, and the session carries a notice saying so.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use forge_backend::Backend;
use forge_core::FsChanges;
use serde_bytes::ByteBuf;
use tokio::sync::mpsc;

use crate::audit::{short_peer, AuditEvent, FileAction, KnockOutcome, SharedAudit};
use crate::docs::{with_eol, DiskChange, Resolution, SharedDoc};
use crate::excludes::Excludes;
use crate::link::{SessionLink, JOIN_SECRET_LEN, SESSION_ID_LEN};
use crate::permissions::{Permissions, Policy};
use crate::presence::{clean_name, ColorAssigner, Participant, ParticipantId, Presence, Role};
use crate::protocol::{
    DocId, GuestMessage, HostMessage, SaveState, SharedFile, ViewOnly, MAX_GUEST_FRAME,
    MAX_HOST_FRAME, MAX_VIEW_SIZE, PROTOCOL_VERSION,
};
use crate::rate::{GuestLimits, Verdict};
use crate::reconnect::{same_token, HOST_GRACE};
use crate::relay::{ConnectionPath, RelayStatus};
use crate::transport::{
    read_msg, write_msg, write_or_skip, BoxFuture, BoxRead, BoxWrite, Connection, PeerKey,
    Transport,
};
use crate::verify::{check_proof, verification_code};

// The host's side of the guests' project backend (tree, search, git, language servers).
#[path = "guest_backend/host_calls.rs"]
mod guest_calls;
use guest_calls::CallState;
pub use guest_calls::{virtual_root, HostServices, LspRequest};

// Shared terminals and runs.
#[path = "share_host.rs"]
mod share_host;
use share_host::ShareState;

/// Messages queued for one guest before the guest counts as too slow and is disconnected.
const GUEST_QUEUE: usize = 4096;
/// Time a new connection has to send its knock.
const KNOCK_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_KNOCK: usize = 16 * 1024;
/// How long a guest's writer may take to flush its last messages.
const WRITER_FLUSH: Duration = Duration::from_secs(5);
const RESUME_TOKEN_LEN: usize = 32;

/// Why a guest is no longer in the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Leaving {
    Left,
    Removed,
    /// Its connection dropped and it did not come back within the grace period.
    Lost,
    /// It sent far more edits and carets than anyone types ([`crate::rate::FLOOD_RATE`]).
    Flooded,
}

/// How a guest's connection ended.
enum ConnectionEnd {
    /// The guest said Leave.
    Left,
    /// The connection dropped.
    Lost,
    /// The session ended, the guest was removed, or a newer connection took over.
    Gone,
}

/// What the host's UI needs to know.
#[derive(Debug, Clone, PartialEq)]
pub enum HostEvent {
    /// Participants or shared files changed; the session view is out of date.
    Changed,
    DocUpdate {
        doc: DocId,
        update: Vec<u8>,
        /// The guest whose edit this is; `None` for changes from disk (the host's editor
        /// marks the lines guests changed until the next save).
        from: Option<ParticipantId>,
    },
    Awareness {
        doc: DocId,
        update: Vec<u8>,
    },
    DocSaved {
        doc: DocId,
        saved: SaveState,
    },
    DocRenamed {
        doc: DocId,
        path: String,
    },
    DocClosed {
        doc: DocId,
        reason: String,
    },
    /// A File Cache Conflict opened (`conflict`) or is over: the file changed on disk while
    /// the document has unsaved changes. The host's user answers it with
    /// [`HostSession::resolve_conflict`]; until then guests cannot save it.
    DocConflict {
        doc: DocId,
        path: String,
        conflict: bool,
    },
    /// Where a guest is now (the host sees every file).
    Presence {
        participant: ParticipantId,
        presence: Presence,
    },
    /// A message for the host ("Alex joined the session.").
    Notice(String),
}

pub type HostSink = Arc<dyn Fn(HostEvent) + Send + Sync>;

/// A guest that proved it has the link and asks to join.
#[derive(Debug, Clone)]
pub struct JoinRequest {
    pub guest: PeerKey,
    pub name: String,
    /// The verification code the guest sees as well.
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Admit the guest with these permissions (`None`: the session's).
    Accept(Option<Permissions>),
    Decline,
    /// The host did not answer in time.
    Unanswered,
}

/// Decides who joins. [`crate::approval::Approvals`] asks the host; tests use [`DevAutoAccept`].
pub trait Admission: Send + Sync + 'static {
    fn decide(&self, request: JoinRequest) -> BoxFuture<'static, Decision>;
}

/// Admits every guest with a valid join proof with the session's permissions, without asking
/// the host. For tests; the app asks the host ([`crate::approval::Approvals`]).
pub struct DevAutoAccept;

impl Admission for DevAutoAccept {
    fn decide(&self, _request: JoinRequest) -> BoxFuture<'static, Decision> {
        Box::pin(async { Decision::Accept(None) })
    }
}

/// The notice of sessions without host approval (tests).
pub const DEV_PREVIEW_NOTICE: &str =
    "Development preview: anyone with the link joins without approval.";

pub struct HostConfig {
    pub host_name: String,
    pub machine: Option<String>,
    /// The session's guest permissions.
    pub permissions: Permissions,
    pub excludes: Excludes,
    /// Files that need Run Full access to change (`codeWithMe.protectedFiles`).
    pub protected: Excludes,
    /// Larger files are not shared (bytes).
    pub max_file_size: usize,
    pub notice: Option<String>,
}

/// A document as a participant opens it.
#[derive(Debug, Clone, PartialEq)]
pub struct DocOpened {
    pub doc: DocId,
    pub path: String,
    /// The whole document as one Yjs update.
    pub update: Vec<u8>,
    pub saved: SaveState,
    pub read_only: bool,
    /// A File Cache Conflict is open.
    pub conflict: bool,
    /// Read-only for everyone: too large to co-edit, or binary.
    pub view: Option<ViewOnly>,
}

struct Guest {
    /// The guest's endpoint key, to tell how its connection travels.
    remote: PeerKey,
    participant: Participant,
    policy: Policy,
    /// The current connection's queue; a closed queue while the guest reconnects.
    tx: mpsc::Sender<HostMessage>,
    /// Lets the guest take its place back after a dropped connection.
    resume: [u8; RESUME_TOKEN_LEN],
    /// Counts the guest's connections; a dropped connection only counts while it is the
    /// newest.
    epoch: u64,
    /// Its rate limits; kept across reconnects.
    limits: GuestLimits,
}

/// A queue that takes nothing: what a guest gets while it is away.
fn closed_queue() -> mpsc::Sender<HostMessage> {
    mpsc::channel(1).0
}

#[derive(Default)]
struct State {
    docs: BTreeMap<DocId, SharedDoc>,
    by_path: HashMap<String, DocId>,
    guests: BTreeMap<ParticipantId, Guest>,
    next_doc: u64,
    next_participant: u32,
    colors: ColorAssigner,
    /// Where everyone is, the host included (as sent, sanitized).
    presence: BTreeMap<ParticipantId, Presence>,
    /// Make Everyone Follow You is on.
    follow_host: bool,
    /// Guests removed by the host while they were reconnecting, to tell them why.
    removed: std::collections::BTreeSet<ParticipantId>,
}

/// Sends collected after the lock is released.
#[derive(Default)]
struct Outbox {
    guests: Vec<(mpsc::Sender<HostMessage>, HostMessage)>,
    events: Vec<HostEvent>,
}

pub struct HostSession {
    transport: Arc<dyn Transport>,
    backend: Arc<dyn Backend>,
    admission: Arc<dyn Admission>,
    sink: HostSink,
    session_id: [u8; SESSION_ID_LEN],
    /// Replaced by Regenerate Link, which makes the old link stop working.
    join_secret: Mutex<[u8; JOIN_SECRET_LEN]>,
    host: Participant,
    permissions: Mutex<Permissions>,
    excludes: Excludes,
    protected: Excludes,
    max_file_size: usize,
    machine: Option<String>,
    notice: Option<String>,
    state: Mutex<State>,
    calls: CallState,
    share: Mutex<ShareState>,
    ended: AtomicBool,
    audit: Mutex<Option<SharedAudit>>,
    /// How long a guest whose connection dropped keeps its place.
    grace: Mutex<Duration>,
}

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("the OS random number generator works");
    b
}

impl HostSession {
    /// Start a session on `transport` (already bound) and accept guests. Needs a tokio
    /// runtime.
    pub fn start(
        transport: Arc<dyn Transport>,
        backend: Arc<dyn Backend>,
        config: HostConfig,
        admission: Arc<dyn Admission>,
        sink: HostSink,
    ) -> Arc<Self> {
        let session = Arc::new(Self {
            transport,
            backend,
            admission,
            sink,
            session_id: random(),
            join_secret: Mutex::new(random()),
            host: Participant {
                id: ParticipantId::HOST,
                name: clean_name(&config.host_name),
                role: Role::Host,
                color: ColorAssigner::host(),
                permissions: Permissions::FULL_ACCESS,
                reconnecting: false,
            },
            permissions: Mutex::new(config.permissions),
            excludes: config.excludes,
            protected: config.protected,
            max_file_size: config.max_file_size,
            machine: config.machine,
            notice: config.notice,
            state: Mutex::new(State {
                next_doc: 1,
                next_participant: 1,
                ..State::default()
            }),
            calls: CallState::default(),
            share: Mutex::new(ShareState::default()),
            ended: AtomicBool::new(false),
            audit: Mutex::new(None),
            grace: Mutex::new(HOST_GRACE),
        });
        let accepting = session.clone();
        tokio::spawn(async move {
            while let Some(conn) = accepting.transport.accept().await {
                if accepting.is_ended() {
                    break;
                }
                tokio::spawn(accepting.clone().serve(conn));
            }
        });
        session
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("collab state lock poisoned")
    }

    pub fn is_ended(&self) -> bool {
        self.ended.load(Ordering::SeqCst)
    }

    /// How long a guest whose connection dropped keeps its place (default
    /// [`HOST_GRACE`]); zero removes it at once.
    pub fn set_reconnect_grace(&self, grace: Duration) {
        *self.grace.lock().expect("grace lock poisoned") = grace;
    }

    fn grace(&self) -> Duration {
        *self.grace.lock().expect("grace lock poisoned")
    }

    /// The rules for a guest with `permissions`.
    fn policy_for(&self, permissions: Permissions) -> Policy {
        Policy::new(permissions, self.excludes.clone()).with_protected(self.protected.clone())
    }

    /// The invitation link.
    pub fn link(&self) -> SessionLink {
        SessionLink {
            host_name: self.host.name.clone(),
            machine: self.machine.clone(),
            session_id: self.session_id,
            host_id: self.transport.local_key(),
            join_secret: self.join_secret(),
            relay: self.transport.relay_url(),
            addrs: self.transport.addresses(),
        }
    }

    /// Write the session log to `log` from now on; records the start of the session.
    pub fn set_audit(&self, log: SharedAudit) {
        log.record(AuditEvent::SessionStarted {
            host: self.host.name.clone(),
            permissions: self.permissions(),
        });
        *self.audit.lock().expect("audit lock poisoned") = Some(log);
    }

    /// The session id as hex (names the session log).
    pub fn session_hex(&self) -> String {
        data_encoding::HEXLOWER.encode(&self.session_id)
    }

    pub(crate) fn audit(&self, event: AuditEvent) {
        let log = self.audit.lock().expect("audit lock poisoned").clone();
        if let Some(log) = log {
            log.record(event);
        }
    }

    /// A guest's name for the log.
    pub(crate) fn guest_name(&self, id: ParticipantId) -> String {
        self.state()
            .guests
            .get(&id)
            .map(|g| g.participant.name.clone())
            .unwrap_or_default()
    }

    fn join_secret(&self) -> [u8; JOIN_SECRET_LEN] {
        *self.join_secret.lock().expect("join secret lock poisoned")
    }

    /// Regenerate Link: a new join secret, so the old link (and every copy of it) stops
    /// working. Guests in the session stay; requests that are waiting were checked already.
    pub fn regenerate_link(&self) -> SessionLink {
        *self.join_secret.lock().expect("join secret lock poisoned") = random();
        self.audit(AuditEvent::LinkRegenerated);
        self.link()
    }

    /// Permissions for the whole session: new guests get them, and every guest in the
    /// session gets them at once (a guest's own permissions are replaced as well).
    pub fn set_permissions(&self, permissions: Permissions) {
        *self.permissions.lock().expect("permissions lock poisoned") = permissions;
        self.audit(AuditEvent::PermissionsChanged {
            participant: None,
            name: None,
            permissions,
        });
        let ids: Vec<ParticipantId> = self.state().guests.keys().copied().collect();
        self.apply_permissions(&ids, permissions);
        (self.sink)(HostEvent::Changed);
    }

    /// Permissions of one guest (the participant's Permissions menu). `false` when the
    /// guest is no longer in the session.
    pub fn set_guest_permissions(&self, id: ParticipantId, permissions: Permissions) -> bool {
        if !self.state().guests.contains_key(&id) {
            return false;
        }
        self.audit(AuditEvent::PermissionsChanged {
            participant: Some(id),
            name: Some(self.guest_name(id)),
            permissions,
        });
        self.apply_permissions(&[id], permissions);
        true
    }

    /// The new permissions count from the next request on: the guest's policy is replaced,
    /// the guest is told, and everyone sees the new participant list.
    fn apply_permissions(&self, ids: &[ParticipantId], permissions: Permissions) {
        let mut out = Outbox::default();
        {
            let mut st = self.state();
            let mut changed = false;
            for id in ids {
                let Some(g) = st.guests.get_mut(id) else {
                    continue;
                };
                if g.participant.permissions == permissions {
                    continue;
                }
                g.participant.permissions = permissions;
                g.policy = self.policy_for(permissions);
                out.guests
                    .push((g.tx.clone(), HostMessage::Permissions { permissions }));
                changed = true;
            }
            // What each guest may see of the others can change with its permissions.
            for id in ids {
                if let Some(g) = st.guests.get(id) {
                    presence_snapshot(&st, *id, g, &mut out);
                }
            }
            if changed {
                participants_changed(&self.host, &st, &mut out);
            }
        }
        self.deliver(out);
        // Terminals and runs the guests may see now; lost terminal access closes theirs.
        self.permissions_applied(ids);
    }

    /// Remove Guest: the guest is disconnected at once and told why. Removing does not ban;
    /// with the same link the guest can ask again ([`Self::regenerate_link`] prevents it).
    pub fn remove(&self, id: ParticipantId) -> bool {
        let tx = {
            let mut st = self.state();
            let Some(g) = st.guests.get(&id) else {
                return false;
            };
            let (tx, away) = (g.tx.clone(), g.participant.reconnecting);
            if away {
                st.removed.insert(id);
            }
            tx
        };
        let _ = tx.try_send(HostMessage::Ended {
            reason: REMOVED.into(),
        });
        self.remove_guest(id, Leaving::Removed);
        true
    }

    pub fn notice(&self) -> Option<String> {
        self.notice.clone()
    }

    pub fn permissions(&self) -> Permissions {
        *self.permissions.lock().expect("permissions lock poisoned")
    }

    /// The host first, then the guests in join order.
    pub fn participants(&self) -> Vec<Participant> {
        participants(&self.host, &self.state())
    }

    /// How each connected guest's connection travels (direct or through the relay).
    pub fn connection_paths(&self) -> Vec<(ParticipantId, ConnectionPath)> {
        let remotes: Vec<(ParticipantId, PeerKey)> = self
            .state()
            .guests
            .iter()
            .map(|(id, g)| (*id, g.remote))
            .collect();
        remotes
            .into_iter()
            .filter_map(|(id, key)| Some((id, self.transport.path(&key)?)))
            .collect()
    }

    /// The state of the connection to the relay, when the session uses one.
    pub fn relay_status(&self) -> Option<RelayStatus> {
        self.transport.relay_status()
    }

    /// Where everyone is (the host sees all files), in participant order.
    pub fn presence(&self) -> Vec<(ParticipantId, Presence)> {
        self.state()
            .presence
            .iter()
            .map(|(id, p)| (*id, p.clone()))
            .collect()
    }

    /// Whether Make Everyone Follow You is on.
    pub fn follow_everyone(&self) -> bool {
        self.state().follow_host
    }

    /// Make Everyone Follow You: every guest (and every guest that joins later) is asked to
    /// follow the host, or no longer asked. Guests can still stop following.
    pub fn set_follow_everyone(&self, on: bool) {
        let mut out = Outbox::default();
        {
            let mut st = self.state();
            if st.follow_host == on {
                return;
            }
            st.follow_host = on;
            for g in st.guests.values() {
                out.guests
                    .push((g.tx.clone(), HostMessage::FollowHost { on }));
            }
            out.events.push(HostEvent::Changed);
        }
        self.deliver(out);
    }

    /// Where the host is (its active editor, caret, viewport, whom it follows).
    pub fn host_presence(&self, presence: Presence) {
        self.set_presence(ParticipantId::HOST, presence);
    }

    /// Store a participant's presence and pass it on: to every guest as far as the guest
    /// may see it, and to the host's UI when it is a guest's.
    fn set_presence(&self, from: ParticipantId, presence: Presence) {
        let mut out = Outbox::default();
        {
            let mut st = self.state();
            let known: Vec<ParticipantId> = std::iter::once(ParticipantId::HOST)
                .chain(st.guests.keys().copied())
                .collect();
            let presence = presence.sanitized(from, &known);
            if st.presence.get(&from) == Some(&presence) {
                return;
            }
            st.presence.insert(from, presence.clone());
            for (id, g) in &st.guests {
                if *id == from {
                    continue;
                }
                out.guests.push((
                    g.tx.clone(),
                    HostMessage::Presence {
                        participant: from,
                        presence: presence.visible_to(Some(&g.policy)),
                    },
                ));
            }
            if from != ParticipantId::HOST {
                out.events.push(HostEvent::Presence {
                    participant: from,
                    presence,
                });
            }
        }
        self.deliver(out);
    }

    /// Messages queued for the slowest connected guest (the backlog). A guest whose queue
    /// fills up ([`GUEST_QUEUE`] messages) is cut off, so this never grows without bound; the
    /// stress test asserts it stays far below that.
    pub fn backlog(&self) -> usize {
        self.state()
            .guests
            .values()
            .filter(|g| !g.tx.is_closed())
            .map(|g| g.tx.max_capacity() - g.tx.capacity())
            .max()
            .unwrap_or(0)
    }

    /// Files guests can open.
    pub fn shared_files(&self) -> Vec<SharedFile> {
        shared_files(&self.state())
    }

    fn deliver(&self, out: Outbox) {
        for (tx, msg) in out.guests {
            // A guest that is reconnecting gets nothing; it resynchronizes when it is back.
            if tx.is_closed() {
                continue;
            }
            // A guest that does not keep up is cut off (its reader ends the connection).
            if tx.try_send(msg).is_err() {
                tracing::warn!("Code With Me guest queue full or closed");
            }
        }
        for e in out.events {
            (self.sink)(e);
        }
    }

    // ---- the host's editor ------------------------------------------------------------

    /// Share a file the host opens (or has open: `buffer` is its editor text). Excluded files,
    /// `.git/` and files over the size limit are refused and stay unshared.
    pub async fn open_doc(&self, path: &str, buffer: Option<String>) -> Result<DocOpened, String> {
        // A file guests view read-only stays a normal tab for the host, who edits it on disk.
        let viewed = {
            let st = self.state();
            st.by_path
                .get(path)
                .and_then(|id| st.docs[id].view.map(|v| (st.docs[id].path.clone(), v)))
        };
        if let Some((path, view)) = viewed {
            return Err(not_shared(&path, view));
        }
        if let Some(opened) = self.join_doc(ParticipantId::HOST, path) {
            return Ok(opened);
        }
        Policy::new(Permissions::READ_ONLY, self.excludes.clone())
            .check_path(path)
            .map_err(|d| d.to_string())?;
        let backend = self.backend.clone();
        let rel = path.to_string();
        let limit = self.max_file_size;
        let shareable = tokio::task::spawn_blocking(move || read_shared(&*backend, &rel, limit))
            .await
            .map_err(|e| e.to_string())??;
        if let Some(view) = shareable.view {
            return Err(not_shared(path, view));
        }
        let disk = shareable.text;
        let mut out = Outbox::default();
        let opened = {
            let mut st = self.state();
            if !st.by_path.contains_key(path) {
                let id = DocId(st.next_doc);
                st.next_doc += 1;
                st.by_path.insert(path.to_string(), id);
                st.docs.insert(
                    id,
                    SharedDoc::new(id, path.to_string(), &disk, buffer.as_deref()),
                );
            }
            let opened = open_in(&mut st, ParticipantId::HOST, path, false).expect("just added");
            files_changed(&st, &mut out);
            opened
        };
        self.deliver(out);
        Ok(opened)
    }

    /// Add `who` to an existing document.
    fn join_doc(&self, who: ParticipantId, path: &str) -> Option<DocOpened> {
        let mut st = self.state();
        let opened = open_in(&mut st, who, path, false)?;
        let mut out = Outbox::default();
        if who == ParticipantId::HOST {
            files_changed(&st, &mut out);
        }
        drop(st);
        self.deliver(out);
        Some(opened)
    }

    /// An edit in the host's editor.
    pub fn host_update(&self, doc: DocId, update: &[u8]) -> Result<(), String> {
        self.apply_update(ParticipantId::HOST, doc, update)
    }

    /// The host's caret and selection.
    pub fn host_awareness(&self, doc: DocId, update: &[u8]) {
        self.apply_awareness(&self.host.clone(), doc, update);
    }

    /// The host closed the file's tab: guests can no longer open it, but keep editing it if
    /// they have it open.
    pub fn host_close(&self, doc: DocId) {
        self.close_doc(ParticipantId::HOST, doc);
    }

    /// Write a document to disk (the host's or a guest's Save).
    pub async fn save(&self, doc: DocId) -> Result<SaveState, String> {
        let (path, text, eol) = {
            let st = self.state();
            let d = st.docs.get(&doc).ok_or("the file is no longer shared")?;
            (d.path.clone(), d.text(), d.eol)
        };
        let backend = self.backend.clone();
        let (rel, contents) = (path.clone(), with_eol(&text, eol));
        tokio::task::spawn_blocking(move || backend.write_file(&rel, &contents))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let mut out = Outbox::default();
        let saved = {
            let mut st = self.state();
            let guests = &st.guests;
            let Some(d) = st.docs.get(&doc) else {
                return Err("the file is no longer shared".into());
            };
            let recipients = recipients(guests, d, None);
            let d = st.docs.get_mut(&doc).expect("checked");
            // Saving during a File Cache Conflict keeps the memory changes (the host's Save;
            // guests cannot save then).
            let settled = d.has_conflict();
            d.mark_saved(text);
            announce(d, recipients, None, true, settled, &mut out);
            d.save_state()
        };
        self.deliver(out);
        Ok(saved)
    }

    // ---- the project's files ----------------------------------------------------------

    /// Files changed on disk (the project watcher): shared documents without unsaved
    /// changes take the new text as minimal edits; deleted files are closed for everyone.
    pub async fn on_fs_changed(&self, changes: &FsChanges) {
        self.forward_fs(changes);
        let changed: Vec<(DocId, String)> = {
            let st = self.state();
            changes
                .changed
                .iter()
                .filter_map(|p| st.by_path.get(p).map(|id| (*id, p.clone())))
                .collect()
        };
        for (doc, path) in changed {
            let backend = self.backend.clone();
            let rel = path.clone();
            let disk = tokio::task::spawn_blocking(move || backend.read_optional(&rel)).await;
            let Ok(Ok(Some(disk))) = disk else { continue };
            self.apply_disk(doc, &disk);
        }
        for path in &changes.removed {
            self.on_deleted(path);
        }
    }

    fn apply_disk(&self, doc: DocId, disk: &str) {
        let mut out = Outbox::default();
        {
            let mut st = self.state();
            let guests = &st.guests;
            let Some(d) = st.docs.get(&doc) else { return };
            let all = recipients(guests, d, None);
            let d = st.docs.get_mut(&doc).expect("checked");
            let had_conflict = d.has_conflict();
            let change = d.apply_disk(disk);
            let conflict = had_conflict != d.has_conflict();
            match change {
                DiskChange::Applied(update) => {
                    announce(d, all, Some(update), true, conflict, &mut out)
                }
                DiskChange::Matches => announce(d, all, None, true, conflict, &mut out),
                DiskChange::Conflict | DiskChange::Unchanged if conflict => {
                    announce(d, all, None, false, true, &mut out)
                }
                DiskChange::Conflict | DiskChange::Unchanged => return,
            }
        }
        self.deliver(out);
    }

    /// The host's user answered a File Cache Conflict ([`HostEvent::DocConflict`]): Load
    /// File System Changes or Keep Memory Changes. Everyone gets the result.
    pub fn resolve_conflict(&self, doc: DocId, how: Resolution) -> Result<(), String> {
        let mut out = Outbox::default();
        {
            let mut st = self.state();
            let guests = &st.guests;
            let d = st.docs.get(&doc).ok_or("The file is no longer shared.")?;
            let all = recipients(guests, d, None);
            let d = st.docs.get_mut(&doc).expect("checked");
            let update = d.resolve_conflict(how)?;
            announce(d, all, update, true, true, &mut out);
        }
        self.deliver(out);
        Ok(())
    }

    /// The host renamed or moved a file or folder.
    pub fn on_renamed(&self, from: &str, to: &str) {
        let mut out = Outbox::default();
        {
            let mut st = self.state();
            let moved: Vec<(DocId, String)> = st
                .docs
                .values()
                .filter_map(|d| {
                    let rest = d.path.strip_prefix(from)?;
                    (rest.is_empty() || rest.starts_with('/'))
                        .then(|| (d.id, format!("{to}{rest}")))
                })
                .collect();
            if moved.is_empty() {
                return;
            }
            for (id, path) in moved {
                let old = std::mem::replace(
                    &mut st.docs.get_mut(&id).expect("listed").path,
                    path.clone(),
                );
                st.by_path.remove(&old);
                st.by_path.insert(path.clone(), id);
                let d = &st.docs[&id];
                // Under its new name the file may be hidden from a guest (`.env.example`
                // renamed to `.env`): the document closes for that guest.
                let mut hidden = Vec::new();
                for who in &d.open_by {
                    let Some(g) = st.guests.get(who) else {
                        continue;
                    };
                    if g.policy.check_path(&path).is_err() {
                        hidden.push(*who);
                        let name = path.rsplit('/').next().unwrap_or(&path);
                        out.guests.push((
                            g.tx.clone(),
                            HostMessage::DocClosed {
                                doc: id,
                                reason: format!("{name} is not shared with guests"),
                            },
                        ));
                    } else {
                        out.guests.push((
                            g.tx.clone(),
                            HostMessage::DocRenamed {
                                doc: id,
                                path: path.clone(),
                            },
                        ));
                    }
                }
                if d.open_by.contains(&ParticipantId::HOST) {
                    out.events.push(HostEvent::DocRenamed { doc: id, path });
                }
                for who in hidden {
                    leave_doc(&mut st, who, id, &mut out);
                }
            }
            files_changed(&st, &mut out);
        }
        self.deliver(out);
    }

    /// A file or folder was deleted: its documents close for everyone.
    pub fn on_deleted(&self, path: &str) {
        let mut out = Outbox::default();
        {
            let mut st = self.state();
            let gone: Vec<DocId> = st
                .docs
                .values()
                .filter(|d| {
                    d.path
                        .strip_prefix(path)
                        .is_some_and(|r| r.is_empty() || r.starts_with('/'))
                })
                .map(|d| d.id)
                .collect();
            if gone.is_empty() {
                return;
            }
            for id in gone {
                let d = st.docs.remove(&id).expect("listed");
                st.by_path.remove(&d.path);
                let name = d.path.rsplit('/').next().unwrap_or(&d.path);
                let reason = format!("{name} was deleted by {}", self.host.name);
                for tx in recipients(&st.guests, &d, None) {
                    out.guests.push((
                        tx,
                        HostMessage::DocClosed {
                            doc: id,
                            reason: reason.clone(),
                        },
                    ));
                }
                if d.open_by.contains(&ParticipantId::HOST) {
                    out.events.push(HostEvent::DocClosed { doc: id, reason });
                }
            }
            files_changed(&st, &mut out);
        }
        self.deliver(out);
    }

    /// End the session for everyone.
    pub async fn end(&self, reason: &str) {
        if self.ended.swap(true, Ordering::SeqCst) {
            return;
        }
        self.audit(AuditEvent::SessionEnded {
            reason: reason.to_string(),
        });
        self.end_shares();
        let guests = std::mem::take(&mut self.state().guests);
        for g in guests.values() {
            if let Err(e) = g.tx.try_send(HostMessage::Ended {
                reason: reason.to_string(),
            }) {
                tracing::warn!(
                    "Code With Me: cannot tell {} that the session ended: {e}",
                    g.participant.name
                );
            }
        }
        drop(guests);
        // Let the writers flush the goodbye before the connections close.
        tokio::time::sleep(Duration::from_millis(150)).await;
        self.transport.close().await;
        self.state().docs.clear();
    }

    // ---- guests -----------------------------------------------------------------------

    async fn serve(self: Arc<Self>, conn: Connection) {
        let Connection {
            remote,
            mut send,
            mut recv,
            keepalive,
        } = conn;
        let first = tokio::time::timeout(
            KNOCK_TIMEOUT,
            read_msg::<_, GuestMessage>(&mut recv, MAX_KNOCK),
        )
        .await;
        let attached = match first {
            Ok(Ok(Some(GuestMessage::Knock {
                protocol,
                session,
                name,
                proof,
            }))) => {
                self.admit(remote, &mut send, protocol, &session, name, &proof)
                    .await
            }
            Ok(Ok(Some(GuestMessage::Resume {
                protocol,
                session,
                participant,
                token,
            }))) => {
                self.resume(remote, &mut send, protocol, &session, participant, &token)
                    .await
            }
            _ => None,
        };
        let Some((id, epoch, rx)) = attached else {
            return;
        };
        self.run_guest(id, epoch, send, recv, keepalive, rx).await;
    }

    /// A knock: check the proof, ask the admission, and add the guest. Returns the guest's
    /// id, connection epoch and queue.
    async fn admit(
        &self,
        remote: PeerKey,
        send: &mut BoxWrite,
        protocol: u32,
        session: &[u8],
        name: String,
        proof: &[u8],
    ) -> Option<(ParticipantId, u64, mpsc::Receiver<HostMessage>)> {
        let host_key = self.transport.local_key();
        // Without the link there is no prompt and no answer.
        if session != self.session_id
            || !check_proof(
                &self.join_secret(),
                &host_key,
                &remote,
                &self.session_id,
                proof,
            )
        {
            tracing::info!("Code With Me: dropped a connection with a wrong join proof");
            self.audit(AuditEvent::Knock {
                name: String::new(),
                peer: short_peer(&remote),
                outcome: KnockOutcome::WrongProof,
                permissions: None,
            });
            return None;
        }
        if protocol != PROTOCOL_VERSION {
            refuse(send, OTHER_VERSION).await;
            return None;
        }
        let name = clean_name(&name);
        let code = verification_code(&host_key, &remote, &self.session_id);
        let decision = self
            .admission
            .decide(JoinRequest {
                guest: remote,
                name: name.clone(),
                code,
            })
            .await;
        let permissions = match decision {
            Decision::Accept(p) => p.unwrap_or_else(|| self.permissions()),
            Decision::Decline | Decision::Unanswered => {
                self.audit(AuditEvent::Knock {
                    name: name.clone(),
                    peer: short_peer(&remote),
                    outcome: if decision == Decision::Unanswered {
                        KnockOutcome::Unanswered
                    } else {
                        KnockOutcome::Declined
                    },
                    permissions: None,
                });
                let reason = match decision {
                    Decision::Unanswered => "The host did not answer.",
                    _ => "The host declined your request.",
                };
                refuse(send, reason).await;
                return None;
            }
        };
        if self.is_ended() {
            return None;
        }
        self.audit(AuditEvent::Knock {
            name: name.clone(),
            peer: short_peer(&remote),
            outcome: KnockOutcome::Accepted,
            permissions: Some(permissions),
        });

        let (tx, rx) = mpsc::channel(GUEST_QUEUE);
        let mut out = Outbox::default();
        let id = {
            let mut st = self.state();
            let id = ParticipantId(st.next_participant);
            st.next_participant += 1;
            let participant = Participant {
                id,
                name: name.clone(),
                role: Role::Guest,
                color: st.colors.next_guest(),
                permissions,
                reconnecting: false,
            };
            let resume: [u8; RESUME_TOKEN_LEN] = random();
            let welcome = HostMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                you: id,
                participants: participants(&self.host, &st)
                    .into_iter()
                    .chain([participant.clone()])
                    .collect(),
                files: shared_files(&st),
                notice: self.notice.clone(),
                resume: ByteBuf::from(resume.to_vec()),
                resumed: false,
            };
            out.guests.push((tx.clone(), welcome));
            st.guests.insert(
                id,
                Guest {
                    remote,
                    participant,
                    policy: self.policy_for(permissions),
                    tx,
                    resume,
                    epoch: 1,
                    limits: GuestLimits::new(Instant::now()),
                },
            );
            participants_changed(&self.host, &st, &mut out);
            let guest = &st.guests[&id];
            presence_snapshot(&st, id, guest, &mut out);
            self.sync_new_guest(&st, id, &mut out);
            if st.follow_host {
                out.guests
                    .push((guest.tx.clone(), HostMessage::FollowHost { on: true }));
            }
            id
        };
        out.events
            .push(HostEvent::Notice(format!("{name} joined the session.")));
        self.deliver(out);
        Some((id, 1, rx))
    }

    /// A guest whose connection dropped is back: same endpoint key, its resume token, and
    /// its place is still kept. It gets a `Welcome` (`resumed`), everyone's presence, and
    /// its terminals and runs again; its documents resynchronize with `DocSync`.
    async fn resume(
        &self,
        remote: PeerKey,
        send: &mut BoxWrite,
        protocol: u32,
        session: &[u8],
        participant: ParticipantId,
        token: &[u8],
    ) -> Option<(ParticipantId, u64, mpsc::Receiver<HostMessage>)> {
        if session != self.session_id || self.is_ended() {
            return None;
        }
        if protocol != PROTOCOL_VERSION {
            refuse(send, OTHER_VERSION).await;
            return None;
        }
        let (tx, rx) = mpsc::channel(GUEST_QUEUE);
        let mut out = Outbox::default();
        let attached = {
            let mut st = self.state();
            let removed = st.removed.remove(&participant);
            let known = st
                .guests
                .get(&participant)
                .is_some_and(|g| g.remote == remote && same_token(&g.resume, token));
            if known {
                let g = st.guests.get_mut(&participant).expect("checked");
                g.epoch += 1;
                g.tx = tx.clone();
                g.participant.reconnecting = false;
                let (epoch, name, resume) = (g.epoch, g.participant.name.clone(), g.resume);
                let welcome = HostMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                    you: participant,
                    participants: participants(&self.host, &st),
                    files: shared_files(&st),
                    notice: self.notice.clone(),
                    resume: ByteBuf::from(resume.to_vec()),
                    resumed: true,
                };
                out.guests.push((tx, welcome));
                participants_changed(&self.host, &st, &mut out);
                let guest = &st.guests[&participant];
                presence_snapshot(&st, participant, guest, &mut out);
                self.resync_guest(&st, participant, &mut out);
                if let Some(fs) = self.take_missed_fs(participant) {
                    out.guests.push((guest.tx.clone(), fs));
                }
                if st.follow_host {
                    out.guests
                        .push((guest.tx.clone(), HostMessage::FollowHost { on: true }));
                }
                Ok((epoch, name))
            } else if removed {
                Err(REMOVED)
            } else {
                Err(PLACE_GONE)
            }
        };
        match attached {
            Ok((epoch, name)) => {
                self.deliver(out);
                self.audit(AuditEvent::Reconnected { participant, name });
                Some((participant, epoch, rx))
            }
            Err(reason) => {
                refuse(send, reason).await;
                None
            }
        }
    }

    /// Serve one connection of an admitted guest until it ends.
    async fn run_guest(
        self: &Arc<Self>,
        id: ParticipantId,
        epoch: u64,
        mut send: BoxWrite,
        mut recv: BoxRead,
        keepalive: Box<dyn std::any::Any + Send>,
        mut rx: mpsc::Receiver<HostMessage>,
    ) {
        let writer = tokio::spawn(async move {
            let _keepalive = keepalive;
            while let Some(msg) = rx.recv().await {
                let last = matches!(msg, HostMessage::Ended { .. });
                match write_or_skip(&mut send, &msg, MAX_HOST_FRAME).await {
                    Ok(true) => {}
                    Ok(false) => tracing::warn!(
                        "Code With Me: not sending a message to a guest: it is larger than {} bytes",
                        MAX_HOST_FRAME
                    ),
                    Err(e) => {
                        tracing::warn!("Code With Me: writing to a guest failed: {e}");
                        break;
                    }
                }
                if last {
                    break;
                }
            }
            use tokio::io::AsyncWriteExt;
            let _ = send.shutdown().await;
        });

        let end = loop {
            match read_msg::<_, GuestMessage>(&mut recv, MAX_GUEST_FRAME).await {
                Ok(Some(GuestMessage::Leave)) => break ConnectionEnd::Left,
                Ok(None) => break ConnectionEnd::Lost,
                Err(e) => {
                    tracing::warn!("Code With Me: reading from a guest failed: {e}");
                    break ConnectionEnd::Lost;
                }
                Ok(Some(msg)) if !self.within_rate(id, &msg) => {}
                Ok(Some(GuestMessage::Call { id: call, payload })) => {
                    self.spawn_call(id, call, payload)
                }
                Ok(Some(msg)) => self.handle(id, msg).await,
            }
            if self.is_ended() || !self.is_current(id, epoch) {
                break ConnectionEnd::Gone;
            }
        };
        match end {
            ConnectionEnd::Left => self.remove_guest(id, Leaving::Left),
            ConnectionEnd::Lost => self.connection_lost(id, epoch),
            ConnectionEnd::Gone => {}
        }
        if tokio::time::timeout(WRITER_FLUSH, writer).await.is_err() {
            tracing::debug!("Code With Me: a guest's writer did not finish");
        }
    }

    /// Whether `epoch` is the guest's newest connection.
    fn is_current(&self, id: ParticipantId, epoch: u64) -> bool {
        self.state()
            .guests
            .get(&id)
            .is_some_and(|g| g.epoch == epoch)
    }

    /// A guest's connection dropped: keep its place for the grace period (everyone sees it
    /// as reconnecting), then remove it unless it came back.
    fn connection_lost(self: &Arc<Self>, id: ParticipantId, epoch: u64) {
        if self.is_ended() || !self.is_current(id, epoch) {
            return;
        }
        let grace = self.grace();
        if grace.is_zero() {
            self.remove_guest(id, Leaving::Lost);
            return;
        }
        let mut out = Outbox::default();
        let name = {
            let mut st = self.state();
            let Some(g) = st.guests.get_mut(&id) else {
                return;
            };
            g.participant.reconnecting = true;
            g.tx = closed_queue();
            let name = g.participant.name.clone();
            participants_changed(&self.host, &st, &mut out);
            name
        };
        self.deliver(out);
        self.audit(AuditEvent::ConnectionLost {
            participant: id,
            name,
        });
        let me = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(grace).await;
            let expired = me
                .state()
                .guests
                .get(&id)
                .is_some_and(|g| g.epoch == epoch && g.participant.reconnecting);
            if expired && !me.is_ended() {
                me.remove_guest(id, Leaving::Lost);
            }
        });
    }

    fn reply(&self, to: ParticipantId, msg: HostMessage) {
        let tx = self.state().guests.get(&to).map(|g| g.tx.clone());
        if let Some(tx) = tx {
            self.deliver(Outbox {
                guests: vec![(tx, msg)],
                events: Vec::new(),
            });
        }
    }

    async fn handle(&self, from: ParticipantId, msg: GuestMessage) {
        let Some((participant, policy)) = self
            .state()
            .guests
            .get(&from)
            .map(|g| (g.participant.clone(), g.policy.clone()))
        else {
            return;
        };
        let denied = policy
            .check(&msg)
            .err()
            .or_else(|| self.document_write_denied(&policy, &msg));
        if let Some(denied) = denied {
            // A client that keeps sending keys is refused once, not once per key.
            if !self.report_refused_input(from, &msg) {
                return;
            }
            let reason = denied.to_string();
            self.audit(AuditEvent::Denied {
                participant: from,
                name: participant.name.clone(),
                request: msg.name().to_string(),
                reason: reason.clone(),
            });
            let answer = match msg {
                GuestMessage::DocOpen { request, .. } => HostMessage::DocError {
                    request,
                    error: reason,
                },
                GuestMessage::DocSave { request, .. } => HostMessage::SaveResult {
                    request,
                    error: Some(reason),
                },
                _ => HostMessage::Denied { reason },
            };
            self.reply(from, answer);
            return;
        }
        match msg {
            GuestMessage::DocOpen { request, path } => {
                // Any file of the project the guest may see opens, not only the host's tabs.
                let answer = match self.ensure_doc(&policy, &path).await {
                    Err(error) => HostMessage::DocError { request, error },
                    Ok(()) => {
                        let mut st = self.state();
                        // Files that configure what runs on the host are read-only without
                        // Full access, like for every other way of changing them.
                        let read_only = policy.check_write_path(&path).is_err();
                        match open_in(&mut st, from, &path, read_only) {
                            Some(o) => HostMessage::DocOpened {
                                request,
                                doc: o.doc,
                                path: o.path,
                                update: ByteBuf::from(o.update),
                                saved: o.saved,
                                read_only: o.read_only,
                                conflict: o.conflict,
                                view: o.view,
                            },
                            None => HostMessage::DocError {
                                request,
                                error: "The file is no longer shared.".into(),
                            },
                        }
                    }
                };
                self.reply(from, answer);
            }
            GuestMessage::DocUpdate { doc, update } => {
                if let Err(e) = self.apply_update(from, doc, &update) {
                    self.reply(from, HostMessage::Denied { reason: e });
                }
            }
            GuestMessage::Awareness { doc, update } => {
                self.apply_awareness(&participant, doc, &update)
            }
            GuestMessage::DocSave { request, doc } => {
                let open = self
                    .state()
                    .docs
                    .get(&doc)
                    .is_some_and(|d| d.open_by.contains(&from));
                let conflict = self.state().docs.get(&doc).and_then(|d| {
                    d.has_conflict()
                        .then(|| d.path.rsplit('/').next().unwrap_or(&d.path).to_string())
                });
                let error = if !open {
                    Some("The file is not open.".into())
                } else if let Some(name) = conflict {
                    Some(format!(
                        "{name} changed on disk. The host is resolving the conflict; \
                         save again afterwards."
                    ))
                } else {
                    self.save(doc).await.err()
                };
                if error.is_none() {
                    let path = self.state().docs.get(&doc).map(|d| d.path.clone());
                    if let Some(path) = path {
                        self.audit(AuditEvent::File {
                            participant: from,
                            name: participant.name.clone(),
                            action: FileAction::Saved,
                            path,
                            to: None,
                        });
                    }
                }
                self.reply(from, HostMessage::SaveResult { request, error });
            }
            GuestMessage::DocClose { doc } => self.close_doc(from, doc),
            GuestMessage::DocSync {
                request,
                doc,
                state_vector,
            } => {
                let answer = {
                    let st = self.state();
                    match st.docs.get(&doc).filter(|d| d.open_by.contains(&from)) {
                        None => HostMessage::DocError {
                            request,
                            error: "The file is no longer shared.".into(),
                        },
                        Some(d) => match d.diff_since(&state_vector) {
                            Err(error) => HostMessage::DocError { request, error },
                            Ok((update, sv)) => HostMessage::DocSynced {
                                request,
                                doc,
                                path: d.path.clone(),
                                update: ByteBuf::from(update),
                                state_vector: ByteBuf::from(sv),
                                saved: d.save_state(),
                                read_only: policy.check_write_path(&d.path).is_err()
                                    || d.view.is_some(),
                                conflict: d.has_conflict(),
                                view: d.view,
                            },
                        },
                    }
                };
                self.reply(from, answer);
            }
            GuestMessage::Presence { presence } => self.set_presence(from, presence),
            GuestMessage::TerminalInput { terminal, data } => {
                if let Err(reason) = self.terminal_input(from, terminal, &data) {
                    self.reply(from, HostMessage::Denied { reason });
                }
            }
            GuestMessage::RunInput { run, data } => {
                if let Err(reason) = self.run_input(from, run, &data) {
                    self.reply(from, HostMessage::Denied { reason });
                }
            }
            GuestMessage::Knock { .. }
            | GuestMessage::Resume { .. }
            | GuestMessage::Leave
            | GuestMessage::Call { .. } => {}
        }
    }

    fn apply_update(&self, from: ParticipantId, doc: DocId, update: &[u8]) -> Result<(), String> {
        let mut out = Outbox::default();
        {
            let mut st = self.state();
            let guests = &st.guests;
            let Some(d) = st.docs.get(&doc) else {
                return Err("The file is no longer shared.".into());
            };
            if !d.open_by.contains(&from) {
                return Err("The file is not open.".into());
            }
            let others = recipients(guests, d, Some(from));
            let to_host = from != ParticipantId::HOST && d.open_by.contains(&ParticipantId::HOST);
            let d = st.docs.get_mut(&doc).expect("checked");
            d.apply_update(update)?;
            for tx in others {
                out.guests.push((
                    tx,
                    HostMessage::DocUpdate {
                        doc,
                        update: ByteBuf::from(update.to_vec()),
                    },
                ));
            }
            if to_host {
                out.events.push(HostEvent::DocUpdate {
                    doc,
                    update: update.to_vec(),
                    from: Some(from),
                });
            }
            // Someone typed the disk's text: the File Cache Conflict is over, and the
            // document is saved.
            if d.settle_conflict() {
                let d = &st.docs[&doc];
                announce(
                    d,
                    recipients(&st.guests, d, None),
                    None,
                    true,
                    true,
                    &mut out,
                );
            }
        }
        self.deliver(out);
        Ok(())
    }

    fn apply_awareness(&self, from: &Participant, doc: DocId, update: &[u8]) {
        let mut out = Outbox::default();
        {
            let mut st = self.state();
            let guests = &st.guests;
            let Some(d) = st.docs.get(&doc) else { return };
            if !d.open_by.contains(&from.id) {
                return;
            }
            let others = recipients(guests, d, Some(from.id));
            let to_host =
                from.id != ParticipantId::HOST && d.open_by.contains(&ParticipantId::HOST);
            let Some(stamped) = st
                .docs
                .get_mut(&doc)
                .expect("checked")
                .stamp_awareness(from, update)
            else {
                return;
            };
            for tx in others {
                out.guests.push((
                    tx,
                    HostMessage::Awareness {
                        doc,
                        update: ByteBuf::from(stamped.clone()),
                    },
                ));
            }
            if to_host {
                out.events.push(HostEvent::Awareness {
                    doc,
                    update: stamped,
                });
            }
        }
        self.deliver(out);
    }

    fn close_doc(&self, who: ParticipantId, doc: DocId) {
        let mut out = Outbox::default();
        {
            let mut st = self.state();
            if !st.docs.contains_key(&doc) {
                return;
            }
            leave_doc(&mut st, who, doc, &mut out);
            if who == ParticipantId::HOST {
                files_changed(&st, &mut out);
            }
        }
        self.deliver(out);
    }

    fn remove_guest(&self, id: ParticipantId, why: Leaving) {
        let mut out = Outbox::default();
        let name = {
            let mut st = self.state();
            let Some(guest) = st.guests.remove(&id) else {
                return;
            };
            st.presence.remove(&id);
            let docs: Vec<DocId> = st.docs.keys().copied().collect();
            for doc in docs {
                leave_doc(&mut st, id, doc, &mut out);
            }
            participants_changed(&self.host, &st, &mut out);
            guest.participant.name
        };
        if !self.is_ended() {
            self.audit(if matches!(why, Leaving::Removed | Leaving::Flooded) {
                AuditEvent::Removed {
                    participant: id,
                    name: name.clone(),
                }
            } else {
                AuditEvent::Left {
                    participant: id,
                    name: name.clone(),
                }
            });
            let text = match why {
                Leaving::Removed => format!("{name} was removed from the session."),
                Leaving::Left => format!("{name} left the session."),
                Leaving::Lost => format!("{name} lost the connection and did not come back."),
                Leaving::Flooded => {
                    format!("{name} was disconnected for sending too many messages.")
                }
            };
            out.events.push(HostEvent::Notice(text));
        }
        self.deliver(out);
        if !self.is_ended() {
            self.guest_gone(id);
        }
    }
}

impl HostSession {
    /// Count a guest's message against its rate limits ([`crate::rate`]). `false`: the
    /// message was refused (and answered) or the guest was disconnected for flooding.
    fn within_rate(&self, from: ParticipantId, msg: &GuestMessage) -> bool {
        let verdict = {
            let mut st = self.state();
            let Some(g) = st.guests.get_mut(&from) else {
                return false;
            };
            g.limits.check(msg, Instant::now())
        };
        let report = match verdict {
            Verdict::Allow => return true,
            Verdict::Refuse { report } => report,
            Verdict::Flood => {
                self.audit(AuditEvent::Denied {
                    participant: from,
                    name: self.guest_name(from),
                    request: msg.name().to_string(),
                    reason: crate::rate::FLOODED_LOG.into(),
                });
                let tx = self.state().guests.get(&from).map(|g| g.tx.clone());
                if let Some(tx) = tx {
                    let _ = tx.try_send(HostMessage::Ended {
                        reason: crate::rate::FLOODED.into(),
                    });
                }
                self.remove_guest(from, Leaving::Flooded);
                return false;
            }
        };
        if report {
            self.audit(AuditEvent::Denied {
                participant: from,
                name: self.guest_name(from),
                request: msg.name().to_string(),
                reason: crate::rate::TOO_MANY.into(),
            });
        }
        let error = crate::rate::TOO_MANY.to_string();
        let answer = match msg {
            GuestMessage::Call { id, .. } => Some(HostMessage::CallResult {
                id: *id,
                result: Err(error),
            }),
            GuestMessage::DocOpen { request, .. } | GuestMessage::DocSync { request, .. } => {
                Some(HostMessage::DocError {
                    request: *request,
                    error,
                })
            }
            GuestMessage::DocSave { request, .. } => Some(HostMessage::SaveResult {
                request: *request,
                error: Some(error),
            }),
            // Input has no answer of its own: tell the guest once per burst.
            _ => report.then_some(HostMessage::Denied { reason: error }),
        };
        if let Some(answer) = answer {
            self.reply(from, answer);
        }
        false
    }
}

const OTHER_VERSION: &str = "The host uses a different version of Forge.";
const REMOVED: &str = "The host removed you from the session.";
const PLACE_GONE: &str =
    "The host no longer keeps your place in the session. Join again with the session link.";

async fn refuse(send: &mut BoxWrite, reason: &str) {
    let _ = write_msg(
        send,
        &HostMessage::Refused {
            reason: reason.to_string(),
        },
        MAX_HOST_FRAME,
    )
    .await;
}

/// A file as guests get it.
pub(crate) struct Shareable {
    /// The text (empty for binary files).
    pub text: String,
    /// Set when the file is read-only for everyone.
    pub view: Option<ViewOnly>,
}

/// Read a file for sharing: inside the project (also after resolving symlinks) and at most
/// [`MAX_VIEW_SIZE`] (or `limit`, if that is higher). Text files over `limit` and binary files
/// are view-only.
pub(crate) fn read_shared(
    backend: &dyn Backend,
    rel: &str,
    limit: usize,
) -> Result<Shareable, String> {
    let abs = backend.resolve(rel).map_err(|e| e.to_string())?;
    let max = limit.max(MAX_VIEW_SIZE);
    if backend.info().remote.is_none() {
        let root = backend
            .info()
            .root
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let real = abs.canonicalize().map_err(|e| e.to_string())?;
        if !real.starts_with(&root) || real.starts_with(root.join(".git")) {
            return Err("The file is outside the project and cannot be shared.".into());
        }
        if std::fs::metadata(&real)
            .map(|m| m.len() as usize > max)
            .unwrap_or(false)
        {
            return Err(too_large(&real));
        }
    }
    let bytes = backend.read_bytes(rel).map_err(|e| e.to_string())?;
    if bytes.len() > max {
        return Err(too_large(&abs));
    }
    Ok(shareable(bytes, limit))
}

/// Classify a file's bytes: binary (a NUL byte near the start, or not UTF-8), text too large
/// to co-edit, or text.
pub(crate) fn shareable(bytes: Vec<u8>, limit: usize) -> Shareable {
    let size = bytes.len() as u64;
    let binary = Shareable {
        text: String::new(),
        view: Some(ViewOnly::Binary { size }),
    };
    if bytes.iter().take(8192).any(|b| *b == 0) {
        return binary;
    }
    match String::from_utf8(bytes) {
        Err(_) => binary,
        Ok(text) => {
            let view = (text.len() > limit).then_some(ViewOnly::Large { size });
            Shareable { text, view }
        }
    }
}

fn too_large(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("{name} is larger than the shared file size limit and is not shared.")
}

/// Why the host's own tab of a view-only file is not shared.
fn not_shared(path: &str, view: ViewOnly) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match view {
        ViewOnly::Large { .. } => {
            format!("{name} is larger than the shared file size limit and is not shared.")
        }
        ViewOnly::Binary { .. } => format!("{name} is a binary file and is not shared."),
    }
}

/// Why nobody may change a view-only file.
pub(crate) fn view_only_reason(path: &str, view: ViewOnly) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match view {
        ViewOnly::Large { .. } => format!(
            "{name} is larger than the shared file size limit and is read-only in this session."
        ),
        ViewOnly::Binary { .. } => format!("{name} is a binary file and is read-only."),
    }
}

fn participants(host: &Participant, st: &State) -> Vec<Participant> {
    std::iter::once(host.clone())
        .chain(st.guests.values().map(|g| g.participant.clone()))
        .collect()
}

fn shared_files(st: &State) -> Vec<SharedFile> {
    let mut files: Vec<SharedFile> = st
        .docs
        .values()
        .filter(|d| d.open_by.contains(&ParticipantId::HOST))
        .map(|d| SharedFile {
            path: d.path.clone(),
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

/// Everyone else's presence as guest `id` may see it (after joining or a permission change).
fn presence_snapshot(st: &State, id: ParticipantId, guest: &Guest, out: &mut Outbox) {
    for (participant, presence) in &st.presence {
        if *participant == id {
            continue;
        }
        out.guests.push((
            guest.tx.clone(),
            HostMessage::Presence {
                participant: *participant,
                presence: presence.visible_to(Some(&guest.policy)),
            },
        ));
    }
}

fn files_changed(st: &State, out: &mut Outbox) {
    let files = shared_files(st);
    for g in st.guests.values() {
        out.guests.push((
            g.tx.clone(),
            HostMessage::SharedFiles {
                files: files.clone(),
            },
        ));
    }
    out.events.push(HostEvent::Changed);
}

fn participants_changed(host: &Participant, st: &State, out: &mut Outbox) {
    let list = participants(host, st);
    for g in st.guests.values() {
        out.guests.push((
            g.tx.clone(),
            HostMessage::Participants {
                participants: list.clone(),
            },
        ));
    }
    out.events.push(HostEvent::Changed);
}

/// Queues of the guests with `doc` open, except `except`.
fn recipients(
    guests: &BTreeMap<ParticipantId, Guest>,
    doc: &SharedDoc,
    except: Option<ParticipantId>,
) -> Vec<mpsc::Sender<HostMessage>> {
    doc.open_by
        .iter()
        .filter(|p| Some(**p) != except)
        .filter_map(|p| guests.get(p).map(|g| g.tx.clone()))
        .collect()
}

/// Tell everyone with `d` open what changed outside their editors: an `update` from disk
/// (or a conflict answer), a new save state (`saved`), and whether a File Cache Conflict
/// opened or closed (`conflict`; the host's user hears about it even without the tab open).
fn announce(
    d: &SharedDoc,
    to: Vec<mpsc::Sender<HostMessage>>,
    update: Option<Vec<u8>>,
    saved: bool,
    conflict: bool,
    out: &mut Outbox,
) {
    let doc = d.id;
    let host_has_it = d.open_by.contains(&ParticipantId::HOST);
    let state = d.save_state();
    let open = d.has_conflict();
    for tx in to {
        if let Some(update) = &update {
            out.guests.push((
                tx.clone(),
                HostMessage::DocUpdate {
                    doc,
                    update: ByteBuf::from(update.clone()),
                },
            ));
        }
        if conflict {
            out.guests.push((
                tx.clone(),
                HostMessage::DocConflict {
                    doc,
                    conflict: open,
                },
            ));
        }
        if saved {
            out.guests.push((
                tx,
                HostMessage::DocSaved {
                    doc,
                    saved: state.clone(),
                },
            ));
        }
    }
    if host_has_it {
        if let Some(update) = update {
            out.events.push(HostEvent::DocUpdate {
                doc,
                update,
                from: None,
            });
        }
    }
    if conflict {
        out.events.push(HostEvent::DocConflict {
            doc,
            path: d.path.clone(),
            conflict: open,
        });
    }
    if saved && host_has_it {
        out.events.push(HostEvent::DocSaved { doc, saved: state });
    }
}

fn open_in(st: &mut State, who: ParticipantId, path: &str, read_only: bool) -> Option<DocOpened> {
    let id = *st.by_path.get(path)?;
    let d = st.docs.get_mut(&id)?;
    d.open_by.insert(who);
    Some(DocOpened {
        doc: id,
        path: d.path.clone(),
        update: d.full_update(),
        saved: d.save_state(),
        read_only: read_only || d.view.is_some(),
        conflict: d.has_conflict(),
        view: d.view,
    })
}

/// `who` closed `doc`: their carets go away; a document nobody has open and without unsaved
/// changes is dropped.
fn leave_doc(st: &mut State, who: ParticipantId, doc: DocId, out: &mut Outbox) {
    let Some(d) = st.docs.get_mut(&doc) else {
        return;
    };
    if !d.open_by.contains(&who) {
        return;
    }
    let removal = d.remove_participant(who);
    let d = &st.docs[&doc];
    if let Some(update) = removal {
        for tx in recipients(&st.guests, d, None) {
            out.guests.push((
                tx,
                HostMessage::Awareness {
                    doc,
                    update: ByteBuf::from(update.clone()),
                },
            ));
        }
        if who != ParticipantId::HOST && d.open_by.contains(&ParticipantId::HOST) {
            out.events.push(HostEvent::Awareness { doc, update });
        }
    }
    if d.open_by.is_empty() && !d.is_dirty() {
        let path = d.path.clone();
        st.docs.remove(&doc);
        st.by_path.remove(&path);
    }
}
