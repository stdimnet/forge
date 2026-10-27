use forge_dbmodel::{
    ColumnInfo, DbModel, RoutineInfo, SchemaInfo, SchemaName, TableInfo, TableKind,
};
use forge_sql::Dialect;

use super::*;
use crate::inspect::inspect;

fn col(name: &str, ty: &str) -> ColumnInfo {
    ColumnInfo {
        name: name.into(),
        data_type: ty.into(),
        nullable: true,
        ..Default::default()
    }
}

fn table(name: &str, cols: &[(&str, &str)]) -> TableInfo {
    TableInfo {
        name: name.into(),
        kind: TableKind::Table,
        columns: cols.iter().map(|(n, t)| col(n, t)).collect(),
        ..Default::default()
    }
}

/// `public`: users, orders, order_items, view active_users, function total_of; `audit`: log.
pub(crate) fn shop() -> DbModel {
    let mut m = DbModel::new(Dialect::Postgres);
    m.schemas = vec![
        SchemaName::new("public", true),
        SchemaName::new("audit", false),
        SchemaName::new("archive", false),
    ];
    let mut active = table(
        "active_users",
        &[("id", "integer"), ("email", "varchar(255)")],
    );
    active.kind = TableKind::View;
    m.loaded.insert(
        "public".into(),
        SchemaInfo {
            name: "public".into(),
            tables: vec![
                table(
                    "users",
                    &[
                        ("id", "integer"),
                        ("email", "varchar(255)"),
                        ("name", "text"),
                        ("active", "boolean"),
                        ("created_at", "timestamp"),
                    ],
                ),
                table(
                    "orders",
                    &[
                        ("id", "bigint"),
                        ("user_id", "integer"),
                        ("total", "numeric(10,2)"),
                        ("status", "varchar(20)"),
                    ],
                ),
                table(
                    "order_items",
                    &[("order_id", "bigint"), ("sku", "text"), ("qty", "integer")],
                ),
                active,
            ],
            routines: vec![RoutineInfo {
                name: "total_of".into(),
                kind: "function".into(),
                ..Default::default()
            }],
            ..Default::default()
        },
    );
    m.loaded.insert(
        "audit".into(),
        SchemaInfo {
            name: "audit".into(),
            tables: vec![table("log", &[("id", "integer"), ("message", "text")])],
            ..Default::default()
        },
    );
    m
}

fn run(sql: &str) -> Analysis {
    analyze(sql, SqlDialect::Postgres, Some(&shop()), None)
}

fn problems(sql: &str) -> Vec<(String, String)> {
    let a = run(sql);
    inspect(&a, sql)
        .into_iter()
        .map(|p| (p.id, sql[p.from..p.to].to_string()))
        .collect()
}

fn ids(sql: &str) -> Vec<String> {
    problems(sql).into_iter().map(|p| p.0).collect()
}

fn target_at(sql: &str, needle: &str, nth: usize) -> Option<Target> {
    let at = sql.match_indices(needle).nth(nth).map(|(i, _)| i)?;
    run(sql).ref_at(at)?.target.clone()
}

#[test]
fn resolves_tables_aliases_and_columns() {
    let sql =
        "SELECT u.email, o.total FROM users u JOIN orders o ON o.user_id = u.id WHERE u.active";
    assert!(problems(sql).is_empty(), "{:?}", problems(sql));
    assert_eq!(
        target_at(sql, "total", 0),
        Some(Target::Column {
            schema: "public".into(),
            table: "orders".into(),
            column: "total".into(),
            data_type: "numeric(10,2)".into()
        })
    );
    assert_eq!(
        target_at(sql, "orders", 0),
        Some(Target::Table {
            schema: "public".into(),
            table: "orders".into(),
            view: false
        })
    );
    // The qualifier points at the alias definition.
    let a = run(sql);
    let u = a.defs.iter().find(|d| d.name == "u").unwrap();
    assert_eq!(
        target_at(sql, "u.email", 0),
        Some(Target::Local { def: u.id })
    );
}

