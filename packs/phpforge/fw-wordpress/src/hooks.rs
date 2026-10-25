//! WordPress hooks in PHP source: registrations (`add_action`, `add_filter`), invocations
//! (`do_action`, `apply_filters` and their `_ref_array` / `_deprecated` variants), removals
//! and checks (`remove_filter`, `has_action`, `did_action`), plus the function and method
//! declarations that callbacks name.
//!
//! Hook names are the first argument. A literal (`'init'`) gives a static name; a dynamic
//! name (`"save_post_{$post->post_type}"`, `'manage_' . $screen . '_columns'`) becomes a
//! pattern whose `{…}` parts match any text, so `add_action('save_post_book', …)` finds the
//! `do_action("save_post_{$post->post_type}")` that runs it.

use serde::{Deserialize, Serialize};

use crate::lexer::{self, string_content, string_value, TokKind, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HookKind {
    Action,
    Filter,
}

impl HookKind {
    pub fn label(self) -> &'static str {
        match self {
            HookKind::Action => "action",
            HookKind::Filter => "filter",
        }
    }
}

/// What a call does with its hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HookRole {
    /// `add_action`, `add_filter`: a callback runs when the hook fires.
    Registration,
    /// `do_action`, `apply_filters`: the hook fires here.
    Invocation,
    /// `remove_action`, `remove_filter`, `remove_all_*`.
    Removal,
    /// `has_action`, `did_action`, `doing_filter`, …
    Check,
}

/// The WordPress functions that take a hook name first: (name, kind, role).
pub const HOOK_FUNCTIONS: &[(&str, HookKind, HookRole)] = &[
    ("add_action", HookKind::Action, HookRole::Registration),
    ("add_filter", HookKind::Filter, HookRole::Registration),
    ("do_action", HookKind::Action, HookRole::Invocation),
    (
        "do_action_ref_array",
        HookKind::Action,
        HookRole::Invocation,
    ),
    (
        "do_action_deprecated",
        HookKind::Action,
        HookRole::Invocation,
    ),
    ("apply_filters", HookKind::Filter, HookRole::Invocation),
    (
        "apply_filters_ref_array",
        HookKind::Filter,
        HookRole::Invocation,
    ),
    (
        "apply_filters_deprecated",
        HookKind::Filter,
        HookRole::Invocation,
    ),
    ("remove_action", HookKind::Action, HookRole::Removal),
    ("remove_filter", HookKind::Filter, HookRole::Removal),
    ("remove_all_actions", HookKind::Action, HookRole::Removal),
    ("remove_all_filters", HookKind::Filter, HookRole::Removal),
    ("has_action", HookKind::Action, HookRole::Check),
    ("has_filter", HookKind::Filter, HookRole::Check),
    ("did_action", HookKind::Action, HookRole::Check),
    ("did_filter", HookKind::Filter, HookRole::Check),
    ("doing_action", HookKind::Action, HookRole::Check),
    ("doing_filter", HookKind::Filter, HookRole::Check),
];

/// The hook function called `name` (case-insensitive, a leading `\` allowed).
pub fn hook_function(name: &str) -> Option<(HookKind, HookRole)> {
    let name = name.trim_start_matches('\\');
    HOOK_FUNCTIONS
        .iter()
        .find(|(f, ..)| f.eq_ignore_ascii_case(name))
        .map(|&(_, k, r)| (k, r))
}

/// One hook name in a call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookRef {
    /// `init`, or a pattern like `save_post_{$post->post_type}`.
    pub name: String,
    pub kind: HookKind,
    pub role: HookRole,
    /// The called function, `add_action`.
    pub function: String,
    /// Project-relative path with `/` separators.
    pub path: String,
    /// 0-based line and byte column of the name (inside the quotes).
    pub line: u32,
    pub column: u32,
    /// Byte range of the name in the file.
    pub start: usize,
    pub end: usize,
    /// Whether the name is a pattern (built from variables).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dynamic: bool,
    /// The callback as written (`'my_init'`, `[ $this, 'boot' ]`, `closure`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback: Option<String>,
    /// The function or method name the callback names, with its byte range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_range: Option<(usize, usize)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    /// The source line, leading whitespace removed (for usage lists).
    pub text: String,
    /// Byte column of `text` in the line.
    pub indent: u32,
}

/// A function or method declaration (callback targets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDecl {
    pub name: String,
    /// The class for methods.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    pub path: String,
    pub line: u32,
    pub column: u32,
}

