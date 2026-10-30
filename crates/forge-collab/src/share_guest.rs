//! A guest's side of shared terminals and runs (S5): the host's terminals and runs this guest
//! may see, their output kept since they became visible, and the editor-facing handles.
//!
//! The guest's UI attaches to a terminal or run when it shows it: it gets what happened so
//! far, then live output through the sink it attached with. Typing goes to the host, which
//! decides whether the guest may; the guest's UI only disables input for usability.
//!
//! This file is a child module of `guest` (see the `#[path]` declaration there), so it can use
//! the session's private state.

use std::collections::BTreeMap;
use std::path::PathBuf;

use forge_backend::Terminal;
use forge_process::{PtyEvent, PtySink};

use super::*;
use crate::guest_backend::calls::HostCall;
use crate::share::{RunId, RunLog, Scrollback, SharedRun, SharedTerminal, TerminalId};

/// Receives the rendered events (JSON text) of a run the UI attached to.
pub type RunEventSink = Arc<dyn Fn(String) + Send + Sync>;

struct GuestTerminal {
    info: SharedTerminal,
    output: Scrollback,
    sink: Option<PtySink>,
}

struct GuestRun {
    info: SharedRun,
    log: RunLog,
    sink: Option<RunEventSink>,
}

#[derive(Default)]
pub(crate) struct GuestShare {
    terminals: Mutex<BTreeMap<TerminalId, GuestTerminal>>,
    runs: Mutex<BTreeMap<RunId, GuestRun>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().expect("collab share lock poisoned")
}

const CALL_TIMEOUT: Duration = Duration::from_secs(30);

