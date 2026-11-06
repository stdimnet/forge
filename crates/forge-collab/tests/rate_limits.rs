//! Per-guest rate limits end to end: a guest that floods the host with requests is refused
//! (one session log entry per burst) and a guest that floods it with edits is disconnected,
//! while a guest working normally next to it is not affected.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_backend::{Backend, LocalBackend};
use forge_collab::audit::SessionLog;
use forge_collab::excludes::Excludes;
use forge_collab::guest::{GuestEvent, GuestSession};
use forge_collab::guest_backend::calls::HostCall;
use forge_collab::host::{DevAutoAccept, HostConfig, HostSession};
use forge_collab::permissions::Permissions;
use forge_collab::rate::{CALL_RATE, FLOODED, FLOOD_RATE, TOO_MANY};
use forge_collab::transport::memory::MemoryNetwork;
use futures_util::future::join_all;
use yrs::updates::decoder::Decode;
use yrs::{Doc, GetString, OffsetKind, Options, ReadTxn, Text, Transact, Update};

const TIMEOUT: Duration = Duration::from_secs(20);

struct World {
    _tmp: tempfile::TempDir,
    net: MemoryNetwork,
    host: Arc<HostSession>,
    log: Arc<SessionLog>,
    next_key: u8,
}

/// A guest and what reached its window.
struct Guest {
    session: Arc<GuestSession>,
    denied: Arc<Mutex<Vec<String>>>,
    ended: Arc<Mutex<Option<String>>>,
}

impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("shop");
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::write(root.join("app/Order.php"), "<?php\n").unwrap();
        let backend: Arc<dyn Backend> = Arc::new(LocalBackend::open(&root).unwrap());
        let net = MemoryNetwork::new();
        let host = HostSession::start(
            Arc::new(net.endpoint([1; 32])),
            backend,
            HostConfig {
                host_name: "Robert".into(),
                machine: None,
                permissions: Permissions::EDIT_FILES,
                excludes: Excludes::defaults(),
                protected: Excludes::protected_defaults(),
                max_file_size: 64 * 1024,
                notice: None,
            },
            Arc::new(DevAutoAccept),
            Arc::new(|_| {}),
        );
        let log = SessionLog::memory();
        host.set_audit(log.clone());
        Self {
            _tmp: tmp,
            net,
            host,
            log,
            next_key: 2,
        }
    }

    async fn join(&mut self, name: &str) -> Guest {
        let key = [self.next_key; 32];
        self.next_key += 1;
        let denied: Arc<Mutex<Vec<String>>> = Arc::default();
        let ended: Arc<Mutex<Option<String>>> = Arc::default();
        let (d, e) = (denied.clone(), ended.clone());
        let session = GuestSession::join(
            Arc::new(self.net.endpoint(key)),
            &self.host.link(),
            name,
            Arc::new(move |event| match event {
                GuestEvent::Denied(reason) => d.lock().unwrap().push(reason),
                GuestEvent::Ended(reason) => *e.lock().unwrap() = Some(reason),
                _ => {}
            }),
        )
        .await
        .unwrap();
        Guest {
            session,
            denied,
            ended,
        }
    }

    /// Session log lines that contain `text`.
    fn logged(&self, text: &str) -> Vec<String> {
        self.log
            .lines()
            .into_iter()
            .map(|l| l.message)
            .filter(|m| m.contains(text))
            .collect()
    }
}

