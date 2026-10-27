//! GDB with rustc's pretty printers: the `rust-gdb` debug adapter wraps the built-in GDB
//! adapter (`gdb -i dap`, declared by the Debugger plugin) and adds the arguments of
//! [`PrettyPrinters::gdb_args`] before `-i dap`, like rustc's own `rust-gdb` script.
//! LLDB needs no wrapper: its pretty printers come as `initCommands` of the launch request.

use forge_dap::{
    AdapterInfo, DapResult, DebugAdapterDescriptor, LaunchRequest, ManifestAdapter, ResolvedLaunch,
    Transport,
};
use forge_plugin_api::DebugAdapterContribution;
use rustforge_lang_rust::PrettyPrinters;

/// The adapter type.
pub const ADAPTER: &str = "rust-gdb";

pub struct RustGdbAdapter {
    inner: ManifestAdapter,
    printers: Option<PrettyPrinters>,
}

impl RustGdbAdapter {
    /// `gdb` is the Debugger plugin's GDB contribution, `command` the user's GDB path.
    pub fn new(
        gdb: DebugAdapterContribution,
        command: Option<String>,
        printers: Option<PrettyPrinters>,
    ) -> Self {
        Self {
            inner: ManifestAdapter::new(gdb).with_command(command),
            printers,
        }
    }
}

impl DebugAdapterDescriptor for RustGdbAdapter {
    fn info(&self) -> AdapterInfo {
        let mut info = self.inner.info();
        info.r#type = ADAPTER.into();
        info.label = "GDB (Rust pretty printers)".into();
        info.languages = vec!["rs".into()];
        info
    }

    fn resolve(&self, request: &LaunchRequest) -> DapResult<ResolvedLaunch> {
        let mut resolved = self.inner.resolve(request)?;
        if let Some(obj) = resolved.arguments.as_object_mut() {
            obj.insert("type".into(), serde_json::json!(ADAPTER));
        }
        if let Some(p) = &self.printers {
            let adapter = match &mut resolved.transport {
                Transport::Stdio { adapter } | Transport::Listen { adapter } => Some(adapter),
                Transport::Connect { adapter, .. } => adapter.as_mut(),
            };
            if let Some(a) = adapter {
                let mut args = p.gdb_args();
                args.append(&mut a.args);
                a.args = args;
            }
        }
        Ok(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn gdb() -> DebugAdapterContribution {
        serde_json::from_value(json!({
            "type": "gdb",
            "label": "GDB",
            "command": "gdb",
            "args": ["-i", "dap"],
            "languages": ["c", "rs"],
            "launch": { "program": "${program}", "args": "${args}", "cwd": "${cwd}" },
            "attach": { "pid": "${pid}" }
        }))
        .unwrap()
    }

    #[test]
    fn gdb_starts_with_the_pretty_printers() {
        let printers = PrettyPrinters {
            etc: "/r/lib/rustlib/etc".into(),
            ..PrettyPrinters::default()
        };
        let a = RustGdbAdapter::new(gdb(), Some("/usr/bin/gdb".into()), Some(printers));
        assert_eq!(a.info().r#type, ADAPTER);
        assert!(a.info().launch);
        let r = a
            .resolve(&LaunchRequest::launch(ADAPTER, "/w/shop/target/debug/shop"))
            .unwrap();
        let Transport::Stdio { adapter } = &r.transport else {
            panic!("stdio expected");
        };
        assert_eq!(adapter.command, "/usr/bin/gdb");
        assert_eq!(
            adapter.args,
            [
                "--directory=/r/lib/rustlib/etc",
                "-iex",
                "add-auto-load-safe-path /r/lib/rustlib/etc",
                "-iex",
                "source /r/lib/rustlib/etc/gdb_load_rust_pretty_printers.py",
                "-i",
                "dap"
            ]
        );
        assert_eq!(r.arguments["program"], "/w/shop/target/debug/shop");
        assert_eq!(r.arguments["type"], ADAPTER);
        let plain = RustGdbAdapter::new(gdb(), None, None);
        let r = plain
            .resolve(&LaunchRequest::launch(ADAPTER, "/x"))
            .unwrap();
        let Transport::Stdio { adapter } = &r.transport else {
            panic!("stdio expected");
        };
        assert_eq!(adapter.args, ["-i", "dap"]);
    }
}
