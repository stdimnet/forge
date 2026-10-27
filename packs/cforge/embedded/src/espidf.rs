//! ESP-IDF projects: detection (`sdkconfig`, `idf_component.yml`, the IDF's `project.cmake`
//! in CMakeLists.txt), the chip target of `sdkconfig`, and `idf.py` commands (build, flash,
//! monitor, menuconfig, fullclean) with the serial port and `IDF_PATH`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ToolCommand;

/// Whether the project is an ESP-IDF project: `exists(path)` checks project files,
/// `cmake_lists` is the root CMakeLists.txt.
pub fn detect(exists: &dyn Fn(&str) -> bool, cmake_lists: Option<&str>) -> bool {
    exists("sdkconfig")
        || exists("sdkconfig.defaults")
        || exists("idf_component.yml")
        || exists("main/idf_component.yml")
        || cmake_lists.is_some_and(|t| t.contains("tools/cmake/project.cmake"))
}

/// `CONFIG_IDF_TARGET="esp32s3"` of `sdkconfig`.
pub fn target(sdkconfig: &str) -> Option<String> {
    sdkconfig.lines().find_map(|l| {
        l.trim()
            .strip_prefix("CONFIG_IDF_TARGET=")
            .map(|v| v.trim().trim_matches('"').to_string())
            .filter(|v| !v.is_empty())
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IdfAction {
    Build,
    Flash,
    Monitor,
    /// Interactive: runs in the terminal.
    Menuconfig,
    FullClean,
}

impl IdfAction {
    /// Whether the action needs a terminal (a full-screen UI).
    pub fn interactive(self) -> bool {
        matches!(self, IdfAction::Menuconfig)
    }

    fn word(self) -> &'static str {
        match self {
            IdfAction::Build => "build",
            IdfAction::Flash => "flash",
            IdfAction::Monitor => "monitor",
            IdfAction::Menuconfig => "menuconfig",
            IdfAction::FullClean => "fullclean",
        }
    }
}

/// Where `idf.py` comes from and the serial port.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct IdfSetup {
    /// The ESP-IDF folder (`IDF_PATH`); empty: `idf.py` on the PATH (an exported shell).
    pub idf_path: String,
    /// The Python of the IDF environment (`python3` when empty).
    pub python: String,
    /// Serial port (`/dev/ttyUSB0`, `COM3`); empty: idf.py looks for one.
    pub port: String,
    /// Flash baud rate; empty: idf.py's default.
    pub baud: String,
}

/// The `idf.py` command of an action in `project_dir`.
pub fn command(action: IdfAction, setup: &IdfSetup, project_dir: &str) -> ToolCommand {
    let idf = setup.idf_path.trim().trim_end_matches(['/', '\\']);
    let mut env = BTreeMap::new();
    let (program, mut args) = if idf.is_empty() {
        ("idf.py".to_string(), Vec::new())
    } else {
        env.insert("IDF_PATH".to_string(), idf.to_string());
        let python = match setup.python.trim() {
            "" => "python3",
            p => p,
        };
        (python.to_string(), vec![format!("{idf}/tools/idf.py")])
    };
    if matches!(action, IdfAction::Flash | IdfAction::Monitor) && !setup.port.trim().is_empty() {
        args.push("-p".into());
        args.push(setup.port.trim().into());
    }
    if action == IdfAction::Flash && !setup.baud.trim().is_empty() {
        args.push("-b".into());
        args.push(setup.baud.trim().into());
    }
    args.push(action.word().into());
    ToolCommand {
        program,
        args,
        cwd: project_dir.into(),
        env,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_projects_and_targets() {
        assert!(detect(&|p| p == "sdkconfig", None));
        assert!(detect(&|p| p == "main/idf_component.yml", None));
        assert!(detect(
            &|_| false,
            Some("cmake_minimum_required(VERSION 3.16)\ninclude($ENV{IDF_PATH}/tools/cmake/project.cmake)\nproject(blink)\n")
        ));
        assert!(!detect(&|_| false, Some("project(x)")));
        assert_eq!(
            target("# comment\nCONFIG_IDF_TARGET=\"esp32s3\"\n"),
            Some("esp32s3".into())
        );
        assert_eq!(target("CONFIG_X=y"), None);
    }

    #[test]
    fn idf_py_commands() {
        let line = |c: ToolCommand| format!("{} {}", c.program, c.args.join(" "));
        let path = IdfSetup::default();
        assert_eq!(line(command(IdfAction::Build, &path, "/p")), "idf.py build");
        let setup = IdfSetup {
            idf_path: "/opt/esp-idf/".into(),
            python: String::new(),
            port: "/dev/ttyUSB0".into(),
            baud: "921600".into(),
        };
        let flash = command(IdfAction::Flash, &setup, "/p");
        assert_eq!(
            flash.env.get("IDF_PATH").map(String::as_str),
            Some("/opt/esp-idf")
        );
        assert_eq!(
            line(flash),
            "python3 /opt/esp-idf/tools/idf.py -p /dev/ttyUSB0 -b 921600 flash"
        );
        assert_eq!(
            line(command(IdfAction::Monitor, &setup, "/p")),
            "python3 /opt/esp-idf/tools/idf.py -p /dev/ttyUSB0 monitor"
        );
        assert_eq!(
            line(command(IdfAction::Menuconfig, &path, "/p")),
            "idf.py menuconfig"
        );
        assert!(IdfAction::Menuconfig.interactive());
        assert!(!IdfAction::Monitor.interactive());
        assert_eq!(
            line(command(IdfAction::FullClean, &path, "/p")),
            "idf.py fullclean"
        );
    }
}
