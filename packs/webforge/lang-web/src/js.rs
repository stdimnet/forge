//! Lexical helpers for JavaScript/TypeScript: matching brackets, top-level members of
//! object literals, type literals and destructuring patterns, string literals.

use crate::text::{is_ident_char, is_ident_start};

/// Offset of the bracket closing the one at `open` (`(`, `[`, `{` or `<`), skipping
/// strings, template literals and comments. For `<`, `=>` does not close.
pub fn matching(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let (o, c) = match *bytes.get(open)? {
        b'(' => (b'(', b')'),
        b'[' => (b'[', b']'),
        b'{' => (b'{', b'}'),
        b'<' => (b'<', b'>'),
        _ => return None,
    };
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'"' | b'\'' | b'`' => {
                i = skip_string(bytes, i);
                continue;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                i = text[i..].find('\n').map_or(bytes.len(), |e| i + e);
                continue;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = text[i + 2..].find("*/").map_or(bytes.len(), |e| i + e + 4);
                continue;
            }
            b'>' if c == b'>' && i > 0 && bytes[i - 1] == b'=' => {}
            _ if b == o => depth += 1,
            _ if b == c => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Offset after the string literal starting at `i`.
fn skip_string(bytes: &[u8], i: usize) -> usize {
    let q = bytes[i];
    let mut j = i + 1;
    while j < bytes.len() && bytes[j] != q {
        if bytes[j] == b'\\' {
            j += 1;
        }
        j += 1;
    }
    (j + 1).min(bytes.len())
}

/// The contents (without quotes) of the string literal starting at `i`: `(start, end)`.
pub fn string_at(text: &str, i: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    if !matches!(bytes.get(i)?, b'"' | b'\'' | b'`') {
        return None;
    }
    let end = skip_string(bytes, i);
    (end > i + 1 && bytes.get(end - 1) == bytes.get(i)).then_some((i + 1, end - 1))
}

/// The first offset at or after `i` that is not whitespace or a comment.
pub fn skip_trivia(text: &str, mut i: usize) -> usize {
    let bytes = text.as_bytes();
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if text[i..].starts_with("//") {
            i = text[i..].find('\n').map_or(bytes.len(), |e| i + e);
        } else if text[i..].starts_with("/*") {
            i = text[i + 2..].find("*/").map_or(bytes.len(), |e| i + e + 4);
        } else {
            return i;
        }
    }
}

/// The identifier starting at `i`, if any: `(start, end)`.
pub fn ident(text: &str, i: usize) -> Option<(usize, usize)> {
    let first = text.get(i..)?.chars().next()?;
    if !is_ident_start(first) {
        return None;
    }
    let end = text[i..]
        .char_indices()
        .find(|(_, c)| !is_ident_char(*c))
        .map_or(text.len(), |(j, _)| i + j);
    Some((i, end))
}

/// A member of an object literal, type literal, interface body or destructuring pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub key: String,
    pub key_start: usize,
    /// `name?: T` in a type.
    pub optional: bool,
    /// The text after `:` (type or value) or `=` (default value), trimmed.
    pub value: Option<(usize, usize)>,
    /// `name = default` in a destructuring pattern.
    pub has_default: bool,
}

