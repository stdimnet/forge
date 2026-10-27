//! Chart mode of a result set: the series for line, bar, area, pie and scatter charts. The
//! UI only draws them (SVG).
//!
//! Rows are grouped by the X column (and split into one series per value of the optional
//! group column); each Y column is aggregated per X value. Scatter charts keep every row as
//! a point and need a numeric X.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use forge_sql::Refused;

use crate::{as_number, cmp_cells, text};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ChartKind {
    #[default]
    Bar,
    Line,
    Area,
    Pie,
    Scatter,
}

/// How Y values of the same X are combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ChartAgg {
    #[default]
    Sum,
    Avg,
    Min,
    Max,
    /// Number of rows (Y columns are not needed).
    Count,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartSpec {
    #[serde(default)]
    pub kind: ChartKind,
    pub x: String,
    #[serde(default)]
    pub y: Vec<String>,
    /// One series per value of this column.
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub agg: ChartAgg,
    /// At most this many categories (the rest are dropped); 0 means 200. Pie charts fold
    /// the rest into "Other".
    #[serde(default)]
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    pub name: String,
    /// One value per category (`None`: no rows), or for scatter charts the Y of each point.
    pub values: Vec<Option<f64>>,
    /// Scatter charts: the X of each point.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub xs: Vec<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartData {
    pub kind: ChartKind,
    /// X labels in display order (empty for scatter charts).
    pub categories: Vec<String>,
    pub series: Vec<Series>,
    /// Range of all values (for the axis), and of X for scatter charts.
    pub min: f64,
    pub max: f64,
    #[serde(default)]
    pub x_min: f64,
    #[serde(default)]
    pub x_max: f64,
    /// Rows skipped because Y (or X for scatter) was not a number.
    pub skipped: usize,
    /// Categories cut off by the limit.
    pub truncated: usize,
}

const DEFAULT_LIMIT: usize = 200;
const MAX_SERIES: usize = 12;

#[derive(Default, Clone, Copy)]
struct Acc {
    n: usize,
    sum: f64,
    min: f64,
    max: f64,
}

impl Acc {
    fn add(&mut self, v: f64) {
        if self.n == 0 {
            self.min = v;
            self.max = v;
        } else {
            self.min = self.min.min(v);
            self.max = self.max.max(v);
        }
        self.n += 1;
        self.sum += v;
    }

    fn get(&self, agg: ChartAgg) -> Option<f64> {
        if self.n == 0 {
            return None;
        }
        Some(match agg {
            ChartAgg::Sum => self.sum,
            ChartAgg::Avg => self.sum / self.n as f64,
            ChartAgg::Min => self.min,
            ChartAgg::Max => self.max,
            ChartAgg::Count => self.n as f64,
        })
    }
}

