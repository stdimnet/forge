//! Foreign-key navigation of the data editor (Ctrl+B on a cell): from a row to the row its
//! foreign key references, and from a row to the rows that reference it. Works the same for
//! real and virtual foreign keys. The result is the target table and the condition its
//! data editor opens with.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use forge_sql::{Dialect, Refused};

use crate::query::ColumnFilter;

/// A foreign key as the model describes it (`forge_dbmodel::ForeignKey`, also virtual).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FkRef {
    #[serde(default)]
    pub name: String,
    pub columns: Vec<String>,
    /// Empty: the schema of the referencing table.
    #[serde(default)]
    pub ref_schema: String,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
    #[serde(default)]
    pub r#virtual: bool,
}

/// Where a navigation goes: open `schema.table` filtered by `filter`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Navigation {
    pub schema: String,
    pub table: String,
    pub filters: Vec<ColumnFilter>,
    /// The `WHERE` text for the filter row, in the dialect.
    pub filter: String,
    /// Menu label, e.g. `orders.user_id → users`.
    pub label: String,
}

/// Value of `column` in a row given as columns and values.
fn cell<'a>(columns: &[String], row: &'a [Value], column: &str) -> Result<&'a Value, Refused> {
    columns
        .iter()
        .position(|c| c == column)
        .and_then(|i| row.get(i))
        .ok_or_else(|| Refused(format!("The column {column} is not in the result")))
}

fn navigation(
    d: Dialect,
    schema: String,
    table: String,
    filters: Vec<ColumnFilter>,
    label: String,
) -> Navigation {
    let filter = filters
        .iter()
        .map(|f| f.sql(d))
        .collect::<Vec<_>>()
        .join(" AND ");
    Navigation {
        schema,
        table,
        filters,
        filter,
        label,
    }
}

/// The row `fk` of the table in `schema` references from `row`. Refused when a key column
/// is NULL (nothing is referenced).
pub fn referenced_row(
    d: Dialect,
    schema: &str,
    table: &str,
    fk: &FkRef,
    columns: &[String],
    row: &[Value],
) -> Result<Navigation, Refused> {
    if fk.columns.len() != fk.ref_columns.len() || fk.columns.is_empty() {
        return Err(Refused(format!("The foreign key {} is incomplete", fk.name)));
    }
    let mut filters = Vec::new();
    for (c, rc) in fk.columns.iter().zip(&fk.ref_columns) {
        let v = cell(columns, row, c)?;
        if v.is_null() {
            return Err(Refused(format!("{c} is NULL, so it references no row")));
        }
        filters.push(ColumnFilter::eq(rc, v.clone()));
    }
    let ref_schema = if fk.ref_schema.is_empty() {
        schema
    } else {
        &fk.ref_schema
    };
    Ok(navigation(
        d,
        ref_schema.to_string(),
        fk.ref_table.clone(),
        filters,
        format!("{table}.{} → {}", fk.columns.join(", "), fk.ref_table),
    ))
}

/// The rows of `from_schema.from_table` whose foreign key `fk` references `row` (a row of
/// the referenced table, given as columns and values).
pub fn referencing_rows(
    d: Dialect,
    from_schema: &str,
    from_table: &str,
    fk: &FkRef,
    columns: &[String],
    row: &[Value],
) -> Result<Navigation, Refused> {
    if fk.columns.len() != fk.ref_columns.len() || fk.columns.is_empty() {
        return Err(Refused(format!("The foreign key {} is incomplete", fk.name)));
    }
    let mut filters = Vec::new();
    for (c, rc) in fk.columns.iter().zip(&fk.ref_columns) {
        let v = cell(columns, row, rc)?;
        if v.is_null() {
            return Err(Refused(format!("{rc} is NULL, so no row references it")));
        }
        filters.push(ColumnFilter::eq(c, v.clone()));
    }
    Ok(navigation(
        d,
        from_schema.to_string(),
        from_table.to_string(),
        filters,
        format!("{from_table}.{} → {}", fk.columns.join(", "), fk.ref_table),
    ))
}

