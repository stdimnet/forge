//! Debug adapter descriptors: how to start an adapter and which `launch` / `attach`
//! arguments it gets. Packs register descriptors in Rust ([`DebugAdapterDescriptor`]) or
//! declare them in their manifest (`[[contributions.debugAdapters]]`, [`ManifestAdapter`]).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use forge_plugin_api::{DebugAdapterContribution, DebugAdapterTransport};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::error::{DapError, DapResult};
use crate::transport::{AdapterCommand, Transport};

/// An adapter as the UI lists it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterInfo {
    pub r#type: String,
    pub label: String,
    /// File extensions whose line breakpoints the adapter handles.
    #[serde(default)]
    pub languages: Vec<String>,
    /// Can start programs (`launch`).
    #[serde(default)]
    pub launch: bool,
    /// Can attach to a local process by pid.
    #[serde(default)]
    pub attach_process: bool,
    /// Can open core dumps.
    #[serde(default)]
    pub core_dump: bool,
    /// The adapter executable (empty: only reachable as a DAP server).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub launch_schema: Value,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub attach_schema: Value,
}

/// What a debug session should do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum LaunchMode {
    /// Start `program` under the debugger.
    Launch,
    /// Attach to a running local process ("Attach to Process…").
    AttachProcess { pid: u32 },
    /// Connect to a debug adapter that already runs as a server ("Attach to DAP server…").
    AttachServer { host: String, port: u16 },
    /// Open a core dump of `program` ("Debug Core Dump…").
    CoreDump { core: String },
}

/// A request to start a debug session, from a run configuration or a dialog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRequest {
    /// Adapter type (`lldb`).
    pub adapter: String,
    /// Session title; defaults to the program's file name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub mode: LaunchMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub stop_on_entry: bool,
    /// Extra adapter-specific arguments, merged over the template (a run configuration's
    /// "launch.json" part).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub configuration: Value,
}

impl LaunchRequest {
    pub fn launch(adapter: &str, program: &str) -> Self {
        Self {
            adapter: adapter.into(),
            name: None,
            mode: LaunchMode::Launch,
            program: Some(program.into()),
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            stop_on_entry: false,
            configuration: Value::Null,
        }
    }

    /// Whether the request starts a program, which runs project code (refused in Safe Mode).
    pub fn runs_code(&self) -> bool {
        matches!(self.mode, LaunchMode::Launch)
    }

    pub fn is_attach(&self) -> bool {
        matches!(
            self.mode,
            LaunchMode::AttachProcess { .. } | LaunchMode::AttachServer { .. }
        )
    }

    /// The session title.
    pub fn title(&self) -> String {
        if let Some(n) = self.name.as_deref().filter(|n| !n.is_empty()) {
            return n.to_string();
        }
        let program = self
            .program
            .as_deref()
            .and_then(|p| p.rsplit(['/', '\\']).next())
            .filter(|p| !p.is_empty());
        match (&self.mode, program) {
            (LaunchMode::AttachProcess { pid }, Some(p)) => format!("{p} ({pid})"),
            (LaunchMode::AttachProcess { pid }, None) => format!("Process {pid}"),
            (LaunchMode::AttachServer { host, port }, None) => format!("{host}:{port}"),
            (LaunchMode::CoreDump { core }, None) => {
                core.rsplit(['/', '\\']).next().unwrap_or(core).to_string()
            }
            (_, Some(p)) => p.to_string(),
            (LaunchMode::Launch, None) => self.adapter.clone(),
        }
    }
}

/// How to reach the adapter and the first request after `initialize`.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedLaunch {
    pub transport: Transport,
    /// `launch` or `attach`.
    pub request: String,
    pub arguments: Value,
}

/// A kind of debug adapter. Implement it in a pack crate for adapters that need code to
/// start (downloads, Python environments); simple ones are declared in the manifest.
pub trait DebugAdapterDescriptor: Send + Sync {
    fn info(&self) -> AdapterInfo;
    /// Turn a request into the transport and the `launch`/`attach` arguments.
    fn resolve(&self, request: &LaunchRequest) -> DapResult<ResolvedLaunch>;
}

