//! A DAP debug session, bridged to Forge's debugger model (`forge_debug`): the same
//! [`SessionInfo`], [`Frame`], [`Property`] and [`DebugEvent`]s as an Xdebug session, so
//! the Debug tool window drives both.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use forge_debug::console::Completion;
use forge_debug::{
    BreakReason, Breakpoint, BreakpointKind, BreakpointSpec, ContextName, DebugEvent,
    ExceptionFilter, ExceptionInfo, Frame, Location, Property, SessionFeatures, SessionInfo,
    SessionStatus, StepKind,
};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::sync::Notify;

use crate::adapter::{AdapterInfo, LaunchRequest, ResolvedLaunch};
use crate::client::{DapClient, Incoming};
use crate::error::{DapError, DapResult};
use crate::manager::Inner;
use crate::protocol::{
    Capabilities, CompletionItem, DataBreakpoint, DataBreakpointInfo, DisassembledInstruction,
    EvaluateResult, Event, ExceptionFilterOptions, FunctionBreakpoint, GotoTarget, Module,
    OutputEvent, ProcessEvent, ReadMemoryResult, Request, RunInTerminalArgs, Scope, Source,
    SourceBreakpoint, StackFrame, StartDebuggingArgs, StoppedEvent, Thread, Variable,
};
use crate::views::{self, Instruction, MemoryBlock};

/// A source file loaded by the debuggee (Loaded Sources).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedSource {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Project-relative path, when the source is in the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

#[derive(Default)]
struct State {
    status: SessionStatus,
    location: Option<Location>,
    reason: Option<BreakReason>,
    exception: Option<ExceptionInfo>,
    stopped_thread: Option<i64>,
    /// The thread whose frames the UI shows.
    current_thread: Option<i64>,
    threads: Vec<Thread>,
    /// Frames of `current_thread`, top first (index = frame level).
    frames: Vec<StackFrame>,
    /// Adapter breakpoint id -> Forge breakpoint id.
    adapter_bps: HashMap<i64, u64>,
    /// Files that have line breakpoints at the adapter.
    bp_files: HashSet<String>,
    /// "Run to Cursor" target, a breakpoint only the adapter knows.
    run_to: Option<(String, u32)>,
    modules: Vec<Module>,
    process: Option<ProcessEvent>,
    has_registers: bool,
    configured: bool,
    ended: bool,
}

pub struct DapSession {
    pub id: String,
    info: AdapterInfo,
    request: LaunchRequest,
    resolved: ResolvedLaunch,
    client: Arc<DapClient>,
    peer: String,
    parent: Option<String>,
    caps: RwLock<Capabilities>,
    state: Mutex<State>,
    initialized: Notify,
    sync_lock: tokio::sync::Mutex<()>,
    child: Mutex<Option<Child>>,
    stderr_tail: Mutex<VecDeque<String>>,
    manager: Weak<Inner>,
    terminate_sent: AtomicBool,
}

const STACK_LEVELS: u32 = 200;
/// How long to wait for an `initialized` event that comes right after `initialize`.
const EARLY_INITIALIZED: Duration = Duration::from_millis(250);

/// DAP `stopped` reason → Forge's break reason.
fn break_reason(reason: &str) -> BreakReason {
    match reason {
        "breakpoint" | "function breakpoint" | "data breakpoint" | "instruction breakpoint" => {
            BreakReason::Breakpoint
        }
        "entry" => BreakReason::FirstLine,
        "exception" => BreakReason::Exception,
        "pause" => BreakReason::Pause,
        _ => BreakReason::Step,
    }
}

/// Count of UTF-16 code units (DAP columns).
fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// The identifier being typed at the end of `text`.
fn typed_prefix(text: &str) -> &str {
    let start = text
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
        .last()
        .map(|(i, _)| i)
        .unwrap_or(text.len());
    &text[start..]
}

/// The logpoint message for an adapter: "Breakpoint reached at …" and/or the expression.
/// Expressions with `{…}` are adapter templates already; others are logged as `expr = {expr}`.
pub fn log_message(bp: &Breakpoint) -> String {
    let mut parts = Vec::new();
    if bp.log_message || (!bp.logs()) {
        parts.push(format!(
            "Breakpoint reached at {}:{}",
            bp.file.as_deref().unwrap_or("?"),
            bp.line.unwrap_or(0)
        ));
    }
    if let Some(expr) = bp.log_expression() {
        if expr.contains('{') {
            parts.push(expr.to_string());
        } else {
            parts.push(format!("{expr} = {{{expr}}}"));
        }
    }
    parts.join(" ")
}

pub(crate) fn variable_property(v: Variable, container: i64) -> Property {
    let children = v.named_variables.unwrap_or(0) + v.indexed_variables.unwrap_or(0);
    Property {
        fullname: v.evaluate_name.clone().unwrap_or_else(|| v.name.clone()),
        name: v.name,
        r#type: "dap".into(),
        classname: v.r#type.filter(|t| !t.is_empty()),
        facet: v
            .presentation_hint
            .and_then(|h| h.visibility.or(h.kind))
            .filter(|f| !f.is_empty()),
        value: Some(v.value),
        has_children: v.variables_reference > 0,
        num_children: children,
        reference: (v.variables_reference > 0).then_some(v.variables_reference),
        container: (container > 0).then_some(container),
        memory_reference: v.memory_reference,
        ..Default::default()
    }
}

fn scope_property(s: &Scope) -> Property {
    Property {
        name: s.name.clone(),
        fullname: String::new(),
        r#type: "scope".into(),
        has_children: s.variables_reference > 0,
        num_children: s.named_variables.unwrap_or(0) + s.indexed_variables.unwrap_or(0),
        reference: (s.variables_reference > 0).then_some(s.variables_reference),
        ..Default::default()
    }
}

