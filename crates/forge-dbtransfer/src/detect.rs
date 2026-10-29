//! Type detection of import columns and the SQL types of a new target table.

use forge_dbmodel::design::{ColumnSpec, TableSpec};
use forge_sql::Dialect;
use serde::{Deserialize, Serialize};

use crate::read::SourceTable;

/// What the values of a column look like.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValueKind {
    Integer,
    Decimal,
    Boolean,
    Date,
    DateTime,
    #[default]
    Text,
}

/// A detected source column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnGuess {
    pub name: String,
    pub kind: ValueKind,
    /// Longest value (characters).
    pub max_len: usize,
    /// Digits before and after the decimal point (decimals).
    pub precision: u32,
    pub scale: u32,
    pub nullable: bool,
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && is_digits(&s[..4])
        && is_digits(&s[5..7])
        && is_digits(&s[8..])
        && (1..=12).contains(&s[5..7].parse::<u32>().unwrap_or(0))
        && (1..=31).contains(&s[8..].parse::<u32>().unwrap_or(0))
}

fn is_datetime(s: &str) -> bool {
    if s.len() < 16 || !s.is_char_boundary(10) || !is_date(&s[..10]) {
        return false;
    }
    let rest = &s[10..];
    let Some(time) = rest.strip_prefix(' ').or_else(|| rest.strip_prefix('T')) else {
        return false;
    };
    let time = time.trim_end_matches('Z').split(['+']).next().unwrap_or("");
    let time = match time.rfind('-') {
        Some(i) if i >= 5 => &time[..i],
        _ => time,
    };
    let parts: Vec<&str> = time.split(':').collect();
    (2..=3).contains(&parts.len())
        && is_digits(parts[0])
        && is_digits(parts[1])
        && parts
            .get(2)
            .is_none_or(|sec| is_digits(sec.split('.').next().unwrap_or("")))
}

/// The kind of one value, with digits before / after the point for numbers.
pub fn classify(v: &str) -> (ValueKind, u32, u32) {
    let s = v.trim();
    let unsigned = s.strip_prefix(['-', '+']).unwrap_or(s);
    if is_digits(unsigned) {
        // Leading zeros (zip codes, ids) and huge numbers stay text.
        if unsigned.len() > 1 && unsigned.starts_with('0') || unsigned.len() > 18 {
            return (ValueKind::Text, 0, 0);
        }
        return (ValueKind::Integer, unsigned.len() as u32, 0);
    }
    if let Some((int, frac)) = unsigned.split_once('.') {
        if (is_digits(int) || int.is_empty())
            && is_digits(frac)
            && !(int.len() > 1 && int.starts_with('0'))
        {
            return (
                ValueKind::Decimal,
                int.len().max(1) as u32,
                frac.len() as u32,
            );
        }
    }
    if unsigned.contains(['e', 'E']) && unsigned.parse::<f64>().is_ok_and(f64::is_finite) {
        return (ValueKind::Decimal, 0, 0);
    }
    match s.to_ascii_lowercase().as_str() {
        "true" | "false" | "yes" | "no" => return (ValueKind::Boolean, 0, 0),
        _ => {}
    }
    if is_date(s) {
        return (ValueKind::Date, 0, 0);
    }
    if is_datetime(s) {
        return (ValueKind::DateTime, 0, 0);
    }
    (ValueKind::Text, 0, 0)
}

fn widen(a: ValueKind, b: ValueKind) -> ValueKind {
    use ValueKind::*;
    match (a, b) {
        (x, y) if x == y => x,
        (Integer, Decimal) | (Decimal, Integer) => Decimal,
        (Date, DateTime) | (DateTime, Date) => DateTime,
        _ => Text,
    }
}

