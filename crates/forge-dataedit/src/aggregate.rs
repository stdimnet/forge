//! Aggregates of selected cells (the data editor's status bar and Aggregate view): count,
//! NULLs, distinct values, and for numbers sum, average, min and max. Decimals arrive as
//! strings (`"19.90"`); their sum is exact as long as it fits in 128 bits.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{as_number, cmp_cells, text};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Aggregate {
    /// Cells, NULLs included.
    pub count: usize,
    pub nulls: usize,
    /// Distinct non-NULL values.
    pub distinct: usize,
    /// All non-NULL values are numbers.
    pub numeric: bool,
    /// Sum and average as text (exact for decimals); numeric cells only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sum: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg: Option<String>,
    /// Smallest and largest non-NULL values (numerically for numbers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<Value>,
}

/// A fixed-point decimal: `digits / 10^scale`.
#[derive(Debug, Clone, Copy)]
struct Fixed {
    digits: i128,
    scale: u32,
}

fn parse_fixed(v: &Value) -> Option<Fixed> {
    let s = match v {
        Value::Number(n) if n.is_i64() || n.is_u64() => n.to_string(),
        Value::String(s) => s.trim().to_string(),
        _ => return None,
    };
    let (neg, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(&s)),
    };
    let (int, frac) = body.split_once('.').unwrap_or((body, ""));
    if int.is_empty() && frac.is_empty()
        || !int.bytes().all(|b| b.is_ascii_digit())
        || !frac.bytes().all(|b| b.is_ascii_digit())
        || frac.len() > 18
    {
        return None;
    }
    let digits: i128 = format!("{int}{frac}").parse().ok()?;
    Some(Fixed {
        digits: if neg { -digits } else { digits },
        scale: frac.len() as u32,
    })
}

fn rescale(f: Fixed, scale: u32) -> Option<i128> {
    f.digits.checked_mul(10i128.checked_pow(scale - f.scale)?)
}

fn fixed_text(digits: i128, scale: u32) -> String {
    if scale == 0 {
        return digits.to_string();
    }
    let neg = digits < 0;
    let s = digits.unsigned_abs().to_string();
    let s = format!("{s:0>width$}", width = scale as usize + 1);
    let (i, f) = s.split_at(s.len() - scale as usize);
    format!("{}{i}.{f}", if neg { "-" } else { "" })
}

/// Exact sum of decimals, or `None` (floats, overflow).
fn exact_sum(values: &[&Value]) -> Option<(i128, u32)> {
    let fixed: Vec<Fixed> = values.iter().map(|v| parse_fixed(v)).collect::<Option<_>>()?;
    let scale = fixed.iter().map(|f| f.scale).max().unwrap_or(0);
    let mut sum: i128 = 0;
    for f in fixed {
        sum = sum.checked_add(rescale(f, scale)?)?;
    }
    Some((sum, scale))
}

