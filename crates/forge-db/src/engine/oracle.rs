//! Oracle through a command-line client the user installed: SQL*Plus (Oracle Instant
//! Client) or SQLcl, found on `PATH`. There is no permissive pure-Rust driver for Oracle's
//! protocol, and Forge never bundles the Oracle client, so every statement runs in a fresh
//! `sqlplus -S -L /nolog` (or `sql -S /nolog`) process that prints CSV. The password goes
//! through the process's stdin, never its command line. Consequences: no transactions
//! across executions (each statement commits when the process exits), and cancelling kills
//! the process.

use std::any::Any;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use futures_util::future::BoxFuture;
use serde_json::Value;
use tokio::io::AsyncWriteExt;

use super::{page_rows, Canceller, ConnectSpec, EngineConn, EnginePool};
use crate::config::Dialect;
use crate::conn::{ColumnMeta, RawResult};
use crate::error::{DbError, DbResult};

const BEGIN: &str = "FORGE-RESULT-BEGIN";
const END: &str = "FORGE-RESULT-END";

/// Which client prints the results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    SqlPlus,
    SqlCl,
}

/// The client on `PATH`: `sqlplus` first, then SQLcl's `sql`.
pub fn find_tool(path_var: Option<&std::ffi::OsStr>) -> Option<(PathBuf, Tool)> {
    let dirs: Vec<PathBuf> = std::env::split_paths(path_var?).collect();
    let exe = |name: &str| {
        dirs.iter().find_map(|d| {
            [
                name.to_string(),
                format!("{name}.exe"),
                format!("{name}.cmd"),
            ]
            .into_iter()
            .map(|n| d.join(n))
            .find(|p| forge_hostspawn::is_file(p))
        })
    };
    exe("sqlplus")
        .map(|p| (p, Tool::SqlPlus))
        .or_else(|| exe("sql").map(|p| (p, Tool::SqlCl)))
}

#[derive(Clone)]
struct Settings {
    tool: PathBuf,
    kind: Tool,
    user: String,
    password: String,
    /// `//host:port/service`.
    target: String,
    read_only: bool,
    init: String,
}

pub struct OraclePool {
    settings: Arc<Settings>,
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', ""))
}

/// The script for one statement: CSV output between markers, errors end the process.
pub fn script(
    kind: Tool,
    user: &str,
    password: &str,
    target: &str,
    read_only: bool,
    init: &str,
    statement: &str,
) -> String {
    let mut s = String::new();
    s.push_str("SET ECHO OFF\nSET VERIFY OFF\nSET FEEDBACK ON\nSET HEADING ON\nSET PAGESIZE 50000\nSET LINESIZE 32767\nSET TRIMSPOOL ON\nSET TRIMOUT ON\nSET LONG 100000\nSET NULL \"\"\n");
    match kind {
        Tool::SqlPlus => s.push_str("SET MARKUP CSV ON QUOTE ON\n"),
        Tool::SqlCl => s.push_str("SET SQLFORMAT CSV\n"),
    }
    s.push_str("WHENEVER SQLERROR EXIT SQL.SQLCODE\nWHENEVER OSERROR EXIT FAILURE\n");
    s.push_str(&format!(
        "CONNECT {}/{}@{}\n",
        quote(user),
        quote(password),
        quote(target)
    ));
    if read_only {
        s.push_str("SET TRANSACTION READ ONLY;\n");
    }
    for st in crate::split::split_statements(init, Dialect::Oracle) {
        s.push_str(&terminated(&st.text));
    }
    s.push_str(&format!("PROMPT {BEGIN}\n"));
    s.push_str(&terminated(statement));
    s.push_str(&format!("PROMPT {END}\nEXIT\n"));
    s
}

