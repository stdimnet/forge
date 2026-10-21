//! Cyclic Expand Word (`Alt+/`, PHPStorm's `HippieCompletion`; `Alt+Shift+/` for
//! `HippieBackwardCompletion`): the words that complete the prefix before the caret, nearest
//! first. `Alt+/` looks above the caret first, then below, then in the other open files;
//! the backward variant looks below first. The UI replaces the prefix with the first word and
//! cycles through the rest on repeated presses.

use serde::{Deserialize, Serialize};

use crate::lang::Lang;
use crate::text::{char_index, u16_len_chars};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpandRequest {
    /// The current file.
    pub text: String,
    /// Caret (UTF-16 offset).
    pub offset: usize,
    /// Texts of the other open files, most recently used first.
    #[serde(default)]
    pub others: Vec<String>,
    /// `HippieBackwardCompletion`: look below the caret first.
    #[serde(default)]
    pub backward: bool,
    /// Forge file type id; decides whether `$` belongs to words.
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExpandResult {
    /// Start of the prefix (UTF-16 offset); the prefix ends at the caret.
    pub from: usize,
    pub prefix: String,
    /// Words to cycle through, nearest first, without duplicates.
    pub candidates: Vec<String>,
}

/// The words that complete the prefix at the caret.
pub fn expand_word(request: &ExpandRequest) -> ExpandResult {
    let lua = request.language.as_deref().and_then(Lang::from_file_type) == Some(Lang::Lua);
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || (c == '$' && !lua);
    let chars: Vec<char> = request.text.chars().collect();
    let caret = char_index(&chars, request.offset);
    let mut start = caret;
    while start > 0 && is_word(chars[start - 1]) {
        start -= 1;
    }
    let prefix: String = chars[start..caret].iter().collect();
    let from = u16_len_chars(&chars[..start]);
    if prefix.is_empty() {
        return ExpandResult {
            from,
            prefix,
            candidates: Vec::new(),
        };
    }
    // The word being typed is not a candidate.
    let mut end = caret;
    while end < chars.len() && is_word(chars[end]) {
        end += 1;
    }
    let words = words_of(&chars, &is_word);
    let above = words.iter().rev().filter(|(s, _)| *s < start);
    let below = words.iter().filter(|(s, _)| *s >= end);
    let near: Vec<&String> = if request.backward {
        below.chain(above).map(|(_, w)| w).collect()
    } else {
        above.chain(below).map(|(_, w)| w).collect()
    };
    let mut candidates: Vec<String> = Vec::new();
    let mut push = |w: &String| {
        if w.len() > prefix.len() && w.starts_with(&prefix) && !candidates.contains(w) {
            candidates.push(w.clone());
        }
    };
    near.into_iter().for_each(&mut push);
    for other in &request.others {
        let chars: Vec<char> = other.chars().collect();
        words_of(&chars, &is_word).iter().for_each(|(_, w)| push(w));
    }
    ExpandResult {
        from,
        prefix,
        candidates,
    }
}

/// Words of `chars` with their start index, in text order.
fn words_of(chars: &[char], is_word: &impl Fn(char) -> bool) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if is_word(chars[i]) {
            let s = i;
            while i < chars.len() && is_word(chars[i]) {
                i += 1;
            }
            out.push((s, chars[s..i].iter().collect()));
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand(src: &str, others: &[&str], backward: bool, language: Option<&str>) -> ExpandResult {
        let offset = src[..src.find('|').unwrap()].encode_utf16().count();
        expand_word(&ExpandRequest {
            text: src.replacen('|', "", 1),
            offset,
            others: others.iter().map(|s| s.to_string()).collect(),
            backward,
            language: language.map(Into::into),
        })
    }

    #[test]
    fn nearest_words_above_first() {
        let r = expand("userName userId\nus| user_list userAge", &[], false, None);
        assert_eq!(r.prefix, "us");
        assert_eq!(r.from, 16);
        assert_eq!(
            r.candidates,
            vec!["userId", "userName", "user_list", "userAge"]
        );
    }

    #[test]
    fn backward_looks_below_first() {
        let r = expand("userName userId\nus| user_list userAge", &[], true, None);
        assert_eq!(
            r.candidates,
            vec!["user_list", "userAge", "userId", "userName"]
        );
    }

    #[test]
    fn other_files_come_last_without_duplicates() {
        let r = expand(
            "$order = 1;\n$or|",
            &["$orders $order $origin"],
            false,
            Some("php"),
        );
        assert_eq!(r.prefix, "$or");
        assert_eq!(r.candidates, vec!["$order", "$orders", "$origin"]);
    }

    #[test]
    fn skips_the_word_at_the_caret_and_the_prefix_itself() {
        let r = expand("count co|unter co", &[], false, None);
        assert_eq!(r.prefix, "co");
        assert_eq!(r.candidates, vec!["count"]);
    }

    #[test]
    fn lua_words_have_no_dollar_and_empty_prefix_has_no_candidates() {
        let r = expand("local value = 1\nprint($va|", &[], false, Some("lua"));
        assert_eq!(
            (r.prefix.as_str(), r.candidates.clone()),
            ("va", vec!["value".to_string()])
        );
        assert!(expand("abc |", &[], false, None).candidates.is_empty());
    }

    #[test]
    fn utf16_offsets() {
        let r = expand("größe 😀 grö|", &[], false, None);
        assert_eq!(r.from, 9);
        assert_eq!(r.candidates, vec!["größe"]);
    }
}