impl GuestSession {
    /// Handle the terminal and run messages; `true` if `msg` was one of them.
    pub(crate) fn dispatch_share_message(&self, msg: &HostMessage, sink: &GuestSink) -> bool {
        match msg {
            HostMessage::Terminals { terminals } => {
                let mut ended = Vec::new();
                {
                    let mut map = lock(&self.share.terminals);
                    // A terminal the host stopped sharing is not an exit: the UI keeps its tab
                    // as ended and says why (it sees the terminal leave the list).
                    map.retain(|id, _| terminals.iter().any(|x| x.id == *id));
                    for info in terminals {
                        match map.get_mut(&info.id) {
                            Some(t) => {
                                if t.info.running && !info.running {
                                    ended.push((t.sink.clone(), info.exit_code));
                                }
                                t.info = info.clone();
                            }
                            None => {
                                map.insert(
                                    info.id,
                                    GuestTerminal {
                                        info: info.clone(),
                                        output: Scrollback::new(),
                                        sink: None,
                                    },
                                );
                            }
                        }
                    }
                }
                for (s, code) in ended {
                    if let Some(s) = s {
                        s(PtyEvent::Exited { code });
                    }
                }
                sink(GuestEvent::Changed);
                true
            }
            HostMessage::TerminalOutput {
                terminal,
                data,
                snapshot,
            } => {
                let live = {
                    let mut map = lock(&self.share.terminals);
                    let Some(t) = map.get_mut(terminal) else {
                        return true;
                    };
                    let new = if *snapshot {
                        // A snapshot comes when the terminal becomes visible and again after a
                        // reconnect: an attached UI gets only what it has not shown yet.
                        let shown = std::mem::replace(&mut t.output, Scrollback::new());
                        snapshot_tail(shown.text(), data)
                    } else {
                        data.clone()
                    };
                    t.output.push(data);
                    t.sink.clone().filter(|_| !new.is_empty()).map(|s| (s, new))
                };
                if let Some((s, data)) = live {
                    s(PtyEvent::Output { data });
                }
                true
            }
            HostMessage::Runs { runs } => {
                {
                    let mut map = lock(&self.share.runs);
                    map.retain(|id, _| runs.iter().any(|x| x.id == *id));
                    for info in runs {
                        map.entry(info.id)
                            .and_modify(|r| r.info = info.clone())
                            .or_insert_with(|| GuestRun {
                                info: info.clone(),
                                log: RunLog::new(),
                                sink: None,
                            });
                    }
                }
                sink(GuestEvent::Changed);
                true
            }
            HostMessage::RunEvents {
                run,
                events,
                snapshot,
            } => {
                let live = {
                    let mut map = lock(&self.share.runs);
                    let Some(r) = map.get_mut(run) else {
                        return true;
                    };
                    let skip = if *snapshot {
                        // Again after a reconnect: an attached UI gets the events it missed.
                        let shown: Vec<String> = std::mem::replace(&mut r.log, RunLog::new())
                            .events()
                            .map(str::to_string)
                            .collect();
                        let continues = events.len() >= shown.len()
                            && events.iter().zip(&shown).all(|(a, b)| a == b);
                        if continues {
                            shown.len()
                        } else {
                            events.len()
                        }
                    } else {
                        0
                    };
                    for e in events {
                        let console = e.contains("\"type\":\"console\"");
                        r.log.push(e.clone(), console);
                    }
                    r.sink.clone().map(|s| (s, skip))
                };
                if let Some((s, skip)) = live {
                    for e in events.iter().skip(skip) {
                        s(e.clone());
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// The session is over: attached terminals end, nothing is shown any more.
    pub(crate) fn end_shares(&self) {
        let sinks: Vec<PtySink> = std::mem::take(&mut *lock(&self.share.terminals))
            .into_values()
            .filter_map(|t| t.sink.filter(|_| t.info.running))
            .collect();
        lock(&self.share.runs).clear();
        for s in sinks {
            s(PtyEvent::Exited { code: None });
        }
    }

    /// The host's terminals this guest sees.
    pub fn terminals(&self) -> Vec<SharedTerminal> {
        lock(&self.share.terminals)
            .values()
            .map(|t| t.info.clone())
            .collect()
    }

    /// The host's runs this guest sees.
    pub fn runs(&self) -> Vec<SharedRun> {
        lock(&self.share.runs)
            .values()
            .map(|r| r.info.clone())
            .collect()
    }

    /// Show a shared terminal: its output so far, then live output and its exit through
    /// `sink`. The returned [`Terminal`] sends keys to the host; resizing is the host's;
    /// killing it detaches (and closes the terminal on the host when it is this guest's own).
    pub fn attach_terminal(
        self: &Arc<Self>,
        terminal: TerminalId,
        sink: PtySink,
    ) -> Result<(String, SharedTerminal, Arc<dyn Terminal>), String> {
        let me = self.me();
        let mut map = lock(&self.share.terminals);
        let t = map
            .get_mut(&terminal)
            .ok_or("The terminal is no longer shared.")?;
        t.sink = Some(sink);
        let proxy = Arc::new(GuestTerminalHandle {
            session: self.clone(),
            terminal,
            own: t.info.owner == me,
        });
        Ok((t.output.text().to_string(), t.info.clone(), proxy))
    }

    /// Stop showing a terminal (its tab closed).
    pub fn detach_terminal(&self, terminal: TerminalId) {
        if let Some(t) = lock(&self.share.terminals).get_mut(&terminal) {
            t.sink = None;
        }
    }

    /// Keys for a shared terminal; the host refuses them without Terminal Full access.
    pub fn terminal_input(&self, terminal: TerminalId, data: &str) {
        self.send(GuestMessage::TerminalInput {
            terminal,
            data: data.to_string(),
        });
    }

    /// Open a terminal on the host (Terminal Full access) and attach to it. Blocks until the
    /// host answered; call it off the async runtime (it is the guest's `open_terminal`).
    pub fn open_terminal_blocking(
        self: &Arc<Self>,
        cols: u16,
        rows: u16,
        sink: PtySink,
    ) -> Result<Arc<dyn Terminal>, String> {
        let bytes = self.call_blocking(&HostCall::TerminalOpen { cols, rows }, CALL_TIMEOUT)?;
        let info: SharedTerminal =
            forge_backend::protocol::decode_value(&bytes).map_err(|e| e.to_string())?;
        let (output, _, proxy) = self.attach_terminal(info.id, sink.clone())?;
        if !output.is_empty() {
            sink(PtyEvent::Output { data: output });
        }
        Ok(proxy)
    }

    /// Close one of this guest's terminals on the host.
    fn close_own_terminal(&self, terminal: TerminalId) {
        self.detach_terminal(terminal);
        // Nobody waits for the answer; the terminal list follows.
        let _ = self.start_call(&HostCall::TerminalClose { terminal }, CALL_TIMEOUT);
    }

    /// Show a run: its events so far, then live events through `sink`.
    pub fn attach_run(&self, run: RunId, sink: RunEventSink) -> Result<Vec<String>, String> {
        let mut map = lock(&self.share.runs);
        let r = map.get_mut(&run).ok_or("The run is no longer shown.")?;
        r.sink = Some(sink);
        Ok(r.log.events().map(str::to_string).collect())
    }

    /// Stop showing a run (its tab closed).
    pub fn detach_run(&self, run: RunId) {
        if let Some(r) = lock(&self.share.runs).get_mut(&run) {
            r.sink = None;
        }
    }

    /// Run one of the host's configurations by name (Run Full access).
    pub async fn start_run(&self, configuration: &str) -> Result<(), String> {
        self.call(
            &HostCall::RunStart {
                configuration: configuration.to_string(),
            },
            Duration::from_secs(120),
        )
        .await
        .map(|_| ())
    }

    /// Stop one of the host's runs (Run Full access).
    pub async fn stop_run(&self, run: RunId) -> Result<(), String> {
        self.call(&HostCall::RunStop { run }, CALL_TIMEOUT)
            .await
            .map(|_| ())
    }

    /// Console input for one of the host's runs (Run Full access).
    pub fn run_input(&self, run: RunId, data: &str) {
        self.send(GuestMessage::RunInput {
            run,
            data: data.to_string(),
        });
    }
}

/// What a terminal that showed `shown` has to write to show `snapshot`: the new part when
/// the snapshot continues it, else a reset and the whole snapshot (the host's scrollback
/// dropped old lines meanwhile).
fn snapshot_tail(shown: &str, snapshot: &str) -> String {
    if shown.is_empty() {
        snapshot.to_string()
    } else if let Some(rest) = snapshot.strip_prefix(shown) {
        rest.to_string()
    } else {
        format!("\x1bc{snapshot}")
    }
}

/// A shared terminal as the guest's terminal tool window uses it.
struct GuestTerminalHandle {
    session: Arc<GuestSession>,
    terminal: TerminalId,
    own: bool,
}

impl Terminal for GuestTerminalHandle {
    fn shell(&self) -> String {
        "Code With Me".into()
    }

    fn cwd(&self) -> PathBuf {
        PathBuf::new()
    }

    fn write(&self, data: &[u8]) -> forge_core::Result<()> {
        self.session
            .terminal_input(self.terminal, &String::from_utf8_lossy(data));
        Ok(())
    }

    /// The host's size wins.
    fn resize(&self, _cols: u16, _rows: u16) -> forge_core::Result<()> {
        Ok(())
    }

    fn kill(&self) -> forge_core::Result<()> {
        if self.own {
            self.session.close_own_terminal(self.terminal);
        } else {
            self.session.detach_terminal(self.terminal);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::snapshot_tail;

    #[test]
    fn snapshots_after_a_reconnect_add_only_the_missed_output() {
        assert_eq!(snapshot_tail("", "$ ls\r\n"), "$ ls\r\n");
        assert_eq!(snapshot_tail("$ ls\r\n", "$ ls\r\na b\r\n"), "a b\r\n");
        assert_eq!(snapshot_tail("$ ls\r\n", "$ ls\r\n"), "");
        assert_eq!(snapshot_tail("old\r\n", "new\r\n"), "\x1bcnew\r\n");
    }
}
