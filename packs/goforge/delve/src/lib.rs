//! GoForge's debugger: Delve (`dlv dap`, MIT) on the shared Debug Adapter Protocol layer
//! (`forge-dap`). Nothing is bundled: Forge uses the user's `dlv` (setting, Forge's tools
//! folder, GOPATH/bin, PATH) or installs it with their consent through their own Go
//! toolchain (`go install github.com/go-delve/delve/cmd/dlv@<pinned>`, module proxy and
//! checksum database).
//!
//! - [`DelveAdapter`]: the `go` debug adapter. Each session starts `dlv dap
//!   --listen=127.0.0.1:<port>` and connects to it; attaching to a remote `dlv --headless
//!   --accept-multiclient` server connects straight to it (`mode: remote`).
//! - [`request`]: the launch request of a Go Build (`mode: debug`), Go Test (`mode:
//!   test`), Go Remote (`mode: remote`) configuration.

use std::collections::BTreeMap;

use forge_dap::adapter::merge;
use forge_dap::{
    AdapterCommand, AdapterInfo, DapError, DapResult, DebugAdapterDescriptor, LaunchMode,
    LaunchRequest, ResolvedLaunch, Transport,
};
use forge_runtools::args::split_args;
use forge_runtools::Configuration;
use goforge_lang_go::{build_target, debug_test, GoContext, GO_BUILD, GO_TEST};
use serde_json::{json, Map, Value};

/// The adapter type (also used by launch requests and breakpoints).
pub const ADAPTER: &str = "go";
/// The run configuration type that attaches to a remote `dlv --headless` server.
pub const GO_REMOTE: &str = "go-remote";

/// Where Delve is and the environment it builds with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DelveConfig {
    /// The `dlv` executable; empty: `dlv` on the PATH.
    pub dlv: String,
    /// GOPATH, GOFLAGS and the project's variables; `PATH` with the SDK's `bin` first.
    pub env: Vec<(String, String)>,
}

pub struct DelveAdapter {
    config: DelveConfig,
}

impl DelveAdapter {
    pub fn new(config: DelveConfig) -> Self {
        DelveAdapter { config }
    }

    fn command(&self) -> AdapterCommand {
        let dlv = if self.config.dlv.trim().is_empty() {
            "dlv".to_string()
        } else {
            self.config.dlv.trim().to_string()
        };
        let mut cmd =
            AdapterCommand::new(dlv, vec!["dap".into(), "--listen=127.0.0.1:${port}".into()]);
        cmd.env = self.config.env.clone();
        cmd
    }

    /// The `launch` / `attach` request and its arguments.
    pub fn arguments(&self, r: &LaunchRequest) -> DapResult<(String, Value)> {
        let mut args = Map::new();
        let (request, mode) = match &r.mode {
            LaunchMode::Launch => {
                let mode = r
                    .configuration
                    .get("mode")
                    .and_then(Value::as_str)
                    .unwrap_or("debug")
                    .to_string();
                let program = r.program.clone().filter(|p| !p.is_empty()).ok_or_else(|| {
                    DapError::Other("Nothing to debug: no Go package or program".into())
                })?;
                args.insert("program".into(), json!(program));
                let mut list = r.args.clone();
                if let Some(line) = r.arguments.as_deref() {
                    list.extend(split_args(line));
                }
                args.insert("args".into(), json!(list));
                if let Some(cwd) = r.cwd.as_deref().filter(|c| !c.is_empty()) {
                    args.insert("cwd".into(), json!(cwd));
                }
                if !r.env.is_empty() {
                    args.insert("env".into(), json!(r.env));
                }
                args.insert("stopOnEntry".into(), json!(r.stop_on_entry));
                ("launch", mode)
            }
            LaunchMode::AttachProcess { pid } => {
                args.insert("processId".into(), json!(pid));
                ("attach", "local".to_string())
            }
            LaunchMode::AttachServer { .. } => ("attach", "remote".to_string()),
            LaunchMode::AttachPort { .. } => {
                return Err(DapError::Unsupported(
                    "Delve attaches to a process or to a dlv --headless server".into(),
                ))
            }
            LaunchMode::CoreDump { core } => {
                let program = r.program.clone().filter(|p| !p.is_empty()).ok_or_else(|| {
                    DapError::Other("Choose the executable the core dump belongs to".into())
                })?;
                args.insert("program".into(), json!(program));
                args.insert("coreFilePath".into(), json!(core));
                ("launch", "core".to_string())
            }
        };
        args.insert("name".into(), json!(r.title()));
        args.insert("type".into(), json!(ADAPTER));
        args.insert("request".into(), json!(request));
        args.insert("mode".into(), json!(mode));
        let mut value = Value::Object(args);
        merge(&mut value, &r.configuration);
        Ok((request.to_string(), value))
    }
}

