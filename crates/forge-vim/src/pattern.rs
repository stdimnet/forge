//! Vim search patterns: translation of Vim's regex dialect (magic, `\v`, `\V`, `\<`, `\(`,
//! `\{n,m}`, `\c`) to a regular expression that the `regex` crate and JavaScript (search
//! highlighting in the editor) both understand, plus `:s` replacement strings.

use regex::{Captures, Regex, RegexBuilder};
use serde::Serialize;

/// A translated pattern. `source` uses the syntax common to Rust's `regex` and JavaScript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pattern {
    pub source: String,
    pub ignore_case: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Magic {
    Very,
    Normal,
    VeryNo,
}

fn push_literal(out: &mut String, c: char) {
    if "\\.+*?()|[]{}^$".contains(c) {
        out.push('\\');
    }
    match c {
        '\n' => out.push_str("\\n"),
        '\t' => out.push_str("\\t"),
        _ => out.push(c),
    }
}

/// Parse a `{n,m}` count body (after `{`), returning the regex quantifier and the chars used.
fn count(chars: &[char]) -> Option<(String, usize)> {
    let mut i = 0;
    let lazy = chars.first() == Some(&'-');
    if lazy {
        i += 1;
    }
    let start = i;
    while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == ',') {
        i += 1;
    }
    let body: String = chars[start..i].iter().collect();
    // `\}` or `}` closes.
    if chars.get(i) == Some(&'\\') {
        i += 1;
    }
    if chars.get(i) != Some(&'}') {
        return None;
    }
    let q = if body.is_empty() || body == "," {
        "*".to_string()
    } else if body.starts_with(',') {
        format!("{{0{body}}}")
    } else {
        format!("{{{body}}}")
    };
    Some((if lazy { format!("{q}?") } else { q }, i + 1))
}

/// Translate a Vim pattern. `ignorecase`/`smartcase` are the options.
pub fn translate(pattern: &str, ignorecase: bool, smartcase: bool) -> Pattern {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut magic = Magic::Normal;
    let mut case: Option<bool> = None;
    let mut has_upper = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            let n = chars[i + 1];
            i += 2;
            match n {
                'v' => magic = Magic::Very,
                'm' | 'M' => magic = Magic::Normal,
                'V' => magic = Magic::VeryNo,
                'c' => case = Some(true),
                'C' => case = Some(false),
                'z' if matches!(chars.get(i), Some('s' | 'e')) => i += 1,
                's' | 'S' | 'd' | 'D' | 'w' | 'W' | 'n' | 't' => {
                    out.push('\\');
                    out.push(n);
                }
                'a' => out.push_str("[A-Za-z]"),
                'A' => out.push_str("[^A-Za-z]"),
                'l' => out.push_str("[a-z]"),
                'L' => out.push_str("[^a-z]"),
                'u' => {
                    out.push_str("[A-Z]");
                }
                'U' => out.push_str("[^A-Z]"),
                'x' => out.push_str("[0-9A-Fa-f]"),
                'h' => out.push_str("[A-Za-z_]"),
                'k' | 'i' | 'f' => out.push_str("\\w"),
                _ if magic == Magic::Very => {
                    if n.is_ascii_uppercase() {
                        has_upper = true;
                    }
                    push_literal(&mut out, n);
                }
                '<' | '>' => out.push_str("\\b"),
                '(' => out.push('('),
                ')' => out.push(')'),
                '%' if chars.get(i) == Some(&'(') => {
                    out.push_str("(?:");
                    i += 1;
                }
                '|' => out.push('|'),
                '+' => out.push('+'),
                '=' | '?' => out.push('?'),
                '{' => match count(&chars[i..]) {
                    Some((q, used)) => {
                        out.push_str(&q);
                        i += used;
                    }
                    None => push_literal(&mut out, '{'),
                },
                '.' | '*' | '[' | ']' | '~' | '^' | '$' if magic == Magic::VeryNo => match n {
                    '.' => out.push('.'),
                    '*' => out.push('*'),
                    '^' => out.push('^'),
                    '$' => out.push('$'),
                    _ => push_literal(&mut out, n),
                },
                _ => {
                    if n.is_ascii_uppercase() {
                        has_upper = true;
                    }
                    push_literal(&mut out, n);
                }
            }
            continue;
        }
        i += 1;
        if c.is_uppercase() {
            has_upper = true;
        }
        match magic {
            Magic::VeryNo => push_literal(&mut out, c),
            Magic::Normal => match c {
                '.' | '*' => out.push(c),
                // `^` and `$` are anchors only at the start and end of a branch.
                '^' if out.is_empty() || out.ends_with('(') || out.ends_with('|') => out.push(c),
                '$' if i == chars.len()
                    || (chars.get(i) == Some(&'\\')
                        && matches!(chars.get(i + 1), Some(')' | '|'))) =>
                {
                    out.push(c)
                }
                '[' => {
                    // Copy a bracket expression as is.
                    let close = chars[i..]
                        .iter()
                        .enumerate()
                        .skip(usize::from(chars.get(i) == Some(&']')))
                        .find(|(_, x)| **x == ']')
                        .map(|(k, _)| i + k);
                    match close {
                        Some(end) => {
                            out.push('[');
                            for x in &chars[i..end] {
                                if *x == '[' {
                                    out.push('\\');
                                }
                                out.push(*x);
                            }
                            out.push(']');
                            i = end + 1;
                        }
                        None => push_literal(&mut out, '['),
                    }
                }
                _ => push_literal(&mut out, c),
            },
            Magic::Very => match c {
                '<' | '>' => out.push_str("\\b"),
                '=' => out.push('?'),
                '%' if chars.get(i) == Some(&'(') => {
                    out.push_str("(?:");
                    i += 1;
                }
                '{' => match count(&chars[i..]) {
                    Some((q, used)) => {
                        out.push_str(&q);
                        i += used;
                    }
                    None => push_literal(&mut out, '{'),
                },
                '.' | '*' | '^' | '$' | '(' | ')' | '|' | '+' | '?' | '[' | ']' => out.push(c),
                _ => push_literal(&mut out, c),
            },
        }
    }
    let ignore_case = case.unwrap_or(ignorecase && !(smartcase && has_upper));
    Pattern {
        source: out,
        ignore_case,
    }
}

