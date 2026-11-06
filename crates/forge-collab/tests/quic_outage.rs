//! Reconnecting after a real network outage. Host and guest are real iroh/QUIC endpoints
//! (`NetTransport`, with Forge's keep-alive and idle timeout) and the session code on top;
//! only the medium between them is simulated: an in-memory datagram network (iroh's test
//! transport) behind a "cable" that drops every datagram while it is unplugged. QUIC then
//! times the connection out itself, unlike the simulated drops in `simulation.rs`, which
//! close the stream at once.
//!
//! Why not a UDP proxy on loopback: iroh's hole punching tells each end the other's own
//! socket address, and on one machine every such address is reachable, so the connection
//! (or the guest's next one) goes around any cut between two UDP sockets.

use std::io;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use forge_backend::{Backend, LocalBackend};
use forge_collab::excludes::Excludes;
use forge_collab::guest::{GuestEvent, GuestSession};
use forge_collab::host::{DevAutoAccept, HostConfig, HostEvent, HostSession};
use forge_collab::permissions::{Permissions, TerminalAccess};
use forge_collab::protocol::DocId;
use forge_collab::reconnect::ReconnectPolicy;
use forge_collab::transport::net::{EndpointHook, ListenOptions, NetTransport, IDLE_TIMEOUT};
use forge_collab::transport::relay::NetConfig;
use forge_process::PtyEvent;
use iroh::endpoint::transports::{
    CustomEndpoint, CustomSender, CustomTransport, RecvInfo, Transmit,
};
use iroh::test_utils::test_transport::{TestNetwork, TestTransport};
use iroh::SecretKey;
use iroh_base::CustomAddr;
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{
    Doc, GetString, OffsetKind, Options, ReadTxn, StateVector, Text, TextRef, Transact, Update,
};

/// The network between the host and the guest. While it is unplugged every datagram in
/// both directions is dropped, as when a Wi-Fi link goes away.
#[derive(Clone, Default)]
struct Cable {
    net: TestNetwork,
    flags: Arc<Flags>,
}

#[derive(Debug, Default)]
struct Flags {
    down: AtomicBool,
    dropped: AtomicU64,
}

impl Cable {
    /// Puts an endpoint on this network and nowhere else (no UDP sockets).
    fn hook(&self) -> EndpointHook {
        let key = SecretKey::generate();
        let plug = Arc::new(Plug {
            inner: self.net.create_transport(key.public()).unwrap(),
            flags: self.flags.clone(),
        });
        let lookup = self.net.address_lookup();
        Box::new(move |b| {
            b.secret_key(key)
                .clear_ip_transports()
                .add_custom_transport(plug)
                .address_lookup(lookup)
        })
    }

    fn unplug(&self) {
        self.flags.down.store(true, Ordering::SeqCst);
    }

    fn plug_in(&self) {
        self.flags.down.store(false, Ordering::SeqCst);
    }

    fn dropped(&self) -> u64 {
        self.flags.dropped.load(Ordering::Relaxed)
    }
}

/// One endpoint's connection to the cable: iroh's test transport, with sending switched off
/// while the cable is unplugged (both ends are plugged into the same cable, so nothing gets
/// through either way).
#[derive(Debug)]
struct Plug {
    inner: Arc<TestTransport>,
    flags: Arc<Flags>,
}

impl CustomTransport for Plug {
    fn bind(&self) -> io::Result<Box<dyn CustomEndpoint>> {
        Ok(Box::new(PlugEndpoint {
            inner: self.inner.bind()?,
            flags: self.flags.clone(),
        }))
    }
}

#[derive(Debug)]
struct PlugEndpoint {
    inner: Box<dyn CustomEndpoint>,
    flags: Arc<Flags>,
}

impl CustomEndpoint for PlugEndpoint {
    fn watch_local_addrs(&self) -> n0_watcher::Direct<Vec<CustomAddr>> {
        self.inner.watch_local_addrs()
    }

    fn create_sender(&self) -> Arc<dyn CustomSender> {
        Arc::new(PlugSender {
            inner: self.inner.create_sender(),
            flags: self.flags.clone(),
        })
    }

