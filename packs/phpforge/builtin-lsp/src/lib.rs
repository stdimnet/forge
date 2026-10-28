//! PHPForge's built-in PHP language server: php-lsp
//! (<https://github.com/stdimnet/php-lsp>, MIT OR Apache-2.0), a pure-Rust Intelephense
//! drop-in, linked into the Forge binary and the remote agent.
//!
//! The server runs in its own process for crash isolation: the binary that hosts it starts
//! itself again with [`FLAG`] (`forge --php-lsp --stdio`), and forge-lsp talks to that
//! process like to any other language server, so restarts, remote projects and the Language
//! Servers UI work unchanged. Call [`main_hook`] first thing in `main`:
//!
//! ```no_run
//! let args: Vec<String> = std::env::args().collect();
//! if let Some(code) = phpforge_builtin_lsp::main_hook(&args) {
//!     std::process::exit(code);
//! }
//! // ... the normal program
//! ```

use std::path::PathBuf;

use phpforge_lang_php::lsp::{register_builtin_server, BuiltinServer};

/// First argument of the hidden server mode.
pub const FLAG: &str = "--php-lsp";

/// php-lsp's version, as pinned in Cargo.toml (`rev`). The crate exposes none, so it is kept
/// here and checked against Cargo.lock by a test.
pub const VERSION: &str = "0.1.0";
/// The pinned php-lsp commit.
pub const REVISION: &str = "0fd60bc7f242112582544e4dfa0bd51232268287";

/// `0.1.0 (0fd60bc)`, as shown in Settings.
pub fn version_label() -> String {
    format!("{VERSION} ({})", &REVISION[..7])
}

/// Whether `args` (with the program name first) ask for the hidden server mode.
pub fn is_server_mode(args: &[String]) -> bool {
    args.get(1).map(String::as_str) == Some(FLAG)
}

/// The arguments that start the hidden server mode of this binary.
pub fn server_args() -> Vec<String> {
    vec![FLAG.into(), "--stdio".into()]
}

/// Run php-lsp on stdin / stdout until the client exits. Returns the exit code.
pub fn run_stdio() -> i32 {
    match php_lsp::run_stdio() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("php-lsp: {e}");
            1
        }
    }
}

/// The startup hook of binaries that host php-lsp. In server mode it runs the server and
/// returns the exit code; otherwise it announces the built-in server (this executable) to
/// the PHP pack and returns `None`.
pub fn main_hook(args: &[String]) -> Option<i32> {
    if is_server_mode(args) {
        if args.iter().any(|a| a == "--version" || a == "-V") {
            println!("php-lsp {}", version_label());
            return Some(0);
        }
        return Some(run_stdio());
    }
    if let Ok(exe) = std::env::current_exe() {
        register(exe);
    }
    None
}

/// Announce `exe` (a binary calling [`main_hook`]) as the built-in PHP language server.
pub fn register(exe: PathBuf) {
    register_builtin_server(BuiltinServer {
        command: exe,
        args: server_args(),
        version: version_label(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_the_hidden_mode() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(is_server_mode(&args(&["forge", "--php-lsp", "--stdio"])));
        assert!(!is_server_mode(&args(&["forge"])));
        assert!(!is_server_mode(&args(&["forge", "inspect", "--php-lsp"])));
        assert_eq!(server_args(), vec!["--php-lsp", "--stdio"]);
        assert_eq!(version_label(), "0.1.0 (0fd60bc)");
    }

    #[test]
    fn version_matches_the_locked_php_lsp() {
        let lock = include_str!("../../../../Cargo.lock");
        let entry = lock
            .split("[[package]]")
            .find(|p| p.contains("name = \"php-lsp\"\n"))
            .expect("php-lsp in Cargo.lock");
        assert!(
            entry.contains(&format!("version = \"{VERSION}\"")),
            "{entry}"
        );
        assert!(entry.contains(REVISION), "{entry}");
    }
}
