//! The "Godot" run configuration: run the project (its main scene) or one scene, with
//! Godot's own options and the program's arguments after `--`
//! (`OS.get_cmdline_user_args()`), and opening the project in the Godot editor.
//!
//! Options: `target` (`project` or `scene`), `scene` (a project path or `res://` path),
//! `headless` (`true`: no window, `--headless`), `godotArgs` (more Godot options such as
//! `--debug-collisions`), `debugger` (`gdscript` or `csharp` for Debug).

use forge_runtools::args::split_args;
use forge_runtools::Configuration;

use super::path_to_res;
use crate::CommandSpec;

/// The run configuration type.
pub const GODOT_RUN: &str = "godot";

/// Godot's arguments of a configuration (without the program).
pub fn run_args(cfg: &Configuration, project_root: &str) -> Result<Vec<String>, String> {
    let mut args = vec!["--path".to_string(), project_root.to_string()];
    if cfg.option("headless") == "true" {
        args.push("--headless".into());
    }
    args.extend(split_args(cfg.option("godotArgs")));
    if cfg.option("target") == "scene" {
        let scene = cfg.option("scene").trim();
        if scene.is_empty() {
            return Err("Choose the scene to run".into());
        }
        args.push(path_to_res(scene));
    }
    let user = split_args(&cfg.arguments);
    if !user.is_empty() {
        args.push("--".into());
        args.extend(user);
    }
    Ok(args)
}

/// The command line of a "Godot" configuration.
pub fn run_command(
    godot: &str,
    cfg: &Configuration,
    project_root: &str,
) -> Result<CommandSpec, String> {
    if godot.trim().is_empty() {
        return Err("Godot was not found: set its executable in Settings | Languages & Frameworks | .NET | Godot, or put it on the PATH".into());
    }
    Ok(CommandSpec::new(godot, run_args(cfg, project_root)?))
}

/// Open the project in the Godot editor (which starts the GDScript language server and the
/// debug adapter).
pub fn editor_command(godot: &str, project_root: &str) -> CommandSpec {
    CommandSpec::new(
        godot,
        vec!["--path".into(), project_root.into(), "--editor".into()],
    )
}

/// `dotnet build` of a C# Godot project before debugging it with netcoredbg.
pub fn csharp_build(dotnet: &str, csproj: &str) -> CommandSpec {
    CommandSpec::new(dotnet, vec!["build".into(), csproj.into()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_configurations() {
        let mut cfg = Configuration::new("Game", GODOT_RUN);
        assert_eq!(run_args(&cfg, "/g").unwrap(), vec!["--path", "/g"]);
        cfg.options.insert("target".into(), "scene".into());
        assert!(run_args(&cfg, "/g").is_err());
        cfg.options
            .insert("scene".into(), "levels/level_1.tscn".into());
        cfg.options.insert("headless".into(), "true".into());
        cfg.options
            .insert("godotArgs".into(), "--debug-collisions".into());
        cfg.arguments = "--level 2".into();
        assert_eq!(
            run_args(&cfg, "/g").unwrap(),
            vec![
                "--path",
                "/g",
                "--headless",
                "--debug-collisions",
                "res://levels/level_1.tscn",
                "--",
                "--level",
                "2"
            ]
        );
        assert!(run_command("", &cfg, "/g").is_err());
        assert_eq!(run_command("godot", &cfg, "/g").unwrap().program, "godot");
        assert_eq!(
            editor_command("godot", "/g").args,
            vec!["--path", "/g", "--editor"]
        );
        assert_eq!(
            csharp_build("dotnet", "Game.csproj").args,
            vec!["build", "Game.csproj"]
        );
    }
}