/// Kinds of every column, from the values of up to `limit` rows.
pub fn detect(t: &SourceTable, limit: usize) -> Vec<ColumnGuess> {
    t.columns
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let mut kind: Option<ValueKind> = None;
            let mut g = ColumnGuess {
                name: name.clone(),
                kind: ValueKind::Text,
                max_len: 0,
                precision: 0,
                scale: 0,
                nullable: false,
            };
            let (mut int_digits, mut scale) = (0u32, 0u32);
            for row in t.rows.iter().take(limit) {
                match row.get(i).and_then(|v| v.as_deref()) {
                    None | Some("") => g.nullable = true,
                    Some(v) => {
                        g.max_len = g.max_len.max(v.chars().count());
                        let (k, p, s) = classify(v);
                        int_digits = int_digits.max(p);
                        scale = scale.max(s);
                        kind = Some(kind.map_or(k, |old| widen(old, k)));
                    }
                }
            }
            g.kind = kind.unwrap_or(ValueKind::Text);
            if g.kind == ValueKind::Decimal {
                g.precision = int_digits + scale;
                g.scale = scale;
            }
            g
        })
        .collect()
}

/// The SQL type of a new column for `g` in dialect `d`.
pub fn sql_type(d: Dialect, g: &ColumnGuess) -> String {
    use ValueKind::*;
    let text_len = |cap: usize| -> usize {
        [32, 64, 255, 1000, 4000]
            .into_iter()
            .find(|n| *n >= g.max_len)
            .filter(|n| *n <= cap)
            .unwrap_or(0)
    };
    let decimal = || {
        if g.precision == 0 {
            None
        } else {
            let p = (g.precision + 2).clamp(10, 38);
            Some(format!("{p},{}", g.scale.min(p - 1)))
        }
    };
    match (d, g.kind) {
        (Dialect::Sqlite, Integer | Boolean) => "INTEGER".into(),
        (Dialect::Sqlite, Decimal) => "REAL".into(),
        (Dialect::Sqlite, _) => "TEXT".into(),
        (Dialect::ClickHouse, Integer) => "Int64".into(),
        (Dialect::ClickHouse, Decimal) => "Float64".into(),
        (Dialect::ClickHouse, Boolean) => "Bool".into(),
        (Dialect::ClickHouse, Date) => "Date".into(),
        (Dialect::ClickHouse, DateTime) => "DateTime".into(),
        (Dialect::ClickHouse, Text) => "String".into(),
        (Dialect::Oracle, Integer) => "NUMBER(19)".into(),
        (Dialect::Oracle, Decimal) => decimal().map_or("NUMBER".into(), |p| format!("NUMBER({p})")),
        (Dialect::Oracle, Boolean) => "NUMBER(1)".into(),
        (Dialect::Oracle, Date) => "DATE".into(),
        (Dialect::Oracle, DateTime) => "TIMESTAMP".into(),
        (Dialect::Oracle, Text) => match text_len(4000) {
            0 => "CLOB".into(),
            n => format!("VARCHAR2({n})"),
        },
        (_, Integer) => "bigint".into(),
        (Dialect::Postgres, Decimal) => {
            decimal().map_or("double precision".into(), |p| format!("numeric({p})"))
        }
        (_, Decimal) => decimal().map_or("float".into(), |p| format!("decimal({p})")),
        (Dialect::SqlServer, Boolean) => "bit".into(),
        (Dialect::MySql, Boolean) => "tinyint(1)".into(),
        (_, Boolean) => "boolean".into(),
        (_, Date) => "date".into(),
        (Dialect::MySql, DateTime) => "datetime".into(),
        (Dialect::SqlServer, DateTime) => "datetime2".into(),
        (_, DateTime) => "timestamp".into(),
        (Dialect::Postgres, Text) => "text".into(),
        (Dialect::SqlServer, Text) => match text_len(4000) {
            0 => "nvarchar(max)".into(),
            n => format!("nvarchar({n})"),
        },
        (_, Text) => match text_len(1000) {
            0 => "text".into(),
            n => format!("varchar({n})"),
        },
    }
}

/// A target identifier from a source column name (`Order Date` → `order_date`).
pub fn column_ident(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    let out = out.trim_end_matches('_').to_string();
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        format!("c_{out}")
    } else {
        out
    }
}

