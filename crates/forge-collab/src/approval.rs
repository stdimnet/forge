//! Host approval: [`Approvals`] is the [`Admission`] that asks the host before a guest joins.
//!
//! A guest that proved it has the link becomes a pending request. The host's window shows it
//! with the verification code ([`ApprovalEvent::Request`]) and answers with
//! [`Approvals::answer`]. Nobody is admitted without that answer: not answering within the
//! time limit, ending the session and requests beyond the limits all refuse the guest.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::oneshot;

use crate::host::{Admission, Decision, JoinRequest};
use crate::transport::{BoxFuture, PeerKey};

/// How long the host has to answer. The guest gives up after two minutes; the host's request
/// disappears a little earlier so it never answers a guest that has already left.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(110);

/// Open requests at once. A guest with the link cannot bury the host in prompts.
pub const MAX_PENDING: usize = 5;

/// What the host's window shows or removes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalEvent {
    /// "`name` wants to join. Make sure the code is `code`."
    Request { id: u64, name: String, code: String },
    /// The request is no longer open (unanswered, or the session ended).
    Closed { id: u64 },
}

pub type ApprovalSink = Arc<dyn Fn(ApprovalEvent) + Send + Sync>;

struct Pending {
    guest: PeerKey,
    answer: oneshot::Sender<bool>,
}

#[derive(Default)]
struct Inner {
    next_id: u64,
    open: HashMap<u64, Pending>,
}

pub struct Approvals {
    inner: Arc<Mutex<Inner>>,
    sink: ApprovalSink,
    timeout: Duration,
}

impl Approvals {
    pub fn new(sink: ApprovalSink) -> Arc<Self> {
        Self::with_timeout(sink, ANSWER_TIMEOUT)
    }

    pub fn with_timeout(sink: ApprovalSink, timeout: Duration) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::default(),
            sink,
            timeout,
        })
    }

    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("approvals lock poisoned")
    }

    /// Answer a request. `false` when it is not open any more (answered, expired).
    pub fn answer(&self, id: u64, accept: bool) -> bool {
        match self.inner().open.remove(&id) {
            Some(p) => p.answer.send(accept).is_ok(),
            None => false,
        }
    }

    /// Requests waiting for an answer.
    pub fn pending(&self) -> usize {
        self.inner().open.len()
    }

    /// The session ended: refuse everyone still waiting.
    pub fn refuse_all(&self) {
        let open = std::mem::take(&mut self.inner().open);
        for (id, p) in open {
            let _ = p.answer.send(false);
            (self.sink)(ApprovalEvent::Closed { id });
        }
    }
}

