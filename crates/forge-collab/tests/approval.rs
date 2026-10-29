//! Host approval end to end over the in-memory network: the host sees the same verification
//! code as the guest, nobody gets in before the host accepts, and a declined or unanswered
//! guest is told so.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_backend::{Backend, LocalBackend};
use forge_collab::approval::{ApprovalEvent, Approvals};
use forge_collab::audit::SessionLog;
use forge_collab::excludes::Excludes;
use forge_collab::guest::{GuestEvent, GuestSession};
use forge_collab::host::{HostConfig, HostSession};
use forge_collab::permissions::Permissions;
use forge_collab::presence::ParticipantId;
use forge_collab::transport::memory::MemoryNetwork;

struct Setup {
    _dir: tempfile::TempDir,
    net: MemoryNetwork,
    host: Arc<HostSession>,
    approvals: Arc<Approvals>,
    events: Arc<Mutex<Vec<ApprovalEvent>>>,
}

fn setup(timeout: Duration) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "# Shop\n").unwrap();
    let backend: Arc<dyn Backend> = Arc::new(LocalBackend::open(dir.path()).unwrap());
    let net = MemoryNetwork::new();
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = events.clone();
    let approvals =
        Approvals::with_timeout(Arc::new(move |e| seen.lock().unwrap().push(e)), timeout);
    let host = HostSession::start(
        Arc::new(net.endpoint([1; 32])),
        backend,
        HostConfig {
            host_name: "Robert".into(),
            machine: None,
            permissions: Permissions::READ_ONLY,
            excludes: Excludes::defaults(),
            max_file_size: 64 * 1024,
            notice: None,
        },
        approvals.clone(),
        Arc::new(|_| {}),
    );
    Setup {
        _dir: dir,
        net,
        host,
        approvals,
        events,
    }
}

impl Setup {
    fn join(
        &self,
        key: u8,
        name: &str,
    ) -> tokio::task::JoinHandle<Result<Arc<GuestSession>, String>> {
        let link = self.host.link();
        let transport = Arc::new(self.net.endpoint([key; 32]));
        let name = name.to_string();
        tokio::spawn(
            async move { GuestSession::join(transport, &link, &name, Arc::new(|_| {})).await },
        )
    }

