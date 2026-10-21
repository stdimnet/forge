//! Unwrap/Remove (Ctrl+Shift+Delete): the statements around the caret and how to take them
//! apart, innermost first, as PHPStorm's popup lists them ("Unwrap 'if...'",
//! "Remove 'else...'", "Unwrap braces").
//!
//! PHP and JavaScript/TypeScript statements are found through braces (`if (…) {…} else {…}`,
//! loops, `try`/`catch`/`finally`, `do … while`, plain blocks); Lua through its keywords
//! (`if … then … elseif … else … end`, `for`/`while … do … end`, `repeat … until`, `do … end`).

use serde::{Deserialize, Serialize};

use crate::context::Lang;
use crate::lex::{lex, match_brackets, Kind, Syntax, Tok};
use crate::text::{byte_of, dedent, line_end, line_indent, line_start, utf16_of};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnwrapRequest {
    pub text: String,
    /// Caret (UTF-16).
    pub offset: usize,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub language_id: Option<String>,
}

/// One text change (UTF-16 offsets into the document).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Edit {
    pub from: usize,
    pub to: usize,
    pub text: String,
}

/// An entry of the Unwrap/Remove popup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnwrapOption {
    pub label: String,
    /// The statement (or removed branch) the entry acts on, highlighted while it is selected.
    pub from: usize,
    pub to: usize,
    /// The part that stays (unwrap), not set when removing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep: Option<(usize, usize)>,
    pub edits: Vec<Edit>,
}

/// A branch of a statement: header start, body (between the braces or keywords) and end.
#[derive(Debug, Clone, Copy)]
struct Branch {
    kind: &'static str,
    start: usize,
    body: (usize, usize),
    end: usize,
}

#[derive(Debug, Clone)]
struct Stmt {
    start: usize,
    end: usize,
    branches: Vec<Branch>,
}

/// Unwrap and remove options for the caret, innermost statement first.
pub fn unwrap_options(req: &UnwrapRequest) -> Vec<UnwrapOption> {
    let text = req.text.as_str();
    let at = byte_of(text, req.offset);
    let stmts = match Lang::of(req.language_id.as_deref(), &req.path) {
        Lang::Php => brace_statements(text, Syntax::Php),
        Lang::JavaScript | Lang::TypeScript => brace_statements(text, Syntax::Js),
        Lang::Lua => lua_statements(text),
        _ => return Vec::new(),
    };
    let mut around: Vec<(&Stmt, usize)> = stmts
        .iter()
        .filter_map(|s| {
            s.branches
                .iter()
                .position(|b| b.body.0 <= at && at <= b.body.1)
                .map(|i| (s, i))
        })
        .collect();
    around.sort_by_key(|(s, _)| std::cmp::Reverse(s.start));
    let mut out = Vec::new();
    for (s, i) in around {
        let b = s.branches[i];
        let unwrap = |label: String| UnwrapOption {
            label,
            from: utf16_of(text, s.start),
            to: utf16_of(text, s.end),
            keep: Some((utf16_of(text, b.body.0), utf16_of(text, b.body.1))),
            edits: vec![replace(
                text,
                s.start,
                s.end,
                &unwrapped(text, s.start, b.body),
            )],
        };
        match b.kind {
            "else" | "elseif" | "else if" | "catch" | "finally" => {
                if b.kind != "catch" {
                    out.push(unwrap(format!("Unwrap '{}...'", b.kind)));
                }
                let prev_end = s.branches[i - 1].end;
                let (from, to) = removed_range(text, prev_end, b);
                out.push(UnwrapOption {
                    label: format!("Remove '{}...'", b.kind),
                    from: utf16_of(text, from),
                    to: utf16_of(text, to),
                    keep: None,
                    edits: vec![Edit {
                        from: utf16_of(text, from),
                        to: utf16_of(text, to),
                        text: String::new(),
                    }],
                });
            }
            "braces" => out.push(unwrap("Unwrap braces".into())),
            kind => out.push(unwrap(format!("Unwrap '{kind}...'"))),
        }
    }
    out
}

