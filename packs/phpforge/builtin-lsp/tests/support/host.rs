//! A minimal host of the built-in PHP language server, like the Forge binary and the remote
//! agent: `phpforge-builtin-lsp-host --php-lsp --stdio`. Used by the integration tests.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = phpforge_builtin_lsp::main_hook(&args) {
        std::process::exit(code);
    }
    eprintln!("usage: phpforge-builtin-lsp-host --php-lsp --stdio");
    std::process::exit(2);
}
