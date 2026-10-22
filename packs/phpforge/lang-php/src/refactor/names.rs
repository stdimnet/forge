//! Name suggestions for a new variable, field, parameter or constant, like PHPStorm: from the
//! method or property an expression reads (`getUserName()` -> `userName`, `name`), the
//! class it creates, the variable it is assigned to, a string literal's words, or its type.

use std::collections::HashSet;

use tree_sitter::Node;

use super::{camel, text, words};

/// Suggestions for `expr` (camelCase, without `$`), best first, never empty.
pub fn suggest(expr: Node, src: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    // The variable the expression is assigned to, the parameter it is passed as... first.
    if let Some(from_context) = context_name(expr, src) {
        push_with_tails(&mut out, &from_context);
    }
    for name in from_expression(expr, src) {
        push_with_tails(&mut out, &name);
    }
    if out.is_empty() {
        out.push("value".into());
    }
    out
}

/// `userName` adds `userName` and `name`.
fn push_with_tails(out: &mut Vec<String>, name: &str) {
    let ws = words(name);
    if ws.is_empty() {
        return;
    }
    for i in 0..ws.len() {
        let candidate = camel(&ws[i..]);
        if is_reserved(&candidate) || candidate.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        if !out.contains(&candidate) {
            out.push(candidate);
        }
    }
}

fn is_reserved(name: &str) -> bool {
    matches!(name, "this" | "" | "class" | "function" | "new" | "static")
}

fn strip_accessor(name: &str) -> String {
    let ws = words(name);
    match ws.first().map(String::as_str) {
        Some("get" | "find" | "fetch" | "load" | "create" | "make" | "build" | "to" | "as")
            if ws.len() > 1 =>
        {
            camel(&ws[1..])
        }
        _ => camel(&ws),
    }
}

fn from_expression(n: Node, src: &str) -> Vec<String> {
    let field = |f: &str| n.child_by_field_name(f).map(|x| text(x, src).to_string());
    match n.kind() {
        "member_call_expression" | "nullsafe_member_call_expression" | "scoped_call_expression" => {
            field("name")
                .map(|m| vec![strip_accessor(&m)])
                .unwrap_or_default()
        }
        "function_call_expression" => field("function")
            .map(|f| {
                let short = f.rsplit('\\').next().unwrap_or(&f).to_string();
                vec![strip_accessor(&short)]
            })
            .unwrap_or_default(),
        "member_access_expression" | "nullsafe_member_access_expression" => {
            field("name").map(|m| vec![m]).unwrap_or_default()
        }
        "scoped_property_access_expression" => field("name")
            .map(|m| vec![m.trim_start_matches('$').to_string()])
            .unwrap_or_default(),
        "class_constant_access_expression" => {
            let last = n.named_child(n.named_child_count().saturating_sub(1));
            match last.map(|l| text(l, src)) {
                Some("class") => {
                    let first = n.named_child(0).map(|f| text(f, src)).unwrap_or_default();
                    vec![format!("{}Class", short(first))]
                }
                Some(c) => vec![c.to_lowercase()],
                None => vec![],
            }
        }
        "variable_name" => vec![text(n, src).trim_start_matches('$').to_string()],
        "object_creation_expression" => {
            let mut c = n.walk();
            let class = n
                .named_children(&mut c)
                .find(|ch| matches!(ch.kind(), "name" | "qualified_name"))
                .map(|ch| short(text(ch, src)).to_string());
            class.into_iter().collect()
        }
        "string" | "encapsed_string" => {
            let content = text(n, src).trim_matches(|c| c == '\'' || c == '"');
            let ws = words(content);
            if !content.is_empty()
                && content.len() <= 30
                && ws.len() <= 3
                && content
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.'))
            {
                vec![camel(&ws), "string".into()]
            } else {
                vec!["string".into()]
            }
        }
        "integer" | "float" => vec!["number".into()],
        "boolean" => vec!["bool".into()],
        "array_creation_expression" => vec!["array".into()],
        "anonymous_function" | "arrow_function" => vec!["callback".into()],
        "binary_expression" => {
            let op = n
                .child_by_field_name("operator")
                .map(|o| text(o, src))
                .unwrap_or_default();
            match op {
                "." => vec!["string".into()],
                "==" | "===" | "!=" | "!==" | "<" | ">" | "<=" | ">=" | "&&" | "||" | "and"
                | "or" | "xor" | "instanceof" => vec!["condition".into()],
                "??" => n
                    .child_by_field_name("left")
                    .map(|l| from_expression(l, src))
                    .unwrap_or_default(),
                _ => vec!["result".into()],
            }
        }
        "unary_op_expression" => vec!["condition".into()],
        "cast_expression" => {
            let ty = n
                .child_by_field_name("type")
                .map(|t| text(t, src).trim().to_lowercase());
            let inner = n
                .child_by_field_name("value")
                .map(|v| from_expression(v, src))
                .unwrap_or_default();
            inner.into_iter().chain(ty).collect()
        }
        "parenthesized_expression" | "clone_expression" | "error_suppression_expression" => n
            .named_child(0)
            .map(|c| from_expression(c, src))
            .unwrap_or_default(),
        "conditional_expression" => n
            .child_by_field_name("body")
            .or_else(|| n.child_by_field_name("alternative"))
            .map(|c| from_expression(c, src))
            .unwrap_or_default(),
        "subscript_expression" => {
            let index = n.named_child(1).map(|i| text(i, src));
            match index {
                Some(i) if i.starts_with('\'') || i.starts_with('"') => {
                    vec![camel(&words(i.trim_matches(|c| c == '\'' || c == '"')))]
                }
                _ => vec!["item".into()],
            }
        }
        "match_expression" => vec!["result".into()],
        _ => vec![],
    }
}

