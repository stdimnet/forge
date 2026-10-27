//! Sort and filter in memory, for result sets of arbitrary queries (a console result cannot
//! be re-queried with another `ORDER BY`/`WHERE` safely), CSV files and data frames.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::query::{ColumnFilter, FilterOp, SortKey};
use crate::{as_number, cmp_cells, same_cell, text};

/// What to show of `rows`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalView {
    #[serde(default)]
    pub order: Vec<SortKey>,
    #[serde(default)]
    pub filters: Vec<ColumnFilter>,
    /// Rows containing this text in any cell (case-insensitive).
    #[serde(default)]
    pub search: String,
}

/// Indexes of the rows to show, in display order. Sorting is stable, so equal rows keep
/// their original order.
pub fn local_view(columns: &[String], rows: &[Vec<Value>], view: &LocalView) -> Vec<usize> {
    let col = |name: &str| columns.iter().position(|c| c == name);
    let filters: Vec<(Option<usize>, &ColumnFilter)> =
        view.filters.iter().map(|f| (col(&f.column), f)).collect();
    let search = view.search.trim().to_lowercase();
    let mut out: Vec<usize> = (0..rows.len())
        .filter(|&r| {
            let row = &rows[r];
            filters.iter().all(|(i, f)| {
                let v = i.and_then(|i| row.get(i)).unwrap_or(&Value::Null);
                matches(v, f)
            }) && (search.is_empty()
                || row.iter().any(|v| text(v).to_lowercase().contains(&search)))
        })
        .collect();
    let keys: Vec<(usize, bool)> = view
        .order
        .iter()
        .filter_map(|k| col(&k.column).map(|i| (i, k.desc)))
        .collect();
    if !keys.is_empty() {
        out.sort_by(|&a, &b| {
            for &(i, desc) in &keys {
                let null = Value::Null;
                let x = rows[a].get(i).unwrap_or(&null);
                let y = rows[b].get(i).unwrap_or(&null);
                let c = cmp_cells(x, y);
                if c.is_ne() {
                    return if desc { c.reverse() } else { c };
                }
            }
            std::cmp::Ordering::Equal
        });
    }
    out
}

/// Whether a cell passes a column filter (SQL semantics: comparisons with NULL fail).
pub fn matches(v: &Value, f: &ColumnFilter) -> bool {
    use std::cmp::Ordering::*;
    match f.op {
        FilterOp::IsNull => v.is_null(),
        FilterOp::IsNotNull => !v.is_null(),
        FilterOp::Eq if f.value.is_null() => v.is_null(),
        FilterOp::Ne if f.value.is_null() => !v.is_null(),
        _ if v.is_null() => false,
        FilterOp::Eq => same_cell(v, &f.value),
        FilterOp::Ne => !same_cell(v, &f.value),
        FilterOp::Lt => !f.value.is_null() && cmp_cells(v, &f.value) == Less,
        FilterOp::Le => !f.value.is_null() && cmp_cells(v, &f.value) != Greater,
        FilterOp::Gt => !f.value.is_null() && cmp_cells(v, &f.value) == Greater,
        FilterOp::Ge => !f.value.is_null() && cmp_cells(v, &f.value) != Less,
        FilterOp::Like => like(&text(v), &text(&f.value)),
        FilterOp::NotLike => !like(&text(v), &text(&f.value)),
        FilterOp::In => f.values.iter().any(|x| same_cell(v, x)),
    }
}

/// SQL `LIKE` (`%`, `_`), case-insensitive like MySQL's default collations.
fn like(s: &str, pattern: &str) -> bool {
    let s: Vec<char> = s.to_lowercase().chars().collect();
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    // Iterative wildcard match with backtracking to the last `%`.
    let (mut i, mut j) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while i < s.len() {
        if j < p.len() && (p[j] == '_' || p[j] == s[i]) {
            i += 1;
            j += 1;
        } else if j < p.len() && p[j] == '%' {
            star = Some((j, i));
            j += 1;
        } else if let Some((sj, si)) = star {
            j = sj + 1;
            i = si + 1;
            star = Some((sj, si + 1));
        } else {
            return false;
        }
    }
    while j < p.len() && p[j] == '%' {
        j += 1;
    }
    j == p.len()
}

/// Whether a column holds numbers only (ignoring NULLs), e.g. to right-align or chart it.
pub fn is_numeric(rows: &[Vec<Value>], col: usize) -> bool {
    let mut any = false;
    for r in rows {
        match r.get(col) {
            None | Some(Value::Null) => {}
            Some(v) if as_number(v).is_some() => any = true,
            _ => return false,
        }
    }
    any
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn data() -> (Vec<String>, Vec<Vec<Value>>) {
        (
            vec!["id".into(), "status".into(), "total".into()],
            vec![
                vec![json!(1), json!("paid"), json!("19.90")],
                vec![json!(2), json!("pending"), json!("120.00")],
                vec![json!(3), Value::Null, json!("5.00")],
                vec![json!(4), json!("paid"), json!("120.00")],
            ],
        )
    }

    #[test]
    fn sorts_numerically_and_stably() {
        let (c, r) = data();
        let v = LocalView {
            order: vec![SortKey {
                column: "total".into(),
                desc: true,
            }],
            ..Default::default()
        };
        assert_eq!(local_view(&c, &r, &v), vec![1, 3, 0, 2]);
        let v = LocalView {
            order: vec![
                SortKey {
                    column: "status".into(),
                    desc: false,
                },
                SortKey {
                    column: "id".into(),
                    desc: true,
                },
            ],
            ..Default::default()
        };
        // NULL first, then text; ties broken by the second key.
        assert_eq!(local_view(&c, &r, &v), vec![2, 3, 0, 1]);
    }

    #[test]
    fn filters_by_value_and_text() {
        let (c, r) = data();
        let v = LocalView {
            filters: vec![ColumnFilter::eq("status", json!("paid"))],
            ..Default::default()
        };
        assert_eq!(local_view(&c, &r, &v), vec![0, 3]);
        let v = LocalView {
            filters: vec![ColumnFilter::eq("status", Value::Null)],
            ..Default::default()
        };
        assert_eq!(local_view(&c, &r, &v), vec![2]);
        let v = LocalView {
            filters: vec![ColumnFilter {
                column: "total".into(),
                op: FilterOp::Gt,
                value: json!(100),
                values: vec![],
            }],
            search: "PEND".into(),
            ..Default::default()
        };
        assert_eq!(local_view(&c, &r, &v), vec![1]);
        assert!(like("Forge Hoodie", "%hood%"));
        assert!(like("abc", "a_c"));
        assert!(!like("abc", "a_"));
        assert!(is_numeric(&r, 2));
        assert!(!is_numeric(&r, 1));
    }
}
