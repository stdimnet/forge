//! End-to-end tests of `DbService` against a SQLite file in a temporary project.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use forge_db::{
    ChangeKind, DataSource, DbService, Driver, MemoryStore, RowChange, Scope, SecretStore, SortKey,
    TableDataRequest, TableKind,
};
use serde_json::{json, Value};

const SCHEMA: &str = "
CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE,
    balance REAL,
    avatar BLOB,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE orders (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    total NUMERIC NOT NULL
);
CREATE INDEX orders_user_id_index ON orders(user_id);
CREATE TABLE audit_log (message TEXT);
CREATE VIEW big_spenders AS SELECT u.name, SUM(o.total) AS spent FROM users u JOIN orders o ON o.user_id = u.id GROUP BY u.name;
INSERT INTO users (name, email, balance, avatar) VALUES
  ('Ada Lovelace', 'ada@example.com', 12.5, X'89504E47'),
  ('Alan Turing', 'alan@example.com', NULL, NULL),
  ('Grace Hopper', 'grace@example.com', 99.0, NULL);
INSERT INTO orders (user_id, total) VALUES (1, 19.99), (1, 5), (3, 120);
INSERT INTO audit_log VALUES ('created');
";

struct Fixture {
    _dir: tempfile::TempDir,
    svc: DbService,
    id: String,
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let svc = DbService::new(None, Arc::new(MemoryStore::default()));
    svc.set_project(Some(dir.path().to_path_buf()))
        .await
        .unwrap();
    let info = svc
        .save(
            DataSource {
                name: "app".into(),
                driver: Driver::Sqlite,
                file: "var/app.db".into(),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
    std::fs::create_dir_all(dir.path().join("var")).unwrap();
    let id = info.source.id.clone();
    let session = svc.open_session(&id).await.unwrap();
    let res = svc.execute(&session.id, SCHEMA, None).await.unwrap();
    assert!(res.iter().all(|r| r.error.is_none()), "{res:?}");
    svc.close_session(&session.id).await.unwrap();
    Fixture { _dir: dir, svc, id }
}

#[tokio::test]
async fn stores_data_sources_in_project_file() {
    let f = fixture().await;
    let text = std::fs::read_to_string(f._dir.path().join(".forge/dataSources.toml")).unwrap();
    assert!(text.contains("driver = \"sqlite\""));
    assert!(text.contains("file = \"var/app.db\""));
    let list = f.svc.list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].scope, Scope::Project);
    let t = f.svc.test(&list[0].source, None).await;
    assert!(t.ok, "{:?}", t.error);
    assert!(t.server_version.unwrap().starts_with('3'));
}

#[tokio::test]
async fn passwords_go_to_the_secret_store() {
    let store = Arc::new(MemoryStore::default());
    let dir = tempfile::tempdir().unwrap();
    let svc = DbService::new(None, store.clone());
    svc.set_project(Some(dir.path().to_path_buf()))
        .await
        .unwrap();
    let info = svc
        .save(
            DataSource {
                name: "shop".into(),
                driver: Driver::Postgres,
                host: "localhost".into(),
                database: "shop".into(),
                ..Default::default()
            },
            Some("s3cret".into()),
        )
        .await
        .unwrap();
    assert!(info.has_password);
    assert_eq!(
        store.get(&info.source.id).unwrap().as_deref(),
        Some("s3cret")
    );
    let text = std::fs::read_to_string(dir.path().join(".forge/dataSources.toml")).unwrap();
    assert!(!text.contains("s3cret"));
    // Saving again without a password keeps it.
    svc.save(info.source.clone(), None).await.unwrap();
    assert_eq!(
        store.get(&info.source.id).unwrap().as_deref(),
        Some("s3cret")
    );
    svc.delete(&info.source.id).await.unwrap();
    assert_eq!(store.get(&info.source.id).unwrap(), None);
    assert!(!dir.path().join(".forge/dataSources.toml").exists());
}

