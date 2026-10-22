//! A small rule-based English grammar and style checker for comments, doc comments, text
//! files and commit messages (PHPStorm's "Grammar" inspection; Forge's own rules):
//!
//! - `repeated-word`: "the the".
//! - `a-an`: "a" / "an" before a vowel or consonant sound ("an hour", "a user", "an SQL").
//! - `sentence-start`: a doc comment sentence that starts with a lowercase letter.
//! - `double-space`: two or more spaces between words.
//!
//! All offsets are UTF-16 code units from the start of the text.

use serde::Serialize;

use crate::words::Context;

/// A quick fix: replace `from..to` with `text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fix {
    pub title: String,
    pub from: usize,
    pub to: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarProblem {
    pub from: usize,
    pub to: usize,
    pub rule: &'static str,
    pub message: String,
    pub fixes: Vec<Fix>,
}

/// Rule ids with their names (Settings | Editor | Natural Languages | Grammar and Style).
pub const RULES: &[(&str, &str)] = &[
    ("repeated-word", "Repeated words"),
    ("a-an", "Use of 'a' and 'an'"),
    (
        "sentence-start",
        "Sentence starts with a capital letter (doc comments)",
    ),
    ("double-space", "Repeated whitespace"),
];

#[derive(Debug, Clone)]
struct Token {
    /// Character indexes.
    start: usize,
    end: usize,
    text: String,
}

fn tokens(chars: &[char]) -> Vec<Token> {
    let mut out = Vec::new();
    let mut i = 0;
    let apostrophe = |i: usize| {
        matches!(chars[i], '\'' | '\u{2019}')
            && i > 0
            && i + 1 < chars.len()
            && chars[i - 1].is_alphabetic()
            && chars[i + 1].is_alphabetic()
    };
    while i < chars.len() {
        if !chars[i].is_alphabetic() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_' || apostrophe(i)) {
            i += 1;
        }
        out.push(Token {
            start,
            end: i,
            text: chars[start..i].iter().collect(),
        });
    }
    out
}

/// The gap between two words is only white space (and comment continuation markers).
fn plain_gap(gap: &[char]) -> bool {
    !gap.is_empty()
        && gap.iter().any(|c| c.is_whitespace())
        && gap.iter().all(|c| c.is_whitespace() || matches!(c, '*' | '/' | '#'))
        // A marker needs a line break before it: "a / b" is not a gap.
        && (gap.iter().all(|c| c.is_whitespace()) || gap.contains(&'\n'))
}

const REPEAT_OK: &[&str] = &[
    "had", "that", "very", "bye", "so", "ha", "blah", "no", "yeah", "really", "ho", "knock", "tut",
    "chop", "go", "hip", "is",
];

/// Whether `word` starts with a vowel sound; `None` when it cannot be told.
pub fn vowel_sound(word: &str) -> Option<bool> {
    let letters: Vec<char> = word.chars().collect();
    let first = *letters.first()?;
    if !first.is_alphabetic() || letters.len() < 2 {
        return None;
    }
    // Acronyms are read letter by letter: "an HTML page", "a URL".
    if letters.iter().all(|c| c.is_uppercase()) && letters.len() <= 5 {
        if matches!(
            word,
            "SQL" | "FAQ" | "NASA" | "SCSS" | "UNIX" | "SASS" | "LESS" | "AJAX"
        ) {
            return None;
        }
        return Some("AEFHILMNORSX".contains(first));
    }
    let lower = word.to_lowercase();
    const CONSONANT_SOUND: &[&str] = &[
        "use", "usa", "usi", "usu", "uti", "ute", "uto", "uvu", "uku", "ubiq", "ufo", "ura", "ure",
        "uri", "uro", "eu", "ewe", "one", "once", "uuid", "url", "uri", "ux", "unix",
    ];
    const VOWEL_SOUND: &[&str] = &["hour", "honest", "honor", "honour", "heir", "herb"];
    if VOWEL_SOUND.iter().any(|p| lower.starts_with(p)) {
        return Some(true);
    }
    if CONSONANT_SOUND.iter().any(|p| lower.starts_with(p)) && !lower.starts_with("usher") {
        return Some(false);
    }
    // "a unique id", "a universal", but "an uninstall", "an unimportant", "an unidentified".
    if let Some(rest) = lower.strip_prefix("uni") {
        return Some(rest.starts_with(['n', 'm', 'd']));
    }
    Some(matches!(
        first.to_ascii_lowercase(),
        'a' | 'e' | 'i' | 'o' | 'u'
    ))
}

