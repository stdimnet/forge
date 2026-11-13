//! Unified diffs as PHP CS Fixer, Pint and Rector print them (`--- Original` / `+++ New`,
//! `@@ -a,b +c,d @@` hunks): parsing, applying them to a text (the fixers' result without
//! touching the file) and the lines they change (where their problems are shown).

/// One line of a hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkLine {
    Context(String),
    Removed(String),
    Added(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// First line of the hunk in the original text, 1-based (0 for an empty original).
    pub old_start: usize,
    pub old_len: usize,
    pub new_start: usize,
    pub new_len: usize,
    pub lines: Vec<HunkLine>,
}

impl Hunk {
    /// The first changed line of the original, 0-based, and how many original lines change.
    pub fn changed_lines(&self) -> (usize, usize) {
        let mut line = self.old_start.saturating_sub(1);
        let mut first = None;
        let mut removed = 0;
        for l in &self.lines {
            match l {
                HunkLine::Context(_) => {
                    if first.is_some() {
                        break;
                    }
                    line += 1;
                }
                HunkLine::Removed(_) => {
                    first.get_or_insert(line);
                    removed += 1;
                    line += 1;
                }
                HunkLine::Added(_) => {
                    first.get_or_insert(line);
                }
            }
        }
        (first.unwrap_or(line), removed)
    }
}

/// `@@ -1,5 +1,6 @@` → (1, 5, 1, 6); a missing length is 1.
fn parse_header(line: &str) -> Option<(usize, usize, usize, usize)> {
    let rest = line.strip_prefix("@@")?;
    let end = rest.find("@@")?;
    let mut parts = rest[..end].split_whitespace();
    let range = |s: &str, sign: char| -> Option<(usize, usize)> {
        let s = s.strip_prefix(sign)?;
        Some(match s.split_once(',') {
            Some((a, b)) => (a.parse().ok()?, b.parse().ok()?),
            None => (s.parse().ok()?, 1),
        })
    };
    let (a, b) = range(parts.next()?, '-')?;
    let (c, d) = range(parts.next()?, '+')?;
    Some((a, b, c, d))
}

/// The hunks of a unified diff (file headers and `\ No newline` markers are skipped).
/// A hunk header without line numbers (`@@ @@`, PHP CS Fixer's short diff) yields nothing.
pub fn parse(diff: &str) -> Vec<Hunk> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut in_hunk = false;
    for line in diff.lines() {
        if line.starts_with("@@") {
            in_hunk = match parse_header(line) {
                Some((old_start, old_len, new_start, new_len)) => {
                    hunks.push(Hunk {
                        old_start,
                        old_len,
                        new_start,
                        new_len,
                        lines: Vec::new(),
                    });
                    true
                }
                None => false,
            };
            continue;
        }
        if !in_hunk || line.starts_with("\\") {
            continue;
        }
        if line.starts_with("--- ") || line.starts_with("+++ ") {
            // The next file's header (a hunk's own lines never look like this in fixer
            // output: they would be "--- " only for a removed line starting with "-- ").
            let h = hunks.last().expect("in a hunk");
            let (old, new) = h.lines.iter().fold((0, 0), |(o, n), l| match l {
                HunkLine::Context(_) => (o + 1, n + 1),
                HunkLine::Removed(_) => (o + 1, n),
                HunkLine::Added(_) => (o, n + 1),
            });
            if old >= h.old_len && new >= h.new_len {
                in_hunk = false;
                continue;
            }
        }
        let h = hunks.last_mut().expect("in a hunk");
        let l = match line.as_bytes().first() {
            Some(b'+') => HunkLine::Added(line[1..].to_string()),
            Some(b'-') => HunkLine::Removed(line[1..].to_string()),
            Some(b' ') => HunkLine::Context(line[1..].to_string()),
            // An empty context line whose leading space was trimmed.
            None => HunkLine::Context(String::new()),
            _ => continue,
        };
        h.lines.push(l);
    }
    hunks
}

/// Why a diff does not apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyError {
    /// The hunk that does not match the text, 1-based.
    pub hunk: usize,
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "hunk {} of the diff does not match the file; it was changed meanwhile",
            self.hunk
        )
    }
}

