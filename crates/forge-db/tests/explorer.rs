//! The Database Explorer of `DbService` against a real SQLite file: the full object tree,
//! the model cache in the cache dir, incremental refresh, virtual foreign keys and columns
//! in the project, DDL data sources, Quick Documentation and full-text search.

use std::sync::{Arc, Mutex};

use forge_db::model::{SearchMode, SearchOptions, VirtualForeignKey};
use forge_db::{
    DataSource, DbService, Dialect, Driver, MemoryStore, ObjectRef, SearchRequest, SearchScope,
    TableKind, VirtualObjects,
};

const SCHEMA: &str = "
CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL, -- display name
    email TEXT NOT NULL UNIQUE CHECK (email LIKE '%@%'),
    avatar BLOB
);
CREATE TABLE orders (
    id INTEGER PRIMARY KEY,
    customer INTEGER NOT NULL,
    note TEXT,
    total NUMERIC NOT NULL,
    CONSTRAINT total_positive CHECK (total >= 0)
);
CREATE VIEW big_orders AS SELECT * FROM orders WHERE total > 100;
CREATE TRIGGER orders_note AFTER INSERT ON orders BEGIN UPDATE orders SET note = 'new' WHERE id = NEW.id AND note IS NULL; END;
INSERT INTO users (name, email) VALUES ('Ada Lovelace', 'ada@example.com'), ('Alan Turing', 'alan@example.com');
INSERT INTO orders (customer, note, total) VALUES (1, 'gift for ADA', 19.99), (2, NULL, 250), (1, 'ada again', 5);
";

