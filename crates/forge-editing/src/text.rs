//! Shared helpers: UTF-16 offsets and a small lexer that tells code from strings and comments.

use crate::lang::Lang;

/// Length of `s` in UTF-16 code units.
pub(crate) fn u16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Length of `chars` in UTF-16 code units.
pub(crate) fn u16_len_chars(chars: &[char]) -> usize {
    chars.iter().map(|c| c.len_utf16()).sum()
}

/// Index into `chars` of the UTF-16 offset `offset` (clamped to the end).
pub(crate) fn char_index(chars: &[char], offset: usize) -> usize {
    let mut units = 0;
    for (i, c) in chars.iter().enumerate() {
        if units >= offset {
            return i;
        }
        units += c.len_utf16();
    }
    chars.len()
}

/// What a character of source text is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Class {
    Code,
    Str,
    Comment,
    /// Text outside `<?php … ?>` in a PHP file (HTML).
    Outside,
}

/// Classify every character of `chars`. PHP files start outside PHP code unless
/// `starts_in_code` is set (a single line or a fragment).
pub(crate) fn classify(chars: &[char], lang: Lang, starts_in_code: bool) -> Vec<Class> {
    let mut out = vec![Class::Code; chars.len()];
    let at = |i: usize, s: &str| {
        s.chars()
            .enumerate()
            .all(|(k, c)| chars.get(i + k) == Some(&c))
    };
    let mut i = 0;
    let mut outside = lang == Lang::Php && !starts_in_code;
    while i < chars.len() {
        if outside {
            if at(i, "<?php") || at(i, "<?=") {
                let n = if at(i, "<?php") { 5 } else { 3 };
                for o in out.iter_mut().skip(i).take(n) {
                    *o = Class::Outside;
                }
                i += n;
                outside = false;
            } else {
                out[i] = Class::Outside;
                i += 1;
            }
            continue;
        }
        let c = chars[i];
        if lang == Lang::Php && at(i, "?>") {
            out[i] = Class::Outside;
            out[i + 1] = Class::Outside;
            i += 2;
            outside = true;
            continue;
        }
        // Comments.
        let line_comment = match lang {
            Lang::Php => at(i, "//") || (c == '#' && chars.get(i + 1) != Some(&'[')),
            Lang::Js => at(i, "//"),
            Lang::Lua => at(i, "--") && long_bracket_at(chars, i + 2).is_none(),
        };
        if line_comment {
            while i < chars.len() && chars[i] != '\n' {
                if lang == Lang::Php && at(i, "?>") {
                    break;
                }
                out[i] = Class::Comment;
                i += 1;
            }
            continue;
        }
        if lang != Lang::Lua && at(i, "/*") {
            let start = i;
            i += 2;
            while i < chars.len() && !at(i, "*/") {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            for o in out.iter_mut().take(i).skip(start) {
                *o = Class::Comment;
            }
            continue;
        }
        if lang == Lang::Lua {
            let (comment, open) = if at(i, "--") {
                (true, i + 2)
            } else {
                (false, i)
            };
            if let Some(level) = long_bracket_at(chars, open) {
                let close: String = format!("]{}]", "=".repeat(level));
                let start = i;
                i = open + level + 2;
                while i < chars.len() && !at(i, &close) {
                    i += 1;
                }
                i = (i + close.len()).min(chars.len());
                let class = if comment { Class::Comment } else { Class::Str };
                for o in out.iter_mut().take(i).skip(start) {
                    *o = class;
                }
                continue;
            }
        }
        // Strings.
        let quote = c == '"' || c == '\'' || (c == '`' && lang != Lang::Lua);
        if quote {
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != c {
                if chars[i] == '\\' {
                    i += 1;
                } else if chars[i] == '\n' && c != '`' {
                    break;
                }
                i += 1;
            }
            i = (i + 1).min(chars.len());
            for o in out.iter_mut().take(i).skip(start) {
                *o = Class::Str;
            }
            continue;
        }
        i += 1;
    }
    out
}

/// `[[` or `[==[` at `i`: the number of `=`.
fn long_bracket_at(chars: &[char], i: usize) -> Option<usize> {
    if chars.get(i) != Some(&'[') {
        return None;
    }
    let mut level = 0;
    while chars.get(i + 1 + level) == Some(&'=') {
        level += 1;
    }
    (chars.get(i + 1 + level) == Some(&'[')).then_some(level)
}

/// The leading whitespace of a line.
pub(crate) fn indent_of(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes(src: &str, lang: Lang, code: bool) -> String {
        let chars: Vec<char> = src.chars().collect();
        classify(&chars, lang, code)
            .into_iter()
            .map(|c| match c {
                Class::Code => 'c',
                Class::Str => 's',
                Class::Comment => '/',
                Class::Outside => 'o',
            })
            .collect()
    }

    #[test]
    fn utf16_offsets() {
        let chars: Vec<char> = "a😀b".chars().collect();
        assert_eq!(u16_len("a😀b"), 4);
        assert_eq!(char_index(&chars, 3), 2);
        assert_eq!(char_index(&chars, 9), 3);
        assert_eq!(u16_len_chars(&chars[..2]), 3);
    }

    #[test]
    fn strings_and_comments() {
        assert_eq!(classes("a 'b' // c", Lang::Js, true), "ccsssc////");
        assert_eq!(classes("x # y", Lang::Php, true), "cc///");
        assert_eq!(classes("#[A]", Lang::Php, true), "cccc");
        assert_eq!(classes("a --x", Lang::Lua, true), "cc///");
        assert_eq!(classes("[[a]] b", Lang::Lua, true), "ssssscc");
        assert_eq!(
            classes("<p><?php $a ?>", Lang::Php, false),
            "ooooooooccccoo"
        );
        assert_eq!(classes("'a\\'b' c", Lang::Php, true), "sssssscc");
    }
}