    async fn request(&self) -> (u64, String, String) {
        for _ in 0..200 {
            if let Some(ApprovalEvent::Request { id, name, code }) =
                self.events.lock().unwrap().last().cloned()
            {
                return (id, name, code);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the host was never asked");
    }
}

#[tokio::test]
async fn the_guest_waits_until_the_host_accepts_and_both_see_the_same_code() {
    let s = setup(Duration::from_secs(30));
    let guest = s.join(2, "Alex");
    let (id, name, code) = s.request().await;
    assert_eq!(name, "Alex");
    assert_eq!(code.len(), 7, "two groups of three digits: {code:?}");
    // Nobody is in the session yet.
    assert!(!guest.is_finished());
    assert!(s.approvals.answer(id, true));
    let guest = guest.await.unwrap().unwrap();
    assert_eq!(guest.verification_code(), code);
}

#[tokio::test]
async fn a_declined_guest_is_told_so() {
    let s = setup(Duration::from_secs(30));
    let guest = s.join(2, "Alex");
    let (id, _, _) = s.request().await;
    assert!(s.approvals.answer(id, false));
    let err = guest.await.unwrap().err().expect("declined");
    assert_eq!(err, "The host declined your request.");
}

#[tokio::test]
async fn an_unanswered_guest_is_told_the_host_did_not_answer() {
    let s = setup(Duration::from_millis(100));
    let err = s.join(2, "Alex").await.unwrap().err().expect("unanswered");
    assert_eq!(err, "The host did not answer.");
    assert_eq!(s.approvals.pending(), 0);
}

#[tokio::test]
async fn ending_the_session_refuses_a_waiting_guest() {
    let s = setup(Duration::from_secs(30));
    let guest = s.join(2, "Alex");
    s.request().await;
    s.approvals.refuse_all();
    assert!(guest.await.unwrap().is_err());
}

// ---- during the session: permissions, Remove Guest, Regenerate Link ------------------------

type Events = Arc<Mutex<Vec<GuestEvent>>>;

impl Setup {
    /// A guest the host accepted, with the events it receives.
    async fn admitted(&self, key: u8, name: &str) -> (Arc<GuestSession>, Events) {
        let events: Events = Arc::default();
        let seen = events.clone();
        let link = self.host.link();
        let transport = Arc::new(self.net.endpoint([key; 32]));
        let name = name.to_string();
        let guest = tokio::spawn(async move {
            GuestSession::join(
                transport,
                &link,
                &name,
                Arc::new(move |e| seen.lock().unwrap().push(e)),
            )
            .await
        });
        let (id, _, _) = self.request().await;
        self.events.lock().unwrap().clear();
        assert!(self.approvals.answer(id, true));
        (guest.await.unwrap().unwrap(), events)
    }
}

async fn until(what: &str, check: impl Fn() -> bool) {
    for _ in 0..300 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

#[tokio::test]
async fn permission_changes_reach_the_guest_and_count_at_once() {
    let s = setup(Duration::from_secs(30));
    let (guest, _) = s.admitted(2, "Alex").await;
    assert_eq!(guest.permissions(), Permissions::READ_ONLY);
    let me = guest.me();

    assert!(s.host.set_guest_permissions(me, Permissions::EDIT_FILES));
    until("Edit files", || {
        guest.permissions() == Permissions::EDIT_FILES
    })
    .await;
    let host_view = s.host.participants();
    assert_eq!(host_view[1].permissions, Permissions::EDIT_FILES);
    let doc = guest.open_doc("README.md").await.unwrap();
    assert!(!doc.read_only);
    guest.save(doc.doc).await.unwrap();

    // The session's permission replaces the guest's own, and new guests get it too.
    s.host.set_permissions(Permissions::READ_ONLY);
    assert_eq!(s.host.permissions(), Permissions::READ_ONLY);
    until("Read-only", || {
        guest.permissions() == Permissions::READ_ONLY
    })
    .await;
    let err = guest.save(doc.doc).await.expect_err("read-only now");
    assert_eq!(err, "Your permission is Read-only");
    assert!(!s
        .host
        .set_guest_permissions(ParticipantId(99), Permissions::FULL_ACCESS));
}

#[tokio::test]
async fn a_removed_guest_is_disconnected_and_told_why() {
    let s = setup(Duration::from_secs(30));
    let (guest, events) = s.admitted(2, "Alex").await;
    assert_eq!(s.host.participants().len(), 2);
    assert!(s.host.remove(guest.me()));
    assert_eq!(s.host.participants().len(), 1);
    until("the guest to hear it", || {
        events.lock().unwrap().contains(&GuestEvent::Ended(
            "The host removed you from the session.".into(),
        ))
    })
    .await;
    assert!(!s.host.remove(guest.me()));
}

#[tokio::test]
async fn a_regenerated_link_locks_out_the_old_one() {
    let s = setup(Duration::from_secs(30));
    let (guest, _) = s.admitted(2, "Alex").await;
    let old = s.host.link();
    let new = s.host.regenerate_link();
    assert_ne!(old.to_url(), new.to_url());
    assert_eq!(new.to_url(), s.host.link().to_url());
    // The guest in the session stays.
    assert_eq!(s.host.participants().len(), 2);
    assert!(!guest.is_ended());

    // The old link is dropped before the host is asked.
    let transport = Arc::new(s.net.endpoint([3; 32]));
    let stale = tokio::time::timeout(
        Duration::from_secs(5),
        GuestSession::join(transport, &old, "Sam", Arc::new(|_| {})),
    )
    .await;
    assert!(!matches!(stale, Ok(Ok(_))), "the old link still works");
    assert!(s.events.lock().unwrap().is_empty(), "the host was asked");

    // The new one asks the host as before.
    let (third, _) = s.admitted(4, "Kim").await;
    assert_eq!(third.permissions(), Permissions::READ_ONLY);
    assert_eq!(s.host.participants().len(), 3);
}

#[tokio::test]
async fn the_session_log_records_admission_changes_and_refusals() {
    let s = setup(Duration::from_secs(30));
    let log = SessionLog::memory();
    s.host.set_audit(log.clone());
    let (guest, _) = s.admitted(2, "Alex").await;
    let doc = guest.open_doc("README.md").await.unwrap();
    // Read-only: the save is refused and logged.
    assert!(guest.save(doc.doc).await.is_err());
    s.host
        .set_guest_permissions(guest.me(), Permissions::EDIT_FILES);
    until("Edit files", || {
        guest.permissions() == Permissions::EDIT_FILES
    })
    .await;
    guest.save(doc.doc).await.unwrap();
    s.host.regenerate_link();
    s.host.remove(guest.me());
    s.host.end("The host ended the session.").await;

    let lines: Vec<String> = log.lines().into_iter().map(|l| l.message).collect();
    assert_eq!(
        lines,
        [
            "Robert started the session (guests: Read-only)",
            "Alex joined (Read-only)",
            "Refused save of Alex: Your permission is Read-only",
            "Permission of Alex: Edit files",
            "Alex saved README.md",
            "Regenerated the session link",
            "Removed Alex",
            "Session ended: The host ended the session.",
        ]
    );
}
