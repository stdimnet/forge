//! Cell values as JSON that survives JavaScript: `null`, booleans, numbers only within
//! ±2^53, everything else (big integers, decimals, dates) as strings, binary data as
//! `{"$hex": "89504e47"}`.

use serde_json::{json, Map, Number, Value};

use crate::dialect::Dialect;

/// Largest integer a JavaScript number represents exactly.
pub const MAX_SAFE_INTEGER: i64 = (1 << 53) - 1;

/// The `DEFAULT` marker of the value editor (`{"$default": true}`): the column's default.
pub fn default_marker() -> Value {
    json!({ "$default": true })
}

/// Whether `v` is the `DEFAULT` marker.
pub fn is_default(v: &Value) -> bool {
    v.as_object()
        .and_then(|o| o.get("$default"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Binary cell value.
pub fn binary(bytes: &[u8]) -> Value {
    json!({ "$hex": hex::encode(bytes) })
}

/// The bytes of a binary cell value, if `v` is one.
pub fn as_binary(v: &Value) -> Option<Vec<u8>> {
    v.as_object()
        .and_then(|o| o.get("$hex"))
        .and_then(Value::as_str)
        .and_then(|h| hex::decode(h).ok())
}

pub fn int(i: i64) -> Value {
    if (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&i) {
        Value::Number(i.into())
    } else {
        Value::String(i.to_string())
    }
}

pub fn uint(u: u64) -> Value {
    if u <= MAX_SAFE_INTEGER as u64 {
        Value::Number(u.into())
    } else {
        Value::String(u.to_string())
    }
}

pub fn float(f: f64) -> Value {
    Number::from_f64(f)
        .map(Value::Number)
        .unwrap_or_else(|| Value::String(f.to_string()))
}

/// Broad category of a column type, used to convert text-protocol values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Int,
    Float,
    Decimal,
    Bool,
    Binary,
    Text,
}

/// Classify a driver type name (`INT UNSIGNED`, `int8`, `BYTEA`, `TINYINT(1)`, ...).
pub fn kind_of(type_name: &str) -> Kind {
    let t = type_name.to_ascii_uppercase();
    let base = t.split(['(', ' ']).next().unwrap_or("");
    match base {
        "BOOL" | "BOOLEAN" => Kind::Bool,
        "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "INTEGER" | "BIGINT" | "INT2" | "INT4"
        | "INT8" | "YEAR" | "OID" | "SERIAL" | "BIGSERIAL" => Kind::Int,
        "FLOAT" | "DOUBLE" | "REAL" | "FLOAT4" | "FLOAT8" => Kind::Float,
        "DECIMAL" | "NUMERIC" | "MONEY" => Kind::Decimal,
        "BLOB" | "TINYBLOB" | "MEDIUMBLOB" | "LONGBLOB" | "BINARY" | "VARBINARY" | "BYTEA"
        | "BIT" | "GEOMETRY" => Kind::Binary,
        _ => Kind::Text,
    }
}

/// Convert a text-protocol value (MySQL text results, PostgreSQL simple query) to JSON.
pub fn from_text(type_name: &str, dialect: Dialect, raw: Option<&[u8]>) -> Value {
    let Some(bytes) = raw else {
        return Value::Null;
    };
    let kind = kind_of(type_name);
    if kind == Kind::Binary {
        if dialect == Dialect::Postgres {
            // bytea hex output: \x0102...
            if let Some(h) = bytes.strip_prefix(b"\\x") {
                if let Ok(b) = hex::decode(h) {
                    return binary(&b);
                }
            }
        }
        return binary(bytes);
    }
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t,
        Err(_) => return binary(bytes),
    };
    match kind {
        Kind::Int => text
            .parse::<i64>()
            .map(int)
            .or_else(|_| text.parse::<u64>().map(uint))
            .unwrap_or_else(|_| Value::String(text.into())),
        Kind::Float => text
            .parse::<f64>()
            .ok()
            .filter(|f| f.is_finite())
            .map(float)
            .unwrap_or_else(|| Value::String(text.into())),
        Kind::Bool => match text {
            "t" | "true" | "1" => Value::Bool(true),
            "f" | "false" | "0" => Value::Bool(false),
            _ => Value::String(text.into()),
        },
        _ => Value::String(text.into()),
    }
}

/// SQL literal for a cell value: `NULL`, `42`, `'text'`, `X'89'`.
pub fn sql_literal(dialect: Dialect, v: &Value) -> String {
    match v {
        Value::Null => "NULL".into(),
        Value::Bool(b) => match dialect {
            Dialect::Sqlite | Dialect::SqlServer | Dialect::Oracle => {
                if *b { "1" } else { "0" }.into()
            }
            _ => if *b { "TRUE" } else { "FALSE" }.into(),
        },
        Value::Number(n) => n.to_string(),
        Value::String(s) => dialect.quote_value(s),
        Value::Object(_) if is_default(v) => "DEFAULT".into(),
        Value::Object(_) if as_binary(v).is_some() => {
            let h = hex::encode(as_binary(v).unwrap_or_default());
            match dialect {
                Dialect::Postgres => format!("'\\x{h}'::bytea"),
                Dialect::SqlServer => format!("0x{h}"),
                Dialect::Oracle => format!("HEXTORAW('{h}')"),
                Dialect::ClickHouse => format!("unhex('{h}')"),
                _ => format!("X'{h}'"),
            }
        }
        other => dialect.quote_value(&other.to_string()),
    }
}

/// Plain-text rendering (CSV, clipboard): NULL as empty, binary as `0x…`.
pub fn display(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Object(_) if as_binary(v).is_some() => {
            format!("0x{}", hex::encode_upper(as_binary(v).unwrap_or_default()))
        }
        other => other.to_string(),
    }
}

