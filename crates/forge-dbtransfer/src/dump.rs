//! Dump and restore with the databases' own tools (DataGrip's "Export with mysqldump /
//! pg_dump" and "Restore with …"): the command line of `mysqldump` / `mysql`, `pg_dump` /
//! `pg_restore` / `psql`, `sqlite3` and `sqlcmd` for a data source's connection. Passwords
//! go into the tool's environment variable (`MYSQL_PWD`, `PGPASSWORD`, `SQLCMDPASSWORD`),
//! never onto the command line.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DumpTool {
    #[default]
    Mysqldump,
    Mysql,
    PgDump,
    PgRestore,
    Psql,
    Sqlite3,
    Sqlcmd,
}

impl DumpTool {
    /// The executable looked up on the PATH when no path is set.
    pub fn program(self) -> &'static str {
        match self {
            DumpTool::Mysqldump => "mysqldump",
            DumpTool::Mysql => "mysql",
            DumpTool::PgDump => "pg_dump",
            DumpTool::PgRestore => "pg_restore",
            DumpTool::Psql => "psql",
            DumpTool::Sqlite3 => "sqlite3",
            DumpTool::Sqlcmd => "sqlcmd",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DumpAction {
    #[default]
    Dump,
    Restore,
}

/// A tool the dialog offers for a data source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolChoice {
    pub tool: DumpTool,
    pub action: DumpAction,
    /// `Export with 'mysqldump'`, …
    pub label: String,
}

/// The tools of a driver (`mysql`, `mariadb`, `postgres`, `sqlite`, `sqlserver`).
pub fn tools_for(driver: &str) -> Vec<ToolChoice> {
    let c = |tool: DumpTool, action: DumpAction| ToolChoice {
        tool,
        action,
        label: format!(
            "{} with '{}'",
            if action == DumpAction::Dump {
                "Export"
            } else {
                "Restore"
            },
            tool.program()
        ),
    };
    match driver {
        "mysql" | "mariadb" => vec![
            c(DumpTool::Mysqldump, DumpAction::Dump),
            c(DumpTool::Mysql, DumpAction::Restore),
        ],
        "postgres" => vec![
            c(DumpTool::PgDump, DumpAction::Dump),
            c(DumpTool::PgRestore, DumpAction::Restore),
            c(DumpTool::Psql, DumpAction::Restore),
        ],
        "sqlite" => vec![
            c(DumpTool::Sqlite3, DumpAction::Dump),
            c(DumpTool::Sqlite3, DumpAction::Restore),
        ],
        "sqlserver" => vec![
            c(DumpTool::Sqlcmd, DumpAction::Dump),
            c(DumpTool::Sqlcmd, DumpAction::Restore),
        ],
        _ => Vec::new(),
    }
}

/// Where the tool connects: the data source's settings (through its SSH tunnel: the local
/// end of the tunnel). The password is passed through the environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DumpTarget {
    pub driver: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub database: String,
    /// SQLite: the database file.
    pub file: String,
    pub ssl: bool,
    #[serde(skip)]
    pub password: Option<String>,
}

/// The options of a dump / restore dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DumpRequest {
    pub tool: DumpTool,
    pub action: DumpAction,
    /// Path of the executable; empty finds the tool on the PATH.
    pub program: String,
    /// The dump file (written by a dump, read by a restore).
    pub file: String,
    /// Schemas (MySQL: databases) to include; empty: the data source's database.
    pub schemas: Vec<String>,
    /// Tables to include (`schema.table` or `table`); empty: all.
    pub tables: Vec<String>,
    pub structure: bool,
    pub data: bool,
    /// `DROP` statements before each `CREATE` (`--add-drop-table`, `--clean --if-exists`).
    pub drop_objects: bool,
    /// Include `CREATE DATABASE` (`--databases`, `--create`).
    pub create_database: bool,
    /// One consistent snapshot (`--single-transaction`).
    pub single_transaction: bool,
    /// pg_dump: `plain`, `custom`, `directory` or `tar`.
    pub format: String,
    /// pg_dump / pg_restore: leave out ownership.
    pub no_owner: bool,
    /// pg_dump (directory format) and pg_restore: parallel jobs (0: one).
    pub jobs: u32,
    /// More arguments, split like a shell does.
    pub extra: String,
}

