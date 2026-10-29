//! A guest's side of a session. The guest keeps no replica of its own: it forwards document
//! updates between its editor and the host, which holds the authoritative copy and checks
//! every request.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde_bytes::ByteBuf;
use tokio::sync::{mpsc, oneshot};

use crate::host::DocOpened;
use crate::link::SessionLink;
use crate::permissions::Permissions;
use crate::presence::{Participant, ParticipantId, Presence};
use crate::protocol::{
    DocId, GuestMessage, HostMessage, SaveState, SharedFile, MAX_GUEST_FRAME, MAX_HOST_FRAME,
    PROTOCOL_VERSION,
};
use crate::relay::{ConnectionPath, RelayStatus};
use crate::transport::{read_msg, write_msg, PeerKey, Transport};
use crate::verify::{join_proof, verification_code};

// Project calls of the guest's backend (`guest_backend::CollabBackend`).
#[path = "guest_backend/session_calls.rs"]
mod session_calls;
pub use session_calls::DiagnosticsSink;
use session_calls::GuestCalls;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// How long the host may take to let a guest in (the host approval waits for a person).
const ADMISSION_TIMEOUT: Duration = Duration::from_secs(120);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const QUEUE: usize = 4096;

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
    /// Where another participant is (only files this guest may see are named).
    Presence {
        participant: ParticipantId,
        presence: Presence,
    },
    /// Make Everyone Follow You was turned on or off by the host.
    FollowHost(bool),
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
    ended: bool,
}

pub struct GuestSession {
    transport: Arc<dyn Transport>,
    tx: mpsc::Sender<GuestMessage>,
    state: Mutex<State>,
    next_request: AtomicU64,
    host_label: String,
    host_key: PeerKey,
    code: String,
    calls: GuestCalls,
}

impl GuestSession {
    /// Connect to the host of `link` and knock. Returns once the host let us in.
    pub async fn join(
        transport: Arc<dyn Transport>,
        link: &SessionLink,
        name: &str,
        sink: GuestSink,
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
        let (you, participants, files, notice) = match first {
            Ok(Some(HostMessage::Welcome { you, participants, files, notice, .. })) => (you, participants, files, notice),
            Ok(Some(HostMessage::Refused { reason })) => return Err(reason),
            // Wrong proofs are not answered: the link is not (or no longer) valid.
            _ => return Err("The host did not accept the connection. The session may have ended or the link is outdated.".into()),
        };

        let (tx, mut rx) = mpsc::channel::<GuestMessage>(QUEUE);
        let session = Arc::new(Self {
            transport,
            tx,
            state: Mutex::new(State {
                me: you,
                participants,
                files,
                notice,
                presence: BTreeMap::new(),
                follow_host: false,
                opens: HashMap::new(),
                saves: HashMap::new(),
                ended: false,
            }),
            next_request: AtomicU64::new(1),
            host_label: link.summary().host_label,
            host_key: link.host_id,
            code: verification_code(&link.host_id, &me, &link.session_id),
            calls: GuestCalls::new(),
        });

        tokio::spawn(async move {
            let _keepalive = keepalive;
            while let Some(msg) = rx.recv().await {
                let last = matches!(msg, GuestMessage::Leave);
                if write_msg(&mut send, &msg, MAX_GUEST_FRAME).await.is_err() || last {
                    break;
                }
            }
            use tokio::io::AsyncWriteExt;
            let _ = send.shutdown().await;
        });

        let reader = session.clone();
        tokio::spawn(async move {
            let reason = loop {
                match read_msg::<_, HostMessage>(&mut recv, MAX_HOST_FRAME).await {
                    Ok(Some(HostMessage::Ended { reason })) => break reason,
                    Ok(Some(msg)) => reader.dispatch(msg, &sink),
                    Ok(None) | Err(_) => break "The connection to the host was lost.".to_string(),
                }
            };
            let was_ended = std::mem::replace(&mut reader.state().ended, true);
            reader.state().opens.clear();
            reader.state().saves.clear();
            reader.calls.fail_all();
            if !was_ended {
                sink(GuestEvent::Ended(reason));
            }
        });
        Ok(session)
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("guest state lock poisoned")
    }

    fn dispatch(&self, msg: HostMessage, sink: &GuestSink) {
        if self.dispatch_call_message(&msg) {
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
            } => {
                if let Some(tx) = self.state().opens.remove(&request) {
                    let _ = tx.send(Ok(DocOpened {
                        doc,
                        path,
                        update: update.into_vec(),
                        saved,
                        read_only,
                    }));
                }
            }
            HostMessage::DocError { request, error } => {
                if let Some(tx) = self.state().opens.remove(&request) {
                    let _ = tx.send(Err(error));
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
            HostMessage::Denied { reason } => sink(GuestEvent::Denied(reason)),
            HostMessage::Welcome { .. }
            | HostMessage::Refused { .. }
            | HostMessage::Ended { .. }
            | HostMessage::CallResult { .. }
            | HostMessage::FsChanged { .. }
            | HostMessage::Diagnostics { .. } => {}
        }
    }

    fn send(&self, msg: GuestMessage) {
        if self.tx.try_send(msg).is_err() {
            tracing::warn!("Code With Me: the connection to the host is congested or closed");
        }
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
        self.send(GuestMessage::Presence { presence });
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
        {
            let mut st = self.state();
            if st.ended {
                return Err("The session has ended.".into());
            }
            st.opens.insert(request, tx);
        }
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
        self.state().saves.insert(request, tx);
        self.send(GuestMessage::DocSave { request, doc });
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(result)) => result,
            _ => Err("The host did not answer.".into()),
        }
    }

    pub async fn leave(&self) {
        self.state().ended = true;
        self.send(GuestMessage::Leave);
        tokio::time::sleep(Duration::from_millis(100)).await;
        self.transport.close().await;
    }
}
