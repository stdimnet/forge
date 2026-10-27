//! Database metadata: schemas, tables, views, columns, keys, indexes and routines.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::config::Dialect;
use crate::conn::Conn;
use crate::error::DbResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TableKind {
    #[default]
    Table,
    View,
    /// A MongoDB collection.
    Collection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    pub auto_increment: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub comment: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKey {
    pub name: String,
    pub columns: Vec<String>,
    pub ref_schema: String,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub on_update: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub on_delete: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub name: String,
    pub kind: TableKind,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub comment: String,
    pub columns: Vec<ColumnInfo>,
    pub primary_key: Vec<String>,
    pub foreign_keys: Vec<ForeignKey>,
    pub indexes: Vec<IndexInfo>,
    /// Estimated row count when the database keeps one cheaply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_estimate: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RoutineInfo {
    pub name: String,
    /// `function` or `procedure`.
    pub kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub returns: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SchemaInfo {
    pub name: String,
    pub tables: Vec<TableInfo>,
    pub routines: Vec<RoutineInfo>,
}

/// One schema in the tree with whether it is the connection's current one.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaName {
    pub name: String,
    pub current: bool,
}

fn s(v: &Option<String>) -> String {
    v.clone().unwrap_or_default()
}

fn flag(v: &Option<String>) -> bool {
    matches!(v.as_deref(), Some("1" | "t" | "true" | "YES" | "yes"))
}

/// Schemas (MySQL databases) visible on the connection.
pub async fn schemas(conn: &mut Conn) -> DbResult<Vec<SchemaName>> {
    if let Conn::Engine(e) = conn {
        if !e.dialect().is_sql() {
            return e.schemas().await;
        }
    }
    let (list, current) = match conn.dialect() {
        Dialect::MySql => (
            "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME",
            "SELECT DATABASE()",
        ),
        Dialect::Postgres => (
            "SELECT nspname FROM pg_namespace WHERE nspname NOT LIKE 'pg\\_%' AND nspname <> 'information_schema' ORDER BY nspname",
            "SELECT current_schema()",
        ),
        Dialect::Sqlite => ("SELECT name FROM pragma_database_list WHERE name <> 'temp' ORDER BY seq", "SELECT 'main'"),
        Dialect::SqlServer => (
            "SELECT name FROM sys.schemas WHERE schema_id < 16384 AND name NOT IN ('sys', 'INFORMATION_SCHEMA', 'guest') ORDER BY name",
            "SELECT SCHEMA_NAME()",
        ),
        Dialect::ClickHouse => (
            "SELECT name FROM system.databases WHERE name NOT IN ('INFORMATION_SCHEMA', 'information_schema') ORDER BY name",
            "SELECT currentDatabase()",
        ),
        Dialect::Oracle => (
            "SELECT username FROM all_users WHERE oracle_maintained = 'N' OR username = SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA') ORDER BY username",
            "SELECT SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA') FROM dual",
        ),
        Dialect::Redis | Dialect::Mongo => unreachable!("answered by the engine"),
    };
    let current = conn
        .text_rows(current)
        .await?
        .first()
        .and_then(|r| r.first().cloned().flatten())
        .unwrap_or_default();
    Ok(conn
        .text_rows(list)
        .await?
        .into_iter()
        .filter_map(|r| r.into_iter().next().flatten())
        .map(|name| SchemaName {
            current: name == current,
            name,
        })
        .collect())
}

/// Tables, views and routines of `schema`; `only_table` limits it to one table.
pub async fn schema(
    conn: &mut Conn,
    schema: &str,
    only_table: Option<&str>,
) -> DbResult<SchemaInfo> {
    match conn.dialect() {
        Dialect::MySql => mysql(conn, schema, only_table).await,
        Dialect::Postgres => postgres(conn, schema, only_table).await,
        Dialect::Sqlite => sqlite(conn, schema, only_table).await,
        Dialect::SqlServer => sqlserver(conn, schema, only_table).await,
        Dialect::ClickHouse => clickhouse(conn, schema, only_table).await,
        Dialect::Oracle => oracle(conn, schema, only_table).await,
        Dialect::Redis | Dialect::Mongo => match conn {
            Conn::Engine(e) => e.schema(schema, only_table).await,
            _ => unreachable!("Redis and MongoDB are engines"),
        },
    }
}

/// Collects per-table rows into [`TableInfo`]s, keeping table order.
#[derive(Default)]
struct Tables {
    order: Vec<String>,
    map: BTreeMap<String, TableInfo>,
}

impl Tables {
    fn add(&mut self, t: TableInfo) {
        self.order.push(t.name.clone());
        self.map.insert(t.name.clone(), t);
    }

    fn get(&mut self, name: &str) -> Option<&mut TableInfo> {
        self.map.get_mut(name)
    }

    /// Append `col` to the index/foreign key named `name` of `table`.
    fn index(&mut self, table: &str, name: &str, col: String, unique: bool, primary: bool) {
        if let Some(t) = self.get(table) {
            match t.indexes.iter_mut().find(|i| i.name == name) {
                Some(i) => i.columns.push(col),
                None => t.indexes.push(IndexInfo {
                    name: name.into(),
                    columns: vec![col],
                    unique,
                    primary,
                }),
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fk(
        &mut self,
        table: &str,
        name: &str,
        col: String,
        ref_schema: String,
        ref_table: String,
        ref_col: String,
        on_update: String,
        on_delete: String,
    ) {
        if let Some(t) = self.get(table) {
            match t.foreign_keys.iter_mut().find(|f| f.name == name) {
                Some(f) => {
                    f.columns.push(col);
                    f.ref_columns.push(ref_col);
                }
                None => t.foreign_keys.push(ForeignKey {
                    name: name.into(),
                    columns: vec![col],
                    ref_schema,
                    ref_table,
                    ref_columns: vec![ref_col],
                    on_update,
                    on_delete,
                }),
            }
        }
    }

    fn finish(mut self) -> Vec<TableInfo> {
        self.order
            .iter()
            .filter_map(|n| self.map.remove(n))
            .map(|mut t| {
                if t.primary_key.is_empty() {
                    if let Some(pk) = t.indexes.iter().find(|i| i.primary) {
                        t.primary_key = pk.columns.clone();
                    }
                }
                t
            })
            .collect()
    }
}

async fn mysql(conn: &mut Conn, schema: &str, only: Option<&str>) -> DbResult<SchemaInfo> {
    let d = Dialect::MySql;
    let sch = d.quote_str(schema);
    let tf = |col: &str| {
        only.map(|t| format!(" AND {col} = {}", d.quote_str(t)))
            .unwrap_or_default()
    };
    let mut tables = Tables::default();
    for r in conn
        .text_rows(&format!(
            "SELECT TABLE_NAME, TABLE_TYPE, TABLE_COMMENT, TABLE_ROWS FROM information_schema.TABLES WHERE TABLE_SCHEMA = {sch}{} ORDER BY TABLE_NAME",
            tf("TABLE_NAME")
        ))
        .await?
    {
        tables.add(TableInfo {
            name: s(&r[0]),
            kind: if s(&r[1]).contains("VIEW") { TableKind::View } else { TableKind::Table },
            comment: s(&r[2]),
            row_estimate: r[3].as_deref().and_then(|n| n.parse().ok()),
            ..Default::default()
        });
    }
    for r in conn
        .text_rows(&format!(
            "SELECT TABLE_NAME, COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT, EXTRA, COLUMN_COMMENT FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = {sch}{} ORDER BY TABLE_NAME, ORDINAL_POSITION",
            tf("TABLE_NAME")
        ))
        .await?
    {
        if let Some(t) = tables.get(&s(&r[0])) {
            t.columns.push(ColumnInfo {
                name: s(&r[1]),
                data_type: s(&r[2]),
                nullable: s(&r[3]) == "YES",
                default: r[4].clone(),
                auto_increment: s(&r[5]).contains("auto_increment"),
                comment: s(&r[6]),
            });
        }
    }
    for r in conn
        .text_rows(&format!(
            "SELECT TABLE_NAME, INDEX_NAME, NON_UNIQUE, COLUMN_NAME FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = {sch}{} ORDER BY TABLE_NAME, INDEX_NAME, SEQ_IN_INDEX",
            tf("TABLE_NAME")
        ))
        .await?
    {
        let name = s(&r[1]);
        let primary = name == "PRIMARY";
        tables.index(&s(&r[0]), &name, s(&r[3]), !flag(&r[2]) || primary, primary);
    }
    for r in conn
        .text_rows(&format!(
            "SELECT k.TABLE_NAME, k.CONSTRAINT_NAME, k.COLUMN_NAME, k.REFERENCED_TABLE_SCHEMA, k.REFERENCED_TABLE_NAME, k.REFERENCED_COLUMN_NAME, r.UPDATE_RULE, r.DELETE_RULE \
             FROM information_schema.KEY_COLUMN_USAGE k JOIN information_schema.REFERENTIAL_CONSTRAINTS r \
             ON r.CONSTRAINT_SCHEMA = k.CONSTRAINT_SCHEMA AND r.CONSTRAINT_NAME = k.CONSTRAINT_NAME \
             WHERE k.TABLE_SCHEMA = {sch} AND k.REFERENCED_TABLE_NAME IS NOT NULL{} ORDER BY k.TABLE_NAME, k.CONSTRAINT_NAME, k.ORDINAL_POSITION",
            tf("k.TABLE_NAME")
        ))
        .await?
    {
        tables.fk(&s(&r[0]), &s(&r[1]), s(&r[2]), s(&r[3]), s(&r[4]), s(&r[5]), s(&r[6]), s(&r[7]));
    }
    let routines = if only.is_some() {
        Vec::new()
    } else {
        conn.text_rows(&format!(
            "SELECT ROUTINE_NAME, ROUTINE_TYPE, DTD_IDENTIFIER FROM information_schema.ROUTINES WHERE ROUTINE_SCHEMA = {sch} ORDER BY ROUTINE_NAME"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| RoutineInfo {
            name: s(&r[0]),
            kind: s(&r[1]).to_ascii_lowercase(),
            returns: s(&r[2]),
        })
        .collect()
    };
    Ok(SchemaInfo {
        name: schema.into(),
        tables: tables.finish(),
        routines,
    })
}

fn pg_rule(code: &str) -> String {
    match code {
        "c" => "CASCADE",
        "n" => "SET NULL",
        "d" => "SET DEFAULT",
        "r" => "RESTRICT",
        _ => "",
    }
    .into()
}

async fn postgres(conn: &mut Conn, schema: &str, only: Option<&str>) -> DbResult<SchemaInfo> {
    let d = Dialect::Postgres;
    let sch = d.quote_str(schema);
    let tf = |col: &str| {
        only.map(|t| format!(" AND {col} = {}", d.quote_str(t)))
            .unwrap_or_default()
    };
    let mut tables = Tables::default();
    for r in conn
        .text_rows(&format!(
            "SELECT c.relname, c.relkind, obj_description(c.oid, 'pg_class'), c.reltuples::bigint FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = {sch} AND c.relkind IN ('r','p','v','m','f'){} ORDER BY c.relname",
            tf("c.relname")
        ))
        .await?
    {
        let kind = s(&r[1]);
        tables.add(TableInfo {
            name: s(&r[0]),
            kind: if kind == "v" || kind == "m" { TableKind::View } else { TableKind::Table },
            comment: s(&r[2]),
            row_estimate: r[3].as_deref().and_then(|n| n.parse().ok()).filter(|n: &i64| *n >= 0),
            ..Default::default()
        });
    }
    for r in conn
        .text_rows(&format!(
            "SELECT c.relname, a.attname, format_type(a.atttypid, a.atttypmod), NOT a.attnotnull, pg_get_expr(d.adbin, d.adrelid), a.attidentity, col_description(c.oid, a.attnum) \
             FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid JOIN pg_namespace n ON n.oid = c.relnamespace \
             LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
             WHERE n.nspname = {sch} AND a.attnum > 0 AND NOT a.attisdropped AND c.relkind IN ('r','p','v','m','f'){} ORDER BY c.relname, a.attnum",
            tf("c.relname")
        ))
        .await?
    {
        let default = r[4].clone();
        let identity = s(&r[5]);
        if let Some(t) = tables.get(&s(&r[0])) {
            t.columns.push(ColumnInfo {
                name: s(&r[1]),
                data_type: s(&r[2]),
                nullable: flag(&r[3]),
                auto_increment: !identity.is_empty()
                    || default.as_deref().is_some_and(|d| d.starts_with("nextval(")),
                default,
                comment: s(&r[6]),
            });
        }
    }
    for r in conn
        .text_rows(&format!(
            "SELECT t.relname, i.relname, ix.indisunique, ix.indisprimary, a.attname FROM pg_index ix \
             JOIN pg_class t ON t.oid = ix.indrelid JOIN pg_class i ON i.oid = ix.indexrelid \
             JOIN pg_namespace n ON n.oid = t.relnamespace \
             CROSS JOIN LATERAL unnest(ix.indkey) WITH ORDINALITY AS k(attnum, ord) \
             LEFT JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = k.attnum \
             WHERE n.nspname = {sch}{} ORDER BY t.relname, i.relname, k.ord",
            tf("t.relname")
        ))
        .await?
    {
        let col = r[4].clone().unwrap_or_else(|| "<expression>".into());
        tables.index(&s(&r[0]), &s(&r[1]), col, flag(&r[2]), flag(&r[3]));
    }
    for r in conn
        .text_rows(&format!(
            "SELECT cl.relname, con.conname, a.attname, fn.nspname, fc.relname, fa.attname, con.confupdtype, con.confdeltype \
             FROM pg_constraint con JOIN pg_class cl ON cl.oid = con.conrelid JOIN pg_namespace n ON n.oid = cl.relnamespace \
             JOIN pg_class fc ON fc.oid = con.confrelid JOIN pg_namespace fn ON fn.oid = fc.relnamespace \
             CROSS JOIN LATERAL unnest(con.conkey, con.confkey) WITH ORDINALITY AS k(col, fcol, ord) \
             JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.col \
             JOIN pg_attribute fa ON fa.attrelid = con.confrelid AND fa.attnum = k.fcol \
             WHERE con.contype = 'f' AND n.nspname = {sch}{} ORDER BY cl.relname, con.conname, k.ord",
            tf("cl.relname")
        ))
        .await?
    {
        tables.fk(&s(&r[0]), &s(&r[1]), s(&r[2]), s(&r[3]), s(&r[4]), s(&r[5]), pg_rule(&s(&r[6])), pg_rule(&s(&r[7])));
    }
    let routines = if only.is_some() {
        Vec::new()
    } else {
        conn.text_rows(&format!(
            "SELECT p.proname, CASE p.prokind WHEN 'p' THEN 'procedure' ELSE 'function' END, pg_get_function_result(p.oid) \
             FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace WHERE n.nspname = {sch} AND p.prokind IN ('f','p') ORDER BY p.proname"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| RoutineInfo {
            name: s(&r[0]),
            kind: s(&r[1]),
            returns: s(&r[2]),
        })
        .collect()
    };
    Ok(SchemaInfo {
        name: schema.into(),
        tables: tables.finish(),
        routines,
    })
}

async fn sqlite(conn: &mut Conn, schema: &str, only: Option<&str>) -> DbResult<SchemaInfo> {
    let d = Dialect::Sqlite;
    let sch = d.quote_ident(schema);
    let schl = d.quote_str(schema);
    let filter = only
        .map(|t| format!(" AND name = {}", d.quote_str(t)))
        .unwrap_or_default();
    let mut tables = Tables::default();
    for r in conn
        .text_rows(&format!(
            "SELECT name, type FROM {sch}.sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'{filter} ORDER BY name"
        ))
        .await?
    {
        tables.add(TableInfo {
            name: s(&r[0]),
            kind: if s(&r[1]) == "view" { TableKind::View } else { TableKind::Table },
            ..Default::default()
        });
    }
    let names = tables.order.clone();
    for name in &names {
        let tl = d.quote_str(name);
        let mut pk: Vec<(i64, String)> = Vec::new();
        let cols = conn
            .text_rows(&format!("SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_info({tl}, {schl})"))
            .await?;
        let pk_count = cols.iter().filter(|r| s(&r[4]) != "0").count();
        let mut columns = Vec::new();
        for r in cols {
            let pk_pos: i64 = r[4].as_deref().and_then(|p| p.parse().ok()).unwrap_or(0);
            let data_type = s(&r[1]);
            if pk_pos > 0 {
                pk.push((pk_pos, s(&r[0])));
            }
            columns.push(ColumnInfo {
                name: s(&r[0]),
                // INTEGER PRIMARY KEY is an alias of the rowid.
                auto_increment: pk_pos > 0
                    && pk_count == 1
                    && data_type.eq_ignore_ascii_case("INTEGER"),
                data_type,
                nullable: s(&r[2]) == "0" && pk_pos == 0,
                default: r[3].clone(),
                comment: String::new(),
            });
        }
        pk.sort();
        let t = tables.get(name).expect("table listed");
        t.columns = columns;
        t.primary_key = pk.into_iter().map(|(_, c)| c).collect();
        if t.kind == TableKind::View {
            continue;
        }
        for r in conn
            .text_rows(&format!(
                "SELECT id, \"table\", \"from\", \"to\", on_update, on_delete FROM pragma_foreign_key_list({tl}, {schl}) ORDER BY id, seq"
            ))
            .await?
        {
            let fk_name = format!("fk_{name}_{}", s(&r[0]));
            let mut to = s(&r[3]);
            if to.is_empty() {
                to = "rowid".into();
            }
            tables.fk(name, &fk_name, s(&r[2]), schema.into(), s(&r[1]), to, s(&r[4]), s(&r[5]));
        }
        let indexes = conn
            .text_rows(&format!(
                "SELECT name, \"unique\", origin FROM pragma_index_list({tl}, {schl})"
            ))
            .await?;
        for r in indexes {
            let iname = s(&r[0]);
            let cols = conn
                .text_rows(&format!(
                    "SELECT name FROM pragma_index_info({}, {schl}) ORDER BY seqno",
                    d.quote_str(&iname)
                ))
                .await?;
            for c in cols {
                tables.index(name, &iname, s(&c[0]), flag(&r[1]), s(&r[2]) == "pk");
            }
        }
    }
    Ok(SchemaInfo {
        name: schema.into(),
        tables: tables.finish(),
        routines: Vec::new(),
    })
}

fn sqlserver_rule(rule: &str) -> String {
    rule.replace('_', " ")
}

async fn sqlserver(conn: &mut Conn, schema: &str, only: Option<&str>) -> DbResult<SchemaInfo> {
    let d = Dialect::SqlServer;
    let sch = d.quote_str(schema);
    let tf = |col: &str| {
        only.map(|t| format!(" AND {col} = {}", d.quote_str(t)))
            .unwrap_or_default()
    };
    let mut tables = Tables::default();
    for r in conn
        .text_rows(&format!(
            "SELECT o.name, o.type, CAST(ep.value AS nvarchar(4000)), \
             (SELECT SUM(p.rows) FROM sys.partitions p WHERE p.object_id = o.object_id AND p.index_id IN (0, 1)) \
             FROM sys.objects o LEFT JOIN sys.extended_properties ep ON ep.major_id = o.object_id AND ep.minor_id = 0 AND ep.name = 'MS_Description' \
             WHERE o.type IN ('U', 'V') AND SCHEMA_NAME(o.schema_id) = {sch}{} ORDER BY o.name",
            tf("o.name")
        ))
        .await?
    {
        tables.add(TableInfo {
            name: s(&r[0]),
            kind: if s(&r[1]).trim() == "V" { TableKind::View } else { TableKind::Table },
            comment: s(&r[2]),
            row_estimate: r[3].as_deref().and_then(|n| n.parse().ok()),
            ..Default::default()
        });
    }
    for r in conn
        .text_rows(&format!(
            "SELECT c.TABLE_NAME, c.COLUMN_NAME, \
             c.DATA_TYPE + CASE WHEN c.CHARACTER_MAXIMUM_LENGTH = -1 THEN '(max)' WHEN c.CHARACTER_MAXIMUM_LENGTH IS NOT NULL THEN '(' + CAST(c.CHARACTER_MAXIMUM_LENGTH AS varchar(10)) + ')' \
             WHEN c.DATA_TYPE IN ('decimal', 'numeric') THEN '(' + CAST(c.NUMERIC_PRECISION AS varchar(5)) + ',' + CAST(c.NUMERIC_SCALE AS varchar(5)) + ')' ELSE '' END, \
             c.IS_NULLABLE, c.COLUMN_DEFAULT, COLUMNPROPERTY(OBJECT_ID(QUOTENAME(c.TABLE_SCHEMA) + '.' + QUOTENAME(c.TABLE_NAME)), c.COLUMN_NAME, 'IsIdentity') \
             FROM INFORMATION_SCHEMA.COLUMNS c WHERE c.TABLE_SCHEMA = {sch}{} ORDER BY c.TABLE_NAME, c.ORDINAL_POSITION",
            tf("c.TABLE_NAME")
        ))
        .await?
    {
        if let Some(t) = tables.get(&s(&r[0])) {
            t.columns.push(ColumnInfo {
                name: s(&r[1]),
                data_type: s(&r[2]),
                nullable: s(&r[3]) == "YES",
                default: r[4].clone(),
                auto_increment: flag(&r[5]),
                comment: String::new(),
            });
        }
    }
    for r in conn
        .text_rows(&format!(
            "SELECT t.name, i.name, i.is_unique, i.is_primary_key, c.name FROM sys.indexes i \
             JOIN sys.tables t ON t.object_id = i.object_id \
             JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
             JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id \
             WHERE i.name IS NOT NULL AND ic.is_included_column = 0 AND SCHEMA_NAME(t.schema_id) = {sch}{} \
             ORDER BY t.name, i.name, ic.key_ordinal",
            tf("t.name")
        ))
        .await?
    {
        tables.index(&s(&r[0]), &s(&r[1]), s(&r[4]), flag(&r[2]), flag(&r[3]));
    }
    for r in conn
        .text_rows(&format!(
            "SELECT tp.name, fk.name, cp.name, SCHEMA_NAME(tr.schema_id), tr.name, cr.name, fk.update_referential_action_desc, fk.delete_referential_action_desc \
             FROM sys.foreign_keys fk JOIN sys.foreign_key_columns fkc ON fkc.constraint_object_id = fk.object_id \
             JOIN sys.tables tp ON tp.object_id = fk.parent_object_id JOIN sys.columns cp ON cp.object_id = fkc.parent_object_id AND cp.column_id = fkc.parent_column_id \
             JOIN sys.tables tr ON tr.object_id = fk.referenced_object_id JOIN sys.columns cr ON cr.object_id = fkc.referenced_object_id AND cr.column_id = fkc.referenced_column_id \
             WHERE SCHEMA_NAME(tp.schema_id) = {sch}{} ORDER BY tp.name, fk.name, fkc.constraint_column_id",
            tf("tp.name")
        ))
        .await?
    {
        tables.fk(&s(&r[0]), &s(&r[1]), s(&r[2]), s(&r[3]), s(&r[4]), s(&r[5]), sqlserver_rule(&s(&r[6])), sqlserver_rule(&s(&r[7])));
    }
    let routines = if only.is_some() {
        Vec::new()
    } else {
        conn.text_rows(&format!(
            "SELECT ROUTINE_NAME, ROUTINE_TYPE, DATA_TYPE FROM INFORMATION_SCHEMA.ROUTINES WHERE ROUTINE_SCHEMA = {sch} ORDER BY ROUTINE_NAME"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| RoutineInfo {
            name: s(&r[0]),
            kind: s(&r[1]).to_ascii_lowercase(),
            returns: s(&r[2]),
        })
        .collect()
    };
    Ok(SchemaInfo {
        name: schema.into(),
        tables: tables.finish(),
        routines,
    })
}

async fn clickhouse(conn: &mut Conn, schema: &str, only: Option<&str>) -> DbResult<SchemaInfo> {
    let d = Dialect::ClickHouse;
    let sch = d.quote_str(schema);
    let tf = |col: &str| {
        only.map(|t| format!(" AND {col} = {}", d.quote_str(t)))
            .unwrap_or_default()
    };
    let mut tables = Tables::default();
    for r in conn
        .text_rows(&format!(
            "SELECT name, engine, comment, total_rows FROM system.tables WHERE database = {sch}{} ORDER BY name",
            tf("name")
        ))
        .await?
    {
        let engine = s(&r[1]);
        tables.add(TableInfo {
            name: s(&r[0]),
            kind: if engine.contains("View") { TableKind::View } else { TableKind::Table },
            comment: s(&r[2]),
            row_estimate: r[3].as_deref().and_then(|n| n.parse().ok()),
            ..Default::default()
        });
    }
    for r in conn
        .text_rows(&format!(
            "SELECT table, name, type, default_kind, default_expression, comment, is_in_primary_key FROM system.columns WHERE database = {sch}{} ORDER BY table, position",
            tf("table")
        ))
        .await?
    {
        let table = s(&r[0]);
        let col = s(&r[1]);
        let pk = flag(&r[6]);
        if let Some(t) = tables.get(&table) {
            let data_type = s(&r[2]);
            let default = r[4].clone().filter(|d| !d.is_empty());
            t.columns.push(ColumnInfo {
                nullable: data_type.starts_with("Nullable("),
                data_type,
                default: default.map(|d| match s(&r[3]).as_str() {
                    "" | "DEFAULT" => d,
                    kind => format!("{kind} {d}"),
                }),
                auto_increment: false,
                comment: s(&r[5]),
                name: col.clone(),
            });
            if pk {
                t.primary_key.push(col);
            }
        }
    }
    Ok(SchemaInfo {
        name: schema.into(),
        tables: tables.finish(),
        routines: Vec::new(),
    })
}

async fn oracle(conn: &mut Conn, schema: &str, only: Option<&str>) -> DbResult<SchemaInfo> {
    let d = Dialect::Oracle;
    let sch = d.quote_str(schema);
    let tf = |col: &str| {
        only.map(|t| format!(" AND {col} = {}", d.quote_str(t)))
            .unwrap_or_default()
    };
    let mut tables = Tables::default();
    for r in conn
        .text_rows(&format!(
            "SELECT o.object_name, o.object_type, c.comments, t.num_rows FROM all_objects o \
             LEFT JOIN all_tab_comments c ON c.owner = o.owner AND c.table_name = o.object_name \
             LEFT JOIN all_tables t ON t.owner = o.owner AND t.table_name = o.object_name \
             WHERE o.owner = {sch} AND o.object_type IN ('TABLE', 'VIEW') AND o.object_name NOT LIKE 'BIN$%'{} ORDER BY o.object_name",
            tf("o.object_name")
        ))
        .await?
    {
        tables.add(TableInfo {
            name: s(&r[0]),
            kind: if s(&r[1]) == "VIEW" { TableKind::View } else { TableKind::Table },
            comment: s(&r[2]),
            row_estimate: r[3].as_deref().and_then(|n| n.parse().ok()),
            ..Default::default()
        });
    }
    for r in conn
        .text_rows(&format!(
            "SELECT table_name, column_name, data_type || CASE WHEN data_type IN ('VARCHAR2', 'NVARCHAR2', 'CHAR', 'NCHAR', 'RAW') THEN '(' || char_length || ')' \
             WHEN data_type = 'NUMBER' AND data_precision IS NOT NULL THEN '(' || data_precision || ',' || data_scale || ')' ELSE '' END, \
             nullable, data_default, identity_column FROM all_tab_columns WHERE owner = {sch}{} ORDER BY table_name, column_id",
            tf("table_name")
        ))
        .await?
    {
        if let Some(t) = tables.get(&s(&r[0])) {
            t.columns.push(ColumnInfo {
                name: s(&r[1]),
                data_type: s(&r[2]),
                nullable: s(&r[3]) == "Y",
                default: r[4].clone().map(|d| d.trim().to_string()).filter(|d| !d.is_empty()),
                auto_increment: s(&r[5]) == "YES",
                comment: String::new(),
            });
        }
    }
    for r in conn
        .text_rows(&format!(
            "SELECT i.table_name, i.index_name, i.uniqueness, CASE WHEN c.constraint_type = 'P' THEN 'Y' ELSE 'N' END, ic.column_name \
             FROM all_indexes i JOIN all_ind_columns ic ON ic.index_owner = i.owner AND ic.index_name = i.index_name \
             LEFT JOIN all_constraints c ON c.owner = i.owner AND c.index_name = i.index_name AND c.constraint_type = 'P' \
             WHERE i.table_owner = {sch}{} ORDER BY i.table_name, i.index_name, ic.column_position",
            tf("i.table_name")
        ))
        .await?
    {
        tables.index(&s(&r[0]), &s(&r[1]), s(&r[4]), s(&r[2]) == "UNIQUE", s(&r[3]) == "Y");
    }
    for r in conn
        .text_rows(&format!(
            "SELECT c.table_name, c.constraint_name, cc.column_name, r.owner, r.table_name, rc.column_name, 'NO ACTION', c.delete_rule \
             FROM all_constraints c JOIN all_cons_columns cc ON cc.owner = c.owner AND cc.constraint_name = c.constraint_name \
             JOIN all_constraints r ON r.owner = c.r_owner AND r.constraint_name = c.r_constraint_name \
             JOIN all_cons_columns rc ON rc.owner = r.owner AND rc.constraint_name = r.constraint_name AND rc.position = cc.position \
             WHERE c.constraint_type = 'R' AND c.owner = {sch}{} ORDER BY c.table_name, c.constraint_name, cc.position",
            tf("c.table_name")
        ))
        .await?
    {
        tables.fk(&s(&r[0]), &s(&r[1]), s(&r[2]), s(&r[3]), s(&r[4]), s(&r[5]), s(&r[6]), s(&r[7]));
    }
    let routines = if only.is_some() {
        Vec::new()
    } else {
        conn.text_rows(&format!(
            "SELECT object_name, LOWER(object_type) FROM all_objects WHERE owner = {sch} AND object_type IN ('PROCEDURE', 'FUNCTION', 'PACKAGE') ORDER BY object_name"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| RoutineInfo {
            name: s(&r[0]),
            kind: s(&r[1]),
            returns: String::new(),
        })
        .collect()
    };
    Ok(SchemaInfo {
        name: schema.into(),
        tables: tables.finish(),
        routines,
    })
}