fn replace(text: &str, from: usize, to: usize, with: &str) -> Edit {
    // An empty statement on its own lines disappears with them.
    let (mut from, mut to) = (from, to);
    if with.is_empty() {
        let ls = line_start(text, from);
        let le = line_end(text, to);
        if text[ls..from].trim().is_empty() && text[to..le].trim().is_empty() {
            from = ls;
            to = (le + 1).min(text.len());
        }
    }
    Edit {
        from: utf16_of(text, from),
        to: utf16_of(text, to),
        text: with.to_string(),
    }
}

/// The body of a branch, re-indented to the statement's indentation.
fn unwrapped(text: &str, stmt_start: usize, body: (usize, usize)) -> String {
    let inner = &text[body.0..body.1];
    let mut lines: Vec<&str> = inner.split('\n').collect();
    if lines.len() == 1 {
        return inner.trim().to_string();
    }
    if lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    if lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    let indent = line_indent(text, stmt_start);
    dedent(&lines)
        .iter()
        .enumerate()
        .map(|(i, l)| {
            if i == 0 || l.is_empty() {
                l.clone()
            } else {
                format!("{indent}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What "Remove" deletes: from the end of the previous branch to the end of this one; for
/// keyword languages whole lines when the branch starts its own line.
fn removed_range(text: &str, prev_end: usize, b: Branch) -> (usize, usize) {
    if prev_end < b.start && text[prev_end..b.start].trim().is_empty() {
        // `} else {…}`: keep the closing brace of the previous branch.
        return (prev_end, b.end);
    }
    let ls = line_start(text, b.start);
    if text[ls..b.start].trim().is_empty() {
        let el = line_start(text, b.end);
        if text[el..b.end].trim().is_empty() {
            return (ls, el);
        }
    }
    (b.start, b.end)
}

fn sig_tokens(text: &str, syntax: Syntax) -> Vec<Tok> {
    lex(text, syntax)
        .into_iter()
        .filter(|t| t.significant())
        .collect()
}

/// Identifier or punctuation of token `i` ("" for others and past the end).
fn word_at<'a>(text: &'a str, toks: &[Tok], i: usize) -> &'a str {
    match toks.get(i) {
        Some(t) if t.kind == Kind::Ident || t.kind == Kind::Punct => t.text(text),
        _ => "",
    }
}

/// Statements of brace languages (PHP, JavaScript).
fn brace_statements(text: &str, syntax: Syntax) -> Vec<Stmt> {
    let toks = sig_tokens(text, syntax);
    let partner = match_brackets(text, &toks);
    let t = |i: usize| word_at(text, &toks, i);
    // `( … ) { … }` starting at token `i`: (header end token, open brace, close brace).
    let cond_block = |i: usize| -> Option<(usize, usize)> {
        if t(i) != "(" {
            return None;
        }
        let close = partner[i]?;
        if t(close + 1) != "{" {
            return None;
        }
        Some((close + 1, partner[close + 1]?))
    };
    let block = |i: usize| -> Option<(usize, usize)> { (t(i) == "{").then_some((i, partner[i]?)) };
    let body = |open: usize, close: usize| (toks[open].end, toks[close].start);
    let mut stmts = Vec::new();
    let mut skip_while: Vec<usize> = Vec::new();
    for i in 0..toks.len() {
        let prev = if i > 0 { t(i - 1) } else { "" };
        if matches!(prev, "->" | "::" | "?->" | "." | "function") {
            continue;
        }
        let branch = |kind: &'static str, start: usize, (open, close): (usize, usize)| Branch {
            kind,
            start: toks[start].start,
            body: body(open, close),
            end: toks[close].end,
        };
        let mut branches = Vec::new();
        match t(i) {
            "if" if prev != "else" => {
                let Some(b) = cond_block(i + 1) else { continue };
                branches.push(branch("if", i, b));
                let mut next = b.1 + 1;
                loop {
                    if t(next) == "elseif" {
                        let Some(b) = cond_block(next + 1) else { break };
                        branches.push(branch("elseif", next, b));
                        next = b.1 + 1;
                    } else if t(next) == "else" && t(next + 1) == "if" {
                        let Some(b) = cond_block(next + 2) else { break };
                        branches.push(branch("else if", next, b));
                        next = b.1 + 1;
                    } else if t(next) == "else" {
                        let Some(b) = block(next + 1) else { break };
                        branches.push(branch("else", next, b));
                        break;
                    } else {
                        break;
                    }
                }
            }
            kw @ ("for" | "foreach" | "while") if !skip_while.contains(&i) => {
                let Some(b) = cond_block(i + 1) else { continue };
                let kind = match kw {
                    "for" => "for",
                    "foreach" => "foreach",
                    _ => "while",
                };
                branches.push(branch(kind, i, b));
            }
            "do" => {
                let Some(b) = block(i + 1) else { continue };
                if t(b.1 + 1) != "while" || t(b.1 + 2) != "(" {
                    continue;
                }
                let Some(close) = partner[b.1 + 2] else {
                    continue;
                };
                skip_while.push(b.1 + 1);
                let mut br = branch("do", i, b);
                let end = if t(close + 1) == ";" {
                    close + 1
                } else {
                    close
                };
                br.end = toks[end].end;
                branches.push(br);
            }
            "try" => {
                let Some(b) = block(i + 1) else { continue };
                branches.push(branch("try", i, b));
                let mut next = b.1 + 1;
                loop {
                    if t(next) == "catch" {
                        // JavaScript allows `catch {` without a binding.
                        let b = cond_block(next + 1).or_else(|| block(next + 1));
                        let Some(b) = b else { break };
                        branches.push(branch("catch", next, b));
                        next = b.1 + 1;
                    } else if t(next) == "finally" {
                        let Some(b) = block(next + 1) else { break };
                        branches.push(branch("finally", next, b));
                        break;
                    } else {
                        break;
                    }
                }
            }
            "{" if matches!(prev, "" | ";" | "{" | "}")
                || (i > 0 && toks[i - 1].kind == Kind::OpenTag) =>
            {
                let Some(b) = block(i) else { continue };
                branches.push(branch("braces", i, b));
            }
            _ => continue,
        }
        if let (Some(first), Some(last)) = (branches.first(), branches.last()) {
            stmts.push(Stmt {
                start: first.start,
                end: last.end,
                branches,
            });
        }
    }
    stmts
}

/// Statements of Lua, from its block keywords.
fn lua_statements(text: &str) -> Vec<Stmt> {
    let toks = sig_tokens(text, Syntax::Lua);
    let word = |t: &Tok| {
        if t.kind == Kind::Ident {
            t.text(text)
        } else {
            ""
        }
    };
    struct Open {
        kind: &'static str,
        start: usize,
        branches: Vec<Branch>,
        /// Loop header waiting for its `do`.
        awaiting_do: bool,
    }
    let mut stack: Vec<Open> = Vec::new();
    let mut stmts = Vec::new();
    let close_branch = |o: &mut Open, at: usize| {
        if let Some(b) = o.branches.last_mut() {
            b.body.1 = at;
            b.end = at;
        }
    };
    for (i, tok) in toks.iter().enumerate() {
        let w = word(tok);
        let open_branch = |kind: &'static str, start: usize| Branch {
            kind,
            start,
            body: (tok.end, tok.end),
            end: tok.end,
        };
        match w {
            "if" => stack.push(Open {
                kind: "if",
                start: tok.start,
                branches: Vec::new(),
                awaiting_do: false,
            }),
            "then" => {
                if let Some(o) = stack.last_mut().filter(|o| o.kind == "if") {
                    let start = o.branches.last().map_or(o.start, |b| b.end);
                    let kind = if o.branches.is_empty() {
                        "if"
                    } else {
                        "elseif"
                    };
                    let header = if kind == "if" { o.start } else { start };
                    o.branches.push(Branch {
                        kind,
                        start: header,
                        body: (tok.end, tok.end),
                        end: tok.end,
                    });
                }
            }
            "elseif" | "else" => {
                if let Some(o) = stack.last_mut().filter(|o| o.kind == "if") {
                    close_branch(o, tok.start);
                    // The `then` of an `elseif` opens its branch.
                    if w == "else" {
                        o.branches.push(open_branch("else", tok.start));
                    }
                }
            }
            "for" | "while" => stack.push(Open {
                kind: if w == "for" { "for" } else { "while" },
                start: tok.start,
                branches: Vec::new(),
                awaiting_do: true,
            }),
            "do" => match stack.last_mut().filter(|o| o.awaiting_do) {
                Some(o) => {
                    o.awaiting_do = false;
                    let start = o.start;
                    let kind = o.kind;
                    o.branches.push(Branch {
                        kind,
                        start,
                        body: (tok.end, tok.end),
                        end: tok.end,
                    });
                }
                None => stack.push(Open {
                    kind: "do",
                    start: tok.start,
                    branches: vec![open_branch("do", tok.start)],
                    awaiting_do: false,
                }),
            },
            "repeat" => stack.push(Open {
                kind: "repeat",
                start: tok.start,
                branches: vec![open_branch("repeat", tok.start)],
                awaiting_do: false,
            }),
            "function" => stack.push(Open {
                kind: "function",
                start: tok.start,
                branches: Vec::new(),
                awaiting_do: false,
            }),
            "end" | "until" => {
                let Some(mut o) = stack.pop() else { continue };
                close_branch(&mut o, tok.start);
                let end = if w == "until" {
                    line_end(text, toks.get(i + 1).map_or(tok.end, |n| n.end))
                } else {
                    tok.end
                };
                // Each branch ends where the next keyword (or `end`/`until`) starts.
                if o.kind != "function" && !o.branches.is_empty() {
                    stmts.push(Stmt {
                        start: o.start,
                        end,
                        branches: o.branches,
                    });
                }
            }
            _ => {}
        }
    }
    stmts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(src: &str, lang: &str) -> (String, Vec<UnwrapOption>) {
        let offset = src.find('|').unwrap();
        let text = src.replacen('|', "", 1);
        let opts = unwrap_options(&UnwrapRequest {
            text: text.clone(),
            offset,
            path: String::new(),
            language_id: Some(lang.into()),
        });
        (text, opts)
    }

    fn apply(text: &str, o: &UnwrapOption) -> String {
        let mut out = text.to_string();
        for e in o.edits.iter().rev() {
            out.replace_range(byte_of(text, e.from)..byte_of(text, e.to), &e.text);
        }
        out
    }

    fn labels(o: &[UnwrapOption]) -> Vec<&str> {
        o.iter().map(|o| o.label.as_str()).collect()
    }

    #[test]
    fn unwraps_php_if_and_loops() {
        let src = "<?php\nfunction f($items) {\n    foreach ($items as $i) {\n        if ($i) {\n            a(|);\n            b();\n        } else {\n            c();\n        }\n    }\n}";
        let (text, o) = options(src, "php");
        assert_eq!(labels(&o), ["Unwrap 'if...'", "Unwrap 'foreach...'"]);
        assert_eq!(
            apply(&text, &o[0]),
            "<?php\nfunction f($items) {\n    foreach ($items as $i) {\n        a();\n        b();\n    }\n}"
        );
        assert_eq!(
            apply(&text, &o[1]),
            "<?php\nfunction f($items) {\n    if ($i) {\n        a();\n        b();\n    } else {\n        c();\n    }\n}"
        );
    }

    #[test]
    fn unwraps_or_removes_else_branches() {
        let src = "<?php\nif ($a) {\n    a();\n} elseif ($b) {\n    b();\n} else {\n    c(|);\n}";
        let (text, o) = options(src, "php");
        assert_eq!(labels(&o), ["Unwrap 'else...'", "Remove 'else...'"]);
        assert_eq!(apply(&text, &o[0]), "<?php\nc();");
        assert_eq!(
            apply(&text, &o[1]),
            "<?php\nif ($a) {\n    a();\n} elseif ($b) {\n    b();\n}"
        );
        let (text, o) = options(
            "<?php\nif ($a) {\n    a();\n} elseif ($b) {\n    |b();\n}",
            "php",
        );
        assert_eq!(labels(&o), ["Unwrap 'elseif...'", "Remove 'elseif...'"]);
        assert_eq!(apply(&text, &o[1]), "<?php\nif ($a) {\n    a();\n}");
    }

    #[test]
    fn handles_try_catch_do_while_and_braces() {
        let src = "try {\n    run();\n} catch (e) {\n    log(|e);\n} finally {\n    done();\n}";
        let (text, o) = options(src, "javascript");
        assert_eq!(labels(&o), ["Remove 'catch...'"]);
        assert_eq!(
            apply(&text, &o[0]),
            "try {\n    run();\n} finally {\n    done();\n}"
        );
        let (text, o) = options("try {\n    ru|n();\n} catch (e) {}", "typescript");
        assert_eq!(labels(&o), ["Unwrap 'try...'"]);
        assert_eq!(apply(&text, &o[0]), "run();");
        let (text, o) = options("do {\n    x|++;\n} while (x < 3);\nnext();", "javascript");
        assert_eq!(labels(&o), ["Unwrap 'do...'"]);
        assert_eq!(apply(&text, &o[0]), "x++;\nnext();");
        let (text, o) = options("a();\n{\n    b|();\n}", "javascript");
        assert_eq!(labels(&o), ["Unwrap braces"]);
        assert_eq!(apply(&text, &o[0]), "a();\nb();");
        // Object literals and function bodies are not statements.
        assert!(options("const o = {\n    a: |1,\n};", "javascript")
            .1
            .is_empty());
        assert!(options("function f() {\n    |\n}", "javascript")
            .1
            .is_empty());
        assert!(options("<?php class A { function f() { | } }", "php")
            .1
            .is_empty());
    }

    #[test]
    fn removes_empty_statements_with_their_lines() {
        let (text, o) = options("<?php\nif ($a) {|}\nb();", "php");
        assert_eq!(apply(&text, &o[0]), "<?php\nb();");
    }

    #[test]
    fn unwraps_lua_blocks() {
        let src = "for i = 1, 3 do\n    if i > 1 then\n        print(|i)\n    else\n        skip()\n    end\nend";
        let (text, o) = options(src, "lua");
        assert_eq!(labels(&o), ["Unwrap 'if...'", "Unwrap 'for...'"]);
        assert_eq!(apply(&text, &o[0]), "for i = 1, 3 do\n    print(i)\nend");
        let (text, o) = options("if a then\n    x()\nelse\n    y(|)\nend", "lua");
        assert_eq!(labels(&o), ["Unwrap 'else...'", "Remove 'else...'"]);
        assert_eq!(apply(&text, &o[0]), "y()");
        assert_eq!(apply(&text, &o[1]), "if a then\n    x()\nend");
        let (text, o) = options("repeat\n    n = n - |1\nuntil n == 0\nprint(n)", "lua");
        assert_eq!(labels(&o), ["Unwrap 'repeat...'"]);
        assert_eq!(apply(&text, &o[0]), "n = n - 1\nprint(n)");
        assert!(options("local function f()\n    |\nend", "lua")
            .1
            .is_empty());
    }
}
