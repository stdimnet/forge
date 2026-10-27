//! Istanbul's JSON coverage (`coverage-final.json`), written by nyc and Istanbul's `json`
//! reporter, by Jest (`--coverageReporters=json`) and Vitest (`--coverage.reporter=json`,
//! with the v8 and the istanbul provider alike) and by c8 (`--reporter=json`):
//!
//! ```json
//! { "/p/src/cart.ts": { "path": "/p/src/cart.ts",
//!     "statementMap": { "0": { "start": { "line": 1, "column": 0 }, "end": { … } } },
//!     "fnMap": { "0": { "name": "add", "decl": { … }, "loc": { … }, "line": 1 } },
//!     "branchMap": { "0": { "type": "if", "loc": { … }, "locations": [ … ], "line": 4 } },
//!     "s": { "0": 3 }, "f": { "0": 3 }, "b": { "0": [2, 0] } } }
//! ```
//!
//! A line's hits are the most hits of the statements starting on it (like Istanbul's own
//! line coverage); a line where some statements ran and others did not is partly covered.
//! Each branch location is a branch of the line the branch starts on; `fnMap` are the
//! functions. Older nyc versions wrap a file's data in `{ "data": { … } }`.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{Builder, CoverageReport, LineHits};

/// Whether a JSON text is Istanbul coverage.
pub fn sniff(text: &str) -> bool {
    text.trim_start().starts_with('{') && text.contains("\"statementMap\"")
}

/// The 1-based start line of a location (`{ start: { line } }`).
fn start_line(loc: &Value) -> Option<u32> {
    loc.get("start")?.get("line")?.as_u64().map(|l| l as u32)
}

fn count(v: Option<&Value>) -> u64 {
    v.and_then(|c| c.as_f64()).unwrap_or(0.0).max(0.0) as u64
}

pub fn parse(text: &str) -> Result<CoverageReport, String> {
    let root: Value = serde_json::from_str(text).map_err(|e| format!("Istanbul JSON: {e}"))?;
    let files = root
        .as_object()
        .ok_or("Istanbul JSON: not an object of files")?;
    let mut b = Builder::default();
    let mut seen = false;
    for (key, data) in files {
        let data = data.get("data").filter(|d| d.is_object()).unwrap_or(data);
        let Some(statements) = data.get("statementMap").and_then(Value::as_object) else {
            continue;
        };
        seen = true;
        let path = data
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or(key)
            .replace('\\', "/");
        b.file(&path);
        // line → (most hits, statements that ran, statements)
        let mut lines: BTreeMap<u32, (u64, u32, u32)> = BTreeMap::new();
        let hits = data.get("s");
        for (id, loc) in statements {
            let Some(line) = start_line(loc) else {
                continue;
            };
            let n = count(hits.and_then(|s| s.get(id)));
            let e = lines.entry(line).or_default();
            e.0 = e.0.max(n);
            e.1 += u32::from(n > 0);
            e.2 += 1;
        }
        let mut branches: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
        if let Some(map) = data.get("branchMap").and_then(Value::as_object) {
            let counts = data.get("b");
            for (id, br) in map {
                let line = br
                    .get("line")
                    .and_then(Value::as_u64)
                    .map(|l| l as u32)
                    .or_else(|| br.get("loc").and_then(start_line))
                    .or_else(|| {
                        br.get("locations")
                            .and_then(|l| l.get(0))
                            .and_then(start_line)
                    });
                let Some(line) = line else {
                    continue;
                };
                let taken: Vec<u64> = counts
                    .and_then(|c| c.get(id))
                    .and_then(Value::as_array)
                    .map(|a| a.iter().map(|c| count(Some(c))).collect())
                    .unwrap_or_default();
                let total = br
                    .get("locations")
                    .and_then(Value::as_array)
                    .map_or(taken.len(), |l| l.len().max(taken.len()));
                let e = branches.entry(line).or_default();
                e.0 += taken.iter().filter(|c| **c > 0).count() as u32;
                e.1 += total as u32;
            }
        }
        if let Some(map) = data.get("fnMap").and_then(Value::as_object) {
            let counts = data.get("f");
            for (id, f) in map {
                let line = f
                    .get("line")
                    .and_then(Value::as_u64)
                    .map(|l| l as u32)
                    .or_else(|| f.get("decl").and_then(start_line))
                    .or_else(|| f.get("loc").and_then(start_line))
                    .unwrap_or(0);
                let name = match f.get("name").and_then(Value::as_str) {
                    Some(n) if !n.is_empty() => n.to_string(),
                    _ => format!("(anonymous_{id})"),
                };
                b.function(&path, &name, line, count(counts.and_then(|c| c.get(id))));
            }
        }
        for (line, (hits, ran, total)) in lines {
            b.line(
                &path,
                LineHits {
                    line,
                    hits,
                    branches: branches.remove(&line),
                    partial: ran > 0 && ran < total,
                    tests: Vec::new(),
                },
            );
        }
        for (line, (taken, total)) in branches {
            b.line(
                &path,
                LineHits {
                    line,
                    hits: u64::from(taken > 0),
                    branches: Some((taken, total)),
                    partial: false,
                    tests: Vec::new(),
                },
            );
        }
    }
    if !seen && !files.is_empty() {
        return Err("not Istanbul JSON coverage".into());
    }
    Ok(b.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage::tests::{fixture, lines};
    use crate::coverage::{CoverageFormat, LineStatus::*};

    #[test]
    fn reads_statements_branches_and_functions() {
        let text = fixture("coverage-final.json");
        assert_eq!(
            CoverageReport::sniff(&text),
            Some(CoverageFormat::IstanbulJson)
        );
        let r = parse(&text).unwrap();
        assert_eq!(
            lines(&r, "/p/src/cart.ts"),
            [
                (1, 3, Covered),
                (2, 3, Covered),
                (4, 3, Partial),
                (5, 2, Covered),
                (7, 1, Partial),
                (9, 0, Uncovered),
            ]
        );
        let f = r.file("/p/src/cart.ts").unwrap();
        assert_eq!(f.branch_counter().text(), "50% (2/4)");
        assert_eq!(f.method_counter().text(), "50% (1/2)");
        assert_eq!(f.functions[0].name, "addItem");
        assert_eq!(f.functions[1].name, "(anonymous_1)");
        let marks = f.marks();
        assert_eq!(marks[2].hint, "Hits: 3, branches: 1 of 2 covered");
        assert_eq!(marks[4].hint, "Hits: 1, branches: 1 of 2 covered");
        // nyc's older `{ "data": … }` wrapping, Windows paths.
        let old = parse(&fixture("coverage-nyc-data.json")).unwrap();
        assert_eq!(
            lines(&old, "C:/p/lib/a.js"),
            [(1, 1, Covered), (2, 0, Uncovered)]
        );
        assert!(parse("[]").is_err());
        assert!(parse("{\"a\": {\"x\": 1}}").is_err());
        assert!(parse("{}").unwrap().files.is_empty());
    }
}
