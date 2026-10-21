//! PHPStorm-style fuzzy matching, shared by the backend ("Go to File" over the file system)
//! and the UI (Search Everywhere, Recent Files, popups) through IPC and WebAssembly.
//!
//! Query characters must appear in order (case-insensitive). Matches at the start, after
//! separators (`/ _ - . : $` and spaces) and on camel humps score higher, as do runs of
//! consecutive characters, so `OrCo` ranks `OrderController` first. The crate is pure (no
//! file system or threads) so it compiles to WebAssembly for browser mode.

use serde::{Deserialize, Serialize};

pub mod everywhere;

const BONUS_START: i32 = 8;
const BONUS_SEPARATOR: i32 = 6;
const BONUS_CAMEL: i32 = 6;
const BONUS_CONSECUTIVE: i32 = 4;
const BONUS_CASE: i32 = 1;
const PENALTY_GAP: i32 = 1;
const PENALTY_HUMP_MISS: i32 = 2;

/// Result of matching a query against one candidate string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FuzzyMatch {
    pub score: i32,
    /// Matched character indices (in `char`s, not bytes) for highlighting.
    pub positions: Vec<usize>,
}

/// How a query may align with a candidate.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchOptions {
    /// PHPStorm-style word matching: after the first character, a match may only jump ahead
    /// to a word start (camel hump or after a separator). "line" then finds "Move Line Up"
    /// but not "Collapse Recursively". Used for actions, classes and symbols.
    #[serde(default)]
    pub strict: bool,
    /// Plain case-insensitive substring match instead of fuzzy matching.
    #[serde(default)]
    pub exact: bool,
    /// Also accept a query with one swapped pair of neighbouring characters or one extra
    /// character ("ordre" finds "Order"), scored below a clean match.
    #[serde(default)]
    pub typo: bool,
}

impl MatchOptions {
    pub const STRICT: Self = Self {
        strict: true,
        exact: false,
        typo: false,
    };
}

const PENALTY_TYPO: i32 = 12;
const TYPO_MIN_CHARS: usize = 4;

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

/// Fuzzy-match `query` against `candidate` and return the best-scoring alignment, or `None`
/// if not all query characters occur in order. Whitespace in the query is ignored; an empty
/// query matches everything with score 0.
pub fn fuzzy_match(query: &str, candidate: &str) -> Option<FuzzyMatch> {
    fuzzy_match_with(query, candidate, MatchOptions::default())
}

/// [`fuzzy_match`] with options (e.g. strict word matching).
pub fn fuzzy_match_with(query: &str, candidate: &str, opts: MatchOptions) -> Option<FuzzyMatch> {
    if opts.exact {
        return exact_match(query, candidate);
    }
    if opts.typo {
        let clean = MatchOptions {
            typo: false,
            ..opts
        };
        return fuzzy_match_with(query, candidate, clean)
            .or_else(|| typo_match(query, candidate, clean));
    }
    // Most candidates fail the "characters in order" test, so reject them before any allocation.
    let fold = |ch: char| ch.to_lowercase().next().unwrap_or(ch);
    let mut wanted = query.chars().filter(|c| !c.is_whitespace()).map(fold);
    let mut next = wanted.next();
    if next.is_some() {
        for ch in candidate.chars() {
            if Some(fold(ch)) == next {
                next = wanted.next();
                if next.is_none() {
                    break;
                }
            }
        }
        if next.is_some() {
            return None;
        }
    }
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
            let gapped = if best > NEG && !(opts.strict && bonus[j] == 0) {
                best - PENALTY_GAP
            } else {
                NEG
            };
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

/// The best match of `query` with one adjacent pair swapped or one character left out.
fn typo_match(query: &str, candidate: &str, opts: MatchOptions) -> Option<FuzzyMatch> {
    let q: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).collect();
    if q.len() < TYPO_MIN_CHARS {
        return None;
    }
    let mut variants: Vec<String> = Vec::new();
    for i in 0..q.len() - 1 {
        if q[i] != q[i + 1] {
            let mut v = q.clone();
            v.swap(i, i + 1);
            variants.push(v.into_iter().collect());
        }
    }
    for i in 0..q.len() {
        let mut v = q.clone();
        v.remove(i);
        variants.push(v.into_iter().collect());
    }
    variants
        .iter()
        .filter_map(|v| fuzzy_match_with(v, candidate, opts))
        .max_by_key(|m| m.score)
        .map(|m| FuzzyMatch {
            score: m.score - PENALTY_TYPO,
            positions: m.positions,
        })
}

