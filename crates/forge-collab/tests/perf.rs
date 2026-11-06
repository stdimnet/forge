//! Measurements of the plan's performance targets (`docs/plans/code-with-me.md`,
//! "Performance targets"): edit and caret propagation over the real transport (iroh/QUIC on
//! the loopback interface) and over the in-memory network with injected latency, joining
//! with 20 open files, opening a 1 MB file as a guest, applying one update to a 100 000-line
//! document, an idle session's CPU and traffic, Find in Files for a guest and the host's
//! memory per shared document.
//!
//! The memory test always runs. The timing tests are `#[ignore]`d (they need a release build
//! and an otherwise idle machine) and print what they measured:
//!
//! ```sh
//! cargo test -p forge-collab --release --test perf -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Each timing test also asserts its target, with the numbers in the failure message.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use forge_backend::{Backend, LocalBackend};
use forge_collab::docs::SharedDoc;
use forge_collab::excludes::Excludes;
use forge_collab::guest::{GuestEvent, GuestSession};
use forge_collab::guest_backend::CollabBackend;
use forge_collab::host::{DevAutoAccept, HostConfig, HostSession};
use forge_collab::link::SessionLink;
use forge_collab::permissions::Permissions;
use forge_collab::protocol::DocId;
use forge_collab::transport::memory::{MemoryNetwork, MemoryTransport};
use forge_collab::transport::net::{ListenOptions, NetTransport};
use forge_collab::transport::{BoxFuture, BoxRead, BoxWrite, Connection, PeerKey, Transport};
use forge_core::search::FindRequest;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc;
use yrs::sync::awareness::{AwarenessUpdate, AwarenessUpdateEntry};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{
    ClientID, Doc, GetString, OffsetKind, Options, ReadTxn, Text, TextRef, Transact, Update,
};

mod cpu;

// ---- memory accounting ------------------------------------------------------------------

/// Counts the bytes each thread has allocated and not freed yet, so one test can measure
/// what a structure it builds holds while other tests run on other threads.
struct Counting;