impl DapSession {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: String,
        info: AdapterInfo,
        request: LaunchRequest,
        resolved: ResolvedLaunch,
        client: Arc<DapClient>,
        child: Option<Child>,
        peer: String,
        parent: Option<String>,
        manager: Weak<Inner>,
    ) -> Self {
        Self {
            id,
            info,
            request,
            resolved,
            client,
            peer,
            parent,
            caps: RwLock::new(Capabilities::default()),
            state: Mutex::new(State::default()),
            initialized: Notify::new(),
            sync_lock: tokio::sync::Mutex::new(()),
            child: Mutex::new(child),
            stderr_tail: Mutex::new(VecDeque::new()),
            manager,
            terminate_sent: AtomicBool::new(false),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("dap session lock")
    }

    pub fn capabilities(&self) -> Capabilities {
        self.caps.read().expect("caps lock").clone()
    }

    pub fn adapter(&self) -> &AdapterInfo {
        &self.info
    }

    pub fn launch_request(&self) -> &LaunchRequest {
        &self.request
    }

    /// Where the adapter was reached (`host:port` for TCP adapters).
    pub(crate) fn peer(&self) -> &str {
        &self.peer
    }

    pub(crate) fn resolved(&self) -> &ResolvedLaunch {
        &self.resolved
    }

    pub fn status(&self) -> SessionStatus {
        self.state().status
    }

    fn emit(&self, event: DebugEvent) {
        if let Some(m) = self.manager.upgrade() {
            m.emit(event);
        }
    }

    fn emit_changed(&self) {
        self.emit(DebugEvent::Session {
            session: self.info(),
        });
    }

    fn output(&self, stream: &str, text: impl Into<String>) {
        self.emit(DebugEvent::Output {
            session_id: self.id.clone(),
            stream: stream.into(),
            text: text.into(),
        });
    }

    fn root(&self) -> Option<PathBuf> {
        self.manager.upgrade().and_then(|m| m.root())
    }

    /// Project-relative path of an adapter path, if it is in the project.
    fn to_local(&self, path: &str) -> Option<String> {
        let p = Path::new(path);
        match self.root() {
            Some(root) if p.is_absolute() => p
                .strip_prefix(&root)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/")),
            Some(_) => Some(path.replace('\\', "/")),
            None => None,
        }
    }

    /// Absolute path of a project file for the adapter.
    fn to_remote(&self, file: &str) -> String {
        if Path::new(file).is_absolute() {
            return file.to_string();
        }
        match self.root() {
            Some(root) => root.join(file).to_string_lossy().into_owned(),
            None => file.to_string(),
        }
    }

    fn features(&self, st: &State) -> SessionFeatures {
        let c = self.caps.read().expect("caps lock");
        SessionFeatures {
            threads: true,
            read_memory: c.supports_read_memory_request,
            write_memory: c.supports_write_memory_request,
            disassemble: c.supports_disassemble_request,
            registers: st.has_registers || c.supports_disassemble_request,
            data_breakpoints: c.supports_data_breakpoints,
            function_breakpoints: c.supports_function_breakpoints,
            conditional_breakpoints: c.supports_conditional_breakpoints,
            hit_conditional_breakpoints: c.supports_hit_conditional_breakpoints,
            log_points: true,
            instruction_stepping: c.supports_stepping_granularity,
            goto: c.supports_goto_targets_request,
            restart: true,
            completions: true,
            set_variable: c.supports_set_variable || c.supports_set_expression,
            modules: c.supports_modules_request || !st.modules.is_empty(),
            loaded_sources: c.supports_loaded_sources_request,
            attached: self.request.is_attach(),
            exception_filters: c
                .exception_breakpoint_filters
                .iter()
                .map(|f| ExceptionFilter {
                    filter: f.filter.clone(),
                    label: f.label.clone(),
                    description: f.description.clone(),
                    default: f.default,
                    supports_condition: f.supports_condition,
                })
                .collect(),
        }
    }

    pub fn info(&self) -> SessionInfo {
        let st = self.state();
        let program = self.request.program.clone().unwrap_or_default();
        SessionInfo {
            id: self.id.clone(),
            ide_key: String::new(),
            language: self
                .info
                .languages
                .first()
                .cloned()
                .unwrap_or_else(|| self.info.r#type.clone()),
            engine: self.info.label.clone(),
            file: (!program.is_empty())
                .then(|| self.to_local(&program))
                .flatten(),
            file_uri: program,
            peer: self.peer.clone(),
            server_name: self.request.title(),
            host: None,
            port: None,
            server_id: None,
            status: st.status,
            location: st.location.clone(),
            reason: st.reason,
            exception: st.exception.clone(),
            adapter: Some(self.info.r#type.clone()),
            features: Some(self.features(&st)),
            thread_id: st.stopped_thread,
            parent_id: self.parent.clone(),
        }
    }

    /// The debuggee's process id, once the adapter reported it.
    pub fn process_id(&self) -> Option<u32> {
        self.state()
            .process
            .as_ref()
            .and_then(|p| p.system_process_id)
    }

    // ---- start-up ---------------------------------------------------------------------

    pub(crate) fn note_stderr(&self, line: String) {
        tracing::debug!("[{}] adapter: {line}", self.id);
        let mut tail = self.stderr_tail.lock().expect("stderr lock");
        if tail.len() >= 20 {
            tail.pop_front();
        }
        tail.push_back(line);
    }

    fn with_stderr(&self, e: DapError) -> DapError {
        let tail = self.stderr_tail.lock().expect("stderr lock");
        if tail.is_empty() {
            return e;
        }
        let lines: Vec<&str> = tail.iter().map(String::as_str).collect();
        DapError::Other(format!("{e}\n{}", lines.join("\n")))
    }

    /// initialize → launch/attach → (initialized) breakpoints → configurationDone.
    pub(crate) async fn start(self: &Arc<Self>, launch_timeout: Duration) -> DapResult<()> {
        let caps: Capabilities = self
            .client
            .call(
                "initialize",
                &json!({
                    "clientID": "forge",
                    "clientName": "Forge",
                    "adapterID": self.info.r#type,
                    "locale": "en",
                    "linesStartAt1": true,
                    "columnsStartAt1": true,
                    "pathFormat": "path",
                    "supportsVariableType": true,
                    "supportsVariablePaging": false,
                    "supportsRunInTerminalRequest": true,
                    "supportsMemoryReferences": true,
                    "supportsInvalidatedEvent": true,
                    "supportsMemoryEvent": true,
                    "supportsStartDebuggingRequest": true,
                    "supportsArgsCanBeInterpretedByShell": false,
                }),
            )
            .await
            .map_err(|e| self.with_stderr(e))?;
        *self.caps.write().expect("caps lock") = caps.clone();
        self.ensure_exception_filters(&caps);

        // Adapters that announce `initialized` right after `initialize` (GDB) may run the
        // program on `launch`: configure them first. Others send it after `launch`.
        let early = tokio::time::timeout(EARLY_INITIALIZED, self.initialized.notified())
            .await
            .is_ok();
        if early {
            self.sync_breakpoints().await;
        }
        let mut launch = Box::pin(
            self.client
                .send(&self.resolved.request, self.resolved.arguments.clone())
                .await?
                .wait(launch_timeout),
        );
        let mut launched: Option<DapResult<Value>> = None;
        if !early {
            tokio::select! {
                _ = self.initialized.notified() => {}
                r = &mut launch => launched = Some(r),
            }
            if let Some(Err(e)) = launched {
                return Err(self.with_stderr(e));
            }
            if launched.is_some() {
                // Adapters that answer `launch` first still send `initialized` afterwards.
                let _ =
                    tokio::time::timeout(Duration::from_secs(5), self.initialized.notified()).await;
            }
            self.sync_breakpoints().await;
        }
        if caps.supports_configuration_done_request {
            if let Err(e) = self.client.request("configurationDone", Value::Null).await {
                tracing::warn!("[{}] configurationDone: {e}", self.id);
            }
        }
        self.state().configured = true;
        if launched.is_none() {
            launch.await.map_err(|e| self.with_stderr(e))?;
        }
        {
            let mut st = self.state();
            if st.status == SessionStatus::Initializing {
                st.status = SessionStatus::Running;
            }
        }
        self.emit_changed();
        Ok(())
    }

    /// Store the adapter's exception filters as breakpoints (on by their default) so the
    /// Breakpoints dialog lists them and the user's choice persists.
    fn ensure_exception_filters(&self, caps: &Capabilities) {
        let Some(m) = self.manager.upgrade() else {
            return;
        };
        let have = m.breakpoints().snapshot().breakpoints;
        for f in &caps.exception_breakpoint_filters {
            let known = have.iter().any(|b| {
                b.kind == BreakpointKind::Exception
                    && b.adapter.as_deref() == Some(self.info.r#type.as_str())
                    && b.exception.as_deref() == Some(f.filter.as_str())
            });
            if !known {
                let spec = BreakpointSpec {
                    kind: BreakpointKind::Exception,
                    exception: Some(f.filter.clone()),
                    description: Some(f.label.clone()),
                    adapter: Some(self.info.r#type.clone()),
                    enabled: Some(f.default),
                    ..Default::default()
                };
                if let Err(e) = m.breakpoints().add(spec) {
                    tracing::warn!("cannot store exception filter {}: {e}", f.filter);
                }
            }
        }
    }

    pub(crate) fn is_configured(&self) -> bool {
        self.state().configured
    }

    // ---- breakpoints --------------------------------------------------------------------

    /// Send every breakpoint of the project to the adapter.
    pub async fn sync_breakpoints(&self) {
        let _guard = self.sync_lock.lock().await;
        if self.client.is_closed() {
            return;
        }
        let Some(m) = self.manager.upgrade() else {
            return;
        };
        let snapshot = m.breakpoints().snapshot();
        let caps = self.capabilities();
        let muted = snapshot.muted;
        let active = |b: &&Breakpoint| b.enabled && !muted;
        let mut ids: HashMap<i64, u64> = HashMap::new();

        // Line breakpoints, one request per file.
        let mut by_file: BTreeMap<String, Vec<(Option<u64>, SourceBreakpoint)>> = BTreeMap::new();
        for bp in snapshot.breakpoints.iter().filter(active) {
            if bp.kind != BreakpointKind::Line {
                continue;
            }
            let (Some(file), Some(line)) = (&bp.file, bp.line) else {
                continue;
            };
            let logpoint = !bp.suspend && caps.supports_log_points;
            let sbp = SourceBreakpoint {
                line,
                condition: bp
                    .condition()
                    .filter(|_| caps.supports_conditional_breakpoints)
                    .map(str::to_string),
                hit_condition: bp
                    .pass_count()
                    .filter(|_| caps.supports_hit_conditional_breakpoints)
                    .map(|n| n.to_string()),
                log_message: logpoint.then(|| log_message(bp)),
                ..Default::default()
            };
            by_file
                .entry(file.clone())
                .or_default()
                .push((Some(bp.id), sbp));
        }
        let (previous, run_to) = {
            let st = self.state();
            (st.bp_files.clone(), st.run_to.clone())
        };
        if let Some((file, line)) = run_to {
            let list = by_file.entry(file).or_default();
            if !list.iter().any(|(_, b)| b.line == line) {
                list.push((
                    None,
                    SourceBreakpoint {
                        line,
                        ..Default::default()
                    },
                ));
            }
        }
        let mut files: Vec<String> = by_file.keys().cloned().collect();
        files.extend(
            previous
                .iter()
                .filter(|f| !by_file.contains_key(*f))
                .cloned(),
        );
        for file in &files {
            let list = by_file.get(file).cloned().unwrap_or_default();
            let path = self.to_remote(file);
            let name = Path::new(file)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned());
            let args = json!({
                "source": Source { name, path: Some(path), ..Default::default() },
                "breakpoints": list.iter().map(|(_, b)| b).collect::<Vec<_>>(),
                "lines": list.iter().map(|(_, b)| b.line).collect::<Vec<_>>(),
                "sourceModified": false,
            });
            match self.client.request("setBreakpoints", args).await {
                Ok(body) => self.record_ids(&body, list.iter().map(|(id, _)| *id), &mut ids),
                Err(DapError::Closed) => return,
                Err(e) => tracing::warn!("setBreakpoints {file}: {e}"),
            }
        }

        if caps.supports_function_breakpoints {
            let fbps: Vec<&Breakpoint> = snapshot
                .breakpoints
                .iter()
                .filter(active)
                .filter(|b| b.kind == BreakpointKind::Function)
                .collect();
            let args = json!({ "breakpoints": fbps.iter().map(|b| FunctionBreakpoint {
                name: b.function.clone().unwrap_or_default(),
                condition: b.condition().map(str::to_string),
                hit_condition: b.pass_count().map(|n| n.to_string()),
            }).collect::<Vec<_>>() });
            match self.client.request("setFunctionBreakpoints", args).await {
                Ok(body) => self.record_ids(&body, fbps.iter().map(|b| Some(b.id)), &mut ids),
                Err(e) => tracing::warn!("setFunctionBreakpoints: {e}"),
            }
        }

        if !caps.exception_breakpoint_filters.is_empty() {
            let mine: Vec<&Breakpoint> = snapshot
                .breakpoints
                .iter()
                .filter(active)
                .filter(|b| {
                    b.kind == BreakpointKind::Exception
                        && b.adapter.as_deref() == Some(self.info.r#type.as_str())
                })
                .filter(|b| {
                    caps.exception_breakpoint_filters
                        .iter()
                        .any(|f| Some(f.filter.as_str()) == b.exception.as_deref())
                })
                .collect();
            let filters: Vec<&str> = mine.iter().filter_map(|b| b.exception.as_deref()).collect();
            let mut args = json!({ "filters": filters });
            if caps.supports_exception_filter_options {
                args["filterOptions"] = json!(mine
                    .iter()
                    .map(|b| ExceptionFilterOptions {
                        filter_id: b.exception.clone().unwrap_or_default(),
                        condition: b.condition().map(str::to_string),
                    })
                    .collect::<Vec<_>>());
                args["filters"] = json!([]);
            }
            if let Err(e) = self.client.request("setExceptionBreakpoints", args).await {
                tracing::warn!("setExceptionBreakpoints: {e}");
            }
        }

        if caps.supports_data_breakpoints {
            let dbps: Vec<&Breakpoint> = snapshot
                .breakpoints
                .iter()
                .filter(active)
                .filter(|b| b.kind == BreakpointKind::Data)
                .filter(|b| {
                    b.adapter.is_none() || b.adapter.as_deref() == Some(self.info.r#type.as_str())
                })
                .collect();
            let args = json!({ "breakpoints": dbps.iter().map(|b| DataBreakpoint {
                data_id: b.data_id.clone().unwrap_or_default(),
                access_type: b.access_type.clone(),
                condition: b.condition().map(str::to_string),
                hit_condition: b.pass_count().map(|n| n.to_string()),
            }).collect::<Vec<_>>() });
            match self.client.request("setDataBreakpoints", args).await {
                Ok(body) => self.record_ids(&body, dbps.iter().map(|b| Some(b.id)), &mut ids),
                Err(e) => tracing::warn!("setDataBreakpoints: {e}"),
            }
        }

        let mut st = self.state();
        st.adapter_bps = ids;
        st.bp_files = by_file.keys().cloned().collect();
    }

    fn record_ids(
        &self,
        body: &Value,
        local: impl Iterator<Item = Option<u64>>,
        ids: &mut HashMap<i64, u64>,
    ) {
        let answered: Vec<crate::protocol::Breakpoint> = body
            .get("breakpoints")
            .cloned()
            .and_then(|b| serde_json::from_value(b).ok())
            .unwrap_or_default();
        for (local, remote) in local.zip(answered) {
            if let (Some(local), Some(remote)) = (local, remote.id) {
                ids.insert(remote, local);
            }
            if !remote.verified {
                if let Some(msg) = remote.message {
                    tracing::debug!("breakpoint not verified: {msg}");
                }
            }
        }
    }

    /// Watch a variable (`container` + `name`) for changes: ask the adapter for a data id
    /// and store a data breakpoint.
    pub async fn add_data_breakpoint(
        &self,
        container: i64,
        name: &str,
        access_type: Option<String>,
    ) -> DapResult<Breakpoint> {
        if !self.capabilities().supports_data_breakpoints {
            return Err(DapError::Unsupported(format!(
                "{} does not support watchpoints",
                self.info.label
            )));
        }
        let info: DataBreakpointInfo = self
            .client
            .call(
                "dataBreakpointInfo",
                &json!({ "variablesReference": container, "name": name }),
            )
            .await?;
        let Some(data_id) = info.data_id else {
            return Err(DapError::Unsupported(if info.description.is_empty() {
                format!("'{name}' cannot be watched")
            } else {
                info.description
            }));
        };
        let access = access_type
            .or_else(|| info.access_types.as_ref().and_then(|a| a.first().cloned()))
            .unwrap_or_else(|| "write".into());
        let m = self.manager.upgrade().ok_or(DapError::Closed)?;
        m.breakpoints().add(BreakpointSpec {
            kind: BreakpointKind::Data,
            data_id: Some(data_id),
            access_type: Some(access),
            description: Some(if info.description.is_empty() {
                name.to_string()
            } else {
                info.description
            }),
            adapter: Some(self.info.r#type.clone()),
            ..Default::default()
        })
    }

    // ---- events -------------------------------------------------------------------------

    pub(crate) async fn on_incoming(self: &Arc<Self>, message: Incoming) {
        match message {
            Incoming::Event(e) => self.on_event(e).await,
            Incoming::Request(r) => self.on_reverse_request(r).await,
        }
    }

    async fn on_event(self: &Arc<Self>, e: Event) {
        match e.event.as_str() {
            "initialized" => self.initialized.notify_one(),
            "stopped" => {
                let ev: StoppedEvent = serde_json::from_value(e.body).unwrap_or_default();
                self.on_stopped(ev).await;
            }
            "continued" => {
                {
                    let mut st = self.state();
                    if st.ended {
                        return;
                    }
                    st.status = SessionStatus::Running;
                    st.location = None;
                    st.frames.clear();
                }
                self.emit_changed();
            }
            "output" => {
                let ev: OutputEvent = serde_json::from_value(e.body).unwrap_or_default();
                let stream = match ev.category.as_deref() {
                    Some("stderr") => "stderr",
                    Some("console") | Some("important") => "log",
                    Some("telemetry") => return,
                    _ => "stdout",
                };
                self.output(stream, ev.output);
            }
            "terminated" => self.end().await,
            "exited" => {
                let code = e.body.get("exitCode").and_then(Value::as_i64).unwrap_or(0);
                self.output(
                    "system",
                    format!("\nProcess finished with exit code {code}\n"),
                );
            }
            "process" => {
                let ev: ProcessEvent = serde_json::from_value(e.body).unwrap_or_default();
                if let Some(pid) = ev.system_process_id {
                    self.output("system", format!("{} (pid {pid})\n", ev.name));
                }
                self.state().process = Some(ev);
                self.emit_changed();
            }
            "thread" => self.invalidate(&["threads"]),
            "module" => {
                let reason = e.body.get("reason").and_then(Value::as_str).unwrap_or("");
                let module: Option<Module> = e
                    .body
                    .get("module")
                    .cloned()
                    .and_then(|m| serde_json::from_value(m).ok());
                if let Some(module) = module {
                    let mut st = self.state();
                    st.modules.retain(|m| m.id != module.id);
                    if reason != "removed" {
                        st.modules.push(module);
                    }
                }
                self.invalidate(&["modules"]);
            }
            "memory" => self.invalidate(&["memory"]),
            "invalidated" => {
                let areas: Vec<String> = e
                    .body
                    .get("areas")
                    .and_then(|a| serde_json::from_value(a.clone()).ok())
                    .unwrap_or_else(|| vec!["all".into()]);
                let areas: Vec<&str> = areas.iter().map(String::as_str).collect();
                self.invalidate(&areas);
            }
            "capabilities" => {
                if let Some(c) = e.body.get("capabilities") {
                    let mut merged = serde_json::to_value(self.capabilities()).unwrap_or_default();
                    crate::adapter::merge(&mut merged, c);
                    if let Ok(caps) = serde_json::from_value(merged) {
                        *self.caps.write().expect("caps lock") = caps;
                    }
                    self.emit_changed();
                }
            }
            "breakpoint" => {
                let bp: Option<crate::protocol::Breakpoint> = e
                    .body
                    .get("breakpoint")
                    .cloned()
                    .and_then(|b| serde_json::from_value(b).ok());
                if let Some(bp) = bp {
                    tracing::debug!(
                        "[{}] breakpoint {:?} verified={}",
                        self.id,
                        bp.id,
                        bp.verified
                    );
                }
            }
            other => tracing::trace!("[{}] ignoring DAP event {other}", self.id),
        }
    }

    fn invalidate(&self, areas: &[&str]) {
        self.emit(DebugEvent::Invalidated {
            session_id: self.id.clone(),
            areas: areas.iter().map(|a| a.to_string()).collect(),
        });
    }

    async fn first_thread(&self) -> Option<i64> {
        if let Some(t) = self.state().threads.first().map(|t| t.id) {
            return Some(t);
        }
        self.threads().await.ok()?.first().map(|t| t.id)
    }

    async fn on_stopped(self: &Arc<Self>, ev: StoppedEvent) {
        if self.state().ended {
            return;
        }
        let thread = match ev.thread_id {
            Some(t) => Some(t),
            None => {
                let current = self.state().current_thread;
                match current {
                    Some(t) => Some(t),
                    None => self.first_thread().await,
                }
            }
        };
        let _ = self.threads().await;
        let frames = match thread {
            Some(t) => self.fetch_frames(t).await.unwrap_or_default(),
            None => Vec::new(),
        };
        let location = frames.first().and_then(|f| {
            let path = f.source.as_ref()?.path.clone()?;
            Some(Location {
                file: self.to_local(&path),
                file_uri: path,
                line: f.line,
            })
        });
        let reason = break_reason(&ev.reason);
        let exception = (reason == BreakReason::Exception).then(|| ExceptionInfo {
            name: ev
                .description
                .clone()
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| "Exception".into()),
            message: ev.text.clone(),
        });

        // Forge's breakpoints that were hit.
        let hits: Vec<Breakpoint> = {
            let st = self.state();
            let snapshot = self
                .manager
                .upgrade()
                .map(|m| m.breakpoints().snapshot())
                .unwrap_or_default();
            let mut ids: Vec<u64> = ev
                .hit_breakpoint_ids
                .clone()
                .unwrap_or_default()
                .iter()
                .filter_map(|id| st.adapter_bps.get(id).copied())
                .collect();
            if ids.is_empty() && reason == BreakReason::Breakpoint {
                if let Some(loc) = &location {
                    ids.extend(
                        snapshot
                            .breakpoints
                            .iter()
                            .filter(|b| {
                                b.kind == BreakpointKind::Line
                                    && b.enabled
                                    && b.file.is_some()
                                    && b.file == loc.file
                                    && b.line == Some(loc.line)
                            })
                            .map(|b| b.id),
                    );
                }
            }
            snapshot
                .breakpoints
                .into_iter()
                .filter(|b| ids.contains(&b.id))
                .collect()
        };
        {
            let mut st = self.state();
            st.status = SessionStatus::Paused;
            st.stopped_thread = thread;
            st.current_thread = thread;
            st.frames = frames;
            st.location = location.clone();
            st.reason = Some(reason);
            st.exception = exception;
        }
        if self.state().run_to.take().is_some() {
            let me = Arc::clone(self);
            tokio::spawn(async move { me.sync_breakpoints().await });
        }
        let adapter_logs = self.capabilities().supports_log_points;
        let mut resume = !hits.is_empty();
        for bp in &hits {
            let emulated_logpoint = !bp.suspend && !adapter_logs;
            if bp.logs() && (bp.suspend || emulated_logpoint) {
                self.log_hit(bp).await;
            }
            if bp.temporary {
                if let Some(m) = self.manager.upgrade() {
                    m.breakpoints().remove(bp.id);
                }
            }
            resume &= emulated_logpoint;
        }
        if resume {
            // A logpoint the adapter could not handle: log and run on.
            let _ = self.continue_with(StepKind::Run).await;
            return;
        }
        self.emit_changed();
    }

    /// Log a breakpoint hit to the console ("Breakpoint reached", stack, expression).
    async fn log_hit(&self, bp: &Breakpoint) {
        let loc = self.state().location.clone();
        let where_ = loc
            .as_ref()
            .map(|l| format!("{}:{}", l.file.as_deref().unwrap_or(&l.file_uri), l.line))
            .unwrap_or_default();
        if bp.log_message {
            self.output("log", format!("Breakpoint reached at {where_}\n"));
        }
        if bp.log_stack {
            let frames = self.state().frames.clone();
            let mut text = String::new();
            for f in frames.iter().take(30) {
                text.push_str(&format!(
                    "\t{}:{}, {}\n",
                    f.source
                        .as_ref()
                        .and_then(|s| s.name.clone())
                        .unwrap_or_default(),
                    f.line,
                    f.name
                ));
            }
            self.output("log", text);
        }
        if let Some(expr) = bp.log_expression() {
            let text = match self.evaluate_raw(expr, 0, "watch").await {
                Ok(r) => format!("{expr} = {}\n", r.result),
                Err(e) => format!("{expr}: {e}\n"),
            };
            self.output("log", text);
        }
    }

    async fn on_reverse_request(self: &Arc<Self>, r: Request) {
        match r.command.as_str() {
            "runInTerminal" => {
                let args: RunInTerminalArgs =
                    serde_json::from_value(r.arguments.clone()).unwrap_or_default();
                let result = self.run_in_terminal(&args);
                let (ok, body, msg) = match result {
                    Ok(pid) => (true, json!({ "processId": pid }), None),
                    Err(e) => (false, Value::Null, Some(e.to_string())),
                };
                let _ = self.client.respond(&r, ok, body, msg).await;
            }
            "startDebugging" => {
                let args: StartDebuggingArgs =
                    serde_json::from_value(r.arguments.clone()).unwrap_or_default();
                let _ = self.client.respond(&r, true, Value::Null, None).await;
                if let Some(m) = self.manager.upgrade() {
                    let parent = Arc::clone(self);
                    tokio::spawn(async move {
                        if let Err(e) = Inner::start_child(&m, &parent, args).await {
                            parent.output("stderr", format!("Cannot start child session: {e}\n"));
                        }
                    });
                }
            }
            other => {
                let _ = self
                    .client
                    .respond(
                        &r,
                        false,
                        Value::Null,
                        Some(format!("'{other}' is not supported")),
                    )
                    .await;
            }
        }
    }

    /// `runInTerminal`: start the debuggee; its output goes to the debug console.
    fn run_in_terminal(&self, args: &RunInTerminalArgs) -> DapResult<Option<u32>> {
        let Some((program, rest)) = args.args.split_first() else {
            return Err(DapError::Other("runInTerminal without a command".into()));
        };
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(rest)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        if !args.cwd.is_empty() {
            cmd.current_dir(&args.cwd);
        }
        for (k, v) in args.env.iter().flatten() {
            match v {
                Value::String(v) => {
                    cmd.env(k, v);
                }
                Value::Null => {
                    cmd.env_remove(k);
                }
                other => {
                    cmd.env(k, other.to_string());
                }
            }
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| DapError::Other(format!("Cannot start {program}: {e}")))?;
        let pid = child.id();
        for (stream, pipe) in [
            (
                "stdout",
                child
                    .stdout
                    .take()
                    .map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Send + Unpin>),
            ),
            (
                "stderr",
                child
                    .stderr
                    .take()
                    .map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Send + Unpin>),
            ),
        ] {
            let Some(pipe) = pipe else { continue };
            let manager = self.manager.clone();
            let id = self.id.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(pipe).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some(m) = manager.upgrade() {
                        m.emit(DebugEvent::Output {
                            session_id: id.clone(),
                            stream: stream.into(),
                            text: format!("{line}\n"),
                        });
                    }
                }
            });
        }
        tokio::spawn(async move {
            let _ = child.wait().await;
        });
        Ok(pid)
    }

    /// The adapter went away (or the session was stopped): tell the UI once.
    pub(crate) async fn end(&self) {
        {
            let mut st = self.state();
            if st.ended {
                return;
            }
            st.ended = true;
            st.status = SessionStatus::Stopped;
        }
        let manager = self.manager.upgrade();
        if let Some(m) = &manager {
            m.remove_session(&self.id);
        }
        if !self.client.is_closed() {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                self.client.request(
                    "disconnect",
                    json!({ "terminateDebuggee": !self.request.is_attach() }),
                ),
            )
            .await;
            self.client.shutdown().await;
        }
        let child = self.child.lock().expect("child lock").take();
        if let Some(mut child) = child {
            tokio::spawn(async move {
                if tokio::time::timeout(Duration::from_secs(2), child.wait())
                    .await
                    .is_err()
                {
                    let _ = child.kill().await;
                }
            });
        }
        if let Some(m) = manager {
            m.emit(DebugEvent::Ended {
                session_id: self.id.clone(),
            });
        }
    }

    // ---- execution ----------------------------------------------------------------------

    fn require_paused(&self) -> DapResult<()> {
        if self.state().status == SessionStatus::Paused {
            Ok(())
        } else {
            Err(DapError::Other(
                "The program is running; this needs a suspended program".into(),
            ))
        }
    }

    async fn thread_for_command(&self) -> DapResult<i64> {
        let t = {
            let st = self.state();
            st.current_thread.or(st.stopped_thread)
        };
        match t {
            Some(t) => Ok(t),
            None => self
                .first_thread()
                .await
                .ok_or_else(|| DapError::Other("The program has no threads".into())),
        }
    }

    fn set_running(&self) {
        {
            let mut st = self.state();
            st.status = SessionStatus::Running;
            st.location = None;
            st.reason = None;
            st.exception = None;
            st.frames.clear();
        }
        self.emit_changed();
    }

    /// Resume, Step Over/Into/Out, and instruction steps.
    pub async fn continue_with(&self, kind: StepKind) -> DapResult<()> {
        let thread = self.thread_for_command().await?;
        let (command, instruction) = match kind {
            StepKind::Run => ("continue", false),
            StepKind::StepOver => ("next", false),
            StepKind::StepInto | StepKind::ForceStepInto => ("stepIn", false),
            StepKind::StepOut => ("stepOut", false),
            StepKind::StepIntoInstruction => ("stepIn", true),
            StepKind::StepOverInstruction => ("next", true),
        };
        let mut args = json!({ "threadId": thread });
        if instruction {
            if !self.capabilities().supports_stepping_granularity {
                return Err(DapError::Unsupported(format!(
                    "{} cannot step by instruction",
                    self.info.label
                )));
            }
            args["granularity"] = json!("instruction");
        }
        let previous = self.state().status;
        self.set_running();
        if let Err(e) = self.client.request(command, args).await {
            if previous == SessionStatus::Paused {
                self.state().status = SessionStatus::Paused;
                self.emit_changed();
            }
            return Err(e);
        }
        Ok(())
    }

    pub async fn pause(&self) -> DapResult<()> {
        let thread = self.thread_for_command().await?;
        self.client
            .request("pause", json!({ "threadId": thread }))
            .await
            .map(|_| ())
    }

    /// Stop: terminate a launched program (detach from an attached one) and end.
    pub async fn stop(&self) {
        let caps = self.capabilities();
        if !self.request.is_attach()
            && caps.supports_terminate_request
            && !self.terminate_sent.swap(true, Ordering::SeqCst)
        {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                self.client.request("terminate", json!({})),
            )
            .await;
        }
        self.end().await;
    }

    /// Detach, leaving the program running.
    pub async fn detach(&self) {
        let _ = tokio::time::timeout(
            Duration::from_secs(2),
            self.client
                .request("disconnect", json!({ "terminateDebuggee": false })),
        )
        .await;
        self.client.shutdown().await;
        self.end().await;
    }

    /// Restart in place when the adapter can; `false` means the caller starts a new
    /// session.
    pub async fn restart_in_place(&self) -> DapResult<bool> {
        if !self.capabilities().supports_restart_request {
            return Ok(false);
        }
        self.set_running();
        self.client
            .request("restart", json!({ "arguments": self.resolved.arguments }))
            .await?;
        Ok(true)
    }

    /// Run to Cursor: a breakpoint only the adapter knows, removed on the next stop.
    pub async fn run_to(&self, file: &str, line: u32) -> DapResult<()> {
        self.require_paused()?;
        self.state().run_to = Some((file.to_string(), line));
        self.sync_breakpoints().await;
        self.continue_with(StepKind::Run).await
    }

    /// Set Execution Point (`gotoTargets` + `goto`).
    pub async fn goto(&self, file: &str, line: u32) -> DapResult<()> {
        self.require_paused()?;
        if !self.capabilities().supports_goto_targets_request {
            return Err(DapError::Unsupported(format!(
                "{} cannot jump to another line",
                self.info.label
            )));
        }
        let body = self
            .client
            .request(
                "gotoTargets",
                json!({ "source": { "path": self.to_remote(file) }, "line": line }),
            )
            .await?;
        let targets: Vec<GotoTarget> = body
            .get("targets")
            .cloned()
            .and_then(|t| serde_json::from_value(t).ok())
            .unwrap_or_default();
        let target = targets
            .first()
            .ok_or_else(|| DapError::Other(format!("Cannot jump to line {line}")))?;
        let thread = self.thread_for_command().await?;
        self.set_running();
        self.client
            .request("goto", json!({ "threadId": thread, "targetId": target.id }))
            .await
            .map(|_| ())
    }

    // ---- inspection ---------------------------------------------------------------------

    pub async fn threads(&self) -> DapResult<Vec<Thread>> {
        let body = self.client.request("threads", Value::Null).await?;
        let threads: Vec<Thread> = body
            .get("threads")
            .cloned()
            .and_then(|t| serde_json::from_value(t).ok())
            .unwrap_or_default();
        self.state().threads = threads.clone();
        Ok(threads)
    }

    async fn fetch_frames(&self, thread: i64) -> DapResult<Vec<StackFrame>> {
        let body = self
            .client
            .request(
                "stackTrace",
                json!({ "threadId": thread, "startFrame": 0, "levels": STACK_LEVELS }),
            )
            .await?;
        Ok(body
            .get("stackFrames")
            .cloned()
            .and_then(|f| serde_json::from_value(f).ok())
            .unwrap_or_default())
    }

    fn frame_of(&self, f: &StackFrame, level: u32) -> Frame {
        let path = f.source.as_ref().and_then(|s| s.path.clone());
        Frame {
            level,
            file: path.as_deref().and_then(|p| self.to_local(p)),
            file_uri: path
                .or_else(|| f.source.as_ref().and_then(|s| s.name.clone()))
                .unwrap_or_default(),
            line: f.line,
            r#where: f.name.clone(),
            id: Some(f.id),
            address: f.instruction_pointer_reference.clone(),
        }
    }

    /// Frames of `thread` (default: the current thread, which it becomes).
    pub async fn stack(&self, thread: Option<i64>) -> DapResult<Vec<Frame>> {
        self.require_paused()?;
        let current = self.state().current_thread;
        let frames = match thread {
            Some(t) if Some(t) != current || self.state().frames.is_empty() => {
                let frames = self.fetch_frames(t).await?;
                let mut st = self.state();
                st.current_thread = Some(t);
                st.frames = frames.clone();
                frames
            }
            _ => {
                let cached = self.state().frames.clone();
                if cached.is_empty() {
                    let t = self.thread_for_command().await?;
                    let frames = self.fetch_frames(t).await?;
                    self.state().frames = frames.clone();
                    frames
                } else {
                    cached
                }
            }
        };
        Ok(frames
            .iter()
            .enumerate()
            .map(|(i, f)| self.frame_of(f, i as u32))
            .collect())
    }

    fn frame_id(&self, depth: u32) -> Option<i64> {
        self.state().frames.get(depth as usize).map(|f| f.id)
    }

    async fn scopes(&self, depth: u32) -> DapResult<Vec<Scope>> {
        self.require_paused()?;
        let frame = self
            .frame_id(depth)
            .ok_or_else(|| DapError::NotFound(format!("no frame {depth}")))?;
        let body = self
            .client
            .request("scopes", json!({ "frameId": frame }))
            .await?;
        let scopes: Vec<Scope> = body
            .get("scopes")
            .cloned()
            .and_then(|s| serde_json::from_value(s).ok())
            .unwrap_or_default();
        if scopes.iter().any(Scope::is_registers) && !self.state().has_registers {
            self.state().has_registers = true;
            self.emit_changed();
        }
        Ok(scopes)
    }

    pub async fn contexts(&self, depth: u32) -> DapResult<Vec<ContextName>> {
        Ok(self
            .scopes(depth)
            .await?
            .iter()
            .enumerate()
            .map(|(i, s)| ContextName {
                id: i as u32,
                name: s.name.clone(),
            })
            .collect())
    }

    /// Children of a variables reference.
    pub async fn children(&self, reference: i64) -> DapResult<Vec<Property>> {
        let body = self
            .client
            .request("variables", json!({ "variablesReference": reference }))
            .await?;
        let vars: Vec<Variable> = body
            .get("variables")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        Ok(vars
            .into_iter()
            .map(|v| variable_property(v, reference))
            .collect())
    }

    /// Variables of a frame. Context 0 is the first scope's variables followed by the
    /// other scopes as expandable groups (registers excluded); context n is scope n.
    pub async fn variables(&self, depth: u32, context: u32) -> DapResult<Vec<Property>> {
        let scopes = self.scopes(depth).await?;
        if context > 0 {
            let scope = scopes
                .get(context as usize)
                .ok_or_else(|| DapError::NotFound(format!("no scope {context}")))?;
            return self.children(scope.variables_reference).await;
        }
        let visible: Vec<&Scope> = scopes.iter().filter(|s| !s.is_registers()).collect();
        let Some(first) = visible.first() else {
            return Ok(Vec::new());
        };
        let mut out = self.children(first.variables_reference).await?;
        out.extend(visible[1..].iter().map(|s| scope_property(s)));
        Ok(out)
    }

    /// The registers of a frame (Registers view): the registers scope's variables.
    pub async fn registers(&self, depth: u32) -> DapResult<Vec<Property>> {
        let scopes = self.scopes(depth).await?;
        match scopes.iter().find(|s| s.is_registers()) {
            Some(s) => self.children(s.variables_reference).await,
            None => Ok(Vec::new()),
        }
    }

    async fn evaluate_raw(
        &self,
        expression: &str,
        depth: u32,
        context: &str,
    ) -> DapResult<EvaluateResult> {
        let mut args = json!({ "expression": expression, "context": context });
        if self.state().status == SessionStatus::Paused {
            if let Some(frame) = self.frame_id(depth) {
                args["frameId"] = json!(frame);
            }
        }
        self.client.call("evaluate", &args).await
    }

    /// Evaluate in a frame. `context`: `watch`, `repl`, `hover` or `clipboard`.
    pub async fn eval(&self, expression: &str, depth: u32, context: &str) -> DapResult<Property> {
        let r = self.evaluate_raw(expression, depth, context).await?;
        let children = r.named_variables.unwrap_or(0) + r.indexed_variables.unwrap_or(0);
        Ok(Property {
            name: expression.to_string(),
            fullname: expression.to_string(),
            r#type: "dap".into(),
            classname: r.r#type.filter(|t| !t.is_empty()),
            value: Some(r.result),
            has_children: r.variables_reference > 0,
            num_children: children,
            reference: (r.variables_reference > 0).then_some(r.variables_reference),
            memory_reference: r.memory_reference,
            ..Default::default()
        })
    }

    /// Completion of `text` (caret at its end). Without adapter support, the frame's
    /// variables are offered.
    pub async fn complete(&self, text: &str, depth: u32) -> DapResult<Vec<Completion>> {
        self.require_paused()?;
        let prefix = typed_prefix(text);
        let prefix_start = text.len() - prefix.len();
        if !self.capabilities().supports_completions_request {
            let lower = prefix.to_lowercase();
            let vars = self.variables(depth, 0).await.unwrap_or_default();
            return Ok(vars
                .into_iter()
                .filter(|v| v.r#type != "scope" && v.name.to_lowercase().starts_with(&lower))
                .map(|v| Completion {
                    insert: v.name.clone(),
                    detail: v.classname.unwrap_or_default(),
                    label: v.name,
                })
                .collect());
        }
        let mut args = json!({ "text": text, "column": utf16_len(text) + 1 });
        if let Some(frame) = self.frame_id(depth) {
            args["frameId"] = json!(frame);
        }
        let body = self.client.request("completions", args).await?;
        let items: Vec<CompletionItem> = body
            .get("targets")
            .cloned()
            .and_then(|t| serde_json::from_value(t).ok())
            .unwrap_or_default();
        let utf16_prefix_start = utf16_len(&text[..prefix_start]);
        Ok(items
            .into_iter()
            .map(|c| {
                let new_text = c.text.clone().unwrap_or_else(|| c.label.clone());
                // `start` is 1-based (columnsStartAt1); keep text between the typed prefix
                // and the replaced range.
                let insert = match c.start.map(|s| s.saturating_sub(1) as usize) {
                    Some(s) if s > utf16_prefix_start && s <= utf16_len(text) => {
                        let kept: String = text[prefix_start..]
                            .encode_utf16()
                            .take(s - utf16_prefix_start)
                            .map(|u| char::from_u32(u as u32).unwrap_or('?'))
                            .collect();
                        format!("{kept}{new_text}")
                    }
                    _ => new_text,
                };
                Completion {
                    label: c.label,
                    insert,
                    detail: c.detail.or(c.r#type).unwrap_or_default(),
                }
            })
            .collect())
    }

    /// Set Value: `setVariable` for a variable of `container`, else `setExpression`.
    pub async fn set_value(
        &self,
        container: Option<i64>,
        name: Option<&str>,
        expression: &str,
        value: &str,
        depth: u32,
    ) -> DapResult<()> {
        self.require_paused()?;
        let caps = self.capabilities();
        if let (Some(container), Some(name), true) = (container, name, caps.supports_set_variable) {
            self.client
                .request(
                    "setVariable",
                    json!({ "variablesReference": container, "name": name, "value": value }),
                )
                .await?;
        } else if caps.supports_set_expression {
            let mut args = json!({ "expression": expression, "value": value });
            if let Some(frame) = self.frame_id(depth) {
                args["frameId"] = json!(frame);
            }
            self.client.request("setExpression", args).await?;
        } else {
            return Err(DapError::Unsupported(format!(
                "{} cannot change values",
                self.info.label
            )));
        }
        self.invalidate(&["memory"]);
        Ok(())
    }

    // ---- native views -------------------------------------------------------------------

    /// `readMemory` as raw bytes: the address of the first byte, the bytes and how many
    /// bytes after them could not be read (CForge's Peripherals view decodes registers).
    pub async fn read_bytes(
        &self,
        memory_reference: &str,
        offset: i64,
        count: u32,
    ) -> DapResult<(u64, Vec<u8>, u32)> {
        if !self.capabilities().supports_read_memory_request {
            return Err(DapError::Unsupported(format!(
                "{} cannot read memory",
                self.info.label
            )));
        }
        let r: ReadMemoryResult = self
            .client
            .call(
                "readMemory",
                &json!({ "memoryReference": memory_reference, "offset": offset, "count": count }),
            )
            .await?;
        let bytes = match r.data.as_deref() {
            Some(d) => B64
                .decode(d.as_bytes())
                .map_err(|e| DapError::Protocol(format!("readMemory data: {e}")))?,
            None => Vec::new(),
        };
        let start = views::parse_address(&r.address).unwrap_or(0);
        Ok((start, bytes, r.unreadable_bytes.unwrap_or(0)))
    }

    pub async fn read_memory(
        &self,
        memory_reference: &str,
        offset: i64,
        count: u32,
        per_row: usize,
    ) -> DapResult<MemoryBlock> {
        let (start, bytes, unreadable) = self.read_bytes(memory_reference, offset, count).await?;
        Ok(MemoryBlock {
            address: views::format_address(start),
            rows: views::memory_rows(start, &bytes, unreadable as usize, per_row),
            unreadable,
        })
    }

    pub async fn write_memory(
        &self,
        memory_reference: &str,
        offset: i64,
        hex: &str,
    ) -> DapResult<()> {
        if !self.capabilities().supports_write_memory_request {
            return Err(DapError::Unsupported(format!(
                "{} cannot write memory",
                self.info.label
            )));
        }
        let bytes = views::parse_hex_bytes(hex)?;
        self.client
            .request(
                "writeMemory",
                json!({ "memoryReference": memory_reference, "offset": offset, "data": B64.encode(bytes) }),
            )
            .await?;
        self.invalidate(&["memory", "variables"]);
        Ok(())
    }

    /// Instructions around `memory_reference` (`instruction_offset` may be negative to
    /// show instructions before it).
    pub async fn disassemble(
        &self,
        memory_reference: &str,
        instruction_offset: i64,
        count: u32,
        current: Option<&str>,
    ) -> DapResult<Vec<Instruction>> {
        if !self.capabilities().supports_disassemble_request {
            return Err(DapError::Unsupported(format!(
                "{} cannot disassemble",
                self.info.label
            )));
        }
        let body = self
            .client
            .request(
                "disassemble",
                json!({
                    "memoryReference": memory_reference,
                    "instructionOffset": instruction_offset,
                    "instructionCount": count,
                    "resolveSymbols": true,
                }),
            )
            .await?;
        let list: Vec<DisassembledInstruction> = body
            .get("instructions")
            .cloned()
            .and_then(|i| serde_json::from_value(i).ok())
            .unwrap_or_default();
        let current = current.and_then(views::parse_address);
        let mut file: Option<String> = None;
        Ok(list
            .into_iter()
            .map(|i| {
                if let Some(path) = i.location.as_ref().and_then(|l| l.path.as_deref()) {
                    file = self.to_local(path).or_else(|| Some(path.to_string()));
                }
                let address = views::parse_address(&i.address);
                Instruction {
                    current: current.is_some() && address == current,
                    address: address.map(views::format_address).unwrap_or(i.address),
                    bytes: i.instruction_bytes,
                    instruction: i.instruction,
                    symbol: i.symbol,
                    file: i.line.and(file.clone()),
                    line: i.line,
                }
            })
            .collect())
    }

    pub async fn modules(&self) -> DapResult<Vec<Module>> {
        if self.capabilities().supports_modules_request {
            let body = self
                .client
                .request("modules", json!({ "startModule": 0, "moduleCount": 0 }))
                .await?;
            let modules: Vec<Module> = body
                .get("modules")
                .cloned()
                .and_then(|m| serde_json::from_value(m).ok())
                .unwrap_or_default();
            self.state().modules = modules.clone();
            return Ok(modules);
        }
        Ok(self.state().modules.clone())
    }

    pub async fn loaded_sources(&self) -> DapResult<Vec<LoadedSource>> {
        if !self.capabilities().supports_loaded_sources_request {
            return Ok(Vec::new());
        }
        let body = self.client.request("loadedSources", json!({})).await?;
        let sources: Vec<Source> = body
            .get("sources")
            .cloned()
            .and_then(|s| serde_json::from_value(s).ok())
            .unwrap_or_default();
        Ok(sources
            .into_iter()
            .map(|s| LoadedSource {
                file: s.path.as_deref().and_then(|p| self.to_local(p)),
                name: s
                    .name
                    .or_else(|| s.path.clone())
                    .unwrap_or_else(|| "<unknown>".into()),
                path: s.path,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logpoint_messages() {
        let mut bp = Breakpoint {
            id: 1,
            kind: BreakpointKind::Line,
            file: Some("src/demo.c".into()),
            line: Some(17),
            exception: None,
            condition: None,
            enabled: true,
            suspend: false,
            log_message: true,
            log_stack: false,
            log_expression: Some("counter".into()),
            temporary: false,
            pass_count: None,
            function: None,
            data_id: None,
            access_type: None,
            description: None,
            adapter: None,
        };
        assert_eq!(
            log_message(&bp),
            "Breakpoint reached at src/demo.c:17 counter = {counter}"
        );
        bp.log_message = false;
        bp.log_expression = Some("n={n} sum={sum}".into());
        assert_eq!(log_message(&bp), "n={n} sum={sum}");
        bp.log_expression = None;
        assert_eq!(log_message(&bp), "Breakpoint reached at src/demo.c:17");
    }

    #[test]
    fn prefixes_and_reasons() {
        assert_eq!(typed_prefix("origin.x + cou"), "cou");
        assert_eq!(typed_prefix("a."), "");
        assert_eq!(break_reason("data breakpoint"), BreakReason::Breakpoint);
        assert_eq!(break_reason("entry"), BreakReason::FirstLine);
        assert_eq!(break_reason("goto"), BreakReason::Step);
    }

    #[test]
    fn variables_map_to_properties() {
        let v: Variable = serde_json::from_value(json!({
            "name": "origin", "value": "{x=3, y=4}", "type": "struct point",
            "variablesReference": 7, "namedVariables": 2, "evaluateName": "origin",
            "memoryReference": "0x1020"
        }))
        .unwrap();
        let p = variable_property(v, 3);
        assert_eq!(p.r#type, "dap");
        assert_eq!(p.classname.as_deref(), Some("struct point"));
        assert!(p.has_children);
        assert_eq!(
            (p.reference, p.container, p.num_children),
            (Some(7), Some(3), 2)
        );
        assert_eq!(p.memory_reference.as_deref(), Some("0x1020"));
    }
}
