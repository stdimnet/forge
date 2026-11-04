//! CLDR plural rules (cardinal and ordinal) for the languages Forge may be translated into.
//! Languages without rules here use the English ones for cardinals and `other` for ordinals.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralCategory {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "zero" => Self::Zero,
            "one" => Self::One,
            "two" => Self::Two,
            "few" => Self::Few,
            "many" => Self::Many,
            "other" => Self::Other,
            _ => return None,
        })
    }
}

/// CLDR plural operands of a number: `i` integer digits, `v` number of visible fraction
/// digits.
#[derive(Debug, Clone, Copy)]
struct Operands {
    n: f64,
    i: u64,
    v: usize,
}

fn operands(n: f64) -> Operands {
    let n = n.abs();
    // Up to 6 fraction digits, as a formatted number would show them.
    let s = format!("{n:.6}");
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let frac = frac.trim_end_matches('0');
    Operands {
        n,
        i: int.parse().unwrap_or(0),
        v: frac.len(),
    }
}

/// The base language of a locale id ("de-AT" → "de").
pub fn language(locale: &str) -> &str {
    locale.split(['-', '_']).next().unwrap_or(locale)
}

/// Cardinal category ("1 file", "2 files") of `n` in `locale`.
pub fn cardinal(locale: &str, n: f64) -> PluralCategory {
    use PluralCategory::*;
    let o = operands(n);
    let (i, v) = (o.i, o.v);
    match language(locale) {
        // No plural forms.
        "ja" | "zh" | "ko" | "vi" | "th" | "id" | "ms" => Other,
        "fr" => {
            if i == 0 || i == 1 {
                One
            } else if v == 0 && i != 0 && i % 1_000_000 == 0 {
                Many
            } else {
                Other
            }
        }
        "pt" => {
            let one = if locale.contains("PT") {
                i == 1 && v == 0
            } else {
                i <= 1
            };
            if one {
                One
            } else {
                Other
            }
        }
        "es" | "it" => {
            if o.n == 1.0 && v == 0 {
                One
            } else if v == 0 && i != 0 && i % 1_000_000 == 0 {
                Many
            } else {
                Other
            }
        }
        "ru" | "uk" | "be" => {
            if v != 0 {
                Other
            } else if i % 10 == 1 && i % 100 != 11 {
                One
            } else if (2..=4).contains(&(i % 10)) && !(12..=14).contains(&(i % 100)) {
                Few
            } else {
                Many
            }
        }
        "pl" => {
            if v != 0 {
                Other
            } else if i == 1 {
                One
            } else if (2..=4).contains(&(i % 10)) && !(12..=14).contains(&(i % 100)) {
                Few
            } else {
                Many
            }
        }
        "cs" | "sk" => {
            if v != 0 {
                Many
            } else if i == 1 {
                One
            } else if (2..=4).contains(&i) {
                Few
            } else {
                Other
            }
        }
        "tr" | "hu" | "el" => {
            if o.n == 1.0 {
                One
            } else {
                Other
            }
        }
        // English, German, Dutch, Swedish, Danish, Norwegian, Finnish, ... and the default.
        _ => {
            if i == 1 && v == 0 {
                One
            } else {
                Other
            }
        }
    }
}

/// Ordinal category ("1st", "2nd") of `n` in `locale`.
pub fn ordinal(locale: &str, n: f64) -> PluralCategory {
    use PluralCategory::*;
    let o = operands(n);
    let i = o.i;
    match language(locale) {
        "en" => {
            if o.v != 0 {
                Other
            } else if i % 10 == 1 && i % 100 != 11 {
                One
            } else if i % 10 == 2 && i % 100 != 12 {
                Two
            } else if i % 10 == 3 && i % 100 != 13 {
                Few
            } else {
                Other
            }
        }
        "fr" => {
            if i == 1 && o.v == 0 {
                One
            } else {
                Other
            }
        }
        "it" => {
            if o.v == 0 && matches!(i, 8 | 11 | 80 | 800) {
                Many
            } else {
                Other
            }
        }
        _ => Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PluralCategory::*;

    #[test]
    fn english_and_german_cardinals() {
        for loc in ["en", "en-GB", "de", "de-AT"] {
            assert_eq!(cardinal(loc, 0.0), Other);
            assert_eq!(cardinal(loc, 1.0), One);
            assert_eq!(cardinal(loc, 2.0), Other);
            assert_eq!(cardinal(loc, 1.5), Other);
        }
    }

    #[test]
    fn slavic_and_french() {
        assert_eq!(cardinal("ru", 1.0), One);
        assert_eq!(cardinal("ru", 3.0), Few);
        assert_eq!(cardinal("ru", 5.0), Many);
        assert_eq!(cardinal("ru", 11.0), Many);
        assert_eq!(cardinal("ru", 21.0), One);
        assert_eq!(cardinal("pl", 22.0), Few);
        assert_eq!(cardinal("pl", 21.0), Many);
        assert_eq!(cardinal("cs", 3.0), Few);
        assert_eq!(cardinal("cs", 1.5), Many);
        assert_eq!(cardinal("fr", 0.0), One);
        assert_eq!(cardinal("fr", 1.5), One);
        assert_eq!(cardinal("ja", 1.0), Other);
    }

    #[test]
    fn ordinals() {
        assert_eq!(ordinal("en", 1.0), One);
        assert_eq!(ordinal("en", 2.0), Two);
        assert_eq!(ordinal("en", 3.0), Few);
        assert_eq!(ordinal("en", 11.0), Other);
        assert_eq!(ordinal("en", 101.0), One);
        assert_eq!(ordinal("de", 1.0), Other);
    }

    #[test]
    fn parse_categories() {
        assert_eq!(PluralCategory::parse("few"), Some(Few));
        assert_eq!(PluralCategory::parse("bogus"), None);
    }
}
