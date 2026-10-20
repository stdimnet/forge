use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::breakpoints::BreakpointsSnapshot;
use crate::protocol::EngineStatus;

/// Lifecycle of a debug session as the UI sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionStatus {
    /// Connected, identifying the server and negotiating features.
    Initializing,
    /// Unknown server: waiting for the user to accept or ignore it.
    AwaitingAcceptance,
    Running,
    Paused,
    Stopped,
}

impl From<EngineStatus> for SessionStatus {
    fn from(s: EngineStatus) -> Self {
        match s {
            EngineStatus::Break => Self::Paused,
            EngineStatus::Stopping | EngineStatus::Stopped => Self::Stopped,
            EngineStatus::Starting => Self::Initializing,
            EngineStatus::Running => Self::Running,
        }
    }
}

/// Where a suspended script is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    /// Project relative file, `None` when the server file is not mapped.
    pub file: Option<String>,
    pub file_uri: String,
    /// 1-based line.
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionInfo {
    pub name: String,
    pub message: Option<String>,
}

/// Why the script suspended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BreakReason {
    Breakpoint,
    Step,
    FirstLine,
    Exception,
    Pause,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub ide_key: String,
    pub language: String,
    pub engine: String,
    /// Script the engine started with.
    pub file_uri: String,
    /// `file_uri` in the project, if mapped.
    pub file: Option<String>,
    /// Remote address of the engine.
    pub peer: String,
    /// "shop.test", "localhost:8080" or "CLI".
    pub server_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Id of the configured server the session uses.
    pub server_id: Option<String>,
    pub status: SessionStatus,
    pub location: Option<Location>,
    pub reason: Option<BreakReason>,
    pub exception: Option<ExceptionInfo>,
}

/// Everything the debugger reports to the UI. The IPC layer emits each on
/// [`DebugEvent::channel`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum DebugEvent {
    Listening {
        listening: bool,
        port: u16,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// A session started or changed state (running, paused at a location, ...).
    Session { session: SessionInfo },
    /// A connection from an unknown server waits for "Accept" or "Ignore".
    Incoming { session: SessionInfo },
    Ended { session_id: String },
    /// Script output (`stdout` copy).
    Output {
        session_id: String,
        stream: String,
        text: String,
    },
    Breakpoints { snapshot: BreakpointsSnapshot },
    /// A failure worth a notification.
    Error {
        session_id: Option<String>,
        message: String,
    },
}

impl DebugEvent {
    pub fn channel(&self) -> &'static str {
        match self {
            Self::Listening { .. } => "debug://listening",
            Self::Session { .. } => "debug://session",
            Self::Incoming { .. } => "debug://incoming",
            Self::Ended { .. } => "debug://ended",
            Self::Output { .. } => "debug://output",
            Self::Breakpoints { .. } => "debug://breakpoints",
            Self::Error { .. } => "debug://error",
        }
    }
}

pub type EventSink = Arc<dyn Fn(DebugEvent) + Send + Sync>;
