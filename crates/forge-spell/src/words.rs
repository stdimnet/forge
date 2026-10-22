//! Word extraction: which parts of a comment, string, identifier, text or file name are
//! words to spell check. Identifiers (and identifier-like words in text) are split at
//! camelCase humps, `snake_case` / `kebab-case` separators, ALLCAPS runs and digits. URLs,
//! e-mail addresses, paths, hex numbers, UUIDs, code references, inline code, escape
//! sequences, words with digits and words shorter than three letters are skipped.
//!
//! All offsets are UTF-16 code units from the start of the text.

use serde::{Deserialize, Serialize};

/// Where a piece of text comes from; decides how it is split and which inspections apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Context {
    Comment,
    DocComment,
    String,
    Identifier,
    /// Markdown or plain text files.
    Text,
    CommitMessage,
    FileName,
}

impl Context {
    pub fn is_prose(self) -> bool {
        !matches!(self, Context::Identifier | Context::FileName)
    }
}

/// A word to check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub from: usize,
    pub to: usize,
    pub text: String,
    /// The whole identifier the word is part of (Rename).
    pub identifier: Option<(usize, usize)>,
}

/// Words shorter than this are never checked.
pub const MIN_WORD_LENGTH: usize = 3;

fn is_apostrophe(c: char) -> bool {
    c == '\'' || c == '\u{2019}'
}

/// The text as characters with their UTF-16 offsets, plus the end offset.
fn chars_of(text: &str) -> (Vec<char>, Vec<usize>) {
    let mut chars = Vec::new();
    let mut offsets = Vec::new();
    let mut at = 0;
    for c in text.chars() {
        chars.push(c);
        offsets.push(at);
        at += c.len_utf16();
    }
    offsets.push(at);
    (chars, offsets)
}

/// Blanks out what is never prose: escape sequences in strings and `inline code`.
fn mask(chars: &mut [char], context: Context) {
    if context == Context::String {
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '\\' {
                chars[i] = ' ';
                if i + 1 < chars.len() {
                    chars[i + 1] = ' ';
                }
                i += 2;
            } else {
                i += 1;
            }
        }
    }
    // Paired backticks on one line.
    let mut open: Option<usize> = None;
    for i in 0..chars.len() {
        match (chars[i], open) {
            ('`', None) => open = Some(i),
            ('`', Some(start)) => {
                for c in &mut chars[start..=i] {
                    *c = ' ';
                }
                open = None;
            }
            ('\n', _) => open = None,
            _ => {}
        }
    }
}

fn is_hex_digit_str(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts.iter().map(|p| p.len()).eq([8, 4, 4, 4, 12])
        && parts.iter().all(|p| is_hex_digit_str(p))
}

/// Chunks (whitespace-separated) that are not prose at all.
fn skip_chunk(chunk: &str) -> bool {
    let trimmed = chunk.trim_matches(|c: char| "()[]{}<>\"'.,;:!?*".contains(c));
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    // URLs and e-mail addresses.
    if lower.contains("://") || lower.starts_with("www.") || lower.starts_with("mailto:") {
        return true;
    }
    if let Some(at) = trimmed.find('@') {
        if at > 0 && trimmed[at..].contains('.') {
            return true;
        }
    }
    // Paths: `/usr/bin`, `./x`, `~/x`, `src/app.ts`, `a/b/c`.
    if trimmed.contains('/') {
        let after_last = trimmed.rsplit('/').next().unwrap_or_default();
        if trimmed.starts_with(['/', '.', '~'])
            || trimmed.matches('/').count() >= 2
            || after_last.contains('.')
        {
            return true;
        }
    }
    // Hex numbers and colors, UUIDs.
    if lower.starts_with("0x") || (trimmed.starts_with('#') && is_hex_digit_str(&trimmed[1..])) {
        return true;
    }
    if is_uuid(trimmed) {
        return true;
    }
    // Code references: `Foo::bar()`, `$this->x`, `a => b`, variables.
    if trimmed.contains("::")
        || trimmed.contains("->")
        || trimmed.contains("()")
        || trimmed.contains("=>")
        || trimmed.starts_with('$')
    {
        return true;
    }
    false
}

