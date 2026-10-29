//! Table DDL: read from the server where it keeps it (MySQL and ClickHouse `SHOW CREATE`,
//! SQLite `sqlite_master`, Oracle `DBMS_METADATA`), otherwise generated from introspection
//! (PostgreSQL, SQL Server tables).

use crate::config::Dialect;
use crate::conn::Conn;
use crate::error::{DbError, DbResult};
use crate::introspect::{self, TableInfo, TableKind};

/// `CREATE TABLE`/`CREATE VIEW` statement(s) for `schema.table`.
pub async fn table_ddl(conn: &mut Conn, schema: &str, table: &str) -> DbResult<String> {
    let d = conn.dialect();
    match d {
        Dialect::MySql => {
            let rows = conn
                .text_rows(&format!("SHOW CREATE TABLE {}", d.qualified(schema, table)))
                .await?;
            let ddl = rows
                .first()
                .and_then(|r| r.get(1).cloned().flatten())
                .ok_or_else(|| DbError::Sql(format!("no DDL for {table}")))?;
            Ok(format!("{ddl};\n"))
        }
        Dialect::Sqlite => {
            let rows = conn
                .text_rows(&format!(
                    "SELECT sql FROM {}.sqlite_master WHERE tbl_name = {} AND sql IS NOT NULL ORDER BY type = 'index', name",
                    d.quote_ident(schema),
                    d.quote_str(table)
                ))
                .await?;
            if rows.is_empty() {
                return Err(DbError::Sql(format!("no DDL for {table}")));
            }
            Ok(rows
                .into_iter()
                .filter_map(|r| r.into_iter().next().flatten())
                .map(|s| format!("{s};\n"))
                .collect::<Vec<_>>()
                .join("\n"))
        }
        Dialect::ClickHouse => {
            let rows = conn
                .text_rows(&format!("SHOW CREATE TABLE {}", d.qualified(schema, table)))
                .await?;
            let ddl = rows
                .first()
                .and_then(|r| r.first().cloned().flatten())
                .ok_or_else(|| DbError::Sql(format!("no DDL for {table}")))?;
            Ok(format!("{};\n", ddl.trim_end()))
        }
        Dialect::Oracle => {
            let rows = conn
                .text_rows(&format!(
                    "SELECT DBMS_METADATA.GET_DDL(CASE WHEN EXISTS (SELECT 1 FROM all_views WHERE owner = {s} AND view_name = {t}) THEN 'VIEW' ELSE 'TABLE' END, {t}, {s}) FROM dual",
                    s = d.quote_str(schema),
                    t = d.quote_str(table)
                ))
                .await?;
            let ddl = rows
                .first()
                .and_then(|r| r.first().cloned().flatten())
                .ok_or_else(|| DbError::Sql(format!("no DDL for {table}")))?;
            Ok(format!("{};\n", ddl.trim()))
        }
        Dialect::Redis | Dialect::Mongo => {
            Err(DbError::Refused("this data source has no DDL".into()))
        }
        Dialect::SqlServer | Dialect::Postgres => {
            let info = introspect::schema(conn, schema, Some(table)).await?;
            let t = info
                .tables
                .into_iter()
                .next()
                .ok_or_else(|| DbError::Sql(format!("table {table} not found")))?;
            if t.kind == TableKind::View && d == Dialect::SqlServer {
                let rows = conn
                    .text_rows(&format!(
                        "SELECT OBJECT_DEFINITION(OBJECT_ID({}))",
                        d.quote_str(&d.qualified(schema, table))
                    ))
                    .await?;
                let body = rows
                    .first()
                    .and_then(|r| r.first().cloned().flatten())
                    .unwrap_or_default();
                return Ok(format!("{}\n", body.trim_end()));
            }
            if t.kind == TableKind::View {
                let rows = conn
                    .text_rows(&format!(
                        "SELECT pg_get_viewdef({}::regclass, true)",
                        d.quote_str(&d.qualified(schema, table))
                    ))
                    .await?;
                let body = rows
                    .first()
                    .and_then(|r| r.first().cloned().flatten())
                    .unwrap_or_default();
                return Ok(format!(
                    "CREATE VIEW {} AS\n{}\n",
                    d.qualified(schema, table),
                    body.trim_end()
                ));
            }
            Ok(generate(d, schema, &t))
        }
    }
}

/// Generate `CREATE TABLE` (plus `CREATE INDEX` for non-unique indexes) from metadata.
pub fn generate(d: Dialect, schema: &str, t: &TableInfo) -> String {
    forge_dbmodel::generate::table(d, schema, t)
}
