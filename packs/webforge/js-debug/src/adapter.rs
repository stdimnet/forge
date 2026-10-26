//! js-debug as Forge debug adapters: `pwa-node` (Node.js programs, npm scripts, tests,
//! attach by process or port), `pwa-chrome` and `pwa-msedge` (pages in Chrome or Edge,
//! attach to a remote debugging port).
//!
//! Each session starts js-debug's DAP server (`node dapDebugServer.js <port> 127.0.0.1`) and
//! connects to it. js-debug runs the real debuggee in a child session that it asks for with
//! the `startDebugging` reverse request; forge-dap connects that child to the same server.
//! Source maps (TypeScript, bundlers) are js-debug's: breakpoints set in `.ts` files bind
//! to the generated code.

use std::path::PathBuf;

use forge_dap::adapter::merge;
use forge_dap::{
    AdapterCommand, AdapterInfo, DapError, DapResult, DebugAdapterDescriptor, LaunchMode,
    LaunchRequest, ResolvedLaunch, Transport,
};
use serde_json::{json, Value};

/// File extensions whose line breakpoints js-debug handles (through source maps for the
/// compiled ones).
pub const LANGUAGES: &[&str] = &[
    "js", "mjs", "cjs", "jsx", "ts", "mts", "cts", "tsx", "vue", "svelte", "astro",
];

/// What a js-debug adapter debugs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Node,
    Chrome,
    Edge,
}

impl Target {
    pub const ALL: [Target; 3] = [Target::Node, Target::Chrome, Target::Edge];

    pub fn adapter_type(self) -> &'static str {
        match self {
            Target::Node => "pwa-node",
            Target::Chrome => "pwa-chrome",
            Target::Edge => "pwa-msedge",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Target::Node => "Node.js (js-debug)",
            Target::Chrome => "Chrome (js-debug)",
            Target::Edge => "Edge (js-debug)",
        }
    }
}

/// Where js-debug comes from and how it runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JsDebugConfig {
    /// Node.js that runs the DAP server (and the debuggee when a request names none).
    pub node: String,
    /// `dapDebugServer.js`; `None` while js-debug is not installed.
    pub server: Option<PathBuf>,
    /// Scripts the debugger steps over (`<node_internals>/**`).
    pub skip_files: Vec<String>,
}

/// One js-debug adapter type.
#[derive(Debug, Clone)]
pub struct JsDebugAdapter {
    pub target: Target,
    pub config: JsDebugConfig,
}

impl JsDebugAdapter {
    pub fn new(target: Target, config: JsDebugConfig) -> Self {
        Self { target, config }
    }

    /// The adapters of every target.
    pub fn all(config: &JsDebugConfig) -> Vec<JsDebugAdapter> {
        Target::ALL
            .into_iter()
            .map(|t| JsDebugAdapter::new(t, config.clone()))
            .collect()
    }

    fn server_command(&self) -> DapResult<AdapterCommand> {
        let server = self.config.server.as_ref().ok_or_else(|| {
            DapError::NotFound(
                "The JavaScript debugger (js-debug) is not installed. Download it with Run | Download JavaScript Debugger, or set its folder in Settings | Languages & Frameworks | Node.js.".into(),
            )
        })?;
        let node = match self.config.node.trim() {
            "" => "node",
            n => n,
        };
        Ok(AdapterCommand::new(
            node,
            vec![
                server.to_string_lossy().into_owned(),
                "${port}".into(),
                "127.0.0.1".into(),
            ],
        ))
    }