struct Fixture {
    dir: tempfile::TempDir,
    cache: tempfile::TempDir,
    svc: DbService,
    id: String,
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let svc = DbService::new(None, Arc::new(MemoryStore::default()))
        .with_cache_dir(cache.path().to_path_buf());
    svc.set_project(Some(dir.path().to_path_buf()))
        .await
        .unwrap();
    let info = svc
        .save(
            DataSource {
                name: "app".into(),
                driver: Driver::Sqlite,
                file: "app.db".into(),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
    let id = info.source.id.clone();
    let session = svc.open_session(&id).await.unwrap();
    let res = svc.execute(&session.id, SCHEMA, None).await.unwrap();
    assert!(res.iter().all(|r| r.error.is_none()), "{res:?}");
    svc.close_session(&session.id).await.unwrap();
    Fixture {
        dir,
        cache,
        svc,
        id,
    }
}

#[tokio::test]
async fn sqlite_object_tree_with_checks_triggers_and_views() {
    let f = fixture().await;
    let model = f.svc.model(&f.id, true).await.unwrap();
    assert_eq!(model.dialect, Dialect::Sqlite);
    assert_eq!(model.schemas.len(), 1);
    assert!(model.schemas[0].visible && model.schemas[0].current);
    let s = f.svc.introspect(&f.id, "main").await.unwrap();
    let orders = s.table("orders").unwrap();
    assert_eq!(orders.checks.len(), 1);
    assert_eq!(orders.checks[0].name, "total_positive");
    assert_eq!(orders.triggers[0].name, "orders_note");
    assert_eq!(orders.triggers[0].timing, "AFTER");
    assert_eq!(orders.triggers[0].events, "INSERT");
    let users = s.table("users").unwrap();
    assert_eq!(users.checks.len(), 1, "{:?}", users.checks);
    let view = s.table("big_orders").unwrap();
    assert_eq!(view.kind, TableKind::View);
    assert!(view.definition.starts_with("SELECT * FROM orders"));
}

#[tokio::test]
async fn caches_the_model_and_refreshes_one_table() {
    let f = fixture().await;
    f.svc.model(&f.id, true).await.unwrap();
    f.svc.introspect(&f.id, "main").await.unwrap();
    let files: Vec<_> = std::fs::read_dir(f.cache.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files.len(), 1, "{files:?}");
    assert!(files[0].starts_with(&f.id));
    assert!(!f.dir.path().join(".forge").join("cache").exists());

    // A new service (next start) shows the tree from the cache without connecting.
    f.svc.disconnect(&f.id).await;
    let again = DbService::new(None, Arc::new(MemoryStore::default()))
        .with_cache_dir(f.cache.path().to_path_buf());
    again
        .set_project(Some(f.dir.path().to_path_buf()))
        .await
        .unwrap();
    let cached = again.model(&f.id, false).await.unwrap();
    assert!(cached.loaded["main"].table("orders").is_some());
    assert!(!again.is_connected(&f.id).await);

    // Refresh one table after a change: the cached model gets the new column.
    let session = f.svc.open_session(&f.id).await.unwrap();
    f.svc
        .execute(
            &session.id,
            "ALTER TABLE orders ADD COLUMN shipped_at TEXT",
            None,
        )
        .await
        .unwrap();
    f.svc.close_session(&session.id).await.unwrap();
    let t = f.svc.table_info(&f.id, "main", "orders").await.unwrap();
    assert!(t.column("shipped_at").is_some());
    let model = f.svc.model(&f.id, false).await.unwrap();
    assert!(model.loaded["main"]
        .table("orders")
        .unwrap()
        .column("shipped_at")
        .is_some());
}

#[tokio::test]
async fn schema_filter_hides_schemas_from_introspection() {
    let f = fixture().await;
    let mut ds = f.svc.get(&f.id).unwrap();
    ds.options.schemas = vec!["!main".into()];
    f.svc.save(ds, None).await.unwrap();
    let model = f.svc.model(&f.id, true).await.unwrap();
    assert!(!model.schemas[0].visible);
    assert!(model.loaded.is_empty());
    let err = f.svc.introspect(&f.id, "main").await.unwrap_err();
    assert!(
        err.to_string().contains("hidden by the schema filter"),
        "{err}"
    );
    let mut ds = f.svc.get(&f.id).unwrap();
    ds.options.schemas = vec!["/(/".into()];
    f.svc.save(ds, None).await.unwrap();
    assert!(f.svc.model(&f.id, false).await.is_err());
}

#[tokio::test]
async fn virtual_foreign_keys_are_stored_in_the_project() {
    let f = fixture().await;
    let mut v = VirtualObjects::default();
    v.foreign_keys.push(VirtualForeignKey {
        schema: "main".into(),
        table: "orders".into(),
        columns: vec!["customer".into()],
        ref_table: "users".into(),
        ref_columns: vec!["id".into()],
        ..Default::default()
    });
    v.columns.push(forge_db::model::VirtualColumn {
        schema: "main".into(),
        table: "orders".into(),
        name: "gross".into(),
        expression: "total * 1.19".into(),
        data_type: "NUMERIC".into(),
        ..Default::default()
    });
    f.svc.save_virtual_objects(&f.id, v).unwrap();
    let text = std::fs::read_to_string(f.dir.path().join(".forge/database/virtual.toml")).unwrap();
    assert!(
        text.contains(&format!("dataSource = \"{}\"", f.id)),
        "{text}"
    );
    let s = f.svc.introspect(&f.id, "main").await.unwrap();
    let orders = s.table("orders").unwrap();
    let fk = orders.foreign_keys.iter().find(|k| k.is_virtual).unwrap();
    assert_eq!(fk.ref_table, "users");
    assert!(orders.column("gross").unwrap().is_virtual);
    let doc = f
        .svc
        .object_doc(
            &f.id,
            "main",
            &ObjectRef {
                kind: "column".into(),
                table: "orders".into(),
                name: "customer".into(),
            },
        )
        .await
        .unwrap();
    assert!(doc
        .properties
        .iter()
        .any(|p| p.name == "References (virtual)" && p.value == "users.id"));
    // Unpaired columns are refused.
    let mut bad = f.svc.virtual_objects(&f.id).unwrap();
    bad.foreign_keys[0].ref_columns.clear();
    assert!(f.svc.save_virtual_objects(&f.id, bad).is_err());
    f.svc
        .save_virtual_objects(&f.id, VirtualObjects::default())
        .unwrap();
    assert!(f.svc.virtual_objects(&f.id).unwrap().is_empty());
}

#[tokio::test]
async fn full_text_search_finds_rows_per_table() {
    let f = fixture().await;
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let summary = f
        .svc
        .full_text_search(
            "s1",
            SearchRequest {
                scopes: vec![SearchScope {
                    data_source: f.id.clone(),
                    ..Default::default()
                }],
                options: SearchOptions {
                    text: "ada".into(),
                    mode: SearchMode::Contains,
                    case_sensitive: false,
                    all_columns: false,
                    limit: Some(1),
                },
            },
            move |p| sink.lock().unwrap().push(p),
        )
        .await
        .unwrap();
    assert_eq!(summary.tables, 3);
    assert_eq!(summary.matched_tables, 2, "big_orders has no matching row");
    assert!(!summary.cancelled);
    let events = events.lock().unwrap();
    assert_eq!(events.last().unwrap().done, 3);
    let orders = events
        .iter()
        .filter_map(|e| e.result.as_ref())
        .find(|r| r.table == "orders")
        .unwrap();
    assert_eq!(orders.rows.len(), 1);
    assert!(orders.truncated, "two orders mention ada; the limit is one");
    let note = orders.columns.iter().position(|c| c == "note").unwrap();
    assert_eq!(orders.matches[0], [note]);
    assert!(orders.sql.contains("LIKE '%ada%'"), "{}", orders.sql);

    // Tables by pattern, case-sensitive; SQLite has no regular expressions.
    let only = f
        .svc
        .full_text_search(
            "s2",
            SearchRequest {
                scopes: vec![SearchScope {
                    data_source: f.id.clone(),
                    schemas: vec!["main".into()],
                    tables: vec!["us*".into()],
                }],
                options: SearchOptions {
                    text: "Ada".into(),
                    case_sensitive: true,
                    ..Default::default()
                },
            },
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!((only.tables, only.rows), (1, 1));
    let regex = f
        .svc
        .full_text_search(
            "s3",
            SearchRequest {
                scopes: vec![SearchScope {
                    data_source: f.id.clone(),
                    tables: vec!["users".into()],
                    ..Default::default()
                }],
                options: SearchOptions {
                    text: "^a".into(),
                    mode: SearchMode::Regex,
                    ..Default::default()
                },
            },
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(regex.errors, 1);
    f.svc.cancel_search("unknown");
}

#[tokio::test]
async fn ddl_data_sources_parse_project_files() {
    let dir = tempfile::tempdir().unwrap();
    let mig = dir.path().join("db/migrations");
    std::fs::create_dir_all(&mig).unwrap();
    std::fs::write(
        mig.join("V1__init.sql"),
        "CREATE TABLE authors (id bigint PRIMARY KEY, name text NOT NULL);\nCREATE TABLE posts (id bigint PRIMARY KEY, title text);",
    )
    .unwrap();
    std::fs::write(
        mig.join("V10__fk.sql"),
        "ALTER TABLE posts ADD CONSTRAINT posts_author_fk FOREIGN KEY (author_id) REFERENCES authors (id);",
    )
    .unwrap();
    std::fs::write(
        mig.join("V2__author.sql"),
        "ALTER TABLE posts ADD COLUMN author_id bigint;\nCREATE INDEX posts_author ON posts (author_id);\nCREATE TABLE oops (;",
    )
    .unwrap();
    let svc = DbService::new(None, Arc::new(MemoryStore::default()));
    svc.set_project(Some(dir.path().to_path_buf()))
        .await
        .unwrap();
    let mut ds = DataSource {
        name: "migrations".into(),
        driver: Driver::Ddl,
        ..Default::default()
    };
    ds.options.ddl_files = vec!["db/migrations".into()];
    ds.options.ddl_dialect = Some(Dialect::Postgres);
    let info = svc.save(ds, None).await.unwrap();
    assert_eq!(info.summary, "db/migrations");
    let id = info.source.id;
    let model = svc.model(&id, false).await.unwrap();
    assert_eq!(model.schemas[0].name, "public");
    let posts = model.table("public", "posts").unwrap();
    assert_eq!(posts.foreign_keys[0].ref_table, "authors");
    assert!(posts.indexes.iter().any(|i| i.name == "posts_author"));
    assert_eq!(model.warnings.len(), 1);
    assert_eq!(model.warnings[0].file, "db/migrations/V2__author.sql");
    assert_eq!(model.warnings[0].line, 3);
    // No connection, but DDL and documentation from the model.
    assert!(svc.pool(&id).await.is_err());
    let ddl = svc.table_ddl(&id, "public", "posts").await.unwrap();
    assert!(ddl.contains("FOREIGN KEY (\"author_id\")"), "{ddl}");
    // A changed file: the source is re-parsed.
    std::fs::write(mig.join("V11__tags.sql"), "CREATE TABLE tags (id int);").unwrap();
    let touched = svc.ddl_sources_touched(&["db/migrations/V11__tags.sql".into()]);
    assert_eq!(touched, [id.clone()]);
    assert!(svc.ddl_sources_touched(&["src/main.rs".into()]).is_empty());
    let model = svc.model(&id, false).await.unwrap();
    assert!(model.table("public", "tags").is_some());
}
