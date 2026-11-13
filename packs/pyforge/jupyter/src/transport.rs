//! What the kernel connections share: channels, the incoming message queue and errors.

use tokio::sync::mpsc;

use crate::protocol::Message;

/// A channel of the messaging protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    Shell,
    Control,
    Iopub,
    Stdin,
}

impl Channel {
    pub fn name(self) -> &'static str {
        match self {
            Channel::Shell => "shell",
            Channel::Control => "control",
            Channel::Iopub => "iopub",
            Channel::Stdin => "stdin",
        }
    }

    pub fn parse(name: &str) -> Option<Channel> {
        Some(match name {
            "shell" => Channel::Shell,
            "control" => Channel::Control,
            "iopub" => Channel::Iopub,
            "stdin" => Channel::Stdin,
            _ => return None,
        })
    }
}

/// Where a connection puts the messages it receives.
pub type Incoming = mpsc::UnboundedSender<(Channel, Message)>;

/// `msg_type` of the message a connection sends when it lost the kernel.
pub const DISCONNECTED: &str = "forge_disconnected";

/// The message that tells the session its connection is gone.
pub fn disconnected() -> Message {
    Message::new("", "", DISCONNECTED, serde_json::Value::Null)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError(pub String);

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TransportError {}