    /// The `launch` / `attach` arguments of a request.
    pub fn arguments(&self, r: &LaunchRequest) -> DapResult<(String, Value)> {
        let node = self.target == Target::Node;
        let mut a = json!({
            "type": self.target.adapter_type(),
            "name": r.title(),
            "sourceMaps": true,
            "skipFiles": self.config.skip_files,
        });
        let cwd = r.cwd.clone().filter(|c| !c.is_empty());
        let request = match &r.mode {
            LaunchMode::Launch => {
                if node {
                    let mut args = r.args.clone();
                    if let Some(line) = r.arguments.as_deref() {
                        args.extend(forge_runtools::args::split_args(line));
                    }
                    if let Some(p) = r.program.as_deref().filter(|p| !p.is_empty()) {
                        a["program"] = json!(p);
                    }
                    a["args"] = json!(args);
                    a["env"] = json!(r.env);
                    a["stopOnEntry"] = json!(r.stop_on_entry);
                    a["console"] = json!("internalConsole");
                    // stdout / stderr of the process, not only console.log.
                    a["outputCapture"] = json!("std");
                    a["autoAttachChildProcesses"] = json!(true);
                    if !self.config.node.trim().is_empty() {
                        a["runtimeExecutable"] = json!(self.config.node.trim());
                    }
                    if let Some(c) = &cwd {
                        a["cwd"] = json!(c);
                        a["__workspaceFolder"] = json!(c);
                        // Compiled scripts whose source maps bind breakpoints in `.ts` files
                        // before the scripts load.
                        a["outFiles"] = json!([
                            format!("{}/**/*.(m|c|)js", c.trim_end_matches(['/', '\\'])),
                            "!**/node_modules/**"
                        ]);
                    }
                } else {
                    if let Some(url) = r.program.as_deref().filter(|p| !p.is_empty()) {
                        a["url"] = json!(url);
                    }
                    if let Some(c) = &cwd {
                        a["webRoot"] = json!(c);
                    }
                }
                "launch"
            }
            LaunchMode::AttachProcess { pid } => {
                if !node {
                    return Err(DapError::Unsupported(format!(
                        "{} cannot attach to processes; attach to its remote debugging port",
                        self.target.label()
                    )));
                }
                a["processId"] = json!(pid.to_string());
                if let Some(c) = &cwd {
                    a["cwd"] = json!(c);
                }
                "attach"
            }
            LaunchMode::AttachPort { host, port } => {
                a["address"] = json!(host);
                a["port"] = json!(port);
                if let Some(c) = &cwd {
                    a[if node { "cwd" } else { "webRoot" }] = json!(c);
                }
                "attach"
            }
            LaunchMode::AttachServer { .. } => r
                .configuration
                .get("request")
                .and_then(Value::as_str)
                .unwrap_or("launch"),
            LaunchMode::CoreDump { .. } => {
                return Err(DapError::Unsupported(
                    "js-debug cannot open core dumps".into(),
                ))
            }
        }
        .to_string();
        a["request"] = json!(request);
        merge(&mut a, &r.configuration);
        Ok((request, a))
    }
}

impl DebugAdapterDescriptor for JsDebugAdapter {
    fn info(&self) -> AdapterInfo {
        let installed = self.config.server.is_some();
        AdapterInfo {
            r#type: self.target.adapter_type().into(),
            label: self.target.label().into(),
            languages: LANGUAGES.iter().map(|l| l.to_string()).collect(),
            launch: installed,
            attach_process: installed && self.target == Target::Node,
            attach_port: installed,
            core_dump: false,
            command: self.config.node.clone(),
            launch_schema: Value::Null,
            attach_schema: Value::Null,
        }
    }

