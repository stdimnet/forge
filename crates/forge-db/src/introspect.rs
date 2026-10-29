//! Database metadata, per dialect: schemas, tables, views, materialized views, columns,
//! keys, foreign keys, indexes, checks, triggers, routines with their arguments, sequences,
//! user-defined types, and server objects (the current database, other databases, roles
//! and users). The model types live in `forge-dbmodel`.
//!
//! Every catalog query starts with a `/* forge:<dialect>.<what> */` tag, which names it in
//! server logs and lets tests answer it from recorded catalog rows. Catalogs that need
//! privileges or newer servers (checks, triggers, routines, roles, …) are optional: when
//! they fail the objects are left out.

use std::collections::BTreeMap;

use crate::config::Dialect;
use crate::conn::Conn;
use crate::error::DbResult;

pub use forge_dbmodel::{
    CheckInfo, ColumnInfo, ForeignKey, IndexInfo, RoleInfo, RoutineArg, RoutineInfo, SchemaInfo,
    SchemaName, SequenceInfo, TableInfo, TableKind, TriggerInfo, TypeInfo,
};

type Rows = Vec<Vec<Option<String>>>;

/// A catalog query with its tag.
pub fn tagged(tag: &str, sql: &str) -> String {
    format!("/* forge:{tag} */ {sql}")
}

/// The tag of a catalog query (`mysql.tables`), for test fixtures.
pub fn tag_of(sql: &str) -> Option<&str> {
    let rest = sql.trim_start().strip_prefix("/* forge:")?;
    rest.split_once(" */").map(|(t, _)| t)
}

async fn rows(conn: &mut Conn, tag: &str, sql: &str) -> DbResult<Rows> {
    conn.text_rows(&tagged(tag, sql)).await
}

/// Rows of a catalog that may be missing or forbidden: empty then.
async fn opt_rows(conn: &mut Conn, tag: &str, sql: &str) -> Rows {
    match rows(conn, tag, sql).await {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("catalog query {tag} skipped: {e}");
            Vec::new()
        }
    }
}

fn s(r: &[Option<String>], i: usize) -> String {
    r.get(i).cloned().flatten().unwrap_or_default()
}

fn o(r: &[Option<String>], i: usize) -> Option<String> {
    r.get(i).cloned().flatten()
}

fn flag(r: &[Option<String>], i: usize) -> bool {
    matches!(
        r.get(i).and_then(|v| v.as_deref()),
        Some("1" | "t" | "true" | "YES" | "yes" | "Y")
    )
}

fn num(r: &[Option<String>], i: usize) -> Option<i64> {
    r.get(i)
        .and_then(|v| v.as_deref())
        .and_then(|n| n.trim().parse::<f64>().ok())
        .map(|n| n as i64)
}

/// Schemas (MySQL and ClickHouse databases, Oracle users) visible on the connection.
pub async fn schemas(conn: &mut Conn) -> DbResult<Vec<SchemaName>> {
    if let Conn::Engine(e) = conn {
        if !e.dialect().is_sql() {
            return e.schemas().await;
        }
    }
    let d = conn.dialect();
    let (list, current) = match d {
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
    let tag = dialect_tag(d);
    let current = rows(conn, &format!("{tag}.current_schema"), current)
        .await?
        .first()
        .map(|r| s(r, 0))
        .unwrap_or_default();
    Ok(rows(conn, &format!("{tag}.schemas"), list)
        .await?
        .into_iter()
        .filter_map(|r| r.into_iter().next().flatten())
        .map(|name| {
            let cur = name == current;
            SchemaName::new(name, cur)
        })
        .collect())
}

fn dialect_tag(d: Dialect) -> &'static str {
    match d {
        Dialect::MySql => "mysql",
        Dialect::Postgres => "postgres",
        Dialect::Sqlite => "sqlite",
        Dialect::SqlServer => "sqlserver",
        Dialect::ClickHouse => "clickhouse",
        Dialect::Oracle => "oracle",
        Dialect::Redis => "redis",
        Dialect::Mongo => "mongo",
    }
}

/// Server objects above the schemas: the connected database, the other databases and the
/// roles and users. Every part is optional.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ServerObjects {
    pub catalog: String,
    pub catalogs: Vec<String>,
    pub roles: Vec<RoleInfo>,
}

pub async fn server_objects(conn: &mut Conn) -> ServerObjects {
    let d = conn.dialect();
    let first = |r: Rows| r.first().map(|r| s(r, 0)).unwrap_or_default();
    let names = |r: Rows| r.iter().map(|r| s(r, 0)).collect::<Vec<_>>();
    let role = |name: String, kind: &str, superuser: bool, can_login: bool| RoleInfo {
        name,
        kind: kind.into(),
        superuser,
        can_login,
    };
    match d {
        Dialect::Postgres => ServerObjects {
            catalog: first(opt_rows(conn, "postgres.catalog", "SELECT current_database()").await),
            catalogs: names(
                opt_rows(
                    conn,
                    "postgres.catalogs",
                    "SELECT datname FROM pg_database WHERE NOT datistemplate ORDER BY datname",
                )
                .await,
            ),
            roles: opt_rows(
                conn,
                "postgres.roles",
                "SELECT rolname, rolsuper, rolcanlogin FROM pg_roles WHERE rolname NOT LIKE 'pg\\_%' ORDER BY rolname",
            )
            .await
            .into_iter()
            .map(|r| {
                let login = flag(&r, 2);
                role(s(&r, 0), if login { "user" } else { "role" }, flag(&r, 1), login)
            })
            .collect(),
        },
        Dialect::SqlServer => ServerObjects {
            catalog: first(opt_rows(conn, "sqlserver.catalog", "SELECT DB_NAME()").await),
            catalogs: names(
                opt_rows(conn, "sqlserver.catalogs", "SELECT name FROM sys.databases ORDER BY name")
                    .await,
            ),
            roles: opt_rows(
                conn,
                "sqlserver.roles",
                "SELECT name, type, IS_SRVROLEMEMBER('sysadmin', name), CASE WHEN is_disabled = 0 THEN 1 ELSE 0 END \
                 FROM sys.server_principals WHERE type IN ('S', 'U', 'G', 'R') AND name NOT LIKE '##%' ORDER BY name",
            )
            .await
            .into_iter()
            .map(|r| {
                let is_role = s(&r, 1).trim() == "R";
                role(
                    s(&r, 0),
                    if is_role { "role" } else { "user" },
                    flag(&r, 2),
                    !is_role && flag(&r, 3),
                )
            })
            .collect(),
        },
        Dialect::MySql => ServerObjects {
            roles: opt_rows(
                conn,
                "mysql.roles",
                "SELECT CONCAT(User, '@', Host), Super_priv, CASE WHEN account_locked = 'Y' THEN 'N' ELSE 'Y' END FROM mysql.user ORDER BY User, Host",
            )
            .await
            .into_iter()
            .map(|r| role(s(&r, 0), "user", flag(&r, 1), flag(&r, 2)))
            .collect(),
            ..Default::default()
        },
        Dialect::ClickHouse => ServerObjects {
            roles: opt_rows(
                conn,
                "clickhouse.roles",
                "SELECT name, 'user' FROM system.users UNION ALL SELECT name, 'role' FROM system.roles ORDER BY name",
            )
            .await
            .into_iter()
            .map(|r| {
                let kind = s(&r, 1);
                let login = kind == "user";
                role(s(&r, 0), &kind, false, login)
            })
            .collect(),
            ..Default::default()
        },
        Dialect::Oracle => ServerObjects {
            catalog: first(
                opt_rows(conn, "oracle.catalog", "SELECT SYS_CONTEXT('USERENV', 'DB_NAME') FROM dual")
                    .await,
            ),
            roles: opt_rows(conn, "oracle.roles", "SELECT role FROM session_roles ORDER BY role")
                .await
                .into_iter()
                .map(|r| role(s(&r, 0), "role", false, false))
                .collect(),
            ..Default::default()
        },
        Dialect::Sqlite | Dialect::Redis | Dialect::Mongo => ServerObjects::default(),
    }
}

