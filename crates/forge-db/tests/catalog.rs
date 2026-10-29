//! Introspection of MySQL, PostgreSQL, SQL Server, Oracle and ClickHouse against recorded
//! catalog rows (`tests/fixtures/catalog/<dialect>.json`, keyed by the `/* forge:… */` tag
//! of each catalog query). Every recorded query must be asked, so a renamed or dropped
//! query fails the test.

use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use forge_db::conn::{Conn, RawResult};
use forge_db::engine::EngineConn;
use forge_db::introspect::{self, tag_of};
use forge_db::model::{apply_patterns, generate, ObjectRef};
use forge_db::{DbError, DbResult, Dialect, SchemaInfo, TableKind};
use futures_util::future::BoxFuture;
use serde_json::Value;

struct Recorded {
    dialect: Dialect,
    rows: BTreeMap<String, Vec<Vec<Value>>>,
    asked: Arc<Mutex<BTreeSet<String>>>,
}

impl EngineConn for Recorded {
    fn dialect(&self) -> Dialect {
        self.dialect
    }

    fn run<'a>(
        &'a mut self,
        sql: &'a str,
        _skip: usize,
        _max: usize,
    ) -> BoxFuture<'a, DbResult<RawResult>> {
        Box::pin(async move {
            let tag = tag_of(sql)
                .ok_or_else(|| DbError::Sql(format!("untagged catalog query: {sql}")))?;
            self.asked.lock().unwrap().insert(tag.to_string());
            match self.rows.get(tag) {
                Some(rows) => Ok(RawResult {
                    rows: rows.clone(),
                    ..Default::default()
                }),
                None => Err(DbError::Sql(format!("no recorded rows for {tag}"))),
            }
        })
    }

    fn server_version(&mut self) -> BoxFuture<'_, DbResult<String>> {
        Box::pin(async { Ok("recorded".into()) })
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

struct Loaded {
    schema: SchemaInfo,
    schemas: Vec<forge_db::SchemaName>,
    server: introspect::ServerObjects,
}

async fn load(dialect: Dialect, file: &str, schema: &str) -> Loaded {
    let path = format!(
        "{}/tests/fixtures/catalog/{file}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let rows: BTreeMap<String, Vec<Vec<Value>>> =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let recorded: BTreeSet<String> = rows.keys().cloned().collect();
    let asked = Arc::new(Mutex::new(BTreeSet::new()));
    let mut conn = Conn::Engine(Box::new(Recorded {
        dialect,
        rows,
        asked: asked.clone(),
    }));
    let schemas = introspect::schemas(&mut conn).await.unwrap();
    let server = introspect::server_objects(&mut conn).await;
    let info = introspect::schema(&mut conn, schema, None).await.unwrap();
    let asked = asked.lock().unwrap().clone();
    let unused: Vec<_> = recorded.difference(&asked).collect();
    assert!(unused.is_empty(), "recorded but never asked: {unused:?}");
    Loaded {
        schema: info,
        schemas,
        server,
    }
}

#[tokio::test]
async fn mysql_full_object_tree() {
    let l = load(Dialect::MySql, "mysql", "shop").await;
    assert!(l.schemas.iter().any(|s| s.name == "shop" && s.current));
    let s = &l.schema;
    let names: Vec<_> = s.tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["customer_totals", "orders", "users"]);
    let orders = s.table("orders").unwrap();
    assert_eq!(orders.comment, "Customer orders");
    assert_eq!(orders.row_estimate, Some(3));
    assert_eq!(orders.primary_key, ["id"]);
    assert!(orders.columns[0].auto_increment);
    assert_eq!(orders.column("total").unwrap().comment, "Gross amount");
    assert_eq!(
        orders
            .column("total_with_tax")
            .unwrap()
            .expression
            .as_deref(),
        Some("(`total` * 1.19)")
    );
    let fk = &orders.foreign_keys[0];
    assert_eq!(
        (fk.ref_table.as_str(), fk.on_delete.as_str()),
        ("users", "CASCADE")
    );
    assert_eq!(orders.checks[0].name, "orders_total_positive");
    assert_eq!(orders.triggers[0].timing, "AFTER");
    assert_eq!(orders.triggers[0].events, "INSERT");
    let view = s.table("customer_totals").unwrap();
    assert_eq!(view.kind, TableKind::View);
    assert!(view.definition.starts_with("select"));
    let users = s.table("users").unwrap();
    assert!(users
        .indexes
        .iter()
        .any(|i| i.unique && i.name == "users_email_unique"));
    let f = s.routines.iter().find(|r| r.name == "order_total").unwrap();
    assert_eq!(f.kind, "function");
    assert_eq!(f.returns, "decimal(10,2)");
    assert_eq!(f.signature(), "(o_id bigint)");
    let p = s
        .routines
        .iter()
        .find(|r| r.name == "archive_orders")
        .unwrap();
    assert_eq!(
        (p.kind.as_str(), p.arguments[0].name.as_str()),
        ("procedure", "before_date")
    );
    assert_eq!(l.server.roles.len(), 2);
    assert!(l.server.roles[0].superuser);
    assert!(l.server.catalog.is_empty());
    let ddl = generate::table(Dialect::MySql, "shop", orders);
    assert!(
        ddl.contains("CONSTRAINT `orders_total_positive` CHECK (`total` >= 0)"),
        "{ddl}"
    );
}

