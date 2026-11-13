//! Unity command lines. The app starts them as runs (Safe Mode refuses them: opening a
//! project in Unity compiles and runs its editor scripts).

use crate::CommandSpec;

use super::project::UnityProject;

/// The hidden run configuration type of Unity command lines.
pub const UNITY_COMMAND: &str = "unity-command";

/// Open the project in the Unity editor ("Start Unity").
pub fn open_editor(editor: &str, root: &str) -> CommandSpec {
    CommandSpec::new(editor, vec!["-projectPath".into(), root.into()])
}

/// The static method that regenerates the project files in batch mode: the Forge bridge's,
/// else the JetBrains Rider Editor package's.
pub fn sync_method(project: &UnityProject) -> Result<&'static str, String> {
    if matches!(project.bridge.kind.as_str(), "embedded" | "manifest") {
        Ok("Forge.Unity.Editor.ForgeProjectFiles.SyncFromCommandLine")
    } else if project.has_package("com.unity.ide.rider") {
        Ok("Packages.Rider.Editor.RiderScriptEditor.SyncSolution")
    } else {
        Err("Install the Forge editor bridge (Tools | Unity | Install Editor Bridge…) so Unity can generate the project files, or open the project in Unity once with Forge as its external script editor.".into())
    }
}

/// Regenerate `.sln` / `.csproj` in batch mode (the editor must not have the project open).
pub fn sync_solution(
    editor: &str,
    root: &str,
    project: &UnityProject,
) -> Result<CommandSpec, String> {
    let method = sync_method(project)?;
    Ok(CommandSpec::new(
        editor,
        vec![
            "-batchmode".into(),
            "-nographics".into(),
            "-quit".into(),
            "-projectPath".into(),
            root.into(),
            "-executeMethod".into(),
            method.into(),
            "-logFile".into(),
            "-".into(),
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unity::project::{BridgeInstall, Package};

    #[test]
    fn command_lines() {
        let o = open_editor("/u/Unity", "/p");
        assert_eq!(o.args, vec!["-projectPath", "/p"]);
        let mut p = UnityProject::default();
        assert!(sync_solution("/u/Unity", "/p", &p).is_err());
        p.packages.push(Package {
            name: "com.unity.ide.rider".into(),
            version: "3.0.24".into(),
        });
        let s = sync_solution("/u/Unity", "/p", &p).unwrap();
        assert!(s
            .args
            .contains(&"Packages.Rider.Editor.RiderScriptEditor.SyncSolution".to_string()));
        p.bridge = BridgeInstall {
            kind: "embedded".into(),
            version: Some("1.0.0".into()),
            update: false,
        };
        let s = sync_solution("/u/Unity", "/p", &p).unwrap();
        assert_eq!(
            s.args[..5],
            ["-batchmode", "-nographics", "-quit", "-projectPath", "/p"]
        );
        assert!(s
            .args
            .contains(&"Forge.Unity.Editor.ForgeProjectFiles.SyncFromCommandLine".to_string()));
    }
}
