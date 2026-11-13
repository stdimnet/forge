//! `forge-remote-agent serve` speaks the agent protocol on stdin/stdout (started by the IDE
//! over SSH); `--version` prints the version the IDE checks before uploading a new binary.
//! Logs go to stderr (`FORGE_AGENT_LOG=debug` for more).

use std::process::ExitCode;

fn main() -> ExitCode {
    // `forge-remote-agent --php-lsp --stdio`: the built-in PHP language server, started by
    // the agent itself for remote PHP projects. Otherwise this registers it for the PHP pack.
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = phpforge_builtin_lsp::main_hook(&args) {
        return ExitCode::from(code.clamp(0, 255) as u8);
    }
    let arg = args.get(1).cloned().unwrap_or_default();
    match arg.as_str() {
        "--version" | "-V" | "version" => {
            println!("{}", forge_remote_agent::version_line());
            ExitCode::SUCCESS
        }
        "serve" => {
            tracing_subscriber::fmt()
                .with_writer(std::io::stderr)
                .with_ansi(false)
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_env("FORGE_AGENT_LOG")
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
                )
                .init();
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("forge-remote-agent: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let result = runtime.block_on(forge_remote_agent::serve(
                tokio::io::stdin(),
                tokio::io::stdout(),
            ));
            // Blocking reads of the PTY threads must not keep the process alive.
            runtime.shutdown_timeout(std::time::Duration::from_secs(1));
            match result {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("forge-remote-agent: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!(
                "usage: forge-remote-agent serve | --version\n\
                 The Forge IDE starts this agent over SSH for remote development."
            );
            ExitCode::from(2)
        }
    }
}
