//! The GDScript language server of the Godot editor. Godot has no stand-alone server: the
//! editor listens on TCP (Editor Settings | Network | Language Server, port 6005) while it
//! has the project open, and Forge connects to it like Rider and VS Code do
//! (`tcp://<host>:<port>`, see `forge_lsp::launch`).

use std::time::Duration;

use forge_lsp::LspServerConfig;

pub const SERVER_ID: &str = "godot-gdscript";
pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_LSP_PORT: u16 = 6005;
pub const DEFAULT_DAP_PORT: u16 = 6006;

/// The language server configuration of `.gd` files.
pub fn server_config(host: &str, port: u16) -> LspServerConfig {
    LspServerConfig {
        id: SERVER_ID.into(),
        name: Some("Godot (GDScript)".into()),
        language_ids: vec!["gdscript".into()],
        file_extensions: vec!["gd".into()],
        command: format!("tcp://{}:{port}", host_or_default(host)),
        ..LspServerConfig::default()
    }
}

fn host_or_default(host: &str) -> &str {
    match host.trim() {
        "" => DEFAULT_HOST,
        h => h,
    }
}

/// Whether something listens on `host:port` (the editor runs with the project open).
pub async fn listening(host: &str, port: u16, timeout: Duration) -> bool {
    let address = format!("{}:{port}", host_or_default(host));
    matches!(
        tokio::time::timeout(timeout, tokio::net::TcpStream::connect(address)).await,
        Ok(Ok(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config() {
        let c = server_config("", 6005);
        assert_eq!(c.command, "tcp://127.0.0.1:6005");
        assert_eq!(forge_lsp::tcp_address(&c.command), Some("127.0.0.1:6005"));
        assert!(c.match_len(std::path::Path::new("player.gd")).is_some());
        assert!(c.match_len(std::path::Path::new("player.cs")).is_none());
    }

    #[tokio::test]
    async fn probes_ports() {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        assert!(listening("127.0.0.1", port, Duration::from_secs(2)).await);
        drop(l);
        assert!(!listening("127.0.0.1", port, Duration::from_secs(2)).await);
    }
}
