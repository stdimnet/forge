//! `config/database.yml`: the database of each environment (and each database of
//! multi-database setups), with the ERB Rails evaluates in it read the simple way:
//! `<%= ENV["X"] %>`, `<%= ENV.fetch("X") { 5 } %>`, `<%= ENV.fetch("X", "d") %>` and
//! `<%= ENV["X"] || "d" %>` take the variable or the default; other ERB is dropped.
//! `url:` (and `DATABASE_URL`) is split into its parts.

use serde::{Deserialize, Serialize};

use crate::yaml::{self, Value};
use crate::Location;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseConfig {
    /// `development`, `production/cache` (multi-database).
    pub environment: String,
    /// `postgresql`, `mysql2`, `trilogy`, `sqlite3`, …
    pub adapter: String,
    /// Forge's driver (`postgres`, `mysql`, `sqlite`, `sqlserver`, `oracle`); `None` when
    /// the Database tool window has none for the adapter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    pub database: String,
    pub username: String,
    /// SQLite's file (relative to the project).
    pub file: String,
    #[serde(skip)]
    pub password: Option<String>,
    pub has_password: bool,
    /// The environment's key in database.yml.
    pub location: Location,
}

/// The value of an ERB expression of database.yml.
fn eval(expr: &str, env: &dyn Fn(&str) -> Option<String>) -> String {
    let e = expr.trim();
    let unq = |s: &str| s.trim().trim_matches(['"', '\'']).to_string();
    let var_at = |s: &str| -> Option<(String, usize)> {
        // ENV["X"] / ENV['X'] / ENV.fetch("X" …
        let i = s.find("ENV")?;
        let rest = &s[i + 3..];
        let (q, after) = if let Some(r) = rest.strip_prefix('[') {
            (r, 1)
        } else if let Some(r) = rest.strip_prefix(".fetch(") {
            (r, 7)
        } else {
            return None;
        };
        let quote = q.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let end = q[1..].find(quote)?;
        Some((q[1..1 + end].to_string(), i + 3 + after + end + 2))
    };
    let Some((name, end)) = var_at(e) else {
        let v = unq(e);
        // A literal (`<%= 5 %>`) stays; Ruby code is dropped.
        return if v.chars().all(|c| c.is_alphanumeric() || "._-/:@".contains(c)) {
            v
        } else {
            String::new()
        };
    };
    if let Some(v) = env(&name).filter(|v| !v.is_empty()) {
        return v;
    }
    let rest = e[end.min(e.len())..].trim();
    // ENV.fetch("X") { 5 } / ENV.fetch("X", 5) / ENV["X"] || "d"
    let default = if let Some(r) = rest.strip_prefix(',') {
        r.trim_end_matches(')').to_string()
    } else if let Some(r) = rest.strip_prefix(")") {
        let r = r.trim();
        r.strip_prefix('{')
            .and_then(|x| x.strip_suffix('}'))
            .unwrap_or("")
            .to_string()
    } else if let Some(r) = rest.strip_prefix("]") {
        r.trim().strip_prefix("||").unwrap_or("").to_string()
    } else {
        String::new()
    };
    unq(&default)
}

