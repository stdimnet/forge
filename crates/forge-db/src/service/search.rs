//! Full-text search across data (DataGrip's "Full-Text Search"): for every selected table a
//! read-only `SELECT` finds the rows containing the text in any searched column. Results
//! and progress stream to the caller table by table; a search can be cancelled.

use std::time::Instant;

use forge_dbmodel::search::{self as fts, CellMatcher, SearchOptions};
use forge_dbmodel::{SchemaFilter, TableKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::DbService;
use crate::config::Driver;
use crate::error::{DbError, DbResult};

/// What to search in one data source.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchScope {
    pub data_source: String,
    /// Schemas to search; empty means the visible ones.
    #[serde(default)]
    pub schemas: Vec<String>,
    /// Table names or patterns (`*`, `order_*`, `/regex/`, `!audit_*`); empty means all.
    #[serde(default)]
    pub tables: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub scopes: Vec<SearchScope>,
    #[serde(flatten)]
    pub options: SearchOptions,
}

/// The rows of one table that contain the text.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableMatches {
    pub data_source: String,
    pub schema: String,
    pub table: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    /// Indexes of the matching cells, per row.
    pub matches: Vec<Vec<usize>>,
    /// More rows matched than the limit.
    pub truncated: bool,
    pub sql: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Progress of a running search: `result` is set for tables with matches or errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchProgress {
    pub search_id: String,
    pub done: usize,
    pub total: usize,
    /// `source › schema.table` being searched.
    pub current: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<TableMatches>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSummary {
    pub tables: usize,
    pub matched_tables: usize,
    pub rows: usize,
    pub errors: usize,
    pub cancelled: bool,
    pub elapsed_ms: u64,
}

struct Target {
    data_source: String,
    schema: String,
    table: forge_dbmodel::TableInfo,
    /// A scope that cannot be searched (no data, not SQL).
    error: Option<String>,
}

impl DbService {
    /// Stop a running search.
    pub fn cancel_search(&self, search_id: &str) {
        if let Some(t) = self.searches.lock().expect("poisoned").get(search_id) {
            t.cancel();
        }
    }

    async fn search_targets(&self, req: &SearchRequest) -> DbResult<Vec<Target>> {
        let mut out = Vec::new();
        for scope in &req.scopes {
            let ds = self.get(&scope.data_source)?;
            let refuse = if ds.driver == Driver::Ddl {
                Some("DDL data sources have no data".to_string())
            } else if !ds.dialect().is_sql() {
                Some(format!("{} cannot be searched; use its console", ds.driver.label()))
            } else {
                None
            };
            if let Some(error) = refuse {
                out.push(Target {
                    data_source: ds.id.clone(),
                    schema: String::new(),
                    table: Default::default(),
                    error: Some(error),
                });
                continue;
            }
            let model = self.model(&ds.id, false).await?;
            let schemas: Vec<String> = if scope.schemas.is_empty() {
                model
                    .schemas
                    .iter()
                    .filter(|s| s.visible)
                    .map(|s| s.name.clone())
                    .collect()
            } else {
                scope.schemas.clone()
            };
            let tables = if scope.tables.is_empty() {
                None
            } else {
                Some(SchemaFilter::parse(&scope.tables).map_err(DbError::Config)?)
            };
            for schema in schemas {
                let info = match model.loaded.get(&schema) {
                    Some(s) => s.clone(),
                    None => self.introspect(&ds.id, &schema).await?,
                };
                for t in info.tables.into_iter().filter(|t| {
                    matches!(
                        t.kind,
                        TableKind::Table | TableKind::View | TableKind::MaterializedView
                    ) && tables.as_ref().is_none_or(|f| f.matches(&t.name, false))
                }) {
                    out.push(Target {
                        data_source: ds.id.clone(),
                        schema: schema.clone(),
                        table: t,
                        error: None,
                    });
                }
            }
        }
        Ok(out)
    }

    /// Search the scopes of `req`; `progress` gets every searched table.
    pub async fn full_text_search(
        &self,
        search_id: &str,
        req: SearchRequest,
        progress: impl Fn(SearchProgress) + Send + Sync,
    ) -> DbResult<SearchSummary> {
        req.options.validate().map_err(DbError::Config)?;
        let matcher = CellMatcher::new(&req.options).map_err(DbError::Config)?;
        let token = CancellationToken::new();
        self.searches
            .lock()
            .expect("poisoned")
            .insert(search_id.into(), token.clone());
        let started = Instant::now();
        let result = self
            .run_search(search_id, &req, &matcher, &token, &progress)
            .await;
        let result = match result {
            Ok(mut s) => {
                s.cancelled = token.is_cancelled();
                s.elapsed_ms = started.elapsed().as_millis() as u64;
                Ok(s)
            }
            Err(e) => Err(e),
        };
        self.searches.lock().expect("poisoned").remove(search_id);
        result
    }

    async fn run_search(
        &self,
        search_id: &str,
        req: &SearchRequest,
        matcher: &CellMatcher,
        token: &CancellationToken,
        progress: &(impl Fn(SearchProgress) + Send + Sync),
    ) -> DbResult<SearchSummary> {
        let opts = &req.options;
        let targets = self.search_targets(req).await?;
        let total = targets.len();
        let mut summary = SearchSummary {
            tables: total,
            ..Default::default()
        };
        let limit = opts.limit();
        for (i, t) in targets.into_iter().enumerate() {
            if token.is_cancelled() {
                break;
            }
            let current = if t.schema.is_empty() {
                t.data_source.clone()
            } else {
                format!("{} › {}.{}", t.data_source, t.schema, t.table.name)
            };
            let mut result = TableMatches {
                data_source: t.data_source.clone(),
                schema: t.schema.clone(),
                table: t.table.name.clone(),
                ..Default::default()
            };
            if let Some(e) = t.error {
                result.error = Some(e);
            } else {
                let ds = self.get(&t.data_source)?;
                match fts::table_query(ds.dialect(), &t.schema, &t.table, opts) {
                    Ok(None) => {}
                    Ok(Some(sql)) => {
                        result.sql = sql.clone();
                        let run = async {
                            let mut conn = self.pool(&t.data_source).await?.acquire().await?;
                            conn.run(&sql, 0, limit + 1).await
                        };
                        let outcome = tokio::select! {
                            r = run => Some(r),
                            _ = token.cancelled() => None,
                        };
                        match outcome {
                            None => break,
                            Some(Err(e)) => result.error = Some(e.to_string()),
                            Some(Ok(raw)) => {
                                result.columns = raw.columns.into_iter().map(|c| c.name).collect();
                                result.truncated = raw.has_more || raw.rows.len() > limit;
                                result.rows = raw.rows.into_iter().take(limit).collect();
                                result.matches = result
                                    .rows
                                    .iter()
                                    .map(|r| matcher.matching_cells(r))
                                    .collect();
                            }
                        }
                    }
                    Err(e) => result.error = Some(e),
                }
            }
            if result.error.is_some() {
                summary.errors += 1;
            }
            if !result.rows.is_empty() {
                summary.matched_tables += 1;
                summary.rows += result.rows.len();
            }
            let show = result.error.is_some() || !result.rows.is_empty();
            progress(SearchProgress {
                search_id: search_id.into(),
                done: i + 1,
                total,
                current,
                result: show.then_some(result),
            });
        }
        Ok(summary)
    }
}
