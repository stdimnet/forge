//! Suggestions for a misspelled word: dictionary words within a small weighted edit
//! distance (Damerau-Levenshtein with adjacent transpositions), where typing slips are
//! cheaper than other edits: neighbouring keys on a QWERTY keyboard, swapped letters,
//! doubled or missing double letters and vowel confusions. `REP` pairs of the Hunspell
//! dictionary add phonetic candidates ("f" for "ph").

/// Position of a key on a QWERTY keyboard: (row, column with the row's stagger).
fn key_position(c: char) -> Option<(f32, f32)> {
    const ROWS: [(&str, f32); 3] = [("qwertyuiop", 0.0), ("asdfghjkl", 0.25), ("zxcvbnm", 0.75)];
    let c = c.to_ascii_lowercase();
    ROWS.iter()
        .enumerate()
        .find_map(|(r, (keys, offset))| keys.find(c).map(|col| (r as f32, col as f32 + offset)))
}

/// Whether two letters are neighbouring keys.
pub fn adjacent_keys(a: char, b: char) -> bool {
    match (key_position(a), key_position(b)) {
        (Some((ra, ca)), Some((rb, cb))) => {
            a != b && (ra - rb).abs() <= 1.0 && (ca - cb).abs() <= 1.0
        }
        _ => false,
    }
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y')
}

fn substitution_cost(a: char, b: char) -> f32 {
    if a == b {
        0.0
    } else if adjacent_keys(a, b) {
        0.6
    } else if is_vowel(a) && is_vowel(b) {
        0.8
    } else {
        1.0
    }
}

/// Weighted edit distance from `a` (the typed word) to `b`, or `None` when it exceeds `max`.
pub fn distance(a: &[char], b: &[char], max: f32) -> Option<f32> {
    let (n, m) = (a.len(), b.len());
    if (n as f32 - m as f32).abs() > max {
        return None;
    }
    // Insertion or deletion of a letter next to the same letter ("adress", "untill").
    let indel = |s: &[char], i: usize| -> f32 {
        let c = s[i];
        if (i > 0 && s[i - 1] == c) || s.get(i + 1) == Some(&c) {
            0.5
        } else {
            1.0
        }
    };
    let mut prev2 = vec![0.0f32; m + 1];
    let mut prev = vec![0.0f32; m + 1];
    let mut cur = vec![0.0f32; m + 1];
    for j in 1..=m {
        prev[j] = prev[j - 1] + indel(b, j - 1);
    }
    for i in 1..=n {
        cur[0] = prev[0] + indel(a, i - 1);
        let mut row_min = cur[0];
        for j in 1..=m {
            let mut d = (prev[j - 1] + substitution_cost(a[i - 1], b[j - 1]))
                .min(prev[j] + indel(a, i - 1))
                .min(cur[j - 1] + indel(b, j - 1));
            if i > 1
                && j > 1
                && a[i - 1] == b[j - 2]
                && a[i - 2] == b[j - 1]
                && a[i - 1] != a[i - 2]
            {
                d = d.min(prev2[j - 2] + 0.5);
            }
            cur[j] = d;
            row_min = row_min.min(d);
        }
        if row_min > max {
            return None;
        }
        std::mem::swap(&mut prev2, &mut prev);
        std::mem::swap(&mut prev, &mut cur);
    }
    (prev[m] <= max).then_some(prev[m])
}

/// The largest distance worth suggesting for a word of `len` letters.
pub fn max_distance(len: usize) -> f32 {
    if len <= 4 {
        1.0
    } else {
        2.0
    }
}

/// A scored candidate.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub word: String,
    pub cost: f32,
}

/// Sorts candidates (cheapest first, then same first letter, then closer length) and keeps
/// the first `limit` distinct words.
pub fn rank(word: &str, mut candidates: Vec<Candidate>, limit: usize) -> Vec<String> {
    let first = word.chars().next();
    let len = word.chars().count() as i64;
    let key = |c: &Candidate| {
        (
            (c.cost * 100.0).round() as i64,
            (c.word.chars().next() != first) as i64,
            (c.word.chars().count() as i64 - len).abs(),
        )
    };
    candidates.sort_by(|a, b| key(a).cmp(&key(b)).then_with(|| a.word.cmp(&b.word)));
    let mut out: Vec<String> = Vec::new();
    for c in candidates {
        if !out.contains(&c.word) {
            out.push(c.word);
        }
        if out.len() == limit {
            break;
        }
    }
    out
}

/// Applies the case of `original` to a lowercase suggestion: `Recieve` → `Receive`,
/// `RECIEVE` → `RECEIVE`.
pub fn match_case(original: &str, suggestion: &str) -> String {
    let letters: Vec<char> = original.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
        return suggestion.to_uppercase();
    }
    if original.chars().next().is_some_and(char::is_uppercase) {
        let mut chars = suggestion.chars();
        return match chars.next() {
            Some(f) => f.to_uppercase().chain(chars).collect(),
            None => String::new(),
        };
    }
    suggestion.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(a: &str, b: &str) -> Option<f32> {
        let a: Vec<char> = a.chars().collect();
        let b: Vec<char> = b.chars().collect();
        distance(&a, &b, 2.0)
    }

    #[test]
    fn typing_slips_are_cheap() {
        assert_eq!(d("receive", "receive"), Some(0.0));
        assert_eq!(d("recieve", "receive"), Some(0.5)); // swapped letters
        assert_eq!(d("adress", "address"), Some(0.5)); // missing double letter
        assert_eq!(d("untill", "until"), Some(0.5));
        assert_eq!(d("tesr", "test"), Some(0.6)); // r is next to t
        assert_eq!(d("seperate", "separate"), Some(0.8)); // vowels
        assert_eq!(d("cat", "xyzzy"), None);
    }

    #[test]
    fn keyboard_neighbours() {
        assert!(adjacent_keys('a', 's') && adjacent_keys('g', 'b') && adjacent_keys('q', 'a'));
        assert!(!adjacent_keys('a', 'p') && !adjacent_keys('a', 'a'));
    }

    #[test]
    fn ranking_and_case() {
        let c = |w: &str, cost: f32| Candidate {
            word: w.into(),
            cost,
        };
        assert_eq!(
            rank(
                "tset",
                vec![
                    c("set", 1.0),
                    c("test", 0.6),
                    c("bset", 1.0),
                    c("test", 1.0)
                ],
                3
            ),
            ["test", "bset", "set"]
        );
        assert_eq!(match_case("Recieve", "receive"), "Receive");
        assert_eq!(match_case("RECIEVE", "receive"), "RECEIVE");
        assert_eq!(match_case("recieve", "receive"), "receive");
    }
}