#[tokio::test]
async fn introspects_tables_views_keys_and_indexes() {
    let f = fixture().await;
    let schemas = f.svc.schemas(&f.id).await.unwrap();
    assert_eq!(schemas[0].name, "main");
    assert!(schemas[0].current);
    let s = f.svc.introspect(&f.id, "main").await.unwrap();
    let names: Vec<_> = s.tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["audit_log", "big_spenders", "orders", "users"]);
    let users = s.tables.iter().find(|t| t.name == "users").unwrap();
    assert_eq!(users.primary_key, ["id"]);
    assert!(users.columns[0].auto_increment);
    assert!(!users.columns[1].nullable);
    assert!(users.columns[3].nullable);
    assert!(users
        .indexes
        .iter()
        .any(|i| i.unique && i.columns == ["email"]));
    let orders = s.tables.iter().find(|t| t.name == "orders").unwrap();
    assert_eq!(orders.foreign_keys[0].ref_table, "users");
    assert_eq!(orders.foreign_keys[0].columns, ["user_id"]);
    assert_eq!(orders.foreign_keys[0].on_delete, "CASCADE");
    assert!(orders
        .indexes
        .iter()
        .any(|i| i.name == "orders_user_id_index" && !i.unique));
    let view = s.tables.iter().find(|t| t.name == "big_spenders").unwrap();
    assert_eq!(view.kind, TableKind::View);
    assert_eq!(view.columns.len(), 2);
    let ddl = f.svc.table_ddl(&f.id, "main", "orders").await.unwrap();
    assert!(ddl.starts_with("CREATE TABLE orders"));
    assert!(ddl.contains("CREATE INDEX orders_user_id_index"));
}

#[tokio::test]
async fn executes_scripts_with_typed_values_and_timing() {
    let f = fixture().await;
    let s = f.svc.open_session(&f.id).await.unwrap();
    let res = f
        .svc
        .execute(
            &s.id,
            "SELECT id, name, balance, avatar FROM users ORDER BY id; UPDATE users SET balance = 0 WHERE balance IS NULL; SELECT 9007199254740993 AS big, NULL AS missing; SELECT * FROM users WHERE 1 = 0",
            None,
        )
        .await
        .unwrap();
    assert_eq!(res.len(), 4, "{res:#?}");
    let q = &res[0];
    assert!(q.returns_rows);
    assert_eq!(
        q.result
            .columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["id", "name", "balance", "avatar"]
    );
    assert_eq!(
        q.result.rows[0],
        vec![
            json!(1),
            json!("Ada Lovelace"),
            json!(12.5),
            json!({"$hex": "89504e47"})
        ]
    );
    assert_eq!(q.result.rows[1][2], Value::Null);
    assert!(!res[1].returns_rows);
    assert_eq!(res[1].result.rows_affected, 1);
    assert_eq!(
        res[2].result.rows[0],
        vec![json!("9007199254740993"), Value::Null]
    );
    // Empty result still has its columns.
    assert!(res[3].result.rows.is_empty());
    assert_eq!(res[3].result.columns.len(), 6);
}

#[tokio::test]
async fn stops_at_first_error() {
    let f = fixture().await;
    let s = f.svc.open_session(&f.id).await.unwrap();
    let res = f
        .svc
        .execute(&s.id, "SELECT 1; SELECT * FROM nope; SELECT 2", None)
        .await
        .unwrap();
    assert_eq!(res.len(), 2);
    assert!(res[1].error.as_deref().unwrap().contains("no such table"));
}

#[tokio::test]
async fn pages_console_results() {
    let f = fixture().await;
    let s = f.svc.open_session(&f.id).await.unwrap();
    let sql = "SELECT name FROM users ORDER BY id";
    let res = f.svc.execute(&s.id, sql, Some(2)).await.unwrap();
    assert_eq!(res[0].result.rows.len(), 2);
    assert!(res[0].result.has_more);
    let page = f.svc.fetch(&s.id, sql, 2, Some(2)).await.unwrap();
    assert_eq!(page.rows, vec![vec![json!("Grace Hopper")]]);
    assert!(!page.has_more);
    // Statements that cannot be wrapped fall back to skipping rows.
    let page = f
        .svc
        .fetch(&s.id, "PRAGMA table_info(users)", 4, Some(10))
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 2);
}