/// The new table of an import ("New table" in the mapping dialog).
pub fn new_table(d: Dialect, schema: &str, name: &str, guesses: &[ColumnGuess]) -> TableSpec {
    TableSpec {
        schema: schema.to_string(),
        name: name.to_string(),
        columns: guesses
            .iter()
            .map(|g| ColumnSpec {
                name: column_ident(&g.name),
                data_type: sql_type(d, g),
                nullable: true,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(rows: &[&[Option<&str>]]) -> SourceTable {
        SourceTable {
            columns: (0..rows[0].len()).map(|i| format!("c{i}")).collect(),
            rows: rows
                .iter()
                .map(|r| r.iter().map(|v| v.map(String::from)).collect())
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn classifies_values() {
        assert_eq!(classify("42").0, ValueKind::Integer);
        assert_eq!(classify("-7").0, ValueKind::Integer);
        assert_eq!(classify("007").0, ValueKind::Text);
        assert_eq!(classify("3.25"), (ValueKind::Decimal, 1, 2));
        assert_eq!(classify("1e5").0, ValueKind::Decimal);
        assert_eq!(classify("Yes").0, ValueKind::Boolean);
        assert_eq!(classify("2024-02-29").0, ValueKind::Date);
        assert_eq!(classify("2024-13-01").0, ValueKind::Text);
        assert_eq!(classify("2024-02-29 13:45:10").0, ValueKind::DateTime);
        assert_eq!(
            classify("2024-02-29T13:45:10.123+02:00").0,
            ValueKind::DateTime
        );
        assert_eq!(classify("2024-02-29T13:45Z").0, ValueKind::DateTime);
        assert_eq!(classify("hello").0, ValueKind::Text);
    }

    #[test]
    fn detects_columns_and_widens() {
        let t = table(&[
            &[
                Some("1"),
                Some("1.5"),
                Some("2024-01-01"),
                Some("true"),
                None,
            ],
            &[
                Some("2"),
                Some("10"),
                Some("2024-01-01 10:00"),
                Some("x"),
                Some(""),
            ],
        ]);
        let g = detect(&t, 100);
        assert_eq!(g[0].kind, ValueKind::Integer);
        assert!(!g[0].nullable);
        assert_eq!(
            (g[1].kind, g[1].precision, g[1].scale),
            (ValueKind::Decimal, 3, 1)
        );
        assert_eq!(g[2].kind, ValueKind::DateTime);
        assert_eq!(g[3].kind, ValueKind::Text);
        assert_eq!(g[4].kind, ValueKind::Text);
        assert!(g[4].nullable);
    }

    #[test]
    fn sql_types_per_dialect() {
        let g = |kind, max_len| ColumnGuess {
            name: "x".into(),
            kind,
            max_len,
            precision: 5,
            scale: 2,
            nullable: true,
        };
        assert_eq!(
            sql_type(Dialect::MySql, &g(ValueKind::Text, 40)),
            "varchar(64)"
        );
        assert_eq!(sql_type(Dialect::MySql, &g(ValueKind::Text, 5000)), "text");
        assert_eq!(
            sql_type(Dialect::Postgres, &g(ValueKind::Decimal, 0)),
            "numeric(10,2)"
        );
        assert_eq!(
            sql_type(Dialect::SqlServer, &g(ValueKind::Boolean, 0)),
            "bit"
        );
        assert_eq!(
            sql_type(Dialect::SqlServer, &g(ValueKind::Text, 9000)),
            "nvarchar(max)"
        );
        assert_eq!(
            sql_type(Dialect::Oracle, &g(ValueKind::DateTime, 0)),
            "TIMESTAMP"
        );
        assert_eq!(sql_type(Dialect::Sqlite, &g(ValueKind::Date, 0)), "TEXT");
        assert_eq!(
            sql_type(Dialect::ClickHouse, &g(ValueKind::Integer, 0)),
            "Int64"
        );
    }

    #[test]
    fn new_table_from_guesses() {
        assert_eq!(column_ident("Order Date"), "order_date");
        assert_eq!(column_ident("2nd #"), "c_2nd");
        let t = new_table(
            Dialect::Postgres,
            "public",
            "people",
            &[ColumnGuess {
                name: "First Name".into(),
                kind: ValueKind::Text,
                max_len: 10,
                precision: 0,
                scale: 0,
                nullable: false,
            }],
        );
        assert_eq!(t.columns[0].name, "first_name");
        assert_eq!(t.columns[0].data_type, "text");
    }
}
