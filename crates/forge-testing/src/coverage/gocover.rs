//! Go coverage profiles (`go test -coverprofile=cover.out`, any `-covermode`):
//!
//! ```text
//! mode: count
//! example.com/shop/cart/cart.go:10.32,12.2 1 3
//! ```
//!
//! Each block (`file:startLine.startCol,endLine.endCol statements count`) covers its lines;
//! a line where some blocks ran and others did not is partly covered. Paths are import
//! paths: a framework's location rules map them into the project.

use std::collections::BTreeMap;

use super::{Builder, CoverageReport, LineHits};

/// Most lines one block may cover. A Go source file is far smaller; a profile that claims
/// more (`1.1,4000000000.1`) is damaged and its block is skipped instead of filling memory.
const MAX_BLOCK_LINES: u32 = 1_000_000;

pub fn parse(text: &str) -> Result<CoverageReport, String> {
    let mut lines_iter = text.lines();
    let first = lines_iter.next().unwrap_or_default().trim();
    if !first.starts_with("mode:") {
        return Err("not a Go coverage profile".into());
    }
    // file → line → (max count, blocks that ran, blocks)
    let mut files: Vec<String> = Vec::new();
    let mut acc: BTreeMap<String, BTreeMap<u32, (u64, u32, u32)>> = BTreeMap::new();
    for raw in lines_iter {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("mode:") {
            continue;
        }
        let Some((file, rest)) = line.rsplit_once(':') else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let (Some(span), Some(_stmts), Some(count)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Ok(count) = count.parse::<u64>() else {
            continue;
        };
        let Some((start, end)) = span.split_once(',') else {
            continue;
        };
        let line_of = |p: &str| p.split('.').next().and_then(|l| l.parse::<u32>().ok());
        let (Some(from), Some(to)) = (line_of(start), line_of(end)) else {
            continue;
        };
        if to.saturating_sub(from) > MAX_BLOCK_LINES {
            continue;
        }
        if !acc.contains_key(file) {
            files.push(file.to_string());
        }
        let per_line = acc.entry(file.to_string()).or_default();
        for l in from..=to.max(from) {
            let e = per_line.entry(l).or_default();
            e.0 = e.0.max(count);
            e.1 = e.1.saturating_add(u32::from(count > 0));
            e.2 = e.2.saturating_add(1);
        }
    }
    let mut b = Builder::default();
    for f in &files {
        b.file(f);
        for (line, (count, ran, blocks)) in acc.remove(f).unwrap_or_default() {
            b.line(
                f,
                LineHits {
                    line,
                    hits: count,
                    branches: None,
                    partial: ran > 0 && ran < blocks,
                    tests: Vec::new(),
                },
            );
        }
    }
    Ok(b.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage::tests::{fixture, lines};
    use crate::coverage::LineStatus::*;

    #[test]
    fn blocks_cover_their_lines() {
        let r = parse(&fixture("cover.out")).unwrap();
        assert_eq!(
            lines(&r, "example.com/shop/cart/cart.go"),
            [
                (10, 3, Covered),
                (11, 3, Covered),
                (12, 3, Partial),
                (13, 0, Uncovered),
                (14, 0, Uncovered),
            ]
        );
        assert!(parse("SF:x").is_err());
    }

    #[test]
    fn damaged_spans_are_skipped() {
        let r = parse("mode: set\na.go:1.1,4000000000.1 1 1\na.go:2.1,3.1 1 1\n").unwrap();
        assert_eq!(lines(&r, "a.go"), [(2, 1, Covered), (3, 1, Covered)]);
    }
}
