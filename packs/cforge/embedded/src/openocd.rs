//! OpenOCD for "OpenOCD Download & Run" and "Embedded GDB Server" configurations: the GDB
//! server command line with the board configuration, flashing an ELF (`program … verify reset
//! exit`), the server's ready line and the address GDB connects to.

use serde::{Deserialize, Serialize};

use crate::ToolCommand;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OpenOcdSetup {
    /// The executable (`openocd`).
    pub openocd: String,
    /// Configuration files, separated by `;` or new lines: a board file
    /// (`board/st_nucleo_f4.cfg`, searched in OpenOCD's scripts) or project files.
    pub config: String,
    pub gdb_port: u16,
    /// Extra command-line arguments.
    pub extra: Vec<String>,
}

impl Default for OpenOcdSetup {
    fn default() -> Self {
        Self {
            openocd: "openocd".into(),
            config: String::new(),
            gdb_port: 3333,
            extra: Vec::new(),
        }
    }
}

fn config_args(setup: &OpenOcdSetup) -> Vec<String> {
    let mut out = Vec::new();
    for f in setup
        .config
        .split([';', '\n'])
        .map(str::trim)
        .filter(|f| !f.is_empty())
    {
        out.push("-f".into());
        out.push(f.into());
    }
    out
}

/// Paths in OpenOCD's Tcl commands: forward slashes, braces against spaces.
fn tcl_path(path: &str) -> String {
    format!("{{{}}}", path.replace('\\', "/"))
}

/// The GDB server: `openocd -f <cfg> -c "gdb_port 3333"`.
pub fn server_command(setup: &OpenOcdSetup, project_dir: &str) -> Result<ToolCommand, String> {
    let mut args = config_args(setup);
    if args.is_empty() {
        return Err("Choose the board configuration file of OpenOCD".into());
    }
    args.push("-c".into());
    args.push(format!("gdb_port {}", setup.gdb_port));
    args.extend(setup.extra.iter().cloned());
    Ok(ToolCommand {
        program: setup.openocd.clone(),
        args,
        cwd: project_dir.into(),
        env: Default::default(),
    })
}

/// Flash `elf` and reset the board: `openocd -f <cfg> -c "program {elf} verify reset exit"`.
pub fn program_command(
    setup: &OpenOcdSetup,
    elf: &str,
    project_dir: &str,
) -> Result<ToolCommand, String> {
    let mut args = config_args(setup);
    if args.is_empty() {
        return Err("Choose the board configuration file of OpenOCD".into());
    }
    args.extend(setup.extra.iter().cloned());
    args.push("-c".into());
    args.push(format!("program {} verify reset exit", tcl_path(elf)));
    Ok(ToolCommand {
        program: setup.openocd.clone(),
        args,
        cwd: project_dir.into(),
        env: Default::default(),
    })
}

/// Whether a line of OpenOCD's output says the GDB server is ready.
pub fn is_ready_line(line: &str) -> bool {
    line.contains("for gdb connections")
}

/// What GDB's `target remote` connects to.
pub fn gdb_target(host: &str, port: u16) -> String {
    let host = if host.trim().is_empty() {
        "localhost"
    } else {
        host.trim()
    };
    format!("{host}:{port}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_and_program_commands() {
        let setup = OpenOcdSetup {
            config: "board/st_nucleo_f4.cfg".into(),
            ..OpenOcdSetup::default()
        };
        let s = server_command(&setup, "/p").unwrap();
        assert_eq!(
            s.args.join(" "),
            "-f board/st_nucleo_f4.cfg -c gdb_port 3333"
        );
        let p = program_command(&setup, "C:\\fw\\my app.elf", "/p").unwrap();
        assert_eq!(
            p.args,
            [
                "-f",
                "board/st_nucleo_f4.cfg",
                "-c",
                "program {C:/fw/my app.elf} verify reset exit"
            ]
        );
        let two = OpenOcdSetup {
            config: "interface/stlink.cfg; target/stm32f4x.cfg".into(),
            gdb_port: 4444,
            ..OpenOcdSetup::default()
        };
        assert_eq!(
            server_command(&two, "/p").unwrap().args.join(" "),
            "-f interface/stlink.cfg -f target/stm32f4x.cfg -c gdb_port 4444"
        );
        assert!(server_command(&OpenOcdSetup::default(), "/p").is_err());
    }

    #[test]
    fn ready_line_and_target() {
        assert!(is_ready_line(
            "Info : Listening on port 3333 for gdb connections"
        ));
        assert!(!is_ready_line(
            "Info : Listening on port 6666 for tcl connections"
        ));
        assert_eq!(gdb_target("", 3333), "localhost:3333");
        assert_eq!(gdb_target("10.0.0.2", 2331), "10.0.0.2:2331");
    }
}
