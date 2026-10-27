//! Compare data: two result sets or tables diffed by key into added, removed, changed and
//! unchanged rows, with the changed cells. Without a key, whole rows are matched (as a
//! multiset), so only added and removed rows exist.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use forge_sql::Refused;

use crate::{as_number, same_cell, text};

/// Columns and rows of one side.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSet {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RowStatus {
    Same,
    Changed,
    /// Only on the right side.
    Added,
    /// Only on the left side.
    Removed,
}

/// One row of the diff; `left` and `right` are aligned to [`DataDiff::columns`] (NULL for a
/// column the side does not have).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffRow {
    pub status: RowStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<Vec<Value>>,
    /// Indexes into `columns` of the cells that differ (changed rows).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub changed: Vec<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataDiff {
    /// Left columns, then the columns only the right side has.
    pub columns: Vec<String>,
    pub key: Vec<String>,
    pub rows: Vec<DiffRow>,
    pub same: usize,
    pub changed: usize,
    pub added: usize,
    pub removed: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub left_only_columns: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub right_only_columns: Vec<String>,
    /// Keys that occur more than once on a side (matched in order).
    pub duplicate_keys: usize,
}

/// A hashable form of a cell: numbers by value (`19.9` = `"19.90"`).
fn norm(v: &Value) -> String {
    match v {
        Value::Null => "\u{0}null".into(),
        _ => match as_number(v) {
            Some(n) => format!("\u{1}{n}"),
            None => format!("\u{2}{}", text(v)),
        },
    }
}

