//! The tests of a JavaScript / TypeScript file: `describe` / `it` / `test` calls (and their
//! `.only`, `.skip`, `.each`, `test.describe` forms) whose name is a string literal, nested
//! by the call they are written in rather than by indentation.
//!
//! A small scanner skips comments, strings, template literals and regular expressions, so
//! braces and parentheses inside them do not count.

use forge_testing::TargetKind;

/// A `describe` or test call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// 0-based line of the call.
    pub line: u32,
    /// [`TargetKind::Describe`] or [`TargetKind::Test`].
    pub kind: TargetKind,
    pub name: String,
    /// The names of the enclosing suites and its own, joined by a space (as Jest, Vitest,
    /// Mocha and `node --test` report them).
    pub full_name: String,
}

const SUITES: &[&str] = &["describe", "context", "suite", "fdescribe", "xdescribe"];
const TESTS: &[&str] = &["it", "test", "specify", "fit", "xit", "xtest"];
const MODIFIERS: &[&str] = &[
    "only",
    "skip",
    "todo",
    "concurrent",
    "serial",
    "parallel",
    "fixme",
    "fail",
    "failing",
    "slow",
    "sequential",
    "shuffle",
    "each",
    "skipIf",
    "runIf",
];

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    /// A template literal without substitutions has a fixed name; one with `${…}` has not.
    Template(Option<String>),
    Punct(char),
    Other,
}

struct Token {
    tok: Tok,
    line: u32,
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Tokens of the source, without comments and whitespace.
fn tokens(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<Token> = Vec::new();
    let mut i = 0;
    let mut line = 0u32;
    // Whether a `/` here starts a regular expression (after an operator or keyword).
    let regex_allowed = |out: &Vec<Token>| match out.last().map(|t| &t.tok) {
        None => true,
        Some(Tok::Punct(c)) => !matches!(c, ')' | ']' | '}'),
        Some(Tok::Ident(w)) => matches!(
            w.as_str(),
            "return"
                | "typeof"
                | "case"
                | "do"
                | "else"
                | "in"
                | "of"
                | "new"
                | "delete"
                | "void"
                | "throw"
                | "yield"
                | "await"
        ),
        _ => false,
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\n' => {
                line += 1;
                i += 1;
            }
            c if c.is_whitespace() => i += 1,
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i += 2;
            }
            '/' if regex_allowed(&out) => {
                let start = line;
                i += 1;
                let mut class = false;
                while i < chars.len() && chars[i] != '\n' {
                    match chars[i] {
                        '\\' => i += 1,
                        '[' => class = true,
                        ']' => class = false,
                        '/' if !class => break,
                        _ => {}
                    }
                    i += 1;
                }
                i += 1;
                while i < chars.len() && is_ident(chars[i]) {
                    i += 1;
                }
                out.push(Token {
                    tok: Tok::Other,
                    line: start,
                });
            }
            '"' | '\'' => {
                let start = line;
                let quote = c;
                let mut s = String::new();
                i += 1;
                while i < chars.len() && chars[i] != quote && chars[i] != '\n' {
                    if chars[i] == '\\' {
                        i += 1;
                        match chars.get(i) {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(&other) => s.push(other),
                            None => {}
                        }
                    } else {
                        s.push(chars[i]);
                    }
                    i += 1;
                }
                i += 1;
                out.push(Token {
                    tok: Tok::Str(s),
                    line: start,
                });
            }
            '`' => {
                let start = line;
                let mut s = String::new();
                let mut fixed = true;
                i += 1;
                while i < chars.len() && chars[i] != '`' {
                    match chars[i] {
                        '\\' => {
                            i += 1;
                            if let Some(&n) = chars.get(i) {
                                s.push(n);
                            }
                        }
                        '$' if chars.get(i + 1) == Some(&'{') => {
                            fixed = false;
                            // Skip the substitution, counting nested braces.
                            let mut depth = 0;
                            while i < chars.len() {
                                match chars[i] {
                                    '{' => depth += 1,
                                    '}' => {
                                        depth -= 1;
                                        if depth == 0 {
                                            break;
                                        }
                                    }
                                    '\n' => line += 1,
                                    _ => {}
                                }
                                i += 1;
                            }
                        }
                        '\n' => {
                            line += 1;
                            s.push('\n');
                        }
                        other => s.push(other),
                    }
                    i += 1;
                }
                i += 1;
                out.push(Token {
                    tok: Tok::Template(fixed.then_some(s)),
                    line: start,
                });
            }
            c if is_ident_start(c) => {
                let mut s = String::new();
                while i < chars.len() && is_ident(chars[i]) {
                    s.push(chars[i]);
                    i += 1;
                }
                out.push(Token {
                    tok: Tok::Ident(s),
                    line,
                });
            }
            c if c.is_ascii_digit() => {
                while i < chars.len() && (is_ident(chars[i]) || chars[i] == '.') {
                    i += 1;
                }
                out.push(Token {
                    tok: Tok::Other,
                    line,
                });
            }
            c => {
                out.push(Token {
                    tok: Tok::Punct(c),
                    line,
                });
                i += 1;
            }
        }
    }
    out
}