/// The foreign keys of a table, for [`incoming`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableKeys {
    pub schema: String,
    pub table: String,
    pub foreign_keys: Vec<FkRef>,
}

/// A foreign key of another table that references the target table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Incoming {
    pub schema: String,
    pub table: String,
    pub fk: FkRef,
}

/// Foreign keys among `tables` that reference `schema.table` ("Find referencing rows").
pub fn incoming(schema: &str, table: &str, tables: &[TableKeys]) -> Vec<Incoming> {
    tables
        .iter()
        .flat_map(|t| {
            t.foreign_keys
                .iter()
                .filter(|fk| {
                    let rs = if fk.ref_schema.is_empty() {
                        &t.schema
                    } else {
                        &fk.ref_schema
                    };
                    rs == schema && fk.ref_table == table
                })
                .map(|fk| Incoming {
                    schema: t.schema.clone(),
                    table: t.table.clone(),
                    fk: fk.clone(),
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fk(cols: &[&str], ref_table: &str, ref_cols: &[&str]) -> FkRef {
        FkRef {
            name: "fk".into(),
            columns: cols.iter().map(|s| s.to_string()).collect(),
            ref_schema: String::new(),
            ref_table: ref_table.into(),
            ref_columns: ref_cols.iter().map(|s| s.to_string()).collect(),
            r#virtual: false,
        }
    }

    fn cols(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn jumps_to_the_referenced_row() {
        let nav = referenced_row(
            Dialect::MySql,
            "shop",
            "orders",
            &fk(&["user_id"], "users", &["id"]),
            &cols(&["id", "user_id"]),
            &[json!(10), json!(7)],
        )
        .unwrap();
        assert_eq!((nav.schema.as_str(), nav.table.as_str()), ("shop", "users"));
        assert_eq!(nav.filter, "`id` = 7");
        assert_eq!(nav.label, "orders.user_id → users");
        // Composite and string keys, other dialect.
        let nav = referenced_row(
            Dialect::Postgres,
            "public",
            "lines",
            &FkRef {
                ref_schema: "sales".into(),
                ..fk(&["order_no", "region"], "orders", &["no", "region"])
            },
            &cols(&["order_no", "region"]),
            &[json!("A-1"), json!("EU")],
        )
        .unwrap();
        assert_eq!(nav.schema, "sales");
        assert_eq!(nav.filter, "\"no\" = 'A-1' AND \"region\" = 'EU'");
        let err = referenced_row(
            Dialect::MySql,
            "shop",
            "orders",
            &fk(&["user_id"], "users", &["id"]),
            &cols(&["user_id"]),
            &[Value::Null],
        )
        .unwrap_err();
        assert!(err.0.contains("NULL"));
    }

    #[test]
    fn finds_referencing_rows() {
        let nav = referencing_rows(
            Dialect::MySql,
            "shop",
            "orders",
            &fk(&["user_id"], "users", &["id"]),
            &cols(&["id", "name"]),
            &[json!(7), json!("Ada")],
        )
        .unwrap();
        assert_eq!(nav.table, "orders");
        assert_eq!(nav.filter, "`user_id` = 7");
        let tables = vec![
            TableKeys {
                schema: "shop".into(),
                table: "orders".into(),
                foreign_keys: vec![fk(&["user_id"], "users", &["id"])],
            },
            TableKeys {
                schema: "shop".into(),
                table: "order_items".into(),
                foreign_keys: vec![
                    fk(&["order_id"], "orders", &["id"]),
                    FkRef {
                        r#virtual: true,
                        ..fk(&["created_by"], "users", &["id"])
                    },
                ],
            },
            TableKeys {
                schema: "other".into(),
                table: "x".into(),
                foreign_keys: vec![fk(&["user_id"], "users", &["id"])],
            },
        ];
        let inc = incoming("shop", "users", &tables);
        assert_eq!(inc.len(), 2);
        assert_eq!(inc[1].table, "order_items");
        assert!(inc[1].fk.r#virtual);
    }
}
