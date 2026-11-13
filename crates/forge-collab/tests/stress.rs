//! The 10-participant stress test of the plan's performance targets: the host and nine guests
//! over the in-memory network type into two shared files, move their carets and follow each
//! other, while the host's shared terminal prints and the guests type into it. Afterwards
//! every replica must hold the same text, everyone must see everyone's last presence, every
//! guest must have the terminal's whole output once, and no queue may have grown: the
//! backlog per guest (host side) and per guest connection stays far below the bound at
//! which a guest is cut off, and drains to zero.
//!
//! `cargo test -p forge-collab --test stress` runs a short version (about two seconds). The
//! heavier variant with network latency and more edits runs on demand:
//!
//! ```sh
//! cargo test -p forge-collab --release --test stress -- --ignored --nocapture
//! ```
//!
//! `FORGE_CWM_STRESS_ROUNDS` sets the number of rounds per participant for both variants.
//!
//! The host's session runs on its own runtime (threads named `cwm-host`), so the CPU time it
//! uses while everyone types is measured apart from the guests' (Linux `/proc`, see
//! `cpu/mod.rs`); the heavy variant checks the plan's "host CPU < 10 % of one core".

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_backend::{Backend, LocalBackend};
use forge_collab::excludes::Excludes;
use forge_collab::guest::{GuestEvent, GuestSession};
use forge_collab::host::{
    DevAutoAccept, HostConfig, HostEvent, HostServices, HostSession, LspRequest,
};
use forge_collab::permissions::Permissions;
use forge_collab::presence::{Caret, ParticipantId, Presence, Viewport};
use forge_collab::protocol::DocId;
use forge_collab::transport::memory::MemoryNetwork;
use forge_process::PtyEvent;
use futures_util::future::BoxFuture;
use yrs::updates::decoder::Decode;
use yrs::{Doc, GetString, OffsetKind, Options, ReadTxn, Text, TextRef, Transact, Update};

mod cpu;

const GUESTS: usize = 9;
const FILES: [&str; 2] = ["app/Order.php", "routes/web.php"];
/// What a guest's connection may queue before the host cuts it off (`host::GUEST_QUEUE`).
const CUT_OFF: usize = 4096;

/// Shell keys the guests typed, as the host's terminal received them.
#[derive(Default)]
struct Shell {
    keys: Mutex<Vec<String>>,
}

impl HostServices for Shell {
    fn lsp(&self, _req: LspRequest) -> BoxFuture<'static, Result<Vec<u8>, String>> {
        Box::pin(async { Err("no language servers".to_string()) })
    }

    fn terminal_write(&self, _key: &str, data: &str) -> Result<(), String> {
        self.keys.lock().unwrap().push(data.to_string());
        Ok(())
    }
}

/// One participant's editor: a replica per document and updates that came early.
#[derive(Default)]
struct Editor {
    docs: HashMap<DocId, (Doc, TextRef)>,
    early: HashMap<DocId, Vec<Vec<u8>>>,
    problems: Vec<String>,
}

type Shared = Arc<Mutex<Editor>>;

impl Editor {
    fn receive(&mut self, doc: DocId, update: &[u8]) {
        match self.docs.get(&doc) {
            Some((d, _)) => d
                .transact_mut()
                .apply_update(Update::decode_v1(update).unwrap())
                .unwrap(),
            None => self.early.entry(doc).or_default().push(update.to_vec()),
        }
    }

    fn open(&mut self, doc: DocId, state: &[u8]) {
        let d = Doc::with_options(Options {
            offset_kind: OffsetKind::Utf16,
            ..Options::default()
        });
        let t = d.get_or_insert_text("content");
        for u in std::iter::once(state.to_vec()).chain(self.early.remove(&doc).unwrap_or_default())
        {
            d.transact_mut()
                .apply_update(Update::decode_v1(&u).unwrap())
                .unwrap();
        }
        self.docs.insert(doc, (d, t));
    }

    fn text(&self, doc: DocId) -> String {
        let (d, t) = &self.docs[&doc];
        t.get_string(&d.transact())
    }