/// Splits an identifier (`getHTTPResponse2`, `max_retry_count`, `user-name`) into its words:
/// index ranges of letter runs.
pub fn split_identifier(chars: &[char]) -> Vec<(usize, usize)> {
    let mut parts = Vec::new();
    let mut start: Option<usize> = None;
    let flush = |start: &mut Option<usize>, end: usize, parts: &mut Vec<(usize, usize)>| {
        if let Some(s) = start.take() {
            if end > s {
                parts.push((s, end));
            }
        }
    };
    for i in 0..chars.len() {
        let c = chars[i];
        if !c.is_alphabetic() {
            flush(&mut start, i, &mut parts);
            continue;
        }
        if let Some(s) = start {
            let prev = chars[i - 1];
            let next = chars.get(i + 1).copied();
            let hump = prev.is_lowercase() && c.is_uppercase();
            // `HTTPResponse`: the last capital of a run starts the next word.
            let acronym_end = prev.is_uppercase()
                && c.is_uppercase()
                && next.is_some_and(char::is_lowercase)
                && i > s;
            if hump || acronym_end {
                flush(&mut start, i, &mut parts);
            }
        }
        if start.is_none() {
            start = Some(i);
        }
    }
    flush(&mut start, chars.len(), &mut parts);
    parts
}

fn letters(chars: &[char]) -> usize {
    chars.iter().filter(|c| c.is_alphabetic()).count()
}