#[tokio::test]
async fn postgres_full_object_tree() {
    let l = load(Dialect::Postgres, "postgres", "public").await;
    let s = &l.schema;
    assert_eq!(
        s.table("sales_daily").unwrap().kind,
        TableKind::MaterializedView
    );
    assert!(s
        .table("order_totals")
        .unwrap()
        .definition
        .contains("GROUP BY o.user_id"));
    let orders = s.table("orders").unwrap();
    assert_eq!(orders.row_estimate, Some(1200));
    assert!(orders.columns[0].auto_increment, "identity column");
    let gross = orders.column("gross").unwrap();
    assert_eq!(gross.expression.as_deref(), Some("(total * 1.19)"));
    assert_eq!(gross.default, None);
    assert_eq!(orders.checks[0].expression, "(total >= 0::numeric)");
    assert_eq!(orders.foreign_keys[0].on_update, "");
    assert_eq!(orders.foreign_keys[0].on_delete, "CASCADE");
    assert!(orders.indexes.iter().any(|i| i.columns == ["<expression>"]));
    assert_eq!(orders.triggers[0].name, "orders_touch");
    let users = s.table("users").unwrap();
    assert_eq!(users.row_estimate, None);
    assert!(users.columns[0].auto_increment, "serial column");
    let f = s.routines.iter().find(|r| r.name == "order_total").unwrap();
    assert_eq!(
        (f.returns.as_str(), f.language.as_str()),
        ("numeric", "sql")
    );
    assert_eq!(f.signature(), "(o_id bigint)");
    assert!(f.definition.starts_with("CREATE OR REPLACE FUNCTION"));
    let p = s
        .routines
        .iter()
        .find(|r| r.name == "close_stale_orders")
        .unwrap();
    assert_eq!(p.arguments[0].data_type, "integer");
    assert_eq!(p.arguments[0].mode, "");
    assert_eq!(
        s.routines
            .iter()
            .find(|r| r.name == "total_spent")
            .unwrap()
            .kind,
        "aggregate"
    );
    assert_eq!(s.sequences[0].name, "users_id_seq");
    let status = s.types.iter().find(|t| t.name == "order_status").unwrap();
    assert_eq!(status.labels, ["new", "paid", "shipped"]);
    assert_eq!(
        s.types
            .iter()
            .find(|t| t.name == "email")
            .unwrap()
            .definition,
        "character varying(255)"
    );
    assert_eq!(l.server.catalog, "shop");
    assert_eq!(l.server.catalogs, ["postgres", "shop", "shop_test"]);
    let kinds: Vec<_> = l.server.roles.iter().map(|r| r.kind.as_str()).collect();
    assert_eq!(kinds, ["user", "role", "user"]);
    // The schema filter over the recorded schemas.
    let mut schemas = l.schemas.clone();
    apply_patterns(&["tenant_*", ":current"], &mut schemas).unwrap();
    let visible: Vec<_> = schemas
        .iter()
        .filter(|s| s.visible)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(visible, ["public", "tenant_1", "tenant_2"]);
    let doc = forge_db::model::document(
        Dialect::Postgres,
        s,
        &ObjectRef {
            kind: "type".into(),
            table: String::new(),
            name: "order_status".into(),
        },
    )
    .unwrap();
    assert_eq!(
        doc.ddl,
        "CREATE TYPE \"public\".\"order_status\" AS ENUM ('new', 'paid', 'shipped');\n"
    );
}

