//! LCOV tracefiles: `TN:<test>`, `SF:<file>`, `FN:<line>[,<end>],<name>`,
//! `FNDA:<hits>,<name>`, `DA:<line>,<hits>[,<checksum>]`,
//! `BRDA:<line>,<block>,<branch>,<taken | ->`, `end_of_record`. Summary records (`FNF`,
//! `FNH`, `LF`, `LH`, `BRF`, `BRH`) are recomputed. A named test (`TN:` with a name, as
//! `geninfo --test-name` or per-test tracefiles write) is recorded on the lines it ran.

use std::collections::BTreeMap;

use super::{Builder, CoverageReport, LineHits};

/// Functions of the current file: name → (line, hits).
type Functions = BTreeMap<String, (u32, u64)>;

pub fn parse(text: &str) -> Result<CoverageReport, String> {
    let mut b = Builder::default();
    let mut file: Option<String> = None;
    let mut test = String::new();
    let mut functions: Functions = BTreeMap::new();
    // Branches of the current file: line → (taken, total).
    let mut branches: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    let mut lines: BTreeMap<u32, u64> = BTreeMap::new();
    let flush = |b: &mut Builder,
                 file: &Option<String>,
                 test: &str,
                 lines: &mut BTreeMap<u32, u64>,
                 branches: &mut BTreeMap<u32, (u32, u32)>,
                 functions: &mut Functions| {
        let Some(f) = file else {
            return;
        };
        b.file(f);
        for (name, (line, hits)) in std::mem::take(functions) {
            b.function(f, &name, line, hits);
        }
        let tests = |hits: u64| {
            if hits > 0 && !test.is_empty() {
                vec![test.to_string()]
            } else {
                Vec::new()
            }
        };
        for (line, hits) in std::mem::take(lines) {
            b.line(
                f,
                LineHits {
                    line,
                    hits,
                    branches: branches.remove(&line),
                    partial: false,
                    tests: tests(hits),
                },
            );
        }
        // Branches on lines without a DA record still count as lines that ran.
        for (line, (taken, total)) in std::mem::take(branches) {
            b.line(
                f,
                LineHits {
                    line,
                    hits: u64::from(taken > 0),
                    branches: Some((taken, total)),
                    partial: false,
                    tests: tests(u64::from(taken > 0)),
                },
            );
        }
    };
    let mut seen = false;
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(name) = line.strip_prefix("TN:") {
            test = name.trim().to_string();
            seen = true;
        } else if let Some(path) = line.strip_prefix("SF:") {
            flush(
                &mut b,
                &file,
                &test,
                &mut lines,
                &mut branches,
                &mut functions,
            );
            file = Some(path.trim().to_string());
            seen = true;
        } else if let Some(fun) = line.strip_prefix("FN:") {
            // `FN:<line>,<name>` or (LCOV 2) `FN:<line>,<end line>,<name>`.
            let Some((n, rest)) = fun.split_once(',') else {
                continue;
            };
            let Ok(n) = n.trim().parse::<u32>() else {
                continue;
            };
            let name = match rest.split_once(',') {
                Some((end, name)) if end.trim().parse::<u32>().is_ok() => name,
                _ => rest,
            };
            functions.entry(name.trim().to_string()).or_insert((n, 0)).0 = n;
        } else if let Some(fnda) = line.strip_prefix("FNDA:") {
            let Some((hits, name)) = fnda.split_once(',') else {
                continue;
            };
            let hits = hits.trim().parse::<f64>().unwrap_or(0.0).max(0.0) as u64;
            let e = functions.entry(name.trim().to_string()).or_insert((0, 0));
            e.1 = e.1.saturating_add(hits);
        } else if let Some(da) = line.strip_prefix("DA:") {
            let mut parts = da.split(',');
            let (Some(n), Some(h)) = (parts.next(), parts.next()) else {
                continue;
            };
            let (Ok(n), Ok(h)) = (n.trim().parse::<u32>(), h.trim().parse::<f64>()) else {
                continue;
            };
            let e = lines.entry(n).or_default();
            *e = e.saturating_add(h.max(0.0) as u64);
        } else if let Some(br) = line.strip_prefix("BRDA:") {
            let parts: Vec<&str> = br.split(',').collect();
            if parts.len() < 4 {
                continue;
            }
            let Ok(n) = parts[0].trim().parse::<u32>() else {
                continue;
            };
            let taken = parts[3].trim().parse::<u64>().is_ok_and(|t| t > 0);
            let e = branches.entry(n).or_default();
            e.0 += u32::from(taken);
            e.1 += 1;
        } else if line == "end_of_record" {
            flush(
                &mut b,
                &file,
                &test,
                &mut lines,
                &mut branches,
                &mut functions,
            );
            file = None;
        }
    }
    flush(
        &mut b,
        &file,
        &test,
        &mut lines,
        &mut branches,
        &mut functions,
    );
    if !seen {
        return Err("not an LCOV report".into());
    }
    Ok(b.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage::tests::{fixture, lines};
    use crate::coverage::FunctionHits;
    use crate::coverage::LineStatus::*;

    #[test]
    fn reads_lines_and_branches() {
        let r = parse(&fixture("lcov.info")).unwrap();
        assert_eq!(r.files.len(), 2);
        assert_eq!(
            lines(&r, "/p/src/cart.js"),
            [
                (1, 3, Covered),
                (2, 3, Covered),
                (4, 2, Partial),
                (5, 2, Covered),
                (7, 1, Covered),
                (9, 0, Uncovered)
            ]
        );
        assert_eq!(r.files[0].branch_counter().total, 2);
        assert_eq!(r.files[0].method_counter().text(), "100% (1/1)");
        assert_eq!(r.files[0].functions[0].name, "addItem");
        assert!(r.files[0].lines.iter().all(|l| l.tests.is_empty()));
        assert!(parse("garbage").is_err());
    }

    #[test]
    fn named_tests_and_lcov2_functions() {
        let r = parse(&fixture("lcov-tests.info")).unwrap();
        let f = r.file("src/cart.js").unwrap();
        let tests = |n: u32| f.lines.iter().find(|l| l.line == n).unwrap().tests.clone();
        assert_eq!(tests(1), ["adds items", "computes the total"]);
        assert_eq!(tests(2), ["adds items"]);
        assert!(tests(3).is_empty(), "never ran");
        assert_eq!(f.lines[0].hits, 3);
        assert_eq!(
            f.functions,
            [
                FunctionHits {
                    name: "addItem".into(),
                    line: 1,
                    hits: 2
                },
                FunctionHits {
                    name: "clear".into(),
                    line: 3,
                    hits: 0
                }
            ]
        );
        assert_eq!(f.marks()[0].hint, "Hits: 3, 2 tests");
    }
}