async fn scalar(svc: &DbService, session: &str, sql: &str) -> Value {
    svc.execute(session, sql, None).await.unwrap()[0]
        .result
        .rows[0][0]
        .clone()
}

#[tokio::test]
async fn manual_transactions_commit_and_rollback() {
    let f = fixture().await;
    let s = f.svc.open_session(&f.id).await.unwrap();
    assert!(s.auto_commit);
    let info = f.svc.set_auto_commit(&s.id, false).await.unwrap();
    assert!(!info.auto_commit);
    f.svc
        .execute(&s.id, "DELETE FROM audit_log", None)
        .await
        .unwrap();
    assert!(f.svc.session(&s.id).unwrap().info().tx_open);
    let info = f.svc.rollback(&s.id).await.unwrap();
    assert!(!info.tx_open);
    let count = "SELECT COUNT(*) FROM audit_log";
    assert_eq!(scalar(&f.svc, &s.id, count).await, json!(1));
    f.svc.rollback(&s.id).await.unwrap();
    f.svc
        .execute(&s.id, "DELETE FROM audit_log", None)
        .await
        .unwrap();
    f.svc.commit(&s.id).await.unwrap();
    // Another connection sees the committed delete.
    let other = f.svc.open_session(&f.id).await.unwrap();
    assert_eq!(scalar(&f.svc, &other.id, count).await, json!(0));
    // Explicit BEGIN/COMMIT typed in auto-commit mode are tracked too.
    let res = f
        .svc
        .execute(&other.id, "BEGIN; INSERT INTO audit_log VALUES ('x')", None)
        .await
        .unwrap();
    assert!(res.iter().all(|r| r.error.is_none()));
    assert!(f.svc.session(&other.id).unwrap().info().tx_open);
    f.svc.execute(&other.id, "COMMIT", None).await.unwrap();
    assert!(!f.svc.session(&other.id).unwrap().info().tx_open);
}

#[tokio::test]
async fn cancels_a_running_statement() {
    let f = fixture().await;
    let s = f.svc.open_session(&f.id).await.unwrap();
    let sid = s.id.clone();
    let svc = Arc::new(f.svc);
    let runner = {
        let svc = svc.clone();
        let sid = sid.clone();
        tokio::spawn(async move {
            svc.execute(
                &sid,
                "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 50000000) SELECT SUM(i) FROM n",
                None,
            )
            .await
        })
    };
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(svc.session(&sid).unwrap().info().running);
    svc.cancel(&sid).await.unwrap();
    let res = tokio::time::timeout(Duration::from_secs(5), runner)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(res[0].error.as_deref(), Some("query cancelled"));
    assert!(!svc.session(&sid).unwrap().info().running);
}

#[tokio::test]
async fn table_data_filters_sorts_and_counts() {
    let f = fixture().await;
    let req = TableDataRequest {
        schema: "main".into(),
        table: "users".into(),
        filter: Some("balance IS NOT NULL;".into()),
        order: vec![SortKey {
            column: "balance".into(),
            desc: true,
        }],
        column_filters: vec![],
        offset: 0,
        limit: Some(1),
    };
    let page = f.svc.table_data(&f.id, &req).await.unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0][1], json!("Grace Hopper"));
    assert!(page.has_more);
    assert_eq!(page.total, Some(2));
    assert_eq!(
        page.sql,
        "SELECT * FROM \"users\" WHERE balance IS NOT NULL ORDER BY \"balance\" DESC"
    );
    let empty = f
        .svc
        .table_data(
            &f.id,
            &TableDataRequest {
                filter: Some("1 = 0".into()),
                order: vec![],
                limit: None,
                ..req
            },
        )
        .await
        .unwrap();
    assert!(empty.rows.is_empty());
    assert_eq!(empty.columns.len(), 6);
    assert_eq!(empty.total, Some(0));
}