/// The substring match behind [`MatchOptions::exact`]: of all occurrences the one at the best
/// word boundary wins.
fn exact_match(query: &str, candidate: &str) -> Option<FuzzyMatch> {
    let lower = |ch: char| ch.to_lowercase().next().unwrap_or(ch);
    let q: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(lower)
        .collect();
    if q.is_empty() {
        return Some(FuzzyMatch {
            score: 0,
            positions: vec![],
        });
    }
    let c: Vec<char> = candidate.chars().collect();
    let folded: Vec<char> = c.iter().copied().map(lower).collect();
    let start = (0..=folded.len().checked_sub(q.len())?)
        .filter(|&j| folded[j..j + q.len()] == q[..])
        .max_by_key(|&j| {
            (
                position_bonus(j.checked_sub(1).map(|p| c[p]), c[j]),
                -(j as i32),
            )
        })?;
    Some(FuzzyMatch {
        score: q.len() as i32 * (2 + BONUS_CONSECUTIVE)
            + position_bonus(start.checked_sub(1).map(|p| c[p]), c[start]),
        positions: (start..start + q.len()).collect(),
    })
}

/// One "Go to File" hit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    match_file_with(query, path, MatchOptions::default())
}

/// [`match_file`] with options.
pub fn match_file_with(query: &str, path: &str, opts: MatchOptions) -> Option<FileMatch> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let matched_path = query.contains('/');
    let target = if matched_path { path } else { name };
    let m = fuzzy_match_with(query, target, opts)?;
    // Prefer short names and shallow paths on ties.
    let tiebreak = (target.chars().count() as i32) / 8 + path.matches('/').count() as i32 / 3;
    Some(FileMatch {
        path: path.to_string(),
        name: name.to_string(),
        score: m.score - tiebreak + name_bonus(query, name, matched_path) - library_penalty(path),
        positions: m.positions,
        matched_path,
    })
}

const BONUS_EXACT_NAME: i32 = 40;
const BONUS_EXACT_STEM: i32 = 30;
const PENALTY_LIBRARY: i32 = 40;
const LIBRARY_DIRS: [&str; 4] = ["vendor", "node_modules", "third_party", "bower_components"];

/// A file called exactly what was typed (with or without its extension) comes first.
fn name_bonus(query: &str, name: &str, matched_path: bool) -> i32 {
    if matched_path {
        return 0;
    }
    let query: String = query.chars().filter(|c| !c.is_whitespace()).collect();
    if name.eq_ignore_ascii_case(&query) {
        return BONUS_EXACT_NAME;
    }
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() && stem.eq_ignore_ascii_case(&query) => {
            BONUS_EXACT_STEM
        }
        _ => 0,
    }
}

/// Dependencies rank below the project's own files with the same match.
fn library_penalty(path: &str) -> i32 {
    let in_library = path
        .split('/')
        .rev()
        .skip(1)
        .any(|part| LIBRARY_DIRS.contains(&part));
    if in_library {
        PENALTY_LIBRARY
    } else {
        0
    }
}

/// Rank `paths` for "Go to File": best first, ties by path; at most `limit` hits.
pub fn rank_files<'a>(
    query: &str,
    paths: impl IntoIterator<Item = &'a str>,
    limit: usize,
) -> Vec<FileMatch> {
    rank_files_with(query, paths, limit, MatchOptions::default())
}

/// [`rank_files`] with options.
pub fn rank_files_with<'a>(
    query: &str,
    paths: impl IntoIterator<Item = &'a str>,
    limit: usize,
    opts: MatchOptions,
) -> Vec<FileMatch> {
    let query = query.trim();
    if query.is_empty() || limit == 0 {
        return vec![];
    }
    let mut hits: Vec<FileMatch> = paths
        .into_iter()
        .filter_map(|p| match_file_with(query, p, opts))
        .collect();
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    hits.truncate(limit);
    hits
}

/// [`rank_files_with`]; when nothing matches and `opts.typo` is set, once more tolerating a typo.
pub fn rank_files_tolerant(
    query: &str,
    paths: &[&str],
    limit: usize,
    opts: MatchOptions,
) -> Vec<FileMatch> {
    let clean = MatchOptions {
        typo: false,
        ..opts
    };
    let hits = rank_files_with(query, paths.iter().copied(), limit, clean);
    if !hits.is_empty() || opts.exact || !opts.typo {
        return hits;
    }
    rank_files_with(query, paths.iter().copied(), limit, opts)
}

/// A candidate for [`filter`]: one text, or several alternatives (a name and its aliases)
/// of which the first that matches counts.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum FilterCandidate {
    One(String),
    Many(Vec<String>),
}