/// The words of `text` in `context`.
pub fn words(text: &str, context: Context) -> Vec<Word> {
    let (mut chars, offsets) = chars_of(text);
    let mut out = Vec::new();
    match context {
        Context::Identifier => identifier_words(&chars, &offsets, 0, chars.len(), true, &mut out),
        Context::FileName => {
            // The name without its extension(s); dot files keep the part after the dot.
            let begin = usize::from(chars.first() == Some(&'.'));
            let end = chars[begin..]
                .iter()
                .position(|c| *c == '.')
                .map_or(chars.len(), |p| p + begin);
            identifier_words(&chars, &offsets, begin, end, false, &mut out);
        }
        _ => {
            mask(&mut chars, context);
            let mut i = 0;
            while i < chars.len() {
                if chars[i].is_whitespace() {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < chars.len() && !chars[i].is_whitespace() {
                    i += 1;
                }
                let chunk: String = chars[start..i].iter().collect();
                if !skip_chunk(&chunk) {
                    prose_words(&chars, &offsets, start, i, &mut out);
                }
            }
        }
    }
    out
}

fn identifier_words(
    chars: &[char],
    offsets: &[usize],
    begin: usize,
    end: usize,
    rename: bool,
    out: &mut Vec<Word>,
) {
    let slice = &chars[begin..end];
    if slice.iter().all(|c| !c.is_alphabetic()) {
        return;
    }
    // The identifier without sigils (`$name`, `@attr`, `#private`).
    let first = slice
        .iter()
        .position(|c| c.is_alphanumeric() || *c == '_')
        .unwrap_or(0)
        + begin;
    let identifier = rename.then(|| (offsets[first], offsets[end]));
    for (s, e) in split_identifier(slice) {
        let (s, e) = (s + begin, e + begin);
        if e - s < MIN_WORD_LENGTH {
            continue;
        }
        out.push(Word {
            from: offsets[s],
            to: offsets[e],
            text: chars[s..e].iter().collect(),
            identifier,
        });
    }
}

fn prose_words(chars: &[char], offsets: &[usize], start: usize, end: usize, out: &mut Vec<Word>) {
    let is_word_char = |i: usize| {
        let c = chars[i];
        c.is_alphanumeric()
            || c == '_'
            || (is_apostrophe(c)
                && i > start
                && i + 1 < end
                && chars[i - 1].is_alphabetic()
                && chars[i + 1].is_alphabetic())
    };
    let mut i = start;
    while i < end {
        if !is_word_char(i) {
            i += 1;
            continue;
        }
        let run_start = i;
        while i < end && is_word_char(i) {
            i += 1;
        }
        let run = &chars[run_start..i];
        // Doc tags, variables, entities and format placeholders.
        if run_start > start && matches!(chars[run_start - 1], '@' | '$' | '&' | '%' | '\\') {
            continue;
        }
        if run.iter().any(|c| c.is_ascii_digit()) {
            continue;
        }
        let camel = run
            .windows(2)
            .any(|w| w[0].is_lowercase() && w[1].is_uppercase());
        if run.contains(&'_') || camel {
            identifier_words(chars, offsets, run_start, i, false, out);
            continue;
        }
        let n = letters(run);
        if n < MIN_WORD_LENGTH {
            continue;
        }
        // Acronyms in text (`HTML`, `ORM`) and their plurals (`PDFs`, `URLs`).
        let stem = match run.split_last() {
            Some((&'s', rest)) if rest.len() >= 2 && rest.iter().all(|c| c.is_uppercase()) => rest,
            _ => run,
        };
        if stem.len() <= 5 && stem.iter().all(|c| c.is_uppercase()) {
            continue;
        }
        out.push(Word {
            from: offsets[run_start],
            to: offsets[i],
            text: run.iter().collect(),
            identifier: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(text: &str, context: Context) -> Vec<String> {
        words(text, context).into_iter().map(|w| w.text).collect()
    }

    #[test]
    fn splits_identifiers() {
        let parts = |s: &str| {
            let chars: Vec<char> = s.chars().collect();
            split_identifier(&chars)
                .into_iter()
                .map(|(a, b)| chars[a..b].iter().collect::<String>())
                .collect::<Vec<_>>()
        };
        assert_eq!(parts("getHTTPResponse"), ["get", "HTTP", "Response"]);
        assert_eq!(parts("XMLHttpRequest"), ["XML", "Http", "Request"]);
        assert_eq!(parts("max_retry_count"), ["max", "retry", "count"]);
        assert_eq!(parts("MAX_RETRY"), ["MAX", "RETRY"]);
        assert_eq!(parts("user-name"), ["user", "name"]);
        assert_eq!(parts("base64Encode"), ["base", "Encode"]);
        assert_eq!(parts("$orderTotl"), ["order", "Totl"]);
    }

    #[test]
    fn identifier_words_carry_the_identifier() {
        let w = words("$usrName", Context::Identifier);
        assert_eq!(w.len(), 2);
        assert_eq!((w[0].from, w[0].to, w[0].identifier), (1, 4, Some((1, 8))));
        assert_eq!(w[1].text, "Name");
        // Short parts are skipped.
        assert_eq!(texts("idx", Context::Identifier), ["idx"]);
        assert!(texts("$id", Context::Identifier).is_empty());
    }

    #[test]
    fn prose_skips_code_urls_and_paths() {
        let text = "Recieve the order from https://exmple.com/x, mail bob@exmple.com or see \
                    src/Foo.php, ~/cfg and /usr/lib. Calls `fooBarr()` via Order::findd() \
                    and $varr; color #ffaacc, id 550e8400-e29b-41d4-a716-446655440000, 0xdeadbeef.";
        assert_eq!(
            texts(text, Context::Comment),
            [
                "Recieve", "the", "order", "from", "mail", "see", "and", "Calls", "via", "and",
                "color"
            ]
        );
    }

    #[test]
    fn prose_splits_identifiers_and_skips_tags_and_acronyms() {
        assert_eq!(
            texts(
                "@param string $name The usrName of the HTML page",
                Context::DocComment
            ),
            ["string", "The", "usr", "Name", "the", "page"]
        );
        assert_eq!(
            texts("don't stop, it’s fine", Context::Comment),
            ["don't", "stop", "it’s", "fine"]
        );
        assert_eq!(
            texts("v2 utf8 abc123 ok", Context::Comment),
            Vec::<String>::new()
        );
        assert_eq!(texts("PDFs and URLs", Context::Comment), ["and"]);
    }

    #[test]
    fn strings_skip_escapes() {
        assert_eq!(
            texts("Hello\\nWorld\\tmore %s items", Context::String),
            ["Hello", "World", "more", "items"]
        );
        let w = words("Hello\\nWorld", Context::String);
        assert_eq!((w[1].from, w[1].to), (7, 12));
    }

    #[test]
    fn file_names() {
        assert_eq!(
            texts("OrdrController.php", Context::FileName),
            ["Ordr", "Controller"]
        );
        assert_eq!(texts(".gitignore", Context::FileName), ["gitignore"]);
        assert_eq!(
            words("OrdrController.php", Context::FileName)[0].identifier,
            None
        );
    }

    #[test]
    fn utf16_offsets() {
        let w = words("😀 naïve wrd", Context::Comment);
        assert_eq!((w[0].from, w[0].to), (3, 8));
        assert_eq!((w[1].from, w[1].to), (9, 12));
    }
}
