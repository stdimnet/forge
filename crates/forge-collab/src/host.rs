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
use std::time::Duration;

use forge_backend::Backend;
use forge_core::FsChanges;
use serde_bytes::ByteBuf;
use tokio::sync::mpsc;

use crate::docs::{with_eol, DiskChange, SharedDoc};
use crate::excludes::Excludes;
use crate::link::{SessionLink, JOIN_SECRET_LEN, SESSION_ID_LEN};
use crate::permissions::{Permissions, Policy};
use crate::presence::{clean_name, ColorAssigner, Participant, ParticipantId, Role};
use crate::protocol::{
    DocId, GuestMessage, HostMessage, SaveState, SharedFile, MAX_GUEST_FRAME, MAX_HOST_FRAME,
    PROTOCOL_VERSION,
};
use crate::relay::{ConnectionPath, RelayStatus};
use crate::transport::{read_msg, write_msg, BoxFuture, Connection, PeerKey, Transport};
use crate::verify::{check_proof, verification_code};

// The host's side of the guests' project backend (tree, search, git, language servers).
#[path = "guest_backend/host_calls.rs"]
mod guest_calls;
use guest_calls::CallState;
pub use guest_calls::{virtual_root, HostServices, LspRequest};

/// Messages queued for one guest before the guest counts as too slow and is disconnected.
const GUEST_QUEUE: usize = 4096;
/// Time a new connection has to send its knock.
const KNOCK_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_KNOCK: usize = 16 * 1024;

/// What the host's UI needs to know.
#[derive(Debug, Clone, PartialEq)]
pub enum HostEvent {
    /// Participants or shared files changed; the session view is out of date.
    Changed,
    DocUpdate {
        doc: DocId,
        update: Vec<u8>,
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
}

struct Guest {
    /// The guest's endpoint key, to tell how its connection travels.
    remote: PeerKey,
    participant: Participant,
    policy: Policy,
    tx: mpsc::Sender<HostMessage>,
}

#[derive(Default)]
struct State {
    docs: BTreeMap<DocId, SharedDoc>,
    by_path: HashMap<String, DocId>,
    guests: BTreeMap<ParticipantId, Guest>,
    next_doc: u64,
    next_participant: u32,
    colors: ColorAssigner,
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
    join_secret: [u8; JOIN_SECRET_LEN],
    host: Participant,
    permissions: Mutex<Permissions>,
    excludes: Excludes,
    max_file_size: usize,
    machine: Option<String>,
    notice: Option<String>,
    state: Mutex<State>,
    calls: CallState,
    ended: AtomicBool,
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
            join_secret: random(),
            host: Participant {
                id: ParticipantId::HOST,
                name: clean_name(&config.host_name),
                role: Role::Host,
                color: ColorAssigner::host(),
                permissions: Permissions::FULL_ACCESS,
            },
            permissions: Mutex::new(config.permissions),
            excludes: config.excludes,
            max_file_size: config.max_file_size,
            machine: config.machine,
            notice: config.notice,
            state: Mutex::new(State {
                next_doc: 1,
                next_participant: 1,
                ..State::default()
            }),
            calls: CallState::default(),
            ended: AtomicBool::new(false),
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