impl FilterCandidate {
    fn texts(&self) -> &[String] {
        match self {
            Self::One(s) => std::slice::from_ref(s),
            Self::Many(v) => v,
        }
    }
}

/// Request of the `fuzzy_filter` IPC command (and its WebAssembly twin).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterRequest {
    pub query: String,
    pub candidates: Vec<FilterCandidate>,
    #[serde(default)]
    pub strict: bool,
    #[serde(default)]
    pub exact: bool,
    /// On equal scores, prefer shorter texts (otherwise the input order is kept).
    #[serde(default)]
    pub prefer_shorter: bool,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// One surviving candidate of [`filter`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterHit {
    /// Index into the request's candidates.
    pub index: usize,
    /// Which of the candidate's texts matched (0 for single texts).
    pub text: usize,
    pub score: i32,
    pub positions: Vec<usize>,
}

/// Keep the candidates that match `query`, best first. An empty query keeps everything in
/// input order.
pub fn filter(req: &FilterRequest) -> Vec<FilterHit> {
    let opts = MatchOptions {
        strict: req.strict,
        exact: req.exact,
        typo: false,
    };
    let mut hits: Vec<(FilterHit, usize)> = req
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(index, c)| {
            c.texts().iter().enumerate().find_map(|(text, t)| {
                fuzzy_match_with(&req.query, t, opts).map(|m| {
                    let hit = FilterHit {
                        index,
                        text,
                        score: m.score,
                        positions: m.positions,
                    };
                    (hit, t.chars().count())
                })
            })
        })
        .collect();
    // `sort_by` is stable: ties keep the input order.
    hits.sort_by(|(a, la), (b, lb)| {
        let by_len = if req.prefer_shorter {
            la.cmp(lb)
        } else {
            std::cmp::Ordering::Equal
        };
        b.score.cmp(&a.score).then(by_len)
    });
    let mut out: Vec<FilterHit> = hits.into_iter().map(|(h, _)| h).collect();
    if let Some(limit) = req.limit {
        out.truncate(limit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn positions(q: &str, c: &str) -> Option<Vec<usize>> {
        fuzzy_match(q, c).map(|m| m.positions)
    }

    fn strict(q: &str, c: &str) -> Option<Vec<usize>> {
        fuzzy_match_with(q, c, MatchOptions::STRICT).map(|m| m.positions)
    }

    #[test]
    fn exact_matches_substrings_only() {
        let exact = MatchOptions {
            strict: false,
            exact: true,
            typo: false,
        };
        assert!(fuzzy_match_with("orderc", "OrderController", exact).is_some());
        assert!(fuzzy_match_with("odrc", "OrderController", exact).is_none());
        let m = fuzzy_match_with("CON", "OrderController", exact).unwrap();
        assert_eq!(m.positions, vec![5, 6, 7]);
        let word = fuzzy_match_with("user", "User.php", exact).unwrap().score;
        let inner = fuzzy_match_with("user", "AbuserLog.php", exact)
            .unwrap()
            .score;
        assert!(word > inner);
        assert!(rank_files_with("ordr", ["app/Order.php"], 5, exact).is_empty());
        assert_eq!(rank_files_with("ord", ["app/Order.php"], 5, exact).len(), 1);
    }

    #[test]
    fn camel_humps_match() {
        assert_eq!(positions("OrCo", "OrderController"), Some(vec![0, 1, 5, 6]));
        assert_eq!(positions("OC", "OrderController"), Some(vec![0, 5]));
        assert!(fuzzy_match("orderc", "OrderController").is_some());
    }

    #[test]
    fn missing_or_out_of_order_characters_fail() {
        assert!(fuzzy_match("xyz", "OrderController").is_none());
        assert!(fuzzy_match("rO", "Order").is_none());
        assert!(fuzzy_match("toolong", "short").is_none());
    }

    #[test]
    fn empty_query_matches_and_whitespace_is_ignored() {
        assert_eq!(
            fuzzy_match("", "Anything"),
            Some(FuzzyMatch {
                score: 0,
                positions: vec![]
            })
        );
        assert!(fuzzy_match("order con", "OrderController").is_some());
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
    fn separators_are_word_starts() {
        assert_eq!(
            positions("cot", "create_orders_table"),
            Some(vec![0, 7, 14])
        );
    }

    #[test]
    fn strict_mode_only_jumps_to_word_starts() {
        assert!(strict("line", "Move Line Up").is_some());
        assert!(strict("line", "Collapse Recursively").is_none());
        assert!(strict("line", "Add Selection for Next Occurrence").is_none());
        assert_eq!(strict("OrCo", "OrderController"), Some(vec![0, 1, 5, 6]));
        // A match may still start mid-word.
        assert_eq!(strict("ine", "Line"), Some(vec![1, 2, 3]));
    }

    #[test]
    fn slash_queries_match_paths() {
        let hit = match_file("mod/us", "app/Models/User.php").unwrap();
        assert!(hit.matched_path);
        assert!(match_file("mod/us", "app/Http/Kernel.php").is_none());
        let hit = match_file("OrCo", "app/Http/Controllers/OrderController.php").unwrap();
        assert!(!hit.matched_path);
        assert_eq!(hit.positions, vec![0, 1, 5, 6]);
        assert!(match_file("Http", "app/Http/Controllers/OrderController.php").is_none());
    }

    #[test]
    fn go_to_file_prefers_the_exact_name_and_the_project_over_dependencies() {
        let paths = [
            "database/seeders/UserSeeder.php",
            "vendor/laravel/framework/src/Foundation/Auth/User.php",
            "app/Http/Controllers/UserController.php",
            "app/Models/User.php",
            "tests/Feature/UserTest.php",
            "vendor/x/y/z/index.php",
            "resources/views/users/index.blade.php",
            "public/index.php",
        ];
        let order = |q: &str| -> Vec<String> {
            rank_files(q, paths, 8)
                .into_iter()
                .map(|h| h.path)
                .collect()
        };
        let user = order("user");
        assert_eq!(user[0], "app/Models/User.php");
        assert_eq!(
            user.last().unwrap(),
            "vendor/laravel/framework/src/Foundation/Auth/User.php"
        );
        assert_eq!(order("User.php")[0], "app/Models/User.php");
        assert_eq!(order("index.php")[0], "public/index.php");
        assert_eq!(order("index.php")[1], "vendor/x/y/z/index.php");
    }

    #[test]
    fn a_typo_in_the_query_still_finds_the_file_when_nothing_else_does() {
        let paths = ["app/Models/Order.php", "README.md"];
        let typo = MatchOptions {
            typo: true,
            ..MatchOptions::default()
        };
        assert!(rank_files("ordre", paths, 5).is_empty());
        let hits = rank_files_tolerant("ordre", &paths, 5, typo);
        assert_eq!(hits[0].path, "app/Models/Order.php");
        assert_eq!(
            rank_files_tolerant("ordeer", &paths, 5, typo)[0].path,
            "app/Models/Order.php"
        );
        assert!(rank_files_tolerant("xyzq", &paths, 5, typo).is_empty());
        let exact = MatchOptions {
            exact: true,
            typo: true,
            ..MatchOptions::default()
        };
        assert!(rank_files_tolerant("ordre", &paths, 5, exact).is_empty());
    }

    #[test]
    fn rank_files_sorts_and_limits() {
        let paths = ["app/Order.php", "app/Http/OrderController.php", "README.md"];
        let hits = rank_files("orco", paths, 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "OrderController.php");
        assert_eq!(rank_files("o", paths, 1).len(), 1);
        assert!(rank_files("  ", paths, 10).is_empty());
    }

    fn req(query: &str, items: &[&str]) -> FilterRequest {
        FilterRequest {
            query: query.into(),
            candidates: items
                .iter()
                .map(|s| FilterCandidate::One(s.to_string()))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn filter_orders_by_score_and_drops_non_matches() {
        let classes = [
            "OrderController",
            "Controller",
            "Order",
            "OrderStatus",
            "UserController",
        ];
        let hits = filter(&req("OrCo", &classes));
        assert_eq!(hits[0].index, 0);
        assert!(!hits.iter().any(|h| h.index == 2));

        let mut r = req("order", &classes);
        r.prefer_shorter = true;
        assert_eq!(filter(&r)[0].index, 2);

        let mut r = req("o", &classes);
        r.limit = Some(2);
        assert_eq!(filter(&r).len(), 2);
    }

    #[test]
    fn filter_keeps_input_order_for_empty_queries_and_matches_aliases() {
        let hits = filter(&req("", &["b", "a", "c"]));
        assert_eq!(
            hits.iter().map(|h| h.index).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        let r: FilterRequest = serde_json::from_str(
            r#"{"query":"mm","candidates":[["cache:clear","c:c"],["make:migration","mm"]]}"#,
        )
        .unwrap();
        let hits = filter(&r);
        assert_eq!(hits[0].index, 1);
        assert_eq!(hits[0].text, 0);
    }
}
