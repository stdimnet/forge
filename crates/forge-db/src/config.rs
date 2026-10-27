//! Data source configuration: `.forge/dataSources.toml` in the project and
//! `<config dir>/forge/dataSources.toml` for global data sources. Passwords never go into
//! these files; they live in the OS keychain (see [`crate::secrets`]).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{DbError, DbResult};

pub use forge_sql::Dialect;

/// Database driver of a data source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Driver {
    #[default]
    Mysql,
    Mariadb,
    Postgres,
    Sqlite,
    /// Microsoft SQL Server and Azure SQL (TDS, `tiberius`).
    Sqlserver,
    /// ClickHouse over its HTTP interface.
    Clickhouse,
    Redis,
    Mongodb,
    /// Oracle through SQL*Plus or SQLcl installed by the user.
    Oracle,
}

/// How a driver's data is organised, which decides the tree and the console.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataModel {
    /// Schemas, tables, columns; SQL consoles.
    Relational,
    /// Redis: databases with keys; a command console.
    KeyValue,
    /// MongoDB: databases with collections of documents; a shell console.
    Document,
}

impl Driver {
    pub const ALL: [Driver; 9] = [
        Driver::Mysql,
        Driver::Mariadb,
        Driver::Postgres,
        Driver::Sqlite,
        Driver::Sqlserver,
        Driver::Clickhouse,
        Driver::Oracle,
        Driver::Redis,
        Driver::Mongodb,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Driver::Mysql => "MySQL",
            Driver::Mariadb => "MariaDB",
            Driver::Postgres => "PostgreSQL",
            Driver::Sqlite => "SQLite",
            Driver::Sqlserver => "SQL Server",
            Driver::Clickhouse => "ClickHouse",
            Driver::Redis => "Redis",
            Driver::Mongodb => "MongoDB",
            Driver::Oracle => "Oracle",
        }
    }

    pub fn default_port(self) -> Option<u16> {
        match self {
            Driver::Mysql | Driver::Mariadb => Some(3306),
            Driver::Postgres => Some(5432),
            Driver::Sqlite => None,
            Driver::Sqlserver => Some(1433),
            Driver::Clickhouse => Some(8123),
            Driver::Redis => Some(6379),
            Driver::Mongodb => Some(27017),
            Driver::Oracle => Some(1521),
        }
    }

    pub fn dialect(self) -> Dialect {
        match self {
            Driver::Mysql | Driver::Mariadb => Dialect::MySql,
            Driver::Postgres => Dialect::Postgres,
            Driver::Sqlite => Dialect::Sqlite,
            Driver::Sqlserver => Dialect::SqlServer,
            Driver::Clickhouse => Dialect::ClickHouse,
            Driver::Redis => Dialect::Redis,
            Driver::Mongodb => Dialect::Mongo,
            Driver::Oracle => Dialect::Oracle,
        }
    }

    pub fn model(self) -> DataModel {
        match self {
            Driver::Redis => DataModel::KeyValue,
            Driver::Mongodb => DataModel::Document,
            _ => DataModel::Relational,
        }
    }

    /// Whether consoles can switch to manual transactions (Commit/Rollback).
    pub fn supports_transactions(self) -> bool {
        matches!(
            self,
            Driver::Mysql | Driver::Mariadb | Driver::Postgres | Driver::Sqlite | Driver::Sqlserver
        )
    }

    /// Whether the data source is a file (no host, port or credentials).
    pub fn is_file(self) -> bool {
        self == Driver::Sqlite
    }
}

/// Where a data source is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    #[default]
    Project,
    Global,
}

/// SSL/TLS of the connection, like libpq's `sslmode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SslMode {
    Disable,
    /// TLS when the server offers it, without verifying the certificate.
    #[default]
    Prefer,
    /// TLS without verifying the certificate.
    Require,
    /// TLS with a certificate signed by the CA file (or the system roots).
    VerifyCa,
    /// Like `VerifyCa`, and the certificate must name the host.
    VerifyFull,
}

impl SslMode {
    pub fn verifies(self) -> bool {
        matches!(self, SslMode::VerifyCa | SslMode::VerifyFull)
    }

    pub fn enabled(self) -> bool {
        self != SslMode::Disable
    }
}

