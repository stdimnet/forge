//! Deployment servers of a project, kept in `.forge/deployment.toml` (PHPStorm's
//! `deployment.xml` / `webServers.xml`). Passwords never go into this file: FTP passwords
//! are in the OS keychain ([`crate::secrets`]), SFTP servers use a saved SSH configuration
//! (Settings | Tools | SSH Configurations) with its own keychain entry.

use serde::{Deserialize, Serialize};

/// Where the project file lives, relative to the project root.
pub const PROJECT_FILE: &str = ".forge/deployment.toml";

/// The server type (PHPStorm's "Type" of a deployment server).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServerKind {
    /// SFTP over a saved SSH configuration.
    #[default]
    Sftp,
    Ftp,
    /// FTP over explicit TLS (`AUTH TLS`).
    Ftps,
    /// A local or mounted folder.
    Local,
}

impl ServerKind {
    pub fn label(self) -> &'static str {
        match self {
            ServerKind::Sftp => "SFTP",
            ServerKind::Ftp => "FTP",
            ServerKind::Ftps => "FTPS",
            ServerKind::Local => "Local or mounted folder",
        }
    }

    pub fn is_ftp(self) -> bool {
        matches!(self, ServerKind::Ftp | ServerKind::Ftps)
    }
}

/// A path mapping (Mappings tab): a project folder, where it is deployed below the root
/// path, and its URL below the web server URL.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Mapping {
    /// Project-relative folder, `""` for the project root.
    pub local: String,
    /// Deployment path relative to the server's root path, `"/"` or `""` for the root.
    pub deployment: String,
    /// Web path relative to the web server URL.
    pub web: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExcludedSide {
    /// A project path (relative to the project root).
    #[default]
    Local,
    /// A deployment path (relative to the server's root path).
    Deployment,
}

/// An excluded path (Excluded Paths tab): never uploaded, downloaded, synced or deleted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExcludedPath {
    pub side: ExcludedSide,
    pub path: String,
}

/// One deployment server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeployServer {
    pub id: String,
    pub name: String,
    pub kind: ServerKind,
    /// SFTP: id of the SSH configuration to connect with.
    pub ssh_config: String,
    /// FTP/FTPS: host name and port (21 when unset).
    pub host: String,
    pub port: Option<u16>,
    /// FTP/FTPS: user name (ignored for anonymous logins).
    pub user: String,
    pub anonymous: bool,
    /// FTP/FTPS: passive mode (PHPStorm's default).
    pub passive: bool,
    /// The folder every deployment path is relative to: an absolute path on the server
    /// (empty: the login folder) or, for local servers, the target folder on this machine.
    pub root_path: String,
    /// Web server URL (`https://shop.example.com`), used for "Open in Browser".
    pub web_url: String,
    pub mappings: Vec<Mapping>,
    pub excluded: Vec<ExcludedPath>,
}

impl Default for DeployServer {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            kind: ServerKind::Sftp,
            ssh_config: String::new(),
            host: String::new(),
            port: None,
            user: String::new(),
            anonymous: false,
            passive: true,
            root_path: String::new(),
            web_url: String::new(),
            mappings: Vec::new(),
            excluded: Vec::new(),
        }
    }
}

impl DeployServer {
    /// The port used for FTP/FTPS.
    pub fn ftp_port(&self) -> u16 {
        self.port.unwrap_or(21)
    }

    /// What the server is for messages: its name, else the target.
    pub fn label(&self) -> String {
        if !self.name.trim().is_empty() {
            return self.name.trim().to_string();
        }
        match self.kind {
            ServerKind::Local => self.root_path.clone(),
            ServerKind::Sftp => self.ssh_config.clone(),
            _ => self.host.clone(),
        }
    }

    /// Problems that keep the server from being used (shown under the form).
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.name.trim().is_empty() {
            problems.push("Specify a server name".to_string());
        }
        match self.kind {
            ServerKind::Sftp if self.ssh_config.trim().is_empty() => {
                problems.push("Select an SSH configuration".to_string())
            }
            ServerKind::Ftp | ServerKind::Ftps => {
                if self.host.trim().is_empty() {
                    problems.push("Specify the host".to_string());
                }
                if !self.anonymous && self.user.trim().is_empty() {
                    problems.push("Specify the user name or log in anonymously".to_string());
                }
                if self.port == Some(0) {
                    problems.push("The port must be between 1 and 65535".to_string());
                }
            }
            ServerKind::Local if self.root_path.trim().is_empty() => {
                problems.push("Specify the folder".to_string())
            }
            _ => {}
        }
        let url = self.web_url.trim();
        if !url.is_empty() && !url.starts_with("http://") && !url.starts_with("https://") {
            problems.push("The web server URL must start with http:// or https://".to_string());
        }
        for (i, a) in self.mappings.iter().enumerate() {
            if self.mappings[..i]
                .iter()
                .any(|b| crate::mapping::normalize(&b.local) == crate::mapping::normalize(&a.local))
            {
                problems.push(format!(
                    "The local path {} is mapped twice",
                    if a.local.is_empty() { "/" } else { &a.local }
                ));
            }
        }
        problems
    }
}

/// The servers of a project and which one is the default (PHPStorm's "Use as Default").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeploymentConfig {
    pub default_server: Option<String>,
    pub servers: Vec<DeployServer>,
}

