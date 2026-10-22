//! A minimal reader for Hunspell dictionaries (`.aff` + `.dic`): it expands every stem with
//! its prefix and suffix rules (`PFX` / `SFX`, cross products, conditions, `FLAG` types,
//! `AF` aliases) into the list of accepted word forms. Compounding rules are ignored (they
//! only matter for ordinals like `21st` in English); `REP` pairs are kept for suggestions.

use std::collections::HashMap;

/// Flags as numbers: one character, a pair of characters (`FLAG long`) or a decimal
/// number (`FLAG num`).
type Flag = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlagMode {
    Char,
    Long,
    Num,
}

impl FlagMode {
    fn parse(self, s: &str) -> Vec<Flag> {
        match self {
            FlagMode::Char => s.chars().map(|c| c as u32).collect(),
            FlagMode::Long => {
                let chars: Vec<char> = s.chars().collect();
                chars
                    .chunks(2)
                    .map(|p| ((p[0] as u32) << 16) | p.get(1).map(|c| *c as u32).unwrap_or(0))
                    .collect()
            }
            FlagMode::Num => s
                .split(',')
                .filter_map(|n| n.trim().parse::<u32>().ok())
                .collect(),
        }
    }
}

/// One element of an affix condition: `.`, a character or a `[...]` / `[^...]` class.
#[derive(Debug, Clone)]
enum CondPart {
    Any,
    Char(char),
    Class { negated: bool, chars: Vec<char> },
}

impl CondPart {
    fn matches(&self, c: char) -> bool {
        match self {
            CondPart::Any => true,
            CondPart::Char(x) => *x == c,
            CondPart::Class { negated, chars } => chars.contains(&c) != *negated,
        }
    }
}

fn parse_condition(s: &str) -> Vec<CondPart> {
    if s == "." {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '.' => out.push(CondPart::Any),
            '[' => {
                let negated = chars.peek() == Some(&'^');
                if negated {
                    chars.next();
                }
                let mut set = Vec::new();
                for c in chars.by_ref() {
                    if c == ']' {
                        break;
                    }
                    set.push(c);
                }
                out.push(CondPart::Class {
                    negated,
                    chars: set,
                });
            }
            c => out.push(CondPart::Char(c)),
        }
    }
    out
}

#[derive(Debug, Clone)]
struct Affix {
    strip: Vec<char>,
    append: String,
    /// Flags of the appended part (twofold affixes): more suffixes may follow.
    cont: Vec<Flag>,
    condition: Vec<CondPart>,
}

#[derive(Debug, Clone)]
struct AffixGroup {
    cross: bool,
    rules: Vec<Affix>,
}

/// The parsed `.aff` file.
#[derive(Debug, Default)]
pub struct AffixFile {
    mode: Option<FlagMode>,
    prefixes: HashMap<Flag, AffixGroup>,
    suffixes: HashMap<Flag, AffixGroup>,
    aliases: Vec<Vec<Flag>>,
    need_affix: Option<Flag>,
    only_in_compound: Option<Flag>,
    forbidden: Option<Flag>,
    no_suggest: Option<Flag>,
    /// `REP` pairs: common misspellings (`_` stands for a space).
    pub replacements: Vec<(String, String)>,
    /// `SET`: the encoding of both files.
    pub encoding: String,
}

impl AffixFile {
    fn mode(&self) -> FlagMode {
        self.mode.unwrap_or(FlagMode::Char)
    }

    fn flag(&self, s: &str) -> Option<Flag> {
        self.mode().parse(s).into_iter().next()
    }

    /// The flags of a dictionary entry (`AF` aliases are 1-based numbers).
    fn flags(&self, s: &str) -> Vec<Flag> {
        if !self.aliases.is_empty() {
            if let Ok(i) = s.parse::<usize>() {
                return self
                    .aliases
                    .get(i.wrapping_sub(1))
                    .cloned()
                    .unwrap_or_default();
            }
        }
        self.mode().parse(s)
    }

