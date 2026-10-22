//! Host approval end to end over the in-memory network: the host sees the same verification
//! code as the guest, nobody gets in before the host accepts, and a declined or unanswered
//! guest is told so.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_backend::{Backend, LocalBackend};
use forge_collab::approval::{ApprovalEvent, Approvals};
use forge_collab::excludes::Excludes;
use forge_collab::guest::GuestSession;
use forge_collab::host::{HostConfig, HostSession};
use forge_collab::permissions::Permissions;
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
