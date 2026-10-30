//! The host's side of shared terminals and runs (S5).
//!
//! This file is a child module of `host` (see the `#[path]` declaration there), so it can use
//! the session's private state. Lock order: the session state first, then the share state.
//!
//! What is enforced here, whatever the guest's client sends:
//! - a guest sees terminals only with Terminal Read-only or Full access and runs only with Run
//!   Read-only or Full access; everything it was not allowed to see is never sent;
//! - keys reach a terminal only with Terminal Full access and only for a terminal that is
//!   shared and running; console input and Stop only with Run Full access;
//! - a guest starts runs only by configuration name, resolved by the host application;
//! - a guest closes only its own terminals; losing Terminal Full access, leaving or being
//!   removed closes them;
//! - every action lands in the session log, the keys never do.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::*;
use crate::audit::{RunAction, TerminalAction};
use crate::share::{
    controls_runs, map_root, run_event_kind, sees_runs, sees_terminals, uses_terminals,
    RunEventKind, RunId, RunLog, Scrollback, SharedRun, SharedTerminal, TerminalId,
};

/// Terminals one guest may have open on the host at once.
const MAX_GUEST_TERMINALS: usize = 8;

struct HostTerminal {
    /// The terminal's id in the host application.
    key: String,
    info: SharedTerminal,
    scrollback: Scrollback,
}

struct HostRun {
    /// The run's id in the host application.
    key: String,
    info: SharedRun,
    log: RunLog,
}

#[derive(Default)]
pub(super) struct ShareState {
    terminals: BTreeMap<TerminalId, HostTerminal>,
    terminal_keys: HashMap<String, TerminalId>,
    runs: BTreeMap<RunId, HostRun>,
    run_keys: HashMap<String, RunId>,
    next: u64,
    /// What each guest has received a snapshot of (sent once, when it becomes visible).
    seen_terminals: HashMap<ParticipantId, BTreeSet<TerminalId>>,
    seen_runs: HashMap<ParticipantId, BTreeSet<RunId>>,
    /// "Typed in the terminal / console" is logged once per guest and terminal or run.
    typed: HashSet<(ParticipantId, u64, bool)>,
    /// Refused input is logged and answered once per guest and kind, not per key.
    refused_input: HashSet<(ParticipantId, &'static str)>,
    /// Run requests of guests until the host's run starts: configuration -> guest.
    starting: HashMap<String, ParticipantId>,
}

impl ShareState {
    fn next_id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }
}

/// Send `guest` the terminals and runs it may see now, with the scrollback or run events of
/// each one it has not seen yet.
fn sync_guest(share: &mut ShareState, id: ParticipantId, g: &Guest, out: &mut Outbox) {
    let p = g.participant.permissions;
    let terminals: Vec<SharedTerminal> = if sees_terminals(&p) {
        share.terminals.values().map(|t| t.info.clone()).collect()
    } else {
        Vec::new()
    };
    out.guests.push((
        g.tx.clone(),
        HostMessage::Terminals {
            terminals: terminals.clone(),
        },
    ));
    let seen = share.seen_terminals.entry(id).or_default();
    for t in &terminals {
        if seen.insert(t.id) {
            out.guests.push((
                g.tx.clone(),
                HostMessage::TerminalOutput {
                    terminal: t.id,
                    data: share.terminals[&t.id].scrollback.text().to_string(),
                    snapshot: true,
                },
            ));
        }
    }
    seen.retain(|t| terminals.iter().any(|x| x.id == *t));

    let runs: Vec<SharedRun> = if sees_runs(&p) {
        share.runs.values().map(|r| r.info.clone()).collect()
    } else {
        Vec::new()
    };
    out.guests
        .push((g.tx.clone(), HostMessage::Runs { runs: runs.clone() }));
    let seen = share.seen_runs.entry(id).or_default();
    for r in &runs {
        if seen.insert(r.id) {
            out.guests.push((
                g.tx.clone(),
                HostMessage::RunEvents {
                    run: r.id,
                    events: share.runs[&r.id].log.events().map(str::to_string).collect(),
                    snapshot: true,
                },
            ));
        }
    }
    seen.retain(|r| runs.iter().any(|x| x.id == *r));
}

