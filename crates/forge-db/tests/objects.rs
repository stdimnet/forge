//! Database objects against a SQLite file: Create/Modify Table scripts, the read-only and
//! production checks, imports with skip / abort / cancel, and the native tool target.

use std::sync::{Arc, Mutex};

use forge_db::model::design::{self, ColumnSpec, CreateOptions, KeySpec, TableSpec};
use forge_db::{DataSource, DbError, DbService, Driver, MemoryStore};
use forge_dbtransfer::insert::{ColumnMapping, ImportPlan, OnError};
use forge_dbtransfer::{read, ReadOptions, ValueKind};
use forge_sql::Dialect;
use tokio_util::sync::CancellationToken;

struct Fixture {
    dir: tempfile::TempDir,
    svc: DbService,
    id: String,
}

async fn fixture(read_only: bool, color: Option<&str>) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let svc = DbService::new(None, Arc::new(MemoryStore::default()));
    svc.set_project(Some(dir.path().to_path_buf()))
        .await
        .unwrap();
    let mut ds = DataSource {
        name: "app".into(),
        driver: Driver::Sqlite,
        file: "app.db".into(),
        ..Default::default()
    };
    ds.options.read_only = read_only;
    ds.options.color = color.map(String::from);
    let info = svc.save(ds, None).await.unwrap();
    Fixture {
        dir,
        svc,
        id: info.source.id,
    }
}

fn people() -> TableSpec {
    TableSpec {
        schema: "main".into(),
        name: "people".into(),
        columns: vec![
            ColumnSpec {
                name: "id".into(),
                data_type: "INTEGER".into(),
                auto_increment: true,
                ..Default::default()
            },
            ColumnSpec {
                name: "name".into(),
                data_type: "TEXT".into(),
                ..Default::default()
            },
            ColumnSpec {
                name: "mail".into(),
                data_type: "TEXT".into(),
                nullable: true,
                ..Default::default()
            },
        ],
        primary_key: Some(KeySpec {
            name: String::new(),
            columns: vec!["id".into()],
        }),
        ..Default::default()
    }
}

#[tokio::test]
async fn creates_and_modifies_a_table() {
    let f = fixture(false, None).await;
    let sql = design::create_table(Dialect::Sqlite, &people(), CreateOptions::default());
    let res = f.svc.run_script(&f.id, &sql, false).await.unwrap();
    assert_eq!((res.executed, res.error.as_deref()), (1, None), "{sql}");
    let info = f.svc.table_info(&f.id, "main", "people").await.unwrap();
    let old = TableSpec::from_table("main", &info);
    assert_eq!(old.columns.len(), 3);
    let mut new = old.clone();
    new.columns[2].name = "email".into();
    new.columns.push(ColumnSpec {
        name: "age".into(),
        data_type: "INTEGER".into(),
        nullable: true,
        ..Default::default()
    });
    let plan = design::alter_table(Dialect::Sqlite, &old, &new);
    assert!(!plan.destructive(), "{:?}", plan.warnings);
    let res = f.svc.run_script(&f.id, &plan.script(), false).await.unwrap();
    assert_eq!(res.error, None);
    assert_eq!(res.executed, 2);
    let info = f.svc.table_info(&f.id, "main", "people").await.unwrap();
    let names: Vec<_> = info.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["id", "name", "email", "age"]);

    // A type change rebuilds the table and keeps the rows.
    f.svc
        .run_script(&f.id, "INSERT INTO people (name, email) VALUES ('Ada', 'ada@x')", false)
        .await
        .unwrap();
    let old = TableSpec::from_table("main", &info);
    let mut new = old.clone();
    new.columns[3].data_type = "REAL".into();
    let plan = design::alter_table(Dialect::Sqlite, &old, &new);
    assert!(plan.destructive());
    let res = f.svc.run_script(&f.id, &plan.script(), false).await.unwrap();
    assert_eq!(res.error, None, "{}", plan.script());
    let info = f.svc.table_info(&f.id, "main", "people").await.unwrap();
    assert_eq!(info.columns[3].data_type.to_uppercase(), "REAL");
    let drop = design::drop_statements(
        Dialect::Sqlite,
        &design::DropRequest {
            schema: "main".into(),
            name: "people".into(),
            kind: "table".into(),
            ..Default::default()
        },
    );
    let res = f.svc.run_script(&f.id, &drop.join("\n"), false).await.unwrap();
    assert_eq!(res.error, None);
    assert!(f.svc.table_info(&f.id, "main", "people").await.is_err());
    // Errors stop the script and name the statement.
    let res = f
        .svc
        .run_script(&f.id, "CREATE TABLE a (x INT); CREATE TABLE a (x INT); CREATE TABLE b (x INT)", false)
        .await
        .unwrap();
    assert_eq!((res.executed, res.total), (1, 3));
    assert_eq!(res.failed.as_deref(), Some("CREATE TABLE a (x INT)"));
    assert!(res.error.is_some());
}