/// A statement as SQL*Plus wants it: `;` after SQL, `/` on its own line after PL/SQL.
fn terminated(statement: &str) -> String {
    let t = statement.trim().trim_end_matches('/').trim_end();
    let words = forge_sql::split::split_statements(t, Dialect::Oracle);
    let plsql = words.len() == 1 && {
        let kw = forge_sql::leading_keyword(t);
        kw == "BEGIN"
            || kw == "DECLARE"
            || t.to_ascii_uppercase().contains(" PROCEDURE ")
            || t.to_ascii_uppercase().contains(" FUNCTION ")
            || t.to_ascii_uppercase().contains(" PACKAGE ")
            || t.to_ascii_uppercase().contains(" TRIGGER ")
    };
    if plsql {
        format!("{t}\n/\n")
    } else {
        format!("{};\n", t.trim_end_matches(';'))
    }
}

fn csv_line(line: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => {
                in_quotes = !in_quotes;
                quoted = true;
            }
            ',' if !in_quotes => {
                out.push((std::mem::take(&mut cur), quoted));
                quoted = false;
            }
            c => cur.push(c),
        }
    }
    out.push((cur, quoted));
    out
}

fn csv_cell((text, quoted): (String, bool)) -> Value {
    if quoted {
        return Value::String(text);
    }
    if text.is_empty() {
        return Value::Null;
    }
    if let Ok(i) = text.parse::<i64>() {
        return crate::value::int(i);
    }
    match text.parse::<f64>() {
        Ok(f) if f.is_finite() && !text.contains(['e', 'E']) => crate::value::float(f),
        _ => Value::String(text),
    }
}

