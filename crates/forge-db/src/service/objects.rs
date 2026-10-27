//! Database objects: scripts of the Create/Modify Table, Drop and Rename dialogs run with
//! the read-only and production checks, data imports (batched inserts with progress, skip
//! or abort on errors, cancel), and where the native dump / restore tools connect.

use std::time::Instant;

use forge_dbtransfer::dump::DumpTarget;
use forge_dbtransfer::insert::{batches, row_insert, ImportPlan, OnError};
use forge_dbtransfer::SourceTable;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::{policy, DbService};
use crate::config::{DataSource, Driver};
use crate::error::{DbError, DbResult};
use crate::guard::{self, write_kind, Guard};
use crate::secrets::ssh_account;
use crate::split::split_statements;
use crate::tunnel::TunnelEndpoint;

/// What a script of the object dialogs did.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScriptResult {
    /// Statements that ran.
    pub executed: usize,
    pub total: usize,
    /// The statement that failed and why (the ones before it stay applied unless the
    /// database runs DDL in transactions).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub elapsed_ms: u64,
}

/// Progress of an import, sent after every batch.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub import_id: String,
    pub done: usize,
    pub total: usize,
    pub inserted: u64,
    pub skipped: usize,
}

/// A row an import skipped or stopped at (0-based source row).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportError {
    pub row: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub inserted: u64,
    pub skipped: usize,
    /// The first errors (at most 100).
    pub errors: Vec<ImportError>,
    pub cancelled: bool,
    /// Aborted at an error; with transactions nothing was kept.
    pub aborted: bool,
    pub created: bool,
    pub elapsed_ms: u64,
}

const MAX_ERRORS: usize = 100;

fn ms(t: Instant) -> u64 {
    t.elapsed().as_millis() as u64
}

impl DbService {
    /// The check before a script of the object dialogs runs: refused on read-only data
    /// sources; on production every change asks first (not only unfiltered ones).
    pub fn objects_guard(&self, id: &str, sql: &str) -> DbResult<Guard> {
        let ds = self.get(id)?;
        let d = ds.dialect();
        let mut verdict = guard::guard(sql, d, &policy(&ds));
        if verdict.blocked.is_none() && verdict.confirm.is_none() && ds.is_production() {
            let writes: Vec<String> = split_statements(sql, d)
                .iter()
                .filter_map(|s| write_kind(&s.text, d))
                .collect();
            if !writes.is_empty() {
                let mut kinds = writes.clone();
                kinds.sort();
                kinds.dedup();
                verdict.confirm = Some(format!(
                    "{} is a production data source. The script runs {} statement(s) that change it ({}). Run it anyway?",
                    ds.name,
                    writes.len(),
                    kinds.join(", ")
                ));
            }
        }
        Ok(verdict)
    }

    fn check(&self, id: &str, sql: &str, confirmed: bool) -> DbResult<DataSource> {
        let ds = self.get(id)?;
        if ds.driver == Driver::Ddl {
            return Err(DbError::Refused(format!(
                "{} is a DDL data source; change its SQL files instead",
                ds.name
            )));
        }
        let verdict = self.objects_guard(id, sql)?;
        if let Some(b) = verdict.blocked {
            return Err(DbError::Refused(b));
        }
        if let (Some(c), false) = (verdict.confirm, confirmed) {
            return Err(DbError::NeedsConfirmation(c));
        }
        Ok(ds)
    }

    /// Run a script of the object dialogs (CREATE / ALTER / DROP / RENAME) on its own
    /// connection, statement by statement; stops at the first error.
    pub async fn run_script(&self, id: &str, sql: &str, confirmed: bool) -> DbResult<ScriptResult> {
        let ds = self.check(id, sql, confirmed)?;
        let started = Instant::now();
        let statements = split_statements(sql, ds.dialect());
        let mut conn = self.pool(id).await?.acquire().await?;
        let mut out = ScriptResult {
            total: statements.len(),
            ..Default::default()
        };
        for st in &statements {
            match conn.execute(&st.text).await {
                Ok(_) => out.executed += 1,
                Err(e) => {
                    out.failed = Some(st.text.clone());
                    out.error = Some(e.to_string());
                    break;
                }
            }
        }
        out.elapsed_ms = ms(started);
        Ok(out)
    }

