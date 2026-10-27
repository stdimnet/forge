//! Rust type renderers for the debuggers: rustc ships pretty printers for `Vec`, `String`,
//! `Option`, `HashMap`, `Rc` and friends in `<sysroot>/lib/rustlib/etc` (what `rust-lldb` and
//! `rust-gdb` load). Forge loads them into the built-in adapters:
//!
//! - LLDB (`lldb-dap`): `initCommands` of the launch request import `lldb_lookup.py` and
//!   source `lldb_commands`,
//! - GDB (`gdb -i dap`): extra adapter arguments add the folder to the auto-load safe path
//!   and source `gdb_load_rust_pretty_printers.py` before the program loads.
//!
//! Both map the standard library's `/rustc/<commit>` paths to the toolchain's `rust-src`
//! component, so stepping into `std` opens its sources.

use serde::{Deserialize, Serialize};

/// The pretty-printer files of a toolchain.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrettyPrinters {
    /// `<sysroot>/lib/rustlib/etc`.
    pub etc: String,
    /// `<sysroot>/lib/rustlib/src/rust` when `rust-src` is installed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sources: String,
    /// rustc's commit hash (`rustc -vV`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub commit_hash: String,
}

fn join(dir: &str, file: &str) -> String {
    let sep = if dir.contains('\\') && !dir.contains('/') {
        '\\'
    } else {
        '/'
    };
    format!("{}{sep}{file}", dir.trim_end_matches(['/', '\\']))
}

impl PrettyPrinters {
    /// Files that must exist for the LLDB setup.
    pub fn lldb_files(&self) -> [String; 2] {
        [
            join(&self.etc, "lldb_lookup.py"),
            join(&self.etc, "lldb_commands"),
        ]
    }

    pub fn gdb_file(&self) -> String {
        join(&self.etc, "gdb_load_rust_pretty_printers.py")
    }

    /// `initCommands` for lldb-dap.
    pub fn lldb_init_commands(&self) -> Vec<String> {
        let [lookup, commands] = self.lldb_files();
        let mut out = vec![
            format!("command script import \"{lookup}\""),
            format!("command source -s 1 \"{commands}\""),
        ];
        if !self.sources.is_empty() && !self.commit_hash.is_empty() {
            out.push(format!(
                "settings append target.source-map /rustc/{} \"{}\"",
                self.commit_hash, self.sources
            ));
        }
        out
    }

    /// Arguments for `gdb -i dap` (before `-i dap`).
    pub fn gdb_args(&self) -> Vec<String> {
        let mut out = vec![
            format!("--directory={}", self.etc),
            "-iex".into(),
            format!("add-auto-load-safe-path {}", self.etc),
            "-iex".into(),
            format!("source {}", self.gdb_file()),
        ];
        if !self.sources.is_empty() && !self.commit_hash.is_empty() {
            out.push("-iex".into());
            out.push(format!(
                "set substitute-path /rustc/{} {}",
                self.commit_hash, self.sources
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_commands_and_gdb_arguments() {
        let p = PrettyPrinters {
            etc: "/home/anna/.rustup/toolchains/stable/lib/rustlib/etc".into(),
            sources: "/home/anna/.rustup/toolchains/stable/lib/rustlib/src/rust".into(),
            commit_hash: "58cf8f9c35b355199727627d07ae34a4e2846d67".into(),
        };
        assert_eq!(
            p.lldb_init_commands(),
            [
                "command script import \"/home/anna/.rustup/toolchains/stable/lib/rustlib/etc/lldb_lookup.py\"",
                "command source -s 1 \"/home/anna/.rustup/toolchains/stable/lib/rustlib/etc/lldb_commands\"",
                "settings append target.source-map /rustc/58cf8f9c35b355199727627d07ae34a4e2846d67 \"/home/anna/.rustup/toolchains/stable/lib/rustlib/src/rust\"",
            ]
        );
        let gdb = p.gdb_args();
        assert_eq!(
            gdb[0],
            "--directory=/home/anna/.rustup/toolchains/stable/lib/rustlib/etc"
        );
        assert_eq!(gdb[4], "source /home/anna/.rustup/toolchains/stable/lib/rustlib/etc/gdb_load_rust_pretty_printers.py");
        assert_eq!(gdb.len(), 7);
        let win = PrettyPrinters {
            etc: "C:\\Users\\anna\\.rustup\\toolchains\\stable\\lib\\rustlib\\etc".into(),
            ..PrettyPrinters::default()
        };
        assert!(win.lldb_files()[0].ends_with("etc\\lldb_lookup.py"));
        assert_eq!(win.lldb_init_commands().len(), 2);
    }
}