/// Certificate files of the SSL/TLS tab. The client key's passphrase is kept in the
/// keychain (account `<id>#ssl-key`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SslFiles {
    /// CA certificate (PEM) the server certificate must chain to.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ca_file: String,
    /// Client certificate (PEM).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cert_file: String,
    /// Client private key (PEM, PKCS#8 may be encrypted).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub key_file: String,
}

impl SslFiles {
    pub fn is_empty(&self) -> bool {
        self.ca_file.is_empty() && self.cert_file.is_empty() && self.key_file.is_empty()
    }
}

/// The SSH tab: reach the database through an SSH tunnel opened by Forge. The password or
/// key passphrase of an ad-hoc tunnel is kept in the keychain (account `<id>#ssh`); a saved
/// SSH configuration uses its own secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshTunnel {
    #[serde(default)]
    pub enabled: bool,
    /// A saved SSH configuration (Settings | Tools | SSH Configurations); the fields below
    /// are used when this is not set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration_id: Option<String>,
    #[serde(default)]
    pub host: String,
    #[serde(default = "ssh_port")]
    pub port: u16,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub auth: forge_remote::AuthMethod,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_file: Option<String>,
}

fn ssh_port() -> u16 {
    22
}

impl Default for SshTunnel {
    fn default() -> Self {
        Self {
            enabled: false,
            configuration_id: None,
            host: String::new(),
            port: ssh_port(),
            user: String::new(),
            auth: forge_remote::AuthMethod::default(),
            key_file: None,
        }
    }
}

impl SshTunnel {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// `user@host:port` of an ad-hoc tunnel.
    pub fn label(&self) -> String {
        match self.port {
            22 => format!("{}@{}", self.user, self.host),
            p => format!("{}@{}:{p}", self.user, self.host),
        }
    }
}

/// Colour tags of the data source (tree, console tabs and editor): `green` for development,
/// `yellow` for staging and `red` for production.
pub const PRODUCTION_COLOR: &str = "red";

fn yes() -> bool {
    true
}

fn default_timeout() -> u32 {
    10
}

/// The "Options" tab of the Data Sources and Drivers dialog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSourceOptions {
    #[serde(default)]
    pub read_only: bool,
    /// New consoles start in auto-commit mode.
    #[serde(default = "yes")]
    pub auto_commit: bool,
    #[serde(default)]
    pub ssl_mode: SslMode,
    #[serde(default, skip_serializing_if = "SslFiles::is_empty")]
    pub ssl: SslFiles,
    #[serde(default, skip_serializing_if = "SshTunnel::is_default")]
    pub ssh: SshTunnel,
    /// Schemas shown in the Database tool window; empty means all.
    #[serde(default)]
    pub schemas: Vec<String>,
    #[serde(default = "default_timeout")]
    pub connect_timeout_secs: u32,
    /// SQL run on every new connection ("Startup script").
    #[serde(default)]
    pub init_sql: String,
    /// Keep the password in the OS keychain ("Save: Forever").
    #[serde(default = "yes")]
    pub save_password: bool,
    /// Rows per page in result sets and table editors.
    #[serde(default)]
    pub page_size: Option<u32>,
    /// Colour tag: `green` (development), `yellow` (staging), `red` (production) or another
    /// theme colour name like `blue`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// "Single session mode": every console of the data source shares one connection.
    #[serde(default)]
    pub single_session: bool,
}

impl Default for DataSourceOptions {
    fn default() -> Self {
        Self {
            read_only: false,
            auto_commit: true,
            ssl_mode: SslMode::default(),
            ssl: SslFiles::default(),
            ssh: SshTunnel::default(),
            schemas: Vec::new(),
            connect_timeout_secs: default_timeout(),
            init_sql: String::new(),
            save_password: true,
            page_size: None,
            color: None,
            single_session: false,
        }
    }
}

/// One configured database connection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DataSource {
    pub id: String,
    pub name: String,
    pub driver: Driver,
    #[serde(default)]
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default)]
    pub database: String,
    #[serde(default)]
    pub user: String,
    /// SQLite database file, absolute or relative to the project root.
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub options: DataSourceOptions,
    /// Filled when loading; not stored in the file.
    #[serde(default, skip_serializing)]
    pub scope: Scope,
}