/// Values for template placeholders.
fn placeholders(r: &LaunchRequest) -> HashMap<&'static str, Value> {
    let mut v = HashMap::new();
    if let Some(p) = r.program.as_deref().filter(|p| !p.is_empty()) {
        v.insert("program", json!(p));
    }
    v.insert("args", json!(r.args));
    if let Some(c) = r.cwd.as_deref().filter(|c| !c.is_empty()) {
        v.insert("cwd", json!(c));
    }
    v.insert("env", json!(r.env));
    v.insert("stopOnEntry", json!(r.stop_on_entry));
    v.insert("name", json!(r.title()));
    match &r.mode {
        LaunchMode::AttachProcess { pid } => {
            v.insert("pid", json!(pid));
        }
        LaunchMode::AttachServer { host, port } => {
            v.insert("host", json!(host));
            v.insert("port", json!(port));
        }
        LaunchMode::CoreDump { core } => {
            v.insert("core", json!(core));
        }
        LaunchMode::Launch => {}
    }
    v
}

/// Expand a request template: a string that is exactly one placeholder takes the value's
/// JSON type, placeholders inside strings are replaced, and entries whose placeholder has
/// no value are dropped.
pub fn expand_template(template: &Value, values: &HashMap<&'static str, Value>) -> Option<Value> {
    match template {
        Value::String(s) => {
            if let Some(name) = s.strip_prefix("${").and_then(|r| r.strip_suffix('}')) {
                if !name.contains("${") {
                    return values.get(name).cloned();
                }
            }
            let mut out = String::new();
            let mut rest = s.as_str();
            while let Some(start) = rest.find("${") {
                out.push_str(&rest[..start]);
                let after = &rest[start + 2..];
                let end = after.find('}')?;
                let value = values.get(&after[..end])?;
                match value {
                    Value::String(v) => out.push_str(v),
                    other => out.push_str(&other.to_string()),
                }
                rest = &after[end + 1..];
            }
            out.push_str(rest);
            Some(Value::String(out))
        }
        Value::Array(items) => Some(Value::Array(
            items
                .iter()
                .filter_map(|i| expand_template(i, values))
                .collect(),
        )),
        Value::Object(map) => Some(Value::Object(
            map.iter()
                .filter_map(|(k, v)| expand_template(v, values).map(|v| (k.clone(), v)))
                .collect(),
        )),
        other => Some(other.clone()),
    }
}

