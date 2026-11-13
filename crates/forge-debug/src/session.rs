//! A debug session: one script run connected through DBGp.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::breakpoints::{Breakpoint, BreakpointKind};
use crate::connection::Connection;
use crate::error::{DebugError, DebugResult};
use crate::events::{BreakReason, DebugEvent, ExceptionInfo, Location, SessionInfo, SessionStatus};
use crate::mapping::PathMapper;
use crate::protocol::{
    b64_decode, check_error, parse_breakpoints, parse_context_names, parse_properties,
    parse_property, parse_stack, ContextName, EngineStatus, InitPacket, Property, RemoteBreakpoint,
    StatusResponse,
};
use crate::server::Shared;
use crate::servers::{find_server, parse_ide_config, ServerConfig, ServerIdentity};
use crate::xml::Element;

/// Continuation commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepKind {
    Run,
    StepInto,
    /// Step into even where "skip" rules would step over (same DBGp command).
    ForceStepInto,
    StepOver,
    StepOut,
    /// One machine instruction (DAP `steppingGranularity: "instruction"`); DBGp steps a line.
    StepIntoInstruction,
    StepOverInstruction,
}

impl StepKind {
    fn command(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::StepInto | Self::ForceStepInto | Self::StepIntoInstruction => "step_into",
            Self::StepOver | Self::StepOverInstruction => "step_over",
            Self::StepOut => "step_out",
        }
    }
}

/// A stack frame with its file mapped into the project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub level: u32,
    pub file: Option<String>,
    pub file_uri: String,
    pub line: u32,
    pub r#where: String,
    /// DAP: the adapter's frame id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    /// DAP: the instruction pointer (Disassembly view).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
}

struct State {
    status: SessionStatus,
    mapper: PathMapper,
    identity: ServerIdentity,
    server_id: Option<String>,
    location: Option<Location>,
    reason: Option<BreakReason>,
    exception: Option<ExceptionInfo>,
    /// Local breakpoint id → engine breakpoint id.
    remote_bps: HashMap<u64, String>,
    last_step: Option<StepKind>,
    first_step: bool,
    auto_skips: u32,
    ended: bool,
}

pub struct Session {
    pub id: String,
    pub init: InitPacket,
    pub peer: String,
    conn: Connection,
    shared: Arc<Shared>,
    state: Mutex<State>,
}

const SHORT: Duration = Duration::from_millis(1500);
/// Upper bound for automatic step-outs of unmapped files in a row.
const MAX_AUTO_SKIPS: u32 = 32;

