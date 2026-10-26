//! The JavaScript debugger (`webforge-js-debug`): js-debug's adapters on the shared DAP
//! manager, its consented download, the browsers for "JavaScript Debug" and the launch
//! requests of WebForge's run configurations.
//!
//! Events: `js-debug://install` (`{ message }` steps and `{ label, done, total }` progress)
//! while `js_debug_install` runs.

use std::sync::Arc;

use forge_core::{ForgeError, Result};
use forge_dap::LaunchRequest;
use forge_langservers::managed::{Event, Http, Installer, Sources};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};
use webforge_js_debug::{
    release, system_browsers, Browser, JsConfiguration, JsDebugAdapter, JsDebugConfig,
    JsDebugStatus, LaunchContext,
};

use crate::commands::debug::DebugState;
use crate::state::AppState;

const PLUGIN: &str = "forge.js-debugger";

fn setting(state: &AppState, key: &str) -> Option<String> {
    state
        .read_settings()
        .get_as::<String>(key)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// The project's Node.js: `web.node.interpreter`, else the machine's or Forge's download.
fn node(state: &AppState) -> String {
    setting(state, "web.node.interpreter")
        .or_else(|| {
            forge_langservers::Detector::system()
                .node()
                .map(|p| p.to_string_lossy().into_owned())
        })
        .unwrap_or_default()
}

fn config(state: &AppState) -> JsDebugConfig {
    let user = setting(state, "web.jsDebug.path");
    let base = release::default_base_dir();
    let (server, _) = webforge_js_debug::locate(user.as_deref(), base.as_deref());
    JsDebugConfig {
        node: node(state),
        server,
        skip_files: setting(state, "web.jsDebug.skipFiles")
            .unwrap_or_else(|| "<node_internals>/**".into())
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
    }
}

/// Register js-debug's adapters with the DAP manager while the plugin is enabled (called
/// whenever the adapters are listed or a session starts).
pub(crate) fn sync(state: &AppState, debug: &DebugState) {
    let enabled = state
        .plugins
        .read()
        .expect("plugins lock poisoned")
        .is_enabled(PLUGIN);
    if enabled {
        for a in JsDebugAdapter::all(&config(state)) {
            debug.dap.register_adapter(Arc::new(a));
        }
    } else {
        for t in webforge_js_debug::Target::ALL {
            debug.dap.unregister_adapter(t.adapter_type());
        }
    }
}

fn status(state: &AppState) -> JsDebugStatus {
    let user = setting(state, "web.jsDebug.path");
    let base = release::default_base_dir();
    let (server, custom) = webforge_js_debug::locate(user.as_deref(), base.as_deref());
    JsDebugStatus {
        server,
        custom,
        release: release::pinned_release(),
        node: node(state),
    }
}

/// Whether js-debug is installed, where, and the release Forge would download.
#[tauri::command]
pub fn js_debug_status(state: State<'_, AppState>) -> JsDebugStatus {
    status(&state)
}

/// Download js-debug (the user agreed in the UI): Node.js first when the machine has none,
/// then the pinned release, SHA-256 checked, into Forge's data folder.
#[tauri::command]
pub async fn js_debug_install(
    app: AppHandle,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<JsDebugStatus> {
    let base = release::default_base_dir()
        .ok_or_else(|| ForgeError::Other("no user data folder for js-debug".into()))?;
    let (registry, mirror) = {
        let s = state.read_settings();
        (
            s.get_as::<String>("langservers.download.npmRegistry"),
            s.get_as::<String>("langservers.download.nodeMirror"),
        )
    };
    let emitter = app.clone();
    let mut on = move |e: Event| {
        let payload: Value = match e {
            Event::Step { message } => json!({ "message": message }),
            Event::Progress { label, done, total } => {
                json!({ "label": label, "done": done, "total": total })
            }
            Event::Output { line, .. } => json!({ "message": line }),
        };
        let _ = emitter.emit("js-debug://install", payload);
    };
    if setting(&state, "web.node.interpreter").is_none() {
        let installer = Installer::system(Sources::new(registry.as_deref(), mirror.as_deref()))
            .map_err(ForgeError::Other)?;
        installer
            .ensure_node(&mut on)
            .await
            .map_err(ForgeError::Other)?;
    }
    let http = Http::new().map_err(ForgeError::Other)?;
    release::ensure(&http, &base, &release::pinned_release(), &mut on)
        .await
        .map_err(ForgeError::Other)?;
    sync(&state, &debug);
    Ok(status(&state))
}

/// Chrome, Edge and Chromium on this machine.
#[tauri::command]
pub async fn js_debug_browsers() -> Vec<Browser> {
    tauri::async_runtime::spawn_blocking(system_browsers)
        .await
        .unwrap_or_default()
}

/// The js-debug launch request of a run configuration (Node.js, npm, JavaScript Debug,
/// Attach to Node.js/Chrome, Jest, Vitest, Mocha, Playwright, node --test).
#[tauri::command]
pub async fn js_debug_request(
    configuration: JsConfiguration,
    state: State<'_, AppState>,
) -> Result<LaunchRequest> {
    let browsers = tauri::async_runtime::spawn_blocking(system_browsers)
        .await
        .unwrap_or_default();
    let ctx = LaunchContext {
        node: node(&state),
        browsers,
        browser: setting(&state, "web.jsDebug.browser").unwrap_or_else(|| "auto".into()),
        test_specs: state
            .plugins
            .read()
            .expect("plugins lock poisoned")
            .test_frameworks()
            .cloned()
            .collect(),
        report_dir: std::env::temp_dir()
            .join("forge-js-debug")
            .to_string_lossy()
            .into_owned(),
    };
    let _ = std::fs::create_dir_all(&ctx.report_dir);
    webforge_js_debug::request(&configuration, &ctx).map_err(ForgeError::Other)
}