/// The error lines of SQL*Plus output (`ORA-00942: …`, `SP2-0734: …`, `PLS-00103: …`).
pub fn error_text(output: &str) -> Option<String> {
    let lines: Vec<&str> = output
        .lines()
        .map(str::trim)
        .filter(|l| {
            ["ORA-", "SP2-", "PLS-", "TNS-", "ERROR at line"]
                .iter()
                .any(|p| l.starts_with(p))
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// Parse the output of [`script`]: a CSV result set, an update count (`3 rows updated.`) or
/// a message (`Table created.`).
pub fn parse_output(output: &str) -> DbResult<RawResult> {
    let body = match (output.find(BEGIN), output.rfind(END)) {
        (Some(b), Some(e)) if e > b => &output[b + BEGIN.len()..e],
        _ => {
            return Err(DbError::Sql(error_text(output).unwrap_or_else(|| {
                let t = output.trim();
                if t.is_empty() {
                    "the Oracle client printed nothing".into()
                } else {
                    t.lines().take(5).collect::<Vec<_>>().join("\n")
                }
            })))
        }
    };
    if let Some(e) = error_text(body) {
        return Err(DbError::Sql(e));
    }
    let mut out = RawResult::default();
    let mut lines: Vec<&str> = body
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
        .collect();
    // Feedback: "3 rows updated.", "1 row selected.", "no rows selected", "Table created."
    if let Some(last) = lines.last().copied() {
        let words: Vec<&str> = last.split_whitespace().collect();
        let count = match words.as_slice() {
            ["no", "rows", ..] => Some(0),
            [n, "row" | "rows", ..] => n.parse::<u64>().ok(),
            _ => None,
        };
        if let Some(n) = count {
            lines.pop();
            if !last.contains("selected") {
                out.rows_affected = n;
            }
        } else if last.ends_with('.') && !last.contains(',') && !last.starts_with('"') {
            lines.pop();
            if lines.is_empty() {
                out.columns = vec![ColumnMeta {
                    name: "result".into(),
                    type_name: "VARCHAR2".into(),
                }];
                out.rows = vec![vec![Value::String(last.to_string())]];
                return Ok(out);
            }
        }
    }
    let mut it = lines.into_iter();
    if let Some(header) = it.next() {
        out.columns = csv_line(header)
            .into_iter()
            .map(|(name, _)| ColumnMeta {
                name,
                type_name: String::new(),
            })
            .collect();
        out.rows = it
            .map(|l| csv_line(l).into_iter().map(csv_cell).collect())
            .collect();
    }
    Ok(out)
}

impl OraclePool {
    pub fn connect(spec: &ConnectSpec<'_>) -> DbResult<Self> {
        let (tool, kind) = find_tool(forge_hostspawn::path_var().as_deref()).ok_or_else(|| {
            DbError::Config(
                "Oracle needs SQL*Plus (Oracle Instant Client with the SQL*Plus package) or SQLcl on PATH; Forge does not bundle the Oracle client".into(),
            )
        })?;
        Self::with_tool(spec, tool, kind)
    }

    /// Use a specific client (tests use a fake one).
    pub fn with_tool(spec: &ConnectSpec<'_>, tool: PathBuf, kind: Tool) -> DbResult<Self> {
        let service = match spec.ds.database.trim() {
            "" => "XEPDB1",
            s => s,
        };
        Ok(Self {
            settings: Arc::new(Settings {
                tool,
                kind,
                user: spec.ds.user.trim().into(),
                password: spec.password.unwrap_or_default().into(),
                target: format!("//{}:{}/{service}", spec.host(), spec.port()),
                read_only: spec.ds.options.read_only,
                init: spec.ds.options.init_sql.clone(),
            }),
        })
    }

    pub fn tool(&self) -> &Path {
        &self.settings.tool
    }
}

impl EnginePool for OraclePool {
    fn dialect(&self) -> Dialect {
        Dialect::Oracle
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn acquire(&self) -> BoxFuture<'_, DbResult<Box<dyn EngineConn>>> {
        Box::pin(async move {
            Ok(Box::new(OracleConn {
                settings: self.settings.clone(),
                child: Arc::default(),
            }) as Box<dyn EngineConn>)
        })
    }
}

pub struct OracleConn {
    settings: Arc<Settings>,
    /// Process id of the running statement (for Cancel).
    child: Arc<Mutex<Option<u32>>>,
}

impl OracleConn {
    async fn query(&mut self, sql: &str, skip: usize, max: usize) -> DbResult<RawResult> {
        let s = &self.settings;
        let text = script(
            s.kind,
            &s.user,
            &s.password,
            &s.target,
            s.read_only,
            &s.init,
            sql,
        );
        let mut std_cmd = std::process::Command::new(&s.tool);
        match s.kind {
            Tool::SqlPlus => std_cmd.args(["-S", "-L", "/nolog"]),
            Tool::SqlCl => std_cmd.args(["-S", "/nolog"]),
        };
        // In Flatpak the Oracle client runs on the host.
        let mut cmd: tokio::process::Command = forge_hostspawn::host_command(std_cmd).into();
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd
            .spawn()
            .map_err(|e| DbError::Io(format!("cannot start {}: {e}", s.tool.display())))?;
        *self.child.lock().expect("poisoned") = child.id();
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .await
                .map_err(|e| DbError::Io(e.to_string()))?;
        }
        let output = child
            .wait_with_output()
            .await
            .map_err(|e| DbError::Io(e.to_string()));
        *self.child.lock().expect("poisoned") = None;
        let output = output?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut raw = parse_output(&stdout).map_err(|e| {
            let err = String::from_utf8_lossy(&output.stderr);
            if err.trim().is_empty() {
                e
            } else {
                DbError::Sql(format!("{e}\n{}", err.trim()))
            }
        })?;
        let (rows, more) = page_rows(std::mem::take(&mut raw.rows), skip, max);
        raw.rows = rows;
        raw.has_more = more;
        Ok(raw)
    }
}

impl EngineConn for OracleConn {
    fn dialect(&self) -> Dialect {
        Dialect::Oracle
    }

    fn run<'a>(
        &'a mut self,
        sql: &'a str,
        skip: usize,
        max: usize,
    ) -> BoxFuture<'a, DbResult<RawResult>> {
        Box::pin(self.query(sql, skip, max))
    }

    fn server_version(&mut self) -> BoxFuture<'_, DbResult<String>> {
        Box::pin(async move {
            let r = self
                .query("SELECT banner FROM v$version WHERE ROWNUM = 1", 0, 1)
                .await?;
            Ok(r.rows
                .first()
                .and_then(|r| r.first())
                .and_then(Value::as_str)
                .unwrap_or("Oracle")
                .to_string())
        })
    }

    fn canceller(&self) -> Option<Canceller> {
        let child = self.child.clone();
        Some(Arc::new(move || {
            let pid = *child.lock().expect("poisoned");
            Box::pin(async move {
                // The statement future is dropped too, which kills the process; this makes
                // sure it stops right away.
                if let Some(pid) = pid {
                    #[cfg(unix)]
                    let _ = tokio::process::Command::new("kill")
                        .arg(pid.to_string())
                        .status()
                        .await;
                    #[cfg(windows)]
                    {
                        let mut taskkill = std::process::Command::new("taskkill");
                        taskkill.args(["/PID", &pid.to_string(), "/F"]);
                        let _ =
                            tokio::process::Command::from(forge_hostspawn::local_command(taskkill))
                                .status()
                                .await;
                    }
                }
            })
        }))
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builds_the_script_with_the_password_on_stdin() {
        let s = script(
            Tool::SqlPlus,
            "hr",
            "p\"w",
            "//db:1521/XEPDB1",
            true,
            "",
            "select 1 from dual;",
        );
        assert!(s.contains("SET MARKUP CSV ON QUOTE ON"));
        assert!(s.contains("CONNECT \"hr\"/\"pw\"@\"//db:1521/XEPDB1\""));
        assert!(s.contains("SET TRANSACTION READ ONLY;"));
        assert!(
            s.contains("PROMPT FORGE-RESULT-BEGIN\nselect 1 from dual;\nPROMPT FORGE-RESULT-END")
        );
        let s = script(
            Tool::SqlCl,
            "hr",
            "pw",
            "//db:1521/x",
            false,
            "",
            "BEGIN\n  null;\nEND;",
        );
        assert!(s.contains("SET SQLFORMAT CSV"));
        assert!(s.contains("BEGIN\n  null;\nEND;\n/\n"));
    }

    #[test]
    fn parses_csv_results_and_feedback() {
        let out = "\nConnected.\nFORGE-RESULT-BEGIN\n\n\"ID\",\"NAME\",\"SALARY\",\"BONUS\"\n100,\"Steven, King\",24000,\n101,\"Neena \"\"N\"\"\",17000.5,\n\n2 rows selected.\n\nFORGE-RESULT-END\n";
        let r = parse_output(out).unwrap();
        let names: Vec<_> = r.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["ID", "NAME", "SALARY", "BONUS"]);
        assert_eq!(
            r.rows[0],
            vec![json!(100), json!("Steven, King"), json!(24000), Value::Null]
        );
        assert_eq!(r.rows[1][1], json!("Neena \"N\""));
        assert_eq!(r.rows[1][2], json!(17000.5));
        let r =
            parse_output("FORGE-RESULT-BEGIN\n\n3 rows updated.\n\nFORGE-RESULT-END\n").unwrap();
        assert_eq!(r.rows_affected, 3);
        assert!(r.columns.is_empty());
        let r = parse_output("FORGE-RESULT-BEGIN\n\nTable created.\n\nFORGE-RESULT-END\n").unwrap();
        assert_eq!(r.rows, vec![vec![json!("Table created.")]]);
        let err = parse_output("FORGE-RESULT-BEGIN\nselect * from nope\n              *\nERROR at line 1:\nORA-00942: table or view does not exist\n").unwrap_err();
        assert!(err.to_string().contains("ORA-00942"));
        let err = parse_output("ERROR:\nORA-01017: invalid username/password; logon denied\n")
            .unwrap_err();
        assert!(err.to_string().contains("ORA-01017"));
    }

    #[test]
    fn finds_the_client_on_path() {
        let dir = tempfile::tempdir().unwrap();
        assert!(find_tool(Some(dir.path().as_os_str())).is_none());
        std::fs::write(dir.path().join("sql"), "").unwrap();
        assert_eq!(
            find_tool(Some(dir.path().as_os_str())).unwrap().1,
            Tool::SqlCl
        );
        std::fs::write(dir.path().join("sqlplus"), "").unwrap();
        assert_eq!(
            find_tool(Some(dir.path().as_os_str())).unwrap().1,
            Tool::SqlPlus
        );
    }
}
