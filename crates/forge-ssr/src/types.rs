//! Types of expressions for the Type modifier, where the syntax tree tells them: literals,
//! `new X`, `$this`, parameters with a declared type and variables assigned with `new X`
//! earlier in the same function.

use tree_sitter::Node;

fn text<'a>(n: Node, src: &'a str) -> &'a str {
    &src[n.start_byte()..n.end_byte()]
}

fn child_of_kind<'t>(n: Node<'t>, kinds: &[&str]) -> Option<Node<'t>> {
    let mut c = n.walk();
    let found = n.named_children(&mut c).find(|k| kinds.contains(&k.kind()));
    found
}

/// The class name of `new X(…)`.
fn created_class(n: Node, src: &str) -> Option<String> {
    let class = child_of_kind(
        n,
        &["name", "qualified_name", "identifier", "member_expression"],
    )?;
    Some(text(class, src).trim_start_matches('\\').to_string())
}

fn enclosing<'t>(n: Node<'t>, kinds: &[&str]) -> Option<Node<'t>> {
    let mut p = n.parent();
    while let Some(x) = p {
        if kinds.contains(&x.kind()) {
            return Some(x);
        }
        p = x.parent();
    }
    None
}

const PHP_FUNCTIONS: &[&str] = &[
    "method_declaration",
    "function_definition",
    "anonymous_function",
    "arrow_function",
];

/// A PHP variable's type: `$this`, a typed parameter, or `$x = new X` before `n`.
fn php_variable(n: Node, src: &str) -> Option<String> {
    let name = text(n, src);
    if name == "$this" {
        let class = enclosing(
            n,
            &["class_declaration", "trait_declaration", "enum_declaration"],
        )?;
        return class
            .child_by_field_name("name")
            .map(|c| text(c, src).to_string());
    }
    let function = enclosing(n, PHP_FUNCTIONS)?;
    if let Some(params) = function.child_by_field_name("parameters") {
        let mut c = params.walk();
        for p in params.named_children(&mut c) {
            let declared = p
                .child_by_field_name("name")
                .is_some_and(|v| text(v, src) == name);
            if declared {
                return p.child_by_field_name("type").map(|t| {
                    text(t, src)
                        .trim_start_matches('?')
                        .trim_start_matches('\\')
                        .to_string()
                });
            }
        }
    }
    // The last `$name = new X` before the node.
    let body = function.child_by_field_name("body")?;
    let mut best = None;
    let mut stack = vec![body];
    while let Some(x) = stack.pop() {
        if x.start_byte() >= n.start_byte() {
            continue;
        }
        if x.kind() == "assignment_expression" {
            let left = x.child_by_field_name("left");
            let right = x.child_by_field_name("right");
            if let (Some(l), Some(r)) = (left, right) {
                if text(l, src) == name && r.kind() == "object_creation_expression" {
                    if let Some(class) = created_class(r, src) {
                        best = Some((x.start_byte(), class));
                    }
                }
            }
        }
        let mut c = x.walk();
        stack.extend(x.named_children(&mut c));
    }
    best.map(|(_, class)| class)
}

/// The type of an expression node, when the tree shows it.
pub(crate) fn infer(n: Node, src: &str) -> Option<String> {
    Some(match n.kind() {
        // PHP
        "string" | "encapsed_string" | "heredoc" | "nowdoc" => "string".into(),
        "integer" => "int".into(),
        "float" => "float".into(),
        "boolean" => "bool".into(),
        "null" => "null".into(),
        "array_creation_expression" => "array".into(),
        "object_creation_expression" => created_class(n, src)?,
        "variable_name" => php_variable(n, src)?,
        "parenthesized_expression" | "argument" => infer(n.named_child(0)?, src)?,
        // JavaScript / TypeScript
        "template_string" => "string".into(),
        "number" => "number".into(),
        "true" | "false" => "boolean".into(),
        "array" => "Array".into(),
        "object" => "Object".into(),
        "new_expression" => {
            let ctor = n.child_by_field_name("constructor")?;
            text(ctor, src).to_string()
        }
        _ => return None,
    })
}