/// Series of `spec` over `columns` and `rows`.
pub fn chart_data(
    columns: &[String],
    rows: &[Vec<Value>],
    spec: &ChartSpec,
) -> Result<ChartData, Refused> {
    let col = |name: &str| {
        columns
            .iter()
            .position(|c| c == name)
            .ok_or_else(|| Refused(format!("No column {name}")))
    };
    let x = col(&spec.x)?;
    let ys: Vec<usize> = spec.y.iter().map(|y| col(y)).collect::<Result<_, _>>()?;
    let group = spec.group.as_deref().map(col).transpose()?;
    if ys.is_empty() && spec.agg != ChartAgg::Count {
        return Err(Refused("Pick a Y column".into()));
    }
    let null = Value::Null;
    let at = |r: &Vec<Value>, i: usize| r.get(i).unwrap_or(&null).clone();
    let label = |v: &Value| {
        if v.is_null() {
            "<null>".to_string()
        } else {
            text(v)
        }
    };
    // Series keys: (group value, y column); names "y", "group", or "group · y".
    let series_name = |g: Option<&str>, y: Option<&str>| match (g, y) {
        (Some(g), Some(y)) if spec.y.len() > 1 => format!("{g} · {y}"),
        (Some(g), _) => g.to_string(),
        (None, Some(y)) => y.to_string(),
        (None, None) => "count".to_string(),
    };
    let y_slots: Vec<Option<usize>> = if spec.agg == ChartAgg::Count {
        vec![None]
    } else {
        ys.iter().map(|&y| Some(y)).collect()
    };
    let mut out = ChartData {
        kind: spec.kind,
        ..Default::default()
    };
    let mut groups: Vec<String> = Vec::new();
    if let Some(g) = group {
        for r in rows {
            let v = label(&at(r, g));
            if !groups.contains(&v) {
                groups.push(v);
            }
        }
        groups.truncate(MAX_SERIES);
    }
    let group_names: Vec<Option<String>> = if group.is_some() {
        groups.iter().cloned().map(Some).collect()
    } else {
        vec![None]
    };

    if spec.kind == ChartKind::Scatter {
        let mut series: Vec<Series> = Vec::new();
        for g in &group_names {
            for y in &y_slots {
                let name = series_name(g.as_deref(), y.map(|i| columns[i].as_str()));
                series.push(Series {
                    name,
                    values: Vec::new(),
                    xs: Vec::new(),
                });
            }
        }
        let (mut lo, mut hi, mut xlo, mut xhi) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for r in rows {
            let gi = match group {
                Some(g) => match groups.iter().position(|x| *x == label(&at(r, g))) {
                    Some(i) => i,
                    None => continue,
                },
                None => 0,
            };
            let Some(xv) = as_number(&at(r, x)) else {
                out.skipped += 1;
                continue;
            };
            for (k, y) in y_slots.iter().enumerate() {
                let yv = match y {
                    Some(i) => as_number(&at(r, *i)),
                    None => Some(1.0),
                };
                let Some(yv) = yv else {
                    out.skipped += 1;
                    continue;
                };
                let s = &mut series[gi * y_slots.len() + k];
                s.xs.push(xv);
                s.values.push(Some(yv));
                lo = lo.min(yv);
                hi = hi.max(yv);
                xlo = xlo.min(xv);
                xhi = xhi.max(xv);
            }
        }
        out.series = series;
        if lo <= hi {
            (out.min, out.max, out.x_min, out.x_max) = (lo, hi, xlo, xhi);
        }
        return Ok(out);
    }

    // Categories: numbers and dates sorted, other text in order of appearance.
    let mut cats: Vec<Value> = Vec::new();
    let mut cat_index: HashMap<String, usize> = HashMap::new();
    for r in rows {
        let v = at(r, x);
        let k = label(&v);
        if !cat_index.contains_key(&k) {
            cat_index.insert(k, cats.len());
            cats.push(v);
        }
    }
    let sortable = cats.iter().all(|v| v.is_null() || as_number(v).is_some())
        || cats
            .iter()
            .all(|v| v.is_null() || looks_like_date(&text(v)));
    if sortable {
        cats.sort_by(cmp_cells);
    }
    let limit = if spec.limit == 0 {
        DEFAULT_LIMIT
    } else {
        spec.limit
    };
    let pie = spec.kind == ChartKind::Pie;
    let keep = if pie { limit.min(12) } else { limit };
    let mut categories: Vec<String> = cats.iter().map(label).collect();
    let slot_of: HashMap<String, usize> = categories
        .iter()
        .enumerate()
        .map(|(i, c)| (c.clone(), i))
        .collect();
    let mut accs = vec![vec![Acc::default(); categories.len()]; group_names.len() * y_slots.len()];
    for r in rows {
        let gi = match group {
            Some(g) => match groups.iter().position(|x| *x == label(&at(r, g))) {
                Some(i) => i,
                None => continue,
            },
            None => 0,
        };
        let ci = slot_of[&label(&at(r, x))];
        for (k, y) in y_slots.iter().enumerate() {
            match y {
                None => accs[gi * y_slots.len() + k][ci].add(1.0),
                Some(i) => match as_number(&at(r, *i)) {
                    Some(v) => accs[gi * y_slots.len() + k][ci].add(v),
                    None if at(r, *i).is_null() => {}
                    None => out.skipped += 1,
                },
            }
        }
    }
    let agg = spec.agg;
    let mut series: Vec<Series> = Vec::new();
    for (gi, g) in group_names.iter().enumerate() {
        for (k, y) in y_slots.iter().enumerate() {
            let a = &accs[gi * y_slots.len() + k];
            series.push(Series {
                name: series_name(g.as_deref(), y.map(|i| columns[i].as_str())),
                values: a.iter().map(|acc| acc.get(agg)).collect(),
                xs: Vec::new(),
            });
        }
    }
    if categories.len() > keep {
        out.truncated = categories.len() - keep;
        if pie {
            // Fold the smallest slices into "Other" (pies use the first series).
            let mut order: Vec<usize> = (0..categories.len()).collect();
            let first = &series[0].values;
            order.sort_by(|&a, &b| {
                first[b]
                    .unwrap_or(0.0)
                    .partial_cmp(&first[a].unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let top: Vec<usize> = order[..keep - 1].to_vec();
            let rest: Vec<usize> = order[keep - 1..].to_vec();
            for s in &mut series {
                let other: f64 = rest.iter().filter_map(|&i| s.values[i]).sum();
                let mut vals: Vec<Option<f64>> = top.iter().map(|&i| s.values[i]).collect();
                vals.push(Some(other));
                s.values = vals;
            }
            let mut cs: Vec<String> = top.iter().map(|&i| categories[i].clone()).collect();
            cs.push("Other".into());
            categories = cs;
            out.truncated = 0;
        } else {
            categories.truncate(keep);
            for s in &mut series {
                s.values.truncate(keep);
            }
        }
    }
    let all = series
        .iter()
        .flat_map(|s| s.values.iter().flatten().copied());
    let (lo, hi) = all.fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
    if lo <= hi {
        out.min = lo;
        out.max = hi;
    }
    out.categories = categories;
    out.series = series;
    Ok(out)
}

/// `2025-01-06`, `2025-01-06 10:00:00`, `10:30`.
fn looks_like_date(s: &str) -> bool {
    let b = s.as_bytes();
    (b.len() >= 10 && b[4] == b'-' && b[7] == b'-' && b[..4].iter().all(u8::is_ascii_digit))
        || (b.len() >= 5 && b[2] == b':' && b[..2].iter().all(u8::is_ascii_digit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn orders() -> (Vec<String>, Vec<Vec<Value>>) {
        (
            vec![
                "day".into(),
                "status".into(),
                "total".into(),
                "items".into(),
            ],
            vec![
                vec![json!("2025-01-02"), json!("paid"), json!("10.00"), json!(1)],
                vec![json!("2025-01-01"), json!("paid"), json!("5.50"), json!(2)],
                vec![
                    json!("2025-01-01"),
                    json!("pending"),
                    json!("4.50"),
                    json!(3),
                ],
                vec![json!("2025-01-02"), json!("paid"), Value::Null, json!(1)],
            ],
        )
    }

    #[test]
    fn aggregates_per_category() {
        let (c, r) = orders();
        let d = chart_data(
            &c,
            &r,
            &ChartSpec {
                kind: ChartKind::Line,
                x: "day".into(),
                y: vec!["total".into(), "items".into()],
                ..Default::default()
            },
        )
        .unwrap();
        // Dates are sorted.
        assert_eq!(d.categories, vec!["2025-01-01", "2025-01-02"]);
        assert_eq!(d.series[0].name, "total");
        assert_eq!(d.series[0].values, vec![Some(10.0), Some(10.0)]);
        assert_eq!(d.series[1].values, vec![Some(5.0), Some(2.0)]);
        assert_eq!((d.min, d.max), (2.0, 10.0));
    }

    #[test]
    fn groups_into_series_and_counts() {
        let (c, r) = orders();
        let d = chart_data(
            &c,
            &r,
            &ChartSpec {
                kind: ChartKind::Bar,
                x: "day".into(),
                group: Some("status".into()),
                agg: ChartAgg::Count,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(d.series.len(), 2);
        assert_eq!(d.series[0].name, "paid");
        assert_eq!(d.series[0].values, vec![Some(1.0), Some(2.0)]);
        assert_eq!(d.series[1].values, vec![Some(1.0), None]);
        let avg = chart_data(
            &c,
            &r,
            &ChartSpec {
                x: "status".into(),
                y: vec!["total".into()],
                agg: ChartAgg::Avg,
                ..Default::default()
            },
        )
        .unwrap();
        // Text categories keep their order of appearance.
        assert_eq!(avg.categories, vec!["paid", "pending"]);
        assert_eq!(avg.series[0].values[0], Some(7.75));
    }

    #[test]
    fn scatter_and_pie() {
        let (c, r) = orders();
        let s = chart_data(
            &c,
            &r,
            &ChartSpec {
                kind: ChartKind::Scatter,
                x: "items".into(),
                y: vec!["total".into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(s.series[0].xs, vec![1.0, 2.0, 3.0]);
        assert_eq!(s.skipped, 1);
        assert_eq!((s.x_min, s.x_max), (1.0, 3.0));
        let rows: Vec<Vec<Value>> = (0..20)
            .map(|i| vec![json!(format!("c{i}")), json!(i)])
            .collect();
        let p = chart_data(
            &["cat".into(), "n".into()],
            &rows,
            &ChartSpec {
                kind: ChartKind::Pie,
                x: "cat".into(),
                y: vec!["n".into()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(p.categories.len(), 12);
        assert_eq!(p.categories[0], "c19");
        assert_eq!(p.categories[11], "Other");
        // 0 + 1 + … + 8
        assert_eq!(p.series[0].values[11], Some(36.0));
        assert!(chart_data(
            &c,
            &r,
            &ChartSpec {
                x: "nope".into(),
                ..Default::default()
            }
        )
        .is_err());
    }
}
