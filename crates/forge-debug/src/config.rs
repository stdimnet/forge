use serde::{Deserialize, Serialize};

/// Debugger options (Settings > Languages & Frameworks > PHP > Debug).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DebugConfig {
    /// Port Forge listens on for Xdebug (`xdebug.client_port`).
    pub port: u16,
    /// Suspend on the first line of every script.
    pub break_at_first_line: bool,
    /// Step out of files that are not mapped into the project instead of suspending there.
    pub ignore_unmapped_files: bool,
    /// Refuse connections that match no configured server and no project file.
    pub ignore_external_connections: bool,
    /// `max_children`: array elements / object properties per page.
    pub max_children: u32,
    /// `max_depth`: nesting levels returned at once.
    pub max_depth: u32,
    /// `max_data`: bytes of string data returned per value.
    pub max_data: u32,
    /// How long to wait for an answer to a non-continuation command.
    pub command_timeout_ms: u64,
}

impl Default for DebugConfig {
    fn default() -> Self {
        Self {
            port: 9003,
            break_at_first_line: false,
            ignore_unmapped_files: true,
            ignore_external_connections: false,
            max_children: 100,
            max_depth: 1,
            max_data: 1024 * 64,
            command_timeout_ms: 10_000,
        }
    }
}
