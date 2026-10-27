// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `forge inspect …`: the command-line inspector runs headless and exits.
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = forge_app_lib::cli::main(&args) {
        std::process::exit(code);
    }
    forge_app_lib::run()
}
