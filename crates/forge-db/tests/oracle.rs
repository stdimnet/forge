//! Oracle through a fake `sqlplus` on PATH (a shell script that answers like SQL*Plus with
//! `SET MARKUP CSV ON`): login over stdin, introspection, queries and update counts.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

use forge_db::engine::oracle::{OraclePool, Tool};
use forge_db::engine::{ConnectSpec, EnginePool};
use forge_db::tls::TlsMaterial;
use forge_db::{DataSource, Driver};
use serde_json::json;

const FAKE: &str = r#"#!/bin/sh
input=$(cat)
case "$input" in
  *'CONNECT "hr"/"tiger"@"//127.0.0.1:1521/FREEPDB1"'*) ;;
  *) echo "ERROR:"; echo "ORA-01017: invalid username/password; logon denied"; exit 1 ;;
esac
stmt=$(printf '%s\n' "$input" | sed -n '/^PROMPT FORGE-RESULT-BEGIN/,/^PROMPT FORGE-RESULT-END/p' | sed '1d;$d')
echo "Connected."
echo "FORGE-RESULT-BEGIN"
echo
case "$stmt" in
  *'v$version'*) echo '"BANNER"'; echo '"Oracle Database 23ai Free Release 23.0.0.0.0"'; echo; echo "1 row selected." ;;
  *"FROM all_users"*) echo '"USERNAME"'; echo '"HR"'; echo '"SALES"'; echo; echo "2 rows selected." ;;
  *"CURRENT_SCHEMA') FROM dual"*) echo '"SYS_CONTEXT(USERENV,CURRENT_SCHEMA)"'; echo '"HR"'; echo; echo "1 row selected." ;;
  *"FROM all_objects o"*) echo '"OBJECT_NAME","OBJECT_TYPE","COMMENTS","NUM_ROWS"'; echo '"EMPLOYEES","TABLE","Staff",107'; echo '"EMP_DETAILS_VIEW","VIEW",,'; echo; echo "2 rows selected." ;;
  *"FROM all_tab_columns"*) echo '"TABLE_NAME","COLUMN_NAME","TYPE","NULLABLE","DATA_DEFAULT","IDENTITY_COLUMN"'; echo '"EMPLOYEES","EMPLOYEE_ID","NUMBER(6,0)","N",,"YES"'; echo '"EMPLOYEES","LAST_NAME","VARCHAR2(25)","N",,"NO"'; echo; echo "2 rows selected." ;;
  *"FROM all_indexes"*) echo '"TABLE_NAME","INDEX_NAME","UNIQUENESS","PK","COLUMN_NAME"'; echo '"EMPLOYEES","EMP_EMP_ID_PK","UNIQUE","Y","EMPLOYEE_ID"'; echo; echo "1 row selected." ;;
  *"SELECT * FROM employees"*) echo '"EMPLOYEE_ID","LAST_NAME","SALARY"'; echo '100,"King",24000'; echo '101,"Kochhar",17000'; echo; echo "2 rows selected." ;;
  *"UPDATE employees"*) echo "2 rows updated." ;;
  *"select nope"*) echo 'select nope'; echo '       *'; echo 'ERROR at line 1:'; echo 'ORA-00904: "NOPE": invalid identifier'; exit 904 ;;
  *) echo "no rows selected" ;;
esac
echo
echo "FORGE-RESULT-END"
"#;

fn source() -> DataSource {
    DataSource {
        id: "hr".into(),
        name: "hr@oracle".into(),
        driver: Driver::Oracle,
        host: "127.0.0.1".into(),
        database: "FREEPDB1".into(),
        user: "hr".into(),
        ..Default::default()
    }
}

fn fake_pool(dir: &std::path::Path, ds: &DataSource, password: &str) -> Arc<dyn EnginePool> {
    let tool = dir.join("sqlplus");
    std::fs::write(&tool, FAKE).unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    let spec = ConnectSpec {
        ds,
        password: Some(password),
        tls: TlsMaterial::default(),
        tls_host: ds.host.clone(),
        root: None,
    };
    Arc::new(OraclePool::with_tool(&spec, tool, Tool::SqlPlus).unwrap())
}

#[tokio::test]
async fn runs_statements_through_sqlplus() {
    let dir = tempfile::tempdir().unwrap();
    let ds = source();
    let pool = forge_db::DbPool::Engine(fake_pool(dir.path(), &ds, "tiger"));
    let mut conn = pool.acquire().await.unwrap();
    assert_eq!(
        conn.server_version().await.unwrap(),
        "Oracle Database 23ai Free Release 23.0.0.0.0"
    );
    let schemas = forge_db::introspect::schemas(&mut conn).await.unwrap();
    assert_eq!(
        schemas
            .iter()
            .map(|s| (s.name.as_str(), s.current))
            .collect::<Vec<_>>(),
        vec![("HR", true), ("SALES", false)]
    );
    let hr = forge_db::introspect::schema(&mut conn, "HR", None)
        .await
        .unwrap();
    let emp = &hr.tables[0];
    assert_eq!(emp.name, "EMPLOYEES");
    assert_eq!(emp.row_estimate, Some(107));
    assert_eq!(emp.primary_key, vec!["EMPLOYEE_ID".to_string()]);
    assert!(emp.columns[0].auto_increment);
    assert_eq!(emp.columns[1].data_type, "VARCHAR2(25)");
    assert_eq!(hr.tables[1].kind, forge_db::TableKind::View);

    let r = conn.run("SELECT * FROM employees", 0, 1).await.unwrap();
    assert_eq!(r.rows, vec![vec![json!(100), json!("King"), json!(24000)]]);
    assert!(r.has_more);
    let r = conn
        .run("UPDATE employees SET salary = salary", 0, 10)
        .await
        .unwrap();
    assert_eq!(r.rows_affected, 2);
    let err = conn.run("select nope from dual", 0, 10).await.unwrap_err();
    assert!(err.to_string().contains("ORA-00904"), "{err}");

    let bad = forge_db::DbPool::Engine(fake_pool(dir.path(), &ds, "wrong"));
    let mut conn = bad.acquire().await.unwrap();
    let err = conn.server_version().await.unwrap_err();
    assert!(err.to_string().contains("ORA-01017"), "{err}");
}