    fn poll_recv(
        &mut self,
        cx: &mut Context,
        bufs: &mut [io::IoSliceMut<'_>],
        metas: &mut [noq_udp::RecvMeta],
        recv_infos: &mut [RecvInfo],
    ) -> Poll<io::Result<usize>> {
        self.inner.poll_recv(cx, bufs, metas, recv_infos)
    }

    fn max_transmit_segments(&self) -> NonZeroUsize {
        self.inner.max_transmit_segments()
    }
}

#[derive(Debug)]
struct PlugSender {
    inner: Arc<dyn CustomSender>,
    flags: Arc<Flags>,
}

impl CustomSender for PlugSender {
    fn is_valid_send_addr(&self, addr: &CustomAddr) -> bool {
        self.inner.is_valid_send_addr(addr)
    }

    fn poll_send(
        &self,
        cx: &mut Context,
        dst: &CustomAddr,
        src: Option<&CustomAddr>,
        transmit: &Transmit<'_>,
    ) -> Poll<io::Result<()>> {
        if self.flags.down.load(Ordering::SeqCst) {
            // Lost on the way: the sender cannot tell.
            self.flags.dropped.fetch_add(1, Ordering::Relaxed);
            return Poll::Ready(Ok(()));
        }
        self.inner.poll_send(cx, dst, src, transmit)
    }
}

/// One editor replica (the UI's Yjs document).
struct Replica {
    doc: Doc,
    text: TextRef,
}

impl Replica {
    fn new(state: &[u8]) -> Self {
        let doc = Doc::with_options(Options {
            offset_kind: OffsetKind::Utf16,
            ..Options::default()
        });
        let text = doc.get_or_insert_text("content");
        doc.transact_mut()
            .apply_update(Update::decode_v1(state).unwrap())
            .unwrap();
        Self { doc, text }
    }

    fn apply(&self, update: &[u8]) {
        self.doc
            .transact_mut()
            .apply_update(Update::decode_v1(update).unwrap())
            .unwrap();
    }

    fn insert(&self, at: u32, s: &str) -> Vec<u8> {
        let before = self.doc.transact().state_vector();
        self.text.insert(&mut self.doc.transact_mut(), at, s);
        self.doc.transact().encode_state_as_update_v1(&before)
    }

    fn string(&self) -> String {
        self.text.get_string(&self.doc.transact())
    }
}

/// What reached a participant's window.
#[derive(Default)]
struct Seen {
    replica: Option<Replica>,
    early: Vec<Vec<u8>>,
    resumed: usize,
    ended: Option<String>,
    notices: Vec<String>,
}

impl Seen {
    fn receive(&mut self, update: Vec<u8>) {
        match &self.replica {
            Some(r) => r.apply(&update),
            None => self.early.push(update),
        }
    }

    fn open(&mut self, state: &[u8]) {
        let r = Replica::new(state);
        for u in self.early.drain(..) {
            r.apply(&u);
        }
        self.replica = Some(r);
    }

    fn insert(&self, at: u32, text: &str) -> Vec<u8> {
        self.replica.as_ref().unwrap().insert(at, text)
    }

    fn text(&self) -> String {
        self.replica.as_ref().unwrap().string()
    }
}

type Shared = Arc<Mutex<Seen>>;

struct World {
    _dir: tempfile::TempDir,
    host: Arc<HostSession>,
    host_seen: Shared,
    cable: Cable,
}

impl World {
    async fn new(permissions: Permissions) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Order.php"), "<?php\n").unwrap();
        let backend: Arc<dyn Backend> = Arc::new(LocalBackend::open(dir.path()).unwrap());
        let cable = Cable::default();
        let transport = NetTransport::listen_with_hook(
            ListenOptions::default(),
            NetConfig::default(),
            cable.hook(),
        )
        .await
        .unwrap();
        let host_seen: Shared = Arc::default();
        let sink = host_seen.clone();
        let host = HostSession::start(
            Arc::new(transport),
            backend,
            HostConfig {
                host_name: "Robert".into(),
                machine: None,
                permissions,
                excludes: Excludes::defaults(),
                protected: Excludes::protected_defaults(),
                max_file_size: 1 << 20,
                notice: None,
            },
            Arc::new(DevAutoAccept),
            Arc::new(move |e| {
                let mut s = sink.lock().unwrap();
                match e {
                    HostEvent::DocUpdate { update, .. } => s.receive(update),
                    HostEvent::Notice(n) => s.notices.push(n),
                    _ => {}
                }
            }),
        );
        Self {
            _dir: dir,
            host,
            host_seen,
            cable,
        }
    }