impl DeploymentConfig {
    /// Parse `.forge/deployment.toml`; an empty text is an empty configuration.
    pub fn parse(text: &str) -> crate::Result<Self> {
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        toml::from_str(text).map_err(|e| crate::DeployError::context("Invalid deployment.toml", e))
    }

    pub fn to_toml(&self) -> String {
        let header =
            "# Deployment servers (Settings | Build, Execution, Deployment | Deployment).\n\
                      # Passwords are kept in the system keychain, never in this file.\n";
        format!(
            "{header}{}",
            toml::to_string_pretty(self).unwrap_or_default()
        )
    }

    pub fn get(&self, id: &str) -> Option<&DeployServer> {
        self.servers.iter().find(|s| s.id == id)
    }

    /// The default server, if it still exists.
    pub fn default_server(&self) -> Option<&DeployServer> {
        self.default_server.as_deref().and_then(|id| self.get(id))
    }

    /// Insert or replace a server; a server without an id gets a fresh one. Returns its id.
    pub fn upsert(&mut self, mut server: DeployServer) -> String {
        if server.id.trim().is_empty() {
            server.id = self.new_id(&server.name);
        }
        let id = server.id.clone();
        match self.servers.iter_mut().find(|s| s.id == id) {
            Some(s) => *s = server,
            None => self.servers.push(server),
        }
        // The first server becomes the default, like in PHPStorm.
        if self.default_server().is_none() {
            self.default_server = Some(id.clone());
        }
        id
    }

    /// Remove a server; the default moves to the first remaining one.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.servers.len();
        self.servers.retain(|s| s.id != id);
        if self.default_server.as_deref() == Some(id) {
            self.default_server = self.servers.first().map(|s| s.id.clone());
        }
        self.servers.len() != before
    }

    /// A unique id derived from the name (`shop-production`, `shop-production-2`).
    pub fn new_id(&self, name: &str) -> String {
        let mut slug: String = name
            .trim()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        while slug.contains("--") {
            slug = slug.replace("--", "-");
        }
        let slug = slug.trim_matches('-');
        let base = if slug.is_empty() { "server" } else { slug };
        let mut id = base.to_string();
        let mut n = 2;
        while self.get(&id).is_some() {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ftp(name: &str) -> DeployServer {
        DeployServer {
            name: name.into(),
            kind: ServerKind::Ftp,
            host: "ftp.example.com".into(),
            user: "deploy".into(),
            root_path: "/htdocs".into(),
            ..DeployServer::default()
        }
    }

    #[test]
    fn round_trips_through_toml() {
        let mut config = DeploymentConfig::default();
        let mut server = ftp("Shop Production");
        server.mappings.push(Mapping {
            local: "public".into(),
            deployment: "/".into(),
            web: "/".into(),
        });
        server.excluded.push(ExcludedPath {
            side: ExcludedSide::Local,
            path: "storage".into(),
        });
        let id = config.upsert(server);
        assert_eq!(id, "shop-production");
        assert_eq!(config.default_server.as_deref(), Some("shop-production"));
        let text = config.to_toml();
        assert!(
            text.contains("defaultServer = \"shop-production\""),
            "{text}"
        );
        assert!(!text
            .lines()
            .any(|l| l.to_lowercase().starts_with("password")));
        assert_eq!(DeploymentConfig::parse(&text).unwrap(), config);
        assert_eq!(
            DeploymentConfig::parse("").unwrap(),
            DeploymentConfig::default()
        );
        assert!(DeploymentConfig::parse("servers = 3").is_err());
    }

    #[test]
    fn ids_are_unique_and_the_default_follows_removals() {
        let mut config = DeploymentConfig::default();
        let a = config.upsert(ftp("Shop"));
        let b = config.upsert(ftp("Shop"));
        assert_eq!((a.as_str(), b.as_str()), ("shop", "shop-2"));
        assert_eq!(config.new_id("  "), "server");
        assert!(config.remove("shop"));
        assert_eq!(config.default_server.as_deref(), Some("shop-2"));
        assert!(!config.remove("shop"));
        // Replacing keeps the position.
        let mut changed = config.get("shop-2").unwrap().clone();
        changed.host = "other".into();
        config.upsert(changed);
        assert_eq!(config.servers.len(), 1);
        assert_eq!(config.servers[0].host, "other");
    }

    #[test]
    fn validation_names_missing_fields() {
        assert!(ftp("Shop").validate().is_empty());
        let mut s = ftp("");
        s.host.clear();
        s.user.clear();
        s.web_url = "shop.example.com".into();
        let problems = s.validate();
        assert_eq!(problems.len(), 4, "{problems:?}");
        s.anonymous = true;
        assert_eq!(s.validate().len(), 3);
        let sftp = DeployServer {
            name: "x".into(),
            ..DeployServer::default()
        };
        assert_eq!(sftp.validate(), vec!["Select an SSH configuration"]);
        let local = DeployServer {
            name: "x".into(),
            kind: ServerKind::Local,
            ..DeployServer::default()
        };
        assert_eq!(local.validate(), vec!["Specify the folder"]);
        let mut twice = ftp("Shop");
        twice.mappings = vec![
            Mapping {
                local: "/public/".into(),
                ..Mapping::default()
            },
            Mapping {
                local: "public".into(),
                ..Mapping::default()
            },
        ];
        assert_eq!(
            twice.validate(),
            vec!["The local path public is mapped twice"]
        );
    }
}