/// `text` as it appears inside a JSON string (backslashes of Windows paths are escaped).
fn json_inner(text: &str) -> String {
    let quoted = serde_json::to_string(text).unwrap_or_default();
    quoted
        .strip_prefix('"')
        .and_then(|q| q.strip_suffix('"'))
        .unwrap_or_default()
        .to_string()
}

impl HostSession {
    fn share(&self) -> MutexGuard<'_, ShareState> {
        self.share.lock().expect("collab share lock poisoned")
    }

    /// Send every guest what it may see of the terminals and runs; the host's view changed.
    pub(super) fn sync_shares(&self) {
        let mut out = Outbox::default();
        {
            let st = self.state();
            let mut share = self.share();
            for (id, g) in &st.guests {
                sync_guest(&mut share, *id, g, &mut out);
            }
        }
        out.events.push(HostEvent::Changed);
        self.deliver(out);
    }

    /// A guest joined: what it may see of the terminals and runs (called with both locks
    /// free of the guest's own entry, from `serve`).
    pub(super) fn sync_new_guest(&self, st: &State, id: ParticipantId, out: &mut Outbox) {
        if let Some(g) = st.guests.get(&id) {
            sync_guest(&mut self.share(), id, g, out);
        }
    }

    /// After a permission change: guests that lost Terminal Full access lose their own
    /// terminals; everyone gets what they may see now.
    pub(super) fn permissions_applied(&self, ids: &[ParticipantId]) {
        let lost: Vec<ParticipantId> = {
            let st = self.state();
            ids.iter()
                .copied()
                .filter(|id| {
                    st.guests
                        .get(id)
                        .is_some_and(|g| !uses_terminals(&g.participant.permissions))
                })
                .collect()
        };
        for id in lost {
            self.close_terminals_of(id);
        }
        self.sync_shares();
    }

    /// A guest left or was removed: its terminals close and its bookkeeping goes.
    pub(super) fn guest_gone(&self, id: ParticipantId) {
        self.close_terminals_of(id);
        {
            let mut share = self.share();
            share.seen_terminals.remove(&id);
            share.seen_runs.remove(&id);
            share.typed.retain(|(p, _, _)| *p != id);
            share.refused_input.retain(|(p, _)| *p != id);
            share.starting.retain(|_, p| *p != id);
        }
        self.sync_shares();
    }

    /// The session ends: the guests' terminals close.
    pub(super) fn end_shares(&self) {
        let keys: Vec<String> = {
            let mut share = self.share();
            let keys = share
                .terminals
                .values()
                .filter(|t| t.info.owner != ParticipantId::HOST)
                .map(|t| t.key.clone())
                .collect();
            *share = ShareState::default();
            keys
        };
        if let Some(services) = self.services() {
            for key in keys {
                services.terminal_close(&key);
            }
        }
    }

    fn close_terminals_of(&self, id: ParticipantId) {
        let closed: Vec<HostTerminal> = {
            let mut share = self.share();
            let ids: Vec<TerminalId> = share
                .terminals
                .values()
                .filter(|t| t.info.owner == id)
                .map(|t| t.info.id)
                .collect();
            ids.into_iter()
                .filter_map(|t| {
                    let removed = share.terminals.remove(&t)?;
                    share.terminal_keys.remove(&removed.key);
                    Some(removed)
                })
                .collect()
        };
        if closed.is_empty() {
            return;
        }
        if let Some(services) = self.services() {
            for t in &closed {
                services.terminal_close(&t.key);
            }
        }
    }

    /// Whether a refused terminal or console input should be logged and answered: only the
    /// first refusal per guest and kind, so a client that keeps sending keys does not flood
    /// the log.
    pub(super) fn report_refused_input(&self, from: ParticipantId, msg: &GuestMessage) -> bool {
        match msg {
            GuestMessage::TerminalInput { .. } | GuestMessage::RunInput { .. } => {
                self.share().refused_input.insert((from, msg.name()))
            }
            _ => true,
        }
    }

    // ---- terminals of the host application -------------------------------------------

    /// Share With Guests on a terminal tab of the host (or stop sharing it). `scrollback`
    /// is what the terminal showed so far. Returns `false` for a guest's terminal, which
    /// cannot be unshared.
    pub fn share_terminal(
        &self,
        key: &str,
        title: &str,
        cols: u16,
        rows: u16,
        scrollback: &str,
        shared: bool,
    ) -> bool {
        let changed = {
            let mut share = self.share();
            match (share.terminal_keys.get(key).copied(), shared) {
                (Some(id), true) => {
                    let t = share.terminals.get_mut(&id).expect("indexed");
                    t.info.title = title.to_string();
                    false
                }
                (None, true) => {
                    let id = TerminalId(share.next_id());
                    let mut buffer = Scrollback::new();
                    buffer.push(scrollback);
                    share.terminals.insert(
                        id,
                        HostTerminal {
                            key: key.to_string(),
                            info: SharedTerminal {
                                id,
                                title: title.to_string(),
                                owner: ParticipantId::HOST,
                                cols,
                                rows,
                                running: true,
                                exit_code: None,
                            },
                            scrollback: buffer,
                        },
                    );
                    share.terminal_keys.insert(key.to_string(), id);
                    true
                }
                (Some(id), false) => {
                    if share.terminals[&id].info.owner != ParticipantId::HOST {
                        return false;
                    }
                    share.terminals.remove(&id);
                    share.terminal_keys.remove(key);
                    true
                }
                (None, false) => false,
            }
        };
        if changed {
            self.audit(AuditEvent::Terminal {
                participant: None,
                name: self.host.name.clone(),
                action: if shared {
                    TerminalAction::Shared
                } else {
                    TerminalAction::Unshared
                },
                terminal: title.to_string(),
            });
        }
        self.sync_shares();
        true
    }

    /// The terminals guests see, with their ids in the host application.
    pub fn shared_terminals(&self) -> Vec<(String, SharedTerminal)> {
        self.share()
            .terminals
            .values()
            .map(|t| (t.key.clone(), t.info.clone()))
            .collect()
    }

    /// Output of a terminal of the host application; passed on if it is shared.
    pub fn terminal_output(&self, key: &str, data: &str) {
        let mut out = Outbox::default();
        {
            let st = self.state();
            let mut share = self.share();
            let Some(id) = share.terminal_keys.get(key).copied() else {
                return;
            };
            if let Some(t) = share.terminals.get_mut(&id) {
                t.scrollback.push(data);
            }
            for (gid, g) in &st.guests {
                let seen = share
                    .seen_terminals
                    .get(gid)
                    .is_some_and(|s| s.contains(&id));
                if seen && sees_terminals(&g.participant.permissions) {
                    out.guests.push((
                        g.tx.clone(),
                        HostMessage::TerminalOutput {
                            terminal: id,
                            data: data.to_string(),
                            snapshot: false,
                        },
                    ));
                }
            }
        }
        self.deliver(out);
    }

    /// The host resized a terminal; guests render at the host's size.
    pub fn terminal_resized(&self, key: &str, cols: u16, rows: u16) {
        let changed = {
            let mut share = self.share();
            let Some(id) = share.terminal_keys.get(key).copied() else {
                return;
            };
            let t = &mut share.terminals.get_mut(&id).expect("indexed").info;
            let changed = (t.cols, t.rows) != (cols, rows);
            t.cols = cols;
            t.rows = rows;
            changed
        };
        if changed {
            self.sync_shares();
        }
    }

    /// The shell of a terminal exited.
    pub fn terminal_exited(&self, key: &str, code: Option<i32>) {
        {
            let mut share = self.share();
            let Some(id) = share.terminal_keys.get(key).copied() else {
                return;
            };
            let t = &mut share.terminals.get_mut(&id).expect("indexed").info;
            t.running = false;
            t.exit_code = code;
        }
        self.sync_shares();
    }

    /// The host closed a terminal tab: it is no longer shared.
    pub fn terminal_closed(&self, key: &str) {
        {
            let mut share = self.share();
            let Some(id) = share.terminal_keys.remove(key) else {
                return;
            };
            share.terminals.remove(&id);
        }
        self.sync_shares();
    }

    /// A guest opens a terminal on the host ("<guest>: Local").
    pub(super) async fn open_guest_terminal(
        &self,
        from: ParticipantId,
        cols: u16,
        rows: u16,
    ) -> Result<SharedTerminal, String> {
        let services = self
            .services()
            .ok_or("Terminals are not available in this session.")?;
        let name = self.guest_name(from);
        let title = format!("{name}: Local");
        let (info, key) = {
            let mut share = self.share();
            let own = share
                .terminals
                .values()
                .filter(|t| t.info.owner == from)
                .count();
            if own >= MAX_GUEST_TERMINALS {
                return Err(format!(
                    "You can have at most {MAX_GUEST_TERMINALS} terminals open on the host."
                ));
            }
            let id = TerminalId(share.next_id());
            let key = format!("cwm-{}-terminal-{}", &self.session_hex()[..8], id.0);
            let info = SharedTerminal {
                id,
                title: title.clone(),
                owner: from,
                cols,
                rows,
                running: true,
                exit_code: None,
            };
            // Registered before the shell starts, so its first output (the prompt) is kept.
            share.terminals.insert(
                id,
                HostTerminal {
                    key: key.clone(),
                    info: info.clone(),
                    scrollback: Scrollback::new(),
                },
            );
            share.terminal_keys.insert(key.clone(), id);
            (info, key)
        };
        self.sync_shares();
        match services
            .terminal_open(key.clone(), title.clone(), cols, rows)
            .await
        {
            Ok(()) => {
                self.audit(AuditEvent::Terminal {
                    participant: Some(from),
                    name,
                    action: TerminalAction::Opened,
                    terminal: title,
                });
                Ok(info)
            }
            Err(e) => {
                self.terminal_closed(&key);
                Err(e)
            }
        }
    }

    /// A guest closes one of its own terminals.
    pub(super) fn close_guest_terminal(
        &self,
        from: ParticipantId,
        terminal: TerminalId,
    ) -> Result<(), String> {
        let t = {
            let mut share = self.share();
            match share.terminals.get(&terminal) {
                Some(t) if t.info.owner == from => {}
                Some(_) => return Err("You can close only your own terminals.".into()),
                None => return Ok(()),
            }
            let t = share.terminals.remove(&terminal).expect("checked");
            share.terminal_keys.remove(&t.key);
            t
        };
        if let Some(services) = self.services() {
            services.terminal_close(&t.key);
        }
        self.audit(AuditEvent::Terminal {
            participant: Some(from),
            name: self.guest_name(from),
            action: TerminalAction::Closed,
            terminal: t.info.title,
        });
        self.sync_shares();
        Ok(())
    }

    /// Keys a guest typed (its policy allowed Terminal Full access already).
    pub(super) fn terminal_input(
        &self,
        from: ParticipantId,
        terminal: TerminalId,
        data: &str,
    ) -> Result<(), String> {
        let (key, title, first) = {
            let st = self.state();
            let mut share = self.share();
            let visible = st
                .guests
                .get(&from)
                .is_some_and(|g| uses_terminals(&g.participant.permissions))
                && share
                    .seen_terminals
                    .get(&from)
                    .is_some_and(|s| s.contains(&terminal));
            let t = share
                .terminals
                .get(&terminal)
                .filter(|t| visible && t.info.running)
                .ok_or("The terminal is no longer shared.")?;
            let (key, title) = (t.key.clone(), t.info.title.clone());
            let first = share.typed.insert((from, terminal.0, true));
            (key, title, first)
        };
        self.services()
            .ok_or("Terminals are not available in this session.")?
            .terminal_write(&key, data)?;
        if first {
            self.audit(AuditEvent::Terminal {
                participant: Some(from),
                name: self.guest_name(from),
                action: TerminalAction::Typed,
                terminal: title,
            });
        }
        Ok(())
    }

    // ---- runs of the host application ------------------------------------------------

    /// A run of the host's Run tool window started (`key`: its id in the host application,
    /// `title`: the configuration's name). Every run is shared with guests who see runs.
    pub fn run_started(&self, key: &str, title: &str, kind: Option<&str>) {
        {
            let mut share = self.share();
            if share.run_keys.contains_key(key) {
                return;
            }
            let started_by = share.starting.remove(title).unwrap_or(ParticipantId::HOST);
            let id = RunId(share.next_id());
            share.runs.insert(
                id,
                HostRun {
                    key: key.to_string(),
                    info: SharedRun {
                        id,
                        title: title.to_string(),
                        kind: kind.map(str::to_string),
                        started_by,
                        running: true,
                        exit_code: None,
                    },
                    log: RunLog::new(),
                },
            );
            share.run_keys.insert(key.to_string(), id);
        }
        self.sync_shares();
    }

    /// A rendered event of a run (`run://<key>` JSON).
    pub fn run_event(&self, key: &str, event: &str) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(event) else {
            return;
        };
        let kind = run_event_kind(&value);
        if kind == RunEventKind::Raw {
            return;
        }
        let root = self.backend.info().root.to_string_lossy().into_owned();
        let guest_root = virtual_root(&self.backend.info().name)
            .to_string_lossy()
            .into_owned();
        let event = map_root(event, &json_inner(&root), &json_inner(&guest_root));
        let mut out = Outbox::default();
        let exited = {
            let st = self.state();
            let mut share = self.share();
            let Some(id) = share.run_keys.get(key).copied() else {
                return;
            };
            let run = share.runs.get_mut(&id).expect("indexed");
            run.log.push(event.clone(), kind == RunEventKind::Console);
            let exited = match kind {
                RunEventKind::Exited(code) => {
                    run.info.running = false;
                    run.info.exit_code = code;
                    true
                }
                _ => false,
            };
            for (gid, g) in &st.guests {
                let seen = share.seen_runs.get(gid).is_some_and(|s| s.contains(&id));
                if seen && sees_runs(&g.participant.permissions) {
                    out.guests.push((
                        g.tx.clone(),
                        HostMessage::RunEvents {
                            run: id,
                            events: vec![event.clone()],
                            snapshot: false,
                        },
                    ));
                }
            }
            exited
        };
        self.deliver(out);
        if exited {
            self.sync_shares();
        }
    }

    /// The host closed a run's tab.
    pub fn run_disposed(&self, key: &str) {
        {
            let mut share = self.share();
            let Some(id) = share.run_keys.remove(key) else {
                return;
            };
            share.runs.remove(&id);
        }
        self.sync_shares();
    }

    /// A guest asks to run one of the host's configurations.
    pub(super) async fn start_run_for(
        &self,
        from: ParticipantId,
        configuration: String,
    ) -> Result<(), String> {
        let services = self
            .services()
            .ok_or("Running is not available in this session.")?;
        let name = self.guest_name(from);
        self.share().starting.insert(configuration.clone(), from);
        match services
            .run_start(configuration.clone(), name.clone())
            .await
        {
            Ok(()) => {
                self.audit(AuditEvent::Run {
                    participant: from,
                    name,
                    action: RunAction::Started,
                    run: configuration,
                });
                Ok(())
            }
            Err(e) => {
                let mut share = self.share();
                if share.starting.get(&configuration) == Some(&from) {
                    share.starting.remove(&configuration);
                }
                Err(e)
            }
        }
    }

    fn visible_run(&self, from: ParticipantId, run: RunId) -> Result<(String, String), String> {
        let st = self.state();
        let share = self.share();
        let allowed = st
            .guests
            .get(&from)
            .is_some_and(|g| controls_runs(&g.participant.permissions));
        share
            .runs
            .get(&run)
            .filter(|_| allowed)
            .map(|r| (r.key.clone(), r.info.title.clone()))
            .ok_or_else(|| "The run is no longer shown.".to_string())
    }

    /// A guest stops one of the host's runs.
    pub(super) fn stop_run_for(&self, from: ParticipantId, run: RunId) -> Result<(), String> {
        let (key, title) = self.visible_run(from, run)?;
        self.services()
            .ok_or("Running is not available in this session.")?
            .run_stop(&key)?;
        self.audit(AuditEvent::Run {
            participant: from,
            name: self.guest_name(from),
            action: RunAction::Stopped,
            run: title,
        });
        Ok(())
    }

    /// Console input of a guest (its policy allowed Run Full access already).
    pub(super) fn run_input(
        &self,
        from: ParticipantId,
        run: RunId,
        data: &str,
    ) -> Result<(), String> {
        let (key, title) = self.visible_run(from, run)?;
        self.services()
            .ok_or("Running is not available in this session.")?
            .run_write(&key, data)?;
        if self.share().typed.insert((from, run.0, false)) {
            self.audit(AuditEvent::Run {
                participant: from,
                name: self.guest_name(from),
                action: RunAction::Typed,
                run: title,
            });
        }
        Ok(())
    }
}