impl Session {
    pub(crate) fn new(
        id: String,
        init: InitPacket,
        peer: String,
        conn: Connection,
        shared: Arc<Shared>,
    ) -> Self {
        let mapper = PathMapper::local(shared.root().unwrap_or_default());
        Self {
            id,
            init,
            peer,
            conn,
            shared,
            state: Mutex::new(State {
                status: SessionStatus::Initializing,
                mapper,
                identity: ServerIdentity::default(),
                server_id: None,
                location: None,
                reason: None,
                exception: None,
                remote_bps: HashMap::new(),
                last_step: None,
                first_step: false,
                auto_skips: 0,
                ended: false,
            }),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("session lock poisoned")
    }

    pub(crate) fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn status(&self) -> SessionStatus {
        self.state().status
    }

    pub fn info(&self) -> SessionInfo {
        let st = self.state();
        SessionInfo {
            id: self.id.clone(),
            ide_key: self.init.ide_key.clone(),
            language: self.init.language.clone(),
            engine: format!("{} {}", self.init.engine_name, self.init.engine_version)
                .trim()
                .to_string(),
            file_uri: self.init.file_uri.clone(),
            file: st.mapper.to_local(&self.init.file_uri),
            peer: self.peer.clone(),
            server_name: st.identity.display_name(),
            host: st.identity.host.clone(),
            port: st.identity.port,
            server_id: st.server_id.clone(),
            status: st.status,
            location: st.location.clone(),
            reason: st.reason,
            exception: st.exception.clone(),
            ..Default::default()
        }
    }

    fn emit_changed(&self) {
        self.shared.emit(DebugEvent::Session {
            session: self.info(),
        });
    }

    fn mapper(&self) -> PathMapper {
        self.state().mapper.clone()
    }

    // ---- setup ------------------------------------------------------------------------

    async fn server_var(&self, expr: &str) -> Option<String> {
        let el = self
            .conn
            .send(
                "property_get",
                &[('c', "1".into()), ('n', expr.into())],
                None,
            )
            .await
            .ok()?;
        parse_property(&el).ok()?.value.filter(|v| !v.is_empty())
    }

    /// Ask the engine which server the request belongs to.
    async fn identify(&self) -> ServerIdentity {
        let ide_config = self.server_var("$_SERVER['PHP_IDE_CONFIG']").await;
        let host = self.server_var("$_SERVER['SERVER_NAME']").await;
        let port = match host {
            Some(_) => self
                .server_var("$_SERVER['SERVER_PORT']")
                .await
                .and_then(|p| p.trim().parse().ok()),
            None => None,
        };
        ServerIdentity {
            server_name: ide_config.as_deref().and_then(parse_ide_config),
            host,
            port,
        }
    }

    /// Identify the server, pick path mappings and either start debugging or ask the user.
    pub(crate) async fn begin(self: &Arc<Self>) {
        let identity = self.identify().await;
        let Some(root) = self.shared.root() else {
            tracing::info!("no project open; detaching debug connection {}", self.id);
            self.detach().await;
            return;
        };
        let servers = self.shared.known_servers();
        // CLI scripts carry no server name: fall back to a server whose mappings cover the
        // script.
        let server = find_server(&servers, &identity)
            .or_else(|| {
                servers.iter().find(|s| {
                    s.use_path_mappings
                        && s.mapper(&root).to_local(&self.init.file_uri).is_some()
                        && identity.host.is_none()
                        && identity.server_name.is_none()
                })
            })
            .cloned();
        let local = crate::mapping::PathMapper::local(&root);
        {
            let mut st = self.state();
            st.identity = identity;
            if let Some(server) = &server {
                st.mapper = server.mapper(&root);
                st.server_id = Some(server.id.clone());
            } else {
                st.mapper = local.clone();
            }
        }
        if server.is_some() || local.to_local(&self.init.file_uri).is_some() {
            self.start().await;
            return;
        }
        if self.shared.config().ignore_external_connections {
            tracing::info!("ignoring connection from unregistered server");
            self.detach().await;
            return;
        }
        self.state().status = SessionStatus::AwaitingAcceptance;
        self.shared.emit(DebugEvent::Incoming {
            session: self.info(),
        });
    }

    /// Continue an unknown connection with a (new) server configuration.
    pub async fn accept(self: &Arc<Self>, server: &ServerConfig) -> DebugResult<()> {
        let root = self
            .shared
            .root()
            .ok_or_else(|| DebugError::Other("no project is open".into()))?;
        {
            let mut st = self.state();
            if st.status != SessionStatus::AwaitingAcceptance {
                return Err(DebugError::Other(
                    "the connection was already handled".into(),
                ));
            }
            st.mapper = server.mapper(&root);
            st.server_id = Some(server.id.clone());
        }
        self.start().await;
        Ok(())
    }

    async fn start(self: &Arc<Self>) {
        let cfg = self.shared.config();
        self.state().status = SessionStatus::Running;
        self.emit_changed();
        let features = [
            ("max_children", cfg.max_children.to_string()),
            ("max_depth", cfg.max_depth.to_string()),
            ("max_data", cfg.max_data.to_string()),
            ("show_hidden", "1".to_string()),
            ("extended_properties", "1".to_string()),
            ("notify_ok", "1".to_string()),
            ("resolved_breakpoints", "1".to_string()),
        ];
        for (name, value) in features {
            match self
                .conn
                .send("feature_set", &[('n', name.into()), ('v', value)], None)
                .await
            {
                Ok(el) => {
                    if el.attr("success") != Some("1") {
                        tracing::debug!("feature {name} not supported");
                    }
                }
                Err(DebugError::Closed) => return,
                Err(e) => tracing::debug!("feature_set {name}: {e}"),
            }
        }
        let _ = self.conn.send("stdout", &[('c', "1".into())], None).await;
        self.sync_all_breakpoints().await;
        if cfg.break_at_first_line {
            self.state().first_step = true;
            let _ = self.continue_with(StepKind::StepInto);
        } else {
            let _ = self.continue_with(StepKind::Run);
        }
    }

    /// Engine feature value (`feature_get`).
    pub async fn feature(&self, name: &str) -> DebugResult<Option<String>> {
        let el = self
            .conn
            .send("feature_get", &[('n', name.into())], None)
            .await?;
        check_error(&el)?;
        Ok((el.attr("supported") != Some("0")).then(|| el.text.trim().to_string()))
    }

    // ---- packets that are not answers ---------------------------------------------------

    pub(crate) fn on_message(&self, el: Element) {
        match el.local_name() {
            "stream" => {
                let text = if el.attr("encoding") == Some("base64") {
                    b64_decode(&el.text)
                } else {
                    el.text.clone()
                };
                self.shared.emit(DebugEvent::Output {
                    session_id: self.id.clone(),
                    stream: el.attr("type").unwrap_or("stdout").to_string(),
                    text,
                });
            }
            "notify" => {
                if el.attr("name") == Some("error") {
                    let text = el
                        .child("message")
                        .map(|m| m.text.trim().to_string())
                        .unwrap_or_default();
                    if !text.is_empty() {
                        self.shared.emit(DebugEvent::Output {
                            session_id: self.id.clone(),
                            stream: "stderr".into(),
                            text: format!("{text}\n"),
                        });
                    }
                }
            }
            other => tracing::debug!("unhandled DBGp packet <{other}>"),
        }
    }

    /// The engine disconnected.
    pub(crate) fn on_closed(&self) {
        self.end();
    }

    fn end(&self) {
        {
            let mut st = self.state();
            if st.ended {
                return;
            }
            st.ended = true;
            st.status = SessionStatus::Stopped;
            st.location = None;
        }
        self.shared.remove_session(&self.id);
        self.shared.emit(DebugEvent::Ended {
            session_id: self.id.clone(),
        });
    }

    // ---- execution ----------------------------------------------------------------------

    fn require_paused(&self) -> DebugResult<()> {
        match self.state().status {
            SessionStatus::Paused => Ok(()),
            SessionStatus::Stopped => Err(DebugError::Closed),
            _ => Err(DebugError::Other("the script is not suspended".into())),
        }
    }

    /// Resume or step. Returns immediately; a `Session` event follows when the script
    /// suspends again.
    pub fn continue_with(self: &Arc<Self>, kind: StepKind) -> DebugResult<()> {
        {
            let mut st = self.state();
            match st.status {
                SessionStatus::Paused | SessionStatus::Running => {}
                SessionStatus::Stopped => return Err(DebugError::Closed),
                _ => return Err(DebugError::Other("the session has not started".into())),
            }
            st.status = SessionStatus::Running;
            st.location = None;
            st.reason = None;
            st.exception = None;
            st.last_step = Some(kind);
        }
        self.emit_changed();
        let me = Arc::clone(self);
        tokio::spawn(async move {
            let res = me.conn.send_untimed(kind.command(), &[]).await;
            me.after_continue(res).await;
        });
        Ok(())
    }

    async fn after_continue(self: &Arc<Self>, res: DebugResult<Element>) {
        let status = match res.and_then(|el| StatusResponse::parse(&el)) {
            Ok(s) => s,
            Err(DebugError::Closed) => return self.end(),
            Err(e) => {
                tracing::warn!("debug continuation failed: {e}");
                self.shared.emit(DebugEvent::Error {
                    session_id: Some(self.id.clone()),
                    message: e.to_string(),
                });
                self.state().status = SessionStatus::Paused;
                self.emit_changed();
                return;
            }
        };
        match status.status {
            EngineStatus::Break => self.suspended(status).await,
            EngineStatus::Stopping => {
                let _ = self.conn.send_with_timeout("stop", &[], None, SHORT).await;
                self.conn.shutdown().await;
                self.end();
            }
            EngineStatus::Stopped => {
                self.conn.shutdown().await;
                self.end();
            }
            EngineStatus::Running | EngineStatus::Starting => {}
        }
    }

    async fn suspended(self: &Arc<Self>, status: StatusResponse) {
        let (file_uri, line) = match (status.file_uri.clone(), status.line) {
            (Some(f), Some(l)) => (f, l),
            _ => match self
                .stack_raw(Some(0))
                .await
                .ok()
                .and_then(|f| f.into_iter().next())
            {
                Some(top) => (top.file_uri, top.line),
                None => (self.init.file_uri.clone(), 0),
            },
        };
        let mapper = self.mapper();
        let file = mapper.to_local(&file_uri);
        let skip_unmapped = self.shared.config().ignore_unmapped_files;
        let (first, last_step) = {
            let mut st = self.state();
            let first = std::mem::take(&mut st.first_step);
            let stepping = matches!(
                st.last_step,
                Some(
                    StepKind::StepInto
                        | StepKind::StepOver
                        | StepKind::StepOut
                        | StepKind::StepIntoInstruction
                        | StepKind::StepOverInstruction
                )
            );
            if file.is_none()
                && skip_unmapped
                && stepping
                && !first
                && status.exception.is_none()
                && st.auto_skips < MAX_AUTO_SKIPS
            {
                st.auto_skips += 1;
                st.status = SessionStatus::Paused;
                drop(st);
                let _ = self.continue_with(StepKind::StepOut);
                return;
            }
            st.auto_skips = 0;
            (first, st.last_step)
        };
        // The breakpoint that fired: only after Resume (a step may end on a breakpoint's
        // line without hitting it) or for an exception.
        let resumed = matches!(last_step, Some(StepKind::Run) | None) && !first;
        let hit = if status.exception.is_some() || resumed {
            self.shared.with_breakpoints(|s| {
                s.find_hit(file.as_deref(), line, status.exception.as_deref())
                    .cloned()
            })
        } else {
            None
        };
        if let Some(bp) = hit.as_ref().filter(|b| b.acts_on_hit()) {
            self.on_hit(bp, file.as_deref(), &file_uri, line, &status)
                .await;
            if !bp.suspend {
                // A logpoint: logged, now let the script run on.
                self.state().status = SessionStatus::Paused;
                let _ = self.continue_with(StepKind::Run);
                return;
            }
        }
        let reason = if status.exception.is_some() {
            BreakReason::Exception
        } else if first {
            BreakReason::FirstLine
        } else {
            match last_step {
                Some(StepKind::Run) | None => {
                    if hit.is_some() {
                        BreakReason::Breakpoint
                    } else {
                        BreakReason::Pause
                    }
                }
                _ => BreakReason::Step,
            }
        };
        {
            let mut st = self.state();
            st.status = SessionStatus::Paused;
            st.reason = Some(reason);
            st.exception = status.exception.map(|name| ExceptionInfo {
                name,
                message: status.message,
            });
            st.location = Some(Location {
                file,
                file_uri,
                line,
            });
        }
        self.emit_changed();
    }

    /// Log what a breakpoint asks for and drop it when it is temporary.
    async fn on_hit(
        &self,
        bp: &Breakpoint,
        file: Option<&str>,
        file_uri: &str,
        line: u32,
        status: &StatusResponse,
    ) {
        let place = file.unwrap_or(file_uri);
        let mut text = String::new();
        if bp.log_message {
            match &status.exception {
                Some(name) => text.push_str(&format!(
                    "Exception breakpoint reached: {name} at {place}:{line}\n"
                )),
                None => text.push_str(&format!("Breakpoint reached at {place}:{line}\n")),
            }
        }
        if bp.log_stack {
            match self.stack_raw(None).await {
                Ok(frames) => {
                    let mapper = self.mapper();
                    for f in frames {
                        let at = mapper
                            .to_local(&f.file_uri)
                            .unwrap_or_else(|| f.file_uri.clone());
                        text.push_str(&format!("\t{} ({at}:{})\n", f.r#where, f.line));
                    }
                }
                Err(e) => text.push_str(&format!("\tstack trace unavailable: {e}\n")),
            }
        }
        if let Some(expr) = bp.log_expression() {
            let value = match self.eval_at(expr, 0).await {
                Ok(p) => crate::console::display_value(&p),
                Err(e) => format!("Cannot evaluate {expr}: {e}"),
            };
            text.push_str(&value);
            text.push('\n');
        }
        if !text.is_empty() {
            self.shared.emit(DebugEvent::Output {
                session_id: self.id.clone(),
                stream: "log".into(),
                text,
            });
        }
        if bp.temporary {
            self.state().remote_bps.remove(&bp.id);
            self.shared.remove_hit_breakpoint(bp.id);
        }
    }

    /// Ask a running script to suspend (needs engine support for async commands).
    pub async fn pause(&self) -> DebugResult<()> {
        if self.status() != SessionStatus::Running {
            return Ok(());
        }
        let el = self
            .conn
            .send_with_timeout("break", &[], None, SHORT)
            .await?;
        check_error(&el)
    }

    /// Stop the script.
    pub async fn stop(&self) {
        if self.status() == SessionStatus::Paused {
            let _ = self.conn.send_with_timeout("stop", &[], None, SHORT).await;
        }
        self.conn.shutdown().await;
        self.end();
    }

    /// Let the script run to its end without the debugger.
    pub async fn detach(&self) {
        let _ = self
            .conn
            .send_with_timeout("detach", &[], None, SHORT)
            .await;
        self.conn.shutdown().await;
        self.end();
    }

    /// Run until `file:line` (a temporary breakpoint), PHPStorm's "Run to Cursor".
    pub async fn run_to(self: &Arc<Self>, file: &str, line: u32) -> DebugResult<()> {
        self.require_paused()?;
        let uri = self.mapper().to_remote(file);
        let el = self
            .conn
            .send(
                "breakpoint_set",
                &[
                    ('t', "line".into()),
                    ('f', uri),
                    ('n', line.to_string()),
                    ('r', "1".into()),
                ],
                None,
            )
            .await?;
        check_error(&el)?;
        self.continue_with(StepKind::Run)
    }

    // ---- breakpoints --------------------------------------------------------------------

    fn accepts_commands(&self) -> bool {
        matches!(
            self.state().status,
            SessionStatus::Running | SessionStatus::Paused
        )
    }

    async fn set_remote(&self, bp: &Breakpoint) -> DebugResult<()> {
        if !bp.enabled || !bp.for_dbgp() {
            return Ok(());
        }
        let (args, data) = match bp.kind {
            BreakpointKind::Line => {
                let (Some(file), Some(line)) = (&bp.file, bp.line) else {
                    return Ok(());
                };
                let uri = self.mapper().to_remote(file);
                let ty = if bp.condition().is_some() {
                    "conditional"
                } else {
                    "line"
                };
                (
                    vec![('t', ty.to_string()), ('f', uri), ('n', line.to_string())],
                    bp.condition(),
                )
            }
            BreakpointKind::Function | BreakpointKind::Data => return Ok(()),
            BreakpointKind::Exception => (
                vec![
                    ('t', "exception".to_string()),
                    (
                        'x',
                        bp.exception
                            .as_deref()
                            .map(|e| e.trim().trim_start_matches('\\'))
                            .filter(|e| !e.is_empty())
                            .unwrap_or("*")
                            .to_string(),
                    ),
                ],
                None,
            ),
        };
        let mut args = args;
        if bp.temporary {
            args.push(('r', "1".into()));
        }
        if let Some(n) = bp.pass_count() {
            args.push(('h', n.to_string()));
            args.push(('o', ">=".into()));
        }
        let el = self.conn.send("breakpoint_set", &args, data).await?;
        check_error(&el)?;
        if let Some(id) = el.attr("id") {
            self.state().remote_bps.insert(bp.id, id.to_string());
        }
        Ok(())
    }

    async fn remove_remote(&self, id: u64) -> DebugResult<()> {
        let remote = self.state().remote_bps.remove(&id);
        if let Some(remote) = remote {
            let el = self
                .conn
                .send("breakpoint_remove", &[('d', remote)], None)
                .await?;
            // "No such breakpoint" (temporary ones already hit) is fine.
            if let Err(DebugError::Engine { code, .. }) = check_error(&el) {
                if code != 205 {
                    tracing::debug!("breakpoint_remove failed with {code}");
                }
            }
        }
        Ok(())
    }

    /// Bring one breakpoint in line with the store (`None` = deleted).
    pub(crate) async fn sync_breakpoint(&self, id: u64, bp: Option<Breakpoint>, muted: bool) {
        if !self.accepts_commands() {
            return;
        }
        if let Err(e) = self.remove_remote(id).await {
            tracing::debug!("removing breakpoint {id}: {e}");
        }
        if let (Some(bp), false) = (bp, muted) {
            if let Err(e) = self.set_remote(&bp).await {
                tracing::warn!("cannot set breakpoint {id}: {e}");
            }
        }
    }

    /// Re-send every breakpoint (after start and when muting changes).
    pub(crate) async fn sync_all_breakpoints(&self) {
        let ids: Vec<u64> = self.state().remote_bps.keys().copied().collect();
        for id in ids {
            let _ = self.remove_remote(id).await;
        }
        let snapshot = self.shared.with_breakpoints(|s| s.snapshot());
        if snapshot.muted {
            return;
        }
        for bp in snapshot.breakpoints {
            if let Err(e) = self.set_remote(&bp).await {
                tracing::warn!("cannot set breakpoint {}: {e}", bp.id);
                if matches!(e, DebugError::Closed) {
                    return;
                }
            }
        }
    }

    /// Breakpoints as the engine knows them.
    pub async fn remote_breakpoints(&self) -> DebugResult<Vec<RemoteBreakpoint>> {
        let el = self.conn.send("breakpoint_list", &[], None).await?;
        parse_breakpoints(&el)
    }

    // ---- inspection ---------------------------------------------------------------------

    async fn stack_raw(&self, depth: Option<u32>) -> DebugResult<Vec<crate::protocol::RawFrame>> {
        let args: Vec<(char, String)> = depth.map(|d| ('d', d.to_string())).into_iter().collect();
        let el = self.conn.send("stack_get", &args, None).await?;
        parse_stack(&el)
    }

    pub async fn stack(&self) -> DebugResult<Vec<Frame>> {
        self.require_paused()?;
        let mapper = self.mapper();
        Ok(self
            .stack_raw(None)
            .await?
            .into_iter()
            .map(|f| Frame {
                level: f.level,
                file: mapper.to_local(&f.file_uri),
                file_uri: f.file_uri,
                line: f.line,
                r#where: f.r#where,
                ..Default::default()
            })
            .collect())
    }

    pub async fn contexts(&self, depth: u32) -> DebugResult<Vec<ContextName>> {
        self.require_paused()?;
        let el = self
            .conn
            .send("context_names", &[('d', depth.to_string())], None)
            .await?;
        parse_context_names(&el)
    }

    /// Variables of a context (0 = locals, 1 = superglobals, 2 = constants) in a frame.
    pub async fn variables(&self, depth: u32, context: u32) -> DebugResult<Vec<Property>> {
        self.require_paused()?;
        let el = self
            .conn
            .send(
                "context_get",
                &[('d', depth.to_string()), ('c', context.to_string())],
                None,
            )
            .await?;
        parse_properties(&el)
    }

    /// One variable with a page of its children (`fullname` as reported by the engine).
    pub async fn property(
        &self,
        fullname: &str,
        depth: u32,
        context: u32,
        page: u32,
    ) -> DebugResult<Property> {
        self.require_paused()?;
        let el = self
            .conn
            .send(
                "property_get",
                &[
                    ('d', depth.to_string()),
                    ('c', context.to_string()),
                    ('n', fullname.to_string()),
                    ('p', page.to_string()),
                ],
                None,
            )
            .await?;
        parse_property(&el)
    }

    /// Evaluate a PHP expression in the given frame's scope.
    pub async fn eval(&self, expression: &str, depth: u32) -> DebugResult<Property> {
        self.require_paused()?;
        self.eval_at(expression, depth).await
    }

    /// `eval` without the state check (logpoints evaluate before the session counts as
    /// suspended).
    async fn eval_at(&self, expression: &str, depth: u32) -> DebugResult<Property> {
        let mut args = Vec::new();
        if depth > 0 {
            args.push(('d', depth.to_string()));
        }
        let el = self.conn.send("eval", &args, Some(expression)).await?;
        let mut p = parse_property(&el)?;
        // Eval results are anonymous; children are fetched with property_get by expression.
        if p.fullname.is_empty() {
            p.fullname = expression.to_string();
        }
        if p.name.is_empty() {
            p.name = expression.to_string();
        }
        Ok(p)
    }

    /// Completion items for the expression `text` (caret at its end) in a frame: variables
    /// after `$`, properties after `->`.
    pub async fn complete(
        &self,
        text: &str,
        depth: u32,
    ) -> DebugResult<Vec<crate::console::Completion>> {
        use crate::console::{complete_members, complete_variables, completion_target};
        self.require_paused()?;
        Ok(match completion_target(text) {
            crate::console::CompletionTarget::Variable { prefix } => {
                let mut vars = self.variables(depth, 0).await?;
                if let Ok(globals) = self.variables(depth, 1).await {
                    vars.extend(globals);
                }
                complete_variables(&vars, &prefix)
            }
            crate::console::CompletionTarget::Member { object, prefix } => {
                match self.property(&object, depth, 0, 0).await {
                    Ok(p) => complete_members(&p, &prefix),
                    Err(DebugError::Closed) => return Err(DebugError::Closed),
                    Err(_) => Vec::new(),
                }
            }
            crate::console::CompletionTarget::None => Vec::new(),
        })
    }

    /// Assign a new value (`property_set`), e.g. from "Set Value…" in the Variables tree.
    pub async fn set_value(&self, fullname: &str, value: &str, depth: u32) -> DebugResult<()> {
        self.require_paused()?;
        let el = self
            .conn
            .send(
                "property_set",
                &[('n', fullname.to_string()), ('d', depth.to_string())],
                Some(value),
            )
            .await?;
        check_error(&el)?;
        if el.attr("success") == Some("0") {
            return Err(DebugError::Other(format!("cannot set {fullname}")));
        }
        Ok(())
    }
}
