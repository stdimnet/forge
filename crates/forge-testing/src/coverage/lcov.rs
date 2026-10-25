//! LCOV tracefiles: `SF:<file>`, `DA:<line>,<hits>[,<checksum>]`,
//! `BRDA:<line>,<block>,<branch>,<taken | ->`, `end_of_record`. Function and summary
//! records (`FN`, `FNDA`, `LF`, `LH`, `BRF`, `BRH`) are recomputed from the lines.

use std::collections::BTreeMap;

use super::{Builder, CoverageReport, LineHits};

pub fn parse(text: &str) -> Result<CoverageReport, String> {
    let mut b = Builder::default();
    let mut file: Option<String> = None;
    // Branches of the current file: line → (taken, total).
    let mut branches: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    let mut lines: BTreeMap<u32, u64> = BTreeMap::new();
    let flush = |b: &mut Builder,
                 file: &Option<String>,
                 lines: &mut BTreeMap<u32, u64>,
                 branches: &mut BTreeMap<u32, (u32, u32)>| {
        let Some(f) = file else {
            return;
        };
        b.file(f);
        for (line, hits) in std::mem::take(lines) {
            b.line(
                f,
                LineHits {
                    line,
                    hits,
                    branches: branches.remove(&line),
                    partial: false,
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
                },
            );
        }
    };
    let mut seen = false;
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(path) = line.strip_prefix("SF:") {
            flush(&mut b, &file, &mut lines, &mut branches);
            file = Some(path.trim().to_string());
            seen = true;
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
            let taken = parts[3].trim().parse::<u64>().map_or(false, |t| t > 0);
            let e = branches.entry(n).or_default();
            e.0 += u32::from(taken);
            e.1 += 1;
        } else if line == "end_of_record" {
            flush(&mut b, &file, &mut lines, &mut branches);
            file = None;
        }
    }
    flush(&mut b, &file, &mut lines, &mut branches);
    if !seen {
        return Err("not an LCOV report".into());
    }
    Ok(b.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage::tests::{fixture, lines};
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
        assert!(parse("garbage").is_err());
    }
}