/// Value for JSON export: binary as a `0x…` string.
pub fn export_json(v: &Value) -> Value {
    match v {
        Value::Object(_) if as_binary(v).is_some() => Value::String(display(v)),
        other => other.clone(),
    }
}

/// Build a JSON object from column names and one row.
pub fn row_object(columns: &[String], row: &[Value]) -> Map<String, Value> {
    columns
        .iter()
        .zip(row)
        .map(|(c, v)| (c.clone(), export_json(v)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_numbers_become_strings() {
        assert_eq!(int(42), json!(42));
        assert_eq!(int(i64::MAX), json!("9223372036854775807"));
        assert_eq!(uint(u64::MAX), json!("18446744073709551615"));
        assert_eq!(float(f64::NAN), json!("NaN"));
    }

    #[test]
    fn text_protocol_values_are_typed() {
        let d = Dialect::MySql;
        assert_eq!(from_text("INT UNSIGNED", d, Some(b"7")), json!(7));
        assert_eq!(
            from_text("BIGINT", d, Some(b"9007199254740993")),
            json!("9007199254740993")
        );
        assert_eq!(from_text("DECIMAL", d, Some(b"19.990")), json!("19.990"));
        assert_eq!(from_text("DOUBLE", d, Some(b"1.5")), json!(1.5));
        assert_eq!(from_text("VARCHAR", d, None), Value::Null);
        assert_eq!(
            from_text("BLOB", d, Some(&[0x89, 0x50])),
            json!({"$hex": "8950"})
        );
        assert_eq!(
            from_text("bool", Dialect::Postgres, Some(b"t")),
            json!(true)
        );
        assert_eq!(
            from_text("BYTEA", Dialect::Postgres, Some(b"\\x0aff")),
            json!({"$hex": "0aff"})
        );
        assert_eq!(
            from_text(
                "TIMESTAMPTZ",
                Dialect::Postgres,
                Some(b"2024-01-02 03:04:05+00")
            ),
            json!("2024-01-02 03:04:05+00")
        );
    }

    #[test]
    fn literals_per_dialect() {
        assert_eq!(sql_literal(Dialect::MySql, &Value::Null), "NULL");
        assert_eq!(sql_literal(Dialect::Sqlite, &json!(true)), "1");
        assert_eq!(sql_literal(Dialect::Postgres, &json!(true)), "TRUE");
        assert_eq!(sql_literal(Dialect::MySql, &json!("O'Brien")), "'O''Brien'");
        assert_eq!(
            sql_literal(Dialect::Postgres, &json!({"$hex": "ff"})),
            "'\\xff'::bytea"
        );
        assert_eq!(
            sql_literal(Dialect::Sqlite, &json!({"$hex": "ff"})),
            "X'ff'"
        );
        assert_eq!(sql_literal(Dialect::MySql, &json!(1.25)), "1.25");
        assert_eq!(sql_literal(Dialect::MySql, &default_marker()), "DEFAULT");
    }

    #[test]
    fn display_renders_binary_and_null() {
        assert_eq!(display(&json!({"$hex": "0aff"})), "0x0AFF");
        assert_eq!(display(&Value::Null), "");
        assert_eq!(display(&json!(3)), "3");
    }
}