/// Replace ERB tags with their values (`<% … %>` statements disappear).
pub fn render_erb(text: &str, env: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("<%") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        let Some(end) = after.find("%>") else {
            rest = "";
            break;
        };
        let inner = &after[..end];
        if let Some(expr) = inner.strip_prefix('=') {
            out.push_str(&eval(expr, env));
        }
        rest = &after[end + 2..];
        // A statement line leaves no empty line behind.
        if !inner.starts_with('=') && out.ends_with('\n') && rest.starts_with('\n') {
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

pub fn driver(adapter: &str) -> Option<&'static str> {
    match adapter {
        "postgresql" | "postgres" | "postgis" | "pg" => Some("postgres"),
        "mysql2" | "mysql" | "trilogy" => Some("mysql"),
        "sqlite3" | "sqlite" => Some("sqlite"),
        "sqlserver" => Some("sqlserver"),
        "oracle_enhanced" | "oracle" => Some("oracle"),
        _ => None,
    }
}

/// `postgres://user:pw@host:5432/db?x=y` into the configuration.
fn apply_url(c: &mut DatabaseConfig, url: &str) {
    let Some((scheme, rest)) = url.split_once("://") else {
        // `sqlite3:db/x.sqlite3`
        if let Some((s, file)) = url.split_once(':') {
            c.adapter = s.to_string();
            c.file = file.to_string();
        }
        return;
    };
    c.adapter = scheme.to_string();
    let rest = rest.split('?').next().unwrap_or(rest);
    if c.adapter.starts_with("sqlite") {
        c.file = rest.to_string();
        return;
    }
    let (auth, hostpart) = match rest.rsplit_once('@') {
        Some((a, h)) => (Some(a), h),
        None => (None, rest),
    };
    if let Some(a) = auth {
        let (u, p) = a.split_once(':').unwrap_or((a, ""));
        c.username = u.to_string();
        if !p.is_empty() {
            c.password = Some(p.to_string());
        }
    }
    let (hp, db) = hostpart.split_once('/').unwrap_or((hostpart, ""));
    if !db.is_empty() {
        c.database = db.to_string();
    }
    let (h, p) = hp.rsplit_once(':').unwrap_or((hp, ""));
    c.host = h.to_string();
    c.port = p.parse().ok();
}

fn config(env: &str, v: &Value, path: &str, line: u32, col: u32) -> DatabaseConfig {
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let mut c = DatabaseConfig {
        environment: env.to_string(),
        adapter: s("adapter"),
        host: s("host"),
        port: s("port").parse().ok(),
        database: s("database"),
        username: {
            let u = s("username");
            if u.is_empty() {
                s("user")
            } else {
                u
            }
        },
        location: Location {
            path: path.to_string(),
            line,
            character: col,
        },
        ..DatabaseConfig::default()
    };
    let pw = s("password");
    if !pw.is_empty() {
        c.password = Some(pw);
    }
    let url = s("url");
    if !url.is_empty() {
        apply_url(&mut c, &url);
    }
    if c.adapter.starts_with("sqlite") {
        if c.file.is_empty() {
            c.file = std::mem::take(&mut c.database);
        }
        c.database.clear();
    }
    c.driver = driver(&c.adapter).map(str::to_string);
    c.has_password = c.password.is_some();
    c
}

fn is_config(v: &Value) -> bool {
    v.is_map() && ["adapter", "database", "url"].iter().any(|k| v.get(k).is_some())
}

/// The databases of `config/database.yml`; `env` reads environment variables
/// (`DATABASE_URL` also applies to the environments without their own `url`).
pub fn parse(path: &str, text: &str, env: &dyn Fn(&str) -> Option<String>) -> Vec<DatabaseConfig> {
    let doc = yaml::parse(&render_erb(text, env));
    let mut out = Vec::new();
    for e in doc.entries() {
        if e.key == "default" || e.key.starts_with("default_") || e.key.starts_with('.') {
            continue;
        }
        if is_config(&e.value) {
            out.push(config(&e.key, &e.value, path, e.line, e.col));
        } else {
            for child in e.value.entries() {
                if is_config(&child.value) {
                    out.push(config(
                        &format!("{}/{}", e.key, child.key),
                        &child.value,
                        path,
                        child.line,
                        child.col,
                    ));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environments_with_erb_and_urls() {
        let yml = r#"default: &default
  adapter: postgresql
  encoding: unicode
  pool: <%= ENV.fetch("RAILS_MAX_THREADS") { 5 } %>
  host: <%= ENV.fetch("DB_HOST", "localhost") %>

development:
  <<: *default
  database: blog_development
  username: <%= ENV["DB_USER"] || "blog" %>
  password: <%= ENV["DB_PASSWORD"] %>

test:
  <<: *default
  database: blog_test

production:
  primary:
    <<: *default
    url: <%= ENV["DATABASE_URL"] %>
  cache:
    adapter: sqlite3
    database: storage/production_cache.sqlite3
"#;
        let env = |k: &str| match k {
            "DB_PASSWORD" => Some("s3cret".to_string()),
            "DATABASE_URL" => Some("postgres://deploy:pw@db.example.com:5433/blog_production".to_string()),
            _ => None,
        };
        let dbs = parse("config/database.yml", yml, &env);
        let got: Vec<String> = dbs
            .iter()
            .map(|d| {
                format!(
                    "{} {} {:?} {}:{} {} {} {} {}",
                    d.environment,
                    d.adapter,
                    d.driver,
                    d.host,
                    d.port.unwrap_or(0),
                    d.database,
                    d.username,
                    d.file,
                    d.has_password
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                "development postgresql Some(\"postgres\") localhost:0 blog_development blog  true",
                "test postgresql Some(\"postgres\") localhost:0 blog_test   false",
                "production/primary postgres Some(\"postgres\") db.example.com:5433 blog_production deploy  true",
                "production/cache sqlite3 Some(\"sqlite\") :0   storage/production_cache.sqlite3 false",
            ]
        );
        assert_eq!(dbs[0].password.as_deref(), Some("s3cret"));
        assert_eq!(dbs[0].location.line, 6);
        assert_eq!(render_erb("pool: <%= ENV.fetch(\"X\") { 5 } %>\n<% if x %>\na: 1\n", &|_| None), "pool: 5\na: 1\n");
    }
}