/// Tables, views, routines, sequences and types of `schema`; `only_table` limits it to one
/// table (the incremental refresh of a table node).
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
        if !self.map.contains_key(&t.name) {
            self.order.push(t.name.clone());
        }
        self.map.insert(t.name.clone(), t);
    }

    fn get(&mut self, name: &str) -> Option<&mut TableInfo> {
        self.map.get_mut(name)
    }

    /// Append `col` to the index named `name` of `table`.
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

    /// A foreign key row: table, name, column, ref schema, ref table, ref column, on update,
    /// on delete.
    fn fk(&mut self, r: &[Option<String>], rule: fn(&str) -> String) {
        let name = s(r, 1);
        if let Some(t) = self.get(&s(r, 0)) {
            match t.foreign_keys.iter_mut().find(|f| f.name == name) {
                Some(f) => {
                    f.columns.push(s(r, 2));
                    f.ref_columns.push(s(r, 5));
                }
                None => t.foreign_keys.push(ForeignKey {
                    name,
                    columns: vec![s(r, 2)],
                    ref_schema: s(r, 3),
                    ref_table: s(r, 4),
                    ref_columns: vec![s(r, 5)],
                    on_update: rule(&s(r, 6)),
                    on_delete: rule(&s(r, 7)),
                    is_virtual: false,
                }),
            }
        }
    }

    /// A check row: table, name, expression.
    fn check(&mut self, r: &[Option<String>]) {
        let expr = s(r, 2);
        let expr = expr
            .strip_prefix("CHECK ")
            .map(str::to_string)
            .unwrap_or(expr);
        if let Some(t) = self.get(&s(r, 0)) {
            t.checks.push(CheckInfo {
                name: s(r, 1),
                expression: expr,
            });
        }
    }

    /// A trigger row: table, name, timing, events, definition.
    fn trigger(&mut self, r: &[Option<String>]) {
        if let Some(t) = self.get(&s(r, 0)) {
            t.triggers.push(TriggerInfo {
                name: s(r, 1),
                timing: s(r, 2).to_uppercase(),
                events: s(r, 3).to_uppercase(),
                definition: s(r, 4).trim().to_string(),
            });
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

fn same(rule: &str) -> String {
    rule.to_string()
}

/// Routines from rows (name, kind, returns, definition, language) and argument rows
/// (routine, name, type, mode).
fn routines(list: Rows, args: Rows) -> Vec<RoutineInfo> {
    let mut by_name: BTreeMap<String, Vec<RoutineArg>> = BTreeMap::new();
    for a in args {
        let mode = s(&a, 3).to_uppercase();
        by_name.entry(s(&a, 0)).or_default().push(RoutineArg {
            name: s(&a, 1),
            data_type: s(&a, 2),
            mode: if mode == "IN" { String::new() } else { mode },
        });
    }
    list.into_iter()
        .map(|r| RoutineInfo {
            arguments: by_name.get(&s(&r, 0)).cloned().unwrap_or_default(),
            name: s(&r, 0),
            kind: s(&r, 1).to_ascii_lowercase(),
            returns: s(&r, 2),
            definition: s(&r, 3).trim().to_string(),
            language: s(&r, 4),
        })
        .collect()
}

/// Arguments of `pg_get_function_arguments` (`o_id bigint, OUT total numeric DEFAULT 0`).
pub fn parse_arguments(text: &str) -> Vec<RoutineArg> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i32, 0);
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    const MULTI: [&str; 6] = [
        "character",
        "double",
        "timestamp",
        "time",
        "bit",
        "interval",
    ];
    parts
        .into_iter()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            let p = match p.to_ascii_uppercase().find(" DEFAULT ") {
                Some(i) => &p[..i],
                None => p.split(" = ").next().unwrap_or(p),
            };
            let mut words: Vec<&str> = p.split_whitespace().collect();
            let mode = match words.first().map(|w| w.to_ascii_uppercase()) {
                Some(m) if matches!(m.as_str(), "IN" | "OUT" | "INOUT" | "VARIADIC") => {
                    words.remove(0);
                    if m == "IN" {
                        String::new()
                    } else {
                        m
                    }
                }
                _ => String::new(),
            };
            let named =
                words.len() >= 2 && !MULTI.contains(&words[0].to_ascii_lowercase().as_str());
            let (name, ty) = if named {
                (words[0].to_string(), words[1..].join(" "))
            } else {
                (String::new(), words.join(" "))
            };
            RoutineArg {
                name,
                data_type: ty,
                mode,
            }
        })
        .collect()
}

