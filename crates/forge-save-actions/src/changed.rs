//! "Reformat code: Changed lines": keep only the reformatting of lines that changed since
//! the last commit. Lines are compared as a line diff; a hunk of the reformatting is applied
//! when it touches a changed line, otherwise the original lines stay.

use std::collections::HashSet;

use forge_text::save::modified_lines;

/// Above this many line pairs the whole reformatting is kept instead of running the
/// quadratic diff.
const MAX_DIFF_CELLS: usize = 4_000_000;

/// `formatted` limited to the lines of `original` that differ from `base` (the committed
/// text). Without a base (new or untracked file) the whole file counts as changed, like
/// PHPStorm.
pub fn restrict_to_changed_lines(base: Option<&str>, original: &str, formatted: &str) -> String {
    let Some(base) = base else {
        return formatted.to_string();
    };
    let changed = modified_lines(base, original);
    if changed.is_empty() {
        return original.to_string();
    }
    let a: Vec<&str> = original.split('\n').collect();
    let b: Vec<&str> = formatted.split('\n').collect();
    let Some(hunks) = diff_hunks(&a, &b) else {
        return formatted.to_string();
    };
    let mut out: Vec<&str> = Vec::with_capacity(b.len());
    let mut ai = 0;
    // A replacement of as many lines as it adds is a line-by-line change.
    let hunks = hunks.into_iter().flat_map(|h| {
        let n = h.a_end - h.a_start;
        if n > 1 && n == h.b_end - h.b_start {
            (0..n)
                .map(|k| Hunk {
                    a_start: h.a_start + k,
                    a_end: h.a_start + k + 1,
                    b_start: h.b_start + k,
                    b_end: h.b_start + k + 1,
                })
                .collect()
        } else {
            vec![h]
        }
    });
    for h in hunks {
        out.extend_from_slice(&a[ai..h.a_start]);
        if touches(&h, &changed) {
            out.extend_from_slice(&b[h.b_start..h.b_end]);
        } else {
            out.extend_from_slice(&a[h.a_start..h.a_end]);
        }
        ai = h.a_end;
    }
    out.extend_from_slice(&a[ai..]);
    out.join("\n")
}

/// A replaced range: `a[a_start..a_end]` became `b[b_start..b_end]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hunk {
    a_start: usize,
    a_end: usize,
    b_start: usize,
    b_end: usize,
}

fn touches(h: &Hunk, changed: &HashSet<usize>) -> bool {
    if h.a_start == h.a_end {
        // A pure insertion counts for the lines around it.
        return changed.contains(&h.a_start) || (h.a_start > 0 && changed.contains(&(h.a_start - 1)));
    }
    (h.a_start..h.a_end).any(|i| changed.contains(&i))
}

/// The hunks of a line diff (longest common subsequence); `None` when it is too large.
fn diff_hunks(a: &[&str], b: &[&str]) -> Option<Vec<Hunk>> {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let max_suffix = a.len().min(b.len()) - prefix;
    let suffix = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(max_suffix)
        .take_while(|(x, y)| x == y)
        .count();
    let am = &a[prefix..a.len() - suffix];
    let bm = &b[prefix..b.len() - suffix];
    if am.len().saturating_mul(bm.len()) > MAX_DIFF_CELLS {
        return None;
    }
    let (n, m) = (am.len(), bm.len());
    // lcs[i][j]: length of the LCS of am[i..] and bm[j..].
    let mut lcs = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[at(i, j)] = if am[i] == bm[j] {
                lcs[at(i + 1, j + 1)] + 1
            } else {
                lcs[at(i + 1, j)].max(lcs[at(i, j + 1)])
            };
        }
    }
    let mut hunks = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut open: Option<(usize, usize)> = None;
    while i < n || j < m {
        if i < n && j < m && am[i] == bm[j] {
            if let Some((s, t)) = open.take() {
                hunks.push(Hunk {
                    a_start: prefix + s,
                    a_end: prefix + i,
                    b_start: prefix + t,
                    b_end: prefix + j,
                });
            }
            i += 1;
            j += 1;
            continue;
        }
        open.get_or_insert((i, j));
        if j < m && (i == n || lcs[at(i, j + 1)] >= lcs[at(i + 1, j)]) {
            j += 1;
        } else {
            i += 1;
        }
    }
    if let Some((s, t)) = open {
        hunks.push(Hunk {
            a_start: prefix + s,
            a_end: prefix + n,
            b_start: prefix + t,
            b_end: prefix + m,
        });
    }
    Some(hunks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_changed_lines_are_reformatted() {
        let base = "a( 1 );\nb( 2 );\nc( 3 );\n";
        let original = "a( 1 );\nb( 22 );\nc( 3 );\n";
        let formatted = "a(1);\nb(22);\nc(3);\n";
        assert_eq!(
            restrict_to_changed_lines(Some(base), original, formatted),
            "a( 1 );\nb(22);\nc( 3 );\n"
        );
    }

    #[test]
    fn untracked_files_are_reformatted_completely() {
        assert_eq!(restrict_to_changed_lines(None, "a( 1 );", "a(1);"), "a(1);");
    }

    #[test]
    fn unchanged_files_stay_as_they_are() {
        let text = "a( 1 );\n";
        assert_eq!(restrict_to_changed_lines(Some(text), text, "a(1);\n"), text);
    }

    #[test]
    fn insertions_next_to_changed_lines_are_kept() {
        let base = "if (x) {\n}\n";
        let original = "if (x) { y(); }\n";
        let formatted = "if (x) {\n    y();\n}\n";
        assert_eq!(
            restrict_to_changed_lines(Some(base), original, formatted),
            formatted
        );
    }

    #[test]
    fn hunks_cover_every_difference() {
        let a = ["1", "2", "3", "4"];
        let b = ["1", "x", "3", "4", "5"];
        let h = diff_hunks(&a, &b).unwrap();
        assert_eq!(
            h,
            vec![
                Hunk { a_start: 1, a_end: 2, b_start: 1, b_end: 2 },
                Hunk { a_start: 4, a_end: 4, b_start: 4, b_end: 5 },
            ]
        );
    }
}