    /// The invitation link.
    pub fn link(&self) -> SessionLink {
        SessionLink {
            host_name: self.host.name.clone(),
            machine: self.machine.clone(),
            session_id: self.session_id,
            host_id: self.transport.local_key(),
            join_secret: self.join_secret,
            relay: self.transport.relay_url(),
            addrs: self.transport.addresses(),
        }
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

    /// Files guests can open.
    pub fn shared_files(&self) -> Vec<SharedFile> {
        shared_files(&self.state())
    }

    fn deliver(&self, out: Outbox) {
        for (tx, msg) in out.guests {
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
        if let Some(opened) = self.join_doc(ParticipantId::HOST, path) {
            return Ok(opened);
        }
        Policy::new(Permissions::READ_ONLY, self.excludes.clone())
            .check_path(path)
            .map_err(|d| d.to_string())?;
        let backend = self.backend.clone();
        let rel = path.to_string();
        let limit = self.max_file_size;
        let disk = tokio::task::spawn_blocking(move || read_shared(&*backend, &rel, limit))
            .await
            .map_err(|e| e.to_string())??;
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
            let host_has_it = d.open_by.contains(&ParticipantId::HOST);
            let d = st.docs.get_mut(&doc).expect("checked");
            d.mark_saved(text);
            let saved = d.save_state();
            for tx in recipients {
                out.guests.push((
                    tx,
                    HostMessage::DocSaved {
                        doc,
                        saved: saved.clone(),
                    },
                ));
            }
            if host_has_it {
                out.events.push(HostEvent::DocSaved {
                    doc,
                    saved: saved.clone(),
                });
            }
            saved
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
            let host_has_it = d.open_by.contains(&ParticipantId::HOST);
            let name = d.path.rsplit('/').next().unwrap_or(&d.path).to_string();
            let d = st.docs.get_mut(&doc).expect("checked");
            match d.apply_disk(disk) {
                DiskChange::Unchanged => return,
                DiskChange::Conflict => out.events.push(HostEvent::Notice(format!(
                    "{name} changed on disk. The unsaved shared changes are kept."
                ))),
                DiskChange::Applied(update) => {
                    let saved = d.save_state();
                    for tx in all {
                        out.guests.push((
                            tx.clone(),
                            HostMessage::DocUpdate {
                                doc,
                                update: ByteBuf::from(update.clone()),
                            },
                        ));
                        out.guests.push((
                            tx,
                            HostMessage::DocSaved {
                                doc,
                                saved: saved.clone(),
                            },
                        ));
                    }
                    if host_has_it {
                        out.events.push(HostEvent::DocUpdate { doc, update });
                        out.events.push(HostEvent::DocSaved { doc, saved });
                    }
                }
            }
        }
        self.deliver(out);
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
                for tx in recipients(&st.guests, d, None) {
                    out.guests.push((
                        tx,
                        HostMessage::DocRenamed {
                            doc: id,
                            path: path.clone(),
                        },
                    ));
                }
                if d.open_by.contains(&ParticipantId::HOST) {
                    out.events.push(HostEvent::DocRenamed { doc: id, path });
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
        let guests = std::mem::take(&mut self.state().guests);
        for g in guests.values() {
            let _ = g.tx.try_send(HostMessage::Ended {
                reason: reason.to_string(),
            });
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
        let knock = tokio::time::timeout(
            KNOCK_TIMEOUT,
            read_msg::<_, GuestMessage>(&mut recv, MAX_KNOCK),
        )
        .await;
        let Ok(Ok(Some(GuestMessage::Knock {
            protocol,
            session,
            name,
            proof,
        }))) = knock
        else {
            return;
        };
        let host_key = self.transport.local_key();
        // Without the link there is no prompt and no answer.
        if session.as_slice() != self.session_id
            || !check_proof(
                &self.join_secret,
                &host_key,
                &remote,
                &self.session_id,
                &proof,
            )
        {
            tracing::info!("Code With Me: dropped a connection with a wrong join proof");
            return;
        }
        if protocol != PROTOCOL_VERSION {
            let reason = "The host uses a different version of Forge.".to_string();
            let _ = write_msg(&mut send, &HostMessage::Refused { reason }, MAX_HOST_FRAME).await;
            return;
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
                let reason = match decision {
                    Decision::Unanswered => "The host did not answer.",
                    _ => "The host declined your request.",
                };
                let _ = write_msg(
                    &mut send,
                    &HostMessage::Refused {
                        reason: reason.to_string(),
                    },
                    MAX_HOST_FRAME,
                )
                .await;
                return;
            }
        };
        if self.is_ended() {
            return;
        }

        let (tx, mut rx) = mpsc::channel(GUEST_QUEUE);
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
            };
            let welcome = HostMessage::Welcome {
                protocol: PROTOCOL_VERSION,
                you: id,
                participants: participants(&self.host, &st)
                    .into_iter()
                    .chain([participant.clone()])
                    .collect(),
                files: shared_files(&st),
                notice: self.notice.clone(),
            };
            out.guests.push((tx.clone(), welcome));
            st.guests.insert(
                id,
                Guest {
                    remote,
                    participant,
                    policy: Policy::new(permissions, self.excludes.clone()),
                    tx,
                },
            );
            participants_changed(&self.host, &st, &mut out);
            id
        };
        out.events
            .push(HostEvent::Notice(format!("{name} joined the session.")));
        self.deliver(out);

        let writer = tokio::spawn(async move {
            let _keepalive = keepalive;
            while let Some(msg) = rx.recv().await {
                let last = matches!(msg, HostMessage::Ended { .. });
                if write_msg(&mut send, &msg, MAX_HOST_FRAME).await.is_err() || last {
                    break;
                }
            }
            use tokio::io::AsyncWriteExt;
            let _ = send.shutdown().await;
        });

        loop {
            match read_msg::<_, GuestMessage>(&mut recv, MAX_GUEST_FRAME).await {
                Ok(Some(GuestMessage::Leave)) | Ok(None) | Err(_) => break,
                Ok(Some(GuestMessage::Call { id: call, payload })) => {
                    self.spawn_call(id, call, payload)
                }
                Ok(Some(msg)) => self.handle(id, msg).await,
            }
            if self.is_ended() || !self.state().guests.contains_key(&id) {
                break;
            }
        }
        self.remove_guest(id);
        let _ = writer.await;
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
            let reason = denied.to_string();
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
                let error = if open {
                    self.save(doc).await.err()
                } else {
                    Some("The file is not open.".into())
                };
                self.reply(from, HostMessage::SaveResult { request, error });
            }
            GuestMessage::DocClose { doc } => self.close_doc(from, doc),
            GuestMessage::Knock { .. } | GuestMessage::Leave | GuestMessage::Call { .. } => {}
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
            st.docs
                .get_mut(&doc)
                .expect("checked")
                .apply_update(update)?;
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
                });
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

    fn remove_guest(&self, id: ParticipantId) {
        let mut out = Outbox::default();
        let name = {
            let mut st = self.state();
            let Some(guest) = st.guests.remove(&id) else {
                return;
            };
            let docs: Vec<DocId> = st.docs.keys().copied().collect();
            for doc in docs {
                leave_doc(&mut st, id, doc, &mut out);
            }
            participants_changed(&self.host, &st, &mut out);
            guest.participant.name
        };
        if !self.is_ended() {
            out.events
                .push(HostEvent::Notice(format!("{name} left the session.")));
        }
        self.deliver(out);
    }
}

/// Read a file for sharing: inside the project (also after resolving symlinks), not too
/// large, and text.
fn read_shared(backend: &dyn Backend, rel: &str, limit: usize) -> Result<String, String> {
    let abs = backend.resolve(rel).map_err(|e| e.to_string())?;
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
            .map(|m| m.len() as usize > limit)
            .unwrap_or(false)
        {
            return Err(too_large(&real));
        }
    }
    let text = backend.read_file(rel).map_err(|e| e.to_string())?;
    if text.len() > limit {
        return Err(too_large(&abs));
    }
    Ok(text)
}

fn too_large(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("{name} is larger than the shared file size limit and is not shared.")
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

fn open_in(st: &mut State, who: ParticipantId, path: &str, read_only: bool) -> Option<DocOpened> {
    let id = *st.by_path.get(path)?;
    let d = st.docs.get_mut(&id)?;
    d.open_by.insert(who);
    Some(DocOpened {
        doc: id,
        path: d.path.clone(),
        update: d.full_update(),
        saved: d.save_state(),
        read_only,
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
