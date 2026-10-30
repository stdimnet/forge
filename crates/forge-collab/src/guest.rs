//! A guest's side of a session. The guest keeps no replica of its own: it forwards document
//! updates between its editor and the host, which holds the authoritative copy and checks
//! every request.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde_bytes::ByteBuf;
use tokio::sync::{mpsc, oneshot, Mutex as AsyncMutex, Notify};

use crate::host::DocOpened;
use crate::link::SessionLink;
use crate::permissions::Permissions;
use crate::presence::{Participant, ParticipantId, Presence};
use crate::protocol::{
    DocId, GuestMessage, HostMessage, SaveState, SharedFile, MAX_GUEST_FRAME, MAX_HOST_FRAME,
    PROTOCOL_VERSION,
};
use crate::reconnect::{ConnectionState, ReconnectPolicy, GAVE_UP, RECONNECTING};
use crate::relay::{ConnectionPath, RelayStatus};
use crate::transport::{read_msg, write_msg, BoxRead, BoxWrite, PeerKey, Transport};
use crate::verify::{join_proof, verification_code};

// Project calls of the guest's backend (`guest_backend::CollabBackend`).
#[path = "guest_backend/session_calls.rs"]
mod session_calls;
pub use session_calls::DiagnosticsSink;
use session_calls::GuestCalls;

// The host's shared terminals and runs.
#[path = "share_guest.rs"]
mod share_guest;
use share_guest::GuestShare;
pub use share_guest::RunEventSink;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// How long the host may take to let a guest in (the host approval waits for a person).
const ADMISSION_TIMEOUT: Duration = Duration::from_secs(120);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const QUEUE: usize = 4096;
/// One reconnect attempt: connect and get the answer to `Resume`.
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(15);
const LOST: &str = "The connection to the host was lost.";

/// An open connection to the host.
struct Live {
    send: BoxWrite,
    recv: BoxRead,
    keepalive: Box<dyn std::any::Any + Send>,
}

enum Attempt {
    /// Try again later.
    Failed(String),
    /// The host does not take us back.
    Refused(String),
}

/// The answer to [`GuestSession::sync_doc`].
#[derive(Debug, Clone, PartialEq)]
pub struct DocSynced {
    pub doc: DocId,
    pub path: String,
    /// What this replica was missing (one Yjs update).
    pub update: Vec<u8>,
    /// The host's state vector: send the host what it is missing.
    pub state_vector: Vec<u8>,
    pub saved: SaveState,
    pub read_only: bool,
    /// The host is resolving a File Cache Conflict for it.
    pub conflict: bool,
}

/// What the guest's UI needs to know.
#[derive(Debug, Clone, PartialEq)]
pub enum GuestEvent {
    /// Participants, shared files or permissions changed.
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
    /// The file changed on the host's disk while the document has unsaved changes, and the
    /// host is choosing what to keep (`true`), or it chose (`false`). Saving is refused
    /// meanwhile.
    DocConflict {
        doc: DocId,
        conflict: bool,
    },
    /// Where another participant is (only files this guest may see are named).
    Presence {
        participant: ParticipantId,
        presence: Presence,
    },
    /// Make Everyone Follow You was turned on or off by the host.
    FollowHost(bool),
    /// The connection was restored after it dropped: the UI resynchronizes its documents
    /// ([`GuestSession::sync_doc`]).
    Resumed,
    /// The host refused a request.
    Denied(String),
    /// The session is over for this guest.
    Ended(String),
}

pub type GuestSink = Arc<dyn Fn(GuestEvent) + Send + Sync>;

struct State {
    me: ParticipantId,
    participants: Vec<Participant>,
    files: Vec<SharedFile>,
    notice: Option<String>,
    presence: BTreeMap<ParticipantId, Presence>,
    follow_host: bool,
    opens: HashMap<u64, oneshot::Sender<Result<DocOpened, String>>>,
    saves: HashMap<u64, oneshot::Sender<Result<(), String>>>,
    syncs: HashMap<u64, oneshot::Sender<Result<DocSynced, String>>>,
    ended: bool,
    connection: ConnectionState,
    /// The token to take our place back after a dropped connection.
    resume: Vec<u8>,
    /// The presence sent last, sent again after a reconnect.
    last_presence: Option<Presence>,
}

