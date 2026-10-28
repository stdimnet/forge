//! RubyForge's debugger: rdbg of the `debug` gem (BSD-2-Clause, Ruby's default debugger)
//! on the shared Debug Adapter Protocol layer (`forge-dap`). rdbg is its own DAP server:
//!
//! - Debug (Shift+F9) on a Ruby, Rake, RSpec or Minitest configuration runs the command in
//!   the Run tool window under `rdbg --open --host 127.0.0.1 --port <port> --command --
//!   <command>` (the program waits for the debugger, its output stays in the Run console),
//!   then attaches to that port;
//! - "Attach to DAP server" connects to any `rdbg --open --port <port>` the user started.
//!
//! Attach arguments: `localfs` (the debuggee runs on this file system, so breakpoint paths
//! are its paths; `localfsMap` from the configuration for containers) and `nonstop` (go on
//! after the configuration is done unless Stop on entry is set).

use forge_dap::adapter::merge;
use forge_dap::{
    AdapterInfo, DapError, DapResult, DebugAdapterDescriptor, LaunchMode, LaunchRequest,
    ResolvedLaunch, Transport,
};
use serde_json::{json, Value};

/// The adapter type (launch requests, breakpoints).
pub const ADAPTER: &str = "rdbg";

/// The `rdbg` adapter.
#[derive(Debug, Clone, Default)]
pub struct RdbgAdapter {
    /// The `rdbg` executable Forge found (shown in the adapter list).
    pub rdbg: String,
}

impl RdbgAdapter {
    pub fn new(rdbg: impl Into<String>) -> Self {
        Self { rdbg: rdbg.into() }
    }
}

fn is_local(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "")
}

/// The `attach` arguments of a request.
pub fn attach_arguments(r: &LaunchRequest, host: &str) -> Value {
    let mut args = json!({
        "type": ADAPTER,
        "request": "attach",
        "name": r.title(),
        "nonstop": !r.stop_on_entry,
    });
    if is_local(host) && r.configuration.get("localfsMap").is_none() {
        args["localfs"] = json!(true);
    }
    merge(&mut args, &r.configuration);
    args
}

impl DebugAdapterDescriptor for RdbgAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            r#type: ADAPTER.into(),
            label: "Ruby (rdbg)".into(),
            languages: vec!["rb".into(), "rake".into(), "ru".into(), "gemspec".into()],
            launch: false,
            attach_process: false,
            attach_port: false,
            core_dump: false,
            command: self.rdbg.clone(),
            ..AdapterInfo::default()
        }
    }

    /// rdbg maps breakpoint paths once `attach` says the file system is shared
    /// (`localfs`), but announces `initialized` right after `initialize`.
    fn breakpoints_after_request(&self) -> bool {
        true
    }

    fn resolve(&self, r: &LaunchRequest) -> DapResult<ResolvedLaunch> {
        match &r.mode {
            LaunchMode::AttachServer { host, port } => Ok(ResolvedLaunch {
                transport: Transport::Connect {
                    host: host.clone(),
                    port: *port,
                    adapter: None,
                },
                request: "attach".into(),
                arguments: attach_arguments(r, host),
            }),
            _ => Err(DapError::Unsupported(
                "rdbg debugs Run/Debug configurations (Debug) or attaches to an `rdbg --open --port` server (Attach to DAP Server)".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attach(port: u16) -> LaunchRequest {
        let mut r = LaunchRequest::launch(ADAPTER, "");
        r.mode = LaunchMode::AttachServer {
            host: "127.0.0.1".into(),
            port,
        };
        r.program = None;
        r.name = Some("spec".into());
        r
    }

    #[test]
    fn attaches_to_the_rdbg_server() {
        let a = RdbgAdapter::new("/gems/bin/rdbg");
        assert_eq!(a.info().r#type, "rdbg");
        let r = a.resolve(&attach(40123)).unwrap();
        assert_eq!(r.request, "attach");
        assert_eq!(
            r.transport,
            Transport::Connect {
                host: "127.0.0.1".into(),
                port: 40123,
                adapter: None
            }
        );
        assert_eq!(r.arguments["nonstop"], json!(true));
        assert_eq!(r.arguments["localfs"], json!(true));
        let mut stop = attach(1);
        stop.stop_on_entry = true;
        stop.configuration = json!({ "localfsMap": "/app:/home/anna/shop" });
        let r = a.resolve(&stop).unwrap();
        assert_eq!(r.arguments["nonstop"], json!(false));
        assert!(r.arguments.get("localfs").is_none());
        assert!(a
            .resolve(&LaunchRequest::launch(ADAPTER, "app.rb"))
            .is_err());
    }
}