/// Diff `left` and `right` by the `key` columns (both sides must have them).
pub fn compare(left: &DataSet, right: &DataSet, key: &[String]) -> Result<DataDiff, Refused> {
    let mut columns = left.columns.clone();
    let right_only: Vec<String> = right
        .columns
        .iter()
        .filter(|c| !left.columns.contains(c))
        .cloned()
        .collect();
    columns.extend(right_only.iter().cloned());
    let left_only: Vec<String> = left
        .columns
        .iter()
        .filter(|c| !right.columns.contains(c))
        .cloned()
        .collect();
    for k in key {
        if !left.columns.contains(k) || !right.columns.contains(k) {
            return Err(Refused(format!(
                "The key column {k} is not in both data sets"
            )));
        }
    }
    let align = |set: &DataSet, row: &[Value]| -> Vec<Value> {
        columns
            .iter()
            .map(|c| {
                set.columns
                    .iter()
                    .position(|x| x == c)
                    .and_then(|i| row.get(i).cloned())
                    .unwrap_or(Value::Null)
            })
            .collect()
    };
    let shared: Vec<usize> = columns
        .iter()
        .enumerate()
        .filter(|(_, c)| left.columns.contains(c) && right.columns.contains(c))
        .map(|(i, _)| i)
        .collect();
    // Whole rows are the key when none is given.
    let key_idx: Vec<usize> = if key.is_empty() {
        shared.clone()
    } else {
        key.iter()
            .map(|k| columns.iter().position(|c| c == k).unwrap_or(0))
            .collect()
    };
    let key_of = |row: &[Value]| -> String {
        key_idx
            .iter()
            .map(|&i| norm(&row[i]))
            .collect::<Vec<_>>()
            .join("\u{3}")
    };
    let lefts: Vec<Vec<Value>> = left.rows.iter().map(|r| align(left, r)).collect();
    let rights: Vec<Vec<Value>> = right.rows.iter().map(|r| align(right, r)).collect();
    let mut index: HashMap<String, VecDeque<usize>> = HashMap::new();
    let mut duplicates = 0;
    for (i, r) in rights.iter().enumerate() {
        let q = index.entry(key_of(r)).or_default();
        if !q.is_empty() && !key.is_empty() {
            duplicates += 1;
        }
        q.push_back(i);
    }
    let mut used = vec![false; rights.len()];
    let mut diff = DataDiff {
        columns,
        key: key.to_vec(),
        left_only_columns: left_only,
        right_only_columns: right_only,
        ..Default::default()
    };
    let mut seen_left: HashMap<String, usize> = HashMap::new();
    for l in lefts {
        let k = key_of(&l);
        let n = seen_left.entry(k.clone()).or_default();
        if *n > 0 && !key.is_empty() {
            duplicates += 1;
        }
        *n += 1;
        match index.get_mut(&k).and_then(|q| q.pop_front()) {
            Some(j) => {
                used[j] = true;
                let r = rights[j].clone();
                let changed: Vec<usize> = shared
                    .iter()
                    .copied()
                    .filter(|&i| !same_cell(&l[i], &r[i]) && !(l[i].is_null() && r[i].is_null()))
                    .collect();
                let status = if changed.is_empty() {
                    diff.same += 1;
                    RowStatus::Same
                } else {
                    diff.changed += 1;
                    RowStatus::Changed
                };
                diff.rows.push(DiffRow {
                    status,
                    left: Some(l),
                    right: Some(r),
                    changed,
                });
            }
            None => {
                diff.removed += 1;
                diff.rows.push(DiffRow {
                    status: RowStatus::Removed,
                    left: Some(l),
                    right: None,
                    changed: Vec::new(),
                });
            }
        }
    }
    for (j, r) in rights.into_iter().enumerate() {
        if !used[j] {
            diff.added += 1;
            diff.rows.push(DiffRow {
                status: RowStatus::Added,
                left: None,
                right: Some(r),
                changed: Vec::new(),
            });
        }
    }
    diff.duplicate_keys = duplicates;
    Ok(diff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn set(columns: &[&str], rows: Vec<Vec<Value>>) -> DataSet {
        DataSet {
            columns: columns.iter().map(|s| s.to_string()).collect(),
            rows,
        }
    }

    #[test]
    fn diffs_by_key() {
        let left = set(
            &["id", "name", "price"],
            vec![
                vec![json!(1), json!("Hoodie"), json!("59.00")],
                vec![json!(2), json!("Mug"), json!("14.50")],
                vec![json!(3), json!("Shirt"), json!("24.99")],
            ],
        );
        let right = set(
            &["id", "name", "price", "stock"],
            vec![
                vec![json!(1), json!("Hoodie"), json!(59), json!(3)],
                vec![json!(3), json!("T-Shirt"), json!("24.99"), json!(0)],
                vec![json!(4), json!("Cap"), json!("9.00"), Value::Null],
            ],
        );
        let d = compare(&left, &right, &["id".into()]).unwrap();
        assert_eq!(d.columns, vec!["id", "name", "price", "stock"]);
        assert_eq!(d.right_only_columns, vec!["stock"]);
        assert_eq!((d.same, d.changed, d.added, d.removed), (1, 1, 1, 1));
        assert_eq!(d.rows[0].status, RowStatus::Same); // 59.00 = 59
        assert_eq!(d.rows[1].status, RowStatus::Removed);
        assert_eq!(d.rows[2].status, RowStatus::Changed);
        assert_eq!(d.rows[2].changed, vec![1]);
        assert_eq!(d.rows[3].status, RowStatus::Added);
        assert_eq!(d.rows[3].right.as_ref().unwrap()[1], json!("Cap"));
        // The left side has no stock column: aligned as NULL.
        assert_eq!(d.rows[0].left.as_ref().unwrap()[3], Value::Null);
    }

    #[test]
    fn composite_keys_nulls_and_duplicates() {
        let left = set(
            &["a", "b", "v"],
            vec![
                vec![json!(1), Value::Null, json!("x")],
                vec![json!(1), json!(2), json!("y")],
                vec![json!(1), json!(2), json!("z")],
            ],
        );
        let right = set(
            &["a", "b", "v"],
            vec![
                vec![json!(1), Value::Null, Value::Null],
                vec![json!(1), json!(2), json!("y")],
            ],
        );
        let d = compare(&left, &right, &["a".into(), "b".into()]).unwrap();
        assert_eq!(d.rows[0].status, RowStatus::Changed); // NULL keys match each other
        assert_eq!(d.rows[0].changed, vec![2]);
        assert_eq!(d.rows[1].status, RowStatus::Same);
        assert_eq!(d.rows[2].status, RowStatus::Removed);
        assert_eq!(d.duplicate_keys, 1);
        assert!(compare(&left, &right, &["missing".into()]).is_err());
    }

    #[test]
    fn without_key_matches_whole_rows() {
        let left = set(&["v"], vec![vec![json!(1)], vec![json!(1)], vec![json!(2)]]);
        let right = set(&["v"], vec![vec![json!(1)], vec![json!(3)]]);
        let d = compare(&left, &right, &[]).unwrap();
        assert_eq!((d.same, d.changed, d.added, d.removed), (1, 0, 1, 2));
        assert_eq!(d.duplicate_keys, 0);
    }
}