/// Members between `start` and `end` (inside the braces). Separators are `,`, `;` and line
/// breaks at the top level; a line that continues a type (`| null`) stays with its member.
/// Spreads, methods' bodies and index signatures are skipped.
pub fn fields(text: &str, start: usize, end: usize) -> Vec<Field> {
    let bytes = text.as_bytes();
    let mut pieces = Vec::new();
    let mut piece_start = start;
    let mut i = start;
    while i < end {
        match bytes[i] {
            b'(' | b'[' | b'{' => {
                i = matching(text, i).map_or(end, |c| c + 1);
                continue;
            }
            b'<' => {
                // Generic arguments in a type; a comparison never appears in these bodies.
                if let Some(c) = matching(text, i).filter(|c| *c < end) {
                    i = c + 1;
                    continue;
                }
            }
            b'"' | b'\'' | b'`' => {
                i = skip_string(bytes, i);
                continue;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                i = text[i..end].find('\n').map_or(end, |e| i + e);
                continue;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = text[i + 2..end].find("*/").map_or(end, |e| i + e + 4);
                continue;
            }
            b',' | b';' => {
                pieces.push((piece_start, i));
                piece_start = i + 1;
            }
            b'\n' => {
                let before = text[piece_start..i].trim_end();
                let after = text[i..end].trim_start();
                let continues = before.ends_with(['|', '&', ':', '=', '?', '>'])
                    || after.starts_with(['|', '&', '.', '?', ':', '='])
                    || before.trim().is_empty();
                if !continues {
                    pieces.push((piece_start, i));
                    piece_start = i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    pieces.push((piece_start, end));
    pieces
        .into_iter()
        .filter_map(|(s, e)| field(text, s, e))
        .collect()
}

fn field(text: &str, s: usize, e: usize) -> Option<Field> {
    let mut i = skip_trivia(text, s);
    if i >= e {
        return None;
    }
    // Modifiers in interfaces and classes.
    while let Some((a, b)) = ident(text, i) {
        let word = &text[a..b];
        let next = skip_trivia(text, b);
        if matches!(
            word,
            "readonly" | "public" | "private" | "protected" | "static" | "declare"
        ) && next < e
            && text[next..].starts_with(|c: char| is_ident_start(c) || c == '\'' || c == '"')
        {
            i = next;
        } else {
            break;
        }
    }
    let (key, key_start, after) = match text.as_bytes()[i] {
        b'"' | b'\'' => {
            let (a, b) = string_at(text, i)?;
            (text[a..b].to_string(), i, b + 1)
        }
        _ => {
            let (a, b) = ident(text, i)?;
            (text[a..b].to_string(), a, b)
        }
    };
    let mut j = skip_trivia(text, after);
    let optional = text[j..].starts_with('?') && !text[j..].starts_with("?.");
    if optional {
        j = skip_trivia(text, j + 1);
    }
    let rest = text[j..e].trim_end();
    let (value, has_default) = if let Some(r) = rest.strip_prefix(':') {
        let vs = j + 1 + (r.len() - r.trim_start().len());
        (Some((vs, j + rest.len())), false)
    } else if let Some(r) = rest.strip_prefix('=').filter(|r| !r.starts_with('>')) {
        let vs = j + 1 + (r.len() - r.trim_start().len());
        (Some((vs, j + rest.len())), true)
    } else if rest.is_empty() {
        (None, false)
    } else {
        // `method(…) {…}`, `(…): T` call signatures and other shapes.
        return None;
    };
    Some(Field {
        key,
        key_start,
        optional,
        value,
        has_default,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(src: &str) -> Vec<(String, bool, String)> {
        let open = src.find('{').unwrap();
        let close = matching(src, open).unwrap();
        fields(src, open + 1, close)
            .into_iter()
            .map(|f| {
                let v = f
                    .value
                    .map(|(a, b)| src[a..b].to_string())
                    .unwrap_or_default();
                (f.key, f.optional, v)
            })
            .collect()
    }

    #[test]
    fn type_literal_members() {
        let src = "{\n  title: string;\n  count?: number\n  onSelect?: (id: number, e: { x: 1 }) => void,\n  // comment\n  status: 'a' | 'b'\n    | 'c';\n  readonly tags: Array<string>;\n}";
        assert_eq!(
            keys(src),
            [
                ("title".into(), false, "string".into()),
                ("count".into(), true, "number".into()),
                (
                    "onSelect".into(),
                    true,
                    "(id: number, e: { x: 1 }) => void".into()
                ),
                ("status".into(), false, "'a' | 'b'\n    | 'c'".into()),
                ("tags".into(), false, "Array<string>".into()),
            ]
        );
    }

    #[test]
    fn destructuring_and_object_literals() {
        let src = "{ title, size = 'md', onClick: handle, ...rest }";
        let k = keys(src);
        assert_eq!(
            k.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(),
            ["title", "size", "onClick"]
        );
        let src = "{ selector: 'app-x', 'styleUrls': ['./a.css'], standalone: true }";
        assert_eq!(
            keys(src)[1],
            ("styleUrls".into(), false, "['./a.css']".into())
        );
    }

    #[test]
    fn brackets_and_strings() {
        let src = "f(a, \")\", `${b})`, /* ) */ g(c))";
        assert_eq!(matching(src, 1), Some(src.len() - 1));
        let src = "defineProps<{ a: () => void }>()";
        assert_eq!(matching(src, 11), Some(29));
        assert_eq!(string_at("'abc'", 0), Some((1, 4)));
        assert_eq!(skip_trivia("  // x\n  /* y */ z", 0), 17);
    }
}
