// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // `forge --php-lsp --stdio`: the built-in PHP language server (php-lsp), started by Forge
    // itself in its own process. Otherwise this registers it for the PHP pack.
    if let Some(code) = phpforge_builtin_lsp::main_hook(&args) {
        std::process::exit(code);
    }
    // `forge inspect …`: the command-line inspector runs headless and exits.
    if let Some(code) = forge_app_lib::cli::main(&args) {
        std::process::exit(code);
    }
    forge_app_lib::run()
}
