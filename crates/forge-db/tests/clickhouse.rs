//! ClickHouse data sources against a local HTTP interface mock: introspection, table data,
//! console statements, read-only mode enforced by the server and error messages.

mod common;

use std::sync::Arc;

use forge_db::{DataSource, DbPool, DbService, Driver, MemoryStore, TableDataRequest, TableKind};
use serde_json::json;

fn source(port: u16) -> DataSource {
    let mut ds = DataSource {
        id: "events".into(),
        name: "analytics".into(),
        driver: Driver::Clickhouse,
        host: "127.0.0.1".into(),
        port: Some(port),
        database: "shop".into(),
        user: "default".into(),
        scope: forge_db::Scope::Global,
        ..Default::default()
    };
    ds.options.connect_timeout_secs = 2;
    ds
}

#[tokio::test]
async fn introspects_and_queries_over_http() {
    let (port, requests) = common::clickhouse::start().await;
    let svc = DbService::new(None, Arc::new(MemoryStore::default()));
    let info = svc.save(source(port), Some("secret".into())).await.unwrap();
    let id = info.source.id;
    let t = svc.test(&svc.get(&id).unwrap(), None).await;
    assert_eq!(
        t.server_version.as_deref(),
        Some("ClickHouse 24.3.1.1"),
        "{t:?}"
    );
    let wrong = svc.test(&svc.get(&id).unwrap(), Some("nope".into())).await;
    assert!(!wrong.ok);
    assert!(wrong.error.unwrap().contains("Authentication failed"));

    let schemas = svc.schemas(&id).await.unwrap();
    assert!(schemas.iter().any(|s| s.name == "shop" && s.current));
    let s = svc.introspect(&id, "shop").await.unwrap();
    let events = s.tables.iter().find(|t| t.name == "events").unwrap();
    assert_eq!(events.kind, TableKind::Table);
    assert_eq!(events.row_estimate, Some(1200));
    assert_eq!(events.primary_key, vec!["id".to_string()]);
    let user = events.columns.iter().find(|c| c.name == "user").unwrap();
    assert!(user.nullable);
    assert_eq!(user.comment, "who");
    let at = events.columns.iter().find(|c| c.name == "at").unwrap();
    assert_eq!(at.default.as_deref(), Some("now()"));
    let view = s.tables.iter().find(|t| t.name == "daily_sales").unwrap();
    assert_eq!(view.kind, TableKind::View);

    let page = svc
        .table_data(
            &id,
            &TableDataRequest {
                schema: "shop".into(),
                table: "events".into(),
                filter: None,
                column_filters: Vec::new(),
                order: Vec::new(),
                offset: 0,
                limit: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        page.rows[0],
        vec![json!("1"), json!("2025-01-06 09:30:00"), json!("ada")]
    );
    assert_eq!(page.total, Some(2));

    let session = svc.open_session(&id).await.unwrap();
    assert!(!session.transactions);
    let res = svc
        .execute(
            &session.id,
            "INSERT INTO events VALUES (3, now(), 'x'); SELECT boom",
            None,
        )
        .await
        .unwrap();
    assert_eq!(res[0].result.rows_affected, 2);
    assert!(
        res[1]
            .error
            .as_deref()
            .unwrap()
            .starts_with("Code: 62. Syntax error"),
        "{res:?}"
    );

    let reqs = requests.lock().unwrap();
    let last = reqs.last().unwrap();
    assert_eq!(last.param("database").as_deref(), Some("shop"));
    assert_eq!(last.param("default_format").as_deref(), Some("JSONCompact"));
    assert!(last.param("session_id").is_some());
    assert_eq!(last.header("X-ClickHouse-User"), Some("default"));
}

#[tokio::test]
async fn read_only_is_enforced_by_the_server() {
    let (port, requests) = common::clickhouse::start().await;
    let mut ds = source(port);
    ds.options.read_only = true;
    let pool = DbPool::connect(&ds, Some("secret"), None).await.unwrap();
    let mut conn = pool.acquire().await.unwrap();
    // Past the console guard, the server refuses the write.
    let err = conn
        .run("INSERT INTO events VALUES (1)", 0, 10)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("readonly mode"), "{err}");
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .param("readonly")
            .as_deref(),
        Some("2")
    );
}
