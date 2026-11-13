//! GDScript debugging through the Godot editor's debug adapter (Editor Settings | Network |
//! Debug Adapter, TCP port 6006). The editor is the adapter: Forge connects to it, and
//! `launch` makes the editor run the project (or a scene) with its debugger attached, like
//! pressing Play in the editor; `attach` attaches to the game the editor runs.
//!
//! Launch arguments Godot reads: `project` (must be the editor's project folder),
//! `scene` (`main`, `current`, `pinned` or a `res://` scene), `playArgs` (program
//! arguments). C# code is debugged with netcoredbg instead (the app's
//! `godot_debug_request`).

use forge_dap::{
    AdapterInfo, DapError, DapResult, DebugAdapterDescriptor, LaunchMode, LaunchRequest,
    ResolvedLaunch, Transport,
};
use serde_json::{json, Map, Value};

/// The adapter type.
pub const GODOT_ADAPTER: &str = "godot";

pub struct GodotAdapter;

/// The launch arguments of a Godot debug session.
pub fn launch_arguments(project: &str, scene: Option<&str>, play_args: &[String]) -> Value {
    let mut v = json!({
        "project": project.replace('\\', "/").trim_end_matches('/'),
        "scene": scene.filter(|s| !s.trim().is_empty()).map(super::path_to_res).unwrap_or_else(|| "main".into()),
    });
    if !play_args.is_empty() {
        v["playArgs"] = json!(play_args);
    }
    v
}

impl DebugAdapterDescriptor for GodotAdapter {
    fn info(&self) -> AdapterInfo {
        AdapterInfo {
            r#type: GODOT_ADAPTER.into(),
            label: "Godot (GDScript)".into(),
            languages: vec!["gd".into()],
            ..AdapterInfo::default()
        }
    }

    fn resolve(&self, r: &LaunchRequest) -> DapResult<ResolvedLaunch> {
        let LaunchMode::AttachServer { host, port } = &r.mode else {
            return Err(DapError::Unsupported(
                "GDScript is debugged by the Godot editor: open the project in Godot, then debug the configuration again".into(),
            ));
        };
        let mut arguments = match &r.configuration {
            Value::Object(m) => m.clone(),
            _ => Map::new(),
        };
        let request = arguments
            .remove("request")
            .and_then(|v| v.as_str().map(str::to_string))
            .filter(|r| r == "attach")
            .unwrap_or_else(|| "launch".into());
        arguments.insert("name".into(), json!(r.title()));
        arguments.insert("type".into(), json!(GODOT_ADAPTER));
        arguments.insert("request".into(), json!(request));
        if let Some(cwd) = r.cwd.as_deref().filter(|c| !c.is_empty()) {
            arguments
                .entry("project")
                .or_insert_with(|| json!(cwd.replace('\\', "/").trim_end_matches('/')));
        }
        Ok(ResolvedLaunch {
            transport: Transport::Connect {
                host: host.clone(),
                port: *port,
                adapter: None,
            },
            request,
            arguments: Value::Object(arguments),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_launch_and_attach() {
        let mut r = LaunchRequest::launch(GODOT_ADAPTER, "");
        r.program = None;
        assert!(GodotAdapter.resolve(&r).is_err());
        r.mode = LaunchMode::AttachServer {
            host: "127.0.0.1".into(),
            port: 6006,
        };
        r.name = Some("Game".into());
        r.cwd = Some("/home/sam/dodge/".into());
        r.configuration = launch_arguments(
            "/home/sam/dodge",
            Some("levels/one.tscn"),
            &["--fast".into()],
        );
        let res = GodotAdapter.resolve(&r).unwrap();
        assert_eq!(res.request, "launch");
        assert_eq!(
            res.transport,
            Transport::Connect {
                host: "127.0.0.1".into(),
                port: 6006,
                adapter: None
            }
        );
        assert_eq!(res.arguments["project"], "/home/sam/dodge");
        assert_eq!(res.arguments["scene"], "res://levels/one.tscn");
        assert_eq!(res.arguments["playArgs"][0], "--fast");
        r.configuration = json!({"request": "attach"});
        let res = GodotAdapter.resolve(&r).unwrap();
        assert_eq!(res.request, "attach");
        assert_eq!(res.arguments["project"], "/home/sam/dodge");
        assert_eq!(launch_arguments("C:\\g\\", None, &[])["scene"], "main");
        assert_eq!(GodotAdapter.info().languages, vec!["gd"]);
    }
}