pub struct GuestSession {
    transport: Arc<dyn Transport>,
    tx: mpsc::Sender<GuestMessage>,
    /// The other end of `tx`, written to whichever connection is current.
    queue: Arc<AsyncMutex<mpsc::Receiver<GuestMessage>>>,
    state: Mutex<State>,
    next_request: AtomicU64,
    host_label: String,
    host_key: PeerKey,
    code: String,
    calls: GuestCalls,
    share: GuestShare,
    link: SessionLink,
    policy: ReconnectPolicy,
    /// Wakes the reconnect loop when the guest leaves.
    wake: Notify,
}

impl GuestSession {
    /// Connect to the host of `link` and knock. Returns once the host let us in. A dropped
    /// connection is restored with the default [`ReconnectPolicy`].
    pub async fn join(
        transport: Arc<dyn Transport>,
        link: &SessionLink,
        name: &str,
        sink: GuestSink,
    ) -> Result<Arc<Self>, String> {
        Self::join_with(transport, link, name, sink, ReconnectPolicy::default()).await
    }

    /// [`Self::join`] with its own reconnect policy (tests).
    pub async fn join_with(
        transport: Arc<dyn Transport>,
        link: &SessionLink,
        name: &str,
        sink: GuestSink,
        policy: ReconnectPolicy,
    ) -> Result<Arc<Self>, String> {
        let conn = tokio::time::timeout(
            CONNECT_TIMEOUT,
            transport.connect(link.host_id, link.addrs.clone()),
        )
        .await
        .map_err(|_| {
            "The host did not answer. Check that the session is running and reachable.".to_string()
        })?
        .map_err(|e| format!("Cannot connect to the host: {e}"))?;
        let me = transport.local_key();
        let proof = join_proof(&link.join_secret, &link.host_id, &me, &link.session_id);
        let mut send = conn.send;
        let mut recv = conn.recv;
        let keepalive = conn.keepalive;
        let knock = GuestMessage::Knock {
            protocol: PROTOCOL_VERSION,
            session: ByteBuf::from(link.session_id.to_vec()),
            name: name.to_string(),
            proof: ByteBuf::from(proof.to_vec()),
        };
        write_msg(&mut send, &knock, MAX_GUEST_FRAME)
            .await
            .map_err(|e| format!("Cannot connect to the host: {e}"))?;
        let first = tokio::time::timeout(
            ADMISSION_TIMEOUT,
            read_msg::<_, HostMessage>(&mut recv, MAX_HOST_FRAME),
        )
        .await
        .map_err(|_| "The host did not answer.".to_string())?;
        let (you, participants, files, notice, resume) = match first {
            Ok(Some(HostMessage::Welcome { you, participants, files, notice, resume, .. })) => (you, participants, files, notice, resume),
            Ok(Some(HostMessage::Refused { reason })) => return Err(reason),
            // Wrong proofs are not answered: the link is not (or no longer) valid.
            _ => return Err("The host did not accept the connection. The session may have ended or the link is outdated.".into()),
        };

        let (tx, rx) = mpsc::channel::<GuestMessage>(QUEUE);
        let session = Arc::new(Self {
            transport,
            tx,
            queue: Arc::new(AsyncMutex::new(rx)),
            state: Mutex::new(State {
                me: you,
                participants,
                files,
                notice,
                presence: BTreeMap::new(),
                follow_host: false,
                opens: HashMap::new(),
                saves: HashMap::new(),
                syncs: HashMap::new(),
                ended: false,
                connection: ConnectionState::Connected,
                resume: resume.into_vec(),
                last_presence: None,
            }),
            next_request: AtomicU64::new(1),
            host_label: link.summary().host_label,
            host_key: link.host_id,
            code: verification_code(&link.host_id, &me, &link.session_id),
            calls: GuestCalls::new(),
            share: GuestShare::default(),
            link: link.clone(),
            policy,
            wake: Notify::new(),
        });
        tokio::spawn(session.clone().supervise(
            Live {
                send,
                recv,
                keepalive,
            },
            sink,
        ));
        Ok(session)
    }