    pub fn parse(text: &str) -> AffixFile {
        let mut aff = AffixFile {
            encoding: "UTF-8".into(),
            ..AffixFile::default()
        };
        // First pass: FLAG must be known before affix flags are read.
        for line in text.lines() {
            let mut parts = line.split_whitespace();
            match (parts.next(), parts.next()) {
                (Some("FLAG"), Some("long")) => aff.mode = Some(FlagMode::Long),
                (Some("FLAG"), Some("num")) => aff.mode = Some(FlagMode::Num),
                (Some("SET"), Some(enc)) => aff.encoding = enc.to_string(),
                _ => {}
            }
        }
        for line in text.lines() {
            let line = line.trim_end();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            match parts[0] {
                "AF" if parts.len() >= 2 && parts[1].parse::<usize>().is_err() => {
                    aff.aliases.push(aff.mode().parse(parts[1]));
                }
                "NEEDAFFIX" | "PSEUDOROOT" if parts.len() >= 2 => {
                    aff.need_affix = aff.flag(parts[1])
                }
                "ONLYINCOMPOUND" if parts.len() >= 2 => aff.only_in_compound = aff.flag(parts[1]),
                "FORBIDDENWORD" if parts.len() >= 2 => aff.forbidden = aff.flag(parts[1]),
                "NOSUGGEST" if parts.len() >= 2 => aff.no_suggest = aff.flag(parts[1]),
                "REP" if parts.len() >= 3 => aff
                    .replacements
                    .push((parts[1].to_string(), parts[2].to_string())),
                "PFX" | "SFX" if parts.len() >= 4 => {
                    let Some(flag) = aff.flag(parts[1]) else {
                        continue;
                    };
                    let map = if parts[0] == "PFX" {
                        &mut aff.prefixes
                    } else {
                        &mut aff.suffixes
                    };
                    // Header: `SFX D Y 4`; rules: `SFX D y ied [^aeiou]y`.
                    if parts.len() == 4
                        && parts[3].parse::<usize>().is_ok()
                        && !map.contains_key(&flag)
                    {
                        map.insert(
                            flag,
                            AffixGroup {
                                cross: parts[2] == "Y",
                                rules: Vec::new(),
                            },
                        );
                        continue;
                    }
                    let strip = if parts[2] == "0" { "" } else { parts[2] };
                    let (append, cont) = match parts[3].split_once('/') {
                        Some((a, f)) => (a, f),
                        None => (parts[3], ""),
                    };
                    let append = if append == "0" { "" } else { append };
                    let condition = parse_condition(parts.get(4).copied().unwrap_or("."));
                    let cont = if cont.is_empty() {
                        Vec::new()
                    } else {
                        let mode = aff.mode.unwrap_or(FlagMode::Char);
                        if !aff.aliases.is_empty() {
                            cont.parse::<usize>()
                                .ok()
                                .and_then(|i| aff.aliases.get(i.wrapping_sub(1)).cloned())
                                .unwrap_or_default()
                        } else {
                            mode.parse(cont)
                        }
                    };
                    let map = if parts[0] == "PFX" {
                        &mut aff.prefixes
                    } else {
                        &mut aff.suffixes
                    };
                    map.entry(flag)
                        .or_insert(AffixGroup {
                            cross: true,
                            rules: Vec::new(),
                        })
                        .rules
                        .push(Affix {
                            strip: strip.chars().collect(),
                            append: append.to_string(),
                            cont,
                            condition,
                        });
                }
                _ => {}
            }
        }
        aff
    }
}

fn apply_suffix(word: &[char], rule: &Affix) -> Option<String> {
    if word.len() < rule.strip.len() || !word.ends_with(&rule.strip) {
        return None;
    }
    let cond = &rule.condition;
    if cond.len() > word.len() {
        return None;
    }
    let tail = &word[word.len() - cond.len()..];
    if !cond.iter().zip(tail).all(|(p, c)| p.matches(*c)) {
        return None;
    }
    let stem: String = word[..word.len() - rule.strip.len()].iter().collect();
    Some(stem + &rule.append)
}

fn apply_prefix(word: &[char], rule: &Affix) -> Option<String> {
    if word.len() < rule.strip.len() || !word.starts_with(&rule.strip) {
        return None;
    }
    let cond = &rule.condition;
    if cond.len() > word.len() {
        return None;
    }
    if !cond.iter().zip(word).all(|(p, c)| p.matches(*c)) {
        return None;
    }
    let rest: String = word[rule.strip.len()..].iter().collect();
    Some(rule.append.clone() + &rest)
}

/// Decodes a dictionary file in the encoding its `.aff` names (UTF-8 or a Latin-1 style
/// single-byte encoding).
pub fn decode(bytes: &[u8], encoding: &str) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    if encoding.eq_ignore_ascii_case("UTF-8") || encoding.eq_ignore_ascii_case("UTF8") {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        bytes.iter().map(|b| *b as char).collect()
    }
}

/// The encoding an `.aff` file declares (`SET`), read from its raw bytes.
pub fn declared_encoding(aff: &[u8]) -> String {
    String::from_utf8_lossy(aff)
        .lines()
        .find_map(|l| l.strip_prefix("SET ").map(|e| e.trim().to_string()))
        .unwrap_or_else(|| "UTF-8".into())
}

/// Every word form of a dictionary: `(form, suggestable)`.
pub fn expand(aff: &AffixFile, dic: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut lines = dic.lines();
    let first = lines.next().unwrap_or_default();
    // The first line is the entry count; plain word lists may start with a word.
    let rest: Box<dyn Iterator<Item = &str>> = if first.trim().parse::<usize>().is_ok() {
        Box::new(lines)
    } else {
        Box::new(std::iter::once(first).chain(lines))
    };
    for line in rest {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') || line.starts_with('\t') {
            continue;
        }
        // `word/flags` followed by optional morphological fields.
        let entry = line.split(['\t', ' ']).next().unwrap_or_default();
        let (word, flags) = split_entry(entry);
        if word.is_empty() {
            continue;
        }
        let flags = flags.map(|f| aff.flags(f)).unwrap_or_default();
        let has = |f: Option<Flag>| f.is_some_and(|f| flags.contains(&f));
        if has(aff.forbidden) || has(aff.only_in_compound) {
            continue;
        }
        let suggest = !has(aff.no_suggest);
        expand_entry(aff, &word, &flags, !has(aff.need_affix), suggest, &mut out);
    }
    out
}

