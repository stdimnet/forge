//! Running Unity editors for "Attach to Unity Process…". Unity's debugger is the Mono soft
//! debugger (not the Debug Adapter Protocol): with "Code Optimization On Startup: Debug"
//! (or the Debug mode toggle in the status bar) an editor listens on port
//! `56000 + pid % 1000`. Forge lists the editors and their ports; attaching needs a Mono
//! soft-debugger debug adapter configured in Settings (see the user guide).

use forge_dap::ProcessInfo;
use serde::{Deserialize, Serialize};

/// The first port of Unity's debugger agents.
pub const DEBUGGER_BASE_PORT: u16 = 56000;

/// The Mono debugger port of a Unity editor or development player process.
pub fn debugger_port(pid: u32) -> u16 {
    DEBUGGER_BASE_PORT + (pid % 1000) as u16
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnityProcess {
    pub pid: u32,
    pub name: String,
    /// The project the editor has open (`-projectPath`), if its command line says so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Whether it is the editor of the open project.
    pub this_project: bool,
    pub debug_port: u16,
}

const NOT_EDITORS: &[&str] = &[
    "unity hub",
    "unityhub",
    "unityshadercompiler",
    "unitypackagemanager",
    "unitycrashhandler",
    "unity.licensing.client",
    "unityhelper",
];

fn is_editor(p: &ProcessInfo) -> bool {
    let name = p.name.to_ascii_lowercase();
    let name = name.trim_end_matches(".exe");
    if NOT_EDITORS.iter().any(|n| name.starts_with(n)) {
        return false;
    }
    name == "unity"
        || (p.command_line.contains("/Unity.app/Contents/MacOS/Unity")
            && !p.command_line.to_ascii_lowercase().contains("hub"))
}

/// `-projectPath <dir>` (any case, also `-projectpath=<dir>`) of a command line.
pub fn project_path(command_line: &str) -> Option<String> {
    let lower = command_line.to_ascii_lowercase();
    let i = lower.find("-projectpath")?;
    let rest = command_line[i + "-projectpath".len()..].trim_start_matches(['=', ' ']);
    let path = if let Some(q) = rest.strip_prefix('"') {
        q.split('"').next()?
    } else {
        // Unquoted: up to the next option.
        match rest.find(" -") {
            Some(end) => &rest[..end],
            None => rest,
        }
        .trim()
    };
    (!path.is_empty()).then(|| path.replace('\\', "/"))
}

fn same_path(a: &str, b: &str) -> bool {
    let norm = |p: &str| {
        p.replace('\\', "/")
            .trim_end_matches('/')
            .to_ascii_lowercase()
    };
    norm(a) == norm(b)
}

/// The Unity editors among `processes`; `editor_pid` is the pid of the project's
/// `Library/EditorInstance.json`.
pub fn unity_processes(
    processes: &[ProcessInfo],
    project_root: &str,
    editor_pid: Option<u32>,
) -> Vec<UnityProcess> {
    let mut out: Vec<UnityProcess> = processes
        .iter()
        .filter(|p| is_editor(p))
        .map(|p| {
            let project = project_path(&p.command_line);
            let this_project = Some(p.pid) == editor_pid
                || project
                    .as_deref()
                    .is_some_and(|x| same_path(x, project_root));
            UnityProcess {
                pid: p.pid,
                name: p.name.clone(),
                project,
                this_project,
                debug_port: debugger_port(p.pid),
            }
        })
        .collect();
    out.sort_by_key(|p| (!p.this_project, p.pid));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, name: &str, cmd: &str) -> ProcessInfo {
        ProcessInfo {
            pid,
            name: name.into(),
            command_line: cmd.into(),
            user: None,
        }
    }

    #[test]
    fn finds_editors_and_ports() {
        let list = vec![
            proc(1201, "Unity", "/home/sam/Unity/Hub/Editor/2022.3.10f1/Editor/Unity -projectpath /home/sam/Other -useHub"),
            proc(4242, "Unity", "/home/sam/Unity/Hub/Editor/2022.3.10f1/Editor/Unity -projectpath /home/sam/Shooter -useHub -hubIPC"),
            proc(77, "unityhub-bin", "/opt/unityhub/unityhub-bin"),
            proc(78, "UnityShaderCompiler", "UnityShaderCompiler"),
            proc(90, "Unity.exe", r#""C:\Program Files\Unity\Hub\Editor\2022.3.10f1\Editor\Unity.exe" -projectPath "C:\Games\My Game" -useHub"#),
        ];
        let found = unity_processes(&list, "/home/sam/Shooter/", None);
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].pid, 4242);
        assert!(found[0].this_project);
        assert_eq!(found[0].debug_port, 56242);
        assert_eq!(found[1].pid, 90);
        assert_eq!(found[1].project.as_deref(), Some("C:/Games/My Game"));
        // The editor of EditorInstance.json counts even without -projectPath.
        let found = unity_processes(&[proc(5000, "Unity", "Unity")], "/p", Some(5000));
        assert!(found[0].this_project);
        assert_eq!(debugger_port(5000), 56000);
        assert_eq!(project_path("Unity -projectPath=/a/b"), Some("/a/b".into()));
        assert_eq!(project_path("Unity"), None);
    }
}