    async fn host_open(&self) -> DocId {
        let o = self.host.open_doc("Order.php", None).await.unwrap();
        self.host_seen.lock().unwrap().open(&o.update);
        o.doc
    }

    fn host_insert(&self, doc: DocId, at: u32, text: &str) {
        let u = self.host_seen.lock().unwrap().insert(at, text);
        self.host.host_update(doc, &u).unwrap();
    }

    fn host_text(&self) -> String {
        self.host_seen.lock().unwrap().text()
    }

    fn reconnecting(&self) -> bool {
        self.host.participants().iter().any(|p| p.reconnecting)
    }

    /// A guest on the cable. The link names no addresses: the test network's lookup finds
    /// the host by its key.
    async fn join(&self) -> Guest {
        let transport = NetTransport::dialer_with_hook(NetConfig::default(), self.cable.hook())
            .await
            .unwrap();
        let seen: Shared = Arc::default();
        let sink = seen.clone();
        let session = GuestSession::join_with(
            Arc::new(transport),
            &self.host.link(),
            "Alex",
            Arc::new(move |e| {
                let mut s = sink.lock().unwrap();
                match e {
                    GuestEvent::DocUpdate { update, .. } => s.receive(update),
                    GuestEvent::Resumed => s.resumed += 1,
                    GuestEvent::Ended(r) => s.ended = Some(r),
                    _ => {}
                }
            }),
            // Retries every 250 ms, so the times measured are the transport's.
            ReconnectPolicy {
                backoff: vec![Duration::from_millis(250)],
                give_up_after: Duration::from_secs(90),
            },
        )
        .await
        .unwrap();
        Guest { session, seen }
    }
}

struct Guest {
    session: Arc<GuestSession>,
    seen: Shared,
}

impl Guest {
    async fn open(&self) -> DocId {
        let o = self.session.open_doc("Order.php").await.unwrap();
        self.seen.lock().unwrap().open(&o.update);
        o.doc
    }

    fn insert(&self, doc: DocId, at: u32, text: &str) {
        let u = self.seen.lock().unwrap().insert(at, text);
        self.session.update(doc, u);
    }

    fn text(&self) -> String {
        self.seen.lock().unwrap().text()
    }

    fn connected(&self) -> bool {
        self.session.connection().is_connected()
    }

    /// What the UI does on `Resumed`: Yjs sync step 1 and 2 both ways.
    async fn resync(&self, doc: DocId) {
        let sv = {
            let s = self.seen.lock().unwrap();
            let sv = s.replica.as_ref().unwrap().doc.transact().state_vector();
            sv.encode_v1()
        };
        let synced = self.session.sync_doc(doc, sv).await.unwrap();
        let missing = {
            let s = self.seen.lock().unwrap();
            let r = s.replica.as_ref().unwrap();
            r.apply(&synced.update);
            let host_sv = StateVector::decode_v1(&synced.state_vector).unwrap();
            let missing = r.doc.transact().encode_state_as_update_v1(&host_sv);
            missing
        };
        self.session.update(doc, missing);
    }
}

/// What reached the guest's terminal tab.
#[derive(Clone, Default)]
struct Tab(Arc<Mutex<String>>);

impl Tab {
    fn sink(&self) -> forge_process::PtySink {
        let t = self.0.clone();
        Arc::new(move |e| {
            if let PtyEvent::Output { data } = e {
                t.lock().unwrap().push_str(&data);
            }
        })
    }

    fn text(&self) -> String {
        self.0.lock().unwrap().clone()
    }
}

