//! Debug Adapter Protocol sessions (`forge-dap`): starting them (launch, Attach to Process,
//! Attach to DAP server, core dumps), threads, the native views (memory, disassembly,
//! registers, modules) and watchpoints. Everything a PHP session can do as well (stepping,
//! frames, variables, evaluate, breakpoints) goes through the shared `debug_*` commands,
//! which route DAP session ids here; events use the same `debug://*` channels.

use std::path::PathBuf;
use std::time::Duration;

use forge_core::{ForgeError, Result};
use forge_dap::protocol::{Module, Thread};
use forge_dap::{
    AdapterInfo, AdapterRegistry, DapConfig, Instruction, LaunchRequest, LoadedSource, MemoryBlock,
    ProcessInfo,
};
use forge_debug::{Breakpoint, Property, SessionInfo};
use tauri::State;

use crate::commands::debug::{dap_session, server, DebugState};
use crate::state::AppState;

/// Adapters of the enabled plugins (with the user's executable paths), the project root
/// and the DAP settings, applied to the manager.
fn sync(state: &AppState, debug: &DebugState) {
    // Opens the project's breakpoints in the shared store.
    let _ = server(state, debug);
    // debugpy's adapter runs in the project's interpreter (resolved before the settings
    // lock is taken: resolving reads the settings too).
    let python_adapter = crate::commands::python_tools::adapter_python(state);
    let settings = state.read_settings();
    let mut registry = AdapterRegistry::new();
    {
        let plugins = state.plugins.read().expect("plugins lock poisoned");
        for spec in plugins.debug_adapters() {
            let mut command = spec
                .command_setting
                .as_deref()
                .and_then(|key| settings.get_as::<String>(key));
            if spec.r#type == "python" && command.is_none() {
                command = python_adapter.clone();
            }
            registry.register_manifest(spec.clone(), command);
        }
    }
    debug.dap.set_registry(registry);
    crate::commands::js_debug::sync(state, debug);
    crate::commands::go::sync_debug(state, debug);
    debug.dap.set_config(DapConfig {
        request_timeout: Duration::from_millis(
            settings
                .get_as::<u64>("debugger.dap.requestTimeout")
                .unwrap_or(10_000)
                .max(1000),
        ),
        trace: settings.get_bool("debugger.dap.trace").unwrap_or(false),
        ..DapConfig::default()
    });
    let root: Option<PathBuf> = state.project_info().map(|i| i.root);
    debug.dap.set_project_root(root);
}

fn session(debug: &DebugState, id: &str) -> Result<std::sync::Arc<forge_dap::DapSession>> {
    dap_session(debug, id)
        .ok_or_else(|| ForgeError::Other(format!("{id} is not a debug adapter session")))
}

/// Debug adapters of the enabled plugins.
#[tauri::command]
pub fn dap_adapters(state: State<'_, AppState>, debug: State<'_, DebugState>) -> Vec<AdapterInfo> {
    sync(&state, &debug);
    debug.dap.adapters()
}

