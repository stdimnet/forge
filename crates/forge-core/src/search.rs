//! Fuzzy file search for "Go to File" / Search Everywhere.
//!
//! The matcher scores like PHPStorm's: query characters must appear in order, matches at
//! the start, after separators (`/ _ - .`) and on camel humps score higher, as do runs of
//! consecutive characters. `OrCo` therefore ranks `OrderController.php` first. The UI has a
//! TypeScript twin of this matcher (`app/ui/src/features/search/fuzzy.ts`) used for
//! in-memory lists (actions, symbols); keep the two in sync.

use serde::Serialize;

use crate::workspace::{Workspace, DEFAULT_EXCLUDES};
use crate::Result;

const BONUS_START: i32 = 8;
const BONUS_SEPARATOR: i32 = 6;
const BONUS_CAMEL: i32 = 6;
const BONUS_CONSECUTIVE: i32 = 4;
const BONUS_CASE: i32 = 1;
const PENALTY_GAP: i32 = 1;
const PENALTY_HUMP_MISS: i32 = 2;
/// Hard cap on files visited per query so huge trees stay responsive.
const MAX_WALKED_FILES: usize = 200_000;

/// Result of matching a query against one candidate string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyMatch {
    pub score: i32,
    /// Matched character indices (in `char`s, not bytes) for highlighting.
    pub positions: Vec<usize>,
}

fn is_separator(c: char) -> bool {
    matches!(c, '/' | '\\' | '_' | '-' | '.' | ' ' | ':' | '$')
}

fn position_bonus(prev: Option<char>, cur: char) -> i32 {
    match prev {
        None => BONUS_START,
        Some(p) if is_separator(p) => BONUS_SEPARATOR,
        Some(p) if (p.is_lowercase() || p.is_ascii_digit()) && cur.is_uppercase() => BONUS_CAMEL,
        _ => 0,
    }
}

/// Fuzzy-match `query` against `candidate` (case-insensitive, in-order subsequence) and
/// return the best-scoring alignment, or `None` if not all query characters occur.
pub fn fuzzy_match(query: &str, candidate: &str) -> Option<FuzzyMatch> {
    let q: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).collect();
    let c: Vec<char> = candidate.chars().collect();
    let (m, n) = (q.len(), c.len());
    if m == 0 {
        return Some(FuzzyMatch {
            score: 0,
            positions: vec![],
        });
    }
    if m > n {
        return None;
    }
    let lower = |ch: char| ch.to_lowercase().next().unwrap_or(ch);
    let bonus: Vec<i32> = (0..n)
        .map(|j| position_bonus(j.checked_sub(1).map(|p| c[p]), c[j]))
        .collect();

    const NEG: i32 = i32::MIN / 4;
    // dp[i][j]: best score with q[i] matched at c[j]; from[i][j]: where q[i-1] matched.
    let mut dp = vec![vec![NEG; n]; m];
    let mut from = vec![vec![usize::MAX; n]; m];
    for i in 0..m {
        // Running best of dp[i-1][0..j-1] and its index, for non-consecutive predecessors.
        let (mut best, mut best_at) = (NEG, usize::MAX);
        for j in 0..n {
            if i > 0 && j >= 2 && dp[i - 1][j - 2] > best {
                best = dp[i - 1][j - 2];
                best_at = j - 2;
            }
            if lower(q[i]) != lower(c[j]) {
                continue;
            }
            let mut s = 1 + bonus[j];
            if q[i] == c[j] && q[i].is_uppercase() {
                s += BONUS_CASE;
            }
            if q[i].is_uppercase() && bonus[j] == 0 {
                s -= PENALTY_HUMP_MISS;
            }
            if i == 0 {
                dp[i][j] = s;
                continue;
            }
            // A run into a word start earns the boundary bonus, not the run bonus too, so
            // "OrCo" aligns O-r + C-o rather than r-C.
            let consecutive = if j >= 1 && dp[i - 1][j - 1] > NEG {
                dp[i - 1][j - 1] + if bonus[j] > 0 { 0 } else { BONUS_CONSECUTIVE }
            } else {
                NEG
            };
            let gapped = if best > NEG { best - PENALTY_GAP } else { NEG };
            if consecutive == NEG && gapped == NEG {
                continue;
            }
            if consecutive >= gapped {
                dp[i][j] = s + consecutive;
                from[i][j] = j - 1;
            } else {
                dp[i][j] = s + gapped;
                from[i][j] = best_at;
            }
        }
    }
    let (mut end, mut score) = (usize::MAX, NEG);
    for (j, &s) in dp[m - 1].iter().enumerate() {
        if s > score {
            score = s;
            end = j;
        }
    }
    if score == NEG {
        return None;
    }
    let mut positions = vec![0; m];
    let mut j = end;
    for i in (0..m).rev() {
        positions[i] = j;
        j = from[i][j];
    }
    Some(FuzzyMatch { score, positions })
}

