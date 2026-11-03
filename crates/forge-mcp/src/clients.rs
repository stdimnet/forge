//! "Copy client configuration": how Claude Code, Claude Desktop, Cursor, Gemini CLI, Codex and
//! VS Code connect to Forge. HTTP clients get the URL and the bearer token; clients that only
//! start stdio servers run `forge --mcp-stdio`, which finds the running Forge itself.

use serde::Serialize;
use serde_json::json;

/// Name of the server in the clients' configurations.
pub const SERVER_KEY: &str = "forge";
/// The hidden launcher flag of the stdio proxy.
pub const STDIO_FLAG: &str = "--mcp-stdio";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientConfig {
    /// `claude-code`, `claude-desktop`, `cursor`, `gemini-cli`, `codex`, `vscode`.
    pub id: String,
    pub name: String,
    /// `http` or `stdio`.
    pub transport: String,
    /// Where it goes: a command to run, or the configuration file to paste into.
    pub target: String,
    /// `shell`, `json` or `toml`.
    pub language: String,
    pub text: String,
}

fn quote_shell(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:@=".contains(c))
    {
        s.to_string()
    } else {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn pretty(v: serde_json::Value) -> String {
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

fn toml_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Configurations for `url` (`http://127.0.0.1:<port>/mcp`), the bearer `token` and the
/// Forge executable (`exe`, for the stdio proxy).
pub fn configs(url: &str, token: &str, exe: &str) -> Vec<ClientConfig> {
    let auth = format!("Bearer {token}");
    let stdio = json!({ "command": exe, "args": [STDIO_FLAG] });
    let c = |id: &str, name: &str, transport: &str, target: &str, language: &str, text: String| {
        ClientConfig {
            id: id.into(),
            name: name.into(),
            transport: transport.into(),
            target: target.into(),
            language: language.into(),
            text,
        }
    };
    vec![
        c(
            "claude-code",
            "Claude Code",
            "http",
            "Run in a terminal",
            "shell",
            format!(
                "claude mcp add --transport http {SERVER_KEY} {url} --header {}",
                quote_shell(&format!("Authorization: {auth}"))
            ),
        ),
        c(
            "claude-desktop",
            "Claude Desktop",
            "stdio",
            "claude_desktop_config.json (Settings › Developer › Edit Config)",
            "json",
            pretty(json!({ "mcpServers": { SERVER_KEY: stdio } })),
        ),
        c(
            "cursor",
            "Cursor",
            "http",
            "~/.cursor/mcp.json or .cursor/mcp.json",
            "json",
            pretty(json!({
                "mcpServers": { SERVER_KEY: { "url": url, "headers": { "Authorization": auth } } }
            })),
        ),
        c(
            "gemini-cli",
            "Gemini CLI",
            "http",
            "~/.gemini/settings.json",
            "json",
            pretty(json!({
                "mcpServers": { SERVER_KEY: { "httpUrl": url, "headers": { "Authorization": auth } } }
            })),
        ),
        c(
            "codex",
            "Codex CLI",
            "stdio",
            "~/.codex/config.toml",
            "toml",
            format!(
                "[mcp_servers.{SERVER_KEY}]\ncommand = {}\nargs = [{}]\n",
                toml_string(exe),
                toml_string(STDIO_FLAG)
            ),
        ),
        c(
            "vscode",
            "VS Code (Copilot)",
            "http",
            ".vscode/mcp.json",
            "json",
            pretty(json!({
                "servers": { SERVER_KEY: { "type": "http", "url": url, "headers": { "Authorization": auth } } }
            })),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_every_client_configuration() {
        let all = configs("http://127.0.0.1:64342/mcp", "abc123", "/opt/Forge/forge");
        let get = |id: &str| all.iter().find(|c| c.id == id).unwrap();
        assert_eq!(
            get("claude-code").text,
            "claude mcp add --transport http forge http://127.0.0.1:64342/mcp --header \"Authorization: Bearer abc123\""
        );
        let desktop: serde_json::Value = serde_json::from_str(&get("claude-desktop").text).unwrap();
        assert_eq!(
            desktop["mcpServers"]["forge"]["command"],
            "/opt/Forge/forge"
        );
        assert_eq!(desktop["mcpServers"]["forge"]["args"][0], "--mcp-stdio");
        let cursor: serde_json::Value = serde_json::from_str(&get("cursor").text).unwrap();
        assert_eq!(
            cursor["mcpServers"]["forge"]["url"],
            "http://127.0.0.1:64342/mcp"
        );
        assert_eq!(
            cursor["mcpServers"]["forge"]["headers"]["Authorization"],
            "Bearer abc123"
        );
        let gemini: serde_json::Value = serde_json::from_str(&get("gemini-cli").text).unwrap();
        assert_eq!(
            gemini["mcpServers"]["forge"]["httpUrl"],
            "http://127.0.0.1:64342/mcp"
        );
        assert_eq!(
            get("codex").text,
            "[mcp_servers.forge]\ncommand = \"/opt/Forge/forge\"\nargs = [\"--mcp-stdio\"]\n"
        );
        assert!(all.iter().all(|c| !c.text.is_empty()));
    }

    #[test]
    fn quotes_windows_paths() {
        let all = configs(
            "http://127.0.0.1:1/mcp",
            "t",
            "C:\\Program Files\\Forge\\forge.exe",
        );
        let codex = all.iter().find(|c| c.id == "codex").unwrap();
        assert!(codex
            .text
            .contains("command = \"C:\\\\Program Files\\\\Forge\\\\forge.exe\""));
    }
}