fn short(name: &str) -> &str {
    name.rsplit('\\').next().unwrap_or(name)
}

/// A name from where the expression is used: `$total = <expr>` -> `total`, `return <expr>`
/// in `getTotal()` -> `total`, a named argument `limit: <expr>` -> `limit`.
fn context_name(n: Node, src: &str) -> Option<String> {
    let p = n.parent()?;
    match p.kind() {
        "assignment_expression" if p.child_by_field_name("right") == Some(n) => {
            let left = p.child_by_field_name("left")?;
            match left.kind() {
                "variable_name" => Some(text(left, src).trim_start_matches('$').to_string()),
                "member_access_expression" => left
                    .child_by_field_name("name")
                    .map(|m| text(m, src).into()),
                _ => None,
            }
        }
        "argument" => p
            .child_by_field_name("name")
            .map(|name| text(name, src).to_string()),
        _ => None,
    }
}

/// `name`, or `name1`, `name2`... when it is taken.
pub fn unique(name: &str, taken: &HashSet<String>) -> String {
    if !taken.contains(name) {
        return name.to_string();
    }
    (1..)
        .map(|i| format!("{name}{i}"))
        .find(|n| !taken.contains(n))
        .unwrap_or_else(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refactor::{expression_at, parse, test_util::sel};

    fn names(marked: &str) -> Vec<String> {
        let (src, s, e) = sel(marked);
        let tree = parse(&src).unwrap();
        let n = expression_at(tree.root_node(), &src, s, e).unwrap();
        suggest(n, &src)
    }

    #[test]
    fn suggests_from_methods_properties_classes_and_literals() {
        assert_eq!(
            names("<?php echo ‹$this->getUserName()›;"),
            vec!["userName", "name"]
        );
        assert_eq!(names("<?php echo ‹$order->total›;"), vec!["total"]);
        assert_eq!(
            names("<?php foo(‹new UserRepository()›);"),
            vec!["userRepository", "repository"]
        );
        assert_eq!(names("<?php foo(‹'active'›);"), vec!["active", "string"]);
        assert_eq!(names("<?php foo(‹$a + $b›);"), vec!["result"]);
        assert_eq!(names("<?php foo(‹count($items)›);"), vec!["count"]);
        assert_eq!(
            names("<?php foo(‹$row['created_at']›);"),
            vec!["createdAt", "at"]
        );
        assert_eq!(names("<?php foo(‹User::class›);"), vec!["userClass"]);
    }

    #[test]
    fn prefers_the_assigned_variable() {
        assert_eq!(names("<?php $total = ‹$a * $b›;"), vec!["total", "result"]);
        assert_eq!(unique("total", &["total".to_string()].into()), "total1");
    }
}