impl Default for DumpRequest {
    fn default() -> Self {
        Self {
            tool: DumpTool::Mysqldump,
            action: DumpAction::Dump,
            program: String::new(),
            file: String::new(),
            schemas: Vec::new(),
            tables: Vec::new(),
            structure: true,
            data: true,
            drop_objects: false,
            create_database: false,
            single_transaction: true,
            format: "plain".into(),
            no_owner: false,
            jobs: 0,
            extra: String::new(),
        }
    }
}

/// A command line to run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolCommand {
    pub program: String,
    pub args: Vec<String>,
    /// Environment on top of the inherited one (the password lives here).
    pub env: BTreeMap<String, String>,
}

impl ToolCommand {
    /// The command line as shown in the Run tool window (the environment is not shown).
    pub fn display(&self) -> String {
        std::iter::once(&self.program)
            .chain(&self.args)
            .map(|a| shell_quote(a))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn shell_quote(a: &str) -> String {
    if !a.is_empty()
        && a.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_=./:,@%+".contains(c))
    {
        a.to_string()
    } else {
        format!("'{}'", a.replace('\'', "'\\''"))
    }
}

/// Split `extra` like a shell (quotes and backslashes), without expansion.
pub fn split_args(s: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                started = true;
            }
            (None, '\\') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                    started = true;
                }
            }
            (None, c) if c.is_whitespace() => {
                if started || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if quote.is_some() {
        return Err("an argument has an unclosed quote".into());
    }
    if started || !cur.is_empty() {
        out.push(cur);
    }
    Ok(out)
}

/// A path inside a sqlite3 dot-command (`.output "…"`).
fn dot_path(p: &str) -> String {
    format!("\"{}\"", p.replace('\\', "\\\\").replace('"', "\\\""))
}

fn table_name(t: &str) -> &str {
    t.rsplit('.').next().unwrap_or(t)
}

