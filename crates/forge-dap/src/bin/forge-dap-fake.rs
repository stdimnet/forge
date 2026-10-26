//! The simulated debug adapter of `forge_dap::fake` as a program, for transport tests and
//! demos.
//!
//! - no arguments: speak DAP on stdin/stdout;
//! - `--connect <port>`: connect to `127.0.0.1:<port>` (Forge listens, the adapter connects back);
//! - `--listen <port>`: accept one client on `127.0.0.1:<port>`.

use tokio::net::{TcpListener, TcpStream};

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let port = |i: usize| -> std::io::Result<u16> {
        args.get(i)
            .and_then(|p| p.parse().ok())
            .ok_or_else(|| std::io::Error::other("expected a port"))
    };
    match args.first().map(String::as_str) {
        Some("--connect") => {
            let stream = TcpStream::connect(("127.0.0.1", port(1)?)).await?;
            let (r, w) = stream.into_split();
            forge_dap::fake::serve(r, w).await
        }
        Some("--listen") => {
            let listener = TcpListener::bind(("127.0.0.1", port(1)?)).await?;
            let (stream, _) = listener.accept().await?;
            let (r, w) = stream.into_split();
            forge_dap::fake::serve(r, w).await
        }
        _ => forge_dap::fake::serve(tokio::io::stdin(), tokio::io::stdout()).await,
    }
}
