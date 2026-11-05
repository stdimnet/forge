//! The project lifecycle of a window: which project closes and which one opens, in four
//! phases every project-based service can subscribe to.
//!
//! | Phase | Project | Purpose |
//! | --- | --- | --- |
//! | [`ProjectPhase::BeforeClose`] | the old one | can object (terminals with running commands) |
//! | [`ProjectPhase::Closing`] | the old one | stop and save (watchers, language servers, terminals) |
//! | [`ProjectPhase::Opened`] | the new one | load and start (after the window's trust decision) |
//! | [`ProjectPhase::Ready`] | the new one | after indexing, for features that need the index |
//!
//! Rust owns the lifecycle because Rust owns the workspace. [`ProjectLifecycleBus`] runs the
//! [`ProjectLifecycle`] subscribers in a fixed order (by [`ProjectLifecycle::order`], then by
//! subscription) with error isolation: a subscriber that fails or panics is reported in the
//! [`PhaseReport`] and the others still run. The desktop app publishes every phase it ran to
//! the UI as one event stream ([`LIFECYCLE_EVENT`]), where `onProject(phase, handler)` follows
//! the same rules.
//!
//! `beforeClose` is a question, not a notification: [`ProjectLifecycleBus::before_close`]
//! collects the subscribers' [`Objection`]s (it closes nothing). The UI shows them (for
//! example "Terminate 2 running processes?") and only a confirmed close goes on to `closing`.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The event that carries every phase to the UI ([`LifecycleEvent`]).
pub const LIFECYCLE_EVENT: &str = "project://lifecycle";

/// A phase of the project lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectPhase {
    BeforeClose,
    Closing,
    Opened,
    Ready,
}

/// The project a phase is about.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRef {
    pub name: String,
    /// The root as the UI identifies the project: a local path, `user@host:/path` for a
    /// remote project, `cwm://host/name` for a Code With Me guest.
    pub root: String,
    /// The repository root containing the project, when it is known (local projects).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vcs_root: Option<String>,
    /// Trusted (not in Safe Mode) at the time of the phase.
    #[serde(default)]
    pub trusted: bool,
    /// Further details of the host application (the remote host, the Code With Me host).
    #[serde(flatten)]
    pub details: Map<String, Value>,
}

impl ProjectRef {
    pub fn new(name: impl Into<String>, root: impl Into<String>) -> Self {
        ProjectRef {
            name: name.into(),
            root: root.into(),
            ..ProjectRef::default()
        }
    }
}

/// Why a subscriber does not want the project to close without asking the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Objection {
    /// The subscriber that objects ([`ProjectLifecycle::name`]).
    pub source: String,
    /// What kind of objection this is, e.g. `terminal.running`; UI handlers claim the
    /// kinds they know and render their own question.
    pub kind: String,
    /// A sentence for a generic question ("2 terminal commands are still running").
    pub message: String,
    /// What is affected, e.g. the running programs (`sleep in Local (2)`).
    #[serde(default)]
    pub items: Vec<String>,
}

/// A subscriber of the project lifecycle (git, language servers, terminals, watchers, packs).
///
/// Every method has a default, so subscribers implement only the phases they need. The
/// methods block the switch while they run; long work belongs on a thread of its own.
pub trait ProjectLifecycle: Send + Sync {
    /// Shown in logs and in [`PhaseReport::failures`].
    fn name(&self) -> &str;

    /// Lower runs first; subscribers with the same order run in the order they subscribed.
    fn order(&self) -> i32 {
        0
    }

    /// The project is about to close: object to it, or `None` to let it close.
    fn before_close(&self, _project: &ProjectRef) -> Option<Objection> {
        None
    }

    /// The project closes: stop and save.
    fn closing(&self, _project: &ProjectRef) -> crate::Result<()> {
        Ok(())
    }

    /// The project opened in this window: load and start.
    fn opened(&self, _project: &ProjectRef) -> crate::Result<()> {
        Ok(())
    }

    /// The project's index is built.
    fn ready(&self, _project: &ProjectRef) -> crate::Result<()> {
        Ok(())
    }
}

/// Identifies a subscription for [`ProjectLifecycleBus::unsubscribe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(u64);

/// What a phase did: the project, the open generation it belongs to and who failed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhaseReport {
    pub phase: ProjectPhase,
    pub project: ProjectRef,
    /// Counts the projects opened in this window; `ready` of an earlier one is dropped.
    pub generation: u64,
    /// `(subscriber, error)` of subscribers that failed or panicked.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<(String, String)>,
}