impl DebugAdapterDescriptor for DelveAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            r#type: ADAPTER.into(),
            label: "Go (Delve)".into(),
            languages: vec!["go".into()],
            launch: true,
            attach_process: true,
            attach_port: false,
            core_dump: true,
            command: self.command().command,
            ..AdapterInfo::default()
        }
    }

    fn resolve(&self, r: &LaunchRequest) -> DapResult<ResolvedLaunch> {
        let (request, mut arguments) = self.arguments(r)?;
        let mut command = self.command();
        if matches!(r.mode, LaunchMode::Launch) {
            // Delve runs `go build` in its own working directory: the package's folder.
            let program = r.program.as_deref().unwrap_or_default();
            let is_file = program.ends_with(".go");
            command.cwd = Some(if is_file {
                program
                    .rsplit_once(['/', '\\'])
                    .map(|(d, _)| d.to_string())
                    .unwrap_or_default()
            } else {
                program.to_string()
            })
            .filter(|d| !d.is_empty());
            // The binary goes to the temp folder, never into the project.
            if arguments.get("output").is_none() && arguments["mode"] != "exec" {
                arguments["output"] = json!(debug_binary(&r.title()));
            }
        }
        let transport = match &r.mode {
            LaunchMode::AttachServer { host, port } => Transport::Connect {
                host: host.clone(),
                port: *port,
                adapter: None,
            },
            _ => Transport::Connect {
                host: "127.0.0.1".into(),
                port: 0,
                adapter: Some(command),
            },
        };
        Ok(ResolvedLaunch {
            transport,
            request,
            arguments,
        })
    }
}

/// Where Delve writes the program it builds: `<temp>/forge-delve/__debug_bin_<name>`.
pub fn debug_binary(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    let exe = if cfg!(windows) { ".exe" } else { "" };
    std::env::temp_dir()
        .join("forge-delve")
        .join(format!("__debug_bin_{safe}{exe}"))
        .to_string_lossy()
        .into_owned()
}

fn absolute(root: &str, rel: &str) -> String {
    let rel = rel.trim_start_matches("./").trim_matches('/');
    let root = root.trim_end_matches(['/', '\\']);
    if rel.is_empty() || rel == "." {
        root.to_string()
    } else if rel.starts_with('/') || rel.get(1..2) == Some(":") {
        rel.to_string()
    } else {
        format!("{root}/{rel}")
    }
}