/// The command line of a dump or restore.
pub fn command(req: &DumpRequest, target: &DumpTarget) -> Result<ToolCommand, String> {
    if req.file.trim().is_empty() {
        return Err(match req.action {
            DumpAction::Dump => "Choose the output file".into(),
            DumpAction::Restore => "Choose the file to restore".into(),
        });
    }
    if !req.structure && !req.data {
        return Err("Include the structure, the data or both".into());
    }
    let program = if req.program.trim().is_empty() {
        req.tool.program().to_string()
    } else {
        req.program.trim().to_string()
    };
    let mut args: Vec<String> = Vec::new();
    let mut env = BTreeMap::new();
    let host = if target.host.trim().is_empty() {
        "localhost".to_string()
    } else {
        target.host.trim().to_string()
    };
    let password = target.password.clone().filter(|p| !p.is_empty());
    let database = if target.database.is_empty() {
        req.schemas.first().cloned().unwrap_or_default()
    } else {
        target.database.clone()
    };
    let mysql_conn = |args: &mut Vec<String>| {
        args.push(format!("--host={host}"));
        if target.port != 0 {
            args.push(format!("--port={}", target.port));
        }
        if !target.user.is_empty() {
            args.push(format!("--user={}", target.user));
        }
        if target.ssl {
            args.push("--ssl-mode=REQUIRED".into());
        }
    };
    let pg_conn = |args: &mut Vec<String>, env: &mut BTreeMap<String, String>| {
        args.push(format!("--host={host}"));
        if target.port != 0 {
            args.push(format!("--port={}", target.port));
        }
        if !target.user.is_empty() {
            args.push(format!("--username={}", target.user));
        }
        if !database.is_empty() {
            args.push(format!("--dbname={database}"));
        }
        args.push("--no-password".into());
        if target.ssl {
            env.insert("PGSSLMODE".into(), "require".into());
        }
    };
    let pg_filters = |args: &mut Vec<String>| {
        for s in &req.schemas {
            args.push(format!("--schema={s}"));
        }
        for t in &req.tables {
            args.push(format!("--table={t}"));
        }
        if req.no_owner {
            args.push("--no-owner".into());
        }
        if req.structure && !req.data {
            args.push("--schema-only".into());
        }
        if req.data && !req.structure {
            args.push("--data-only".into());
        }
        if req.drop_objects {
            args.push("--clean".into());
            args.push("--if-exists".into());
        }
        if req.create_database {
            args.push("--create".into());
        }
    };
    match (req.tool, req.action) {
        (DumpTool::Mysqldump, _) => {
            if let Some(p) = password {
                env.insert("MYSQL_PWD".into(), p);
            }
            mysql_conn(&mut args);
            args.push(format!("--result-file={}", req.file));
            if req.single_transaction {
                args.push("--single-transaction".into());
            }
            if req.structure {
                args.push("--routines".into());
            } else {
                args.push("--no-create-info".into());
                args.push("--skip-triggers".into());
            }
            if !req.data {
                args.push("--no-data".into());
            }
            args.push(
                if req.drop_objects {
                    "--add-drop-table"
                } else {
                    "--skip-add-drop-table"
                }
                .into(),
            );
            let dbs: Vec<String> = if req.schemas.is_empty() {
                vec![database.clone()]
            } else {
                req.schemas.clone()
            };
            if req.tables.is_empty() && (dbs.len() > 1 || req.create_database) {
                args.push("--databases".into());
                args.extend(dbs);
            } else {
                let db = dbs.into_iter().next().unwrap_or_default();
                if db.is_empty() {
                    return Err("Choose the database to dump".into());
                }
                args.push(db);
                args.extend(req.tables.iter().map(|t| table_name(t).to_string()));
            }
        }
        (DumpTool::Mysql, _) => {
            if let Some(p) = password {
                env.insert("MYSQL_PWD".into(), p);
            }
            mysql_conn(&mut args);
            if !database.is_empty() && !req.create_database {
                args.push(format!("--database={database}"));
            }
            args.push(format!("--execute=source {}", req.file));
        }
        (DumpTool::PgDump, _) => {
            if let Some(p) = password {
                env.insert("PGPASSWORD".into(), p);
            }
            pg_conn(&mut args, &mut env);
            args.push(format!("--file={}", req.file));
            let format = match req.format.as_str() {
                "custom" | "directory" | "tar" => req.format.as_str(),
                _ => "plain",
            };
            args.push(format!("--format={format}"));
            if req.jobs > 1 && format == "directory" {
                args.push(format!("--jobs={}", req.jobs));
            }
            pg_filters(&mut args);
        }
        (DumpTool::PgRestore, _) => {
            if let Some(p) = password {
                env.insert("PGPASSWORD".into(), p);
            }
            pg_conn(&mut args, &mut env);
            if req.jobs > 1 {
                args.push(format!("--jobs={}", req.jobs));
            }
            pg_filters(&mut args);
            args.push("--exit-on-error".into());
            args.push(req.file.clone());
        }
        (DumpTool::Psql, _) => {
            if let Some(p) = password {
                env.insert("PGPASSWORD".into(), p);
            }
            pg_conn(&mut args, &mut env);
            args.push("--set=ON_ERROR_STOP=1".into());
            if req.single_transaction {
                args.push("--single-transaction".into());
            }
            args.push(format!("--file={}", req.file));
        }
        (DumpTool::Sqlite3, action) => {
            if target.file.trim().is_empty() {
                return Err("The data source has no database file".into());
            }
            args.push("-bail".into());
            args.push(target.file.clone());
            match action {
                DumpAction::Dump => {
                    args.push(format!(".output {}", dot_path(&req.file)));
                    let tables = req
                        .tables
                        .iter()
                        .map(|t| format!(" {}", dot_path(table_name(t))))
                        .collect::<String>();
                    args.push(if req.structure && !req.data {
                        format!(".schema{tables}")
                    } else if req.data && !req.structure {
                        format!(".dump --data-only{tables}")
                    } else {
                        format!(".dump{tables}")
                    });
                }
                DumpAction::Restore => args.push(format!(".read {}", dot_path(&req.file))),
            }
        }
        (DumpTool::Sqlcmd, action) => {
            if let Some(p) = password {
                env.insert("SQLCMDPASSWORD".into(), p);
            }
            args.push("-S".into());
            args.push(if target.port != 0 {
                format!("{host},{}", target.port)
            } else {
                host.clone()
            });
            if !target.user.is_empty() {
                args.push("-U".into());
                args.push(target.user.clone());
            }
            if target.ssl {
                args.push("-N".into());
            }
            args.push("-b".into());
            let db = forge_sql::Dialect::SqlServer.quote_ident(&database);
            let file = forge_sql::Dialect::SqlServer.quote_str(&req.file);
            match action {
                DumpAction::Dump => {
                    if database.is_empty() {
                        return Err("Choose the database to back up".into());
                    }
                    args.push("-Q".into());
                    args.push(format!(
                        "BACKUP DATABASE {db} TO DISK = {file} WITH {}COPY_ONLY",
                        if req.drop_objects { "INIT, " } else { "" }
                    ));
                }
                DumpAction::Restore if req.file.to_ascii_lowercase().ends_with(".bak") => {
                    args.push("-Q".into());
                    args.push(format!(
                        "RESTORE DATABASE {db} FROM DISK = {file}{}",
                        if req.drop_objects {
                            " WITH REPLACE"
                        } else {
                            ""
                        }
                    ));
                }
                DumpAction::Restore => {
                    if !database.is_empty() {
                        args.push("-d".into());
                        args.push(database.clone());
                    }
                    args.push("-i".into());
                    args.push(req.file.clone());
                }
            }
        }
    }
    args.extend(split_args(&req.extra)?);
    Ok(ToolCommand { program, args, env })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(driver: &str) -> DumpTarget {
        DumpTarget {
            driver: driver.into(),
            host: "db.local".into(),
            port: 3306,
            user: "forge".into(),
            database: "shop".into(),
            file: String::new(),
            ssl: false,
            password: Some("s3cret pass".into()),
        }
    }

    fn no_password_in_args(c: &ToolCommand) {
        assert!(c.args.iter().all(|a| !a.contains("s3cret")), "{:?}", c.args);
        assert!(!c.display().contains("s3cret"));
    }

    #[test]
    fn mysqldump_and_mysql() {
        let req = DumpRequest {
            tables: vec!["shop.orders".into(), "users".into()],
            drop_objects: true,
            ..Default::default()
        };
        let c = command(&req, &target("mysql")).unwrap_err();
        assert_eq!(c, "Choose the output file");
        let req = DumpRequest {
            file: "/tmp/shop dump.sql".into(),
            ..req
        };
        let c = command(&req, &target("mysql")).unwrap();
        assert_eq!(c.program, "mysqldump");
        assert_eq!(
            c.args,
            [
                "--host=db.local",
                "--port=3306",
                "--user=forge",
                "--result-file=/tmp/shop dump.sql",
                "--single-transaction",
                "--routines",
                "--add-drop-table",
                "shop",
                "orders",
                "users",
            ]
        );
        assert_eq!(
            c.env.get("MYSQL_PWD").map(String::as_str),
            Some("s3cret pass")
        );
        no_password_in_args(&c);
        assert!(c.display().contains("'--result-file=/tmp/shop dump.sql'"));

        let req = DumpRequest {
            file: "/tmp/all.sql".into(),
            schemas: vec!["shop".into(), "crm".into()],
            data: false,
            ..Default::default()
        };
        let c = command(&req, &target("mysql")).unwrap();
        assert!(c
            .args
            .ends_with(&["--databases".into(), "shop".into(), "crm".into()]));
        assert!(c.args.contains(&"--no-data".to_string()));

        let req = DumpRequest {
            tool: DumpTool::Mysql,
            action: DumpAction::Restore,
            file: "/tmp/all.sql".into(),
            program: "/opt/mysql/bin/mysql".into(),
            extra: "--force --default-character-set='utf8mb4'".into(),
            ..Default::default()
        };
        let mut t = target("mysql");
        t.ssl = true;
        let c = command(&req, &t).unwrap();
        assert_eq!(c.program, "/opt/mysql/bin/mysql");
        assert_eq!(
            c.args,
            [
                "--host=db.local",
                "--port=3306",
                "--user=forge",
                "--ssl-mode=REQUIRED",
                "--database=shop",
                "--execute=source /tmp/all.sql",
                "--force",
                "--default-character-set=utf8mb4",
            ]
        );
        no_password_in_args(&c);
    }

    #[test]
    fn pg_dump_pg_restore_and_psql() {
        let mut t = target("postgres");
        t.port = 5432;
        t.ssl = true;
        let req = DumpRequest {
            tool: DumpTool::PgDump,
            file: "/backups/shop.dump".into(),
            format: "directory".into(),
            jobs: 4,
            schemas: vec!["public".into()],
            tables: vec!["public.orders".into()],
            no_owner: true,
            structure: true,
            data: false,
            drop_objects: true,
            ..Default::default()
        };
        let c = command(&req, &t).unwrap();
        assert_eq!(
            c.args,
            [
                "--host=db.local",
                "--port=5432",
                "--username=forge",
                "--dbname=shop",
                "--no-password",
                "--file=/backups/shop.dump",
                "--format=directory",
                "--jobs=4",
                "--schema=public",
                "--table=public.orders",
                "--no-owner",
                "--schema-only",
                "--clean",
                "--if-exists",
            ]
        );
        assert_eq!(
            c.env.get("PGPASSWORD").map(String::as_str),
            Some("s3cret pass")
        );
        assert_eq!(c.env.get("PGSSLMODE").map(String::as_str), Some("require"));
        no_password_in_args(&c);

        let req = DumpRequest {
            tool: DumpTool::PgRestore,
            action: DumpAction::Restore,
            file: "/backups/shop.dump".into(),
            ..Default::default()
        };
        let c = command(&req, &t).unwrap();
        assert_eq!(c.program, "pg_restore");
        assert_eq!(c.args.last().unwrap(), "/backups/shop.dump");
        assert!(c.args.contains(&"--exit-on-error".to_string()));

        let req = DumpRequest {
            tool: DumpTool::Psql,
            action: DumpAction::Restore,
            file: "/backups/shop.sql".into(),
            ..Default::default()
        };
        let c = command(&req, &t).unwrap();
        assert_eq!(
            &c.args[5..],
            [
                "--set=ON_ERROR_STOP=1",
                "--single-transaction",
                "--file=/backups/shop.sql"
            ]
        );
        no_password_in_args(&c);
    }

    #[test]
    fn sqlite3_dot_commands() {
        let mut t = target("sqlite");
        t.file = "/data/app.db".into();
        t.password = None;
        let req = DumpRequest {
            tool: DumpTool::Sqlite3,
            file: "/tmp/my \"dump\".sql".into(),
            tables: vec!["main.users".into()],
            ..Default::default()
        };
        let c = command(&req, &t).unwrap();
        assert_eq!(
            c.args,
            [
                "-bail",
                "/data/app.db",
                ".output \"/tmp/my \\\"dump\\\".sql\"",
                ".dump \"users\""
            ]
        );
        assert!(c.env.is_empty());
        let schema_only = DumpRequest {
            data: false,
            tables: vec![],
            ..req.clone()
        };
        assert_eq!(command(&schema_only, &t).unwrap().args[3], ".schema");
        let restore = DumpRequest {
            action: DumpAction::Restore,
            file: "/tmp/in.sql".into(),
            ..req
        };
        assert_eq!(
            command(&restore, &t).unwrap().args[2],
            ".read \"/tmp/in.sql\""
        );
    }

    #[test]
    fn sqlcmd_backup_and_restore() {
        let mut t = target("sqlserver");
        t.port = 1433;
        let req = DumpRequest {
            tool: DumpTool::Sqlcmd,
            file: "C:\\backup\\shop.bak".into(),
            ..Default::default()
        };
        let c = command(&req, &t).unwrap();
        assert_eq!(
            c.args,
            [
                "-S",
                "db.local,1433",
                "-U",
                "forge",
                "-b",
                "-Q",
                "BACKUP DATABASE [shop] TO DISK = N'C:\\backup\\shop.bak' WITH COPY_ONLY",
            ]
        );
        assert_eq!(
            c.env.get("SQLCMDPASSWORD").map(String::as_str),
            Some("s3cret pass")
        );
        let restore = DumpRequest {
            action: DumpAction::Restore,
            drop_objects: true,
            ..req.clone()
        };
        assert_eq!(
            command(&restore, &t).unwrap().args.last().unwrap(),
            "RESTORE DATABASE [shop] FROM DISK = N'C:\\backup\\shop.bak' WITH REPLACE"
        );
        let script = DumpRequest {
            action: DumpAction::Restore,
            file: "/tmp/seed.sql".into(),
            ..req
        };
        let c = command(&script, &t).unwrap();
        assert!(c.args.ends_with(&[
            "-d".into(),
            "shop".into(),
            "-i".into(),
            "/tmp/seed.sql".into()
        ]));
        no_password_in_args(&c);
    }

    #[test]
    fn tools_and_arguments() {
        assert_eq!(tools_for("postgres").len(), 3);
        assert_eq!(tools_for("mariadb")[0].label, "Export with 'mysqldump'");
        assert!(tools_for("redis").is_empty());
        assert_eq!(
            split_args("a 'b c' \"d \\\"e\\\"\" f\\ g ''").unwrap(),
            ["a", "b c", "d \"e\"", "f g", ""]
        );
        assert!(split_args("'open").is_err());
        let req = DumpRequest {
            file: "x".into(),
            structure: false,
            data: false,
            ..Default::default()
        };
        assert!(command(&req, &target("mysql")).is_err());
    }
}