    /// Serve the connection; when it drops, reconnect and serve the new one, until the host
    /// ends the session, the guest leaves or reconnecting gives up.
    async fn supervise(self: Arc<Self>, mut live: Live, sink: GuestSink) {
        loop {
            if let Some(reason) = self.serve_connection(live, &sink).await {
                return self.finish(Some(reason), &sink);
            }
            if self.is_ended() {
                return self.finish(None, &sink);
            }
            self.connection_dropped(&sink);
            match self.reconnect(&sink).await {
                Ok(next) => {
                    live = next;
                    sink(GuestEvent::Changed);
                    sink(GuestEvent::Resumed);
                }
                Err(reason) => return self.finish(reason, &sink),
            }
        }
    }

    /// Write the queue to the connection and dispatch what the host sends. `Some(reason)`
    /// when the host ended the session, `None` when the connection dropped (or we left).
    async fn serve_connection(&self, live: Live, sink: &GuestSink) -> Option<String> {
        let Live {
            mut send,
            mut recv,
            keepalive,
        } = live;
        let queue = self.queue.clone();
        let writer = tokio::spawn(async move {
            let _keepalive = keepalive;
            let mut rx = queue.lock().await;
            while let Some(msg) = rx.recv().await {
                let last = matches!(msg, GuestMessage::Leave);
                if write_msg(&mut send, &msg, MAX_GUEST_FRAME).await.is_err() || last {
                    break;
                }
            }
            drop(rx);
            use tokio::io::AsyncWriteExt;
            let _ = send.shutdown().await;
        });
        let ended = loop {
            match read_msg::<_, HostMessage>(&mut recv, MAX_HOST_FRAME).await {
                Ok(Some(HostMessage::Ended { reason })) => break Some(reason),
                Ok(Some(msg)) => self.dispatch(msg, sink),
                Ok(None) | Err(_) => break None,
            }
        };
        if ended.is_some() || self.is_ended() {
            // Let a last Leave go out.
            let _ = tokio::time::timeout(Duration::from_millis(200), &mut { writer }).await;
        } else {
            writer.abort();
        }
        ended
    }

    /// The connection dropped: requests in flight fail, nothing is sent until it is back.
    /// Terminals, runs, presence and the documents in the UI stay.
    fn connection_dropped(&self, sink: &GuestSink) {
        tracing::info!("Code With Me: the connection to the host dropped, reconnecting");
        let (opens, saves, syncs) = {
            let mut st = self.state();
            st.connection = ConnectionState::Reconnecting {
                attempt: 1,
                next_retry_secs: self.policy.backoff.first().map_or(0, |d| d.as_secs()),
                error: None,
            };
            (
                std::mem::take(&mut st.opens),
                std::mem::take(&mut st.saves),
                std::mem::take(&mut st.syncs),
            )
        };
        for tx in opens.into_values() {
            let _ = tx.send(Err(LOST.into()));
        }
        for tx in saves.into_values() {
            let _ = tx.send(Err(LOST.into()));
        }
        for tx in syncs.into_values() {
            let _ = tx.send(Err(LOST.into()));
        }
        self.calls.fail_all();
        sink(GuestEvent::Changed);
    }