fn with_case(original: &str, replacement: &str) -> String {
    if original.chars().next().is_some_and(char::is_uppercase) {
        let mut c = replacement.chars();
        c.next()
            .map(|f| f.to_uppercase().chain(c).collect())
            .unwrap_or_default()
    } else {
        replacement.to_string()
    }
}

/// Lines (character index ranges) of doc comment text that start with a tag (`@param`).
fn tag_line_start(chars: &[char]) -> Option<usize> {
    let mut line_start = 0;
    for i in 0..=chars.len() {
        if i == chars.len() || chars[i] == '\n' {
            let line = &chars[line_start..i];
            let first = line
                .iter()
                .position(|c| !c.is_whitespace() && !matches!(c, '*' | '/'));
            if first.is_some_and(|f| line[f] == '@') {
                return Some(line_start);
            }
            line_start = i + 1;
        }
    }
    None
}

/// Checks `text` (UTF-16 offsets in the result).
pub fn check(text: &str, context: Context) -> Vec<GrammarProblem> {
    if !matches!(
        context,
        Context::Comment | Context::DocComment | Context::Text | Context::CommitMessage
    ) {
        return Vec::new();
    }
    let mut chars: Vec<char> = text.chars().collect();
    let mut offsets = Vec::with_capacity(chars.len() + 1);
    let mut at = 0;
    for c in &chars {
        offsets.push(at);
        at += c.len_utf16();
    }
    offsets.push(at);
    // Comment lines of commit messages are not part of the message.
    if context == Context::CommitMessage {
        let mut line_start = true;
        let mut comment = false;
        for c in chars.iter_mut() {
            if line_start {
                comment = *c == '#';
            }
            line_start = *c == '\n';
            if comment && *c != '\n' {
                *c = '\u{1}';
            }
        }
    }
    // Inline code is not prose (a placeholder that is neither a word nor a space).
    let mut open: Option<usize> = None;
    for i in 0..chars.len() {
        match (chars[i], open) {
            ('`', None) => open = Some(i),
            ('`', Some(s)) => {
                chars[s..=i].iter_mut().for_each(|c| *c = '\u{1}');
                open = None;
            }
            ('\n', _) => open = None,
            _ => {}
        }
    }
    let toks = tokens(&chars);
    let mut out = Vec::new();
    let utf16 = |i: usize| offsets[i];

    for pair in toks.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let gap = &chars[a.end..b.start];
        if !plain_gap(gap) {
            continue;
        }
        let (la, lb) = (a.text.to_lowercase(), b.text.to_lowercase());
        // repeated-word
        if la == lb && !REPEAT_OK.contains(&la.as_str()) && a.text.chars().all(char::is_alphabetic)
        {
            out.push(GrammarProblem {
                from: utf16(b.start),
                to: utf16(b.end),
                rule: "repeated-word",
                message: "Possible typo: you repeated a word".into(),
                fixes: vec![Fix {
                    title: format!("Remove the repeated '{}'", b.text),
                    from: utf16(a.end),
                    to: utf16(b.end),
                    text: String::new(),
                }],
            });
            continue;
        }
        // a-an
        if (la == "a" || la == "an") && !gap.contains(&'\n') {
            // "Option A is": a capital A in the middle of a sentence is a name.
            let before = chars[..a.start].iter().rev().find(|c| !c.is_whitespace());
            if a.text == "A"
                && before.is_some_and(|c| !matches!(c, '.' | '!' | '?' | '*' | '/' | '#'))
            {
                continue;
            }
            // "a.k.a.", "a-b": the next word must follow directly.
            if chars.get(a.end).is_some_and(|c| !c.is_whitespace()) {
                continue;
            }
            let Some(vowel) = vowel_sound(&b.text) else {
                continue;
            };
            let want = if vowel { "an" } else { "a" };
            if la != want {
                let replacement = with_case(&a.text, want);
                out.push(GrammarProblem {
                    from: utf16(a.start),
                    to: utf16(a.end),
                    rule: "a-an",
                    message: if vowel {
                        "Use 'an' instead of 'a' if the following word starts with a vowel sound, e.g. 'an article', 'an hour'.".into()
                    } else {
                        "Use 'a' instead of 'an' if the following word doesn't start with a vowel sound, e.g. 'a sentence', 'a university'.".into()
                    },
                    fixes: vec![Fix {
                        title: format!("Replace with '{replacement}'"),
                        from: utf16(a.start),
                        to: utf16(a.end),
                        text: replacement,
                    }],
                });
            }
        }
    }

    if context == Context::DocComment {
        sentence_starts(&chars, &toks, &offsets, &mut out);
    }
    double_spaces(&chars, &offsets, &mut out);
    out.sort_by_key(|p| (p.from, p.to));
    out
}

