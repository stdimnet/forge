//! End-to-end tests of the schema tools against real SQLite files: compare two databases,
//! run the generated migration on the target (tables rebuilt with their rows), compare again,
//! lay out the ER diagram of the introspected schema, and Explain Plan in a console session.

use std::sync::Arc;

use forge_db::schema::{self, CompareOptions, DiagramRequest, DiffKind, MigrationOptions};
use forge_db::{DataSource, DbService, Dialect, Driver, MemoryStore};
use serde_json::json;

/// How the schema should be (the source).
const DEV: &str = "
CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    name VARCHAR(150) NOT NULL DEFAULT '',
    last_login TEXT
);
CREATE TABLE orders (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    total NUMERIC NOT NULL,
    note TEXT
);
CREATE INDEX orders_total_index ON orders(total);
CREATE TABLE audit (id INTEGER PRIMARY KEY, message TEXT);
";

/// How it is (the target), with rows that must survive the migration.
const PROD: &str = "
CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    name VARCHAR(100)
);
CREATE TABLE orders (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id),
    total NUMERIC NOT NULL
);
CREATE INDEX orders_user_index ON orders(user_id);
CREATE TABLE legacy (id INTEGER);
INSERT INTO users (id, email, name) VALUES (1, 'ada@example.com', 'Ada'), (2, 'alan@example.com', 'Alan');
INSERT INTO orders (user_id, total) VALUES (1, 19.99), (2, 5);
";

struct Fixture {
    _dir: tempfile::TempDir,
    svc: DbService,
    dev: String,
    prod: String,
}

async fn source(svc: &DbService, name: &str, sql: &str) -> String {
    let info = svc
        .save(
            DataSource {
                name: name.into(),
                driver: Driver::Sqlite,
                file: format!("{name}.db"),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
    let id = info.source.id.clone();
    let s = svc.open_session(&id).await.unwrap();
    let res = svc.execute(&s.id, sql, None).await.unwrap();
    assert!(res.iter().all(|r| r.error.is_none()), "{res:?}");
    svc.close_session(&s.id).await.unwrap();
    id
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let svc = DbService::new(None, Arc::new(MemoryStore::default()));
    svc.set_project(Some(dir.path().to_path_buf()))
        .await
        .unwrap();
    let dev = source(&svc, "dev", DEV).await;
    let prod = source(&svc, "prod", PROD).await;
    Fixture {
        _dir: dir,
        svc,
        dev,
        prod,
    }
}

#[tokio::test]
async fn compare_and_migrate_sqlite_databases() {
    let f = fixture().await;
    let dev = f.svc.introspect(&f.dev, "main").await.unwrap();
    let prod = f.svc.introspect(&f.prod, "main").await.unwrap();
    let diff = schema::compare(&dev, &prod, &CompareOptions::default());
    let kinds: Vec<(&str, DiffKind)> = diff
        .tables
        .iter()
        .map(|t| (t.name.as_str(), t.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("audit", DiffKind::Added),
            ("orders", DiffKind::Changed),
            ("users", DiffKind::Changed),
            ("legacy", DiffKind::Removed)
        ],
        "{diff:#?}"
    );
    // The UNIQUE autoindex and generated foreign key names are not differences.
    let users = &diff.tables[2];
    assert!(
        users
            .items
            .iter()
            .all(|i| i.name != "sqlite_autoindex_users_1"),
        "{users:#?}"
    );

    let m = schema::migration(Dialect::Sqlite, &dev, &prod, &MigrationOptions::default());
    assert!(m.destructive);
    let s = f.svc.open_session(&f.prod).await.unwrap();
    let res = f.svc.execute(&s.id, &m.sql, None).await.unwrap();
    assert!(res.iter().all(|r| r.error.is_none()), "{}\n{res:#?}", m.sql);

    let after = f.svc.introspect(&f.prod, "main").await.unwrap();
    let again = schema::compare(&dev, &after, &CompareOptions::default());
    assert!(again.is_empty(), "{}\n{again:#?}", m.sql);
    // The rows were copied by the rebuilds.
    let rows = f
        .svc
        .execute(
            &s.id,
            "SELECT u.name, o.total FROM orders o JOIN users u ON u.id = o.user_id ORDER BY o.id",
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        rows[0].result.rows,
        vec![
            vec![json!("Ada"), json!(19.99)],
            vec![json!("Alan"), json!(5)]
        ]
    );
    // Foreign keys are enforced again.
    let fk = f
        .svc
        .execute(&s.id, "PRAGMA foreign_keys", None)
        .await
        .unwrap();
    assert_eq!(fk[0].result.rows, vec![vec![json!(1)]]);
    f.svc.close_session(&s.id).await.unwrap();

    // The diagram of the migrated schema.
    let d = schema::layout(&DiagramRequest {
        schema: after,
        ..Default::default()
    });
    assert_eq!(d.nodes.len(), 3);
    let e = &d.edges[0];
    assert_eq!((e.from.as_str(), e.to.as_str()), ("orders", "users"));
    let users = d.nodes.iter().find(|n| n.id == "users").unwrap();
    let orders = d.nodes.iter().find(|n| n.id == "orders").unwrap();
    assert!(users.y < orders.y);
}

#[tokio::test]
async fn explain_plan_in_a_console_session() {
    let f = fixture().await;
    let s = f.svc.open_session(&f.prod).await.unwrap();
    let sql = "SELECT * FROM orders o JOIN users u ON u.id = o.user_id WHERE o.user_id = 1 ORDER BY o.total;";
    let r = f.svc.explain(&s.id, sql, false, false).await.unwrap();
    assert!(r.sql.starts_with("EXPLAIN QUERY PLAN SELECT"), "{}", r.sql);
    let plan = r.plan.expect("plan");
    assert_eq!(plan.root.operation, "QUERY PLAN");
    let ops: Vec<String> = plan
        .root
        .children
        .iter()
        .map(|c| format!("{} {}", c.operation, c.object))
        .collect();
    assert!(
        ops.iter()
            .any(|o| o.starts_with("SEARCH o") || o.starts_with("SCAN o")),
        "{ops:?}"
    );
    assert!(ops.iter().any(|o| o.starts_with("SEARCH u")), "{ops:?}");
    assert_eq!(plan.flame.len(), plan.root.children.len() + 1);
    // Raw: the rows only.
    let raw = f.svc.explain(&s.id, sql, true, false).await.unwrap();
    assert!(raw.plan.is_none() && !raw.rows.is_empty());
    assert_eq!(
        raw.columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["id", "parent", "notused", "detail"]
    );
    // Explaining a DELETE does not run it.
    f.svc
        .explain(&s.id, "DELETE FROM orders", false, false)
        .await
        .unwrap();
    let count = f
        .svc
        .execute(&s.id, "SELECT COUNT(*) FROM orders", None)
        .await
        .unwrap();
    assert_eq!(count[0].result.rows, vec![vec![json!(2)]]);
    // Errors of the statement come back as errors.
    assert!(f
        .svc
        .explain(&s.id, "SELECT * FROM missing", false, false)
        .await
        .is_err());
    f.svc.close_session(&s.id).await.unwrap();
}