thread_local! {
    static LIVE: Cell<isize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = LIVE.try_with(|l| l.set(l.get() + layout.size() as isize));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _ = LIVE.try_with(|l| l.set(l.get() - layout.size() as isize));
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = LIVE.try_with(|l| l.set(l.get() + new_size as isize - layout.size() as isize));
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn live() -> isize {
    LIVE.with(Cell::get)
}

// ---- helpers ----------------------------------------------------------------------------

fn config(max_file_size: usize) -> HostConfig {
    HostConfig {
        host_name: "Robert".into(),
        machine: None,
        permissions: Permissions::FULL_ACCESS,
        excludes: Excludes::defaults(),
        protected: Excludes::protected_defaults(),
        max_file_size,
        notice: None,
    }
}

fn loopback() -> ListenOptions {
    ListenOptions {
        address: Some("127.0.0.1".parse().unwrap()),
        port: 0,
    }
}

/// PHP-like source text of `lines` lines (about 40 bytes each).
fn source(lines: usize, seed: usize) -> String {
    (0..lines)
        .map(|i| {
            format!(
                "    $order{seed}->line({i}, 'item-{:05}');\n",
                (i * 7919) % 100_000
            )
        })
        .collect()
}

/// A project of `files` (path, text).
fn project(files: &[(String, String)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn start_host(
    transport: Arc<dyn Transport>,
    dir: &std::path::Path,
    max: usize,
) -> Arc<HostSession> {
    let backend: Arc<dyn Backend> = Arc::new(LocalBackend::open(dir).unwrap());
    HostSession::start(
        transport,
        backend,
        config(max),
        Arc::new(DevAutoAccept),
        Arc::new(|_| {}),
    )
}

/// One participant's editor replica of a document.
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

    fn len(&self) -> u32 {
        self.text.len(&self.doc.transact())
    }

    /// Type `insert` at `at` (clamped); the update to send.
    fn type_at(&self, at: u32, insert: &str) -> Vec<u8> {
        let before = self.doc.transact().state_vector();
        {
            let mut txn = self.doc.transact_mut();
            let at = at.min(self.text.len(&txn));
            self.text.insert(&mut txn, at, insert);
        }
        self.doc.transact().encode_state_as_update_v1(&before)
    }

    fn string(&self) -> String {
        self.text.get_string(&self.doc.transact())
    }
}

/// xorshift, so runs are repeatable.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// p50, p99 and max of `samples`.
struct Stats {
    p50: Duration,
    p99: Duration,
    max: Duration,
}

impl Stats {
    fn of(samples: &mut [Duration]) -> Self {
        samples.sort();
        let at = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
        Self {
            p50: at(0.5),
            p99: at(0.99),
            max: *samples.last().unwrap(),
        }
    }
}

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms",
            ms(self.p50),
            ms(self.p99),
            ms(self.max)
        )
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

// ---- edits and carets -------------------------------------------------------------------

/// A host and two guests, Alex and Sam, with `app/Order.php` open; Sam's replica applies
/// what arrives and reports when.
struct Trio {
    host: Arc<HostSession>,
    _dir: tempfile::TempDir,
    doc: DocId,
    host_replica: Replica,
    alex: Arc<GuestSession>,
    alex_replica: Replica,
    _sam: Arc<GuestSession>,
    sam_replica: Arc<Mutex<Option<Replica>>>,
    edits: mpsc::UnboundedReceiver<Instant>,
    carets: mpsc::UnboundedReceiver<Instant>,
    /// The document's length when it was opened, and the characters typed since.
    initial: u32,
    typed: u32,
}

impl Trio {
    async fn start(
        host_t: Arc<dyn Transport>,
        alex_t: Arc<dyn Transport>,
        sam_t: Arc<dyn Transport>,
    ) -> Self {
        let dir = project(&[("app/Order.php".into(), source(300, 0))]);
        let host = start_host(host_t, dir.path(), 1 << 20);
        let opened = host.open_doc("app/Order.php", None).await.unwrap();
        let host_replica = Replica::new(&opened.update);
        let host_replica_len = host_replica.len();
        let link = SessionLink::parse(&host.link().to_url()).unwrap();
        let alex = GuestSession::join(alex_t, &link, "Alex", Arc::new(|_| {}))
            .await
            .unwrap();
        let alex_replica = Replica::new(&alex.open_doc("app/Order.php").await.unwrap().update);
        let (edit_tx, edits) = mpsc::unbounded_channel();
        let (caret_tx, carets) = mpsc::unbounded_channel();
        let sam_replica: Arc<Mutex<Option<Replica>>> = Arc::default();
        let replica = sam_replica.clone();
        let sam = GuestSession::join(
            sam_t,
            &link,
            "Sam",
            Arc::new(move |e| match e {
                GuestEvent::DocUpdate { update, .. } => {
                    if let Some(r) = replica.lock().unwrap().as_ref() {
                        r.apply(&update);
                    }
                    let _ = edit_tx.send(Instant::now());
                }
                GuestEvent::Awareness { .. } => {
                    let _ = caret_tx.send(Instant::now());
                }
                _ => {}
            }),
        )
        .await
        .unwrap();
        let opened = sam.open_doc("app/Order.php").await.unwrap();
        *sam_replica.lock().unwrap() = Some(Replica::new(&opened.update));
        Self {
            host,
            _dir: dir,
            doc: opened.doc,
            host_replica,
            alex,
            alex_replica,
            _sam: sam,
            sam_replica,
            edits,
            carets,
            initial: host_replica_len,
            typed: 0,
        }
    }

    fn drain(&mut self) {
        while self.edits.try_recv().is_ok() {}
        while self.carets.try_recv().is_ok() {}
    }

    /// Alex types `n` characters, one every `gap`: how long each took to reach Sam's replica
    /// through the host.
    async fn guest_to_guest(&mut self, n: usize, gap: Duration) -> Stats {
        self.drain();
        let mut rng = Rng(7);
        let mut samples = Vec::with_capacity(n);
        for _ in 0..n {
            let at = (rng.next() % self.alex_replica.len() as u64) as u32;
            let update = self.alex_replica.type_at(at, "x");
            let sent = Instant::now();
            self.alex.update(self.doc, update);
            let seen = tokio::time::timeout(Duration::from_secs(10), self.edits.recv())
                .await
                .expect("the edit arrived")
                .unwrap();
            samples.push(seen - sent);
            self.typed += 1;
            tokio::time::sleep(gap).await;
        }
        Stats::of(&mut samples)
    }

    /// The host types `n` characters: how long each took to reach Sam's replica.
    async fn host_to_guest(&mut self, n: usize, gap: Duration) -> Stats {
        self.drain();
        let mut rng = Rng(11);
        let mut samples = Vec::with_capacity(n);
        for _ in 0..n {
            let at = (rng.next() % self.host_replica.len() as u64) as u32;
            let update = self.host_replica.type_at(at, "h");
            let sent = Instant::now();
            self.host.host_update(self.doc, &update).unwrap();
            let seen = tokio::time::timeout(Duration::from_secs(10), self.edits.recv())
                .await
                .expect("the edit arrived")
                .unwrap();
            samples.push(seen - sent);
            self.typed += 1;
            tokio::time::sleep(gap).await;
        }
        Stats::of(&mut samples)
    }

    /// Alex moves the caret `n` times (an awareness update each, as y-codemirror sends):
    /// how long each took to reach Sam.
    async fn carets(&mut self, n: usize, gap: Duration) -> Stats {
        self.drain();
        let mut samples = Vec::with_capacity(n);
        for i in 0..n {
            let json = format!(r#"{{"cursor":{{"anchor":{{"pos":{i}}},"head":{{"pos":{i}}}}}}}"#);
            let update = AwarenessUpdate {
                clients: HashMap::from([(
                    ClientID::new(4242),
                    AwarenessUpdateEntry {
                        clock: i as u32 + 1,
                        json: json.into(),
                    },
                )]),
            };
            let sent = Instant::now();
            self.alex.awareness(self.doc, update.encode_v1());
            let seen = tokio::time::timeout(Duration::from_secs(10), self.carets.recv())
                .await
                .expect("the caret arrived")
                .unwrap();
            samples.push(seen - sent);
            tokio::time::sleep(gap).await;
        }
        Stats::of(&mut samples)
    }

    /// Sam's replica holds every character typed so far.
    async fn check_converged(&self) {
        let want = self.initial + self.typed;
        let len = || self.sam_replica.lock().unwrap().as_ref().unwrap().len();
        for _ in 0..500 {
            if len() == want {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(len(), want);
    }
}

/// Remote edits and carets over iroh/QUIC on the loopback interface (the LAN target; the
/// loopback RTT is far below a LAN's, so this is the software's share of the latency).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "timing: run on demand in release, see the module docs"]
async fn edits_and_carets_over_quic() {
    let host_t = Arc::new(NetTransport::listen(loopback()).await.unwrap());
    let mut trio = Trio::start(
        host_t,
        Arc::new(NetTransport::dialer().await.unwrap()),
        Arc::new(NetTransport::dialer().await.unwrap()),
    )
    .await;
    // Warm up the connections.
    trio.guest_to_guest(20, Duration::from_millis(5)).await;
    // Typing pace: 5 to 10 characters a second per participant is normal, 50 ms is fast.
    let g2g = trio.guest_to_guest(500, Duration::from_millis(20)).await;
    let h2g = trio.host_to_guest(500, Duration::from_millis(20)).await;
    let carets = trio.carets(500, Duration::from_millis(20)).await;
    trio.check_converged().await;
    eprintln!("QUIC loopback: guest -> host -> guest edit: {g2g}");
    eprintln!("QUIC loopback: host -> guest edit: {h2g}");
    eprintln!("QUIC loopback: guest -> host -> guest caret: {carets}");
    assert!(g2g.p50 < Duration::from_millis(30) && g2g.p99 < Duration::from_millis(100));
    assert!(h2g.p50 < Duration::from_millis(30) && h2g.p99 < Duration::from_millis(100));
    assert!(carets.p99 < Duration::from_millis(50));
    trio.host.end("The host ended the session.").await;
}

/// Remote edits and carets over the in-memory network with a one-way delay per link: 1 ms
/// for a LAN, 40 ms for a relay (80 ms RTT). Alex's edit travels Alex -> host -> Sam, so
/// the network alone accounts for one RTT; the overhead on top of it is the software's.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "timing: run on demand in release, see the module docs"]
async fn edits_and_carets_with_latency() {
    for (name, one_way) in [("LAN", 1u64), ("relay", 40)] {
        let one_way = Duration::from_millis(one_way);
        let rtt = one_way * 2;
        let net = MemoryNetwork::new();
        net.set_latency(one_way);
        let mut trio = Trio::start(
            Arc::new(net.endpoint([1; 32])),
            Arc::new(net.endpoint([2; 32])),
            Arc::new(net.endpoint([3; 32])),
        )
        .await;
        // Edits further apart than the delay, so they never queue behind each other.
        let gap = rtt + Duration::from_millis(10);
        let n = if one_way > Duration::from_millis(10) {
            150
        } else {
            400
        };
        let g2g = trio.guest_to_guest(n, gap).await;
        let carets = trio.carets(n, gap).await;
        trio.check_converged().await;
        eprintln!(
            "memory, {name} (one-way {one_way:?}, RTT {rtt:?}): guest -> host -> guest edit: {g2g}; caret: {carets}"
        );
        if name == "LAN" {
            assert!(g2g.p50 < Duration::from_millis(30) && g2g.p99 < Duration::from_millis(100));
        } else {
            assert!(g2g.p50 < rtt + Duration::from_millis(30), "{g2g}");
        }
        assert!(carets.p50 < rtt + Duration::from_millis(50), "{carets}");
        trio.host.end("The host ended the session.").await;
    }
}

// ---- joining and opening ----------------------------------------------------------------

/// The host has 20 files open (5 to 40 KB each); a guest joins over QUIC and opens all of
/// them into editor replicas (the most a guest's workbench needs before it is usable).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "timing: run on demand in release, see the module docs"]
async fn joining_with_twenty_open_files() {
    let files: Vec<(String, String)> = (0..20)
        .map(|i| (format!("app/Models/Model{i}.php"), source(125 + i * 50, i)))
        .collect();
    let total: usize = files.iter().map(|(_, t)| t.len()).sum();
    let dir = project(&files);
    let host = start_host(
        Arc::new(NetTransport::listen(loopback()).await.unwrap()),
        dir.path(),
        1 << 20,
    );
    for (path, _) in &files {
        host.open_doc(path, None).await.unwrap();
    }
    let link = SessionLink::parse(&host.link().to_url()).unwrap();
    let mut joins = Vec::new();
    let mut usable = Vec::new();
    for round in 0..5 {
        let transport = Arc::new(NetTransport::dialer().await.unwrap());
        let started = Instant::now();
        let guest = GuestSession::join(
            transport,
            &link,
            &format!("Guest {round}"),
            Arc::new(|_| {}),
        )
        .await
        .unwrap();
        joins.push(started.elapsed());
        let opens = files.iter().map(|(path, _)| {
            let guest = guest.clone();
            async move {
                let opened = guest.open_doc(path).await.unwrap();
                Replica::new(&opened.update).string().len()
            }
        });
        let sizes = futures_util::future::join_all(opens).await;
        usable.push(started.elapsed());
        assert_eq!(sizes.iter().sum::<usize>(), total);
        guest.leave().await;
    }
    let joins = Stats::of(&mut joins);
    let usable = Stats::of(&mut usable);
    eprintln!("join over QUIC loopback: {joins}");
    eprintln!(
        "join + open 20 files ({} KB) into replicas: {usable}",
        total / 1024
    );
    assert!(usable.max < Duration::from_secs(2), "{usable}");
    host.end("The host ended the session.").await;
}

/// A guest opens 1 MB files over QUIC: request, the host reads the file and builds its
/// document, the whole state crosses, the guest's replica applies it and renders the text.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "timing: run on demand in release, see the module docs"]
async fn opening_a_one_megabyte_file_as_a_guest() {
    let files: Vec<(String, String)> = (0..5)
        .map(|i| {
            let mut text = source(25_000, i);
            text.truncate(1_000_000);
            (format!("storage/big{i}.php"), text)
        })
        .collect();
    let dir = project(&files);
    let host = start_host(
        Arc::new(NetTransport::listen(loopback()).await.unwrap()),
        dir.path(),
        1 << 20,
    );
    let link = SessionLink::parse(&host.link().to_url()).unwrap();
    let guest = GuestSession::join(
        Arc::new(NetTransport::dialer().await.unwrap()),
        &link,
        "Alex",
        Arc::new(|_| {}),
    )
    .await
    .unwrap();
    let mut samples = Vec::new();
    for (path, text) in &files {
        let started = Instant::now();
        let opened = guest.open_doc(path).await.unwrap();
        assert!(!opened.read_only);
        let shown = Replica::new(&opened.update).string();
        samples.push(started.elapsed());
        assert_eq!(shown.len(), text.len());
    }
    let stats = Stats::of(&mut samples);
    eprintln!("open a 1 MB file as a guest over QUIC loopback (first open each): {stats}");
    assert!(stats.max < Duration::from_millis(300), "{stats}");
    host.end("The host ended the session.").await;
}

// ---- one update on a large document -----------------------------------------------------

/// Apply single keystrokes from a guest's replica to the host's document of 100 000 lines:
/// `SharedDoc::apply_update` alone, the whole host path (`HostSession::host_update` with
/// nine guests that have the file open) and a Rust replica (the guest side without the UI).
/// Measured on a fresh document and again after 20 000 edits from three writers.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "timing: run on demand in release, see the module docs"]
async fn one_update_on_a_hundred_thousand_line_document() {
    let text = source(100_000, 1);
    let mut doc = SharedDoc::new(DocId(1), "big.php".into(), &text, None);
    let typist = Replica::new(&doc.full_update());
    let watcher = Replica::new(&doc.full_update());
    let mut rng = Rng(3);
    for phase in ["fresh", "after 20 000 edits"] {
        if phase != "fresh" {
            let others = [
                Replica::new(&doc.full_update()),
                Replica::new(&doc.full_update()),
            ];
            for i in 0..20_000 {
                let r = &others[i % 2];
                let u = r.type_at((rng.next() % r.len() as u64) as u32, "y");
                doc.apply_update(&u).unwrap();
                typist.apply(&u);
                watcher.apply(&u);
                others[(i + 1) % 2].apply(&u);
            }
        }
        let (mut host, mut replica) = (Vec::new(), Vec::new());
        for _ in 0..2000 {
            let u = typist.type_at((rng.next() % typist.len() as u64) as u32, "z");
            let t = Instant::now();
            doc.apply_update(&u).unwrap();
            host.push(t.elapsed());
            let t = Instant::now();
            watcher.apply(&u);
            replica.push(t.elapsed());
        }
        eprintln!(
            "100 000 lines, {phase}: SharedDoc::apply_update {}; Rust replica {}",
            Stats::of(&mut host),
            Stats::of(&mut replica)
        );
        assert!(Stats::of(&mut host).p99 < Duration::from_millis(1));
    }
    assert_eq!(doc.text(), watcher.string());

    // The host's whole path, fanning out to nine guests over the in-memory network.
    let dir = project(&[("big.php".into(), text)]);
    let net = MemoryNetwork::new();
    let host = start_host(Arc::new(net.endpoint([1; 32])), dir.path(), 8 << 20);
    let opened = host.open_doc("big.php", None).await.unwrap();
    let editor = Replica::new(&opened.update);
    let link = host.link();
    let mut guests = Vec::new();
    for i in 0..9u8 {
        let g = GuestSession::join(
            Arc::new(net.endpoint([i + 2; 32])),
            &link,
            &format!("Guest {i}"),
            Arc::new(|_| {}),
        )
        .await
        .unwrap();
        g.open_doc("big.php").await.unwrap();
        guests.push(g);
    }
    let mut samples = Vec::new();
    for _ in 0..2000 {
        let u = editor.type_at((rng.next() % editor.len() as u64) as u32, "h");
        let t = Instant::now();
        host.host_update(opened.doc, &u).unwrap();
        samples.push(t.elapsed());
        // Typing pace, so the guests' queues drain.
        if samples.len() % 20 == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    let stats = Stats::of(&mut samples);
    eprintln!("100 000 lines: HostSession::host_update with 9 guests: {stats}");
    assert!(stats.p99 < Duration::from_millis(1), "{stats}");
    host.end("The host ended the session.").await;
}

// ---- idle session -----------------------------------------------------------------------

/// A transport that counts the bytes its connections send and receive.
struct Metered {
    inner: MemoryTransport,
    bytes: Arc<AtomicU64>,
}

struct MeteredIo<T> {
    inner: T,
    bytes: Arc<AtomicU64>,
}

impl AsyncRead for MeteredIo<BoxRead> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let poll = Pin::new(&mut self.inner).poll_read(cx, buf);
        let n = buf.filled().len() - before;
        self.bytes.fetch_add(n as u64, Ordering::Relaxed);
        poll
    }
}

impl AsyncWrite for MeteredIo<BoxWrite> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let poll = Pin::new(&mut self.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = &poll {
            self.bytes.fetch_add(*n as u64, Ordering::Relaxed);
        }
        poll
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl Metered {
    fn wrap(&self, c: Connection) -> Connection {
        Connection {
            remote: c.remote,
            send: Box::new(MeteredIo {
                inner: c.send,
                bytes: self.bytes.clone(),
            }),
            recv: Box::new(MeteredIo {
                inner: c.recv,
                bytes: self.bytes.clone(),
            }),
            keepalive: c.keepalive,
        }
    }
}

impl Transport for Metered {
    fn local_key(&self) -> PeerKey {
        self.inner.local_key()
    }

    fn addresses(&self) -> Vec<SocketAddr> {
        self.inner.addresses()
    }

    fn accept(&self) -> BoxFuture<'_, Option<Connection>> {
        Box::pin(async move { self.inner.accept().await.map(|c| self.wrap(c)) })
    }

    fn connect(
        &self,
        host: PeerKey,
        addrs: Vec<SocketAddr>,
    ) -> BoxFuture<'_, io::Result<Connection>> {
        Box::pin(async move { self.inner.connect(host, addrs).await.map(|c| self.wrap(c)) })
    }

    fn close(&self) -> BoxFuture<'_, ()> {
        self.inner.close()
    }
}

/// Bytes sent on the loopback interface so far (`/proc/net/dev`; the whole network
/// namespace, so other programs using loopback meanwhile count too).
fn loopback_bytes() -> Option<u64> {
    let dev = std::fs::read_to_string("/proc/net/dev").ok()?;
    let line = dev.lines().find(|l| l.trim_start().starts_with("lo:"))?;
    let fields: Vec<&str> = line.split_once(':')?.1.split_whitespace().collect();
    fields.get(8)?.parse().ok()
}

/// An idle session: a host and two guests with a file open, nobody typing. Over QUIC on
/// loopback: the CPU time of the process's threads and the bytes on the loopback interface
/// (both guests' connections, QUIC keep-alives included); on the in-memory network: the
/// session's own messages on one guest's connection.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "timing: run on demand in release, see the module docs"]
async fn an_idle_session() {
    const IDLE: Duration = Duration::from_secs(10);
    let trio = Trio::start(
        Arc::new(NetTransport::listen(loopback()).await.unwrap()),
        Arc::new(NetTransport::dialer().await.unwrap()),
        Arc::new(NetTransport::dialer().await.unwrap()),
    )
    .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let (cpu_before, lo_before, started) = (cpu::threads(""), loopback_bytes(), Instant::now());
    tokio::time::sleep(IDLE).await;
    let wall = started.elapsed();
    let cpu_used = cpu::threads("")
        .zip(cpu_before)
        .map(|(a, b)| a.saturating_sub(b));
    let quic = loopback_bytes()
        .zip(lo_before)
        .map(|(a, b)| (a - b) as f64 / wall.as_secs_f64() / 2.0);
    trio.host.end("The host ended the session.").await;

    let net = MemoryNetwork::new();
    let bytes = Arc::new(AtomicU64::new(0));
    let trio = Trio::start(
        Arc::new(net.endpoint([1; 32])),
        Arc::new(Metered {
            inner: net.endpoint([2; 32]),
            bytes: bytes.clone(),
        }),
        Arc::new(net.endpoint([3; 32])),
    )
    .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let before = bytes.load(Ordering::Relaxed);
    tokio::time::sleep(IDLE).await;
    let per_second = (bytes.load(Ordering::Relaxed) - before) as f64 / IDLE.as_secs_f64();
    trio.host.end("The host ended the session.").await;

    let percent = cpu_used.map(|c| cpu::percent(c, wall));
    eprintln!(
        "idle session (host + 2 guests in one process, QUIC loopback): CPU {}; \
         loopback traffic per guest {} (QUIC keep-alives); session messages per guest {per_second:.1} B/s",
        percent.map_or("n/a".into(), |p| format!("{p:.3} % of one core")),
        quic.map_or("n/a".into(), |b| format!("{b:.1} B/s")),
    );
    if let Some(p) = percent {
        assert!(p < 1.0, "idle CPU {p:.2} %");
    }
    if let Some(b) = quic {
        assert!(b < 1024.0, "idle traffic {b:.1} B/s per guest");
    }
    assert!(per_second < 1024.0);
}

// ---- Find in Files ----------------------------------------------------------------------

/// Find in Files from a guest over QUIC against the host's own search on 2 000 files: the
/// guest's extra time should be about one round trip (plus moving the results).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "timing: run on demand in release, see the module docs"]
async fn find_in_files_for_a_guest() {
    let files: Vec<(String, String)> = (0..2000)
        .map(|i| (format!("src/M{}/F{i}.php", i % 40), source(60, i)))
        .collect();
    let dir = project(&files);
    let local = LocalBackend::open(dir.path()).unwrap();
    let host = start_host(
        Arc::new(NetTransport::listen(loopback()).await.unwrap()),
        dir.path(),
        1 << 20,
    );
    let link = SessionLink::parse(&host.link().to_url()).unwrap();
    let session = GuestSession::join(
        Arc::new(NetTransport::dialer().await.unwrap()),
        &link,
        "Alex",
        Arc::new(|_| {}),
    )
    .await
    .unwrap();
    let s = session.clone();
    let guest = Arc::new(
        tokio::task::spawn_blocking(move || CollabBackend::open(s))
            .await
            .unwrap()
            .unwrap(),
    );
    let request = FindRequest {
        query: "item-0042".into(),
        ..FindRequest::default()
    };
    let (mut local_t, mut guest_t, mut rtt) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..30 {
        let t = Instant::now();
        let want = local.find_in_files(&request).unwrap();
        local_t.push(t.elapsed());
        let (g, r) = (guest.clone(), request.clone());
        let t = Instant::now();
        let got = tokio::task::spawn_blocking(move || g.find_in_files(&r))
            .await
            .unwrap()
            .unwrap();
        guest_t.push(t.elapsed());
        assert_eq!(got.match_count, want.match_count);
        let t = Instant::now();
        session.ping().await.unwrap();
        rtt.push(t.elapsed());
    }
    let (local_t, guest_t, rtt) = (
        Stats::of(&mut local_t),
        Stats::of(&mut guest_t),
        Stats::of(&mut rtt),
    );
    eprintln!(
        "Find in Files, 2 000 files: host's search {local_t}; guest over QUIC loopback {guest_t}; round trip {rtt}"
    );
    // One RTT on a LAN is a few milliseconds; allow that on top of the host's search.
    assert!(guest_t.p50 < local_t.p50 + Duration::from_millis(5) + rtt.p50);
    host.end("The host ended the session.").await;
}

// ---- memory -----------------------------------------------------------------------------

/// The bytes a `SharedDoc` of `lines` lines holds, as multiples of the text's size: fresh,
/// after `edits` characters typed in runs of 20 (as people type) and after `edits` more
/// single characters at random places (the worst case: every character is its own CRDT
/// item and splits another one). The target is for the first two; the third is printed.
fn memory_per_shared_document(lines: usize, edits: usize) {
    let text = source(lines, 2);
    let base = live();
    let mut doc = SharedDoc::new(DocId(1), "big.php".into(), &text, None);
    let fresh = live() - base;
    let typist = Replica::new(&doc.full_update());
    let mut rng = Rng(5);
    let mut held = fresh;
    let mut ratios = Vec::new();
    for run in [20, 1] {
        let mut at = 0;
        let updates: Vec<Vec<u8>> = (0..edits)
            .map(|i| {
                if i % run == 0 {
                    at = (rng.next() % typist.len() as u64) as u32;
                }
                at += 1;
                typist.type_at(at - 1, "e")
            })
            .collect();
        let before = live();
        for u in &updates {
            doc.apply_update(u).unwrap();
        }
        held += live() - before;
        ratios.push(held as f64 / text.len() as f64);
    }
    let fresh = fresh as f64 / text.len() as f64;
    eprintln!(
        "SharedDoc of {} KB ({lines} lines): {fresh:.2} x the text fresh, {:.2} x after \
         {edits} characters typed in runs of 20, {:.2} x after {edits} more at random places",
        text.len() / 1024,
        ratios[0],
        ratios[1],
    );
    assert!(fresh < 3.0, "{fresh:.2}");
    assert!(ratios[0] < 3.0, "{:.2}", ratios[0]);
}

/// The host's memory for a shared 1 MB file stays below 3 times its text.
#[test]
fn host_memory_per_shared_document() {
    memory_per_shared_document(25_000, 1_000);
}

/// The same for 1 MB and 4 MB files (25 000 and 100 000 lines) and 10 000 edits each.
#[test]
#[ignore = "heavy: run on demand in release, see the module docs"]
fn host_memory_per_large_shared_document() {
    memory_per_shared_document(25_000, 10_000);
    memory_per_shared_document(100_000, 10_000);
}