impl Pattern {
    pub fn regex(&self) -> Result<Regex, String> {
        RegexBuilder::new(&self.source)
            .case_insensitive(self.ignore_case)
            .multi_line(true)
            .build()
            .map_err(|_| "E383: Invalid search string".to_string())
    }
}

/// Expand a Vim replacement string (`&`, `\0`–`\9`, `\r`/`\n`, `\t`, `\u`, `\U`, `\l`, `\L`,
/// `\e`, `\E`) for one match.
pub fn expand_replacement(rep: &str, caps: &Captures) -> String {
    #[derive(Clone, Copy, PartialEq)]
    enum Case {
        None,
        Upper,
        Lower,
    }
    let mut out = String::new();
    let mut span = Case::None;
    let mut one = Case::None;
    let push = |out: &mut String, s: &str, span: Case, one: &mut Case| {
        for ch in s.chars() {
            let c = match (*one, span) {
                (Case::Upper, _) => {
                    *one = Case::None;
                    ch.to_uppercase().collect::<String>()
                }
                (Case::Lower, _) => {
                    *one = Case::None;
                    ch.to_lowercase().collect::<String>()
                }
                (_, Case::Upper) => ch.to_uppercase().collect(),
                (_, Case::Lower) => ch.to_lowercase().collect(),
                _ => ch.to_string(),
            };
            out.push_str(&c);
        }
    };
    let chars: Vec<char> = rep.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        i += 1;
        if c == '&' {
            push(&mut out, &caps[0], span, &mut one);
            continue;
        }
        if c != '\\' || i >= chars.len() {
            push(&mut out, &c.to_string(), span, &mut one);
            continue;
        }
        let n = chars[i];
        i += 1;
        match n {
            '0'..='9' => {
                let g = n.to_digit(10).unwrap() as usize;
                let text = caps.get(g).map(|m| m.as_str()).unwrap_or("");
                push(&mut out, text, span, &mut one);
            }
            'r' | 'n' => out.push('\n'),
            't' => out.push('\t'),
            'u' => one = Case::Upper,
            'l' => one = Case::Lower,
            'U' => span = Case::Upper,
            'L' => span = Case::Lower,
            'e' | 'E' => span = Case::None,
            _ => push(&mut out, &n.to_string(), span, &mut one),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(p: &str) -> String {
        translate(p, false, false).source
    }

    #[test]
    fn translates_magic_patterns() {
        assert_eq!(src(r"\<foo\>"), r"\bfoo\b");
        assert_eq!(src(r"\(a\|b\)\+"), "(a|b)+");
        assert_eq!(src("a(b)+?"), r"a\(b\)\+\?");
        assert_eq!(src(r"x\{2,3}"), "x{2,3}");
        assert_eq!(src(r"x\{-}"), "x*?");
        assert_eq!(src("^\\s*$"), "^\\s*$");
        assert_eq!(src("[a-z]\\+"), "[a-z]+");
        assert_eq!(src(r"\v(a|b){2}<"), r"(a|b){2}\b");
        assert_eq!(src(r"\Va.b*"), r"a\.b\*");
        assert_eq!(src("$foo^"), r"\$foo\^");
    }

    #[test]
    fn applies_case_options() {
        assert!(translate("foo", true, true).ignore_case);
        assert!(!translate("Foo", true, true).ignore_case);
        assert!(translate("Foo", true, false).ignore_case);
        assert!(translate(r"foo\c", false, false).ignore_case);
        assert!(!translate(r"foo\C", true, false).ignore_case);
    }

    #[test]
    fn expands_replacements() {
        let re = Regex::new(r"(\w+) (\w+)").unwrap();
        let caps = re.captures("hello world").unwrap();
        assert_eq!(expand_replacement(r"\2 \1", &caps), "world hello");
        assert_eq!(expand_replacement(r"[&]", &caps), "[hello world]");
        assert_eq!(expand_replacement(r"\u\1\r\U\2\E!", &caps), "Hello\nWORLD!");
        assert_eq!(expand_replacement(r"a\&b", &caps), "a&b");
    }
}