/// Is the token at `i` a call of a global function (not a method, a declaration or `new`)?
fn is_global_call(tokens: &[Token<'_>], i: usize) -> bool {
    if !tokens.get(i + 1).is_some_and(|t| t.is("(")) {
        return false;
    }
    match i.checked_sub(1).map(|p| &tokens[p]) {
        Some(p) if p.is("->") || p.is("?->") || p.is("::") => false,
        Some(p) if p.is_ident("function") || p.is_ident("new") || p.is_ident("fn") => false,
        _ => true,
    }
}

/// The hook name of an argument: `(name, dynamic, byte range of the name)`.
fn name_of(
    tokens: &[Token<'_>],
    range: std::ops::Range<usize>,
) -> Option<(String, bool, usize, usize)> {
    let toks = &tokens[range];
    let first = toks.first()?;
    if toks.len() == 1 && first.kind == TokKind::Str {
        let (value, interpolated) = string_value(first.text);
        let (s, e) = string_content(first);
        let (value, dynamic) = if interpolated {
            (normalize_interpolation(&value), true)
        } else {
            (value, false)
        };
        return Some((value, dynamic, s, e));
    }
    // A concatenation: 'manage_' . $screen . '_columns'.
    if !toks.iter().any(|t| t.is(".")) {
        return None;
    }
    let mut name = String::new();
    let mut part = String::new();
    let flush = |name: &mut String, part: &mut String| {
        if !part.is_empty() {
            name.push('{');
            name.push_str(part.trim());
            name.push('}');
            part.clear();
        }
    };
    for t in toks {
        if t.is(".") {
            flush(&mut name, &mut part);
        } else if t.kind == TokKind::Str && part.is_empty() {
            let (v, interpolated) = string_value(t.text);
            name.push_str(&if interpolated {
                normalize_interpolation(&v)
            } else {
                v
            });
        } else {
            part.push_str(t.text);
        }
    }
    flush(&mut name, &mut part);
    let s = if first.kind == TokKind::Str {
        string_content(first).0
    } else {
        first.start
    };
    let last = toks.last()?;
    let e = if last.kind == TokKind::Str {
        string_content(last).1
    } else {
        last.end
    };
    Some((name, true, s, e))
}

/// `save_post_$type` → `save_post_{$type}` (one placeholder form for patterns).
fn normalize_interpolation(v: &str) -> String {
    let mut out = String::new();
    let mut chars = v.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c == '{' && chars.peek().is_some_and(|&(_, d)| d == '$') {
            out.push('{');
            let mut depth = 1;
            for (_, d) in chars.by_ref() {
                match d {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
                if depth == 0 {
                    break;
                }
                out.push(d);
            }
            out.push('}');
        } else if c == '$'
            && chars
                .peek()
                .is_some_and(|&(_, d)| d.is_alphabetic() || d == '_')
        {
            out.push_str("{$");
            while let Some(&(_, d)) = chars.peek() {
                if d.is_alphanumeric() || d == '_' {
                    out.push(d);
                    chars.next();
                } else if d == '-' {
                    // `$post->post_type`
                    let rest: String = chars.clone().take(2).map(|(_, x)| x).collect();
                    if rest == "->" {
                        out.push_str("->");
                        chars.next();
                        chars.next();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            out.push('}');
        } else {
            out.push(c);
        }
    }
    out
}

/// The callback argument: display text, the named function/method and its byte range.
fn callback_of(
    src: &str,
    tokens: &[Token<'_>],
    range: std::ops::Range<usize>,
) -> (String, Option<(String, usize, usize)>) {
    let toks = &tokens[range.clone()];
    let Some(first) = toks.first() else {
        return (String::new(), None);
    };
    let last = toks.last().expect("non-empty");
    let mut text: String = src[first.start..last.end]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if first.is_ident("function") || first.is_ident("fn") || first.is_ident("static") {
        text = "closure".into();
        return (text, None);
    }
    if text.chars().count() > 60 {
        text = text.chars().take(57).collect::<String>() + "…";
    }
    // 'my_function', 'Class::method', [ $this, 'method' ], array( 'Class', 'method' ).
    let named = toks
        .iter()
        .rev()
        .find(|t| t.kind == TokKind::Str)
        .and_then(|t| {
            let (v, interpolated) = string_value(t.text);
            if interpolated || v.is_empty() {
                return None;
            }
            let (s, e) = string_content(t);
            let v = v.rsplit("::").next().unwrap_or(&v).to_string();
            let skip = e - s - v.len();
            Some((v, s + skip, e))
        });
    (text, named)
}

fn line_text(src: &str, offset: usize) -> (String, u32) {
    let start = src[..offset].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let end = src[offset..]
        .find('\n')
        .map(|p| offset + p)
        .unwrap_or(src.len());
    let line = src[start..end].trim_end_matches('\r');
    let trimmed = line.trim_start();
    (trimmed.to_string(), (line.len() - trimmed.len()) as u32)
}

/// The hook references of one file.
pub fn scan(path: &str, src: &str) -> Vec<HookRef> {
    scan_tokens(path, src, &lexer::tokenize(src))
}

pub fn scan_tokens(path: &str, src: &str, tokens: &[Token<'_>]) -> Vec<HookRef> {
    let mut out = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        if t.kind != TokKind::Ident {
            continue;
        }
        let Some((kind, role)) = hook_function(t.text) else {
            continue;
        };
        if !is_global_call(tokens, i) {
            continue;
        }
        let args = lexer::arguments(tokens, i + 1);
        let Some(first) = args.first().filter(|r| !r.is_empty()) else {
            continue;
        };
        let Some((name, dynamic, start, end)) = name_of(tokens, first.clone()) else {
            continue;
        };
        let (line, column) = lexer::line_col(src, start);
        let (text, indent) = line_text(src, start);
        let mut r = HookRef {
            name,
            kind,
            role,
            function: t.text.trim_start_matches('\\').to_ascii_lowercase(),
            path: path.to_string(),
            line,
            column,
            start,
            end,
            dynamic,
            callback: None,
            callback_name: None,
            callback_range: None,
            priority: None,
            text,
            indent,
        };
        if matches!(
            role,
            HookRole::Registration | HookRole::Removal | HookRole::Check
        ) {
            if let Some(cb) = args.get(1).filter(|r| !r.is_empty()) {
                let (text, named) = callback_of(src, tokens, cb.clone());
                r.callback = Some(text);
                if let Some((n, s, e)) = named {
                    r.callback_name = Some(n);
                    r.callback_range = Some((s, e));
                }
            }
            if let Some(p) = args.get(2).filter(|r| r.len() == 1) {
                r.priority = tokens[p.start].text.parse().ok();
            }
            if role == HookRole::Registration && r.priority.is_none() {
                r.priority = Some(10);
            }
        }
        out.push(r);
    }
    out
}

/// Function and method declarations of one file.
pub fn functions(path: &str, src: &str, tokens: &[Token<'_>]) -> Vec<FunctionDecl> {
    let mut out = Vec::new();
    // (class name, brace depth of its body)
    let mut classes: Vec<(String, i32)> = Vec::new();
    let mut depth = 0i32;
    let mut pending_class: Option<String> = None;
    for (i, t) in tokens.iter().enumerate() {
        match t.kind {
            TokKind::Punct if t.text == "{" => {
                depth += 1;
                if let Some(c) = pending_class.take() {
                    classes.push((c, depth));
                }
            }
            TokKind::Punct if t.text == "}" => {
                if classes.last().is_some_and(|(_, d)| *d == depth) {
                    classes.pop();
                }
                depth -= 1;
            }
            TokKind::Ident
                if (t.is_ident("class") || t.is_ident("trait") || t.is_ident("interface"))
                    && !tokens.get(i.wrapping_sub(1)).is_some_and(|p| p.is("::")) =>
            {
                if let Some(n) = tokens.get(i + 1).filter(|n| n.kind == TokKind::Ident) {
                    pending_class = Some(n.text.to_string());
                }
            }
            TokKind::Ident if t.is_ident("function") => {
                let mut j = i + 1;
                if tokens.get(j).is_some_and(|n| n.is("&")) {
                    j += 1;
                }
                if let Some(n) = tokens.get(j).filter(|n| {
                    n.kind == TokKind::Ident && tokens.get(j + 1).is_some_and(|p| p.is("("))
                }) {
                    let (line, column) = lexer::line_col(src, n.start);
                    out.push(FunctionDecl {
                        name: n.text.to_string(),
                        class: classes.last().map(|(c, _)| c.clone()),
                        path: path.to_string(),
                        line,
                        column,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

/// Whether the hook `name` (static) matches `pattern` (a static name or a pattern with
/// `{…}` placeholders that stand for any non-empty text).
pub fn pattern_matches(pattern: &str, name: &str) -> bool {
    if !pattern.contains('{') {
        return pattern == name;
    }
    let mut parts = Vec::new();
    let mut rest = pattern;
    while let Some(open) = rest.find('{') {
        parts.push(&rest[..open]);
        match rest[open..].find('}') {
            Some(close) => rest = &rest[open + close + 1..],
            None => {
                rest = "";
                break;
            }
        }
    }
    parts.push(rest);
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !name.starts_with(first) || !name.ends_with(last) || name.len() < first.len() + last.len() {
        return false;
    }
    let mut pos = first.len();
    let end = name.len() - last.len();
    for mid in &parts[1..parts.len() - 1] {
        // Each placeholder stands for at least one character.
        let from = pos + 1;
        match name.get(from..end).and_then(|hay| hay.find(mid)) {
            Some(p) => pos = from + p + mid.len(),
            None => return false,
        }
    }
    end > pos
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLUGIN: &str = r#"<?php
/**
 * Plugin Name: Acme Events
 */
add_action( 'init', 'acme_register_post_type' );
add_filter( 'the_content', [ $this, 'append_badge' ], 20, 1 );
\add_action('admin_menu', function () { echo 'x'; });
$obj->add_action('not_a_hook');
function acme_register_post_type() {
    do_action( 'acme_before_register', $args );
    $title = apply_filters( "acme_title_{$post->post_type}", $title );
    do_action( 'acme_' . $kind . '_saved', $id );
}
class Acme_Badge {
    public function append_badge( $content ) { return $content; }
}
remove_filter( 'the_content', 'wpautop' );
did_action( 'init' );
"#;

    #[test]
    fn finds_registrations_and_invocations() {
        let refs = scan("acme-events.php", PLUGIN);
        let names: Vec<(&str, HookRole)> = refs.iter().map(|r| (r.name.as_str(), r.role)).collect();
        assert_eq!(
            names,
            vec![
                ("init", HookRole::Registration),
                ("the_content", HookRole::Registration),
                ("admin_menu", HookRole::Registration),
                ("acme_before_register", HookRole::Invocation),
                ("acme_title_{$post->post_type}", HookRole::Invocation),
                ("acme_{$kind}_saved", HookRole::Invocation),
                ("the_content", HookRole::Removal),
                ("init", HookRole::Check),
            ]
        );
        let init = &refs[0];
        assert_eq!((init.line, init.column), (4, 13));
        assert_eq!(&PLUGIN[init.start..init.end], "init");
        assert_eq!(init.callback.as_deref(), Some("'acme_register_post_type'"));
        assert_eq!(
            init.callback_name.as_deref(),
            Some("acme_register_post_type")
        );
        assert_eq!(init.priority, Some(10));
        assert_eq!(refs[1].callback_name.as_deref(), Some("append_badge"));
        assert_eq!(refs[1].priority, Some(20));
        assert_eq!(refs[2].callback.as_deref(), Some("closure"));
        assert_eq!(refs[2].function, "add_action");
        assert!(refs[4].dynamic && refs[5].dynamic && !refs[3].dynamic);
        assert_eq!(refs[3].kind, HookKind::Action);
        assert_eq!(refs[4].kind, HookKind::Filter);
        assert_eq!(refs[3].text, "do_action( 'acme_before_register', $args );");
        assert_eq!(refs[3].indent, 4);
    }

    #[test]
    fn finds_function_and_method_declarations() {
        let toks = lexer::tokenize(PLUGIN);
        let decls = functions("p.php", PLUGIN, &toks);
        let names: Vec<(&str, Option<&str>)> = decls
            .iter()
            .map(|d| (d.name.as_str(), d.class.as_deref()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("acme_register_post_type", None),
                ("append_badge", Some("Acme_Badge"))
            ]
        );
        assert_eq!(decls[0].line, 8);
    }

    #[test]
    fn matches_patterns() {
        assert!(pattern_matches("init", "init"));
        assert!(!pattern_matches("init", "init_x"));
        assert!(pattern_matches(
            "save_post_{$post->post_type}",
            "save_post_book"
        ));
        assert!(!pattern_matches(
            "save_post_{$post->post_type}",
            "save_post_"
        ));
        assert!(pattern_matches("acme_{$kind}_saved", "acme_event_saved"));
        assert!(!pattern_matches("acme_{$kind}_saved", "acme_event_deleted"));
        assert!(pattern_matches("{$tag}", "anything"));
        assert!(pattern_matches("a_{$x}_b_{$y}_c", "a_1_b_2_c"));
        assert!(!pattern_matches("a_{$x}_b_{$y}_c", "a_1_b__c"));
    }

    #[test]
    fn normalizes_simple_interpolation() {
        assert_eq!(
            normalize_interpolation("save_post_$post_type"),
            "save_post_{$post_type}"
        );
        assert_eq!(normalize_interpolation("x_$post->type"), "x_{$post->type}");
        assert_eq!(normalize_interpolation("x_{$a['k']}"), "x_{$a['k']}");
    }
}
