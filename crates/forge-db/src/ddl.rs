//! Table DDL: read from the server where it keeps it (MySQL `SHOW CREATE`, SQLite
//! `sqlite_master`), otherwise generated from introspection (PostgreSQL).

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
        Dialect::Postgres => {
            let info = introspect::schema(conn, schema, Some(table)).await?;
            let t = info
                .tables
                .into_iter()
                .next()
                .ok_or_else(|| DbError::Sql(format!("table {table} not found")))?;
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
    let mut lines: Vec<String> = t
        .columns
        .iter()
        .map(|c| {
            let mut l = format!("    {} {}", d.quote_ident(&c.name), c.data_type);
            if !c.nullable {
                l.push_str(" NOT NULL");
            }
            if let Some(def) = &c.default {
                l.push_str(&format!(" DEFAULT {def}"));
            }
            l
        })
        .collect();
    let cols = |cs: &[String]| {
        cs.iter()
            .map(|c| d.quote_ident(c))
            .collect::<Vec<_>>()
            .join(", ")
    };
    if !t.primary_key.is_empty() {
        let name = t.indexes.iter().find(|i| i.primary).map(|i| i.name.clone());
        lines.push(match name {
            Some(n) => format!(
                "    CONSTRAINT {} PRIMARY KEY ({})",
                d.quote_ident(&n),
                cols(&t.primary_key)
            ),
            None => format!("    PRIMARY KEY ({})", cols(&t.primary_key)),
        });
    }
    for i in t.indexes.iter().filter(|i| i.unique && !i.primary) {
        lines.push(format!(
            "    CONSTRAINT {} UNIQUE ({})",
            d.quote_ident(&i.name),
            cols(&i.columns)
        ));
    }
    for f in &t.foreign_keys {
        let mut l = format!(
            "    CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({})",
            d.quote_ident(&f.name),
            cols(&f.columns),
            d.qualified(&f.ref_schema, &f.ref_table),
            cols(&f.ref_columns)
        );
        if !f.on_update.is_empty() && f.on_update != "NO ACTION" {
            l.push_str(&format!(" ON UPDATE {}", f.on_update));
        }
        if !f.on_delete.is_empty() && f.on_delete != "NO ACTION" {
            l.push_str(&format!(" ON DELETE {}", f.on_delete));
        }
        lines.push(l);
    }
    let mut out = format!(
        "CREATE TABLE {} (\n{}\n);\n",
        d.qualified(schema, &t.name),
        lines.join(",\n")
    );
    for i in t.indexes.iter().filter(|i| !i.unique && !i.primary) {
        out.push_str(&format!(
            "\nCREATE INDEX {} ON {} ({});\n",
            d.quote_ident(&i.name),
            d.qualified(schema, &t.name),
            cols(&i.columns)
        ));
    }
    if !t.comment.is_empty() && d == Dialect::Postgres {
        out.push_str(&format!(
            "\nCOMMENT ON TABLE {} IS {};\n",
            d.qualified(schema, &t.name),
            d.quote_str(&t.comment)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::introspect::{ColumnInfo, ForeignKey, IndexInfo};

    #[test]
    fn generates_postgres_table() {
        let t = TableInfo {
            name: "orders".into(),
            columns: vec![
                ColumnInfo {
                    name: "id".into(),
                    data_type: "bigint".into(),
                    nullable: false,
                    default: Some("nextval('orders_id_seq'::regclass)".into()),
                    auto_increment: true,
                    ..Default::default()
                },
                ColumnInfo {
                    name: "user_id".into(),
                    data_type: "bigint".into(),
                    nullable: true,
                    ..Default::default()
                },
            ],
            primary_key: vec!["id".into()],
            indexes: vec![
                IndexInfo {
                    name: "orders_pkey".into(),
                    columns: vec!["id".into()],
                    unique: true,
                    primary: true,
                },
                IndexInfo {
                    name: "orders_user_id_index".into(),
                    columns: vec!["user_id".into()],
                    unique: false,
                    primary: false,
                },
            ],
            foreign_keys: vec![ForeignKey {
                name: "orders_user_id_foreign".into(),
                columns: vec!["user_id".into()],
                ref_schema: "public".into(),
                ref_table: "users".into(),
                ref_columns: vec!["id".into()],
                on_update: "NO ACTION".into(),
                on_delete: "CASCADE".into(),
            }],
            ..Default::default()
        };
        let ddl = generate(Dialect::Postgres, "public", &t);
        assert_eq!(
            ddl,
            "CREATE TABLE \"public\".\"orders\" (\n    \"id\" bigint NOT NULL DEFAULT nextval('orders_id_seq'::regclass),\n    \"user_id\" bigint,\n    CONSTRAINT \"orders_pkey\" PRIMARY KEY (\"id\"),\n    CONSTRAINT \"orders_user_id_foreign\" FOREIGN KEY (\"user_id\") REFERENCES \"public\".\"users\" (\"id\") ON DELETE CASCADE\n);\n\nCREATE INDEX \"orders_user_id_index\" ON \"public\".\"orders\" (\"user_id\");\n"
        );
    }
}
