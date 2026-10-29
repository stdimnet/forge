//! The formatter's fixture corpus: `tests/format/*.sql` hold the input, then a line
//! `-- @expected` and the expected output. Header lines `-- @dialect <id>` and
//! `-- @style key=value …` (fields of `SqlStyle`) set the options. Every expected output must
//! also be stable when formatted again. `FORGE_UPDATE_SNAPSHOTS=1` rewrites the expectations.

use std::fs;
use std::path::Path;

use forge_sqlcode::{format, SqlDialect, SqlStyle};
use serde_json::{Map, Value};

struct Case {
    dialect: SqlDialect,
    style: SqlStyle,
    header: String,
    input: String,
    expected: String,
}

fn parse(text: &str) -> Case {
    let mut dialect = SqlDialect::Generic;
    let mut style = Map::new();
    let mut header = String::new();
    let mut body = text;
    while let Some(line) = body
        .lines()
        .next()
        .filter(|l| l.starts_with("-- @") && !l.starts_with("-- @expected"))
    {
        header.push_str(line);
        header.push('\n');
        if let Some(d) = line.strip_prefix("-- @dialect ") {
            dialect = SqlDialect::parse(d.trim());
        }
        if let Some(s) = line.strip_prefix("-- @style ") {
            for kv in s.split_whitespace() {
                let (k, v) = kv.split_once('=').expect("key=value");
                let value = match v {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    n if n.parse::<u64>().is_ok() => Value::from(n.parse::<u64>().unwrap()),
                    s => Value::String(s.into()),
                };
                style.insert(k.into(), value);
            }
        }
        body = &body[line.len() + 1..];
    }
    let (input, expected) = body
        .split_once("-- @expected\n")
        .expect("-- @expected line");
    Case {
        dialect,
        style: serde_json::from_value(Value::Object(style)).expect("style"),
        header,
        input: input.to_string(),
        expected: expected.to_string(),
    }
}

#[test]
fn formats_the_corpus() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/format");
    let update = std::env::var_os("FORGE_UPDATE_SNAPSHOTS").is_some();
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "sql"))
        .collect();
    files.sort();
    assert!(files.len() >= 10, "fixture corpus missing");
    let mut failures = Vec::new();
    for path in files {
        let text = fs::read_to_string(&path).unwrap();
        let case = parse(&text);
        let actual = format(&case.input, case.dialect, &case.style);
        if update {
            fs::write(
                &path,
                format!("{}{}-- @expected\n{}", case.header, case.input, actual),
            )
            .unwrap();
            continue;
        }
        if actual != case.expected {
            failures.push(format!(
                "{}:\n--- expected\n{}--- actual\n{}",
                path.display(),
                case.expected,
                actual
            ));
            continue;
        }
        let again = format(&actual, case.dialect, &case.style);
        if again != actual {
            failures.push(format!("{} is not stable:\n{}", path.display(), again));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