fn sentence_starts(
    chars: &[char],
    toks: &[Token],
    offsets: &[usize],
    out: &mut Vec<GrammarProblem>,
) {
    let limit = tag_line_start(chars).unwrap_or(chars.len());
    for t in toks.iter().filter(|t| t.start < limit) {
        let first = chars[t.start];
        if !first.is_lowercase() {
            continue;
        }
        // Code-like words: `iPhone`, `getName`, `snake_case`, `foo()`.
        if t.text
            .chars()
            .any(|c| c.is_uppercase() || c == '_' || c.is_ascii_digit())
            || chars.get(t.end).is_some_and(|c| {
                matches!(c, '(' | '.' | ':' | '-')
                    && chars.get(t.end + 1).is_some_and(|n| !n.is_whitespace())
            })
        {
            continue;
        }
        // What comes before the word: the comment start or the end of a sentence.
        let mut i = t.start;
        let mut saw_space = false;
        while i > 0 && (chars[i - 1].is_whitespace() || matches!(chars[i - 1], '*' | '/')) {
            saw_space |= chars[i - 1].is_whitespace();
            i -= 1;
        }
        let starts = if i == 0 {
            true
        } else if matches!(chars[i - 1], '.' | '!' | '?') && saw_space {
            // Not after abbreviations like "e.g." or "etc.".
            let before: String = chars[i.saturating_sub(5)..i]
                .iter()
                .collect::<String>()
                .to_lowercase();
            !["e.g.", "i.e.", "etc.", "vs.", "cf.", "approx."]
                .iter()
                .any(|a| before.ends_with(a))
                && !chars[..i - 1].last().is_some_and(|c| c.is_ascii_digit())
        } else {
            false
        };
        if !starts {
            continue;
        }
        let capitalized = with_case("X", &t.text);
        out.push(GrammarProblem {
            from: offsets[t.start],
            to: offsets[t.end],
            rule: "sentence-start",
            message: "This sentence does not start with an uppercase letter.".into(),
            fixes: vec![Fix {
                title: format!("Replace with '{capitalized}'"),
                from: offsets[t.start],
                to: offsets[t.end],
                text: capitalized,
            }],
        });
    }
}

