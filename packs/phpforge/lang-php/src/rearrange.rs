//! Rearrange Code for PHP: orders the members of classes, interfaces, traits and enums
//! by a simple arrangement, like PHPStorm's default one:
//!
//! 1. trait `use` statements
//! 2. enum cases
//! 3. constants
//! 4. properties
//! 5. the constructor
//! 6. the other methods
//!
//! Members keep their relative order inside a group. Comments and docblocks move with the
//! member below them; a comment on the same line as a member stays with that member. The
//! whitespace between members stays where it was, so blank-line layout is kept. Files with
//! syntax errors are left alone.

use tree_sitter::Node;

use crate::imports::apply_edits;
use crate::parser::new_parser;

/// Rearrange every class-like declaration of `src`. `None` when nothing changes.
pub fn rearrange(src: &str) -> Option<String> {
    let tree = new_parser().parse(src, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }
    let mut bodies = Vec::new();
    class_bodies(root, &mut bodies);
    let edits: Vec<(usize, usize, String)> = bodies
        .into_iter()
        .filter_map(|body| rearrange_body(body, src))
        .collect();
    if edits.is_empty() {
        return None;
    }
    Some(apply_edits(src, edits))
}

/// Bodies of class-likes declared at statement level (not inside functions or methods,
/// whose text moves with the enclosing member).
fn class_bodies<'t>(node: Node<'t>, out: &mut Vec<Node<'t>>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "class_declaration"
            | "interface_declaration"
            | "trait_declaration"
            | "enum_declaration" => {
                if let Some(body) = child.child_by_field_name("body") {
                    out.push(body);
                }
            }
            "namespace_definition"
            | "compound_statement"
            | "if_statement"
            | "else_clause"
            | "declare_statement" => class_bodies(child, out),
            _ => {}
        }
    }
}

/// Group of a member in the arrangement; `None` for nodes that are not members.
fn rank(node: Node, src: &str) -> Option<u8> {
    Some(match node.kind() {
        "use_declaration" => 0,
        "enum_case" => 1,
        "const_declaration" => 2,
        "property_declaration" => 3,
        "method_declaration" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| &src[n.byte_range()])
                .unwrap_or("");
            if name.eq_ignore_ascii_case("__construct") {
                4
            } else {
                5
            }
        }
        _ => return None,
    })
}

struct Item {
    start: usize,
    end: usize,
    rank: u8,
}

fn line_start(src: &str, at: usize) -> usize {
    src[..at].rfind('\n').map_or(0, |i| i + 1)
}

/// The edit that reorders one class body, if its order changes.
fn rearrange_body(body: Node, src: &str) -> Option<(usize, usize, String)> {
    let mut cursor = body.walk();
    let children: Vec<Node> = body.named_children(&mut cursor).collect();
    let mut items: Vec<Item> = Vec::new();
    // Leading comments waiting for the member below them.
    let mut pending: Option<usize> = None;
    for child in &children {
        if child.kind() == "comment" {
            // A comment on the line where the previous member ends belongs to that member.
            if pending.is_none() {
                if let Some(last) = items.last_mut() {
                    let gap = &src[last.end..child.start_byte()];
                    if !gap.contains('\n') {
                        last.end = child.end_byte();
                        continue;
                    }
                }
            }
            pending.get_or_insert(child.start_byte());
            continue;
        }
        let rank = rank(*child, src)?;
        let start = pending.take().unwrap_or(child.start_byte());
        items.push(Item {
            start,
            end: child.end_byte(),
            rank,
        });
    }
    if items.windows(2).all(|w| w[0].rank <= w[1].rank) {
        return None;
    }
    // Items start at the beginning of their line when only indentation precedes them.
    for item in &mut items {
        let ls = line_start(src, item.start);
        if src[ls..item.start].trim().is_empty() && ls > body.start_byte() {
            item.start = ls;
        }
    }
    let separators: Vec<&str> = items
        .windows(2)
        .map(|w| &src[w[0].end..w[1].start])
        .collect();
    if separators.iter().any(|s| !s.trim().is_empty()) {
        return None;
    }
    let mut order: Vec<&Item> = items.iter().collect();
    order.sort_by_key(|i| i.rank);
    let mut text = String::new();
    for (i, item) in order.iter().enumerate() {
        if i > 0 {
            text.push_str(separators[i - 1]);
        }
        text.push_str(&src[item.start..item.end]);
    }
    Some((items[0].start, items[items.len() - 1].end, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_constants_properties_constructor_methods() {
        let src = "<?php\nclass A\n{\n    public function run(): void {}\n\n    private int $count = 0;\n\n    public function __construct() {}\n\n    const MAX = 3;\n}\n";
        assert_eq!(
            rearrange(src).unwrap(),
            "<?php\nclass A\n{\n    const MAX = 3;\n\n    private int $count = 0;\n\n    public function __construct() {}\n\n    public function run(): void {}\n}\n"
        );
    }

    #[test]
    fn docblocks_and_attributes_move_with_their_member() {
        let src = "<?php\nclass A\n{\n    /**\n     * Runs.\n     */\n    #[Pure]\n    public function run() {}\n    // the limit\n    public $limit = 1; // trailing\n}\n";
        assert_eq!(
            rearrange(src).unwrap(),
            "<?php\nclass A\n{\n    // the limit\n    public $limit = 1; // trailing\n    /**\n     * Runs.\n     */\n    #[Pure]\n    public function run() {}\n}\n"
        );
    }

    #[test]
    fn trait_uses_and_enum_cases_come_first() {
        let src = "<?php\nenum S: string\n{\n    const X = 1;\n    use T;\n    case A = 'a';\n}\n";
        assert_eq!(
            rearrange(src).unwrap(),
            "<?php\nenum S: string\n{\n    use T;\n    case A = 'a';\n    const X = 1;\n}\n"
        );
    }

    #[test]
    fn ordered_classes_are_unchanged() {
        let src = "<?php\nclass A\n{\n    const X = 1;\n    public $a;\n    public function __construct() {}\n    public function b() {}\n    public function a() {}\n}\n";
        assert_eq!(rearrange(src), None);
    }

    #[test]
    fn every_class_of_a_namespace_is_rearranged() {
        let src = "<?php\nnamespace App;\n\ninterface I { function f(); const C = 1; }\n\ntrait T { function f() {} public $p; }\n";
        assert_eq!(
            rearrange(src).unwrap(),
            "<?php\nnamespace App;\n\ninterface I { const C = 1; function f(); }\n\ntrait T { public $p; function f() {} }\n"
        );
    }

    #[test]
    fn syntax_errors_leave_the_file_alone() {
        let src = "<?php\nclass A { function f() {} const X = ; }\n";
        assert_eq!(rearrange(src), None);
    }
}
