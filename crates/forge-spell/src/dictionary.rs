//! Word lists the checker looks words up in: the bundled English dictionary, custom
//! Hunspell dictionaries and plain word lists. Words are kept lowercase (the checker
//! ignores case), sorted for binary search and indexed by length for suggestions.

use std::io::Read;
use std::sync::{Arc, OnceLock};

use crate::hunspell::{self, AffixFile};

const EN_US_AFF: &str = include_str!("../data/en_US.aff");
const EN_US_DIC_GZ: &[u8] = include_bytes!("../data/en_US.dic.gz");
const PROGRAMMING: &str = include_str!("../data/programming.txt");

#[derive(Debug, Default)]
pub struct Dictionary {
    /// Display name ("English (USA)", a file name).
    pub name: String,
    words: Vec<Box<str>>,
    /// Parallel to `words`: offered as a suggestion (Hunspell `NOSUGGEST` is not).
    suggest: Vec<bool>,
    /// Parallel to `words`: only listed capitalized (names, acronyms).
    proper: Vec<bool>,
    /// Indexes into `words` by length in characters.
    by_len: Vec<Vec<u32>>,
    /// `REP` pairs of the `.aff` file.
    pub replacements: Vec<(String, String)>,
}

impl Dictionary {
    /// A dictionary of the given words (`(word, suggestable)`).
    pub fn from_words(name: &str, words: impl IntoIterator<Item = (String, bool)>) -> Dictionary {
        let mut all: Vec<(Box<str>, bool, bool)> = words
            .into_iter()
            .map(|(w, s)| {
                let proper = w.chars().any(char::is_uppercase);
                (normalize(&w).into_boxed_str(), s, proper)
            })
            .filter(|(w, _, _)| !w.is_empty())
            .collect();
        all.sort();
        // Keep one entry per word; it is suggestable when any form was, and a name only
        // when no lowercase form is listed.
        let mut words: Vec<Box<str>> = Vec::with_capacity(all.len());
        let mut suggest: Vec<bool> = Vec::with_capacity(all.len());
        let mut proper: Vec<bool> = Vec::with_capacity(all.len());
        for (w, s, p) in all {
            if words.last() == Some(&w) {
                *suggest.last_mut().unwrap() |= s;
                *proper.last_mut().unwrap() &= p;
            } else {
                words.push(w);
                suggest.push(s);
                proper.push(p);
            }
        }
        let mut by_len: Vec<Vec<u32>> = Vec::new();
        for (i, w) in words.iter().enumerate() {
            let n = w.chars().count();
            if by_len.len() <= n {
                by_len.resize(n + 1, Vec::new());
            }
            by_len[n].push(i as u32);
        }
        Dictionary {
            name: name.to_string(),
            words,
            suggest,
            proper,
            by_len,
            replacements: Vec::new(),
        }
    }

    /// A Hunspell dictionary from the contents of its `.aff` and `.dic` files. Without an
    /// `.aff` file the `.dic` file is read as a plain word list (one word per line).
    pub fn from_hunspell(name: &str, aff: Option<&[u8]>, dic: &[u8]) -> Dictionary {
        let encoding = aff
            .map(hunspell::declared_encoding)
            .unwrap_or_else(|| "UTF-8".into());
        let affixes = aff
            .map(|a| AffixFile::parse(&hunspell::decode(a, &encoding)))
            .unwrap_or_default();
        let forms = hunspell::expand(&affixes, &hunspell::decode(dic, &encoding));
        let mut dict = Dictionary::from_words(name, forms);
        dict.replacements = affixes.replacements;
        dict
    }

    /// The bundled en_US dictionary (SCOWL, see `data/en_US.LICENSE`), parsed once.
    pub fn english() -> Arc<Dictionary> {
        static EN: OnceLock<Arc<Dictionary>> = OnceLock::new();
        EN.get_or_init(|| {
            let mut dic = Vec::new();
            flate2::read::GzDecoder::new(EN_US_DIC_GZ)
                .read_to_end(&mut dic)
                .expect("bundled en_US.dic.gz is valid");
            Arc::new(Dictionary::from_hunspell(
                "English (USA)",
                Some(EN_US_AFF.as_bytes()),
                &dic,
            ))
        })
        .clone()
    }

    /// Programming terms (`data/programming.txt`) accepted everywhere.
    pub fn programming() -> Arc<Dictionary> {
        static TERMS: OnceLock<Arc<Dictionary>> = OnceLock::new();
        TERMS
            .get_or_init(|| {
                Arc::new(Dictionary::from_words(
                    "Programming terms",
                    PROGRAMMING
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty() && !l.starts_with('#'))
                        .map(|l| (l.to_string(), true)),
                ))
            })
            .clone()
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Whether the (lowercase, normalized) word is listed.
    pub fn contains(&self, word: &str) -> bool {
        self.words.binary_search_by(|w| (**w).cmp(word)).is_ok()
    }

    /// Suggestable words of `len` characters, with whether they are names or acronyms.
    pub(crate) fn words_of_len(&self, len: usize) -> impl Iterator<Item = (&str, bool)> {
        self.by_len
            .get(len)
            .into_iter()
            .flatten()
            .filter(|i| self.suggest[**i as usize])
            .map(|i| (&*self.words[*i as usize], self.proper[*i as usize]))
    }
}

/// Lowercase with typographic apostrophes as `'`.
pub fn normalize(word: &str) -> String {
    word.chars()
        .map(|c| if c == '\u{2019}' { '\'' } else { c })
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_english_dictionary() {
        let en = Dictionary::english();
        assert!(en.len() > 100_000, "{} words", en.len());
        for w in [
            "receive",
            "received",
            "receiving",
            "address",
            "the",
            "doesn't",
            "london",
            "unhappiness",
        ] {
            assert!(en.contains(w), "{w}");
        }
        for w in ["recieve", "teh", "adress"] {
            assert!(!en.contains(w), "{w}");
        }
        assert!(!en.replacements.is_empty());
    }

    #[test]
    fn programming_terms() {
        let t = Dictionary::programming();
        assert!(t.contains("laravel") && t.contains("json") && t.contains("args"));
        assert!(!t.contains("# general"));
    }

    #[test]
    fn plain_lists_merge_duplicates() {
        let d = Dictionary::from_words(
            "x",
            [
                ("Forge".into(), false),
                ("forge".into(), true),
                ("it\u{2019}s".into(), true),
            ],
        );
        assert_eq!(d.len(), 2);
        assert!(d.contains("forge") && d.contains("it's"));
        assert_eq!(d.words_of_len(5).collect::<Vec<_>>(), [("forge", false)]);
        let d = Dictionary::from_words("x", [("USSR".into(), true)]);
        assert_eq!(d.words_of_len(4).collect::<Vec<_>>(), [("ussr", true)]);
    }
}