/// Shortest text of a float (`3` rather than `3.0`, at most 10 decimals).
fn float_text(f: f64) -> String {
    let s = format!("{f:.10}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

/// Aggregate of cells.
pub fn aggregate(values: &[Value]) -> Aggregate {
    let non_null: Vec<&Value> = values.iter().filter(|v| !v.is_null()).collect();
    let mut seen = HashSet::new();
    for v in &non_null {
        // Numbers by value, so 1 and "1.0" count once.
        let k = match as_number(v) {
            Some(n) => format!("n:{n}"),
            None => format!("t:{}", text(v)),
        };
        seen.insert(k);
    }
    let numeric = !non_null.is_empty() && non_null.iter().all(|v| as_number(v).is_some());
    let mut out = Aggregate {
        count: values.len(),
        nulls: values.len() - non_null.len(),
        distinct: seen.len(),
        numeric,
        ..Default::default()
    };
    out.min = non_null
        .iter()
        .min_by(|a, b| cmp_cells(a, b))
        .map(|v| (*v).clone());
    // The first of equal maxima, like the minimum.
    out.max = non_null
        .iter()
        .rev()
        .max_by(|a, b| cmp_cells(a, b))
        .map(|v| (*v).clone());
    if numeric {
        let n = non_null.len() as f64;
        match exact_sum(&non_null) {
            Some((sum, scale)) => {
                out.sum = Some(fixed_text(sum, scale));
                // Average with a few more decimals than the inputs.
                let avg_scale = (scale + 4).min(18);
                let scaled = sum
                    .checked_mul(10i128.pow(avg_scale - scale))
                    .map(|s| {
                        let n = non_null.len() as i128;
                        // Round half away from zero.
                        let q = s / n;
                        let r = s % n;
                        if r.abs() * 2 >= n {
                            q + s.signum()
                        } else {
                            q
                        }
                    });
                out.avg = Some(match scaled {
                    Some(a) => trim_fraction(fixed_text(a, avg_scale)),
                    None => float_text(sum as f64 / 10f64.powi(scale as i32) / n),
                });
            }
            None => {
                let sum: f64 = non_null.iter().filter_map(|v| as_number(v)).sum();
                out.sum = Some(float_text(sum));
                out.avg = Some(float_text(sum / n));
            }
        }
    }
    out
}

fn trim_fraction(s: String) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// The aggregates of one column (Aggregate view).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnAggregate {
    pub column: String,
    #[serde(flatten)]
    pub aggregate: Aggregate,
}

/// Aggregates per column of `rows` (the selected rows and columns of the grid).
pub fn aggregate_columns(columns: &[String], rows: &[Vec<Value>]) -> Vec<ColumnAggregate> {
    columns
        .iter()
        .enumerate()
        .map(|(i, c)| ColumnAggregate {
            column: c.clone(),
            aggregate: aggregate(
                &rows
                    .iter()
                    .map(|r| r.get(i).cloned().unwrap_or(Value::Null))
                    .collect::<Vec<_>>(),
            ),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_decimal_aggregates() {
        let a = aggregate(&[json!("19.90"), json!("0.10"), json!(5), Value::Null, json!("19.9")]);
        assert_eq!(a.count, 5);
        assert_eq!(a.nulls, 1);
        assert_eq!(a.distinct, 3);
        assert!(a.numeric);
        assert_eq!(a.sum.as_deref(), Some("44.90"));
        assert_eq!(a.avg.as_deref(), Some("11.225"));
        assert_eq!(a.min, Some(json!("0.10")));
        assert_eq!(a.max, Some(json!("19.90")));
        let a = aggregate(&[json!(1), json!(2), json!(2)]);
        assert_eq!(a.sum.as_deref(), Some("5"));
        assert_eq!(a.avg.as_deref(), Some("1.6667"));
        let a = aggregate(&[json!("-1.5"), json!("0.5")]);
        assert_eq!(a.sum.as_deref(), Some("-1.0"));
        assert_eq!(a.avg.as_deref(), Some("-0.5"));
    }

    #[test]
    fn floats_and_text() {
        let a = aggregate(&[json!(0.1), json!(0.2)]);
        assert_eq!(a.sum.as_deref(), Some("0.3"));
        let a = aggregate(&[json!("paid"), json!("pending"), json!("paid"), Value::Null]);
        assert!(!a.numeric);
        assert_eq!(a.sum, None);
        assert_eq!(a.distinct, 2);
        assert_eq!(a.min, Some(json!("paid")));
        assert_eq!(a.max, Some(json!("pending")));
        let empty = aggregate(&[]);
        assert_eq!((empty.count, empty.numeric, empty.min), (0, false, None));
        let cols = aggregate_columns(
            &["a".into(), "b".into()],
            &[vec![json!(1), json!("x")], vec![json!(3)]],
        );
        assert_eq!(cols[0].aggregate.sum.as_deref(), Some("4"));
        assert_eq!(cols[1].aggregate.nulls, 1);
    }
}
