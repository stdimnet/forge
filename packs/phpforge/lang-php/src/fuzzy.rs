//! Fuzzy matcher for Go to Class / Go to Symbol: subsequence matching with bonuses for
//! word starts, camel humps (`OrCo` -> `OrderController`) and consecutive runs.

/// A successful match: higher `score` is better, `positions` are matched char indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyMatch {
    pub score: i32,
    pub positions: Vec<usize>,
}

fn is_boundary(prev: Option<char>, cur: char) -> bool {
    match prev {
        None => true,
        Some(p) => {
            matches!(p, '_' | '\\' | ':' | '-' | ' ' | '$' | '.')
                || (p.is_lowercase() && cur.is_uppercase())
                || (p.is_alphabetic() && cur.is_ascii_digit())
        }
    }
}

/// Match `query` against `candidate` (case-insensitive). Empty queries match everything.
pub fn fuzzy_match(query: &str, candidate: &str) -> Option<FuzzyMatch> {
    let q: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if q.is_empty() {
        return Some(FuzzyMatch {
            score: 0,
            positions: Vec::new(),
        });
    }
    let c: Vec<char> = candidate.chars().collect();
    let cl: Vec<char> = c
        .iter()
        .map(|ch| ch.to_lowercase().next().unwrap_or(*ch))
        .collect();
    let (m, n) = (q.len(), c.len());
    if m > n {
        return None;
    }
    const NEG: i32 = i32::MIN / 4;
    // dp[i][j]: best score with q[i] matched at c[j].
    let mut dp = vec![vec![NEG; n]; m];
    let mut back = vec![vec![usize::MAX; n]; m];
    let bonus = |j: usize| -> i32 {
        if is_boundary(if j == 0 { None } else { Some(c[j - 1]) }, c[j]) {
            if j == 0 {
                12
            } else {
                9
            }
        } else {
            0
        }
    };
    for j in 0..n {
        if cl[j] == q[0] {
            dp[0][j] = 10 + bonus(j) - (j as i32).min(10);
        }
    }
    for i in 1..m {
        // Best predecessor with a gap (k <= j - 2), penalised by the gap length.
        let mut best_prev = NEG;
        let mut best_prev_idx = usize::MAX;
        for j in 1..n {
            let k = j - 1;
            if j >= 2 {
                if best_prev > NEG {
                    best_prev -= 1;
                }
                let cand = dp[i - 1][j - 2];
                if cand > NEG && cand - 1 > best_prev {
                    best_prev = cand - 1;
                    best_prev_idx = j - 2;
                }
            }
            if cl[j] != q[i] {
                continue;
            }
            let consecutive = dp[i - 1][k];
            let mut score = NEG;
            let mut from = usize::MAX;
            if consecutive > NEG {
                score = consecutive + 10 + 6 + bonus(j);
                from = k;
            }
            if best_prev > NEG {
                let s = best_prev + 10 + bonus(j) * 2;
                if s > score {
                    score = s;
                    from = best_prev_idx;
                }
            }
            if from != usize::MAX {
                dp[i][j] = score;
                back[i][j] = from;
            }
        }
    }
    let (mut j, best) = (0..n).map(|j| (j, dp[m - 1][j])).max_by_key(|(_, s)| *s)?;
    if best <= NEG {
        return None;
    }
    let mut positions = vec![0; m];
    for i in (0..m).rev() {
        positions[i] = j;
        if i > 0 {
            j = back[i][j];
        }
    }
    let mut score = best - (n as i32 - m as i32) / 4;
    if cl.iter().collect::<String>() == q.iter().collect::<String>() {
        score += 100;
    } else if positions.iter().enumerate().all(|(i, p)| i == *p) {
        score += 40;
    }
    Some(FuzzyMatch { score, positions })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_subsequences() {
        assert!(fuzzy_match("chk", "Checkout").is_some());
        assert!(fuzzy_match("xyz", "Checkout").is_none());
        assert!(fuzzy_match("", "Anything").is_some());
        assert!(fuzzy_match("toolong", "tool").is_none());
    }

    #[test]
    fn camel_humps_beat_scattered_matches() {
        let hump = fuzzy_match("OC", "OrderController").unwrap();
        let scattered = fuzzy_match("OC", "Coordinator")
            .map(|m| m.score)
            .unwrap_or(i32::MIN);
        assert!(hump.score > scattered);
        assert_eq!(hump.positions, vec![0, 5]);
    }

    #[test]
    fn exact_and_prefix_rank_first() {
        let exact = fuzzy_match("order", "Order").unwrap().score;
        let prefix = fuzzy_match("order", "OrderStatus").unwrap().score;
        let inner = fuzzy_match("order", "PendingOrders").unwrap().score;
        assert!(exact > prefix, "{exact} > {prefix}");
        assert!(prefix > inner, "{prefix} > {inner}");
    }

    #[test]
    fn namespace_segments_are_boundaries() {
        let m = fuzzy_match("amu", "App\\Models\\User").unwrap();
        assert_eq!(m.positions, vec![0, 4, 11]);
    }
}
