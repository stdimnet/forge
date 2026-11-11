//! Batched `INSERT` statements of an import, per dialect: multi-row `VALUES` (at most 1000
//! rows per statement on SQL Server), `INSERT ALL … SELECT 1 FROM dual` on Oracle, and
//! literals by the mapped column kind.

use forge_dbmodel::design::TableSpec;
use forge_sql::Dialect;
use serde::{Deserialize, Serialize};

use crate::detect::ValueKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnError {
    /// Rows that fail are skipped (their batch is retried row by row) and reported.
    #[default]
    Skip,
    /// The first error stops the import; with transactions nothing is kept.
    Abort,
}

/// A target column fed from a source column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnMapping {
    /// Index of the source column.
    pub source: usize,
    pub target: String,
    #[serde(default)]
    pub kind: ValueKind,
}

/// What the import does with the rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImportPlan {
    pub schema: String,
    pub table: String,
    pub columns: Vec<ColumnMapping>,
    /// A new table: created before the rows are inserted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create: Option<TableSpec>,
    pub batch_size: usize,
    pub on_error: OnError,
    /// Empty strings are inserted as NULL.
    pub empty_as_null: bool,
    /// Only the first rows (0: all).
    pub limit: usize,
}

impl Default for ImportPlan {
    fn default() -> Self {
        Self {
            schema: String::new(),
            table: String::new(),
            columns: Vec::new(),
            create: None,
            batch_size: 500,
            on_error: OnError::Skip,
            empty_as_null: true,
            limit: 0,
        }
    }
}

/// One `INSERT` statement and the rows (0-based, of the source) it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Batch {
    pub sql: String,
    pub first: usize,
    pub rows: usize,
}

fn numeric(s: &str) -> Option<&str> {
    let t = s.trim();
    let t = t.strip_prefix('+').unwrap_or(t);
    let body = t.strip_prefix('-').unwrap_or(t);
    let ok = !body.is_empty()
        && body.bytes().all(|b| {
            b.is_ascii_digit() || b == b'.' || b == b'e' || b == b'E' || b == b'-' || b == b'+'
        })
        && body
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_digit() || b == b'.')
        && t.parse::<f64>().is_ok();
    ok.then_some(t)
}

/// The SQL literal of one value of a `kind` column.
pub fn literal(d: Dialect, kind: ValueKind, v: Option<&str>, empty_as_null: bool) -> String {
    let Some(v) = v else {
        return "NULL".into();
    };
    if v.is_empty() && (empty_as_null || kind != ValueKind::Text) {
        return "NULL".into();
    }
    match kind {
        ValueKind::Integer | ValueKind::Decimal => match numeric(v) {
            Some(n) => n.to_string(),
            None => d.quote_value(v),
        },
        ValueKind::Boolean => {
            let truth = match v.trim().to_ascii_lowercase().as_str() {
                "true" | "yes" | "1" | "t" | "y" => Some(true),
                "false" | "no" | "0" | "f" | "n" => Some(false),
                _ => None,
            };
            match (truth, d) {
                (None, _) => d.quote_value(v),
                (Some(b), Dialect::Postgres | Dialect::MySql | Dialect::ClickHouse) => {
                    if b { "TRUE" } else { "FALSE" }.into()
                }
                (Some(b), _) => if b { "1" } else { "0" }.into(),
            }
        }
        ValueKind::Date if d == Dialect::Oracle => format!("DATE {}", d.quote_str(v.trim())),
        ValueKind::DateTime if d == Dialect::Oracle => {
            // Oracle TIMESTAMP literals carry no zone: values with one are converted to UTC.
            let t = v.trim().replacen('T', " ", 1);
            let t = to_utc(&t).unwrap_or(t);
            format!("TIMESTAMP {}", d.quote_str(&t))
        }
        ValueKind::DateTime if d == Dialect::MySql => {
            d.quote_str(v.trim().replacen('T', " ", 1).trim_end_matches('Z'))
        }
        _ => d.quote_value(v),
    }
}

