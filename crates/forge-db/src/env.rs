//! Data sources suggested from a project's `.env` files: Laravel's `DB_*` variables and
//! Symfony/Doctrine's `DATABASE_URL`.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::config::{parse_url, DataSource, Driver};

/// A data source found in an env file, with the password it contained (if any).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedDataSource {
    pub source: DataSource,
    #[serde(skip_serializing)]
    pub password: Option<String>,
    pub has_password: bool,
    /// File the settings came from, e.g. `.env`.
    pub origin: String,
}

/// Parse dotenv text into a map (later keys win, `${VAR}` references are expanded).
pub fn parse_dotenv(text: &str) -> BTreeMap<String, String> {
    let mut vars = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_string();
        let value = value.trim();
        let value = if let Some(v) = value.strip_prefix('"') {
            v.split('"').next().unwrap_or("").replace("\\n", "\n")
        } else if let Some(v) = value.strip_prefix('\'') {
            v.split('\'').next().unwrap_or("").to_string()
        } else {
            // Unquoted: an inline comment starts with " #".
            value.split(" #").next().unwrap_or("").trim().to_string()
        };
        let expanded = expand(&value, &vars);
        vars.insert(key, expanded);
    }
    vars
}

fn expand(value: &str, vars: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find('}') {
            Some(end) => {
                let name = &rest[start + 2..start + 2 + end];
                out.push_str(vars.get(name).map(String::as_str).unwrap_or(""));
                rest = &rest[start + 3 + end..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Data source described by Laravel `DB_*` variables.
pub fn from_laravel(vars: &BTreeMap<String, String>) -> Option<(DataSource, Option<String>)> {
    let conn = vars.get("DB_CONNECTION")?.to_ascii_lowercase();
    let driver = match conn.as_str() {
        "mysql" => Driver::Mysql,
        "mariadb" => Driver::Mariadb,
        "pgsql" | "postgres" | "postgresql" => Driver::Postgres,
        "sqlite" => Driver::Sqlite,
        _ => return None,
    };
    if let Some(url) = vars
        .get("DB_URL")
        .or_else(|| vars.get("DATABASE_URL"))
        .filter(|u| !u.is_empty())
    {
        if let Some(p) = parse_url(url) {
            return Some((p.source, p.password));
        }
    }
    let get = |k: &str| vars.get(k).cloned().unwrap_or_default();
    let mut ds = DataSource {
        driver,
        ..Default::default()
    };
    if driver == Driver::Sqlite {
        let db = get("DB_DATABASE");
        ds.file = if db.is_empty() {
            "database/database.sqlite".into()
        } else {
            db
        };
        return Some((ds, None));
    }
    ds.host = get("DB_HOST");
    if ds.host.is_empty() {
        ds.host = "127.0.0.1".into();
    }
    ds.port = vars.get("DB_PORT").and_then(|p| p.parse().ok());
    ds.database = get("DB_DATABASE");
    ds.user = get("DB_USERNAME");
    let password = vars.get("DB_PASSWORD").filter(|p| !p.is_empty()).cloned();
    Some((ds, password))
}

/// Data source described by Symfony/Doctrine's `DATABASE_URL`.
pub fn from_symfony(vars: &BTreeMap<String, String>) -> Option<(DataSource, Option<String>)> {
    let url = vars.get("DATABASE_URL")?;
    let p = parse_url(url)?;
    Some((p.source, p.password))
}

/// Look at `.env` and `.env.local` in the project root. `.env.local` overrides `.env`, as in
/// Symfony; Laravel only reads `.env`.
pub fn detect(root: &Path) -> Vec<DetectedDataSource> {
    let mut vars = BTreeMap::new();
    let mut origin = String::new();
    for name in [".env", ".env.local"] {
        if let Ok(text) = std::fs::read_to_string(root.join(name)) {
            let parsed = parse_dotenv(&text);
            if parsed.contains_key("DB_CONNECTION") || parsed.contains_key("DATABASE_URL") {
                origin = name.to_string();
            }
            vars.extend(parsed);
        }
    }
    let found = from_laravel(&vars).or_else(|| from_symfony(&vars));
    found
        .map(|(mut source, password)| {
            source.name = default_name(&source);
            DetectedDataSource {
                has_password: password.is_some(),
                source,
                password,
                origin: if origin.is_empty() {
                    ".env".into()
                } else {
                    origin
                },
            }
        })
        .into_iter()
        .collect()
}

/// PHPStorm-like default name: `shop@localhost`, `database.sqlite`.
pub fn default_name(ds: &DataSource) -> String {
    match ds.driver {
        Driver::Sqlite => Path::new(&ds.file)
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| ds.file.clone()),
        _ => {
            let host = match ds.host.as_str() {
                "" | "127.0.0.1" | "::1" => "localhost",
                h => h,
            };
            if ds.database.is_empty() {
                host.to_string()
            } else {
                format!("{}@{host}", ds.database)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotenv_quotes_comments_and_references() {
        let v = parse_dotenv(
            "# comment\nAPP_NAME=\"Acme Shop\"\nDB_HOST=127.0.0.1 # local\nexport DB_USER='root'\nDB_DATABASE=${APP_SLUG}_db\nAPP_SLUG=x\nURL=${APP_NAME}/x\n",
        );
        assert_eq!(v["APP_NAME"], "Acme Shop");
        assert_eq!(v["DB_HOST"], "127.0.0.1");
        assert_eq!(v["DB_USER"], "root");
        assert_eq!(v["DB_DATABASE"], "_db");
        assert_eq!(v["URL"], "Acme Shop/x");
    }

    #[test]
    fn detects_laravel_env() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".env"),
            "DB_CONNECTION=mysql\nDB_HOST=127.0.0.1\nDB_PORT=3306\nDB_DATABASE=shop\nDB_USERNAME=shop\nDB_PASSWORD=secret\n",
        )
        .unwrap();
        let found = detect(dir.path());
        assert_eq!(found.len(), 1);
        let d = &found[0];
        assert_eq!(d.source.driver, Driver::Mysql);
        assert_eq!(d.source.name, "shop@localhost");
        assert_eq!(d.source.user, "shop");
        assert_eq!(d.password.as_deref(), Some("secret"));
        assert_eq!(d.origin, ".env");
    }

    #[test]
    fn detects_laravel_sqlite_default_file() {
        let vars = parse_dotenv("DB_CONNECTION=sqlite\n");
        let (ds, _) = from_laravel(&vars).unwrap();
        assert_eq!(ds.file, "database/database.sqlite");
        assert_eq!(default_name(&ds), "database.sqlite");
    }

    #[test]
    fn detects_symfony_database_url_with_local_override() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".env"),
            "DATABASE_URL=\"postgresql://app:!ChangeMe!@127.0.0.1:5432/app?serverVersion=16&charset=utf8\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join(".env.local"),
            "DATABASE_URL=\"postgresql://dev:pw@localhost:5433/dev\"\n",
        )
        .unwrap();
        let found = detect(dir.path());
        assert_eq!(found[0].source.driver, Driver::Postgres);
        assert_eq!(found[0].source.database, "dev");
        assert_eq!(found[0].source.port, Some(5433));
        assert_eq!(found[0].origin, ".env.local");
    }

    #[test]
    fn nothing_without_db_settings() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".env"), "APP_ENV=local\n").unwrap();
        assert!(detect(dir.path()).is_empty());
    }
}
