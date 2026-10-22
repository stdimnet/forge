//! Pure logic behind Run Anything, compound run configurations, "Before launch" tasks and
//! External Tools. No I/O, so `forge-wasm` builds it for browser mode; the desktop app
//! collects the inputs (scripts, files, settings) and starts the processes.
//!
//! - [`args`]: splitting and quoting of argument strings (shared with `forge-process`).
//! - [`macros`]: PHPStorm's macros (`$FilePath$`, `$Prompt$`, …), their previews and
//!   expansion.
//! - [`external`]: External Tools (Settings | Tools | External Tools): the model, its
//!   settings encoding, validation and the command line of a tool run.
//! - [`launch`]: what starting a configuration means: compound members and the flattened,
//!   ordered "Before launch" tasks, with missing configurations and cycles reported.
//! - [`anything`]: Run Anything (double Ctrl): grouped, ranked suggestions, help, history
//!   and turning a typed command into a temporary run configuration.

pub mod anything;
pub mod args;
pub mod external;
pub mod launch;
pub mod macros;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A run configuration as far as these modules need it; serialized like
/// `forge_process::RunConfiguration` (`{ name, type, options, … }`), so values from the UI
/// and the configuration files deserialize into it and results start as temporary
/// configurations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Configuration {
    pub name: String,
    #[serde(rename = "type")]
    pub type_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub working_directory: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub interpreter_options: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub arguments: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub before_launch: Vec<Task>,
}

impl Configuration {
    pub fn new(name: impl Into<String>, type_id: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            type_id: type_id.into(),
            ..Self::default()
        }
    }

    pub fn option(&self, key: &str) -> &str {
        self.options.get(key).map(String::as_str).unwrap_or("")
    }

    pub fn with_option(mut self, key: &str, value: impl Into<String>) -> Self {
        self.options.insert(key.into(), value.into());
        self
    }
}

/// A "Before launch" task: `{ type, value }` like `forge_process::BeforeLaunchTask`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Task {
    #[serde(rename = "type")]
    pub kind: String,
    pub value: String,
}

impl Task {
    pub fn new(kind: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            value: value.into(),
        }
    }
}
