//! Data source configuration: `.forge/dataSources.toml` in the project and
//! `<config dir>/forge/dataSources.toml` for global data sources. Passwords never go into
//! these files; they live in the OS keychain (see [`crate::secrets`]).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{DbError, DbResult};

/// Database driver of a data source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Driver {
    #[default]
    Mysql,
    Mariadb,
    Postgres,
    Sqlite,
}

impl Driver {
    pub fn label(self) -> &'static str {
        match self {
            Driver::Mysql => "MySQL",
            Driver::Mariadb => "MariaDB",
            Driver::Postgres => "PostgreSQL",
            Driver::Sqlite => "SQLite",
        }
    }

    pub fn default_port(self) -> Option<u16> {
        match self {
            Driver::Mysql | Driver::Mariadb => Some(3306),
            Driver::Postgres => Some(5432),
            Driver::Sqlite => None,
        }
    }

    pub fn dialect(self) -> Dialect {
        match self {
            Driver::Mysql | Driver::Mariadb => Dialect::MySql,
            Driver::Postgres => Dialect::Postgres,
            Driver::Sqlite => Dialect::Sqlite,
        }
    }
}

/// SQL flavour used to quote identifiers and literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    MySql,
    Postgres,
    Sqlite,
}

impl Dialect {
    /// Quote an identifier (`users` → `` `users` `` or `"users"`).
    pub fn quote_ident(self, name: &str) -> String {
        match self {
            Dialect::MySql => format!("`{}`", name.replace('`', "``")),
            Dialect::Postgres | Dialect::Sqlite => format!("\"{}\"", name.replace('"', "\"\"")),
        }
    }

    /// Quote a string literal.
    pub fn quote_str(self, s: &str) -> String {
        match self {
            Dialect::MySql => format!("'{}'", s.replace('\\', "\\\\").replace('\'', "''")),
            Dialect::Postgres | Dialect::Sqlite => format!("'{}'", s.replace('\'', "''")),
        }
    }

    /// `schema.table`, leaving the schema out for SQLite's `main`.
    pub fn qualified(self, schema: &str, table: &str) -> String {
        if schema.is_empty() || (self == Dialect::Sqlite && schema == "main") {
            self.quote_ident(table)
        } else {
            format!("{}.{}", self.quote_ident(schema), self.quote_ident(table))
        }
    }

    /// Statement that starts a transaction.
    pub fn begin(self) -> &'static str {
        match self {
            Dialect::MySql => "START TRANSACTION",
            Dialect::Postgres | Dialect::Sqlite => "BEGIN",
        }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SslMode {
    Disable,
    #[default]
    Prefer,
    Require,
}

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
    /// Optional color tag, a theme color name like "blue".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl Default for DataSourceOptions {
    fn default() -> Self {
        Self {
            read_only: false,
            auto_commit: true,
            ssl_mode: SslMode::default(),
            schemas: Vec::new(),
            connect_timeout_secs: default_timeout(),
            init_sql: String::new(),
            save_password: true,
            page_size: None,
            color: None,
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

/// Read a data sources file; a missing file is an empty list.
pub fn load_file(path: &Path, scope: Scope) -> DbResult<Vec<DataSource>> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(DbError::Io(format!("{}: {e}", path.display()))),
    };
    let file: DataSourcesFile =
        toml::from_str(&text).map_err(|e| DbError::Config(format!("{}: {e}", path.display())))?;
    Ok(file
        .data_sources
        .into_iter()
        .map(|mut d| {
            d.scope = scope;
            d
        })
        .collect())
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
    let file = DataSourcesFile {
        data_sources: sources.to_vec(),
    };
    let text = toml::to_string_pretty(&file).map_err(|e| DbError::Config(e.to_string()))?;
    let text =
        format!("# Forge data sources. Passwords are kept in the system keychain.\n\n{text}");
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
    fn ids_are_slugs() {
        assert!(new_id("Shop (local)").starts_with("shop-local-"));
    }
}
