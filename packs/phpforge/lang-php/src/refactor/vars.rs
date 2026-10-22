//! Local variables of a function body: every occurrence with whether it reads or writes
//! the variable, in evaluation order. Closures (`function () use (...)`) only see their
//! `use` list; arrow functions see the enclosing variables except their own parameters.

use std::collections::HashSet;

use tree_sitter::Node;

use super::{after_as, is_nested_declaration, text, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
    /// `$x .= ...`, `$x++`, `$arr[] = ...`: reads the old value and changes it.
    ReadWrite,
}

impl Access {
    pub fn reads(self) -> bool {
        self != Access::Write
    }
    pub fn writes(self) -> bool {
        self != Access::Read
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarUse {
    /// Without `$`.
    pub name: String,
    /// The `$name` token.
    pub span: Span,
    pub access: Access,
    /// Evaluation order: a write of an assignment happens after its right side.
    pub order: usize,
}

const SUPERGLOBALS: &[&str] = &[
    "GLOBALS",
    "_SERVER",
    "_GET",
    "_POST",
    "_FILES",
    "_COOKIE",
    "_SESSION",
    "_REQUEST",
    "_ENV",
    "http_response_header",
    "argc",
    "argv",
];

/// The variable occurrences in `root` (a function body or the file), in evaluation order.
/// `$this` and superglobals are left out.
pub fn collect(root: Node, src: &str) -> Vec<VarUse> {
    let mut out = Vec::new();
    visit(root, root, src, &HashSet::new(), &mut out);
    out.sort_by_key(|u| (u.order, u.span.start));
    out
}

fn visit(n: Node, root: Node, src: &str, shadow: &HashSet<String>, out: &mut Vec<VarUse>) {
    if n != root && is_nested_declaration(n) {
        return;
    }
    match n.kind() {
        "variable_name" => {
            if let Some(u) = occurrence(n, src) {
                if !shadow.contains(&u.name) {
                    out.push(u);
                }
            }
            return;
        }
        "anonymous_function" if n != root => {
            // Only the `use` list belongs to this scope.
            let mut c = n.walk();
            for ch in n.children(&mut c) {
                if ch.kind() == "anonymous_function_use_clause" {
                    let mut c2 = ch.walk();
                    for v in ch.named_children(&mut c2) {
                        let (var, access) = if v.kind() == "by_ref" {
                            (v.named_child(0), Access::ReadWrite)
                        } else {
                            (Some(v), Access::Read)
                        };
                        if let Some(var) = var.filter(|v| v.kind() == "variable_name") {
                            let name = text(var, src).trim_start_matches('$').to_string();
                            if !shadow.contains(&name) {
                                out.push(VarUse {
                                    name,
                                    span: Span::of(var),
                                    access,
                                    order: var.start_byte(),
                                });
                            }
                        }
                    }
                }
            }
            return;
        }
        "arrow_function" if n != root => {
            let mut inner = shadow.clone();
            if let Some(params) = n.child_by_field_name("parameters") {
                inner.extend(parameter_names(params, src));
            }
            if let Some(body) = n.child_by_field_name("body") {
                visit(body, root, src, &inner, out);
            }
            return;
        }
        _ => {}
    }
    let mut c = n.walk();
    for ch in n.children(&mut c) {
        visit(ch, root, src, shadow, out);
    }
}

/// The parameter names (without `$`) of a `formal_parameters` node.
pub fn parameter_names(params: Node, src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut c = params.walk();
    for p in params.named_children(&mut c) {
        if let Some(name) = p.child_by_field_name("name") {
            out.push(text(name, src).trim_start_matches('$').to_string());
        }
    }
    out
}

fn occurrence(n: Node, src: &str) -> Option<VarUse> {
    let name = text(n, src).strip_prefix('$')?;
    // `$$x` and `${...}` are dynamic.
    if name.is_empty() || name == "this" || SUPERGLOBALS.contains(&name) {
        return None;
    }
    if !name.chars().all(|c| c == '_' || c.is_alphanumeric()) {
        return None;
    }
    let p = n.parent()?;
    // `static::$prop`, `$obj->$name` is a read of `$name` though.
    if matches!(p.kind(), "scoped_property_access_expression")
        && p.child_by_field_name("name") == Some(n)
    {
        return None;
    }
    if matches!(
        p.kind(),
        "simple_parameter"
            | "variadic_parameter"
            | "property_promotion_parameter"
            | "property_element"
    ) {
        return None;
    }
    let (access, order) = classify(n);
    Some(VarUse {
        name: name.to_string(),
        span: Span::of(n),
        access,
        order,
    })
}

fn is_left_of(p: Node, n: Node) -> bool {
    p.child_by_field_name("left") == Some(n)
}

/// How an occurrence uses its variable, and when.
fn classify(n: Node) -> (Access, usize) {
    let at = n.start_byte();
    let Some(p) = n.parent() else {
        return (Access::Read, at);
    };
    match p.kind() {
        "assignment_expression" | "reference_assignment_expression" if is_left_of(p, n) => {
            (Access::Write, p.end_byte())
        }
        "augmented_assignment_expression" if is_left_of(p, n) => (Access::ReadWrite, p.end_byte()),
        "update_expression" => (Access::ReadWrite, at),
        "catch_clause" | "unset_statement" | "global_declaration" => (Access::Write, at),
        "static_variable_declaration" => (Access::Write, p.end_byte()),
        "foreach_statement" if after_as(p, n) => (Access::Write, at),
        "list_literal" | "pair" | "by_ref" => match destructuring_target(n) {
            Some(end) => (Access::Write, end),
            None => (Access::Read, at),
        },
        "subscript_expression" if p.named_child(0) == Some(n) => {
            // `$arr[] = 1`, `$arr['k'] .= 'x'`: the array is changed.
            let mut top = p;
            while let Some(pp) = top.parent() {
                if pp.kind() == "subscript_expression" && pp.named_child(0) == Some(top) {
                    top = pp;
                } else {
                    break;
                }
            }
            match top.parent() {
                Some(a)
                    if matches!(
                        a.kind(),
                        "assignment_expression"
                            | "augmented_assignment_expression"
                            | "reference_assignment_expression"
                    ) && is_left_of(a, top) =>
                {
                    (Access::ReadWrite, a.end_byte())
                }
                Some(u) if u.kind() == "update_expression" => (Access::ReadWrite, at),
                _ => (Access::Read, at),
            }
        }
        _ => (Access::Read, at),
    }
}

/// `[$a, $b] = ...`, `foreach (... as [$a, $b])`: the order of the write (the end of the
/// assignment), or `None` when the list is not an assignment target.
fn destructuring_target(n: Node) -> Option<usize> {
    let mut cur = n;
    while let Some(p) = cur.parent() {
        match p.kind() {
            "list_literal" | "pair" | "by_ref" | "array_element_initializer" => cur = p,
            "assignment_expression" if is_left_of(p, cur) => return Some(p.end_byte()),
            "foreach_statement" if after_as(p, cur) => return Some(cur.start_byte()),
            _ => return None,
        }
    }
    None
}

/// Uses `$this` (closures and arrow functions bind it too).
pub fn uses_this(n: Node, src: &str) -> bool {
    if n.kind() == "variable_name" && text(n, src) == "$this" {
        return true;
    }
    if is_nested_declaration(n) {
        return false;
    }
    let mut c = n.walk();
    let found = n.children(&mut c).any(|ch| uses_this(ch, src));
    found
}

/// The names of all variables in a scope (for unique new names).
pub fn names_in(root: Node, src: &str) -> HashSet<String> {
    let mut out: HashSet<String> = collect(root, src).into_iter().map(|u| u.name).collect();
    if let Some(params) = root
        .parent()
        .and_then(|f| f.child_by_field_name("parameters"))
    {
        out.extend(parameter_names(params, src));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refactor::parse;

    fn uses(src: &str) -> Vec<(String, Access)> {
        let tree = parse(src).unwrap();
        collect(tree.root_node(), src)
            .into_iter()
            .map(|u| (u.name, u.access))
            .collect()
    }

    #[test]
    fn classifies_reads_and_writes_in_evaluation_order() {
        let found = uses("<?php $x = $a + 1; $x .= 'b'; $n++; [$c, $d] = foo(); $arr[] = $x;");
        use Access::*;
        assert_eq!(
            found,
            vec![
                ("a".into(), Read),
                ("x".into(), Write),
                ("x".into(), ReadWrite),
                ("n".into(), ReadWrite),
                ("c".into(), Write),
                ("d".into(), Write),
                ("x".into(), Read),
                ("arr".into(), ReadWrite),
            ]
        );
    }

    #[test]
    fn closures_see_only_their_use_list_and_arrow_functions_shadow_parameters() {
        let found = uses(
            "<?php $f = function ($u) use ($x, &$y) { return $u + $z; }; $g = fn($w) => $w + $v;\nforeach ($items as $k => $item) {}",
        );
        let names: Vec<_> = found.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["x", "y", "f", "v", "g", "items", "k", "item"]);
        assert_eq!(found[1].1, Access::ReadWrite);
        assert_eq!(found[6].1, Access::Write);
    }

    #[test]
    fn this_and_properties_are_not_variables() {
        let src = "<?php $this->a = static::$b + $c->d;";
        assert_eq!(uses(src), vec![("c".into(), Access::Read)]);
        let tree = parse(src).unwrap();
        assert!(uses_this(tree.root_node(), src));
    }
}