/// The index after the parenthesis that closes the one at `open`.
fn skip_parens(toks: &[Token], open: usize) -> usize {
    let mut depth = 0;
    let mut i = open;
    while i < toks.len() {
        match toks[i].tok {
            Tok::Punct('(') => depth += 1,
            Tok::Punct(')') => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    toks.len()
}

/// The `describe` and test calls of a file, in source order.
pub fn discover(text: &str) -> Vec<Found> {
    let toks = tokens(text);
    let mut found = Vec::new();
    // Open suites: (paren depth at which the suite's call closes, name).
    let mut open: Vec<(i32, String)> = Vec::new();
    let mut depth: i32 = 0;
    let mut i = 0;
    while i < toks.len() {
        match &toks[i].tok {
            Tok::Punct('(') => depth += 1,
            Tok::Punct(')') => {
                depth -= 1;
                while open.last().is_some_and(|(d, _)| *d > depth) {
                    open.pop();
                }
            }
            Tok::Ident(head) => {
                let after_dot = i > 0 && toks[i - 1].tok == Tok::Punct('.');
                if after_dot {
                    i += 1;
                    continue;
                }
                // The member chain: `test.describe.only`, `it.each`.
                let mut chain = vec![head.clone()];
                let mut j = i + 1;
                while j + 1 < toks.len() && toks[j].tok == Tok::Punct('.') {
                    match &toks[j + 1].tok {
                        Tok::Ident(n) => chain.push(n.clone()),
                        _ => break,
                    }
                    j += 2;
                }
                if let Some((kind, call)) = classify(&chain) {
                    // `.each(table)(name, …)` and `.skipIf(cond)(name, …)`: the name is in
                    // the second call.
                    let mut k = j;
                    if call && toks.get(k).map(|t| &t.tok) == Some(&Tok::Punct('(')) {
                        k = skip_parens(&toks, k);
                    }
                    if toks.get(k).map(|t| &t.tok) == Some(&Tok::Punct('(')) {
                        let name = match toks.get(k + 1).map(|t| &t.tok) {
                            Some(Tok::Str(s)) | Some(Tok::Template(Some(s))) => Some(s.clone()),
                            _ => None,
                        };
                        if let Some(name) = name.filter(|n| !n.trim().is_empty()) {
                            let full_name = open
                                .iter()
                                .map(|(_, n)| n.as_str())
                                .chain([name.as_str()])
                                .collect::<Vec<_>>()
                                .join(" ");
                            found.push(Found {
                                line: toks[i].line,
                                kind,
                                name: name.clone(),
                                full_name,
                            });
                            if kind == TargetKind::Describe {
                                // The call's parenthesis is opened at `k`; the suite ends
                                // when it closes.
                                let opened_at = depth
                                    + toks[j..k]
                                        .iter()
                                        .map(|t| match t.tok {
                                            Tok::Punct('(') => 1,
                                            Tok::Punct(')') => -1,
                                            _ => 0,
                                        })
                                        .sum::<i32>()
                                    + 1;
                                open.push((opened_at, name));
                            }
                        }
                    }
                }
                // Continue after the chain; parentheses are counted as usual.
                i = j;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    found
}

/// The kind of a call chain, and whether its first call only takes options
/// (`.each(table)`, `.skipIf(cond)`) before the call with the name.
fn classify(chain: &[String]) -> Option<(TargetKind, bool)> {
    let (head, rest) = chain.split_first()?;
    let (kind, rest) = if SUITES.contains(&head.as_str()) {
        (TargetKind::Describe, rest)
    } else if TESTS.contains(&head.as_str()) {
        // `test.describe(…)` is Playwright's suite.
        match rest.first().map(String::as_str) {
            Some("describe") => (TargetKind::Describe, &rest[1..]),
            _ => (TargetKind::Test, rest),
        }
    } else {
        return None;
    };
    if !rest.iter().all(|m| MODIFIERS.contains(&m.as_str())) {
        return None;
    }
    let call = rest
        .last()
        .is_some_and(|m| matches!(m.as_str(), "each" | "skipIf" | "runIf"));
    Some((kind, call))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(text: &str) -> Vec<(u32, TargetKind, String, String)> {
        discover(text)
            .into_iter()
            .map(|f| (f.line, f.kind, f.name, f.full_name))
            .collect()
    }

    #[test]
    fn nests_by_the_enclosing_call() {
        let text = r#"import { describe, it, expect } from "vitest";

describe("Cart", () => {
  it("adds items", () => {
    expect(add(1)).toBe(1); // it("not a test")
  });
  describe('totals', () => { it(`rounds to cents`, () => {}) });
  test.each([[1, 2]])("adds %i", (a) => {});
});
it("stands alone", async () => { const s = "describe(\"x\""; const r = /it\(/g; });
"#;
        assert_eq!(
            rows(text),
            [
                (2, TargetKind::Describe, "Cart".into(), "Cart".into()),
                (
                    3,
                    TargetKind::Test,
                    "adds items".into(),
                    "Cart adds items".into()
                ),
                (
                    6,
                    TargetKind::Describe,
                    "totals".into(),
                    "Cart totals".into()
                ),
                (
                    6,
                    TargetKind::Test,
                    "rounds to cents".into(),
                    "Cart totals rounds to cents".into()
                ),
                (7, TargetKind::Test, "adds %i".into(), "Cart adds %i".into()),
                (
                    9,
                    TargetKind::Test,
                    "stands alone".into(),
                    "stands alone".into()
                ),
            ]
        );
    }

    #[test]
    fn playwright_and_modifiers() {
        let text = r#"import { test, expect } from '@playwright/test';

test.describe.serial('Checkout', () => {
  test.beforeEach(async ({ page }) => { await page.goto('/'); });
  test('adds the voucher', async ({ page }) => {
    await test.step('open', async () => {});
  });
  test.skip('keeps the cart', async () => {});
  test.describe.configure({ mode: 'parallel' });
});
foo.it('no', () => {});
it(name, () => {});
it(`dynamic ${x}`, () => {});
/* describe("commented", () => {}) */
"#;
        assert_eq!(
            rows(text),
            [
                (
                    2,
                    TargetKind::Describe,
                    "Checkout".into(),
                    "Checkout".into()
                ),
                (
                    4,
                    TargetKind::Test,
                    "adds the voucher".into(),
                    "Checkout adds the voucher".into()
                ),
                (
                    7,
                    TargetKind::Test,
                    "keeps the cart".into(),
                    "Checkout keeps the cart".into()
                ),
            ]
        );
    }

    #[test]
    fn mocha_context_and_function_callbacks() {
        let text = "describe('Api', function () {\n  context('when logged in', function () {\n    specify('lists orders', function (done) { done(); });\n  });\n  it.only('pings', () => {});\n});\n";
        let names: Vec<String> = discover(text).into_iter().map(|f| f.full_name).collect();
        assert_eq!(
            names,
            [
                "Api",
                "Api when logged in",
                "Api when logged in lists orders",
                "Api pings"
            ]
        );
    }
}
