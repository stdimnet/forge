//! Explain Plan in a console's session: the dialect's `EXPLAIN` statements
//! ([`forge_dbschema::plan`]) run on the console's connection, and their rows parsed into a
//! plan tree. Explaining never executes the statement; a statement that is already an
//! `EXPLAIN` (which could be `EXPLAIN ANALYZE`) goes through the read-only / production guard.

use std::time::Instant;

use forge_dbschema::plan::{explain_script, parse_plan, Plan};
use serde::Serialize;

use super::{elapsed_ms, policy, DbService};
use crate::conn::ColumnMeta;
use crate::error::{DbError, DbResult};
use crate::guard;
use crate::split::leading_keyword;

/// Rows of an explain statement are capped at this many.
const MAX_PLAN_ROWS: usize = 10_000;

/// The result of Explain Plan: the raw rows and, unless they could not be parsed, the plan.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainResult {
    /// The statement whose rows are shown.
    pub sql: String,
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<Plan>,
    /// Why there is no plan (the rows did not parse).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub elapsed_ms: u64,
}

impl DbService {
    /// Explain `sql` (one statement) in a console session; `raw` for "Explain Plan (Raw)".
    pub async fn explain(
        &self,
        session_id: &str,
        sql: &str,
        raw: bool,
        confirmed: bool,
    ) -> DbResult<ExplainResult> {
        let s = self.session(session_id)?;
        let ds = self.get(&s.ds_id)?;
        let script = explain_script(s.dialect, sql, raw).map_err(DbError::Refused)?;
        if leading_keyword(sql.trim()) == "EXPLAIN" {
            let verdict = guard::guard(sql, s.dialect, &policy(&ds));
            if let Some(blocked) = verdict.blocked {
                return Err(DbError::Refused(blocked));
            }
            if let (Some(confirm), false) = (verdict.confirm, confirmed) {
                return Err(DbError::NeedsConfirmation(confirm));
            }
        }
        let started = Instant::now();
        let mut conn = s.conn.lock().await;
        let mut outcome = Ok(None);
        let mut shown = script.statements[script.result].clone();
        for (i, st) in script.statements.iter().enumerate() {
            match conn.run(st, 0, MAX_PLAN_ROWS).await {
                Ok(r) if i == script.result => {
                    outcome = Ok(Some(r));
                }
                Ok(_) => {}
                Err(e) => {
                    let fallback = (i == script.result)
                        .then_some(script.fallback.as_ref())
                        .flatten();
                    match fallback {
                        Some(f) => {
                            shown = f.clone();
                            outcome = conn.run(f, 0, MAX_PLAN_ROWS).await.map(Some);
                        }
                        None => outcome = Err(e),
                    }
                    if outcome.is_err() {
                        break;
                    }
                }
            }
        }
        for st in &script.cleanup {
            if let Err(e) = conn.run(st, 0, 0).await {
                tracing::warn!("explain cleanup failed: {e}");
            }
        }
        drop(conn);
        let raw_rows = outcome?.unwrap_or_default();
        let names: Vec<String> = raw_rows.columns.iter().map(|c| c.name.clone()).collect();
        let (plan, error) = if raw {
            (None, None)
        } else {
            match parse_plan(&names, &raw_rows.rows) {
                Ok(p) => (Some(p), None),
                Err(e) => (None, Some(e)),
            }
        };
        Ok(ExplainResult {
            sql: shown,
            columns: raw_rows.columns,
            rows: raw_rows.rows,
            plan,
            error,
            elapsed_ms: elapsed_ms(started),
        })
    }
}