/// Merge `extra` into `base` (objects recursively, everything else replaced).
pub fn merge(base: &mut Value, extra: &Value) {
    match (base, extra) {
        (Value::Object(b), Value::Object(e)) => {
            for (k, v) in e {
                merge(b.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (b, e) if !e.is_null() => *b = e.clone(),
        _ => {}
    }
}

/// An adapter declared with `[[contributions.debugAdapters]]`.
pub struct ManifestAdapter {
    spec: DebugAdapterContribution,
    /// The adapter executable (the `commandSetting` value, else `command`).
    command: String,
}

impl ManifestAdapter {
    pub fn new(spec: DebugAdapterContribution) -> Self {
        let command = spec.command.clone();
        Self { spec, command }
    }

    /// Use another executable (the user's path from `commandSetting`).
    pub fn with_command(mut self, command: Option<String>) -> Self {
        if let Some(c) = command.filter(|c| !c.trim().is_empty()) {
            self.command = c.trim().to_string();
        }
        self
    }

    fn adapter_command(&self) -> DapResult<AdapterCommand> {
        if self.command.is_empty() {
            return Err(DapError::Unsupported(format!(
                "{} can only be attached to as a running DAP server",
                self.spec.label
            )));
        }
        Ok(AdapterCommand::new(&self.command, self.spec.args.clone()))
    }
}

impl DebugAdapterDescriptor for ManifestAdapter {
    fn info(&self) -> AdapterInfo {
        let s = &self.spec;
        let has_command = !self.command.is_empty();
        AdapterInfo {
            r#type: s.r#type.clone(),
            label: s.label.clone(),
            languages: s.languages.clone(),
            launch: has_command && !s.launch.is_null(),
            attach_process: has_command && !s.attach.is_null(),
            core_dump: has_command && !s.core_dump.is_null(),
            command: self.command.clone(),
            launch_schema: s.launch_schema.clone(),
            attach_schema: s.attach_schema.clone(),
        }
    }

    fn resolve(&self, r: &LaunchRequest) -> DapResult<ResolvedLaunch> {
        let s = &self.spec;
        let values = placeholders(r);
        let (template, request) = match &r.mode {
            LaunchMode::Launch => (&s.launch, "launch".to_string()),
            LaunchMode::AttachProcess { .. } => (&s.attach, "attach".to_string()),
            LaunchMode::AttachServer { .. } => (&Value::Null, "attach".to_string()),
            LaunchMode::CoreDump { .. } => (
                &s.core_dump,
                s.core_dump_request
                    .clone()
                    .unwrap_or_else(|| "launch".into()),
            ),
        };
        if template.is_null() && !matches!(r.mode, LaunchMode::AttachServer { .. }) {
            let what = match r.mode {
                LaunchMode::Launch => "start programs",
                LaunchMode::AttachProcess { .. } => "attach to processes",
                _ => "open core dumps",
            };
            return Err(DapError::Unsupported(format!("{} cannot {what}", s.label)));
        }
        let mut arguments = expand_template(template, &values)
            .filter(Value::is_object)
            .unwrap_or_else(|| Value::Object(Map::new()));
        if let Some(obj) = arguments.as_object_mut() {
            obj.entry("name").or_insert_with(|| json!(r.title()));
            obj.entry("type").or_insert_with(|| json!(s.r#type));
            obj.entry("request").or_insert_with(|| json!(request));
        }
        merge(&mut arguments, &r.configuration);
        let transport = match &r.mode {
            LaunchMode::AttachServer { host, port } => Transport::Connect {
                host: host.clone(),
                port: *port,
                adapter: None,
            },
            _ => match s.transport {
                DebugAdapterTransport::Stdio => Transport::Stdio {
                    adapter: self.adapter_command()?,
                },
                DebugAdapterTransport::Connect => Transport::Connect {
                    host: "127.0.0.1".into(),
                    port: 0,
                    adapter: Some(self.adapter_command()?),
                },
                DebugAdapterTransport::Listen => Transport::Listen {
                    adapter: self.adapter_command()?,
                },
            },
        };
        Ok(ResolvedLaunch {
            transport,
            request,
            arguments,
        })
    }
}

/// All known adapters, by type. Later registrations of a type replace earlier ones.
#[derive(Default, Clone)]
pub struct AdapterRegistry {
    adapters: BTreeMap<String, Arc<dyn DebugAdapterDescriptor>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, adapter: Arc<dyn DebugAdapterDescriptor>) {
        self.adapters.insert(adapter.info().r#type, adapter);
    }

    pub fn register_manifest(&mut self, spec: DebugAdapterContribution, command: Option<String>) {
        self.register(Arc::new(ManifestAdapter::new(spec).with_command(command)));
    }

    pub fn get(&self, r#type: &str) -> DapResult<Arc<dyn DebugAdapterDescriptor>> {
        self.adapters.get(r#type).cloned().ok_or_else(|| {
            DapError::NotFound(format!(
                "No debug adapter of type '{type}' is installed. Enable the plugin that provides it."
            ))
        })
    }

    pub fn list(&self) -> Vec<AdapterInfo> {
        self.adapters.values().map(|a| a.info()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lldb() -> DebugAdapterContribution {
        DebugAdapterContribution {
            r#type: "lldb".into(),
            label: "LLDB".into(),
            command: "lldb-dap".into(),
            languages: vec!["c".into()],
            launch: json!({"program": "${program}", "args": "${args}", "cwd": "${cwd}",
                           "env": "${env}", "stopOnEntry": "${stopOnEntry}",
                           "initCommands": ["settings set target.x ${program}"]}),
            attach: json!({"pid": "${pid}", "program": "${program}"}),
            core_dump: json!({"program": "${program}", "coreFile": "${core}"}),
            core_dump_request: Some("attach".into()),
            ..Default::default()
        }
    }

    #[test]
    fn launch_template_expands_typed_values() {
        let a = ManifestAdapter::new(lldb());
        let mut r = LaunchRequest::launch("lldb", "/w/demo");
        r.args = vec!["--fast".into()];
        r.env.insert("A".into(), "1".into());
        r.stop_on_entry = true;
        r.configuration = json!({"sourceMap": {"/b": "/w"}, "stopOnEntry": false});
        let resolved = a.resolve(&r).unwrap();
        assert_eq!(resolved.request, "launch");
        let args = resolved.arguments;
        assert_eq!(args["program"], "/w/demo");
        assert_eq!(args["args"], json!(["--fast"]));
        assert_eq!(args["env"], json!({"A": "1"}));
        assert!(args.get("cwd").is_none(), "no cwd: dropped");
        assert_eq!(args["stopOnEntry"], false, "configuration wins");
        assert_eq!(args["initCommands"][0], "settings set target.x /w/demo");
        assert_eq!(args["sourceMap"]["/b"], "/w");
        assert_eq!(args["type"], "lldb");
        assert_eq!(args["name"], "demo");
        assert!(matches!(resolved.transport, Transport::Stdio { .. }));
    }

    #[test]
    fn attach_core_dump_and_server_modes() {
        let a = ManifestAdapter::new(lldb()).with_command(Some("/opt/llvm/bin/lldb-dap".into()));
        let mut r = LaunchRequest::launch("lldb", "/w/demo");
        r.mode = LaunchMode::AttachProcess { pid: 4242 };
        let resolved = a.resolve(&r).unwrap();
        assert_eq!(resolved.request, "attach");
        assert_eq!(resolved.arguments["pid"], 4242);
        let Transport::Stdio { adapter } = resolved.transport else {
            panic!()
        };
        assert_eq!(adapter.command, "/opt/llvm/bin/lldb-dap");
        assert_eq!(r.title(), "demo (4242)");

        r.mode = LaunchMode::CoreDump {
            core: "/tmp/core.1".into(),
        };
        let resolved = a.resolve(&r).unwrap();
        assert_eq!(resolved.request, "attach");
        assert_eq!(resolved.arguments["coreFile"], "/tmp/core.1");

        r.mode = LaunchMode::AttachServer {
            host: "127.0.0.1".into(),
            port: 4711,
        };
        let resolved = a.resolve(&r).unwrap();
        assert_eq!(
            resolved.transport,
            Transport::Connect {
                host: "127.0.0.1".into(),
                port: 4711,
                adapter: None
            }
        );
        assert!(!r.runs_code() && r.is_attach());
    }

    #[test]
    fn missing_templates_are_unsupported() {
        let mut spec = lldb();
        spec.attach = Value::Null;
        spec.command = String::new();
        let a = ManifestAdapter::new(spec);
        let info = a.info();
        assert!(!info.launch && !info.attach_process);
        let mut r = LaunchRequest::launch("lldb", "/w/demo");
        assert!(a.resolve(&r).is_err(), "no command");
        r.mode = LaunchMode::AttachServer {
            host: "h".into(),
            port: 1,
        };
        assert!(a.resolve(&r).is_ok(), "servers need no command");
        let mut reg = AdapterRegistry::new();
        reg.register(Arc::new(a));
        assert!(reg.get("lldb").is_ok());
        assert!(reg.get("gdb").is_err());
    }

    #[test]
    fn requests_deserialize_from_the_ui() {
        let r: LaunchRequest = serde_json::from_value(json!({
            "adapter": "lldb", "mode": "attachProcess", "pid": 12, "program": "/bin/x"
        }))
        .unwrap();
        assert_eq!(r.mode, LaunchMode::AttachProcess { pid: 12 });
    }
}