#[tokio::test]
async fn read_only_and_production_checks() {
    let ro = fixture(true, None).await;
    let err = ro
        .svc
        .run_script(&ro.id, "CREATE TABLE t (x INT)", true)
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::Refused(m) if m.contains("read-only")));
    assert!(ro.svc.restore_guard(&ro.id).is_err());

    let prod = fixture(false, Some("red")).await;
    let err = prod
        .svc
        .run_script(&prod.id, "CREATE TABLE t (x INT)", false)
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::NeedsConfirmation(m) if m.contains("production") && m.contains("CREATE")));
    assert!(prod.svc.run_script(&prod.id, "CREATE TABLE t (x INT)", true).await.unwrap().error.is_none());
    assert!(prod.svc.restore_guard(&prod.id).unwrap().unwrap().contains("production"));
    // Reads need no confirmation.
    assert!(prod.svc.objects_guard(&prod.id, "SELECT 1").unwrap().confirm.is_none());
}

fn plan(on_error: OnError) -> ImportPlan {
    ImportPlan {
        schema: "main".into(),
        table: "people".into(),
        columns: vec![
            ColumnMapping {
                source: 0,
                target: "id".into(),
                kind: ValueKind::Integer,
            },
            ColumnMapping {
                source: 1,
                target: "name".into(),
                kind: ValueKind::Text,
            },
        ],
        batch_size: 2,
        on_error,
        ..Default::default()
    }
}

const CSV: &str = "id,name\n1,Ada\n2,Grace\n2,Duplicate\n3,Alan\n4,\n";

async fn count(f: &Fixture) -> String {
    let session = f.svc.open_session(&f.id).await.unwrap();
    let r = f
        .svc
        .execute(&session.id, "SELECT COUNT(*) FROM people", None)
        .await
        .unwrap();
    f.svc.close_session(&session.id).await.unwrap();
    r[0].result.rows[0][0].to_string()
}

#[tokio::test]
async fn imports_with_skip_abort_and_cancel() {
    let f = fixture(false, None).await;
    let source = read(CSV.as_bytes(), &ReadOptions::default()).unwrap();
    let mut spec = people();
    spec.columns.truncate(2);
    spec.columns[0].auto_increment = false;
    let progress = Arc::new(Mutex::new(Vec::new()));
    let p = progress.clone();
    let skip = ImportPlan {
        create: Some(spec),
        ..plan(OnError::Skip)
    };
    let s = f
        .svc
        .import(&f.id, "imp-1", &source, &skip, false, CancellationToken::new(), move |e| {
            p.lock().unwrap().push((e.done, e.total))
        })
        .await
        .unwrap();
    assert!(s.created);
    // The duplicate id fails; the empty name is NULL in a NOT NULL column and fails too.
    assert_eq!((s.inserted, s.skipped), (3, 2), "{s:?}");
    assert_eq!(s.errors.iter().map(|e| e.row).collect::<Vec<_>>(), [2, 4]);
    assert_eq!(*progress.lock().unwrap(), [(2, 5), (4, 5), (5, 5)]);
    assert_eq!(count(&f).await, "3");

    // Abort rolls everything back.
    f.svc.run_script(&f.id, "DELETE FROM people", false).await.unwrap();
    let s = f
        .svc
        .import(&f.id, "imp-2", &source, &plan(OnError::Abort), false, CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert!(s.aborted);
    assert_eq!(s.inserted, 0);
    assert_eq!(s.errors[0].row, 2);
    assert_eq!(count(&f).await, "0");

    let cancel = CancellationToken::new();
    cancel.cancel();
    let s = f
        .svc
        .import(&f.id, "imp-3", &source, &plan(OnError::Skip), false, cancel, |_| {})
        .await
        .unwrap();
    assert!(s.cancelled);
    assert_eq!(s.inserted, 0);
}

#[tokio::test]
async fn imports_are_checked_like_scripts() {
    let ro = fixture(true, None).await;
    let source = read(CSV.as_bytes(), &ReadOptions::default()).unwrap();
    let err = ro
        .svc
        .import(&ro.id, "x", &source, &plan(OnError::Skip), true, CancellationToken::new(), |_| {})
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::Refused(_)));
    let prod = fixture(false, Some("red")).await;
    let err = prod
        .svc
        .import(&prod.id, "x", &source, &plan(OnError::Skip), false, CancellationToken::new(), |_| {})
        .await
        .unwrap_err();
    assert!(matches!(err, DbError::NeedsConfirmation(_)));
}

#[tokio::test]
async fn native_tool_target_of_a_sqlite_file() {
    let f = fixture(false, None).await;
    let (target, tunnel) = f.svc.dump_target(&f.id).await.unwrap();
    assert!(tunnel.is_none());
    assert_eq!(target.driver, "sqlite");
    assert_eq!(std::path::Path::new(&target.file), f.dir.path().join("app.db"));
    let req = forge_dbtransfer::DumpRequest {
        tool: forge_dbtransfer::DumpTool::Sqlite3,
        file: "/tmp/out.sql".into(),
        ..Default::default()
    };
    let cmd = forge_dbtransfer::dump::command(&req, &target).unwrap();
    assert_eq!(cmd.args[1], target.file);
}
