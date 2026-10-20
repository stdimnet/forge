//! Parsers and models of process output, SQL, DQL, Markdown and the AI chat, mirroring the
//! IPC commands of the desktop app for the browser-mode mocks.

pub mod chat;
pub mod dql;
pub mod markdown;
pub mod output;
pub mod sql;

/// Parse a JSON request; the error is already the JSON the UI expects.
pub(crate) fn parse<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, String> {
    serde_json::from_str(json)
        .map_err(|e| crate::to_json(&serde_json::json!({ "error": e.to_string() })))
}