impl DataSource {
    pub fn dialect(&self) -> Dialect {
        self.driver.dialect()
    }

    /// Production data sources ask before unfiltered changes.
    pub fn is_production(&self) -> bool {
        self.options.color.as_deref() == Some(PRODUCTION_COLOR)
    }

    pub fn port_or_default(&self) -> u16 {
        self.port.or(self.driver.default_port()).unwrap_or(0)
    }

    /// The SQLite file resolved against the project root.
    pub fn sqlite_path(&self, project_root: Option<&Path>) -> PathBuf {
        let p = PathBuf::from(&self.file);
        match project_root {
            Some(root) if p.is_relative() => root.join(p),
            _ => p,
        }
    }

    /// Short description for the tree ("shop@127.0.0.1", "database.sqlite").
    pub fn summary(&self) -> String {
        match self.driver {
            Driver::Sqlite => self.file.clone(),
            Driver::Redis => format!(
                "{}:{}/{}",
                if self.host.is_empty() {
                    "localhost"
                } else {
                    &self.host
                },
                self.port_or_default(),
                if self.database.is_empty() {
                    "0"
                } else {
                    &self.database
                }
            ),
            _ => {
                let host = if self.host.is_empty() {
                    "localhost"
                } else {
                    &self.host
                };
                if self.database.is_empty() {
                    host.to_string()
                } else {
                    format!("{}@{host}", self.database)
                }
            }
        }
    }

    /// Whether `other` points at the same database (used to skip `.env` suggestions that
    /// are already configured).
    pub fn same_target(&self, other: &DataSource) -> bool {
        if self.driver.dialect() != other.driver.dialect() {
            return false;
        }
        match self.driver {
            Driver::Sqlite => self.file == other.file,
            _ => {
                let norm = |h: &str| match h {
                    "" | "localhost" | "127.0.0.1" | "::1" => "localhost".to_string(),
                    h => h.to_string(),
                };
                norm(&self.host) == norm(&other.host)
                    && self.port_or_default() == other.port_or_default()
                    && self.database == other.database
            }
        }
    }
}

/// Make an id from a name: `Shop (local)` → `shop-local-1f3a`.
pub fn new_id(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() ^ (d.as_secs() as u32))
        .unwrap_or(0);
    let slug = if slug.is_empty() {
        "db".to_string()
    } else {
        slug
    };
    format!("{slug}-{:04x}", nanos & 0xffff)
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct DataSourcesFile {
    #[serde(default, rename = "dataSource")]
    data_sources: Vec<DataSource>,
}

/// Project data sources file.
pub fn project_file(root: &Path) -> PathBuf {
    root.join(".forge").join("dataSources.toml")
}

/// Global data sources file (`<config dir>/forge/dataSources.toml`).
pub fn global_file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("forge").join("dataSources.toml"))
}

/// The project data sources file relative to the project root.
pub const PROJECT_FILE: &str = ".forge/dataSources.toml";

/// Read a data sources file; a missing file is an empty list.
pub fn load_file(path: &Path, scope: Scope) -> DbResult<Vec<DataSource>> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(DbError::Io(format!("{}: {e}", path.display()))),
    };
    parse_text(&text, scope, &path.display().to_string())
}

/// Data sources of a file's text (`origin` names it in errors).
pub fn parse_text(text: &str, scope: Scope, origin: &str) -> DbResult<Vec<DataSource>> {
    let file: DataSourcesFile =
        toml::from_str(text).map_err(|e| DbError::Config(format!("{origin}: {e}")))?;
    Ok(file
        .data_sources
        .into_iter()
        .map(|mut d| {
            d.scope = scope;
            d
        })
        .collect())
}

/// The text of a data sources file.
pub fn to_text(sources: &[DataSource]) -> DbResult<String> {
    let file = DataSourcesFile {
        data_sources: sources.to_vec(),
    };
    let text = toml::to_string_pretty(&file).map_err(|e| DbError::Config(e.to_string()))?;
    Ok(format!(
        "# Forge data sources. Passwords are kept in the system keychain.\n\n{text}"
    ))
}