async fn mysql(conn: &mut Conn, schema: &str, only: Option<&str>) -> DbResult<SchemaInfo> {
    let d = Dialect::MySql;
    let sch = d.quote_str(schema);
    let tf = |col: &str| {
        only.map(|t| format!(" AND {col} = {}", d.quote_str(t)))
            .unwrap_or_default()
    };
    let mut tables = Tables::default();
    for r in rows(
        conn,
        "mysql.tables",
        &format!(
            "SELECT TABLE_NAME, TABLE_TYPE, TABLE_COMMENT, TABLE_ROWS FROM information_schema.TABLES WHERE TABLE_SCHEMA = {sch}{} ORDER BY TABLE_NAME",
            tf("TABLE_NAME")
        ),
    )
    .await?
    {
        let ty = s(&r, 1);
        if ty == "SEQUENCE" {
            continue;
        }
        tables.add(TableInfo {
            name: s(&r, 0),
            kind: if ty.contains("VIEW") { TableKind::View } else { TableKind::Table },
            comment: s(&r, 2),
            row_estimate: num(&r, 3),
            ..Default::default()
        });
    }
    for r in rows(
        conn,
        "mysql.columns",
        &format!(
            "SELECT TABLE_NAME, COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT, EXTRA, COLUMN_COMMENT, GENERATION_EXPRESSION FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = {sch}{} ORDER BY TABLE_NAME, ORDINAL_POSITION",
            tf("TABLE_NAME")
        ),
    )
    .await?
    {
        if let Some(t) = tables.get(&s(&r, 0)) {
            t.columns.push(ColumnInfo {
                name: s(&r, 1),
                data_type: s(&r, 2),
                nullable: s(&r, 3) == "YES",
                default: o(&r, 4),
                auto_increment: s(&r, 5).contains("auto_increment"),
                comment: s(&r, 6),
                expression: o(&r, 7).filter(|e| !e.is_empty()),
                ..Default::default()
            });
        }
    }
    for r in rows(
        conn,
        "mysql.indexes",
        &format!(
            "SELECT TABLE_NAME, INDEX_NAME, NON_UNIQUE, COLUMN_NAME FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = {sch}{} ORDER BY TABLE_NAME, INDEX_NAME, SEQ_IN_INDEX",
            tf("TABLE_NAME")
        ),
    )
    .await?
    {
        let name = s(&r, 1);
        let primary = name == "PRIMARY";
        tables.index(&s(&r, 0), &name, s(&r, 3), !flag(&r, 2) || primary, primary);
    }
    for r in rows(
        conn,
        "mysql.foreign_keys",
        &format!(
            "SELECT k.TABLE_NAME, k.CONSTRAINT_NAME, k.COLUMN_NAME, k.REFERENCED_TABLE_SCHEMA, k.REFERENCED_TABLE_NAME, k.REFERENCED_COLUMN_NAME, r.UPDATE_RULE, r.DELETE_RULE \
             FROM information_schema.KEY_COLUMN_USAGE k JOIN information_schema.REFERENTIAL_CONSTRAINTS r \
             ON r.CONSTRAINT_SCHEMA = k.CONSTRAINT_SCHEMA AND r.CONSTRAINT_NAME = k.CONSTRAINT_NAME \
             WHERE k.TABLE_SCHEMA = {sch} AND k.REFERENCED_TABLE_NAME IS NOT NULL{} ORDER BY k.TABLE_NAME, k.CONSTRAINT_NAME, k.ORDINAL_POSITION",
            tf("k.TABLE_NAME")
        ),
    )
    .await?
    {
        tables.fk(&r, same);
    }
    // MySQL 8.0.16+ and MariaDB 10.2+.
    for r in opt_rows(
        conn,
        "mysql.checks",
        &format!(
            "SELECT tc.TABLE_NAME, cc.CONSTRAINT_NAME, cc.CHECK_CLAUSE FROM information_schema.CHECK_CONSTRAINTS cc \
             JOIN information_schema.TABLE_CONSTRAINTS tc ON tc.CONSTRAINT_SCHEMA = cc.CONSTRAINT_SCHEMA AND tc.CONSTRAINT_NAME = cc.CONSTRAINT_NAME AND tc.CONSTRAINT_TYPE = 'CHECK' \
             WHERE cc.CONSTRAINT_SCHEMA = {sch}{} ORDER BY tc.TABLE_NAME, cc.CONSTRAINT_NAME",
            tf("tc.TABLE_NAME")
        ),
    )
    .await
    {
        tables.check(&r);
    }
    for r in opt_rows(
        conn,
        "mysql.triggers",
        &format!(
            "SELECT EVENT_OBJECT_TABLE, TRIGGER_NAME, ACTION_TIMING, EVENT_MANIPULATION, ACTION_STATEMENT FROM information_schema.TRIGGERS WHERE TRIGGER_SCHEMA = {sch}{} ORDER BY EVENT_OBJECT_TABLE, ACTION_ORDER",
            tf("EVENT_OBJECT_TABLE")
        ),
    )
    .await
    {
        tables.trigger(&r);
    }
    for r in opt_rows(
        conn,
        "mysql.views",
        &format!(
            "SELECT TABLE_NAME, VIEW_DEFINITION FROM information_schema.VIEWS WHERE TABLE_SCHEMA = {sch}{}",
            tf("TABLE_NAME")
        ),
    )
    .await
    {
        if let Some(t) = tables.get(&s(&r, 0)) {
            t.definition = s(&r, 1);
        }
    }
    let routines = if only.is_some() {
        Vec::new()
    } else {
        let list = opt_rows(
            conn,
            "mysql.routines",
            &format!(
                "SELECT ROUTINE_NAME, ROUTINE_TYPE, DTD_IDENTIFIER, ROUTINE_DEFINITION, ROUTINE_BODY FROM information_schema.ROUTINES WHERE ROUTINE_SCHEMA = {sch} ORDER BY ROUTINE_NAME"
            ),
        )
        .await;
        let args = opt_rows(
            conn,
            "mysql.parameters",
            &format!(
                "SELECT SPECIFIC_NAME, PARAMETER_NAME, DTD_IDENTIFIER, PARAMETER_MODE FROM information_schema.PARAMETERS WHERE SPECIFIC_SCHEMA = {sch} AND ORDINAL_POSITION > 0 ORDER BY SPECIFIC_NAME, ORDINAL_POSITION"
            ),
        )
        .await;
        self::routines(list, args)
    };
    Ok(SchemaInfo {
        name: schema.into(),
        tables: tables.finish(),
        routines,
        ..Default::default()
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
    for r in rows(
        conn,
        "postgres.tables",
        &format!(
            "SELECT c.relname, c.relkind, obj_description(c.oid, 'pg_class'), c.reltuples::bigint, \
             CASE WHEN c.relkind IN ('v','m') THEN pg_get_viewdef(c.oid, true) END FROM pg_class c \
             JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = {sch} AND c.relkind IN ('r','p','v','m','f'){} ORDER BY c.relname",
            tf("c.relname")
        ),
    )
    .await?
    {
        let kind = match s(&r, 1).as_str() {
            "v" => TableKind::View,
            "m" => TableKind::MaterializedView,
            _ => TableKind::Table,
        };
        tables.add(TableInfo {
            name: s(&r, 0),
            kind,
            comment: s(&r, 2),
            row_estimate: num(&r, 3).filter(|n| *n >= 0),
            definition: s(&r, 4).trim().to_string(),
            ..Default::default()
        });
    }
    for r in rows(
        conn,
        "postgres.columns",
        &format!(
            "SELECT c.relname, a.attname, format_type(a.atttypid, a.atttypmod), NOT a.attnotnull, pg_get_expr(d.adbin, d.adrelid), a.attidentity, col_description(c.oid, a.attnum), a.attgenerated \
             FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid JOIN pg_namespace n ON n.oid = c.relnamespace \
             LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
             WHERE n.nspname = {sch} AND a.attnum > 0 AND NOT a.attisdropped AND c.relkind IN ('r','p','v','m','f'){} ORDER BY c.relname, a.attnum",
            tf("c.relname")
        ),
    )
    .await?
    {
        let default = o(&r, 4);
        let identity = s(&r, 5);
        let generated = s(&r, 7) == "s";
        if let Some(t) = tables.get(&s(&r, 0)) {
            t.columns.push(ColumnInfo {
                name: s(&r, 1),
                data_type: s(&r, 2),
                nullable: flag(&r, 3),
                auto_increment: !identity.is_empty()
                    || default.as_deref().is_some_and(|d| d.starts_with("nextval(")),
                expression: if generated { default.clone() } else { None },
                default: if generated { None } else { default },
                comment: s(&r, 6),
                ..Default::default()
            });
        }
    }
    for r in rows(
        conn,
        "postgres.indexes",
        &format!(
            "SELECT t.relname, i.relname, ix.indisunique, ix.indisprimary, a.attname FROM pg_index ix \
             JOIN pg_class t ON t.oid = ix.indrelid JOIN pg_class i ON i.oid = ix.indexrelid \
             JOIN pg_namespace n ON n.oid = t.relnamespace \
             CROSS JOIN LATERAL unnest(ix.indkey) WITH ORDINALITY AS k(attnum, ord) \
             LEFT JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = k.attnum \
             WHERE n.nspname = {sch}{} ORDER BY t.relname, i.relname, k.ord",
            tf("t.relname")
        ),
    )
    .await?
    {
        let col = o(&r, 4).unwrap_or_else(|| "<expression>".into());
        tables.index(&s(&r, 0), &s(&r, 1), col, flag(&r, 2), flag(&r, 3));
    }
    for r in rows(
        conn,
        "postgres.foreign_keys",
        &format!(
            "SELECT cl.relname, con.conname, a.attname, fn.nspname, fc.relname, fa.attname, con.confupdtype, con.confdeltype \
             FROM pg_constraint con JOIN pg_class cl ON cl.oid = con.conrelid JOIN pg_namespace n ON n.oid = cl.relnamespace \
             JOIN pg_class fc ON fc.oid = con.confrelid JOIN pg_namespace fn ON fn.oid = fc.relnamespace \
             CROSS JOIN LATERAL unnest(con.conkey, con.confkey) WITH ORDINALITY AS k(col, fcol, ord) \
             JOIN pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.col \
             JOIN pg_attribute fa ON fa.attrelid = con.confrelid AND fa.attnum = k.fcol \
             WHERE con.contype = 'f' AND n.nspname = {sch}{} ORDER BY cl.relname, con.conname, k.ord",
            tf("cl.relname")
        ),
    )
    .await?
    {
        tables.fk(&r, pg_rule);
    }
    for r in opt_rows(
        conn,
        "postgres.checks",
        &format!(
            "SELECT cl.relname, con.conname, pg_get_constraintdef(con.oid, true) FROM pg_constraint con \
             JOIN pg_class cl ON cl.oid = con.conrelid JOIN pg_namespace n ON n.oid = cl.relnamespace \
             WHERE con.contype = 'c' AND n.nspname = {sch}{} ORDER BY cl.relname, con.conname",
            tf("cl.relname")
        ),
    )
    .await
    {
        tables.check(&r);
    }
    for r in opt_rows(
        conn,
        "postgres.triggers",
        &format!(
            "SELECT c.relname, t.tgname, CASE WHEN t.tgtype & 2 = 2 THEN 'BEFORE' WHEN t.tgtype & 64 = 64 THEN 'INSTEAD OF' ELSE 'AFTER' END, \
             concat_ws(' OR ', CASE WHEN t.tgtype & 4 = 4 THEN 'INSERT' END, CASE WHEN t.tgtype & 16 = 16 THEN 'UPDATE' END, \
             CASE WHEN t.tgtype & 8 = 8 THEN 'DELETE' END, CASE WHEN t.tgtype & 32 = 32 THEN 'TRUNCATE' END), pg_get_triggerdef(t.oid, true) \
             FROM pg_trigger t JOIN pg_class c ON c.oid = t.tgrelid JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE NOT t.tgisinternal AND n.nspname = {sch}{} ORDER BY c.relname, t.tgname",
            tf("c.relname")
        ),
    )
    .await
    {
        tables.trigger(&r);
    }
    let (routines, sequences, types) = if only.is_some() {
        Default::default()
    } else {
        let routines = opt_rows(
            conn,
            "postgres.routines",
            &format!(
                "SELECT p.proname, CASE p.prokind WHEN 'p' THEN 'procedure' WHEN 'a' THEN 'aggregate' ELSE 'function' END, \
                 pg_get_function_result(p.oid), pg_get_function_arguments(p.oid), l.lanname, \
                 CASE WHEN p.prokind IN ('f','p') THEN pg_get_functiondef(p.oid) END \
                 FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace JOIN pg_language l ON l.oid = p.prolang \
                 WHERE n.nspname = {sch} AND p.prokind IN ('f','p','a') ORDER BY p.proname"
            ),
        )
        .await
        .into_iter()
        .map(|r| RoutineInfo {
            name: s(&r, 0),
            kind: s(&r, 1),
            returns: s(&r, 2),
            arguments: parse_arguments(&s(&r, 3)),
            language: s(&r, 4),
            definition: s(&r, 5).trim().to_string(),
        })
        .collect();
        let sequences = opt_rows(
            conn,
            "postgres.sequences",
            &format!(
                "SELECT sequencename, data_type::text, start_value, increment_by FROM pg_sequences WHERE schemaname = {sch} ORDER BY sequencename"
            ),
        )
        .await
        .into_iter()
        .map(|r| SequenceInfo {
            name: s(&r, 0),
            data_type: s(&r, 1),
            start: num(&r, 2),
            increment: num(&r, 3),
        })
        .collect();
        let types = opt_rows(
            conn,
            "postgres.types",
            &format!(
                "SELECT t.typname, CASE t.typtype WHEN 'e' THEN 'enum' WHEN 'd' THEN 'domain' WHEN 'c' THEN 'composite' ELSE 'type' END, \
                 CASE t.typtype WHEN 'e' THEN (SELECT string_agg(e.enumlabel, chr(10) ORDER BY e.enumsortorder) FROM pg_enum e WHERE e.enumtypid = t.oid) \
                 WHEN 'd' THEN format_type(t.typbasetype, t.typtypmod) \
                 WHEN 'c' THEN (SELECT string_agg(a.attname || ' ' || format_type(a.atttypid, a.atttypmod), ', ' ORDER BY a.attnum) FROM pg_attribute a WHERE a.attrelid = t.typrelid AND a.attnum > 0) END \
                 FROM pg_type t JOIN pg_namespace n ON n.oid = t.typnamespace \
                 WHERE n.nspname = {sch} AND (t.typtype IN ('e','d') OR (t.typtype = 'c' AND (SELECT c.relkind FROM pg_class c WHERE c.oid = t.typrelid) = 'c')) ORDER BY t.typname"
            ),
        )
        .await
        .into_iter()
        .map(|r| {
            let kind = s(&r, 1);
            let body = s(&r, 2);
            TypeInfo {
                name: s(&r, 0),
                labels: if kind == "enum" {
                    body.lines().map(str::to_string).collect()
                } else {
                    Vec::new()
                },
                definition: if kind == "enum" { String::new() } else { body },
                kind,
            }
        })
        .collect();
        (routines, sequences, types)
    };
    Ok(SchemaInfo {
        name: schema.into(),
        tables: tables.finish(),
        routines,
        sequences,
        types,
    })
}

async fn sqlite(conn: &mut Conn, schema: &str, only: Option<&str>) -> DbResult<SchemaInfo> {
    let d = Dialect::Sqlite;
    let sch = d.quote_ident(schema);
    let schl = d.quote_str(schema);
    let filter = only
        .map(|t| format!(" AND tbl_name = {}", d.quote_str(t)))
        .unwrap_or_default();
    let mut tables = Tables::default();
    let master = rows(
        conn,
        "sqlite.master",
        &format!(
            "SELECT type, name, tbl_name, sql FROM {sch}.sqlite_master WHERE type IN ('table','view','trigger') AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'{filter} ORDER BY name"
        ),
    )
    .await?;
    // CHECK constraints and column comments are only in the table's SQL.
    let mut sources = Vec::new();
    for r in master.iter().filter(|r| s(r, 0) != "trigger") {
        let is_view = s(r, 0) == "view";
        let sql = s(r, 3);
        tables.add(TableInfo {
            name: s(r, 1),
            kind: if is_view {
                TableKind::View
            } else {
                TableKind::Table
            },
            definition: if is_view {
                view_query(&sql)
            } else {
                String::new()
            },
            ..Default::default()
        });
        if !is_view {
            sources.push(forge_dbmodel::ddl::DdlFile {
                path: s(r, 1),
                text: sql,
            });
        }
    }
    let parsed = forge_dbmodel::ddl::parse(Dialect::Sqlite, schema, &sources);
    let names = tables.order.clone();
    for name in &names {
        let tl = d.quote_str(name);
        let mut pk: Vec<(i64, String)> = Vec::new();
        let cols = rows(
            conn,
            "sqlite.columns",
            &format!("SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_info({tl}, {schl})"),
        )
        .await?;
        let pk_count = cols.iter().filter(|r| s(r, 4) != "0").count();
        let parsed_table = parsed.schemas.first().and_then(|p| p.table(name)).cloned();
        let mut columns = Vec::new();
        for r in cols {
            let pk_pos: i64 = num(&r, 4).unwrap_or(0);
            let data_type = s(&r, 1);
            if pk_pos > 0 {
                pk.push((pk_pos, s(&r, 0)));
            }
            let comment = parsed_table
                .as_ref()
                .and_then(|t| t.column(&s(&r, 0)).map(|c| c.comment.clone()))
                .unwrap_or_default();
            columns.push(ColumnInfo {
                name: s(&r, 0),
                // INTEGER PRIMARY KEY is an alias of the rowid.
                auto_increment: pk_pos > 0
                    && pk_count == 1
                    && data_type.eq_ignore_ascii_case("INTEGER"),
                data_type,
                nullable: s(&r, 2) == "0" && pk_pos == 0,
                default: o(&r, 3),
                comment,
                ..Default::default()
            });
        }
        pk.sort();
        let t = tables.get(name).expect("table listed");
        t.columns = columns;
        t.primary_key = pk.into_iter().map(|(_, c)| c).collect();
        if let Some(p) = &parsed_table {
            t.checks = p.checks.clone();
        }
        if t.kind == TableKind::View {
            continue;
        }
        for r in rows(
            conn,
            "sqlite.foreign_keys",
            &format!(
                "SELECT id, \"table\", \"from\", \"to\", on_update, on_delete FROM pragma_foreign_key_list({tl}, {schl}) ORDER BY id, seq"
            ),
        )
        .await?
        {
            let mut to = s(&r, 3);
            if to.is_empty() {
                to = "rowid".into();
            }
            let row = vec![
                Some(name.clone()),
                Some(format!("fk_{name}_{}", s(&r, 0))),
                o(&r, 2),
                Some(schema.into()),
                o(&r, 1),
                Some(to),
                o(&r, 4),
                o(&r, 5),
            ];
            tables.fk(&row, same);
        }
        let indexes = rows(
            conn,
            "sqlite.indexes",
            &format!("SELECT name, \"unique\", origin FROM pragma_index_list({tl}, {schl})"),
        )
        .await?;
        for r in indexes {
            let iname = s(&r, 0);
            let cols = rows(
                conn,
                "sqlite.index_columns",
                &format!(
                    "SELECT name FROM pragma_index_info({}, {schl}) ORDER BY seqno",
                    d.quote_str(&iname)
                ),
            )
            .await?;
            for c in cols {
                tables.index(name, &iname, s(&c, 0), flag(&r, 1), s(&r, 2) == "pk");
            }
        }
    }
    for r in master.iter().filter(|r| s(r, 0) == "trigger") {
        let sql = s(r, 3);
        let (timing, events) = forge_dbmodel::ddl::trigger_head(&sql);
        tables.trigger(&[o(r, 2), o(r, 1), Some(timing), Some(events), Some(sql)]);
    }
    Ok(SchemaInfo {
        name: schema.into(),
        tables: tables.finish(),
        ..Default::default()
    })
}

/// The query of `CREATE VIEW v AS SELECT …`.
fn view_query(sql: &str) -> String {
    let upper = sql.to_ascii_uppercase();
    match upper.find(" AS ") {
        Some(i) => sql[i + 4..].trim().to_string(),
        None => sql.trim().to_string(),
    }
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
    for r in rows(
        conn,
        "sqlserver.tables",
        &format!(
            "SELECT o.name, o.type, CAST(ep.value AS nvarchar(4000)), \
             (SELECT SUM(p.rows) FROM sys.partitions p WHERE p.object_id = o.object_id AND p.index_id IN (0, 1)), \
             CASE WHEN o.type = 'V' THEN OBJECT_DEFINITION(o.object_id) END \
             FROM sys.objects o LEFT JOIN sys.extended_properties ep ON ep.major_id = o.object_id AND ep.minor_id = 0 AND ep.name = 'MS_Description' \
             WHERE o.type IN ('U', 'V') AND SCHEMA_NAME(o.schema_id) = {sch}{} ORDER BY o.name",
            tf("o.name")
        ),
    )
    .await?
    {
        let view = s(&r, 1).trim() == "V";
        tables.add(TableInfo {
            name: s(&r, 0),
            kind: if view { TableKind::View } else { TableKind::Table },
            comment: s(&r, 2),
            row_estimate: num(&r, 3),
            definition: s(&r, 4).trim().to_string(),
            ..Default::default()
        });
    }
    for r in rows(
        conn,
        "sqlserver.columns",
        &format!(
            "SELECT c.TABLE_NAME, c.COLUMN_NAME, \
             c.DATA_TYPE + CASE WHEN c.CHARACTER_MAXIMUM_LENGTH = -1 THEN '(max)' WHEN c.CHARACTER_MAXIMUM_LENGTH IS NOT NULL THEN '(' + CAST(c.CHARACTER_MAXIMUM_LENGTH AS varchar(10)) + ')' \
             WHEN c.DATA_TYPE IN ('decimal', 'numeric') THEN '(' + CAST(c.NUMERIC_PRECISION AS varchar(5)) + ',' + CAST(c.NUMERIC_SCALE AS varchar(5)) + ')' ELSE '' END, \
             c.IS_NULLABLE, c.COLUMN_DEFAULT, COLUMNPROPERTY(OBJECT_ID(QUOTENAME(c.TABLE_SCHEMA) + '.' + QUOTENAME(c.TABLE_NAME)), c.COLUMN_NAME, 'IsIdentity'), \
             (SELECT CAST(ep.value AS nvarchar(4000)) FROM sys.extended_properties ep WHERE ep.major_id = OBJECT_ID(QUOTENAME(c.TABLE_SCHEMA) + '.' + QUOTENAME(c.TABLE_NAME)) \
              AND ep.minor_id = COLUMNPROPERTY(OBJECT_ID(QUOTENAME(c.TABLE_SCHEMA) + '.' + QUOTENAME(c.TABLE_NAME)), c.COLUMN_NAME, 'ColumnId') AND ep.name = 'MS_Description'), \
             (SELECT cc.definition FROM sys.computed_columns cc WHERE cc.object_id = OBJECT_ID(QUOTENAME(c.TABLE_SCHEMA) + '.' + QUOTENAME(c.TABLE_NAME)) AND cc.name = c.COLUMN_NAME) \
             FROM INFORMATION_SCHEMA.COLUMNS c WHERE c.TABLE_SCHEMA = {sch}{} ORDER BY c.TABLE_NAME, c.ORDINAL_POSITION",
            tf("c.TABLE_NAME")
        ),
    )
    .await?
    {
        if let Some(t) = tables.get(&s(&r, 0)) {
            t.columns.push(ColumnInfo {
                name: s(&r, 1),
                data_type: s(&r, 2),
                nullable: s(&r, 3) == "YES",
                default: o(&r, 4),
                auto_increment: flag(&r, 5),
                comment: s(&r, 6),
                expression: o(&r, 7),
                ..Default::default()
            });
        }
    }
    for r in rows(
        conn,
        "sqlserver.indexes",
        &format!(
            "SELECT t.name, i.name, i.is_unique, i.is_primary_key, c.name FROM sys.indexes i \
             JOIN sys.tables t ON t.object_id = i.object_id \
             JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id \
             JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id \
             WHERE i.name IS NOT NULL AND ic.is_included_column = 0 AND SCHEMA_NAME(t.schema_id) = {sch}{} \
             ORDER BY t.name, i.name, ic.key_ordinal",
            tf("t.name")
        ),
    )
    .await?
    {
        tables.index(&s(&r, 0), &s(&r, 1), s(&r, 4), flag(&r, 2), flag(&r, 3));
    }
    for r in rows(
        conn,
        "sqlserver.foreign_keys",
        &format!(
            "SELECT tp.name, fk.name, cp.name, SCHEMA_NAME(tr.schema_id), tr.name, cr.name, fk.update_referential_action_desc, fk.delete_referential_action_desc \
             FROM sys.foreign_keys fk JOIN sys.foreign_key_columns fkc ON fkc.constraint_object_id = fk.object_id \
             JOIN sys.tables tp ON tp.object_id = fk.parent_object_id JOIN sys.columns cp ON cp.object_id = fkc.parent_object_id AND cp.column_id = fkc.parent_column_id \
             JOIN sys.tables tr ON tr.object_id = fk.referenced_object_id JOIN sys.columns cr ON cr.object_id = fkc.referenced_object_id AND cr.column_id = fkc.referenced_column_id \
             WHERE SCHEMA_NAME(tp.schema_id) = {sch}{} ORDER BY tp.name, fk.name, fkc.constraint_column_id",
            tf("tp.name")
        ),
    )
    .await?
    {
        tables.fk(&r, sqlserver_rule);
    }
    for r in opt_rows(
        conn,
        "sqlserver.checks",
        &format!(
            "SELECT t.name, cc.name, cc.definition FROM sys.check_constraints cc JOIN sys.tables t ON t.object_id = cc.parent_object_id \
             WHERE SCHEMA_NAME(t.schema_id) = {sch}{} ORDER BY t.name, cc.name",
            tf("t.name")
        ),
    )
    .await
    {
        tables.check(&r);
    }
    for r in opt_rows(
        conn,
        "sqlserver.triggers",
        &format!(
            "SELECT o.name, tr.name, CASE WHEN tr.is_instead_of_trigger = 1 THEN 'INSTEAD OF' ELSE 'AFTER' END, \
             (SELECT STRING_AGG(te.type_desc, ' OR ') FROM sys.trigger_events te WHERE te.object_id = tr.object_id), OBJECT_DEFINITION(tr.object_id) \
             FROM sys.triggers tr JOIN sys.objects o ON o.object_id = tr.parent_id \
             WHERE tr.parent_class = 1 AND SCHEMA_NAME(o.schema_id) = {sch}{} ORDER BY o.name, tr.name",
            tf("o.name")
        ),
    )
    .await
    {
        tables.trigger(&r);
    }
    let (routines, sequences, types) = if only.is_some() {
        Default::default()
    } else {
        let list = opt_rows(
            conn,
            "sqlserver.routines",
            &format!(
                "SELECT ROUTINE_NAME, ROUTINE_TYPE, DATA_TYPE, OBJECT_DEFINITION(OBJECT_ID(QUOTENAME(ROUTINE_SCHEMA) + '.' + QUOTENAME(ROUTINE_NAME))), ROUTINE_BODY \
                 FROM INFORMATION_SCHEMA.ROUTINES WHERE ROUTINE_SCHEMA = {sch} ORDER BY ROUTINE_NAME"
            ),
        )
        .await;
        let args = opt_rows(
            conn,
            "sqlserver.parameters",
            &format!(
                "SELECT SPECIFIC_NAME, PARAMETER_NAME, DATA_TYPE, PARAMETER_MODE FROM INFORMATION_SCHEMA.PARAMETERS \
                 WHERE SPECIFIC_SCHEMA = {sch} AND ORDINAL_POSITION > 0 ORDER BY SPECIFIC_NAME, ORDINAL_POSITION"
            ),
        )
        .await;
        let sequences = opt_rows(
            conn,
            "sqlserver.sequences",
            &format!(
                "SELECT name, TYPE_NAME(user_type_id), CAST(start_value AS bigint), CAST(increment AS bigint) FROM sys.sequences WHERE SCHEMA_NAME(schema_id) = {sch} ORDER BY name"
            ),
        )
        .await
        .into_iter()
        .map(|r| SequenceInfo {
            name: s(&r, 0),
            data_type: s(&r, 1),
            start: num(&r, 2),
            increment: num(&r, 3),
        })
        .collect();
        let types = opt_rows(
            conn,
            "sqlserver.types",
            &format!(
                "SELECT t.name, CASE WHEN t.is_table_type = 1 THEN 'type' ELSE 'domain' END, TYPE_NAME(t.system_type_id) \
                 FROM sys.types t WHERE t.is_user_defined = 1 AND SCHEMA_NAME(t.schema_id) = {sch} ORDER BY t.name"
            ),
        )
        .await
        .into_iter()
        .map(|r| TypeInfo {
            name: s(&r, 0),
            kind: s(&r, 1),
            labels: Vec::new(),
            definition: s(&r, 2),
        })
        .collect();
        (self::routines(list, args), sequences, types)
    };
    Ok(SchemaInfo {
        name: schema.into(),
        tables: tables.finish(),
        routines,
        sequences,
        types,
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
    for r in rows(
        conn,
        "clickhouse.tables",
        &format!(
            "SELECT name, engine, comment, total_rows, as_select FROM system.tables WHERE database = {sch}{} ORDER BY name",
            tf("name")
        ),
    )
    .await?
    {
        let engine = s(&r, 1);
        tables.add(TableInfo {
            name: s(&r, 0),
            kind: if engine == "MaterializedView" {
                TableKind::MaterializedView
            } else if engine.contains("View") {
                TableKind::View
            } else {
                TableKind::Table
            },
            comment: s(&r, 2),
            row_estimate: num(&r, 3),
            definition: s(&r, 4),
            ..Default::default()
        });
    }
    for r in rows(
        conn,
        "clickhouse.columns",
        &format!(
            "SELECT table, name, type, default_kind, default_expression, comment, is_in_primary_key FROM system.columns WHERE database = {sch}{} ORDER BY table, position",
            tf("table")
        ),
    )
    .await?
    {
        let col = s(&r, 1);
        let pk = flag(&r, 6);
        if let Some(t) = tables.get(&s(&r, 0)) {
            let data_type = s(&r, 2);
            let default = o(&r, 4).filter(|d| !d.is_empty());
            let kind = s(&r, 3);
            let computed = matches!(kind.as_str(), "MATERIALIZED" | "ALIAS");
            t.columns.push(ColumnInfo {
                nullable: data_type.starts_with("Nullable("),
                data_type,
                expression: if computed { default.clone() } else { None },
                default: if computed {
                    None
                } else {
                    default.map(|d| match kind.as_str() {
                        "" | "DEFAULT" => d,
                        k => format!("{k} {d}"),
                    })
                },
                comment: s(&r, 5),
                name: col.clone(),
                ..Default::default()
            });
            if pk {
                t.primary_key.push(col);
            }
        }
    }
    for r in opt_rows(
        conn,
        "clickhouse.indexes",
        &format!(
            "SELECT table, name, expr FROM system.data_skipping_indices WHERE database = {sch}{} ORDER BY table, name",
            tf("table")
        ),
    )
    .await
    {
        tables.index(&s(&r, 0), &s(&r, 1), s(&r, 2), false, false);
    }
    Ok(SchemaInfo {
        name: schema.into(),
        tables: tables.finish(),
        ..Default::default()
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
    // A materialized view also has a table of the same name: the view wins.
    for r in rows(
        conn,
        "oracle.tables",
        &format!(
            "SELECT o.object_name, o.object_type, c.comments, t.num_rows FROM all_objects o \
             LEFT JOIN all_tab_comments c ON c.owner = o.owner AND c.table_name = o.object_name \
             LEFT JOIN all_tables t ON t.owner = o.owner AND t.table_name = o.object_name \
             WHERE o.owner = {sch} AND o.object_type IN ('TABLE', 'VIEW', 'MATERIALIZED VIEW') AND o.object_name NOT LIKE 'BIN$%'{} \
             ORDER BY o.object_name, CASE o.object_type WHEN 'TABLE' THEN 0 ELSE 1 END",
            tf("o.object_name")
        ),
    )
    .await?
    {
        tables.add(TableInfo {
            name: s(&r, 0),
            kind: match s(&r, 1).as_str() {
                "VIEW" => TableKind::View,
                "MATERIALIZED VIEW" => TableKind::MaterializedView,
                _ => TableKind::Table,
            },
            comment: s(&r, 2),
            row_estimate: num(&r, 3),
            ..Default::default()
        });
    }
    for r in rows(
        conn,
        "oracle.columns",
        &format!(
            "SELECT c.table_name, c.column_name, c.data_type || CASE WHEN c.data_type IN ('VARCHAR2', 'NVARCHAR2', 'CHAR', 'NCHAR', 'RAW') THEN '(' || c.char_length || ')' \
             WHEN c.data_type = 'NUMBER' AND c.data_precision IS NOT NULL THEN '(' || c.data_precision || ',' || c.data_scale || ')' ELSE '' END, \
             c.nullable, c.data_default, c.identity_column, cm.comments FROM all_tab_columns c \
             LEFT JOIN all_col_comments cm ON cm.owner = c.owner AND cm.table_name = c.table_name AND cm.column_name = c.column_name \
             WHERE c.owner = {sch}{} ORDER BY c.table_name, c.column_id",
            tf("c.table_name")
        ),
    )
    .await?
    {
        if let Some(t) = tables.get(&s(&r, 0)) {
            t.columns.push(ColumnInfo {
                name: s(&r, 1),
                data_type: s(&r, 2),
                nullable: s(&r, 3) == "Y",
                default: o(&r, 4)
                    .map(|d| d.trim().to_string())
                    .filter(|d| !d.is_empty()),
                auto_increment: s(&r, 5) == "YES",
                comment: s(&r, 6),
                ..Default::default()
            });
        }
    }
    for r in rows(
        conn,
        "oracle.indexes",
        &format!(
            "SELECT i.table_name, i.index_name, i.uniqueness, CASE WHEN c.constraint_type = 'P' THEN 'Y' ELSE 'N' END, ic.column_name \
             FROM all_indexes i JOIN all_ind_columns ic ON ic.index_owner = i.owner AND ic.index_name = i.index_name \
             LEFT JOIN all_constraints c ON c.owner = i.owner AND c.index_name = i.index_name AND c.constraint_type = 'P' \
             WHERE i.table_owner = {sch}{} ORDER BY i.table_name, i.index_name, ic.column_position",
            tf("i.table_name")
        ),
    )
    .await?
    {
        tables.index(&s(&r, 0), &s(&r, 1), s(&r, 4), s(&r, 2) == "UNIQUE", s(&r, 3) == "Y");
    }
    for r in rows(
        conn,
        "oracle.foreign_keys",
        &format!(
            "SELECT c.table_name, c.constraint_name, cc.column_name, r.owner, r.table_name, rc.column_name, 'NO ACTION', c.delete_rule \
             FROM all_constraints c JOIN all_cons_columns cc ON cc.owner = c.owner AND cc.constraint_name = c.constraint_name \
             JOIN all_constraints r ON r.owner = c.r_owner AND r.constraint_name = c.r_constraint_name \
             JOIN all_cons_columns rc ON rc.owner = r.owner AND rc.constraint_name = r.constraint_name AND rc.position = cc.position \
             WHERE c.constraint_type = 'R' AND c.owner = {sch}{} ORDER BY c.table_name, c.constraint_name, cc.position",
            tf("c.table_name")
        ),
    )
    .await?
    {
        tables.fk(&r, same);
    }
    for r in opt_rows(
        conn,
        "oracle.checks",
        &format!(
            "SELECT table_name, constraint_name, search_condition_vc FROM all_constraints \
             WHERE owner = {sch} AND constraint_type = 'C' AND generated = 'USER NAME'{} ORDER BY table_name, constraint_name",
            tf("table_name")
        ),
    )
    .await
    {
        tables.check(&r);
    }
    for r in opt_rows(
        conn,
        "oracle.triggers",
        &format!(
            "SELECT table_name, trigger_name, trigger_type, triggering_event, description FROM all_triggers \
             WHERE owner = {sch} AND base_object_type IN ('TABLE', 'VIEW'){} ORDER BY table_name, trigger_name",
            tf("table_name")
        ),
    )
    .await
    {
        let ty = s(&r, 2).to_uppercase();
        let timing = ["INSTEAD OF", "BEFORE", "AFTER"]
            .into_iter()
            .find(|t| ty.starts_with(t))
            .unwrap_or("")
            .to_string();
        tables.trigger(&[o(&r, 0), o(&r, 1), Some(timing), o(&r, 3), o(&r, 4)]);
    }
    let (routines, sequences, types) = if only.is_some() {
        Default::default()
    } else {
        let mut routines: Vec<RoutineInfo> = opt_rows(
            conn,
            "oracle.routines",
            &format!(
                "SELECT object_name, LOWER(object_type) FROM all_objects WHERE owner = {sch} AND object_type IN ('PROCEDURE', 'FUNCTION', 'PACKAGE') ORDER BY object_name"
            ),
        )
        .await
        .into_iter()
        .map(|r| RoutineInfo {
            name: s(&r, 0),
            kind: s(&r, 1),
            ..Default::default()
        })
        .collect();
        for a in opt_rows(
            conn,
            "oracle.arguments",
            &format!(
                "SELECT object_name, argument_name, data_type, in_out, position FROM all_arguments \
                 WHERE owner = {sch} AND package_name IS NULL AND data_level = 0 ORDER BY object_name, position"
            ),
        )
        .await
        {
            let Some(r) = routines.iter_mut().find(|r| r.name == s(&a, 0)) else {
                continue;
            };
            if num(&a, 4) == Some(0) {
                r.returns = s(&a, 2);
                continue;
            }
            let mode = s(&a, 3).replace('/', "");
            r.arguments.push(RoutineArg {
                name: s(&a, 1),
                data_type: s(&a, 2),
                mode: if mode == "IN" { String::new() } else { mode },
            });
        }
        let sequences = opt_rows(
            conn,
            "oracle.sequences",
            &format!(
                "SELECT sequence_name, min_value, increment_by FROM all_sequences WHERE sequence_owner = {sch} ORDER BY sequence_name"
            ),
        )
        .await
        .into_iter()
        .map(|r| SequenceInfo {
            name: s(&r, 0),
            data_type: "NUMBER".into(),
            start: num(&r, 1),
            increment: num(&r, 2),
        })
        .collect();
        let types = opt_rows(
            conn,
            "oracle.types",
            &format!(
                "SELECT type_name, typecode FROM all_types WHERE owner = {sch} ORDER BY type_name"
            ),
        )
        .await
        .into_iter()
        .map(|r| TypeInfo {
            name: s(&r, 0),
            kind: if s(&r, 1) == "OBJECT" {
                "composite".into()
            } else {
                "type".into()
            },
            labels: Vec::new(),
            definition: s(&r, 1),
        })
        .collect();
        (routines, sequences, types)
    };
    Ok(SchemaInfo {
        name: schema.into(),
        tables: tables.finish(),
        routines,
        sequences,
        types,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_queries() {
        let q = tagged("mysql.tables", "SELECT 1");
        assert_eq!(q, "/* forge:mysql.tables */ SELECT 1");
        assert_eq!(tag_of(&q), Some("mysql.tables"));
        assert_eq!(tag_of("SELECT 1"), None);
        assert_eq!(forge_sql::leading_keyword(&q), "SELECT");
    }

    #[test]
    fn parses_postgres_arguments() {
        let a = parse_arguments(
            "o_id bigint, OUT total numeric(10,2), character varying, VARIADIC tags text[] DEFAULT '{}'::text[]",
        );
        assert_eq!(a.len(), 4);
        assert_eq!(
            (a[0].name.as_str(), a[0].data_type.as_str()),
            ("o_id", "bigint")
        );
        assert_eq!(a[1].mode, "OUT");
        assert_eq!(a[1].data_type, "numeric(10,2)");
        assert_eq!(
            (a[2].name.as_str(), a[2].data_type.as_str()),
            ("", "character varying")
        );
        assert_eq!(
            (
                a[3].mode.as_str(),
                a[3].name.as_str(),
                a[3].data_type.as_str()
            ),
            ("VARIADIC", "tags", "text[]")
        );
        assert!(parse_arguments("").is_empty());
    }
}
