//! Code With Me IPC (toolbar popup, Tools | Code With Me, Settings | Tools | Code With Me).
//! The model, protocol and sessions are in `forge-collab`; this module only adapts them.
//!
//! Events: `collab://status` [`SessionView`] whenever the session changes.

use forge_collab::link::{LinkSummary, SessionLink};
use forge_collab::permissions::Permissions;
use forge_collab::presence::{Participant, ParticipantId};
use forge_collab::protocol::SharedFile;
use forge_core::{ForgeError, Result};
use serde::Serialize;

pub const STATUS_EVENT: &str = "collab://status";

/// Code With Me's state in this window.
#[derive(Default)]
pub struct CollabState {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionRole {
    None,
    Host,
    Guest,
}

/// The session as the UI shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub role: SessionRole,
    /// Whether Start Session can be used; otherwise `host_unavailable` says why.
    pub can_host: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_unavailable: Option<String>,
    /// The invitation link (host).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub me: Option<ParticipantId>,
    pub participants: Vec<Participant>,
    /// Files guests can open.
    pub files: Vec<SharedFile>,
    /// The session's guest permissions (host) or this guest's permissions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Permissions>,
    /// `name (machine)` of the host (guest).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_label: Option<String>,
    /// A notice for every participant, e.g. that the session is a development preview.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

impl SessionView {
    fn idle() -> Self {
        Self {
            role: SessionRole::None,
            can_host: false,
            host_unavailable: Some("Sharing sessions is not available yet.".into()),
            link: None,
            me: None,
            participants: Vec::new(),
            files: Vec::new(),
            permissions: None,
            host_label: None,
            notice: None,
        }
    }
}

#[tauri::command]
pub fn collab_status() -> SessionView {
    SessionView::idle()
}

/// What the Join Session dialog shows about a pasted link.
#[tauri::command]
pub fn collab_parse_link(link: String) -> Result<LinkSummary> {
    SessionLink::parse(&link)
        .map(|l| l.summary())
        .map_err(|e| ForgeError::Other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_excludes_match_the_crate_defaults() {
        let registry = crate::plugins::builtin_registry().unwrap();
        let spec = registry
            .all_settings()
            .find(|s| s.key == "codeWithMe.excludedFiles")
            .unwrap();
        let defaults: Vec<String> = serde_json::from_value(spec.default.clone()).unwrap();
        assert_eq!(defaults, forge_collab::excludes::DEFAULT_PATTERNS);
    }

    #[test]
    fn bad_links_are_reported() {
        assert!(collab_parse_link("https://example.com".into()).is_err());
    }
}
