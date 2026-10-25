//! "Use Yoda condition" hint of the WordPress Coding Standards: in a comparison with `==`,
//! `!=`, `===` or `!==`, the literal goes on the left (`if ( 'publish' === $status )`), so
//! a mistyped `=` cannot assign. Reported as a weak warning with a quick-fix that swaps the
//! operands. Only simple operands are checked: a variable with property, static and array
//! accesses on the left, a string, number, `true`, `false` or `null` on the right.

use serde::{Deserialize, Serialize};

use crate::lexer::{self, TokKind, Token};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YodaHint {
    /// Byte range of the whole comparison.
    pub start: usize,
    pub end: usize,
    pub message: String,
    /// The comparison with swapped operands (`'publish' === $status`).
    pub replacement: String,
}

const OPERATORS: &[&str] = &["==", "!=", "===", "!==", "<>"];

fn is_literal(t: &Token<'_>) -> bool {
    match t.kind {
        TokKind::Str => !t.text.starts_with("<<<"),
        TokKind::Number => true,
        TokKind::Ident => t.is_ident("true") || t.is_ident("false") || t.is_ident("null"),
        _ => false,
    }
}

/// End index (exclusive) of a variable operand starting at `i`: `$a`, `$a->b`, `$a['k']`,
/// `$a->b[0]->c`, `self::$x` is not included.
fn variable_end(tokens: &[Token<'_>], i: usize) -> Option<usize> {
    if tokens.get(i)?.kind != TokKind::Variable {
        return None;
    }
    let mut j = i + 1;
    loop {
        match tokens.get(j) {
            Some(t)
                if (t.is("->") || t.is("?->"))
                    && tokens.get(j + 1).is_some_and(|n| n.kind == TokKind::Ident) =>
            {
                // A method call is not a simple operand.
                if tokens.get(j + 2).is_some_and(|n| n.is("(")) {
                    return None;
                }
                j += 2;
            }
            Some(t) if t.is("[") => {
                let close = lexer::matching(tokens, j);
                if !tokens.get(close).is_some_and(|c| c.is("]")) {
                    return None;
                }
                j = close + 1;
            }
            _ => return Some(j),
        }
    }
}

/// Tokens that may stand before a comparison operand (so `$a . $b == 'x'` is skipped).
fn starts_operand(prev: Option<&Token<'_>>) -> bool {
    match prev {
        None => true,
        Some(p) => {
            p.kind == TokKind::Punct
                && matches!(
                    p.text,
                    "(" | "&&" | "||" | "!" | "," | ";" | "?" | ":" | "=" | "[" | "{" | "=>"
                )
                || p.is_ident("and")
                || p.is_ident("or")
                || p.is_ident("xor")
                || p.is_ident("return")
        }
    }
}

/// Tokens that may follow the literal (so `$a == 'x' . $b` is skipped).
fn ends_operand(next: Option<&Token<'_>>) -> bool {
    match next {
        None => true,
        Some(n) => {
            n.kind == TokKind::Punct
                && matches!(
                    n.text,
                    ")" | "&&" | "||" | "," | ";" | "?" | ":" | "]" | "}"
                )
                || n.is_ident("and")
                || n.is_ident("or")
                || n.is_ident("xor")
        }
    }
}

/// The comparisons in `src` that should be Yoda conditions.
pub fn hints(src: &str) -> Vec<YodaHint> {
    let tokens = lexer::tokenize(src);
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let Some(var_end) = variable_end(&tokens, i)
            .filter(|_| starts_operand(i.checked_sub(1).map(|p| &tokens[p])))
        else {
            i += 1;
            continue;
        };
        let (Some(op), Some(lit)) = (tokens.get(var_end), tokens.get(var_end + 1)) else {
            break;
        };
        if op.kind == TokKind::Punct
            && OPERATORS.contains(&op.text)
            && is_literal(lit)
            && ends_operand(tokens.get(var_end + 2))
        {
            let left = &src[tokens[i].start..tokens[var_end - 1].end];
            out.push(YodaHint {
                start: tokens[i].start,
                end: lit.end,
                message: format!(
                    "Use Yoda condition: put {} on the left of '{}' (WordPress Coding Standards)",
                    lit.text, op.text
                ),
                replacement: format!("{} {} {}", lit.text, op.text, left),
            });
            i = var_end + 2;
            continue;
        }
        i = var_end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_comparisons_with_the_literal_on_the_right() {
        let src = "<?php\nif ( $status === 'publish' && $post->ID != 0 ) {}\nif ( true === $ok ) {}\nwhile ( $a['k'] == null ) {}\n";
        let h = hints(src);
        let found: Vec<&str> = h.iter().map(|x| &src[x.start..x.end]).collect();
        assert_eq!(
            found,
            vec!["$status === 'publish'", "$post->ID != 0", "$a['k'] == null"]
        );
        assert_eq!(h[0].replacement, "'publish' === $status");
        assert_eq!(h[1].replacement, "0 != $post->ID");
        assert!(h[0].message.starts_with("Use Yoda condition"));
    }

    #[test]
    fn skips_complex_operands() {
        let src = "<?php\nif ( $a . $b == 'x' ) {}\nif ( $a == 'x' . $b ) {}\nif ( $o->get() === 1 ) {}\n$x = $a === $b;\n// $c == 'x'\n$y = $count > 1;\n";
        assert!(hints(src).is_empty(), "{:?}", hints(src));
    }

    #[test]
    fn checks_assignments_and_returns() {
        let src = "<?php\n$is = $type === 'page';\nreturn $n !== 3;";
        let found: Vec<String> = hints(src).into_iter().map(|h| h.replacement).collect();
        assert_eq!(found, vec!["'page' === $type", "3 !== $n"]);
    }
}