    /// Type `insert` at the `pick`-th character boundary; the update to send.
    fn edit(&self, doc: DocId, pick: u64, delete: bool, insert: &str) -> Vec<u8> {
        let (d, t) = &self.docs[&doc];
        let before = d.transact().state_vector();
        {
            let mut txn = d.transact_mut();
            let text = t.get_string(&txn);
            let bounds: Vec<u32> = std::iter::once(0)
                .chain(text.chars().scan(0u32, |at, c| {
                    *at += c.len_utf16() as u32;
                    Some(*at)
                }))
                .collect();
            let i = (pick % bounds.len() as u64) as usize;
            let at = bounds[i];
            if delete && i + 1 < bounds.len() {
                t.remove_range(&mut txn, at, bounds[i + 1] - at);
            }
            t.insert(&mut txn, at, insert);
        }
        d.transact().encode_state_as_update_v1(&before)
    }
}

/// xorshift, so a failure replays.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

const SNIPPETS: &[&str] = &["a", "ü", "😀", "\n", "$x", " ", "𝄞", "->"];

struct Guest {
    session: Arc<GuestSession>,
    editor: Shared,
    terminal: Arc<Mutex<String>>,
}

fn rounds(default: usize) -> usize {
    std::env::var("FORGE_CWM_STRESS_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// The host's own runtime, shut down without blocking when the test ends (dropping a
/// runtime inside another one panics).
struct HostRuntime(Option<tokio::runtime::Runtime>);

impl HostRuntime {
    fn new() -> Self {
        Self(Some(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("cwm-host")
                .enable_all()
                .build()
                .unwrap(),
        ))
    }
}

impl Drop for HostRuntime {
    fn drop(&mut self) {
        if let Some(rt) = self.0.take() {
            rt.shutdown_background();
        }
    }
}

/// What the stress run measured while everyone typed.
struct Load {
    /// CPU time of the host's session threads, as a percentage of one core.
    host_cpu: Option<f64>,
}

async fn until(what: &str, limit: Duration, mut ok: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ok() {
        assert!(start.elapsed() < limit, "timed out waiting until {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn stress(rounds: usize, latency: Duration, pause: Duration) -> Load {
    let started = Instant::now();
    let host_rt = HostRuntime::new();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("app")).unwrap();
    std::fs::create_dir_all(root.join("routes")).unwrap();
    std::fs::write(root.join(FILES[0]), "<?php\n\nclass Order\n{\n}\n").unwrap();
    std::fs::write(
        root.join(FILES[1]),
        "<?php\n\nRoute::get('/', fn () => 'ok');\n",
    )
    .unwrap();
    let backend: Arc<dyn Backend> = Arc::new(LocalBackend::open(root).unwrap());
    let net = MemoryNetwork::new();
    net.set_latency(latency);

    let host_editor: Shared = Arc::default();
    let sink = host_editor.clone();
    // Everything the session spawns (accepting, one task per guest) runs on the host's runtime.
    let host_context = host_rt.0.as_ref().unwrap().enter();
    let host = HostSession::start(
        Arc::new(net.endpoint([1; 32])),
        backend,
        HostConfig {
            host_name: "Robert".into(),
            machine: None,
            permissions: Permissions::FULL_ACCESS,
            excludes: Excludes::defaults(),
            protected: Excludes::protected_defaults(),
            max_file_size: 1024 * 1024,
            notice: None,
        },
        Arc::new(DevAutoAccept),
        Arc::new(move |e| {
            if let HostEvent::DocUpdate { doc, update, .. } = e {
                sink.lock().unwrap().receive(doc, &update);
            }
        }),
    );
    drop(host_context);
    let shell = Arc::new(Shell::default());
    host.set_services(shell.clone());
    host.share_terminal("term-1", "Local", 120, 30, "$ ", true);

    let mut docs = Vec::new();
    for path in FILES {
        let o = host.open_doc(path, None).await.unwrap();
        host_editor.lock().unwrap().open(o.doc, &o.update);
        docs.push(o.doc);
    }

    // Nine guests join, open both files and attach to the shared terminal.
    let mut guests = Vec::new();
    for i in 0..GUESTS {
        let editor: Shared = Arc::default();
        let sink = editor.clone();
        let session = GuestSession::join(
            Arc::new(net.endpoint([i as u8 + 2; 32])),
            &host.link(),
            &format!("Guest {}", i + 1),
            Arc::new(move |e| {
                let mut ed = sink.lock().unwrap();
                match e {
                    GuestEvent::DocUpdate { doc, update } => ed.receive(doc, &update),
                    GuestEvent::Denied(r) | GuestEvent::Ended(r) => ed.problems.push(r),
                    GuestEvent::DocClosed { reason, .. } => ed.problems.push(reason),
                    _ => {}
                }
            }),
        )
        .await
        .unwrap();
        for path in FILES {
            let o = session.open_doc(path).await.unwrap();
            editor.lock().unwrap().open(o.doc, &o.update);
        }
        until("the shared terminal", Duration::from_secs(10), || {
            !session.terminals().is_empty()
        })
        .await;
        let terminal: Arc<Mutex<String>> = Arc::default();
        let out = terminal.clone();
        let id = session.terminals()[0].id;
        let (before, _, _) = session
            .attach_terminal(
                id,
                Arc::new(move |e| {
                    if let PtyEvent::Output { data } = e {
                        out.lock().unwrap().push_str(&data);
                    }
                }),
            )
            .unwrap();
        terminal.lock().unwrap().push_str(&before);
        guests.push(Guest {
            session,
            editor,
            terminal,
        });
    }
    let ids: Vec<ParticipantId> = guests.iter().map(|g| g.session.me()).collect();
    let joined = started.elapsed();
    let cpu_start = (cpu::process(), cpu::threads("cwm-host"));

    // Sample the backlogs while everyone works.
    let peak = Arc::new(Mutex::new((0usize, 0usize)));
    let sampling = {
        let (host, peak) = (host.clone(), peak.clone());
        let sessions: Vec<_> = guests.iter().map(|g| g.session.clone()).collect();
        tokio::spawn(async move {
            loop {
                let guest = sessions.iter().map(|s| s.backlog()).max().unwrap_or(0);
                let host = host.backlog();
                {
                    let mut p = peak.lock().unwrap();
                    p.0 = p.0.max(host);
                    p.1 = p.1.max(guest);
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
    };

    let mut tasks = Vec::new();
    for (i, g) in guests.iter().enumerate() {
        let (session, editor, docs, ids) = (
            g.session.clone(),
            g.editor.clone(),
            docs.clone(),
            ids.clone(),
        );
        tasks.push(tokio::spawn(async move {
            let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ (i as u64 + 1));
            let terminal = session.terminals()[0].id;
            for round in 0..rounds {
                let doc = docs[(rng.next() % docs.len() as u64) as usize];
                let snippet = SNIPPETS[(rng.next() % SNIPPETS.len() as u64) as usize];
                let update = editor.lock().unwrap().edit(
                    doc,
                    rng.next(),
                    rng.next().is_multiple_of(5),
                    snippet,
                );
                session.update(doc, update);
                // Every few rounds the caret moves, or the guest follows someone else.
                if round.is_multiple_of(4) {
                    let line = (rng.next() % 40) as u32 + 1;
                    let following = rng
                        .next()
                        .is_multiple_of(3)
                        .then(|| ids[(rng.next() % ids.len() as u64) as usize])
                        .filter(|p| *p != session.me())
                        .or(rng.next().is_multiple_of(2).then_some(ParticipantId::HOST));
                    session.set_presence(Presence {
                        file: Some(FILES[(rng.next() % 2) as usize].into()),
                        caret: Some(Caret { line, column: 1 }),
                        anchor: None,
                        viewport: Some(Viewport {
                            top_line: line,
                            bottom_line: line + 30,
                        }),
                        following,
                    });
                }
                if round.is_multiple_of(8) {
                    session.terminal_input(terminal, "l");
                }
                tokio::time::sleep(pause).await;
            }
            // The last word on where this guest is, so everyone can check it.
            session.set_presence(Presence {
                file: Some(FILES[0].into()),
                caret: Some(Caret {
                    line: i as u32 + 1,
                    column: 7,
                }),
                ..Presence::default()
            });
        }));
    }
    // The host types and its terminal prints meanwhile.
    let mut expected_output = String::from("$ ");
    let mut rng = Rng(42);
    for round in 0..rounds {
        let doc = docs[round % docs.len()];
        let update = host_editor
            .lock()
            .unwrap()
            .edit(doc, rng.next(), false, "h");
        host.host_update(doc, &update).unwrap();
        let line = format!("output line {round}\r\n");
        host.terminal_output("term-1", &line);
        expected_output.push_str(&line);
        if round.is_multiple_of(4) {
            host.host_presence(Presence {
                file: Some(FILES[round % 2].into()),
                caret: Some(Caret {
                    line: round as u32 % 40 + 1,
                    column: 1,
                }),
                ..Presence::default()
            });
        }
        tokio::time::sleep(pause).await;
    }
    for t in tasks {
        t.await.unwrap();
    }
    let typed = started.elapsed();
    let cpu_end = (cpu::process(), cpu::threads("cwm-host"));
    let typing = typed - joined;
    let usage = |a: Option<Duration>, b: Option<Duration>| {
        a.zip(b)
            .map(|(a, b)| cpu::percent(b.saturating_sub(a), typing))
    };
    let process_cpu = usage(cpu_start.0, cpu_end.0);
    let host_cpu = usage(cpu_start.1, cpu_end.1);

    // Convergence: every replica equals the host's editor.
    let limit = Duration::from_secs(30);
    for &doc in &docs {
        let want = host_editor.lock().unwrap().text(doc);
        for (i, g) in guests.iter().enumerate() {
            let ed = g.editor.clone();
            let host_editor = host_editor.clone();
            until(&format!("guest {} converged", i + 1), limit, || {
                ed.lock().unwrap().text(doc) == host_editor.lock().unwrap().text(doc)
            })
            .await;
        }
        assert!(want.len() > 20);
    }
    let converged = started.elapsed();
    // Everyone sees the last presence of everyone else.
    for g in &guests {
        let s = g.session.clone();
        let me = s.me();
        until("everyone's last presence", limit, || {
            let seen: HashMap<_, _> = s.presence().into_iter().collect();
            ids.iter()
                .enumerate()
                .filter(|(_, p)| **p != me)
                .all(|(i, p)| {
                    seen.get(p).and_then(|x| x.caret)
                        == Some(Caret {
                            line: i as u32 + 1,
                            column: 7,
                        })
                })
        })
        .await;
    }
    // Every guest has the terminal's output exactly once; the shell got every guest's keys.
    for g in &guests {
        let t = g.terminal.clone();
        until("the terminal output", limit, || {
            t.lock().unwrap().len() >= expected_output.len()
        })
        .await;
        assert_eq!(*g.terminal.lock().unwrap(), expected_output);
    }
    let keys = GUESTS * rounds.div_ceil(8);
    until("the guests' keys", limit, || {
        shell.keys.lock().unwrap().len() == keys
    })
    .await;
    // Nothing was refused or closed, nobody was cut off, and the queues drained.
    for g in &guests {
        assert_eq!(g.editor.lock().unwrap().problems, Vec::<String>::new());
    }
    assert_eq!(host.participants().len(), GUESTS + 1);
    until("the host's queues drained", limit, || host.backlog() == 0).await;
    sampling.abort();
    let (host_peak, guest_peak) = *peak.lock().unwrap();
    assert!(
        host_peak < CUT_OFF / 4 && guest_peak < CUT_OFF / 4,
        "backlog peaks: host {host_peak}, guest {guest_peak} (cut off at {CUT_OFF})"
    );
    let cores = cpu::cores();
    eprintln!(
        "stress: {GUESTS} guests x {rounds} rounds, latency {latency:?}: joined {joined:?}, \
         typed {typed:?}, converged {converged:?}; backlog peaks host {host_peak}, guest {guest_peak}; \
         CPU while typing ({typing:?}, {cores} cores): host session {}, whole process {} \
         ({} per core)",
        host_cpu.map_or("n/a".into(), |p| format!("{p:.1} % of one core")),
        process_cpu.map_or("n/a".into(), |p| format!("{p:.1} % of one core")),
        process_cpu.map_or("n/a".into(), |p| format!("{:.1} %", p / cores as f64)),
    );
    Load { host_cpu }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ten_participants_edit_follow_and_share_a_terminal() {
    stress(rounds(200), Duration::ZERO, Duration::from_millis(1)).await;
}

/// The plan's target load: everyone typing 5 characters a second for a minute, 5 ms latency.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "heavy: run on demand, see the module docs"]
async fn ten_participants_under_load() {
    let load = stress(
        rounds(300),
        Duration::from_millis(5),
        Duration::from_millis(200),
    )
    .await;
    if let Some(host) = load.host_cpu {
        assert!(host < 10.0, "the host used {host:.1} % of one core");
    }
}