fn map(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[tokio::test]
async fn applies_row_changes_in_one_transaction() {
    let f = fixture().await;
    let changes = vec![
        RowChange {
            kind: ChangeKind::Update,
            key: map(&[("id", json!(2))]),
            values: map(&[
                ("email", json!("turing@example.com")),
                ("balance", json!(1.5)),
            ]),
        },
        RowChange {
            kind: ChangeKind::Insert,
            key: BTreeMap::new(),
            values: map(&[
                ("name", json!("Linus")),
                ("email", json!("linus@example.com")),
            ]),
        },
        RowChange {
            kind: ChangeKind::Delete,
            key: map(&[("id", json!(3))]),
            values: BTreeMap::new(),
        },
    ];
    let res = f
        .svc
        .apply_changes(&f.id, "main", "users", &changes, false)
        .await
        .unwrap();
    assert_eq!(res.statements.len(), 3);
    assert_eq!(res.rows_affected, 3);
    let s = f.svc.open_session(&f.id).await.unwrap();
    let rows = f
        .svc
        .execute(&s.id, "SELECT id, email FROM users ORDER BY id", None)
        .await
        .unwrap()
        .remove(0)
        .result
        .rows;
    assert_eq!(
        rows,
        vec![
            vec![json!(1), json!("ada@example.com")],
            vec![json!(2), json!("turing@example.com")],
            vec![json!(4), json!("linus@example.com")],
        ]
    );

    // A failing change rolls back the whole batch.
    let bad = vec![
        RowChange {
            kind: ChangeKind::Update,
            key: map(&[("id", json!(1))]),
            values: map(&[("name", json!("Changed"))]),
        },
        RowChange {
            kind: ChangeKind::Insert,
            key: BTreeMap::new(),
            values: map(&[("name", json!("Dup")), ("email", json!("ada@example.com"))]),
        },
    ];
    assert!(f
        .svc
        .apply_changes(&f.id, "main", "users", &bad, false)
        .await
        .is_err());
    let name = f
        .svc
        .execute(&s.id, "SELECT name FROM users WHERE id = 1", None)
        .await
        .unwrap()
        .remove(0)
        .result
        .rows[0][0]
        .clone();
    assert_eq!(name, json!("Ada Lovelace"));

    // Tables without a primary key are not editable.
    let err = f
        .svc
        .apply_changes(
            &f.id,
            "main",
            "audit_log",
            &[RowChange {
                kind: ChangeKind::Update,
                key: map(&[("message", json!("created"))]),
                values: map(&[("message", json!("x"))]),
            }],
            false,
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("no primary key"));
}

#[tokio::test]
async fn detects_env_data_sources_once() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".env"),
        "DB_CONNECTION=sqlite\nDB_DATABASE=database/app.sqlite\n",
    )
    .unwrap();
    let svc = DbService::new(None, Arc::new(MemoryStore::default()));
    svc.set_project(Some(dir.path().to_path_buf()))
        .await
        .unwrap();
    let found = svc.detect_env();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].source.name, "app.sqlite");
    let added = svc.add_from_env().await.unwrap();
    assert_eq!(added[0].source.comment, "From .env");
    assert!(svc.detect_env().is_empty());
}

