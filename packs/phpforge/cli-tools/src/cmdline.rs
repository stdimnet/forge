//! The console input (`alias command args`): split like a POSIX shell (single and double
//! quotes, backslash escapes) with the offsets of every token, and quoting for display.
//! Offsets are UTF-16 code units, like the UI's string indices.

use serde::{Deserialize, Serialize};

/// A word of the input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    /// The word without quotes and escapes.
    pub value: String,
    /// UTF-16 offsets of the raw word in the input (`end` exclusive).
    pub start: usize,
    pub end: usize,
    /// The word started with a quote.
    pub quoted: bool,
}

/// Split `input` into words. An unterminated quote runs to the end of the input.
pub fn tokenize(input: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut cur: Option<Token> = None;
    let mut quote: Option<char> = None;
    let mut pos = 0usize;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        let at = pos;
        pos += c.len_utf16();
        let begin = |cur: &mut Option<Token>, quoted: bool| {
            cur.get_or_insert_with(|| Token {
                value: String::new(),
                start: at,
                end: at,
                quoted,
            });
        };
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => {
                if let Some(&n) = chars.peek() {
                    if matches!(n, '"' | '\\' | '$' | '`') {
                        chars.next();
                        pos += n.len_utf16();
                        cur.as_mut().expect("in a word").value.push(n);
                    } else {
                        cur.as_mut().expect("in a word").value.push('\\');
                    }
                }
            }
            (Some(_), c) => cur.as_mut().expect("in a word").value.push(c),
            (None, '\'' | '"') => {
                begin(&mut cur, true);
                quote = Some(c);
            }
            (None, '\\') => {
                begin(&mut cur, false);
                if let Some(n) = chars.next() {
                    pos += n.len_utf16();
                    cur.as_mut().expect("in a word").value.push(n);
                }
            }
            (None, c) if c.is_whitespace() => {
                if let Some(mut t) = cur.take() {
                    t.end = at;
                    out.push(t);
                }
            }
            (None, c) => {
                begin(&mut cur, false);
                cur.as_mut().expect("in a word").value.push(c);
            }
        }
    }
    if let Some(mut t) = cur {
        t.end = pos;
        out.push(t);
    }
    out
}

/// The words of `input`.
pub fn split(input: &str) -> Vec<String> {
    tokenize(input).into_iter().map(|t| t.value).collect()
}

/// Quote a word for a POSIX shell when it needs it.
pub fn quote(word: &str) -> String {
    if !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:@%+,^~".contains(c))
    {
        return word.to_string();
    }
    format!("'{}'", word.replace('\'', "'\\''"))
}

/// `words` as one line, quoted where needed.
pub fn join(words: &[String]) -> String {
    words.iter().map(|w| quote(w)).collect::<Vec<_>>().join(" ")
}

/// UTF-16 length of `s`.
pub fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// The part of `input` before the UTF-16 offset `offset`.
pub fn prefix_utf16(input: &str, offset: usize) -> &str {
    let mut pos = 0;
    for (i, c) in input.char_indices() {
        if pos >= offset {
            return &input[..i];
        }
        pos += c.len_utf16();
    }
    input
}

/// The text of `input` between two UTF-16 offsets.
pub fn slice_utf16(input: &str, start: usize, end: usize) -> &str {
    let head = prefix_utf16(input, start).len();
    let tail = prefix_utf16(input, end).len();
    &input[head..tail.max(head)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_with_offsets_and_quotes() {
        let t = tokenize(r#"a  make:model "Order Item" --x='a b' \"q"#);
        let v: Vec<(&str, usize, usize)> = t
            .iter()
            .map(|t| (t.value.as_str(), t.start, t.end))
            .collect();
        assert_eq!(
            v,
            [
                ("a", 0, 1),
                ("make:model", 3, 13),
                ("Order Item", 14, 26),
                ("--x=a b", 27, 36),
                ("\"q", 37, 40),
            ]
        );
        assert!(t[2].quoted && !t[3].quoted);
        assert_eq!(
            split(r#"tinker --execute="echo \"hi\";""#)[1],
            "--execute=echo \"hi\";"
        );
        assert_eq!(split("x 'open"), ["x", "open"]);
        assert!(split("   ").is_empty());
        assert_eq!(split("x ''"), ["x", ""]);
    }

    #[test]
    fn offsets_count_utf16_units() {
        let t = tokenize("a ünï 😀x");
        assert_eq!((t[1].start, t[1].end), (2, 5));
        assert_eq!((t[2].start, t[2].end), (6, 9));
        assert_eq!(prefix_utf16("a 😀x", 4), "a 😀");
        assert_eq!(slice_utf16("a 😀x", 2, 5), "😀x");
    }

    #[test]
    fn quotes_for_display() {
        assert_eq!(
            join(&[
                "make:model".into(),
                "Order Item".into(),
                "it's".into(),
                "".into()
            ]),
            r#"make:model 'Order Item' 'it'\''s' ''"#
        );
        assert_eq!(split(&join(&["a b".into(), "c'd".into()])), ["a b", "c'd"]);
    }
}