    fn resolve(&self, r: &LaunchRequest) -> DapResult<ResolvedLaunch> {
        let (request, arguments) = self.arguments(r)?;
        let transport = match &r.mode {
            LaunchMode::AttachServer { host, port } => Transport::Connect {
                host: host.clone(),
                port: *port,
                adapter: None,
            },
            _ => Transport::Connect {
                host: "127.0.0.1".into(),
                port: 0,
                adapter: Some(self.server_command()?),
            },
        };
        Ok(ResolvedLaunch {
            transport,
            request,
            arguments,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> JsDebugConfig {
        JsDebugConfig {
            node: "/opt/node/bin/node".into(),
            server: Some(PathBuf::from(
                "/data/js-debug/1.140.0/src/dapDebugServer.js",
            )),
            skip_files: vec!["<node_internals>/**".into()],
        }
    }

    #[test]
    fn node_launch_starts_the_dap_server_and_its_program() {
        let a = JsDebugAdapter::new(Target::Node, config());
        let mut r = LaunchRequest::launch("pwa-node", "/w/src/server.ts");
        r.arguments = Some("--port 8080".into());
        r.cwd = Some("/w".into());
        r.env.insert("DEBUG".into(), "shop:*".into());
        r.configuration = json!({ "runtimeArgs": ["--import", "tsx"] });
        let resolved = a.resolve(&r).unwrap();
        let Transport::Connect {
            host,
            port,
            adapter: Some(cmd),
        } = &resolved.transport
        else {
            panic!("{:?}", resolved.transport)
        };
        assert_eq!((host.as_str(), *port), ("127.0.0.1", 0));
        assert_eq!(cmd.command, "/opt/node/bin/node");
        assert_eq!(
            cmd.args,
            [
                "/data/js-debug/1.140.0/src/dapDebugServer.js",
                "${port}",
                "127.0.0.1"
            ]
        );
        let args = &resolved.arguments;
        assert_eq!(resolved.request, "launch");
        assert_eq!(args["type"], "pwa-node");
        assert_eq!(args["request"], "launch");
        assert_eq!(args["program"], "/w/src/server.ts");
        assert_eq!(args["args"], json!(["--port", "8080"]));
        assert_eq!(args["cwd"], "/w");
        assert_eq!(args["env"]["DEBUG"], "shop:*");
        assert_eq!(args["runtimeArgs"], json!(["--import", "tsx"]));
        assert_eq!(args["runtimeExecutable"], "/opt/node/bin/node");
        assert_eq!(args["sourceMaps"], true);
        assert_eq!(args["skipFiles"], json!(["<node_internals>/**"]));
        assert_eq!(args["name"], "server.ts");
        let info = a.info();
        assert!(info.launch && info.attach_process && info.attach_port);
        assert!(info.languages.contains(&"ts".to_string()));
    }

    #[test]
    fn chrome_launch_and_attach() {
        let a = JsDebugAdapter::new(Target::Chrome, config());
        let mut r = LaunchRequest::launch("pwa-chrome", "http://localhost:5173/");
        r.cwd = Some("/w".into());
        r.name = Some("App".into());
        r.configuration = json!({ "runtimeExecutable": "/usr/bin/chromium" });
        let (request, args) = a.arguments(&r).unwrap();
        assert_eq!(request, "launch");
        assert_eq!(args["url"], "http://localhost:5173/");
        assert_eq!(args["webRoot"], "/w");
        assert_eq!(args["runtimeExecutable"], "/usr/bin/chromium");
        assert!(args.get("program").is_none());
        r.mode = LaunchMode::AttachPort {
            host: "localhost".into(),
            port: 9222,
        };
        let (request, args) = a.arguments(&r).unwrap();
        assert_eq!(request, "attach");
        assert_eq!(
            (args["address"].as_str(), args["port"].as_u64()),
            (Some("localhost"), Some(9222))
        );
        r.mode = LaunchMode::AttachProcess { pid: 1 };
        assert!(a.resolve(&r).is_err());
        assert!(!a.info().attach_process);
        let edge = JsDebugAdapter::new(Target::Edge, config());
        assert_eq!(edge.info().r#type, "pwa-msedge");
    }

    #[test]
    fn node_attach_by_process_and_port() {
        let a = JsDebugAdapter::new(Target::Node, config());
        let mut r = LaunchRequest::launch("pwa-node", "");
        r.program = None;
        r.mode = LaunchMode::AttachProcess { pid: 4242 };
        let (request, args) = a.arguments(&r).unwrap();
        assert_eq!(
            (request.as_str(), &args["processId"]),
            ("attach", &json!("4242"))
        );
        r.mode = LaunchMode::AttachPort {
            host: "127.0.0.1".into(),
            port: 9229,
        };
        let resolved = a.resolve(&r).unwrap();
        assert_eq!(resolved.arguments["port"], 9229);
        assert!(matches!(
            resolved.transport,
            Transport::Connect {
                adapter: Some(_),
                ..
            }
        ));
        r.mode = LaunchMode::AttachServer {
            host: "127.0.0.1".into(),
            port: 8123,
        };
        let resolved = a.resolve(&r).unwrap();
        assert_eq!(
            resolved.transport,
            Transport::Connect {
                host: "127.0.0.1".into(),
                port: 8123,
                adapter: None
            }
        );
    }

    #[test]
    fn without_js_debug_nothing_starts() {
        let a = JsDebugAdapter::new(
            Target::Node,
            JsDebugConfig {
                server: None,
                ..config()
            },
        );
        assert!(!a.info().launch);
        let err = a
            .resolve(&LaunchRequest::launch("pwa-node", "/w/a.js"))
            .unwrap_err();
        assert!(err.to_string().contains("not installed"), "{err}");
    }
}
