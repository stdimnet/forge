//! Pending changes of the data editor: which key identifies a row, and the DML preview
//! (`INSERT`/`UPDATE`/`DELETE`) shown before Submit. Tables without a primary or usable
//! unique key refuse updates and deletes.

use serde::{Deserialize, Serialize};

use forge_sql::{change_sql, ChangeKind, Dialect, Refused, RowChange};

/// A unique index of the table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UniqueKey {
    #[serde(default)]
    pub name: String,
    pub columns: Vec<String>,
}

/// The key that identifies rows of a table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyChoice {
    pub columns: Vec<String>,
    /// `primary key` or the unique index name.
    pub source: String,
}

/// The primary key, else the first unique key whose columns are all `NOT NULL` (a unique
/// key over nullable columns may match several rows with NULLs).
pub fn choose_key(
    primary_key: &[String],
    unique: &[UniqueKey],
    nullable: &dyn Fn(&str) -> bool,
) -> Option<KeyChoice> {
    if !primary_key.is_empty() {
        return Some(KeyChoice {
            columns: primary_key.to_vec(),
            source: "primary key".into(),
        });
    }
    unique
        .iter()
        .find(|u| !u.columns.is_empty() && u.columns.iter().all(|c| !nullable(c)))
        .map(|u| KeyChoice {
            columns: u.columns.clone(),
            source: if u.name.is_empty() {
                "unique key".into()
            } else {
                format!("unique key {}", u.name)
            },
        })
}

/// The statements for `changes`, in order: updates, deletes, inserts as the grid lists
/// them. Refused (with the reason) when a change needs a key the table does not have.
pub fn preview(
    d: Dialect,
    schema: &str,
    table: &str,
    key: Option<&KeyChoice>,
    changes: &[RowChange],
) -> Result<Vec<String>, Refused> {
    let cols = key.map(|k| k.columns.clone()).unwrap_or_default();
    changes
        .iter()
        .map(|c| {
            if c.kind == ChangeKind::Update || c.kind == ChangeKind::Delete {
                if let Some(k) = key {
                    for col in &k.columns {
                        if !c.key.contains_key(col) {
                            return Err(Refused(format!(
                                "The {} column {col} is not in the result, so the row cannot be identified",
                                k.source
                            )));
                        }
                    }
                }
            }
            change_sql(d, schema, table, &cols, c)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::collections::BTreeMap;

    fn map(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn chooses_primary_then_not_null_unique_key() {
        let u = vec![
            UniqueKey {
                name: "users_nick_unique".into(),
                columns: vec!["nick".into()],
            },
            UniqueKey {
                name: "users_email_unique".into(),
                columns: vec!["email".into()],
            },
        ];
        let nullable = |c: &str| c == "nick";
        assert_eq!(
            choose_key(&["id".into()], &u, &nullable).unwrap().source,
            "primary key"
        );
        let k = choose_key(&[], &u, &nullable).unwrap();
        assert_eq!(k.columns, vec!["email".to_string()]);
        assert_eq!(k.source, "unique key users_email_unique");
        assert!(choose_key(&[], &u[..1], &nullable).is_none());
    }

    #[test]
    fn composite_keys_and_nulls() {
        let key = KeyChoice {
            columns: vec!["order_id".into(), "line".into()],
            source: "primary key".into(),
        };
        let changes = vec![
            RowChange {
                kind: ChangeKind::Update,
                key: map(&[("order_id", json!(7)), ("line", json!(2))]),
                values: map(&[("note", Value::Null), ("qty", json!(3))]),
            },
            RowChange {
                kind: ChangeKind::Delete,
                key: map(&[("order_id", json!(7)), ("line", Value::Null)]),
                values: BTreeMap::new(),
            },
            RowChange {
                kind: ChangeKind::Insert,
                key: BTreeMap::new(),
                values: map(&[
                    ("order_id", json!(7)),
                    ("line", json!(3)),
                    ("note", Value::Null),
                ]),
            },
        ];
        assert_eq!(
            preview(Dialect::Postgres, "public", "lines", Some(&key), &changes).unwrap(),
            vec![
                "UPDATE \"public\".\"lines\" SET \"note\" = NULL, \"qty\" = 3 WHERE \"order_id\" = 7 AND \"line\" = 2",
                "DELETE FROM \"public\".\"lines\" WHERE \"order_id\" = 7 AND \"line\" IS NULL",
                "INSERT INTO \"public\".\"lines\" (\"line\", \"note\", \"order_id\") VALUES (3, NULL, 7)",
            ]
        );
        assert_eq!(
            preview(Dialect::SqlServer, "dbo", "lines", Some(&key), &changes[..1]).unwrap(),
            vec!["UPDATE [dbo].[lines] SET [note] = NULL, [qty] = 3 WHERE [order_id] = 7 AND [line] = 2"]
        );
    }

    #[test]
    fn refuses_without_key() {
        let upd = RowChange {
            kind: ChangeKind::Update,
            key: BTreeMap::new(),
            values: map(&[("a", json!(1))]),
        };
        let err = preview(Dialect::MySql, "shop", "audit_log", None, &[upd.clone()]).unwrap_err();
        assert!(err.0.contains("no primary key or unique key"), "{err}");
        let key = KeyChoice {
            columns: vec!["id".into()],
            source: "primary key".into(),
        };
        let err = preview(Dialect::MySql, "shop", "t", Some(&key), &[upd]).unwrap_err();
        assert!(err.0.contains("not in the result"), "{err}");
        let ins = RowChange {
            kind: ChangeKind::Insert,
            key: BTreeMap::new(),
            values: BTreeMap::new(),
        };
        assert_eq!(
            preview(Dialect::MySql, "shop", "audit_log", None, &[ins]).unwrap(),
            vec!["INSERT INTO `shop`.`audit_log` () VALUES ()"]
        );
    }
}