fn split_entry(entry: &str) -> (String, Option<&str>) {
    // `\/` escapes a slash inside the word.
    let bytes = entry.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'/' => {
                return (entry[..i].replace("\\/", "/"), Some(&entry[i + 1..]));
            }
            _ => i += 1,
        }
    }
    (entry.replace("\\/", "/"), None)
}

fn expand_entry(
    aff: &AffixFile,
    word: &str,
    flags: &[Flag],
    root_ok: bool,
    suggest: bool,
    out: &mut Vec<(String, bool)>,
) {
    if root_ok {
        out.push((word.to_string(), suggest));
    }
    let chars: Vec<char> = word.chars().collect();
    let mut suffixed: Vec<(String, bool)> = Vec::new();
    for flag in flags {
        let Some(group) = aff.suffixes.get(flag) else {
            continue;
        };
        for rule in &group.rules {
            if let Some(form) = apply_suffix(&chars, rule) {
                // Twofold suffixes: a second suffix on top of the first one.
                for f2 in &rule.cont {
                    if let Some(g2) = aff.suffixes.get(f2) {
                        let fc: Vec<char> = form.chars().collect();
                        for r2 in &g2.rules {
                            if let Some(f) = apply_suffix(&fc, r2) {
                                out.push((f, suggest));
                            }
                        }
                    }
                }
                if group.cross {
                    suffixed.push((form.clone(), true));
                }
                out.push((form, suggest));
            }
        }
    }
    for flag in flags {
        let Some(group) = aff.prefixes.get(flag) else {
            continue;
        };
        for rule in &group.rules {
            if let Some(form) = apply_prefix(&chars, rule) {
                out.push((form, suggest));
            }
            if group.cross {
                for (s, _) in &suffixed {
                    let sc: Vec<char> = s.chars().collect();
                    if let Some(form) = apply_prefix(&sc, rule) {
                        out.push((form, suggest));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AFF: &str = "SET UTF-8\nNOSUGGEST !\n\nPFX A Y 1\nPFX A 0 re .\n\nSFX D Y 4\nSFX D 0 d e\nSFX D y ied [^aeiou]y\nSFX D 0 ed [^ey]\nSFX D 0 ed [aeiou]y\n\nSFX S Y 1\nSFX S 0 s .\n\nREP f ph\n";

    fn forms(dic: &str) -> Vec<String> {
        let aff = AffixFile::parse(AFF);
        let mut f: Vec<String> = expand(&aff, dic).into_iter().map(|(w, _)| w).collect();
        f.sort();
        f
    }

    #[test]
    fn expands_suffixes_prefixes_and_cross_products() {
        assert_eq!(
            forms("3\ncreate/AD\ncry/D\nplay/DS\n"),
            [
                "create",
                "created",
                "cried",
                "cry",
                "play",
                "played",
                "plays",
                "recreate",
                "recreated"
            ]
        );
    }

    #[test]
    fn reads_flag_types_aliases_and_special_flags() {
        let aff = AffixFile::parse("FLAG long\nNEEDAFFIX zz\nSFX Ss Y 1\nSFX Ss 0 s .\n");
        let w: Vec<String> = expand(&aff, "2\nbook/Ss\nonly/Sszz\n")
            .into_iter()
            .map(|(w, _)| w)
            .collect();
        assert_eq!(w, ["book", "books", "onlys"]);

        let aff = AffixFile::parse("FLAG num\nSFX 7 Y 1\nSFX 7 0 s .\n");
        let w: Vec<String> = expand(&aff, "1\ncar/7,9\n")
            .into_iter()
            .map(|(w, _)| w)
            .collect();
        assert_eq!(w, ["car", "cars"]);

        let aff = AffixFile::parse("AF 1\nAF S\nSFX S Y 1\nSFX S 0 s .\n");
        let w: Vec<String> = expand(&aff, "1\ncat/1\n")
            .into_iter()
            .map(|(w, _)| w)
            .collect();
        assert_eq!(w, ["cat", "cats"]);

        let aff = AffixFile::parse(AFF);
        assert_eq!(expand(&aff, "1\ndarn/!\n"), [("darn".to_string(), false)]);
        assert_eq!(aff.replacements, [("f".to_string(), "ph".to_string())]);
    }

    #[test]
    fn plain_word_lists_and_latin1() {
        let aff = AffixFile::parse("");
        let w: Vec<String> = expand(&aff, "Forge\nartisan\n")
            .into_iter()
            .map(|(w, _)| w)
            .collect();
        assert_eq!(w, ["Forge", "artisan"]);
        assert_eq!(decode(b"caf\xe9", "ISO8859-1"), "café");
        assert_eq!(declared_encoding(b"SET ISO8859-1\n"), "ISO8859-1");
    }
}