    /// Insert the rows of `source` as `plan` maps them (creating the table first when the
    /// plan has one). Skip mode retries a failing batch row by row and skips the rows that
    /// fail; abort mode stops at the first error and, where the driver has transactions,
    /// keeps nothing. `progress` is called after every batch; `cancel` stops between
    /// batches.
    #[allow(clippy::too_many_arguments)]
    pub async fn import(
        &self,
        id: &str,
        import_id: &str,
        source: &SourceTable,
        plan: &ImportPlan,
        confirmed: bool,
        cancel: CancellationToken,
        progress: impl Fn(ImportProgress),
    ) -> DbResult<ImportSummary> {
        let ds = self.get(id)?;
        let d = ds.dialect();
        if !d.is_sql() {
            return Err(DbError::Refused(format!(
                "{} data sources cannot import rows",
                ds.driver.label()
            )));
        }
        if plan.columns.is_empty() {
            return Err(DbError::Refused("Map at least one column".into()));
        }
        let create = plan
            .create
            .as_ref()
            .map(|t| forge_dbmodel::design::create_statements(d, t, Default::default(), &[]))
            .unwrap_or_default();
        let target = d.qualified(&plan.schema, &plan.table);
        let probe = format!("INSERT INTO {target} VALUES (1)");
        let mut check_sql = create.join("\n");
        check_sql.push('\n');
        check_sql.push_str(&probe);
        let ds = self.check(id, &check_sql, confirmed)?;
        let started = Instant::now();
        let mut conn = self.pool(id).await?.acquire().await?;
        let mut summary = ImportSummary::default();
        for st in &create {
            for s in split_statements(st, d) {
                conn.execute(&s.text).await?;
            }
            summary.created = true;
        }
        let rows = &source.rows;
        let total = if plan.limit > 0 {
            plan.limit.min(rows.len())
        } else {
            rows.len()
        };
        let all = batches(d, plan, rows);
        let tx = plan.on_error == OnError::Abort && ds.driver.supports_transactions();
        if tx {
            conn.execute(d.begin()).await?;
        }
        let mut done = 0;
        let fail = |summary: &mut ImportSummary, row: usize, e: &DbError| {
            if summary.errors.len() < MAX_ERRORS {
                summary.errors.push(ImportError {
                    row,
                    message: e.to_string(),
                });
            }
        };
        for b in &all {
            if cancel.is_cancelled() {
                summary.cancelled = true;
                break;
            }
            match conn.execute(&b.sql).await {
                Ok(n) => summary.inserted += if n == 0 { b.rows as u64 } else { n },
                Err(e) if plan.on_error == OnError::Abort => {
                    fail(&mut summary, b.first, &e);
                    summary.aborted = true;
                    break;
                }
                Err(_) => {
                    // Find the rows that fail.
                    for (i, row) in rows.iter().enumerate().skip(b.first).take(b.rows) {
                        match conn.execute(&row_insert(d, plan, row)).await {
                            Ok(_) => summary.inserted += 1,
                            Err(e) => {
                                summary.skipped += 1;
                                fail(&mut summary, i, &e);
                            }
                        }
                    }
                }
            }
            done += b.rows;
            progress(ImportProgress {
                import_id: import_id.into(),
                done,
                total,
                inserted: summary.inserted,
                skipped: summary.skipped,
            });
        }
        if tx {
            if summary.aborted || summary.cancelled {
                let _ = conn.execute("ROLLBACK").await;
                summary.inserted = 0;
            } else {
                conn.execute("COMMIT").await?;
            }
        }
        summary.elapsed_ms = ms(started);
        Ok(summary)
    }

    /// Before a restore: refused on read-only data sources; on production the returned
    /// text must be confirmed.
    pub fn restore_guard(&self, id: &str) -> DbResult<Option<String>> {
        let ds = self.get(id)?;
        if ds.options.read_only {
            return Err(DbError::Refused(format!(
                "{} is read-only: restoring a dump is not allowed. Turn off Read-only in the data source properties first.",
                ds.name
            )));
        }
        Ok(ds.is_production().then(|| {
            format!(
                "{} is a production data source. Restoring a dump overwrites its objects and data. Restore anyway?",
                ds.name
            )
        }))
    }

    /// The settings a native tool connects with (without opening the SSH tunnel; the
    /// dialog's command line preview).
    pub fn dump_target_settings(&self, id: &str) -> DbResult<DumpTarget> {
        let ds = self.get(id)?;
        let driver = match ds.driver {
            Driver::Mysql => "mysql",
            Driver::Mariadb => "mariadb",
            Driver::Postgres => "postgres",
            Driver::Sqlite => "sqlite",
            Driver::Sqlserver => "sqlserver",
            _ => {
                return Err(DbError::Refused(format!(
                    "{} has no dump tools in Forge",
                    ds.driver.label()
                )))
            }
        };
        let sqlite = ds.driver == Driver::Sqlite;
        Ok(DumpTarget {
            driver: driver.into(),
            host: ds.host.clone(),
            port: if sqlite { 0 } else { ds.port_or_default() },
            user: ds.user.clone(),
            database: ds.database.clone(),
            file: if sqlite {
                ds.sqlite_path(self.project_root().as_deref())
                    .to_string_lossy()
                    .into_owned()
            } else {
                String::new()
            },
            ssl: ds.options.ssl_mode.enabled() && !sqlite,
            password: None,
        })
    }

    /// Where a native tool connects to data source `id`: its settings, through its own
    /// SSH tunnel when it has one (keep the endpoint alive while the tool runs), with the
    /// stored password for the tool's environment.
    pub async fn dump_target(&self, id: &str) -> DbResult<(DumpTarget, Option<TunnelEndpoint>)> {
        let ds = self.get(id)?;
        let mut target = self.dump_target_settings(id)?;
        target.password = self.password(&ds);
        if ds.driver == Driver::Sqlite {
            return Ok((target, None));
        }
        if ds.options.ssh.enabled {
            let secret = self.secret(&ssh_account(&ds.id));
            let (local, end) = self.endpoint(&ds, secret, None).await?;
            target.port = local.port_or_default();
            target.host = local.host;
            return Ok((target, end));
        }
        Ok((target, None))
    }
}