#[test]
fn reports_unresolved_tables_columns_and_qualifiers() {
    let p = problems("SELECT u.nope, x.id, missing FROM users u JOIN ordrs o ON true");
    assert!(p.contains(&("SqlResolve".into(), "nope".into())), "{p:?}");
    assert!(p.contains(&("SqlResolve".into(), "x".into())), "{p:?}");
    assert!(p.contains(&("SqlResolve".into(), "ordrs".into())), "{p:?}");
    // `missing` is not reported: `ordrs` has unknown columns.
    assert!(!p.iter().any(|x| x.1 == "missing"), "{p:?}");
    let p = problems("SELECT missing FROM users");
    assert_eq!(p, [("SqlResolve".to_string(), "missing".to_string())]);
}

#[test]
fn schemas_views_and_functions() {
    let sql =
        "SELECT l.message, total_of(a.id) FROM audit.log l, active_users a WHERE a.email <> ''";
    assert!(problems(sql).is_empty(), "{:?}", problems(sql));
    assert_eq!(
        target_at(sql, "audit", 0),
        Some(Target::Schema {
            schema: "audit".into()
        })
    );
    assert_eq!(
        target_at(sql, "total_of", 0),
        Some(Target::Routine {
            schema: "public".into(),
            name: "total_of".into()
        })
    );
    assert_eq!(ids("SELECT * FROM nowhere.t"), ["SqlResolve"]);
    // A known schema that is not loaded: nothing is reported.
    assert!(ids("SELECT x FROM archive.t").is_empty());
    assert_eq!(ids("SELECT * FROM audit.nope"), ["SqlResolve"]);
    // Built-in functions are not in the model and never reported.
    assert!(ids("SELECT now(), count(*), coalesce(name, 'x') FROM users").is_empty());
}

#[test]
fn ctes_and_subqueries() {
    let sql = "WITH big AS (SELECT user_id, sum(total) AS spent FROM orders GROUP BY user_id)
SELECT u.name, b.spent, s.n FROM users u JOIN big b ON b.user_id = u.id
JOIN (SELECT order_id, count(*) n FROM order_items GROUP BY order_id) s ON s.order_id = u.id
ORDER BY spent DESC";
    assert!(problems(sql).is_empty(), "{:?}", problems(sql));
    let a = run(sql);
    let big = a.defs.iter().find(|d| d.name == "big").unwrap();
    assert!(big.used);
    assert_eq!(
        target_at(sql, "spent", 1),
        Some(Target::LocalColumn {
            def: big.id,
            column: "spent".into()
        })
    );
    assert_eq!(
        ids("WITH c AS (SELECT 1 AS a) SELECT b FROM c"),
        ["SqlResolve"]
    );
    assert_eq!(
        ids("WITH c (x) AS (SELECT 1) SELECT x FROM c"),
        Vec::<String>::new()
    );
    // Recursive CTEs see themselves.
    assert!(ids("WITH RECURSIVE t(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM t WHERE n < 5) SELECT n FROM t").is_empty());
    // Correlated subqueries see the outer query.
    assert!(ids(
        "SELECT name FROM users u WHERE EXISTS (SELECT 1 FROM orders o WHERE o.user_id = u.id)"
    )
    .is_empty());
}

#[test]
fn ambiguous_columns_with_qualify_fixes() {
    let sql = "SELECT id FROM users u JOIN orders o ON o.user_id = u.id";
    let a = run(sql);
    let p = inspect(&a, sql);
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].id, "SqlAmbiguousColumn");
    let titles: Vec<&str> = p[0].fixes.iter().map(|f| f.title.as_str()).collect();
    assert_eq!(titles, ["Qualify with 'u'", "Qualify with 'o'"]);
    assert_eq!(p[0].fixes[0].edits[0].text, "u.");
    // USING columns are shared.
    assert!(
        ids("SELECT order_id FROM order_items a JOIN order_items b USING (order_id)")
            .iter()
            .all(|i| i != "SqlAmbiguousColumn")
    );
}

