//! DotNetForge for game engines, in the style of Rider's Unity and Godot support.
//!
//! - [`unity`]: Unity projects and their editor version, the Unity Hub's editors, the Forge
//!   editor bridge (a Unity package, [`unity::package`], and its localhost protocol,
//!   [`unity::bridge`]), Unity tests in batch mode ([`unity::test_runner`]) and Unity processes
//!   for debugging ([`unity::processes`]).
//! - [`godot`]: Godot projects, the Godot executable, run configurations, the editor's
//!   GDScript language server and debug adapter ([`godot::lsp`], [`godot::dap`]) and
//!   GdUnit4 tests ([`godot::gdunit`]).
//!
//! Everything here is pure except the bridge client (tokio) and the editor searches, which
//! take the file system as functions so they can be tested with fixtures. Nothing starts
//! Unity or Godot: the app turns the command lines into runs, which Safe Mode refuses.

pub mod godot;
pub mod unity;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A command line for the app to run (in the project folder unless `cwd` is set).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cwd: String,
}

impl CommandSpec {
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
            ..Self::default()
        }
    }
}

/// Escape a string for a regular expression (Unity's `-testFilter`).
pub fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\.+*?()|[]{}^$".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Shared helpers.
pub(crate) mod util {
    /// `/`-separated path text.
    pub fn slash(p: &str) -> String {
        p.replace('\\', "/")
    }

    /// The file name of a `/` or `\` separated path.
    pub fn file_name(p: &str) -> &str {
        p.rsplit(['/', '\\']).next().unwrap_or(p)
    }
}