#[tokio::test]
async fn sqlserver_full_object_tree() {
    let l = load(Dialect::SqlServer, "sqlserver", "dbo").await;
    let s = &l.schema;
    let c = s.table("customers").unwrap();
    assert_eq!(c.comment, "Customers");
    assert!(c.columns[0].auto_increment);
    assert_eq!(c.column("name").unwrap().comment, "Display name");
    assert_eq!(
        c.column("name_upper").unwrap().expression.as_deref(),
        Some("(upper([name]))")
    );
    assert_eq!(c.primary_key, ["id"]);
    assert_eq!(c.checks[0].name, "CK_customers_name");
    assert_eq!(c.triggers[0].events, "INSERT OR UPDATE");
    assert!(s
        .table("v_customers")
        .unwrap()
        .definition
        .starts_with("CREATE VIEW"));
    let p = s.routines.iter().find(|r| r.name == "usp_archive").unwrap();
    assert_eq!(p.signature(), "(@before date, INOUT @moved int)");
    assert_eq!(s.sequences[0].start, Some(1000));
    assert_eq!(s.types[0].kind, "domain");
    assert_eq!(l.server.catalog, "shop");
    let sa = l.server.roles.iter().find(|r| r.name == "sa").unwrap();
    assert!(sa.superuser && sa.can_login && sa.kind == "user");
    assert_eq!(
        l.server
            .roles
            .iter()
            .find(|r| r.name == "sysadmin")
            .unwrap()
            .kind,
        "role"
    );
}

#[tokio::test]
async fn oracle_full_object_tree() {
    let l = load(Dialect::Oracle, "oracle", "SHOP").await;
    let s = &l.schema;
    let names: Vec<_> = s.tables.iter().map(|t| (t.name.as_str(), t.kind)).collect();
    assert_eq!(
        names,
        [
            ("CUSTOMERS", TableKind::Table),
            ("MV_SALES", TableKind::MaterializedView),
            ("V_CUST", TableKind::View)
        ]
    );
    let c = s.table("CUSTOMERS").unwrap();
    assert_eq!(c.column("NAME").unwrap().default.as_deref(), Some("'n/a'"));
    assert_eq!(c.column("NAME").unwrap().comment, "Full name");
    assert_eq!(c.primary_key, ["ID"]);
    assert_eq!(c.triggers[0].timing, "BEFORE");
    assert_eq!(c.checks.len(), 1);
    let f = s.routines.iter().find(|r| r.name == "TOTAL_FOR").unwrap();
    assert_eq!(f.returns, "NUMBER");
    assert_eq!(f.signature(), "(P_ID NUMBER, INOUT P_TAX NUMBER)");
    assert_eq!(s.sequences[0].increment, Some(1));
    assert_eq!(s.types[0].kind, "composite");
    assert_eq!(l.server.catalog, "FREEPDB1");
}

#[tokio::test]
async fn clickhouse_full_object_tree() {
    let l = load(Dialect::ClickHouse, "clickhouse", "shop").await;
    let s = &l.schema;
    assert_eq!(
        s.table("daily_mv").unwrap().kind,
        TableKind::MaterializedView
    );
    assert_eq!(s.table("v_users").unwrap().kind, TableKind::View);
    let e = s.table("events").unwrap();
    assert_eq!(e.primary_key, ["id"]);
    assert!(e.column("user").unwrap().nullable);
    assert_eq!(
        e.column("day").unwrap().expression.as_deref(),
        Some("toDate(at)")
    );
    assert_eq!(e.column("at").unwrap().default.as_deref(), Some("now()"));
    assert_eq!(e.indexes[0].columns, ["user"]);
    assert_eq!(l.server.roles.len(), 2);
}