#[test]
fn delete_and_update_without_where() {
    let sql = "DELETE FROM users";
    let a = run(sql);
    let p = inspect(&a, sql);
    assert_eq!(p[0].id, "SqlWithoutWhere");
    let fix = &p[0].fixes[0];
    assert_eq!(fix.title, "Add WHERE clause");
    assert_eq!(fix.edits[0].text, " WHERE FALSE");
    let (s, e) = fix.select.unwrap();
    let applied = format!("{}{}", sql, fix.edits[0].text);
    assert_eq!(&applied[s..e], "FALSE");
    assert_eq!(
        ids("UPDATE users SET name = 'x' RETURNING id"),
        ["SqlWithoutWhere"]
    );
    let sql = "UPDATE users SET name = 'x' RETURNING id";
    let p = inspect(&run(sql), sql);
    let e = &p[0].fixes[0].edits[0];
    assert_eq!(
        format!("{}{}{}", &sql[..e.from], e.text, &sql[e.from..]),
        "UPDATE users SET name = 'x' WHERE FALSE RETURNING id"
    );
    assert!(ids("DELETE FROM users WHERE id = 1").is_empty());
    assert!(
        ids("UPDATE users u SET name = o.status FROM orders o WHERE o.user_id = u.id").is_empty()
    );
}

#[test]
fn unused_alias_and_cte() {
    let sql =
        "WITH unused AS (SELECT 1), used AS (SELECT 2 AS v) SELECT name, v FROM users u, used";
    let a = run(sql);
    let p = inspect(&a, sql);
    let found: Vec<(&str, &str)> = p
        .iter()
        .map(|x| (x.id.as_str(), &sql[x.from..x.to]))
        .collect();
    assert_eq!(found, [("SqlUnusedCte", "unused"), ("SqlUnusedAlias", "u")]);
    // Removing the CTE takes its comma.
    let e = &p[0].fixes[0].edits[0];
    let fixed = format!("{}{}", &sql[..e.from], &sql[e.to..]);
    assert_eq!(
        fixed,
        "WITH used AS (SELECT 2 AS v) SELECT name, v FROM users u, used"
    );
    let e = &p[1].fixes[0].edits[0];
    assert_eq!(
        format!("{}{}", &sql[..e.from], &sql[e.to..]),
        "WITH unused AS (SELECT 1), used AS (SELECT 2 AS v) SELECT name, v FROM users, used"
    );
    // The only CTE: the whole WITH clause goes.
    let sql = "WITH c AS (SELECT 1) SELECT 2";
    let p = inspect(&run(sql), sql);
    let e = &p[0].fixes[0].edits[0];
    assert_eq!(format!("{}{}", &sql[..e.from], &sql[e.to..]), "SELECT 2");
}

#[test]
fn type_mismatch_in_comparisons() {
    let p = problems("SELECT * FROM users WHERE id = 'abc' AND name = 5 AND id = '5' AND active = 'yes' AND created_at > 3");
    let found: Vec<&str> = p
        .iter()
        .filter(|x| x.0 == "SqlType")
        .map(|x| x.1.as_str())
        .collect();
    assert_eq!(found, ["id = 'abc'", "name = 5", "created_at > 3"]);
    let sql = "SELECT * FROM users WHERE name = 5";
    let p = inspect(&run(sql), sql);
    let fix = &p.iter().find(|x| x.id == "SqlType").unwrap().fixes[0];
    assert_eq!(fix.edits[0].text, "'5'");
    assert!(ids("SELECT * FROM users u JOIN orders o ON o.user_id = u.id + 1").is_empty());
}

#[test]
fn redundant_code() {
    let p = problems("SELECT name AS name, CASE WHEN active THEN 1 ELSE NULL END FROM users users ORDER BY name ASC");
    let found: Vec<(&str, &str)> = p.iter().map(|x| (x.0.as_str(), x.1.as_str())).collect();
    assert_eq!(
        found,
        [
            ("SqlRedundantAlias", "name"),
            ("SqlRedundantElseNull", "ELSE NULL"),
            ("SqlRedundantAlias", "users"),
            ("SqlRedundantOrderingDirection", "ASC")
        ]
    );
}

