//! Zephyr applications: detection (`west.yml`, `prj.conf` with `find_package(Zephyr)`), the
//! board of an existing build (`CMakeCache.txt`), boards the application defines itself,
//! and the `west build` / `west flash` / `west debug` commands.

use serde::{Deserialize, Serialize};

use crate::ToolCommand;

/// Whether the project is a Zephyr application or workspace.
pub fn detect(exists: &dyn Fn(&str) -> bool, cmake_lists: Option<&str>) -> bool {
    exists("west.yml") || (exists("prj.conf") && cmake_lists.is_none_or(|t| t.contains("Zephyr")))
}

/// The board a build folder was configured for (`BOARD` / `CACHED_BOARD` in CMakeCache.txt).
pub fn board_of_cache(cache: &str) -> Option<String> {
    let mut found = None;
    for line in cache.lines() {
        for key in ["CACHED_BOARD:", "BOARD:"] {
            if let Some(rest) = line.strip_prefix(key) {
                if let Some((_, v)) = rest.split_once('=') {
                    if !v.trim().is_empty() {
                        found = found.or(Some(v.trim().to_string()));
                    }
                }
            }
        }
    }
    found
}

/// Boards an application defines below `boards/` (`boards/<vendor>/<board>/board.yml`, or the
/// older `boards/<arch>/<board>/<board>_defconfig`), from project paths.
pub fn local_boards(paths: &[String]) -> Vec<String> {
    let mut out: Vec<String> = paths
        .iter()
        .filter_map(|p| {
            let rest = p.strip_prefix("boards/")?;
            let parts: Vec<&str> = rest.split('/').collect();
            match parts.as_slice() {
                [_, board, "board.yml"] => Some(board.to_string()),
                [_, board, file] if file.ends_with("_defconfig") => Some(board.to_string()),
                _ => None,
            }
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WestAction {
    Build,
    /// A pristine build (`-p always`).
    Rebuild,
    Flash,
    /// Starts GDB in the terminal.
    Debug,
}

impl WestAction {
    pub fn interactive(self) -> bool {
        matches!(self, WestAction::Debug)
    }
}

/// The `west` command of an action: `west build -b <board> -d <build> <app>`, `west flash -d
/// <build>`, `west debug -d <build>`.
pub fn command(
    west: &str,
    action: WestAction,
    board: &str,
    app_dir: &str,
    build_dir: &str,
    workspace_dir: &str,
) -> ToolCommand {
    let build = if build_dir.trim().is_empty() {
        "build"
    } else {
        build_dir.trim()
    };
    let mut args: Vec<String> = Vec::new();
    match action {
        WestAction::Build | WestAction::Rebuild => {
            args.push("build".into());
            if !board.trim().is_empty() {
                args.push("-b".into());
                args.push(board.trim().into());
            }
            args.push("-d".into());
            args.push(build.into());
            args.push("-p".into());
            args.push(
                if action == WestAction::Rebuild {
                    "always"
                } else {
                    "auto"
                }
                .into(),
            );
            if !app_dir.trim().is_empty() {
                args.push(app_dir.trim().into());
            }
        }
        WestAction::Flash | WestAction::Debug => {
            args.push(
                if action == WestAction::Flash {
                    "flash"
                } else {
                    "debug"
                }
                .into(),
            );
            args.push("-d".into());
            args.push(build.into());
        }
    }
    ToolCommand {
        program: west.into(),
        args,
        cwd: workspace_dir.into(),
        env: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_and_boards() {
        assert!(detect(&|p| p == "west.yml", None));
        assert!(detect(
            &|p| p == "prj.conf",
            Some("find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})")
        ));
        assert!(!detect(&|p| p == "prj.conf", Some("project(x)")));
        assert!(!detect(&|_| false, None));
        let cache =
            "# cache\nBOARD:STRING=nrf52840dk/nrf52840\nCACHED_BOARD:STRING=nrf52840dk/nrf52840\n";
        assert_eq!(board_of_cache(cache), Some("nrf52840dk/nrf52840".into()));
        assert_eq!(board_of_cache("X:STRING=1"), None);
        let paths = [
            "boards/acme/sensor_hub/board.yml".to_string(),
            "boards/arm/old_board/old_board_defconfig".to_string(),
            "boards/acme/sensor_hub/sensor_hub.dts".to_string(),
            "src/main.c".to_string(),
        ];
        assert_eq!(local_boards(&paths), ["old_board", "sensor_hub"]);
    }

    #[test]
    fn west_commands() {
        let line = |a, b| {
            let c = command("west", a, b, "app", "", "/ws");
            format!("{} {}", c.program, c.args.join(" "))
        };
        assert_eq!(
            line(WestAction::Build, "nrf52840dk/nrf52840"),
            "west build -b nrf52840dk/nrf52840 -d build -p auto app"
        );
        assert_eq!(
            line(WestAction::Rebuild, "qemu_x86"),
            "west build -b qemu_x86 -d build -p always app"
        );
        assert_eq!(line(WestAction::Flash, ""), "west flash -d build");
        assert_eq!(line(WestAction::Debug, ""), "west debug -d build");
        assert!(WestAction::Debug.interactive());
        assert_eq!(
            command("west", WestAction::Build, "", "", "out", "/ws")
                .args
                .join(" "),
            "build -d out -p auto"
        );
    }
}