/// Write a data sources file (creating `.forge/`); an empty list removes the file.
pub fn save_file(path: &Path, sources: &[DataSource]) -> DbResult<()> {
    if sources.is_empty() {
        return match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(DbError::Io(format!("{}: {e}", path.display()))),
        };
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| DbError::Io(format!("{}: {e}", dir.display())))?;
    }
    let text = to_text(sources)?;
    std::fs::write(path, text).map_err(|e| DbError::Io(format!("{}: {e}", path.display())))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 3 <= bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(b) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A data source parsed from a connection URL plus the password it contained.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedUrl {
    pub source: DataSource,
    pub password: Option<String>,
}

/// Parse a Doctrine/Symfony style URL: `mysql://user:pass@host:3306/db?serverVersion=8.0`,
/// `postgresql://…`, `sqlite:///%kernel.project_dir%/var/data.db`.
pub fn parse_url(url: &str) -> Option<ParsedUrl> {
    let (scheme, rest) = url.trim().split_once("://")?;
    let driver = match scheme.to_ascii_lowercase().as_str() {
        "mysql" | "mysql2" | "pdo-mysql" => Driver::Mysql,
        "mariadb" => Driver::Mariadb,
        "postgres" | "postgresql" | "pgsql" | "pdo-pgsql" => Driver::Postgres,
        "sqlite" | "sqlite3" | "pdo-sqlite" => Driver::Sqlite,
        "sqlsrv" | "pdo-sqlsrv" | "mssql" | "sqlserver" => Driver::Sqlserver,
        "oci8" | "pdo-oci" | "oracle" => Driver::Oracle,
        _ => return None,
    };
    let rest = rest.split(['?', '#']).next().unwrap_or("");
    let mut ds = DataSource {
        driver,
        ..Default::default()
    };
    if driver == Driver::Sqlite {
        // Doctrine: `sqlite:////abs/path` is absolute, `sqlite:///rel/path` relative.
        ds.file = if rest.contains(":memory:") {
            ":memory:".into()
        } else if rest.contains("%kernel.project_dir%") {
            rest.trim_start_matches('/')
                .replace("%kernel.project_dir%/", "")
                .replace("%kernel.project_dir%", "")
        } else if let Some(abs) = rest.strip_prefix("//") {
            format!("/{}", abs.trim_start_matches('/'))
        } else {
            rest.trim_start_matches('/').to_string()
        };
        return Some(ParsedUrl {
            source: ds,
            password: None,
        });
    }
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (userinfo, hostport) = match authority.rsplit_once('@') {
        Some((u, h)) => (Some(u), h),
        None => (None, authority),
    };
    let mut password = None;
    if let Some(u) = userinfo {
        match u.split_once(':') {
            Some((user, pass)) => {
                ds.user = percent_decode(user);
                password = Some(percent_decode(pass));
            }
            None => ds.user = percent_decode(u),
        }
    }
    let (host, port) = if let Some(stripped) = hostport.strip_prefix('[') {
        // [::1]:5432
        let (h, p) = stripped.split_once(']').unwrap_or((stripped, ""));
        (h.to_string(), p.trim_start_matches(':').parse().ok())
    } else {
        match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().ok()),
            None => (hostport.to_string(), None),
        }
    };
    ds.host = host;
    ds.port = port;
    ds.database = percent_decode(path.trim_end_matches('/'));
    Some(ParsedUrl {
        source: ds,
        password,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_symfony_urls() {
        let p = parse_url(
            "mysql://app:%21ChangeMe%21@127.0.0.1:3306/app?serverVersion=8.0.32&charset=utf8mb4",
        )
        .unwrap();
        assert_eq!(p.source.driver, Driver::Mysql);
        assert_eq!(p.source.user, "app");
        assert_eq!(p.password.as_deref(), Some("!ChangeMe!"));
        assert_eq!(p.source.host, "127.0.0.1");
        assert_eq!(p.source.port, Some(3306));
        assert_eq!(p.source.database, "app");

        let p = parse_url("postgresql://app:pw@db/app?serverVersion=16&charset=utf8").unwrap();
        assert_eq!(p.source.driver, Driver::Postgres);
        assert_eq!(p.source.host, "db");
        assert_eq!(p.source.port, None);
        assert_eq!(p.source.port_or_default(), 5432);

        let p = parse_url("sqlite:///%kernel.project_dir%/var/data.db").unwrap();
        assert_eq!(p.source.driver, Driver::Sqlite);
        assert_eq!(p.source.file, "var/data.db");

        let p = parse_url("sqlite:////tmp/x.db").unwrap();
        assert_eq!(p.source.file, "/tmp/x.db");
        let p = parse_url("sqlite:///var/app.db").unwrap();
        assert_eq!(p.source.file, "var/app.db");

        assert!(parse_url("redis://localhost").is_none());
        let p = parse_url("sqlsrv://sa:pw@db:1433/shop").unwrap();
        assert_eq!(p.source.driver, Driver::Sqlserver);
        assert_eq!(p.source.port_or_default(), 1433);
    }

    #[test]
    fn quotes_per_dialect() {
        assert_eq!(Dialect::MySql.quote_ident("a`b"), "`a``b`");
        assert_eq!(Dialect::Postgres.quote_ident("a\"b"), "\"a\"\"b\"");
        assert_eq!(Dialect::MySql.quote_str("it's \\"), "'it''s \\\\'");
        assert_eq!(Dialect::Sqlite.quote_str("it's \\"), "'it''s \\'");
        assert_eq!(Dialect::Sqlite.qualified("main", "users"), "\"users\"");
        assert_eq!(
            Dialect::Postgres.qualified("public", "users"),
            "\"public\".\"users\""
        );
    }

    #[test]
    fn file_round_trip_without_scope() {
        let dir = tempfile::tempdir().unwrap();
        let path = project_file(dir.path());
        let ds = DataSource {
            id: "shop".into(),
            name: "shop@localhost".into(),
            driver: Driver::Postgres,
            host: "localhost".into(),
            database: "shop".into(),
            user: "shop".into(),
            ..Default::default()
        };
        save_file(&path, std::slice::from_ref(&ds)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[[dataSource]]"));
        assert!(!text.contains("scope"));
        assert!(!text.contains("\npassword"));
        let loaded = load_file(&path, Scope::Global).unwrap();
        assert_eq!(loaded[0].scope, Scope::Global);
        assert_eq!(loaded[0].database, "shop");
        save_file(&path, &[]).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn same_target_normalizes_localhost() {
        let a = DataSource {
            driver: Driver::Mysql,
            host: "127.0.0.1".into(),
            database: "shop".into(),
            ..Default::default()
        };
        let b = DataSource {
            driver: Driver::Mariadb,
            host: "localhost".into(),
            port: Some(3306),
            database: "shop".into(),
            ..Default::default()
        };
        assert!(a.same_target(&b));
    }

    #[test]
    fn ssl_and_ssh_options_round_trip() {
        let text = r#"
[[dataSource]]
id = "prod"
name = "prod"
driver = "sqlserver"
host = "db.internal"

[dataSource.options]
sslMode = "verify-full"
color = "red"

[dataSource.options.ssl]
caFile = "/etc/ssl/ca.pem"

[dataSource.options.ssh]
enabled = true
host = "bastion.example.com"
user = "deploy"
auth = "keyPair"
keyFile = "~/.ssh/id_ed25519"
"#;
        let list = parse_text(text, Scope::Project, "test").unwrap();
        let ds = &list[0];
        assert_eq!(ds.options.ssl_mode, SslMode::VerifyFull);
        assert_eq!(ds.options.ssl.ca_file, "/etc/ssl/ca.pem");
        assert!(ds.options.ssh.enabled);
        assert_eq!(ds.options.ssh.port, 22);
        assert_eq!(ds.options.ssh.label(), "deploy@bastion.example.com");
        assert!(ds.is_production());
        let back = to_text(&list).unwrap();
        assert!(back.contains("verify-full"));
        assert!(back.contains("bastion.example.com"));
        // Defaults stay out of the file.
        let plain = to_text(&[DataSource::default()]).unwrap();
        assert!(!plain.contains("ssh"));
        assert!(!plain.contains("caFile"));
    }

    #[test]
    fn ids_are_slugs() {
        assert!(new_id("Shop (local)").starts_with("shop-local-"));
    }
}
