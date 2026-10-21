//! Code Cleanup for PHP: the safe quick fixes, applied to a whole file.
//!
//! - unused `use` imports are removed ([`crate::imports::remove_unused_imports`]),
//! - unnecessary semicolons (empty statements in a statement list, such as `;;` or the `;`
//!   after a class body) are removed; an empty statement that is the body of a loop or `if`
//!   stays, because removing it would change the code,
//! - a redundant closing tag `?>` at the end of a pure PHP file is removed.
//!
//! Files with syntax errors are left alone.

use tree_sitter::Node;

use crate::imports::{apply_edits, remove_lines, remove_unused_imports};
use crate::parser::new_parser;

/// Apply every cleanup fix. `None` when nothing changes.
pub fn code_cleanup(src: &str) -> Option<String> {
    let mut text = src.to_string();
    let mut changed = false;
    for step in [remove_unused_imports, remove_semicolons, remove_closing_tag] {
        if let Some(next) = step(&text) {
            text = next;
            changed = true;
        }
    }
    changed.then_some(text)
}

/// Statement lists: an empty statement directly inside them does nothing.
fn is_statement_list(kind: &str) -> bool {
    matches!(kind, "program" | "compound_statement" | "declaration_list")
}

fn empty_statements<'t>(node: Node<'t>, out: &mut Vec<Node<'t>>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "empty_statement" {
            if is_statement_list(node.kind()) {
                out.push(child);
            }
        } else {
            empty_statements(child, out);
        }
    }
}

/// Remove unnecessary semicolons. `None` when there are none.
pub fn remove_semicolons(src: &str) -> Option<String> {
    let tree = new_parser().parse(src, None)?;
    if tree.root_node().has_error() {
        return None;
    }
    let mut found = Vec::new();
    empty_statements(tree.root_node(), &mut found);
    if found.is_empty() {
        return None;
    }
    let edits = found
        .iter()
        .map(|n| remove_lines(src, n.start_byte(), n.end_byte()))
        .collect();
    Some(apply_edits(src, edits))
}

fn has_html(node: Node) -> bool {
    if matches!(node.kind(), "text" | "text_interpolation") {
        return true;
    }
    let mut cursor = node.walk();
    let found = node.named_children(&mut cursor).any(has_html);
    found
}

/// Remove `?>` at the end of a file that holds nothing but PHP. `None` when it has none.
pub fn remove_closing_tag(src: &str) -> Option<String> {
    let tree = new_parser().parse(src, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }
    let mut cursor = root.walk();
    let children: Vec<Node> = root.named_children(&mut cursor).collect();
    let last = *children.last()?;
    if last.kind() != "text_interpolation" || last.named_child_count() != 1 {
        return None;
    }
    // Inline HTML anywhere means a template, where the closing tag is kept.
    let html = children[..children.len() - 1].iter().any(|n| has_html(*n));
    if html || !src[last.end_byte()..].trim().is_empty() {
        return None;
    }
    let mut out = src[..last.start_byte()].trim_end().to_string();
    out.push('\n');
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_unnecessary_semicolons_but_not_empty_loop_bodies() {
        let src = "<?php\nclass A {};\n$a = 1;;\nwhile (next());\nfor (;;) { break; }\n";
        assert_eq!(
            remove_semicolons(src).unwrap(),
            "<?php\nclass A {}\n$a = 1;\nwhile (next());\nfor (;;) { break; }\n"
        );
    }

    #[test]
    fn a_semicolon_on_its_own_line_goes_with_the_line() {
        let src = "<?php\nfunction f() {\n    ;\n    return 1;\n}\n";
        assert_eq!(
            remove_semicolons(src).unwrap(),
            "<?php\nfunction f() {\n    return 1;\n}\n"
        );
    }

    #[test]
    fn removes_the_final_closing_tag_of_pure_php_files() {
        assert_eq!(
            remove_closing_tag("<?php\necho 1;\n?>\n").unwrap(),
            "<?php\necho 1;\n"
        );
        assert_eq!(remove_closing_tag("<?php\necho 1;\n"), None);
        assert_eq!(remove_closing_tag("<?php if ($a): ?>\n<b>x</b>\n<?php endif; ?>\n"), None);
        assert_eq!(remove_closing_tag("<?php echo 1; ?>\n<div></div>\n"), None);
    }

    #[test]
    fn cleanup_combines_the_fixes() {
        let src = "<?php\nuse A\\Gone;\nuse A\\Kept;\n\nnew Kept();;\n?>\n";
        assert_eq!(
            code_cleanup(src).unwrap(),
            "<?php\nuse A\\Kept;\n\nnew Kept();\n"
        );
        assert_eq!(code_cleanup("<?php\necho 1;\n"), None);
    }
}