/// Wait up to `limit` for `ok`; returns how long it took.
async fn within(limit: Duration, what: &str, mut ok: impl FnMut() -> bool) -> Duration {
    let start = Instant::now();
    while start.elapsed() < limit {
        if ok() {
            return start.elapsed();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("timed out after {limit:?} waiting for {what}");
}

/// iroh's keep-alive interval: an idle connection sends a ping this often.
const KEEP_ALIVE: Duration = Duration::from_secs(5);

/// How long after `cut` the host and the guest noticed that the connection is dead.
///
/// QUIC's idle timer restarts on every packet received, and on the first packet sent after
/// one was received. So a dead connection is noticed `IDLE_TIMEOUT` after the last packet
/// sent into the outage: right after the cut when someone was typing, up to one keep-alive
/// interval later on a quiet connection. Not before: nothing else ends the connection.
async fn noticed(w: &World, alex: &Guest, cut: Instant) -> (Duration, Duration) {
    let latest = IDLE_TIMEOUT + KEEP_ALIVE + Duration::from_secs(3);
    let (mut host, mut guest) = (None, None);
    while host.is_none() || guest.is_none() {
        assert!(
            cut.elapsed() < latest,
            "outage not noticed within {latest:?}: host {host:?}, guest {guest:?}"
        );
        if host.is_none() && w.reconnecting() {
            host = Some(cut.elapsed());
        }
        if guest.is_none() && !alex.connected() {
            guest = Some(cut.elapsed());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (host, guest) = (host.unwrap(), guest.unwrap());
    for (who, at) in [("host", host), ("guest", guest)] {
        assert!(
            at + Duration::from_millis(500) >= IDLE_TIMEOUT,
            "the {who} noticed after {at:?}, before the idle timeout {IDLE_TIMEOUT:?}"
        );
    }
    (host, guest)
}

/// An outage shorter than the idle timeout is invisible: QUIC retransmits, nobody
/// reconnects, and what both sides typed meanwhile arrives.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_short_outage_passes_without_a_reconnect() {
    let w = World::new(Permissions::EDIT_FILES).await;
    let doc = w.host_open().await;
    let alex = w.join().await;
    alex.open().await;
    w.cable.unplug();
    alex.insert(doc, 0, "// offline\n");
    w.host_insert(doc, 5, " // host");
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert!(alex.connected());
    assert!(!w.reconnecting());
    w.cable.plug_in();
    let took = within(Duration::from_secs(10), "both edits everywhere", || {
        let t = w.host_text();
        t.contains("// offline") && t.contains("// host") && alex.text() == t
    })
    .await;
    eprintln!(
        "4 s outage ({} datagrams dropped): the edits arrived {took:?} after the network came back",
        w.cable.dropped()
    );
    assert!(alex.connected());
    let (resumed, ended) = {
        let seen = alex.seen.lock().unwrap();
        (seen.resumed, seen.ended.clone())
    };
    assert_eq!(resumed, 0);
    assert_eq!(ended, None);
    let notices = w.host_seen.lock().unwrap().notices.clone();
    assert!(
        !notices.iter().any(|n| n.contains("connection")),
        "{notices:?}"
    );
    w.host.end("The host ended the session.").await;
}

/// A longer outage: both ends notice after the idle timeout, the guest dials again once the
/// network is back and takes its place back (same participant, color and permissions), the
/// edits both sides made meanwhile merge, and the terminal output it missed arrives once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_guest_comes_back_after_a_real_outage_and_both_sides_edits_merge() {
    let permissions = Permissions {
        terminal: TerminalAccess::ReadOnly,
        ..Permissions::EDIT_FILES
    };
    let w = World::new(permissions).await;
    w.host
        .share_terminal("term-1", "Local", 80, 24, "$ make\r\n", true);
    let doc = w.host_open().await;
    let alex = w.join().await;
    alex.open().await;
    let g = alex.session.clone();
    within(Duration::from_secs(5), "the shared terminal", || {
        !g.terminals().is_empty()
    })
    .await;
    let tab = Tab::default();
    let (shown, _, _) = g.attach_terminal(g.terminals()[0].id, tab.sink()).unwrap();
    assert_eq!(shown, "$ make\r\n");
    let me = g.me();
    let place = w
        .host
        .participants()
        .into_iter()
        .find(|p| p.id == me)
        .unwrap();
    assert_eq!(place.permissions, permissions);

    w.cable.unplug();
    let cut = Instant::now();
    alex.insert(doc, 0, "// offline\n");
    w.host_insert(doc, 5, " // host");
    w.host.terminal_output("term-1", "built while away\r\n");
    let (host_noticed, guest_noticed) = noticed(&w, &alex, cut).await;
    eprintln!(
        "outage noticed: host after {host_noticed:?}, guest after {guest_noticed:?} \
         (idle timeout {IDLE_TIMEOUT:?}, {} datagrams dropped)",
        w.cable.dropped()
    );
    assert_eq!(w.host.participants().len(), 2, "the place is kept");
    // While the network is down the guest's attempts fail.
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(
        !alex.connected() && w.reconnecting(),
        "reconnected during the outage: {:?}",
        g.connection_path()
    );

    w.cable.plug_in();
    let back = within(Duration::from_secs(30), "the guest is back", || {
        alex.connected() && !w.reconnecting()
    })
    .await;
    eprintln!("reconnected {back:?} after the network came back");
    within(Duration::from_secs(5), "Resumed", || {
        alex.seen.lock().unwrap().resumed == 1
    })
    .await;
    assert_eq!(g.me(), me);
    let again = w
        .host
        .participants()
        .into_iter()
        .find(|p| p.id == me)
        .unwrap();
    assert_eq!(again.color, place.color);
    assert_eq!(again.permissions, permissions);
    assert_eq!(g.permissions(), permissions);
    alex.resync(doc).await;
    within(Duration::from_secs(10), "both edits everywhere", || {
        let t = w.host_text();
        t.contains("// offline") && t.contains("// host") && alex.text() == t
    })
    .await;
    alex.insert(doc, 0, "x");
    within(Duration::from_secs(10), "live edits flow again", || {
        w.host_text().starts_with("x// offline")
    })
    .await;
    within(Duration::from_secs(10), "the missed output", || {
        tab.text().contains("built while away")
    })
    .await;
    w.host.terminal_output("term-1", "live again\r\n");
    within(Duration::from_secs(10), "live output", || {
        tab.text().contains("live again")
    })
    .await;
    assert_eq!(tab.text(), "built while away\r\nlive again\r\n");
    assert!(alex.seen.lock().unwrap().ended.is_none());
    w.host.end("The host ended the session.").await;
}

/// An outage longer than the host's grace: the host frees the place, and once the network
/// is back the guest is told it lost it instead of being let in again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn after_an_outage_longer_than_the_grace_the_guest_hears_its_place_is_gone() {
    let w = World::new(Permissions::EDIT_FILES).await;
    w.host.set_reconnect_grace(Duration::from_secs(1));
    let alex = w.join().await;
    w.cable.unplug();
    let cut = Instant::now();
    let (host_noticed, guest_noticed) = noticed(&w, &alex, cut).await;
    within(Duration::from_secs(5), "the place is freed", || {
        w.host.participants().len() == 1
    })
    .await;
    eprintln!("outage noticed: host after {host_noticed:?}, guest after {guest_noticed:?}");
    within(Duration::from_secs(5), "the host was told", || {
        w.host_seen
            .lock()
            .unwrap()
            .notices
            .iter()
            .any(|n| n == "Alex lost the connection and did not come back.")
    })
    .await;
    assert!(!alex.connected());
    w.cable.plug_in();
    let told = within(Duration::from_secs(30), "the guest hears why", || {
        alex.seen.lock().unwrap().ended.is_some()
    })
    .await;
    eprintln!("the guest heard it {told:?} after the network came back");
    let ended = alex.seen.lock().unwrap().ended.clone().unwrap();
    assert!(ended.contains("no longer keeps your place"), "{ended}");
    assert!(alex.session.is_ended());
    assert_eq!(w.host.participants().len(), 1);
    w.host.end("The host ended the session.").await;
}