#[test]
fn insert_update_delete_targets() {
    assert!(ids("INSERT INTO users (email, name) VALUES ('a', 'b') ON CONFLICT (email) DO UPDATE SET name = excluded.name RETURNING id").is_empty());
    assert_eq!(ids("INSERT INTO users (mail) VALUES ('a')"), ["SqlResolve"]);
    assert!(ids("INSERT INTO orders (user_id, total) SELECT u.id, 0 FROM users u JOIN active_users a ON a.id = u.id").is_empty());
    assert_eq!(ids("UPDATE users SET nme = 1 WHERE id = 2"), ["SqlResolve"]);
    assert!(ids("DELETE FROM orders o USING users u WHERE o.user_id = u.id").is_empty());
    assert_eq!(ids("ALTER TABLE users DROP COLUMN nope"), ["SqlResolve"]);
    assert!(ids("DROP TABLE IF EXISTS nope").is_empty());
    assert_eq!(ids("TRUNCATE nope"), ["SqlResolve"]);
    assert!(ids("CREATE INDEX i ON orders (user_id, status)").is_empty());
    assert!(ids("CREATE VIEW v AS SELECT id FROM users").is_empty());
}

#[test]
fn order_by_and_group_by_aliases() {
    assert!(ids(
        "SELECT count(*) AS n, status FROM orders GROUP BY status HAVING n > 1 ORDER BY n"
    )
    .is_empty());
    let sql = "SELECT count(*) AS n FROM orders ORDER BY n";
    let a = run(sql);
    let n = a.defs.iter().find(|d| d.name == "n").unwrap();
    let last = a
        .ref_at(sql.rfind('n').unwrap())
        .and_then(|r| r.target.clone());
    assert_eq!(last, Some(Target::Local { def: n.id }));
}

#[test]
fn without_model_only_script_names() {
    let sql = "SELECT x.id, y FROM users u WHERE u.id = 1";
    let a = analyze(sql, SqlDialect::Generic, None, None);
    let p: Vec<String> = inspect(&a, sql)
        .into_iter()
        .map(|p| sql[p.from..p.to].to_string())
        .collect();
    assert_eq!(p, ["x"]);
}

#[test]
fn tolerates_half_typed_queries() {
    for sql in [
        "SELECT",
        "SELECT * FROM",
        "SELECT u. FROM users u",
        "SELECT * FROM users WHERE",
        "UPDATE",
        "WITH x AS (",
        "INSERT INTO users (",
        "SELECT (SELECT",
        "DELETE FROM users u WHERE u.",
    ] {
        let _ = problems(sql);
    }
}

#[test]
fn soft_keywords_and_casts() {
    assert!(ids("SELECT extract(YEAR FROM created_at), created_at::date, CAST(id AS text), interval '1 day', DATE '2020-01-01' FROM users").is_empty());
    assert!(ids(
        "SELECT row_number() OVER w FROM users WINDOW w AS (PARTITION BY active ORDER BY id DESC)"
    )
    .is_empty());
}

#[test]
fn mysql_and_sqlserver_quirks() {
    let my = shop();
    let a = analyze(
        "SELECT `u`.`email` FROM `users` `u` WHERE `u`.id = 1",
        SqlDialect::MySql,
        Some(&my),
        Some("public"),
    );
    assert!(inspect(&a, "").is_empty() || a.refs.iter().all(|r| r.state != State::Unresolved));
    let sql = "SELECT TOP 10 [u].[email] FROM [users] AS [u]";
    let a = analyze(sql, SqlDialect::SqlServer, Some(&my), Some("public"));
    assert!(
        a.refs.iter().all(|r| r.state == State::Resolved),
        "{:?}",
        a.refs
    );
}