/// One "Go to File" hit.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMatch {
    /// Workspace-relative path with `/` separators.
    pub path: String,
    pub name: String,
    pub score: i32,
    /// Highlight positions (char indices) into `name`, or into `path` when `matched_path`.
    pub positions: Vec<usize>,
    pub matched_path: bool,
}

/// Score a workspace-relative path: queries with `/` match the whole path, others the
/// file name.
pub fn match_file(query: &str, path: &str) -> Option<FileMatch> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let matched_path = query.contains('/');
    let target = if matched_path { path } else { name };
    let m = fuzzy_match(query, target)?;
    // Prefer short names and shallow paths on ties.
    let tiebreak = (target.chars().count() as i32) / 8 + path.matches('/').count() as i32 / 3;
    Some(FileMatch {
        path: path.to_string(),
        name: name.to_string(),
        score: m.score - tiebreak,
        positions: m.positions,
        matched_path,
    })
}

impl Workspace {
    /// Find files whose name (or path, if the query contains `/`) fuzzy-matches `query`.
    /// Respects `.gitignore`/`.ignore` and the default excludes; best matches first.
    pub fn find_files(&self, query: &str, limit: usize) -> Result<Vec<FileMatch>> {
        let query = query.trim();
        if query.is_empty() || limit == 0 {
            return Ok(vec![]);
        }
        let walker = ignore::WalkBuilder::new(self.root())
            .hidden(false)
            .require_git(false)
            .filter_entry(|e| !DEFAULT_EXCLUDES.contains(&e.file_name().to_string_lossy().as_ref()))
            .build();
        let mut hits: Vec<FileMatch> = walker
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
            .take(MAX_WALKED_FILES)
            .filter_map(|e| match_file(query, &self.relative(e.path())))
            .collect();
        hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
        hits.truncate(limit);
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn camel_humps_match() {
        let m = fuzzy_match("OrCo", "OrderController").unwrap();
        assert_eq!(m.positions, vec![0, 1, 5, 6]);
        assert!(fuzzy_match("xyz", "OrderController").is_none());
    }

    #[test]
    fn humps_beat_scattered_matches() {
        let hump = fuzzy_match("OC", "OrderController").unwrap();
        let scattered = fuzzy_match("OC", "Obscure").unwrap();
        assert!(hump.score > scattered.score);
    }

    #[test]
    fn prefix_beats_middle() {
        let a = fuzzy_match("user", "User.php").unwrap();
        let b = fuzzy_match("user", "AbuserLog.php").unwrap();
        assert!(a.score > b.score);
    }

    #[test]
    fn slash_queries_match_paths() {
        let hit = match_file("mod/us", "app/Models/User.php").unwrap();
        assert!(hit.matched_path);
        assert!(match_file("mod/us", "app/Http/Kernel.php").is_none());
    }

    #[test]
    fn find_files_respects_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("app/Http/Controllers")).unwrap();
        fs::create_dir_all(root.join("vendor/x")).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".gitignore"), "vendor/\n").unwrap();
        fs::write(root.join("app/Http/Controllers/OrderController.php"), "").unwrap();
        fs::write(root.join("app/Http/Controllers/Controller.php"), "").unwrap();
        fs::write(root.join("vendor/x/OrderCollection.php"), "").unwrap();
        fs::write(root.join(".git/ORIG_HEAD"), "").unwrap();
        let ws = Workspace::open(root).unwrap();

        let hits = ws.find_files("OrCo", 10).unwrap();
        let paths: Vec<_> = hits.iter().map(|h| h.path.as_str()).collect();
        assert_eq!(paths, vec!["app/Http/Controllers/OrderController.php"]);

        let hits = ws.find_files("contr", 1).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "Controller.php");
        assert!(ws.find_files("ORIG", 10).unwrap().is_empty());
        assert!(ws.find_files("  ", 10).unwrap().is_empty());
    }
}
