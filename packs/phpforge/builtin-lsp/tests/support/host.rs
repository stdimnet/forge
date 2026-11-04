//! A minimal host of the built-in PHP language server, like the Forge binary and the remote
//! agent: `phpforge-builtin-lsp-host --php-lsp --stdio`. Used by the integration tests.
//!
//! A GUI-subsystem program on Windows, like a release build of Forge, so the tests prove that
//! the server's stdio pipes work without a console.

#![windows_subsystem = "windows"]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = phpforge_builtin_lsp::main_hook(&args) {
        std::process::exit(code);
    }
    eprintln!("usage: phpforge-builtin-lsp-host --php-lsp --stdio");
    std::process::exit(2);
}