/// The payload of [`LIFECYCLE_EVENT`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleEvent {
    pub phase: ProjectPhase,
    pub generation: u64,
    pub project: ProjectRef,
}

impl From<&PhaseReport> for LifecycleEvent {
    fn from(r: &PhaseReport) -> Self {
        LifecycleEvent {
            phase: r.phase,
            generation: r.generation,
            project: r.project.clone(),
        }
    }
}

struct Entry {
    id: u64,
    order: i32,
    subscriber: Arc<dyn ProjectLifecycle>,
}

#[derive(Default)]
struct Current {
    project: Option<ProjectRef>,
    generation: u64,
    ready: bool,
}

/// The lifecycle of one window's project and its subscribers.
#[derive(Default)]
pub struct ProjectLifecycleBus {
    subscribers: RwLock<Vec<Entry>>,
    next_id: Mutex<u64>,
    current: Mutex<Current>,
}

impl ProjectLifecycleBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a subscriber; it takes part from the next phase on.
    pub fn subscribe(&self, subscriber: Arc<dyn ProjectLifecycle>) -> SubscriptionId {
        let id = {
            let mut next = self.next_id.lock().unwrap_or_else(|e| e.into_inner());
            *next += 1;
            *next
        };
        let order = subscriber.order();
        let mut list = self.subscribers.write().unwrap_or_else(|e| e.into_inner());
        // Stable: after every subscriber with the same or a lower order.
        let at = list.partition_point(|e| e.order <= order);
        list.insert(
            at,
            Entry {
                id,
                order,
                subscriber,
            },
        );
        SubscriptionId(id)
    }

    /// Remove a subscriber; `false` when it was not subscribed (any more).
    pub fn unsubscribe(&self, id: SubscriptionId) -> bool {
        let mut list = self.subscribers.write().unwrap_or_else(|e| e.into_inner());
        let before = list.len();
        list.retain(|e| e.id != id.0);
        list.len() != before
    }

    /// The open project, if any.
    pub fn current(&self) -> Option<ProjectRef> {
        self.lock_current().project.clone()
    }

    /// The generation of the open project (0 before the first one opened).
    pub fn generation(&self) -> u64 {
        self.lock_current().generation
    }

    /// Ask every subscriber whether the open project may close, in order. Nothing closes
    /// here; an empty list means nobody objects. A subscriber that panics does not object.
    pub fn before_close(&self) -> Vec<Objection> {
        let Some(project) = self.current() else {
            return Vec::new();
        };
        self.snapshot()
            .iter()
            .filter_map(|s| {
                catch_unwind(AssertUnwindSafe(|| s.before_close(&project)))
                    .unwrap_or_else(|_| {
                        tracing::warn!("lifecycle subscriber {} panicked in beforeClose", s.name());
                        None
                    })
                    .map(|mut o| {
                        if o.source.is_empty() {
                            o.source = s.name().to_string();
                        }
                        o
                    })
            })
            .collect()
    }

    /// Close the open project (`closing`); `None` when no project is open.
    pub fn close(&self) -> Option<PhaseReport> {
        let (project, generation) = {
            let mut cur = self.lock_current();
            let project = cur.project.take()?;
            cur.ready = false;
            (project, cur.generation)
        };
        Some(self.run(ProjectPhase::Closing, project, generation))
    }

    /// A project opened in this window: `closing` of the previous one first (when another
    /// one was open), then `opened`. Returns the reports of the phases that ran.
    pub fn open(&self, project: ProjectRef) -> Vec<PhaseReport> {
        let mut reports = Vec::new();
        if let Some(closed) = self.close() {
            reports.push(closed);
        }
        let generation = {
            let mut cur = self.lock_current();
            cur.generation += 1;
            cur.project = Some(project.clone());
            cur.ready = false;
            cur.generation
        };
        reports.push(self.run(ProjectPhase::Opened, project, generation));
        reports
    }

    /// The index of the project opened as `generation` is built: `ready`, once. `None` when
    /// that project is no longer open or was ready already.
    pub fn ready(&self, generation: u64) -> Option<PhaseReport> {
        let project = {
            let mut cur = self.lock_current();
            if cur.generation != generation || cur.ready {
                return None;
            }
            cur.ready = true;
            cur.project.clone()?
        };
        Some(self.run(ProjectPhase::Ready, project, generation))
    }

    /// Update what is known about the open project (trusted later, VCS root found) without
    /// running a phase.
    pub fn update_current(&self, f: impl FnOnce(&mut ProjectRef)) {
        if let Some(p) = self.lock_current().project.as_mut() {
            f(p);
        }
    }

    fn lock_current(&self) -> std::sync::MutexGuard<'_, Current> {
        self.current.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The subscribers in order; phases run on this copy, so a subscriber may subscribe or
    /// unsubscribe while a phase runs.
    fn snapshot(&self) -> Vec<Arc<dyn ProjectLifecycle>> {
        self.subscribers
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|e| e.subscriber.clone())
            .collect()
    }

    fn run(&self, phase: ProjectPhase, project: ProjectRef, generation: u64) -> PhaseReport {
        let mut failures = Vec::new();
        for s in self.snapshot() {
            let result = catch_unwind(AssertUnwindSafe(|| match phase {
                ProjectPhase::Closing => s.closing(&project),
                ProjectPhase::Opened => s.opened(&project),
                ProjectPhase::Ready => s.ready(&project),
                ProjectPhase::BeforeClose => Ok(()),
            }));
            let error = match result {
                Ok(Ok(())) => continue,
                Ok(Err(e)) => e.to_string(),
                Err(panic) => panic
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "panicked".into()),
            };
            tracing::warn!(
                "lifecycle subscriber {} failed in {phase:?}: {error}",
                s.name()
            );
            failures.push((s.name().to_string(), error));
        }
        PhaseReport {
            phase,
            project,
            generation,
            failures,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ForgeError;

    /// Records `<name>:<phase>:<root>` into a shared log.
    struct Recorder {
        name: String,
        order: i32,
        log: Arc<Mutex<Vec<String>>>,
        fail: Option<ProjectPhase>,
        object: bool,
    }

    impl Recorder {
        fn new(name: &str, order: i32, log: &Arc<Mutex<Vec<String>>>) -> Self {
            Recorder {
                name: name.into(),
                order,
                log: log.clone(),
                fail: None,
                object: false,
            }
        }
        fn note(&self, phase: &str, p: &ProjectRef) -> crate::Result<()> {
            self.log
                .lock()
                .unwrap()
                .push(format!("{}:{phase}:{}", self.name, p.root));
            Ok(())
        }
        fn step(&self, phase: ProjectPhase, label: &str, p: &ProjectRef) -> crate::Result<()> {
            self.note(label, p)?;
            if self.fail == Some(phase) {
                if self.name.starts_with("panic") {
                    panic!("boom");
                }
                return Err(ForgeError::Other("broken".into()));
            }
            Ok(())
        }
    }

    impl ProjectLifecycle for Recorder {
        fn name(&self) -> &str {
            &self.name
        }
        fn order(&self) -> i32 {
            self.order
        }
        fn before_close(&self, p: &ProjectRef) -> Option<Objection> {
            let _ = self.note("beforeClose", p);
            self.object.then(|| Objection {
                source: String::new(),
                kind: "test.busy".into(),
                message: "busy".into(),
                items: vec!["sleep".into()],
            })
        }
        fn closing(&self, p: &ProjectRef) -> crate::Result<()> {
            self.step(ProjectPhase::Closing, "closing", p)
        }
        fn opened(&self, p: &ProjectRef) -> crate::Result<()> {
            self.step(ProjectPhase::Opened, "opened", p)
        }
        fn ready(&self, p: &ProjectRef) -> crate::Result<()> {
            self.step(ProjectPhase::Ready, "ready", p)
        }
    }

    fn take(log: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
        std::mem::take(&mut *log.lock().unwrap())
    }

    #[test]
    fn phases_run_in_order_and_close_the_previous_project_first() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let bus = ProjectLifecycleBus::new();
        bus.subscribe(Arc::new(Recorder::new("late", 10, &log)));
        bus.subscribe(Arc::new(Recorder::new("first", -5, &log)));
        bus.subscribe(Arc::new(Recorder::new("middle", 0, &log)));
        bus.subscribe(Arc::new(Recorder::new("middle2", 0, &log)));

        let reports = bus.open(ProjectRef::new("a", "/a"));
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].generation, 1);
        assert_eq!(
            take(&log),
            [
                "first:opened:/a",
                "middle:opened:/a",
                "middle2:opened:/a",
                "late:opened:/a"
            ]
        );

        let reports = bus.open(ProjectRef::new("b", "/b"));
        let phases: Vec<_> = reports
            .iter()
            .map(|r| (r.phase, r.project.root.as_str()))
            .collect();
        assert_eq!(
            phases,
            [(ProjectPhase::Closing, "/a"), (ProjectPhase::Opened, "/b")]
        );
        let log = take(&log);
        assert_eq!(log[0], "first:closing:/a");
        assert_eq!(log[3], "late:closing:/a");
        assert_eq!(log[4], "first:opened:/b");
        assert_eq!(bus.current().unwrap().root, "/b");
        assert_eq!(bus.generation(), 2);
    }

    #[test]
    fn a_failing_or_panicking_subscriber_does_not_stop_the_others() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let bus = ProjectLifecycleBus::new();
        let mut broken = Recorder::new("broken", 0, &log);
        broken.fail = Some(ProjectPhase::Opened);
        let mut panics = Recorder::new("panics", 1, &log);
        panics.fail = Some(ProjectPhase::Opened);
        panics.name = "panic-subscriber".into();
        bus.subscribe(Arc::new(broken));
        bus.subscribe(Arc::new(panics));
        bus.subscribe(Arc::new(Recorder::new("healthy", 2, &log)));

        let report = bus.open(ProjectRef::new("a", "/a")).pop().unwrap();
        assert_eq!(
            take(&log),
            [
                "broken:opened:/a",
                "panic-subscriber:opened:/a",
                "healthy:opened:/a"
            ]
        );
        assert_eq!(
            report.failures,
            [
                ("broken".to_string(), "broken".to_string()),
                ("panic-subscriber".to_string(), "boom".to_string())
            ]
        );
    }

    #[test]
    fn before_close_collects_objections_without_closing() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let bus = ProjectLifecycleBus::new();
        assert!(
            bus.before_close().is_empty(),
            "nothing open, nothing to object to"
        );
        let mut busy = Recorder::new("terminal", 0, &log);
        busy.object = true;
        bus.subscribe(Arc::new(Recorder::new("git", -1, &log)));
        bus.subscribe(Arc::new(busy));
        bus.open(ProjectRef::new("a", "/a"));
        take(&log);

        let objections = bus.before_close();
        assert_eq!(objections.len(), 1);
        assert_eq!(objections[0].source, "terminal");
        assert_eq!(objections[0].kind, "test.busy");
        assert_eq!(
            take(&log),
            ["git:beforeClose:/a", "terminal:beforeClose:/a"]
        );
        // Vetoed: the project is still open and nothing ran `closing`.
        assert_eq!(bus.current().unwrap().root, "/a");
    }

    #[test]
    fn unsubscribed_subscribers_and_stale_ready_are_skipped() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let bus = ProjectLifecycleBus::new();
        let id = bus.subscribe(Arc::new(Recorder::new("gone", 0, &log)));
        bus.subscribe(Arc::new(Recorder::new("stays", 0, &log)));
        bus.open(ProjectRef::new("a", "/a"));
        assert!(bus.unsubscribe(id));
        assert!(!bus.unsubscribe(id));
        take(&log);

        let first = bus.generation();
        bus.open(ProjectRef::new("b", "/b"));
        assert!(
            bus.ready(first).is_none(),
            "the index of /a is no longer wanted"
        );
        let ready = bus.ready(bus.generation()).unwrap();
        assert_eq!(ready.project.root, "/b");
        assert!(bus.ready(bus.generation()).is_none(), "ready runs once");
        assert_eq!(
            take(&log),
            ["stays:closing:/a", "stays:opened:/b", "stays:ready:/b"]
        );

        let closed = bus.close().unwrap();
        assert_eq!(closed.phase, ProjectPhase::Closing);
        assert!(bus.current().is_none());
        assert!(bus.close().is_none());
    }

    #[test]
    fn events_serialize_for_the_ui() {
        let mut project = ProjectRef::new("shop", "/p/shop");
        project.vcs_root = Some("/p/shop".into());
        project.trusted = true;
        project
            .details
            .insert("remote".into(), serde_json::json!({ "host": "h" }));
        let event = LifecycleEvent {
            phase: ProjectPhase::BeforeClose,
            generation: 3,
            project,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "phase": "beforeClose",
                "generation": 3,
                "project": { "name": "shop", "root": "/p/shop", "vcsRoot": "/p/shop", "trusted": true, "remote": { "host": "h" } }
            })
        );
    }
}