#[tokio::test]
async fn shared_sessions_and_the_sessions_view() {
    let f = fixture().await;
    let a = f
        .svc
        .open_session_with(&f.id, forge_db::SessionMode::Shared)
        .await
        .unwrap();
    let b = f
        .svc
        .open_session_with(&f.id, forge_db::SessionMode::Shared)
        .await
        .unwrap();
    assert_eq!(a.id, b.id);
    assert!(b.shared);
    assert_eq!(b.consoles, 2);
    let own = f.svc.open_session(&f.id).await.unwrap();
    assert_ne!(own.id, a.id);
    assert_eq!(f.svc.sessions().len(), 2);

    // A transaction of the shared session is visible to every console using it.
    f.svc.set_auto_commit(&a.id, false).await.unwrap();
    f.svc
        .execute(
            &a.id,
            "INSERT INTO audit_log VALUES ('a'); INSERT INTO audit_log VALUES ('b')",
            None,
        )
        .await
        .unwrap();
    let info = f.svc.sessions().into_iter().find(|s| s.shared).unwrap();
    assert!(info.tx_open);
    assert_eq!(info.tx_statements, 2);
    assert!(info.tx_started_at.is_some());
    assert_eq!(
        info.last_sql.as_deref(),
        Some("INSERT INTO audit_log VALUES ('b')")
    );
    let info = f.svc.commit(&b.id).await.unwrap();
    assert!(!info.tx_open);
    assert_eq!(info.tx_statements, 0);

    // Closing one console keeps the shared connection for the other.
    f.svc.close_session(&a.id).await.unwrap();
    assert_eq!(f.svc.session(&b.id).unwrap().info().consoles, 1);
    f.svc.close_session(&b.id).await.unwrap();
    assert!(f.svc.session(&b.id).is_err());
    // The Sessions view closes a session whatever uses it.
    f.svc.terminate_session(&own.id).await.unwrap();
    assert!(f.svc.sessions().is_empty());
}

#[tokio::test]
async fn read_only_and_production_data_sources() {
    let f = fixture().await;
    let mut ds = f.svc.get(&f.id).unwrap();
    ds.options.read_only = true;
    f.svc.save(ds.clone(), None).await.unwrap();
    let s = f.svc.open_session(&f.id).await.unwrap();
    let err = f
        .svc
        .execute(&s.id, "DELETE FROM audit_log WHERE 1 = 1", None)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("app is read-only: DELETE"),
        "{err}"
    );
    // SQLite opens the file read-only too.
    let pool = f.svc.pool(&f.id).await.unwrap();
    let mut conn = pool.acquire().await.unwrap();
    assert!(conn.execute("DELETE FROM audit_log").await.is_err());
    drop(conn);

    ds.options.read_only = false;
    ds.options.color = Some("red".into());
    f.svc.save(ds, None).await.unwrap();
    let guard = f.svc.guard(&f.id, "UPDATE users SET balance = 0").unwrap();
    assert!(guard.confirm.unwrap().contains("every row of users"));
    assert!(f
        .svc
        .guard(&f.id, "UPDATE users SET balance = 0 WHERE id = 1")
        .unwrap()
        .confirm
        .is_none());
    let s = f.svc.open_session(&f.id).await.unwrap();
    let err = f
        .svc
        .execute(&s.id, "DELETE FROM audit_log", None)
        .await
        .unwrap_err();
    assert!(matches!(err, forge_db::DbError::NeedsConfirmation(_)));
    assert_eq!(
        scalar(&f.svc, &s.id, "SELECT COUNT(*) FROM audit_log").await,
        json!(1)
    );
    f.svc
        .execute_with(&s.id, "DELETE FROM audit_log", None, true)
        .await
        .unwrap();
    assert_eq!(
        scalar(&f.svc, &s.id, "SELECT COUNT(*) FROM audit_log").await,
        json!(0)
    );
}