impl Admission for Approvals {
    fn decide(&self, request: JoinRequest) -> BoxFuture<'static, Decision> {
        let (tx, rx) = oneshot::channel();
        let id = {
            let mut inner = self.inner();
            // One open request per guest, and a limit overall.
            if inner.open.len() >= MAX_PENDING
                || inner.open.values().any(|p| p.guest == request.guest)
            {
                return Box::pin(async { Decision::Decline });
            }
            inner.next_id += 1;
            let id = inner.next_id;
            inner.open.insert(
                id,
                Pending {
                    guest: request.guest,
                    answer: tx,
                },
            );
            id
        };
        (self.sink)(ApprovalEvent::Request {
            id,
            name: request.name,
            code: request.code,
        });
        let timeout = self.timeout;
        let sink = self.sink.clone();
        let inner = self.inner.clone();
        Box::pin(async move {
            match tokio::time::timeout(timeout, rx).await {
                Ok(Ok(true)) => Decision::Accept(None),
                Ok(_) => Decision::Decline,
                Err(_) => {
                    // Unanswered: the request goes away like an answered one.
                    inner
                        .lock()
                        .expect("approvals lock poisoned")
                        .open
                        .remove(&id);
                    sink(ApprovalEvent::Closed { id });
                    Decision::Unanswered
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn request(guest: u8, name: &str) -> JoinRequest {
        JoinRequest {
            guest: [guest; 32],
            name: name.into(),
            code: "482 913".into(),
        }
    }

    fn approvals(timeout: Duration) -> (Arc<Approvals>, Arc<Mutex<Vec<ApprovalEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let seen = events.clone();
        let a = Approvals::with_timeout(Arc::new(move |e| seen.lock().unwrap().push(e)), timeout);
        (a, events)
    }

    #[tokio::test]
    async fn the_host_sees_the_request_with_the_code_and_accepts() {
        let (a, events) = approvals(Duration::from_secs(5));
        let decision = tokio::spawn(a.decide(request(1, "Alex")));
        tokio::task::yield_now().await;
        assert_eq!(
            *events.lock().unwrap(),
            vec![ApprovalEvent::Request {
                id: 1,
                name: "Alex".into(),
                code: "482 913".into()
            }]
        );
        assert!(a.answer(1, true));
        assert_eq!(decision.await.unwrap(), Decision::Accept(None));
        assert_eq!(a.pending(), 0);
        // Answering twice does nothing.
        assert!(!a.answer(1, true));
    }

    #[tokio::test]
    async fn declining_refuses_the_guest() {
        let (a, _) = approvals(Duration::from_secs(5));
        let decision = tokio::spawn(a.decide(request(1, "Alex")));
        tokio::task::yield_now().await;
        assert!(a.answer(1, false));
        assert_eq!(decision.await.unwrap(), Decision::Decline);
    }

    #[tokio::test(start_paused = true)]
    async fn an_unanswered_request_expires() {
        let (a, events) = approvals(Duration::from_secs(110));
        let decision = a.decide(request(1, "Alex")).await;
        assert_eq!(decision, Decision::Unanswered);
        assert_eq!(a.pending(), 0);
        assert_eq!(
            events.lock().unwrap().last(),
            Some(&ApprovalEvent::Closed { id: 1 })
        );
        // Too late to accept.
        assert!(!a.answer(1, true));
    }

    #[tokio::test]
    async fn a_guest_has_one_open_request() {
        let (a, events) = approvals(Duration::from_secs(5));
        let _first = tokio::spawn(a.decide(request(1, "Alex")));
        tokio::task::yield_now().await;
        assert_eq!(a.decide(request(1, "Alex again")).await, Decision::Decline);
        assert_eq!(events.lock().unwrap().len(), 1);
        assert_eq!(a.pending(), 1);
    }

    #[tokio::test]
    async fn requests_beyond_the_limit_are_refused() {
        let (a, events) = approvals(Duration::from_secs(5));
        for n in 0..MAX_PENDING as u8 {
            tokio::spawn(a.decide(request(n, "Guest")));
        }
        tokio::task::yield_now().await;
        assert_eq!(a.pending(), MAX_PENDING);
        assert_eq!(
            a.decide(request(99, "One too many")).await,
            Decision::Decline
        );
        assert_eq!(events.lock().unwrap().len(), MAX_PENDING);
    }

    #[tokio::test]
    async fn ending_the_session_refuses_everyone_waiting() {
        let (a, events) = approvals(Duration::from_secs(5));
        let count = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for n in 0..3u8 {
            let c = count.clone();
            let d = a.decide(request(n, "Guest"));
            tasks.push(tokio::spawn(async move {
                let r = d.await;
                c.fetch_add(1, Ordering::SeqCst);
                r
            }));
        }
        tokio::task::yield_now().await;
        a.refuse_all();
        for t in tasks {
            assert_eq!(t.await.unwrap(), Decision::Decline);
        }
        assert_eq!(count.load(Ordering::SeqCst), 3);
        assert_eq!(a.pending(), 0);
        let closed = events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e, ApprovalEvent::Closed { .. }))
            .count();
        assert_eq!(closed, 3);
    }
}