/// Start a session. Launching runs project code and is refused in Safe Mode; attaching to
/// a running process or DAP server and opening a core dump are not.
#[tauri::command]
pub async fn dap_start(
    request: LaunchRequest,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<SessionInfo> {
    if request.runs_code() {
        state.require_trusted("Debugging a program")?;
    }
    sync(&state, &debug);
    Ok(debug.dap.start(request).await?)
}

/// Rerun: in place when the adapter can restart, else a new session.
#[tauri::command]
pub async fn dap_restart(
    session_id: String,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<SessionInfo> {
    let s = session(&debug, &session_id)?;
    if s.launch_request().runs_code() {
        state.require_trusted("Debugging a program")?;
    }
    Ok(debug.dap.restart(&session_id).await?)
}

#[tauri::command]
pub async fn dap_threads(session_id: String, debug: State<'_, DebugState>) -> Result<Vec<Thread>> {
    Ok(session(&debug, &session_id)?.threads().await?)
}

/// Set Execution Point: continue at `line` of `file` (project relative).
#[tauri::command]
pub async fn dap_goto(
    session_id: String,
    file: String,
    line: u32,
    debug: State<'_, DebugState>,
) -> Result<()> {
    Ok(session(&debug, &session_id)?.goto(&file, line).await?)
}

/// The registers of a frame (Registers view).
#[tauri::command]
pub async fn dap_registers(
    session_id: String,
    depth: u32,
    debug: State<'_, DebugState>,
) -> Result<Vec<Property>> {
    Ok(session(&debug, &session_id)?.registers(depth).await?)
}

/// Memory view: `count` bytes at `memoryReference + offset`, as hex/ASCII rows.
#[tauri::command]
pub async fn dap_read_memory(
    session_id: String,
    memory_reference: String,
    offset: Option<i64>,
    count: u32,
    state: State<'_, AppState>,
    debug: State<'_, DebugState>,
) -> Result<MemoryBlock> {
    let per_row = state
        .read_settings()
        .get_as::<usize>("debugger.memory.bytesPerRow")
        .unwrap_or(16);
    Ok(session(&debug, &session_id)?
        .read_memory(
            &memory_reference,
            offset.unwrap_or(0),
            count.min(64 * 1024),
            per_row,
        )
        .await?)
}

/// Write hex bytes (`de ad be ef`) at `memoryReference + offset`.
#[tauri::command]
pub async fn dap_write_memory(
    session_id: String,
    memory_reference: String,
    offset: Option<i64>,
    data: String,
    debug: State<'_, DebugState>,
) -> Result<()> {
    Ok(session(&debug, &session_id)?
        .write_memory(&memory_reference, offset.unwrap_or(0), &data)
        .await?)
}

/// Disassembly view: `count` instructions from `instructionOffset` relative to
/// `memoryReference`; `current` marks the instruction pointer.
#[tauri::command]
pub async fn dap_disassemble(
    session_id: String,
    memory_reference: String,
    instruction_offset: Option<i64>,
    count: u32,
    current: Option<String>,
    debug: State<'_, DebugState>,
) -> Result<Vec<Instruction>> {
    Ok(session(&debug, &session_id)?
        .disassemble(
            &memory_reference,
            instruction_offset.unwrap_or(0),
            count.min(2000),
            current.as_deref(),
        )
        .await?)
}

#[tauri::command]
pub async fn dap_modules(session_id: String, debug: State<'_, DebugState>) -> Result<Vec<Module>> {
    Ok(session(&debug, &session_id)?.modules().await?)
}

#[tauri::command]
pub async fn dap_loaded_sources(
    session_id: String,
    debug: State<'_, DebugState>,
) -> Result<Vec<LoadedSource>> {
    Ok(session(&debug, &session_id)?.loaded_sources().await?)
}

/// Add a watchpoint on a variable (`container` reference + `name`): a data breakpoint in
/// the project's breakpoints.
#[tauri::command]
pub async fn dap_data_breakpoint(
    session_id: String,
    container: i64,
    name: String,
    access_type: Option<String>,
    debug: State<'_, DebugState>,
) -> Result<Breakpoint> {
    Ok(session(&debug, &session_id)?
        .add_data_breakpoint(container, &name, access_type)
        .await?)
}

/// Local processes for Attach to Process (Ctrl+Alt+F5), filtered by name, command line
/// or pid.
#[tauri::command]
pub async fn dap_processes(filter: Option<String>) -> Result<Vec<ProcessInfo>> {
    let list = tokio::task::spawn_blocking(forge_dap::list_processes)
        .await
        .map_err(|e| ForgeError::Other(e.to_string()))?;
    Ok(forge_dap::processes::filter_processes(
        &list,
        filter.as_deref().unwrap_or(""),
    ))
}