#[tokio::test]
async fn data_editor_filters_navigates_and_edits_by_unique_key() {
    use forge_db::dataedit::{nav, ColumnFilter};
    let f = fixture().await;
    let s = f.svc.open_session(&f.id).await.unwrap();
    let res = f
        .svc
        .execute(
            &s.id,
            "CREATE TABLE tags (code TEXT NOT NULL UNIQUE, label TEXT, weight INTEGER DEFAULT 1);
             INSERT INTO tags (code, label) VALUES ('a', 'Alpha'), ('b', NULL);",
            None,
        )
        .await
        .unwrap();
    assert!(res.iter().all(|r| r.error.is_none()), "{res:?}");

    // "Filter by value" and the typed condition combine; NULL means IS NULL.
    let page = f
        .svc
        .table_data(
            &f.id,
            &TableDataRequest {
                schema: "main".into(),
                table: "tags".into(),
                filter: Some("weight = 1".into()),
                column_filters: vec![ColumnFilter::eq("label", Value::Null)],
                order: vec![],
                offset: 0,
                limit: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.rows, vec![vec![json!("b"), Value::Null, json!(1)]]);
    assert_eq!(
        page.sql,
        "SELECT * FROM \"tags\" WHERE (weight = 1) AND \"label\" IS NULL"
    );
    // A filter that writes is refused before it reaches the database.
    let err = f
        .svc
        .table_data(
            &f.id,
            &TableDataRequest {
                schema: "main".into(),
                table: "tags".into(),
                filter: Some("1=1; DELETE FROM tags".into()),
                column_filters: vec![],
                order: vec![],
                offset: 0,
                limit: None,
            },
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("one condition"), "{err}");

    // Foreign-key navigation: order 3 → its user, and Grace's orders.
    let info = f.svc.table_info(&f.id, "main", "orders").await.unwrap();
    let fk =
        serde_json::from_value::<nav::FkRef>(serde_json::to_value(&info.foreign_keys[0]).unwrap())
            .unwrap();
    let to_user = nav::referenced_row(
        forge_db::Dialect::Sqlite,
        "main",
        "orders",
        &fk,
        &["id".into(), "user_id".into()],
        &[json!(3), json!(3)],
    )
    .unwrap();
    let users = f
        .svc
        .table_data(
            &f.id,
            &TableDataRequest {
                schema: to_user.schema.clone(),
                table: to_user.table.clone(),
                filter: None,
                column_filters: to_user.filters.clone(),
                order: vec![],
                offset: 0,
                limit: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(users.rows.len(), 1);
    assert_eq!(users.rows[0][1], json!("Grace Hopper"));
    let back = nav::referencing_rows(
        forge_db::Dialect::Sqlite,
        "main",
        "orders",
        &fk,
        &["id".into(), "name".into()],
        &[json!(1), json!("Ada Lovelace")],
    )
    .unwrap();
    let orders = f
        .svc
        .table_data(
            &f.id,
            &TableDataRequest {
                schema: "main".into(),
                table: back.table.clone(),
                filter: Some(back.filter.clone()),
                column_filters: vec![],
                order: vec![SortKey {
                    column: "id".into(),
                    desc: false,
                }],
                offset: 0,
                limit: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(orders.rows.len(), 2);

    // No primary key, but a NOT NULL unique key identifies the rows.
    let change = RowChange {
        kind: ChangeKind::Update,
        key: map(&[("code", json!("b"))]),
        values: map(&[("label", json!("Beta"))]),
    };
    let preview = f
        .svc
        .preview_changes(&f.id, "main", "tags", std::slice::from_ref(&change))
        .await
        .unwrap();
    assert_eq!(
        preview.statements,
        vec!["UPDATE \"tags\" SET \"label\" = 'Beta' WHERE \"code\" = 'b'"]
    );
    assert!(preview.key.unwrap().source.starts_with("unique key"));
    assert!(preview.blocked.is_none() && preview.confirm.is_none());
    let res = f
        .svc
        .apply_changes(&f.id, "main", "tags", &[change], false)
        .await
        .unwrap();
    assert_eq!(res.rows_affected, 1);
}

#[tokio::test]
async fn production_writes_need_confirmation() {
    let f = fixture().await;
    let mut ds = f.svc.get(&f.id).unwrap();
    ds.options.color = Some("red".into());
    f.svc.save(ds, None).await.unwrap();
    let change = RowChange {
        kind: ChangeKind::Update,
        key: map(&[("id", json!(1))]),
        values: map(&[("name", json!("Ada"))]),
    };
    let preview = f
        .svc
        .preview_changes(&f.id, "main", "users", std::slice::from_ref(&change))
        .await
        .unwrap();
    assert!(preview.confirm.unwrap().contains("production"));
    let err = f
        .svc
        .apply_changes(&f.id, "main", "users", std::slice::from_ref(&change), false)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("confirm"), "{err}");
    f.svc
        .apply_changes(&f.id, "main", "users", &[change], true)
        .await
        .unwrap();
}
