//! WebForge's JavaScript debugger: Microsoft's js-debug (vscode-js-debug, MIT) over the
//! Debug Adapter Protocol, on the shared debugger (`forge-dap`).
//!
//! - [`release`]: the pinned js-debug release (standalone DAP server), downloaded on the
//!   user's consent from GitHub, SHA-256 checked and unpacked into Forge's data folder.
//!   Nothing is bundled.
//! - [`adapter`]: [`JsDebugAdapter`], the `pwa-node`, `pwa-chrome` and `pwa-msedge` debug
//!   adapters: each session runs js-debug's DAP server on Node.js; the debuggee runs in the
//!   child session js-debug asks for (`startDebugging`).
//! - [`browsers`]: Chrome, Edge and Chromium on this machine for "JavaScript Debug".
//! - [`launch`]: the launch request of a run configuration (Node.js, npm, JavaScript
//!   Debug, Attach to Node.js/Chrome, the Node.js test runners).

pub mod adapter;
pub mod browsers;
pub mod launch;
pub mod release;

use std::path::PathBuf;

use serde::Serialize;

pub use adapter::{JsDebugAdapter, JsDebugConfig, Target};
pub use browsers::{system_browsers, Browser, BrowserKind};
pub use launch::{request, JsConfiguration, LaunchContext};
pub use release::{pinned_release, Release};

/// What the UI shows about js-debug (the consent dialog, Settings).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JsDebugStatus {
    /// The DAP server script Forge uses, if any.
    pub server: Option<PathBuf>,
    /// Whether `server` is the user's own folder (`web.jsDebug.path`).
    pub custom: bool,
    /// The release Forge downloads.
    pub release: Release,
    /// The Node.js that runs it (empty: none found).
    pub node: String,
}

/// Where js-debug is: the user's folder when set (and valid), else Forge's download.
pub fn locate(user_path: Option<&str>, base: Option<&std::path::Path>) -> (Option<PathBuf>, bool) {
    if let Some(p) = user_path.map(str::trim).filter(|p| !p.is_empty()) {
        return (release::user_server(std::path::Path::new(p)), true);
    }
    let release = pinned_release();
    (
        base.and_then(|b| release::installed(b, &release.version)),
        false,
    )
}
