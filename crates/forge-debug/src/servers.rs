//! PHP servers (PHPStorm's "Settings > PHP > Servers"): which host a debug connection
//! comes from and how its paths map into the project. Stored per project in
//! `<project>/.forge/php-servers.json`.

use forge_core::ProjectFiles;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{DebugError, DebugResult};
use crate::mapping::{PathMapper, PathMapping};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub id: String,
    /// Matched against `PHP_IDE_CONFIG=serverName=<name>`.
    pub name: String,
    /// Matched against `$_SERVER['SERVER_NAME']`.
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_debugger")]
    pub debugger: String,
    #[serde(default)]
    pub use_path_mappings: bool,
    #[serde(default)]
    pub mappings: Vec<PathMapping>,
}

fn default_port() -> u16 {
    80
}

fn default_debugger() -> String {
    "Xdebug".into()
}

impl ServerConfig {
    pub fn mapper(&self, root: &Path) -> PathMapper {
        if self.use_path_mappings {
            PathMapper::new(root, self.mappings.clone())
        } else {
            PathMapper::local(root)
        }
    }
}

/// What the engine told us about where the request runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerIdentity {
    /// From `PHP_IDE_CONFIG` (`serverName=...`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    /// `$_SERVER['SERVER_NAME']` (absent for CLI scripts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

impl ServerIdentity {
    /// Display name, like PHPStorm's "Incoming connection from 'shop.test'".
    pub fn display_name(&self) -> String {
        match (&self.server_name, &self.host, self.port) {
            (Some(n), _, _) => n.clone(),
            (None, Some(h), Some(p)) if p != 80 && p != 443 => format!("{h}:{p}"),
            (None, Some(h), _) => h.clone(),
            _ => "CLI".into(),
        }
    }
}

/// Parse `PHP_IDE_CONFIG`, e.g. `serverName=shop.test`.
pub fn parse_ide_config(value: &str) -> Option<String> {
    value.split(['&', ';', ' ']).find_map(|part| {
        let (k, v) = part.split_once('=')?;
        (k.trim() == "serverName" && !v.trim().is_empty()).then(|| v.trim().to_string())
    })
}

/// The configured server a connection belongs to.
pub fn find_server<'a>(
    servers: &'a [ServerConfig],
    id: &ServerIdentity,
) -> Option<&'a ServerConfig> {
    if let Some(name) = &id.server_name {
        return servers.iter().find(|s| s.name.eq_ignore_ascii_case(name));
    }
    let host = id.host.as_ref()?;
    let by_host = |s: &&ServerConfig| s.host.eq_ignore_ascii_case(host);
    servers
        .iter()
        .filter(by_host)
        .find(|s| id.port.is_none_or(|p| p == s.port))
        .or_else(|| servers.iter().find(by_host))
}

/// The servers file relative to the project root.
pub const SERVERS_FILE: &str = ".forge/php-servers.json";

pub fn servers_path(root: &Path) -> PathBuf {
    root.join(SERVERS_FILE)
}

pub fn load_servers(root: &Path) -> Vec<ServerConfig> {
    load_servers_from(&forge_core::LocalFiles::new(root))
}

pub fn save_servers(root: &Path, servers: &[ServerConfig]) -> DebugResult<()> {
    save_servers_to(&forge_core::LocalFiles::new(root), servers)
}

/// The project's servers read through `files`.
pub fn load_servers_from(files: &dyn ProjectFiles) -> Vec<ServerConfig> {
    files
        .read(SERVERS_FILE)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Save the servers through `files`; an empty list creates no file.
pub fn save_servers_to(files: &dyn ProjectFiles, servers: &[ServerConfig]) -> DebugResult<()> {
    let other = |e: forge_core::ForgeError| DebugError::Other(e.to_string());
    if servers.is_empty() && files.read(SERVERS_FILE).map_err(other)?.is_none() {
        return Ok(());
    }
    let json =
        serde_json::to_string_pretty(servers).map_err(|e| DebugError::Other(e.to_string()))?;
    files.write(SERVERS_FILE, &(json + "\n")).map_err(other)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(name: &str, host: &str, port: u16) -> ServerConfig {
        ServerConfig {
            id: name.into(),
            name: name.into(),
            host: host.into(),
            port,
            debugger: "Xdebug".into(),
            use_path_mappings: true,
            mappings: vec![],
        }
    }

    #[test]
    fn identifies_servers_by_name_then_host_and_port() {
        let servers = vec![
            server("docker", "localhost", 8080),
            server("shop", "shop.test", 80),
        ];
        let by_name = ServerIdentity {
            server_name: Some("Docker".into()),
            ..Default::default()
        };
        assert_eq!(find_server(&servers, &by_name).unwrap().id, "docker");
        let by_host = ServerIdentity {
            host: Some("localhost".into()),
            port: Some(8080),
            ..Default::default()
        };
        assert_eq!(find_server(&servers, &by_host).unwrap().id, "docker");
        assert!(find_server(&servers, &ServerIdentity::default()).is_none());
        assert_eq!(
            parse_ide_config("serverName=shop.test").as_deref(),
            Some("shop.test")
        );
        assert_eq!(parse_ide_config("foo=1"), None);
        assert_eq!(by_host.display_name(), "localhost:8080");
    }

    #[test]
    fn servers_persist() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_servers(dir.path()).is_empty());
        save_servers(dir.path(), &[]).unwrap();
        assert!(!servers_path(dir.path()).exists());
        let s = vec![server("docker", "localhost", 8080)];
        save_servers(dir.path(), &s).unwrap();
        assert_eq!(load_servers(dir.path()), s);
    }
}