async fn until(what: &str, mut f: impl FnMut() -> bool) {
    for _ in 0..1000 {
        if f() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

async fn pings(g: &Guest, n: usize) -> Vec<Result<Vec<u8>, String>> {
    join_all((0..n).map(|_| g.session.call(&HostCall::Ping, TIMEOUT))).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_guest_flooding_requests_is_refused_and_logged_once() {
    let mut w = World::new();
    let alex = w.join("Alex").await;
    let mallory = w.join("Mallory").await;

    // Twice the burst at once: the rest of the burst is answered "too many".
    let flood = (CALL_RATE.burst * 2) as usize;
    let results = pings(&mallory, flood).await;
    let refused = results
        .iter()
        .filter(|r| r.as_ref().err().map(String::as_str) == Some(TOO_MANY))
        .count();
    assert!(
        refused >= flood / 3,
        "{refused} of {flood} calls refused for their rate"
    );
    // Documents have their own limit and still open.
    assert!(mallory.session.open_doc("app/Order.php").await.is_ok());

    // Alex, at the same time, works normally.
    for r in pings(&alex, 20).await {
        assert!(r.is_ok(), "{r:?}");
    }
    assert!(alex.session.open_doc("app/Order.php").await.is_ok());

    // One log entry for the whole burst, none for Alex.
    assert_eq!(
        w.logged("Too many requests"),
        vec![format!("Refused call of Mallory: {TOO_MANY}")]
    );
    assert!(w.logged("of Alex").is_empty(), "{:?}", w.logged("of Alex"));
    assert!(alex.denied.lock().unwrap().is_empty());
    assert!(alex.ended.lock().unwrap().is_none());
    assert!(
        mallory.ended.lock().unwrap().is_none(),
        "refused, not removed"
    );

    // Once the bucket refilled, Mallory's calls go through again.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(mallory.session.call(&HostCall::Ping, TIMEOUT).await.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_guest_flooding_edits_is_disconnected_and_nobody_else() {
    let mut w = World::new();
    let alex = w.join("Alex").await;
    let mallory = w.join("Mallory").await;
    let opened = mallory.session.open_doc("app/Order.php").await.unwrap();
    let alex_doc = alex.session.open_doc("app/Order.php").await.unwrap();
    assert_eq!(alex_doc.doc, opened.doc);

    // Valid edits, one character each, far more at once than anyone types.
    let replica = Doc::with_options(Options {
        offset_kind: OffsetKind::Utf16,
        ..Options::default()
    });
    let text = replica.get_or_insert_text("content");
    replica
        .transact_mut()
        .apply_update(Update::decode_v1(&opened.update).unwrap())
        .unwrap();
    for _ in 0..FLOOD_RATE.burst * 2 {
        let before = replica.transact().state_vector();
        text.insert(&mut replica.transact_mut(), 0, "x");
        let update = replica.transact().encode_state_as_update_v1(&before);
        mallory.session.update(opened.doc, update);
    }

    until("Mallory is disconnected", || {
        mallory.ended.lock().unwrap().is_some()
    })
    .await;
    assert_eq!(mallory.ended.lock().unwrap().as_deref(), Some(FLOODED));
    until("Mallory leaves the participants", || {
        w.host.participants().iter().all(|p| p.name != "Mallory")
    })
    .await;
    assert_eq!(w.logged("far more messages").len(), 1);
    assert_eq!(w.logged("Removed Mallory").len(), 1);

    // Edits were never dropped one by one: the host has every edit up to the cut-off.
    let host_doc = w.host.open_doc("app/Order.php", None).await.unwrap();
    let check = Doc::with_options(Options {
        offset_kind: OffsetKind::Utf16,
        ..Options::default()
    });
    let check_text = check.get_or_insert_text("content");
    check
        .transact_mut()
        .apply_update(Update::decode_v1(&host_doc.update).unwrap())
        .unwrap();
    let got = check_text.get_string(&check.transact());
    let typed = got.strip_suffix("<?php\n").unwrap();
    assert!(typed.chars().all(|c| c == 'x'), "{got:?}");
    assert!(
        typed.len() >= FLOOD_RATE.burst as usize / 2 && typed.len() < 2 * FLOOD_RATE.burst as usize,
        "{} edits arrived",
        typed.len()
    );

    // Alex stays in the session and keeps working.
    assert!(w.host.participants().iter().any(|p| p.name == "Alex"));
    assert!(alex.ended.lock().unwrap().is_none());
    assert!(alex.denied.lock().unwrap().is_empty());
    assert!(alex.session.call(&HostCall::Ping, TIMEOUT).await.is_ok());
    assert!(w.logged("of Alex").is_empty());
}
