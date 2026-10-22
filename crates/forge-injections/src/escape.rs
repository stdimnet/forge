//! Escaping between a string literal's source text and the value of its injected fragment
//! (what Edit Fragment shows and Run SQL sends).

use crate::lexer::LiteralKind;

/// The value of `src[from..to]` (the content of a literal of `kind`) and, for every byte of
/// the value plus its end, the source offset it came from.
pub fn unescape(kind: LiteralKind, src: &str, from: usize, to: usize) -> (String, Vec<usize>) {
    let s = &src[from..to];
    let mut out = String::with_capacity(s.len());
    let mut map = Vec::with_capacity(s.len() + 1);
    let push = |out: &mut String, map: &mut Vec<usize>, ch: char, at: usize| {
        out.push(ch);
        for _ in 0..ch.len_utf8() {
            map.push(from + at);
        }
    };
    let mut i = 0;
    while i < s.len() {
        if s.as_bytes()[i] == b'\\' && kind != LiteralKind::PhpNowdoc {
            if let Some((ch, len)) = decode_escape(kind, &s[i..]) {
                if let Some(ch) = ch {
                    push(&mut out, &mut map, ch, i);
                }
                i += len;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap_or('\u{fffd}');
        push(&mut out, &mut map, ch, i);
        i += ch.len_utf8().max(1);
    }
    map.push(to);
    (out, map)
}

/// The escape sequence at the start of `s` (which starts with `\`): the character it stands
/// for (`None` for a JavaScript line continuation) and its length; `None` when the
/// backslash is literal.
fn decode_escape(kind: LiteralKind, s: &str) -> Option<(Option<char>, usize)> {
    let b = s.as_bytes();
    let next = *b.get(1)?;
    let simple = |c: char| Some((Some(c), 2));
    match kind {
        LiteralKind::PhpNowdoc => None,
        LiteralKind::PhpSingle => match next {
            b'\\' => simple('\\'),
            b'\'' => simple('\''),
            _ => None,
        },
        LiteralKind::PhpDouble | LiteralKind::PhpHeredoc => match next {
            b'\\' => simple('\\'),
            b'$' => simple('$'),
            b'"' if kind == LiteralKind::PhpDouble => simple('"'),
            b'n' => simple('\n'),
            b't' => simple('\t'),
            b'r' => simple('\r'),
            b'v' => simple('\u{b}'),
            b'f' => simple('\u{c}'),
            b'e' => simple('\u{1b}'),
            b'0'..=b'7' => {
                let digits = b[1..]
                    .iter()
                    .take(3)
                    .take_while(|c| (b'0'..=b'7').contains(*c))
                    .count();
                let v = u32::from_str_radix(&s[1..1 + digits], 8).ok()? & 0xff;
                Some((char::from_u32(v), 1 + digits))
            }
            b'x' => {
                let digits = b[2..]
                    .iter()
                    .take(2)
                    .take_while(|c| c.is_ascii_hexdigit())
                    .count();
                if digits == 0 {
                    return None;
                }
                let v = u32::from_str_radix(&s[2..2 + digits], 16).ok()?;
                Some((char::from_u32(v), 2 + digits))
            }
            b'u' => braced_unicode(s),
            _ => None,
        },
        LiteralKind::JsSingle | LiteralKind::JsDouble | LiteralKind::JsTemplate => match next {
            b'n' => simple('\n'),
            b't' => simple('\t'),
            b'r' => simple('\r'),
            b'b' => simple('\u{8}'),
            b'f' => simple('\u{c}'),
            b'v' => simple('\u{b}'),
            b'0' if !b.get(2).is_some_and(|c| c.is_ascii_digit()) => simple('\0'),
            b'x' => {
                let hex = s.get(2..4)?;
                let v = u32::from_str_radix(hex, 16).ok()?;
                Some((char::from_u32(v), 4))
            }
            b'u' if b.get(2) == Some(&b'{') => braced_unicode(s),
            b'u' => {
                let hex = s.get(2..6)?;
                let v = u32::from_str_radix(hex, 16).ok()?;
                Some((Some(char::from_u32(v).unwrap_or('\u{fffd}')), 6))
            }
            b'\r' if b.get(2) == Some(&b'\n') => Some((None, 3)),
            b'\n' => Some((None, 2)),
            _ => {
                // Any other escaped character stands for itself (`\'`, `\"`, `` \` ``, `\\`, `\$`).
                let ch = s[1..].chars().next()?;
                Some((Some(ch), 1 + ch.len_utf8()))
            }
        },
    }
}

/// `\u{1F600}`.
fn braced_unicode(s: &str) -> Option<(Option<char>, usize)> {
    let rest = s.get(2..)?.strip_prefix('{')?;
    let close = rest.find('}')?;
    let v = u32::from_str_radix(&rest[..close], 16).ok()?;
    Some((char::from_u32(v), 3 + close + 1))
}

/// `text` written into a literal of `kind`, so that the literal's value contains `text`.
/// Line breaks stay literal where the language allows them (PHP strings, templates).
pub fn escape(kind: LiteralKind, text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for ch in text.chars() {
        match (kind, ch) {
            (LiteralKind::PhpNowdoc, _) => out.push(ch),
            (_, '\\') => out.push_str("\\\\"),
            (LiteralKind::PhpSingle, '\'') => out.push_str("\\'"),
            (LiteralKind::PhpDouble, '"') => out.push_str("\\\""),
            (LiteralKind::PhpDouble | LiteralKind::PhpHeredoc, '$') => out.push_str("\\$"),
            (LiteralKind::JsSingle, '\'') => out.push_str("\\'"),
            (LiteralKind::JsDouble, '"') => out.push_str("\\\""),
            (LiteralKind::JsSingle | LiteralKind::JsDouble, '\n') => out.push_str("\\n"),
            (LiteralKind::JsSingle | LiteralKind::JsDouble, '\r') => out.push_str("\\r"),
            (LiteralKind::JsTemplate, '`') => out.push_str("\\`"),
            (LiteralKind::JsTemplate, '$') => out.push_str("\\$"),
            _ => out.push(ch),
        }
    }
    out
}