    /// Try again with backoff until the host takes us back, refuses, or the policy gives
    /// up. `Err(None)`: the guest left meanwhile.
    async fn reconnect(&self, sink: &GuestSink) -> Result<Live, Option<String>> {
        let started = tokio::time::Instant::now();
        let mut attempt = 0u32;
        let mut error = None;
        loop {
            let Some(wait) = self.policy.delay(attempt, started.elapsed()) else {
                return Err(Some(GAVE_UP.into()));
            };
            attempt += 1;
            self.state().connection = ConnectionState::Reconnecting {
                attempt,
                next_retry_secs: wait.as_secs(),
                error: error.clone(),
            };
            sink(GuestEvent::Changed);
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = self.wake.notified() => {}
            }
            if self.is_ended() {
                return Err(None);
            }
            match tokio::time::timeout(ATTEMPT_TIMEOUT, self.try_resume()).await {
                Ok(Ok(live)) => {
                    tracing::info!("Code With Me: reconnected to the host");
                    return Ok(live);
                }
                Ok(Err(Attempt::Refused(reason))) => return Err(Some(reason)),
                Ok(Err(Attempt::Failed(e))) => error = Some(e),
                Err(_) => error = Some("The host did not answer.".into()),
            }
        }
    }

    /// One attempt: connect and ask for our place back.
    async fn try_resume(&self) -> Result<Live, Attempt> {
        let conn = self
            .transport
            .connect(self.link.host_id, self.link.addrs.clone())
            .await
            .map_err(|e| Attempt::Failed(format!("Cannot connect to the host: {e}")))?;
        let (me, token) = {
            let st = self.state();
            (st.me, st.resume.clone())
        };
        let mut send = conn.send;
        let mut recv = conn.recv;
        let resume = GuestMessage::Resume {
            protocol: PROTOCOL_VERSION,
            session: ByteBuf::from(self.link.session_id.to_vec()),
            participant: me,
            token: ByteBuf::from(token),
        };
        write_msg(&mut send, &resume, MAX_GUEST_FRAME)
            .await
            .map_err(|e| Attempt::Failed(format!("Cannot connect to the host: {e}")))?;
        match read_msg::<_, HostMessage>(&mut recv, MAX_HOST_FRAME).await {
            Ok(Some(HostMessage::Welcome {
                participants,
                files,
                notice,
                resume,
                resumed: true,
                ..
            })) => {
                // What was queued while the connection was down is stale: documents
                // resynchronize, presence is sent again below.
                {
                    let mut rx = self.queue.lock().await;
                    while rx.try_recv().is_ok() {}
                }
                let presence = {
                    let mut st = self.state();
                    st.participants = participants;
                    st.files = files;
                    st.notice = notice;
                    if !resume.is_empty() {
                        st.resume = resume.into_vec();
                    }
                    st.connection = ConnectionState::Connected;
                    st.last_presence.clone()
                };
                if let Some(presence) = presence {
                    self.send(GuestMessage::Presence { presence });
                }
                Ok(Live {
                    send,
                    recv,
                    keepalive: conn.keepalive,
                })
            }
            Ok(Some(HostMessage::Refused { reason })) => Err(Attempt::Refused(reason)),
            Ok(Some(_)) => Err(Attempt::Failed("Unexpected answer from the host.".into())),
            Ok(None) | Err(_) => Err(Attempt::Failed("The host did not answer.".into())),
        }
    }

    /// The session is over for this guest; `reason` is shown unless the guest left.
    fn finish(&self, reason: Option<String>, sink: &GuestSink) {
        let was_ended = std::mem::replace(&mut self.state().ended, true);
        {
            let mut st = self.state();
            st.opens.clear();
            st.saves.clear();
            st.syncs.clear();
        }
        self.calls.fail_all();
        self.end_shares();
        if let (false, Some(reason)) = (was_ended, reason) {
            sink(GuestEvent::Ended(reason));
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("guest state lock poisoned")
    }

    fn dispatch(&self, msg: HostMessage, sink: &GuestSink) {
        if self.dispatch_call_message(&msg) || self.dispatch_share_message(&msg, sink) {
            return;
        }
        match msg {
            HostMessage::Participants { participants } => {
                let mut st = self.state();
                st.presence
                    .retain(|id, _| participants.iter().any(|p| p.id == *id));
                st.participants = participants;
                drop(st);
                sink(GuestEvent::Changed);
            }
            HostMessage::Presence {
                participant,
                presence,
            } => {
                self.state().presence.insert(participant, presence.clone());
                sink(GuestEvent::Presence {
                    participant,
                    presence,
                });
            }
            HostMessage::FollowHost { on } => {
                self.state().follow_host = on;
                sink(GuestEvent::FollowHost(on));
                sink(GuestEvent::Changed);
            }
            HostMessage::SharedFiles { files } => {
                self.state().files = files;
                sink(GuestEvent::Changed);
            }
            HostMessage::Permissions { permissions } => {
                let mut st = self.state();
                let me = st.me;
                if let Some(p) = st.participants.iter_mut().find(|p| p.id == me) {
                    p.permissions = permissions;
                }
                drop(st);
                sink(GuestEvent::Changed);
            }
            HostMessage::DocOpened {
                request,
                doc,
                path,
                update,
                saved,
                read_only,
                conflict,
            } => {
                if let Some(tx) = self.state().opens.remove(&request) {
                    let _ = tx.send(Ok(DocOpened {
                        doc,
                        path,
                        update: update.into_vec(),
                        saved,
                        read_only,
                        conflict,
                    }));
                }
            }
            HostMessage::DocError { request, error } => {
                let mut st = self.state();
                if let Some(tx) = st.opens.remove(&request) {
                    let _ = tx.send(Err(error));
                } else if let Some(tx) = st.syncs.remove(&request) {
                    let _ = tx.send(Err(error));
                }
            }
            HostMessage::DocSynced {
                request,
                doc,
                path,
                update,
                state_vector,
                saved,
                read_only,
                conflict,
            } => {
                if let Some(tx) = self.state().syncs.remove(&request) {
                    let _ = tx.send(Ok(DocSynced {
                        doc,
                        path,
                        update: update.into_vec(),
                        state_vector: state_vector.into_vec(),
                        saved,
                        read_only,
                        conflict,
                    }));
                }
            }
            HostMessage::SaveResult { request, error } => {
                if let Some(tx) = self.state().saves.remove(&request) {
                    let _ = tx.send(error.map_or(Ok(()), Err));
                }
            }
            HostMessage::DocUpdate { doc, update } => sink(GuestEvent::DocUpdate {
                doc,
                update: update.into_vec(),
            }),
            HostMessage::Awareness { doc, update } => sink(GuestEvent::Awareness {
                doc,
                update: update.into_vec(),
            }),
            HostMessage::DocSaved { doc, saved } => sink(GuestEvent::DocSaved { doc, saved }),
            HostMessage::DocRenamed { doc, path } => sink(GuestEvent::DocRenamed { doc, path }),
            HostMessage::DocClosed { doc, reason } => sink(GuestEvent::DocClosed { doc, reason }),
            HostMessage::DocConflict { doc, conflict } => {
                sink(GuestEvent::DocConflict { doc, conflict })
            }
            HostMessage::Denied { reason } => sink(GuestEvent::Denied(reason)),
            HostMessage::Welcome { .. }
            | HostMessage::Refused { .. }
            | HostMessage::Ended { .. }
            | HostMessage::CallResult { .. }
            | HostMessage::FsChanged { .. }
            | HostMessage::Diagnostics { .. }
            | HostMessage::Terminals { .. }
            | HostMessage::TerminalOutput { .. }
            | HostMessage::Runs { .. }
            | HostMessage::RunEvents { .. } => {}
        }
    }

    fn send(&self, msg: GuestMessage) {
        // While the connection is down nothing is queued: documents resynchronize and
        // presence is sent again when it is back; terminal and console input is lost.
        if !self.state().connection.is_connected() {
            return;
        }
        if self.tx.try_send(msg).is_err() {
            tracing::warn!("Code With Me: the connection to the host is congested or closed");
        }
    }

    /// Messages waiting to go to the host (bounded: when the queue is full, sending fails).
    pub fn backlog(&self) -> usize {
        self.tx.max_capacity() - self.tx.capacity()
    }

    pub fn me(&self) -> ParticipantId {
        self.state().me
    }

    pub fn participants(&self) -> Vec<Participant> {
        self.state().participants.clone()
    }

    pub fn files(&self) -> Vec<SharedFile> {
        self.state().files.clone()
    }

    /// Where the other participants are, as far as this guest may see.
    pub fn presence(&self) -> Vec<(ParticipantId, Presence)> {
        self.state()
            .presence
            .iter()
            .map(|(id, p)| (*id, p.clone()))
            .collect()
    }

    /// Whether the host asked everyone to follow it.
    pub fn follow_host(&self) -> bool {
        self.state().follow_host
    }

    /// Tell the others where this guest is.
    pub fn set_presence(&self, presence: Presence) {
        self.state().last_presence = Some(presence.clone());
        self.send(GuestMessage::Presence { presence });
    }

    /// Whether the connection to the host is up or being restored.
    pub fn connection(&self) -> ConnectionState {
        self.state().connection.clone()
    }

    /// Fails while the session is over or the connection is being restored.
    fn check_usable(&self) -> Result<(), String> {
        let st = self.state();
        if st.ended {
            Err("The session has ended.".into())
        } else if !st.connection.is_connected() {
            Err(RECONNECTING.into())
        } else {
            Ok(())
        }
    }

    /// Bring a document up to date after a reconnect: `state_vector` is the UI replica's
    /// (Yjs v1). The answer has what the replica is missing and the host's state vector, to
    /// send the host the edits it is missing with [`Self::update`].
    pub async fn sync_doc(&self, doc: DocId, state_vector: Vec<u8>) -> Result<DocSynced, String> {
        let request = self.next_request.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.check_usable()?;
        self.state().syncs.insert(request, tx);
        self.send(GuestMessage::DocSync {
            request,
            doc,
            state_vector: ByteBuf::from(state_vector),
        });
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            _ => Err("The host did not answer.".into()),
        }
    }

    pub fn notice(&self) -> Option<String> {
        self.state().notice.clone()
    }

    pub fn is_ended(&self) -> bool {
        self.state().ended
    }

    /// This guest's permissions.
    pub fn permissions(&self) -> Permissions {
        let st = self.state();
        st.participants
            .iter()
            .find(|p| p.id == st.me)
            .map(|p| p.permissions)
            .unwrap_or_default()
    }

    /// `name (machine)` of the host.
    pub fn host_label(&self) -> &str {
        &self.host_label
    }

    /// How the connection to the host travels: direct, or through the relay.
    pub fn connection_path(&self) -> Option<ConnectionPath> {
        self.transport.path(&self.host_key)
    }

    /// The state of the connection to the relay named in the link, if there is one.
    pub fn relay_status(&self) -> Option<RelayStatus> {
        self.transport.relay_status()
    }

    /// The verification code (the host sees the same one).
    pub fn verification_code(&self) -> &str {
        &self.code
    }

    pub async fn open_doc(&self, path: &str) -> Result<DocOpened, String> {
        let request = self.next_request.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.check_usable()?;
        self.state().opens.insert(request, tx);
        self.send(GuestMessage::DocOpen {
            request,
            path: path.to_string(),
        });
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            _ => Err("The host did not answer.".into()),
        }
    }

    pub fn update(&self, doc: DocId, update: Vec<u8>) {
        self.send(GuestMessage::DocUpdate {
            doc,
            update: ByteBuf::from(update),
        });
    }

    pub fn awareness(&self, doc: DocId, update: Vec<u8>) {
        self.send(GuestMessage::Awareness {
            doc,
            update: ByteBuf::from(update),
        });
    }

    pub fn close_doc(&self, doc: DocId) {
        self.send(GuestMessage::DocClose { doc });
    }

    pub async fn save(&self, doc: DocId) -> Result<(), String> {
        let request = self.next_request.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.check_usable()?;
        self.state().saves.insert(request, tx);
        self.send(GuestMessage::DocSave { request, doc });
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            _ => Err("The host did not answer.".into()),
        }
    }

    pub async fn leave(&self) {
        self.state().ended = true;
        self.wake.notify_one();
        self.end_shares();
        self.send(GuestMessage::Leave);
        tokio::time::sleep(Duration::from_millis(100)).await;
        self.transport.close().await;
    }
}
