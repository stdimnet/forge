//! Data sources of a project on a remote host: the project's `.forge/dataSources.toml` and
//! `.env` are read through its files, and project data sources connect through a tunnel to
//! the database host as the remote host sees it.

use std::sync::{Arc, Mutex};

use forge_core::MemoryFiles;
use forge_db::{
    DataSource, DbError, DbResult, DbService, DbTunnel, Driver, MemoryStore, ProjectContext, Scope,
    TunnelEndpoint,
};
use futures_util::future::BoxFuture;

/// Records the requested targets and forwards to a port nobody listens on.
#[derive(Default)]
struct FakeTunnel {
    opened: Mutex<Vec<(String, u16)>>,
}

impl DbTunnel for FakeTunnel {
    fn open(&self, host: String, port: u16) -> BoxFuture<'static, DbResult<TunnelEndpoint>> {
        self.opened.lock().unwrap().push((host, port));
        Box::pin(async move {
            let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = free.local_addr().unwrap().port();
            drop(free);
            Ok(TunnelEndpoint::new(port, ()))
        })
    }

    fn label(&self) -> String {
        "deploy@shop.example.com".into()
    }
}

#[tokio::test]
async fn remote_project_data_sources_go_through_the_project_files_and_a_tunnel() {
    let files = Arc::new(MemoryFiles::new());
    files.insert(
        ".env",
        "DB_CONNECTION=mysql\nDB_HOST=127.0.0.1\nDB_PORT=3306\nDB_DATABASE=shop\nDB_USERNAME=shop\nDB_PASSWORD=secret\n",
    );
    files.insert(
        ".forge/dataSources.toml",
        "[[dataSource]]\nid = \"reporting\"\nname = \"reporting\"\ndriver = \"postgres\"\nhost = \"db.internal\"\ndatabase = \"reports\"\n\n[dataSource.options]\nconnectTimeoutSecs = 1\n",
    );
    let tunnel = Arc::new(FakeTunnel::default());
    let svc = DbService::new(None, Arc::new(MemoryStore::default()));
    svc.set_project_context(ProjectContext {
        root: "/var/www/shop".into(),
        files: files.clone(),
        tunnel: Some(tunnel.clone()),
    })
    .await
    .unwrap();

    let list = svc.list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].via.as_deref(), Some("deploy@shop.example.com"));

    // `.env` of the remote project.
    let detected = svc.detect_env();
    assert_eq!(detected.len(), 1);
    assert_eq!(detected[0].source.database, "shop");
    let added = svc.add_from_env().await.unwrap();
    assert_eq!(added[0].source.scope, Scope::Project);
    let saved = files.get(".forge/dataSources.toml").unwrap();
    assert!(saved.contains("database = \"shop\""), "{saved}");
    assert!(saved.contains("database = \"reports\""), "{saved}");
    assert!(!saved.contains("secret"), "passwords stay out of the file");

    // Connecting asks for a tunnel to the host and port as the remote host sees them.
    let err = svc.schemas("reporting").await.unwrap_err();
    assert!(!err.to_string().is_empty());
    assert_eq!(
        tunnel.opened.lock().unwrap().clone(),
        vec![("db.internal".to_string(), 5432)]
    );

    // SQLite files of a remote project are not reachable from here.
    let sqlite = svc
        .save(
            DataSource {
                name: "local".into(),
                driver: Driver::Sqlite,
                file: "database/database.sqlite".into(),
                scope: Scope::Project,
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
    match svc.schemas(&sqlite.source.id).await {
        Err(DbError::Config(message)) => assert!(message.contains("SQLite"), "{message}"),
        other => panic!("expected a config error, got {other:?}"),
    }

    // Global data sources connect directly.
    let global = svc
        .save(
            DataSource {
                name: "scratch".into(),
                driver: Driver::Sqlite,
                file: ":memory:".into(),
                scope: Scope::Global,
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();
    assert!(global.via.is_none());
    assert!(svc.schemas(&global.source.id).await.is_ok());
}