/// The Delve launch request of a Go configuration (`go-build`, `go-test`, `go-remote`).
/// `root` is the project's absolute folder, `env` the configuration's own variables.
pub fn request(
    cfg: &Configuration,
    env: &BTreeMap<String, String>,
    go: &GoContext,
    root: &str,
) -> Result<LaunchRequest, String> {
    let mut environment = go.environment();
    environment.extend(env.clone());
    let opt = |k: &str| cfg.option(k).trim().to_string();
    let mut r = LaunchRequest::launch(ADAPTER, "");
    r.name = Some(cfg.name.clone()).filter(|n| !n.is_empty());
    match cfg.type_id.as_str() {
        GO_BUILD => {
            let target = build_target(cfg, go)?;
            let program = match cfg.option("kind").trim() {
                // One file: Delve builds it like `go run file.go`.
                "file" if target.targets.len() == 1 => absolute(
                    root,
                    &goforge_lang_go::env::join(&target.cwd, &target.targets[0]),
                ),
                _ => absolute(root, &target.package_dir),
            };
            r.program = Some(program);
            r.cwd = Some(absolute(root, &target.cwd));
            r.arguments = Some(cfg.arguments.clone()).filter(|a| !a.trim().is_empty());
            let mut flags = Vec::new();
            let tags = go.tags(&opt("tags"));
            if !tags.is_empty() {
                flags.push(format!("-tags={}", tags.join(",")));
            }
            flags.extend(split_args(&opt("flags")));
            let mut c = json!({ "mode": "debug" });
            if !flags.is_empty() {
                c["buildFlags"] = json!(flags.join(" "));
            }
            if !opt("output").is_empty() {
                c["output"] = json!(absolute(root, &opt("output")));
            }
            r.configuration = c;
        }
        GO_TEST => {
            let t = debug_test(cfg, go)?;
            let dir = absolute(root, &t.package_dir);
            r.program = Some(dir.clone());
            // `go test` runs a package's tests in the package folder.
            r.cwd = Some(dir);
            r.args = t.args;
            let mut c = json!({ "mode": "test" });
            if !t.build_flags.is_empty() {
                c["buildFlags"] = json!(t.build_flags.join(" "));
            }
            r.configuration = c;
        }
        GO_REMOTE => {
            let port: u16 = opt("port")
                .parse()
                .map_err(|_| "Enter the port of the dlv --headless server".to_string())?;
            let host = match opt("host") {
                h if h.is_empty() => "localhost".to_string(),
                h => h,
            };
            r.mode = LaunchMode::AttachServer { host, port };
            r.program = None;
            let remote = opt("remotePath");
            if !remote.is_empty() {
                r.configuration = json!({
                    "substitutePath": [{ "from": root.trim_end_matches(['/', '\\']), "to": remote.trim_end_matches('/') }]
                });
            }
        }
        other => {
            return Err(format!(
                "'{other}' configurations are not debugged with Delve"
            ))
        }
    }
    r.env = environment;
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use goforge_lang_go::ModuleRoot;

    fn adapter() -> DelveAdapter {
        DelveAdapter::new(DelveConfig {
            dlv: "/home/u/go/bin/dlv".into(),
            env: vec![("GOFLAGS".into(), "-mod=mod".into())],
        })
    }

    fn go() -> GoContext {
        GoContext {
            build_tags: vec!["dev".into()],
            gopath: "/home/u/go".into(),
            modules: vec![ModuleRoot {
                path: "github.com/acme/ledger".into(),
                dir: "".into(),
            }],
            ..GoContext::default()
        }
    }

    #[test]
    fn launch_starts_dlv_dap_and_debugs_a_package() {
        let mut cfg = Configuration::new("ledger", GO_BUILD)
            .with_option("package", "./cmd/ledger")
            .with_option("flags", "-race");
        cfg.arguments = "--port 8080".into();
        let env = BTreeMap::from([("LEDGER_DB".to_string(), "mem".to_string())]);
        let r = request(&cfg, &env, &go(), "/w/ledger").unwrap();
        assert!(r.runs_code());
        assert_eq!(r.env["GOPATH"], "/home/u/go");
        assert_eq!(r.env["LEDGER_DB"], "mem");
        let resolved = adapter().resolve(&r).unwrap();
        let Transport::Connect {
            host,
            port,
            adapter: Some(cmd),
        } = &resolved.transport
        else {
            panic!("{:?}", resolved.transport)
        };
        assert_eq!((host.as_str(), *port), ("127.0.0.1", 0));
        assert_eq!(cmd.command, "/home/u/go/bin/dlv");
        assert_eq!(cmd.args, ["dap", "--listen=127.0.0.1:${port}"]);
        assert_eq!(cmd.env, [("GOFLAGS".to_string(), "-mod=mod".to_string())]);
        assert_eq!(cmd.cwd.as_deref(), Some("/w/ledger/cmd/ledger"));
        assert!(resolved.arguments["output"]
            .as_str()
            .unwrap()
            .contains("forge-delve"));
        assert_eq!(resolved.request, "launch");
        let a = &resolved.arguments;
        assert_eq!(a["mode"], "debug");
        assert_eq!(a["program"], "/w/ledger/cmd/ledger");
        assert_eq!(a["cwd"], "/w/ledger");
        assert_eq!(a["args"], json!(["--port", "8080"]));
        assert_eq!(a["buildFlags"], "-tags=dev -race");
        assert_eq!(a["type"], "go");
        assert_eq!(a["name"], "ledger");
    }

    #[test]
    fn test_mode_for_go_test_configurations() {
        let cfg = Configuration::new("TestRound in money", GO_TEST)
            .with_option("scope", "test")
            .with_option("file", "internal/money/money_test.go")
            .with_option("test", "TestRound/negative");
        let r = request(&cfg, &BTreeMap::new(), &go(), "/w/ledger").unwrap();
        let a = adapter().resolve(&r).unwrap().arguments;
        assert_eq!(a["mode"], "test");
        assert_eq!(a["program"], "/w/ledger/internal/money");
        assert_eq!(a["cwd"], "/w/ledger/internal/money");
        assert_eq!(
            a["args"],
            json!(["-test.run=^TestRound$/^negative$", "-test.v"])
        );
        assert_eq!(a["buildFlags"], "-tags=dev");
        let file = Configuration::new("main.go", GO_BUILD)
            .with_option("kind", "file")
            .with_option("file", "cmd/tool/main.go");
        let r = request(&file, &BTreeMap::new(), &go(), "/w/ledger").unwrap();
        assert_eq!(r.program.as_deref(), Some("/w/ledger/cmd/tool/main.go"));
    }

    #[test]
    fn attach_remote_and_core_dumps() {
        let remote = Configuration::new("staging", GO_REMOTE)
            .with_option("host", "10.0.0.5")
            .with_option("port", "2345")
            .with_option("remotePath", "/app/");
        let r = request(&remote, &BTreeMap::new(), &go(), "/w/ledger").unwrap();
        assert!(r.is_attach() && !r.runs_code());
        let resolved = adapter().resolve(&r).unwrap();
        assert_eq!(
            resolved.transport,
            Transport::Connect {
                host: "10.0.0.5".into(),
                port: 2345,
                adapter: None
            }
        );
        assert_eq!(resolved.request, "attach");
        assert_eq!(resolved.arguments["mode"], "remote");
        assert_eq!(
            resolved.arguments["substitutePath"],
            json!([{ "from": "/w/ledger", "to": "/app" }])
        );
        assert!(request(
            &Configuration::new("x", GO_REMOTE),
            &BTreeMap::new(),
            &go(),
            "/w"
        )
        .is_err());

        let mut pid = LaunchRequest::launch(ADAPTER, "");
        pid.program = None;
        pid.mode = LaunchMode::AttachProcess { pid: 4242 };
        let a = adapter().resolve(&pid).unwrap();
        assert_eq!(a.request, "attach");
        assert_eq!(a.arguments["mode"], "local");
        assert_eq!(a.arguments["processId"], 4242);

        let mut core = LaunchRequest::launch(ADAPTER, "/w/ledger/bin/ledger");
        core.mode = LaunchMode::CoreDump {
            core: "/tmp/core.1".into(),
        };
        let a = adapter().resolve(&core).unwrap().arguments;
        assert_eq!(
            (a["mode"].as_str(), a["coreFilePath"].as_str()),
            (Some("core"), Some("/tmp/core.1"))
        );

        let mut port = pid.clone();
        port.mode = LaunchMode::AttachPort {
            host: "h".into(),
            port: 1,
        };
        assert!(adapter().resolve(&port).is_err());
        let info = adapter().info();
        assert!(info.launch && info.attach_process && info.core_dump && !info.attach_port);
        assert_eq!(info.languages, ["go"]);
    }
}