/// `2024-01-02 10:00:00+02:00` (also `Z`, `+0200`, `-05`) as UTC without a zone:
/// `2024-01-02 08:00:00`. `None` for a value without a zone or one that does not parse.
fn to_utc(t: &str) -> Option<String> {
    let (local, offset_min) = if let Some(local) = t.strip_suffix(['Z', 'z']) {
        (local, 0i64)
    } else {
        let at = t.rfind(['+', '-']).filter(|&i| i > 10)?;
        let zone = &t[at + 1..];
        let digits: String = zone.chars().filter(|c| *c != ':').collect();
        if !(digits.len() == 2 || digits.len() == 4) || !digits.bytes().all(|c| c.is_ascii_digit())
        {
            return None;
        }
        let h: i64 = digits[..2].parse().ok()?;
        let m: i64 = if digits.len() == 4 {
            digits[2..].parse().ok()?
        } else {
            0
        };
        let sign = if t.as_bytes()[at] == b'-' { -1 } else { 1 };
        (&t[..at], sign * (h * 60 + m))
    };
    let local = local.trim_end();
    // `YYYY-MM-DD HH:MM[:SS[.fff]]`
    let (date, time) = local.split_once(' ')?;
    let mut dp = date.split('-');
    let (y, mo, da): (i64, i64, i64) = (
        dp.next()?.parse().ok()?,
        dp.next()?.parse().ok()?,
        dp.next()?.parse().ok()?,
    );
    let (hms, frac) = match time.split_once('.') {
        Some((hms, frac)) if frac.bytes().all(|c| c.is_ascii_digit()) => (hms, Some(frac)),
        Some(_) => return None,
        None => (time, None),
    };
    let mut tp = hms.split(':');
    let (hh, mm): (i64, i64) = (tp.next()?.parse().ok()?, tp.next()?.parse().ok()?);
    let ss: i64 = tp.next().map(str::parse).transpose().ok()?.unwrap_or(0);
    if dp.next().is_some()
        || tp.next().is_some()
        || !(1..=12).contains(&mo)
        || !(1..=31).contains(&da)
        || hh > 23
        || mm > 59
        || ss > 60
    {
        return None;
    }
    let minutes = days_from_civil(y, mo, da) * 1440 + hh * 60 + mm - offset_min;
    let (days, rest) = (minutes.div_euclid(1440), minutes.rem_euclid(1440));
    let (y, mo, da) = civil_from_days(days);
    let mut out = format!(
        "{y:04}-{mo:02}-{da:02} {:02}:{:02}:{ss:02}",
        rest / 60,
        rest % 60
    );
    if let Some(f) = frac {
        out.push('.');
        out.push_str(f);
    }
    Some(out)
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Rows per statement: the plan's batch size within the dialect's limit.
pub fn batch_rows(d: Dialect, plan: &ImportPlan) -> usize {
    let n = plan.batch_size.max(1);
    match d {
        Dialect::SqlServer => n.min(1000),
        Dialect::Oracle => n.min(500),
        _ => n,
    }
}

/// One `INSERT` of literal rows.
pub fn insert_sql(
    d: Dialect,
    schema: &str,
    table: &str,
    columns: &[String],
    rows: &[Vec<String>],
) -> String {
    let target = d.qualified(schema, table);
    let cols = columns
        .iter()
        .map(|c| d.quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    if d == Dialect::Oracle && rows.len() > 1 {
        let mut s = String::from("INSERT ALL\n");
        for r in rows {
            s.push_str(&format!(
                "    INTO {target} ({cols}) VALUES ({})\n",
                r.join(", ")
            ));
        }
        s.push_str("SELECT 1 FROM dual");
        return s;
    }
    let values = rows
        .iter()
        .map(|r| format!("({})", r.join(", ")))
        .collect::<Vec<_>>()
        .join(",\n       ");
    format!("INSERT INTO {target} ({cols})\nVALUES {values}")
}

fn row_literals(d: Dialect, plan: &ImportPlan, row: &[Option<String>]) -> Vec<String> {
    plan.columns
        .iter()
        .map(|m| {
            literal(
                d,
                m.kind,
                row.get(m.source).and_then(|v| v.as_deref()),
                plan.empty_as_null,
            )
        })
        .collect()
}

/// The `INSERT` statements of `rows` (each with the source rows it covers).
pub fn batches(d: Dialect, plan: &ImportPlan, rows: &[Vec<Option<String>>]) -> Vec<Batch> {
    let rows = if plan.limit > 0 && plan.limit < rows.len() {
        &rows[..plan.limit]
    } else {
        rows
    };
    let names: Vec<String> = plan.columns.iter().map(|m| m.target.clone()).collect();
    let size = batch_rows(d, plan);
    rows.chunks(size)
        .enumerate()
        .map(|(i, chunk)| Batch {
            sql: insert_sql(
                d,
                &plan.schema,
                &plan.table,
                &names,
                &chunk
                    .iter()
                    .map(|r| row_literals(d, plan, r))
                    .collect::<Vec<_>>(),
            ),
            first: i * size,
            rows: chunk.len(),
        })
        .collect()
}

/// The `INSERT` of one row (the retry of a failed batch, row by row).
pub fn row_insert(d: Dialect, plan: &ImportPlan, row: &[Option<String>]) -> String {
    let names: Vec<String> = plan.columns.iter().map(|m| m.target.clone()).collect();
    insert_sql(
        d,
        &plan.schema,
        &plan.table,
        &names,
        &[row_literals(d, plan, row)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> ImportPlan {
        ImportPlan {
            schema: "shop".into(),
            table: "people".into(),
            columns: vec![
                ColumnMapping {
                    source: 0,
                    target: "id".into(),
                    kind: ValueKind::Integer,
                },
                ColumnMapping {
                    source: 2,
                    target: "name".into(),
                    kind: ValueKind::Text,
                },
                ColumnMapping {
                    source: 1,
                    target: "active".into(),
                    kind: ValueKind::Boolean,
                },
            ],
            batch_size: 2,
            ..Default::default()
        }
    }

    fn rows() -> Vec<Vec<Option<String>>> {
        let r = |a: &str, b: &str, c: Option<&str>| {
            vec![Some(a.into()), Some(b.into()), c.map(String::from)]
        };
        vec![
            r("1", "yes", Some("O'Brien")),
            r("2", "no", None),
            r("3", "true", Some("")),
        ]
    }

    #[test]
    fn literals_per_kind_and_dialect() {
        assert_eq!(
            literal(Dialect::MySql, ValueKind::Integer, Some(" +42 "), true),
            "42"
        );
        assert_eq!(
            literal(Dialect::MySql, ValueKind::Integer, Some("n/a"), true),
            "'n/a'"
        );
        assert_eq!(
            literal(Dialect::Postgres, ValueKind::Decimal, Some("-1.5e3"), true),
            "-1.5e3"
        );
        assert_eq!(
            literal(Dialect::Postgres, ValueKind::Boolean, Some("Yes"), true),
            "TRUE"
        );
        assert_eq!(
            literal(Dialect::SqlServer, ValueKind::Boolean, Some("false"), true),
            "0"
        );
        assert_eq!(
            literal(Dialect::SqlServer, ValueKind::Text, Some("ä"), true),
            "N'ä'"
        );
        assert_eq!(
            literal(Dialect::Oracle, ValueKind::Date, Some("2024-01-02"), true),
            "DATE '2024-01-02'"
        );
        assert_eq!(
            literal(
                Dialect::Oracle,
                ValueKind::DateTime,
                Some("2024-01-02T10:00:00+02:00"),
                true
            ),
            "TIMESTAMP '2024-01-02 08:00:00'"
        );
        let oracle = |v: &str| literal(Dialect::Oracle, ValueKind::DateTime, Some(v), true);
        // Negative offsets convert too, across the day and year boundary.
        assert_eq!(
            oracle("2023-12-31T22:30:00-05:00"),
            "TIMESTAMP '2024-01-01 03:30:00'"
        );
        assert_eq!(
            oracle("2024-03-01 00:15:00.250+0100"),
            "TIMESTAMP '2024-02-29 23:15:00.250'"
        );
        assert_eq!(
            oracle("2024-01-02T10:00:00Z"),
            "TIMESTAMP '2024-01-02 10:00:00'"
        );
        assert_eq!(oracle("2024-01-02 10:00"), "TIMESTAMP '2024-01-02 10:00'");
        assert_eq!(
            literal(
                Dialect::MySql,
                ValueKind::DateTime,
                Some("2024-01-02T10:00:00Z"),
                true
            ),
            "'2024-01-02 10:00:00'"
        );
        assert_eq!(
            literal(Dialect::MySql, ValueKind::Text, Some(""), true),
            "NULL"
        );
        assert_eq!(
            literal(Dialect::MySql, ValueKind::Text, Some(""), false),
            "''"
        );
        assert_eq!(
            literal(Dialect::MySql, ValueKind::Integer, Some(""), false),
            "NULL"
        );
        assert_eq!(
            literal(Dialect::MySql, ValueKind::Text, None, false),
            "NULL"
        );
    }

    #[test]
    fn batches_rows() {
        let b = batches(Dialect::Postgres, &plan(), &rows());
        assert_eq!(b.len(), 2);
        assert_eq!(
            b[0].sql,
            "INSERT INTO \"shop\".\"people\" (\"id\", \"name\", \"active\")\nVALUES (1, 'O''Brien', TRUE),\n       (2, NULL, FALSE)"
        );
        assert_eq!((b[1].first, b[1].rows), (2, 1));
        assert_eq!(b[1].sql, "INSERT INTO \"shop\".\"people\" (\"id\", \"name\", \"active\")\nVALUES (3, NULL, TRUE)");
        let limited = ImportPlan { limit: 1, ..plan() };
        assert_eq!(batches(Dialect::Postgres, &limited, &rows()).len(), 1);
    }

    #[test]
    fn dialect_limits_and_oracle_insert_all() {
        let big = ImportPlan {
            batch_size: 5000,
            ..plan()
        };
        assert_eq!(batch_rows(Dialect::SqlServer, &big), 1000);
        assert_eq!(batch_rows(Dialect::MySql, &big), 5000);
        let b = batches(Dialect::Oracle, &plan(), &rows());
        assert_eq!(
            b[0].sql,
            "INSERT ALL\n    INTO \"shop\".\"people\" (\"id\", \"name\", \"active\") VALUES (1, 'O''Brien', 1)\n    INTO \"shop\".\"people\" (\"id\", \"name\", \"active\") VALUES (2, NULL, 0)\nSELECT 1 FROM dual"
        );
        assert_eq!(
            row_insert(Dialect::MySql, &plan(), &rows()[1]),
            "INSERT INTO `shop`.`people` (`id`, `name`, `active`)\nVALUES (2, NULL, FALSE)"
        );
    }
}