/// Apply `diff` to `text`. Hunks may have moved a few lines (searched near their place);
/// the line ending of `text` is kept.
pub fn apply(text: &str, diff: &str) -> Result<String, ApplyError> {
    let hunks = parse(diff);
    if hunks.is_empty() {
        return Ok(text.to_string());
    }
    let crlf = text.contains("\r\n");
    let normalized = text.replace("\r\n", "\n");
    let trailing_newline = normalized.ends_with('\n');
    let lines: Vec<&str> = if normalized.is_empty() {
        Vec::new()
    } else {
        normalized
            .strip_suffix('\n')
            .unwrap_or(&normalized)
            .split('\n')
            .collect()
    };
    let mut out: Vec<String> = Vec::new();
    let mut pos = 0usize;
    for (i, h) in hunks.iter().enumerate() {
        let old: Vec<&str> = h
            .lines
            .iter()
            .filter_map(|l| match l {
                HunkLine::Context(s) | HunkLine::Removed(s) => Some(s.as_str()),
                HunkLine::Added(_) => None,
            })
            .collect();
        let want = h.old_start.saturating_sub(1).max(pos);
        let matches_at = |at: usize| {
            at + old.len() <= lines.len() && lines[at..at + old.len()].iter().eq(old.iter())
        };
        let found = (0..=lines.len())
            .flat_map(|d| [want.checked_add(d), want.checked_sub(d)])
            .flatten()
            .filter(|&at| at >= pos && at <= lines.len())
            .find(|&at| matches_at(at));
        let Some(at) = found else {
            return Err(ApplyError { hunk: i + 1 });
        };
        out.extend(lines[pos..at].iter().map(|s| s.to_string()));
        for l in &h.lines {
            match l {
                HunkLine::Context(s) | HunkLine::Added(s) => out.push(s.clone()),
                HunkLine::Removed(_) => {}
            }
        }
        pos = at + old.len();
    }
    out.extend(lines[pos..].iter().map(|s| s.to_string()));
    let mut result = out.join("\n");
    if trailing_newline && !result.is_empty() {
        result.push('\n');
    }
    Ok(if crlf {
        result.replace('\n', "\r\n")
    } else {
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "--- Original\n+++ New\n@@ -1,5 +1,5 @@\n <?php\n-$a = array(1,2);\n+$a = [1, 2];\n \n function f()\n {\n@@ -8,3 +8,4 @@\n }\n \n-echo 'x' ;\n+echo 'x';\n+\n";

    #[test]
    fn parses_hunks_and_changed_lines() {
        let h = parse(DIFF);
        assert_eq!(h.len(), 2);
        assert_eq!((h[0].old_start, h[0].old_len), (1, 5));
        assert_eq!(h[0].changed_lines(), (1, 1));
        assert_eq!(h[1].changed_lines(), (9, 1));
    }

    #[test]
    fn applies_a_diff_and_keeps_line_endings() {
        let text = "<?php\n$a = array(1,2);\n\nfunction f()\n{\n}\n\n}\n\necho 'x' ;\n";
        assert_eq!(
            apply(text, DIFF).unwrap(),
            "<?php\n$a = [1, 2];\n\nfunction f()\n{\n}\n\n}\n\necho 'x';\n\n"
        );
        let crlf = text.replace('\n', "\r\n");
        assert!(apply(&crlf, DIFF)
            .unwrap()
            .starts_with("<?php\r\n$a = [1, 2];\r\n"));
    }

    #[test]
    fn hunks_that_moved_are_found_near_their_place() {
        let text = "// added\n// above\n<?php\n$a = array(1,2);\n\nfunction f()\n{\n}\n\n}\n\necho 'x' ;\n";
        let out = apply(text, DIFF).unwrap();
        assert!(out.starts_with("// added\n// above\n<?php\n$a = [1, 2];\n"));
        assert!(out.ends_with("echo 'x';\n\n"));
    }

    #[test]
    fn a_diff_that_does_not_match_fails() {
        let err = apply("<?php\n$b = 1;\n", DIFF).unwrap_err();
        assert_eq!(err.hunk, 1);
        assert_eq!(apply("x\n", "").unwrap(), "x\n");
    }

    #[test]
    fn short_headers_are_skipped() {
        assert!(parse("--- Original\n+++ New\n@@ @@\n-a\n+b\n").is_empty());
    }
}