fn double_spaces(chars: &[char], offsets: &[usize], out: &mut Vec<GrammarProblem>) {
    let mut line_start = 0;
    for end in 0..=chars.len() {
        if end < chars.len() && chars[end] != '\n' {
            continue;
        }
        let line = &chars[line_start..end];
        let content = line
            .iter()
            .position(|c| !c.is_whitespace() && !matches!(c, '*' | '/' | '#'))
            .unwrap_or(line.len());
        // Aligned tags and tables use spaces for layout.
        let skip = line.get(content) == Some(&'@') || line.contains(&'|') || line.contains(&'\t');
        let mut runs = Vec::new();
        let mut i = content;
        while i < line.len() {
            if line[i] == ' ' {
                let s = i;
                while i < line.len() && line[i] == ' ' {
                    i += 1;
                }
                let before = line[s - 1];
                let after = line.get(i).copied();
                if i - s >= 2
                    && after.is_some_and(char::is_alphabetic)
                    && (before.is_alphabetic()
                        || matches!(before, '.' | ',' | ';' | ':' | '!' | '?' | ')'))
                {
                    runs.push((s, i));
                }
            } else {
                i += 1;
            }
        }
        if !skip && runs.len() == 1 {
            let (s, e) = runs[0];
            let (s, e) = (offsets[line_start + s], offsets[line_start + e]);
            out.push(GrammarProblem {
                from: s,
                to: e,
                rule: "double-space",
                message: "Possible typo: you repeated a whitespace".into(),
                fixes: vec![Fix {
                    title: "Replace with a single space".into(),
                    from: s,
                    to: e,
                    text: " ".into(),
                }],
            });
        }
        line_start = end + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(text: &str, context: Context) -> Vec<(&'static str, String)> {
        check(text, context)
            .into_iter()
            .map(|p| {
                (
                    p.rule,
                    text.chars().skip(p.from).take(p.to - p.from).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn repeated_words() {
        assert_eq!(
            rules("Returns the the order.", Context::Comment),
            [("repeated-word", "the".into())]
        );
        let p = &check("Returns the\n * the order.", Context::DocComment)[0];
        assert_eq!(
            (p.rule, p.fixes[0].from, p.fixes[0].to),
            ("repeated-word", 11, 18)
        );
        assert!(rules("I had had enough", Context::Comment).is_empty());
        assert!(rules("the `the` keyword", Context::Comment).is_empty());
    }

    #[test]
    fn articles() {
        assert_eq!(
            rules("Creates a order for an user.", Context::Comment),
            [("a-an", "a".into()), ("a-an", "an".into())]
        );
        assert!(rules(
            "An hour, a URL, an HTML page, a unique id, an uninstall step, a one-off.",
            Context::Comment
        )
        .is_empty());
        let p = &check("A apple", Context::Comment)[0];
        assert_eq!(p.fixes[0].text, "An");
        assert!(rules("Option A is set, a-b, a.k.a. foo", Context::Comment).is_empty());
        assert_eq!(vowel_sound("SQL"), None);
        assert_eq!(vowel_sound("MVC"), Some(true));
    }

    #[test]
    fn sentence_start_in_doc_comments() {
        let doc = "/**\n * returns the order. it is cached, e.g. per request.\n *\n * @param int $id the id\n */";
        assert_eq!(
            rules(doc, Context::DocComment),
            [
                ("sentence-start", "returns".into()),
                ("sentence-start", "it".into())
            ]
        );
        assert!(rules("// returns the order", Context::Comment).is_empty());
        assert!(rules("/** getName() is used */", Context::DocComment).is_empty());
        assert_eq!(
            check("/** returns x */", Context::DocComment)[0].fixes[0].text,
            "Returns"
        );
    }

    #[test]
    fn double_spaces() {
        assert_eq!(
            rules("Loads the  order.", Context::Comment),
            [("double-space", "  ".into())]
        );
        assert!(rules("//   indented text", Context::Comment).is_empty());
        assert!(rules(" * @param int    $id   the id", Context::DocComment).is_empty());
        assert!(rules("name  type  default", Context::Comment).is_empty());
        assert!(rules("Loads the  order.", Context::String).is_empty());
        assert!(rules("Fix it\n# the the  order", Context::CommitMessage).is_empty());
    }
}
