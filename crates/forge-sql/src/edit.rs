//! Row editing from the data grid: pending inserts, updates and deletes become
//! `INSERT`/`UPDATE`/`DELETE` statements that identify rows by their primary key (or a
//! unique key, see `forge-dataedit`). A value `{"$default": true}` sets the column default.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dialect::Dialect;
use crate::value::{is_default, sql_literal};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Insert,
    Update,
    Delete,
}

/// One pending change of the grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowChange {
    pub kind: ChangeKind,
    /// Original values of the key columns (update/delete).
    #[serde(default)]
    pub key: BTreeMap<String, Value>,
    /// New column values (insert/update). Columns left out of an insert get their default.
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
}

/// Why a change cannot become SQL (e.g. a table without a primary key).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused(pub String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}

type DbResult<T> = Result<T, Refused>;

/// SQL for one change, or an error when the row cannot be identified. `pk` are the columns
/// that identify a row: the primary key or a unique key.
pub fn change_sql(
    d: Dialect,
    schema: &str,
    table: &str,
    pk: &[String],
    change: &RowChange,
) -> DbResult<String> {
    let target = d.qualified(schema, table);
    let where_pk = || -> DbResult<String> {
        if pk.is_empty() {
            return Err(Refused(format!(
                "Table {table} has no primary key or unique key, so its rows cannot be identified. Edit it with an UPDATE statement in a console instead."
            )));
        }
        pk.iter()
            .map(|c| match change.key.get(c) {
                Some(Value::Null) => Ok(format!("{} IS NULL", d.quote_ident(c))),
                Some(v) => Ok(format!("{} = {}", d.quote_ident(c), sql_literal(d, v))),
                None => Err(Refused(format!("missing key value for {c}"))),
            })
            .collect::<DbResult<Vec<_>>>()
            .map(|parts| parts.join(" AND "))
    };
    match change.kind {
        ChangeKind::Insert => {
            // Columns set to DEFAULT are simply left out.
            let values: BTreeMap<_, _> = change
                .values
                .iter()
                .filter(|(_, v)| !is_default(v))
                .collect();
            if values.is_empty() {
                return Ok(match d {
                    Dialect::MySql => format!("INSERT INTO {target} () VALUES ()"),
                    _ => format!("INSERT INTO {target} DEFAULT VALUES"),
                });
            }
            let cols: Vec<_> = values.keys().map(|c| d.quote_ident(c)).collect();
            let vals: Vec<_> = values.values().map(|v| sql_literal(d, v)).collect();
            Ok(format!(
                "INSERT INTO {target} ({}) VALUES ({})",
                cols.join(", "),
                vals.join(", ")
            ))
        }
        ChangeKind::Update => {
            let w = where_pk()?;
            if change.values.is_empty() {
                return Err(Refused("nothing to update".into()));
            }
            if d == Dialect::Sqlite && change.values.values().any(is_default) {
                return Err(Refused(
                    "SQLite cannot set a column to DEFAULT in an UPDATE; enter the value instead"
                        .into(),
                ));
            }
            let sets: Vec<_> = change
                .values
                .iter()
                .map(|(c, v)| format!("{} = {}", d.quote_ident(c), sql_literal(d, v)))
                .collect();
            Ok(format!("UPDATE {target} SET {} WHERE {w}", sets.join(", ")))
        }
        ChangeKind::Delete => Ok(format!("DELETE FROM {target} WHERE {}", where_pk()?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn builds_statements() {
        let pk = vec!["id".to_string()];
        let upd = RowChange {
            kind: ChangeKind::Update,
            key: map(&[("id", json!(7))]),
            values: map(&[("email", json!("a'b@x.io")), ("deleted_at", Value::Null)]),
        };
        assert_eq!(
            change_sql(Dialect::MySql, "shop", "users", &pk, &upd).unwrap(),
            "UPDATE `shop`.`users` SET `deleted_at` = NULL, `email` = 'a''b@x.io' WHERE `id` = 7"
        );
        let del = RowChange {
            kind: ChangeKind::Delete,
            key: map(&[("id", json!("9007199254740993"))]),
            values: BTreeMap::new(),
        };
        assert_eq!(
            change_sql(Dialect::Postgres, "public", "users", &pk, &del).unwrap(),
            "DELETE FROM \"public\".\"users\" WHERE \"id\" = '9007199254740993'"
        );
        let ins = RowChange {
            kind: ChangeKind::Insert,
            key: BTreeMap::new(),
            values: map(&[("name", json!("Ada"))]),
        };
        assert_eq!(
            change_sql(Dialect::Sqlite, "main", "users", &pk, &ins).unwrap(),
            "INSERT INTO \"users\" (\"name\") VALUES ('Ada')"
        );
    }

    #[test]
    fn default_marker_sets_or_omits_the_default() {
        let pk = vec!["id".to_string()];
        let upd = RowChange {
            kind: ChangeKind::Update,
            key: map(&[("id", json!(1))]),
            values: map(&[("status", crate::value::default_marker())]),
        };
        assert_eq!(
            change_sql(Dialect::Postgres, "public", "orders", &pk, &upd).unwrap(),
            "UPDATE \"public\".\"orders\" SET \"status\" = DEFAULT WHERE \"id\" = 1"
        );
        assert!(change_sql(Dialect::Sqlite, "main", "orders", &pk, &upd).is_err());
        let ins = RowChange {
            kind: ChangeKind::Insert,
            key: BTreeMap::new(),
            values: map(&[
                ("status", crate::value::default_marker()),
                ("total", json!("9.50")),
            ]),
        };
        assert_eq!(
            change_sql(Dialect::MySql, "shop", "orders", &pk, &ins).unwrap(),
            "INSERT INTO `shop`.`orders` (`total`) VALUES ('9.50')"
        );
    }

    #[test]
    fn refuses_without_primary_key() {
        let upd = RowChange {
            kind: ChangeKind::Update,
            key: BTreeMap::new(),
            values: map(&[("a", json!(1))]),
        };
        let err = change_sql(Dialect::MySql, "s", "logs", &[], &upd).unwrap_err();
        assert!(err.to_string().contains("no primary key"));
        // Inserts do not need a key.
        let ins = RowChange {
            kind: ChangeKind::Insert,
            ..upd
        };
        assert!(change_sql(Dialect::MySql, "s", "logs", &[], &ins).is_ok());
    }
}
