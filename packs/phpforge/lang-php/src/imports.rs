//! Optimize Imports (Ctrl+Alt+O) for PHP: removes `use` imports that the file does not
//! reference and sorts the rest (classes, then functions, then constants, each
//! alphabetically), like PHPStorm. Code Cleanup removes unused imports without sorting.
//!
//! The analysis is conservative: a name counts as used when it appears anywhere in the code
//! (types, `new`, static calls, attributes, function calls, constants) or in a comment
//! (docblock types and annotations such as `@ORM\Column`), so an import is only removed
//! when nothing can refer to it. Files with syntax errors are left alone.

use std::collections::HashSet;

use tree_sitter::Node;

use crate::model::UseKind;
use crate::parser::new_parser;

/// Remove unused imports and sort the remaining ones. `None` when nothing changes.
pub fn optimize_imports(src: &str) -> Option<String> {
    rewrite_imports(src, true)
}

/// Remove unused imports, keeping the order of the others. `None` when nothing changes.
pub fn remove_unused_imports(src: &str) -> Option<String> {
    rewrite_imports(src, false)
}

/// One imported name of a `use` statement (group uses are expanded).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Clause {
    kind: UseKind,
    fqn: String,
    alias: Option<String>,
}

impl Clause {
    fn local_name(&self) -> &str {
        self.alias
            .as_deref()
            .unwrap_or_else(|| self.fqn.rsplit('\\').next().unwrap_or(&self.fqn))
    }

    fn render(&self) -> String {
        let keyword = match self.kind {
            UseKind::Class => "",
            UseKind::Function => "function ",
            UseKind::Const => "const ",
        };
        match &self.alias {
            Some(a) => format!("use {keyword}{} as {a};", self.fqn),
            None => format!("use {keyword}{};", self.fqn),
        }
    }

    fn kind_rank(&self) -> u8 {
        match self.kind {
            UseKind::Class => 0,
            UseKind::Function => 1,
            UseKind::Const => 2,
        }
    }
}

/// Names the code refers to: exact spellings (constants are case-sensitive) and lower-cased
/// ones (classes and functions are not).
#[derive(Default)]
struct Usages {
    exact: HashSet<String>,
    lower: HashSet<String>,
}

impl Usages {
    fn add(&mut self, name: &str) {
        let first = name.trim_start_matches('\\').split('\\').next().unwrap_or("");
        if first.is_empty() {
            return;
        }
        self.lower.insert(first.to_lowercase());
        self.exact.insert(first.to_string());
    }

    fn uses(&self, clause: &Clause) -> bool {
        let name = clause.local_name();
        match clause.kind {
            UseKind::Const => self.exact.contains(name),
            _ => self.lower.contains(&name.to_lowercase()),
        }
    }

    fn collect(&mut self, node: Node, src: &str) {
        match node.kind() {
            "namespace_use_declaration" | "variable_name" => return,
            "namespace_definition" => {
                // The namespace's own name is no usage; its body (braced form) is.
                if let Some(body) = node.child_by_field_name("body") {
                    self.collect(body, src);
                }
                return;
            }
            "qualified_name" | "namespace_name" | "name" => {
                self.add(&src[node.byte_range()]);
                return;
            }
            "comment" => {
                self.comment(&src[node.byte_range()]);
                return;
            }
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.collect(child, src);
        }
    }

    /// Every identifier-like word in a comment (`@param User $u`, `@ORM\Entity`, `{@see X}`).
    fn comment(&mut self, text: &str) {
        let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '\\';
        for word in text.split(|c: char| !is_word(c)) {
            if word.chars().any(|c| c.is_alphabetic() || c == '_') {
                self.add(word);
            }
        }
    }
}

fn clause_kind(text: &str, default: UseKind) -> UseKind {
    match text.split_whitespace().next() {
        Some("function") => UseKind::Function,
        Some("const") => UseKind::Const,
        _ => default,
    }
}

/// The clauses of one `use` statement.
fn clauses_of(decl: Node, src: &str) -> Vec<Clause> {
    let text = |n: Node| &src[n.byte_range()];
    let head = text(decl).trim_start_matches("use").trim_start();
    let kind = clause_kind(head, UseKind::Class);
    let mut prefix = String::new();
    let mut out = Vec::new();
    let mut cursor = decl.walk();
    for child in decl.named_children(&mut cursor) {
        match child.kind() {
            "namespace_name" => prefix = text(child).trim_matches('\\').to_string(),
            "namespace_use_clause" => out.extend(clause(child, src, "", kind)),
            "namespace_use_group" => {
                let mut c = child.walk();
                for item in child.named_children(&mut c) {
                    if item.kind() == "namespace_use_clause" {
                        let k = clause_kind(text(item), kind);
                        out.extend(clause(item, src, &prefix, k));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn clause(node: Node, src: &str, prefix: &str, kind: UseKind) -> Option<Clause> {
    let mut cursor = node.walk();
    let names: Vec<Node> = node
        .named_children(&mut cursor)
        .filter(|n| matches!(n.kind(), "name" | "qualified_name"))
        .collect();
    let name = src[names.first()?.byte_range()].trim_start_matches('\\');
    let alias = names.get(1).map(|a| src[a.byte_range()].to_string());
    let fqn = if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}\\{name}")
    };
    Some(Clause { kind, fqn, alias })
}

/// Consecutive `use` statements separated only by whitespace.
fn import_blocks<'t>(list: Node<'t>, src: &str, out: &mut Vec<Vec<Node<'t>>>) {
    let mut cursor = list.walk();
    let mut current: Vec<Node<'t>> = Vec::new();
    for child in list.named_children(&mut cursor) {
        match child.kind() {
            "namespace_use_declaration" => {
                let adjacent = current
                    .last()
                    .is_none_or(|prev| src[prev.end_byte()..child.start_byte()].trim().is_empty());
                if !adjacent {
                    out.push(std::mem::take(&mut current));
                }
                current.push(child);
            }
            "namespace_definition" => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
                if let Some(body) = child.child_by_field_name("body") {
                    import_blocks(body, src, out);
                }
            }
            _ => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
}

fn line_start(src: &str, at: usize) -> usize {
    src[..at].rfind('\n').map_or(0, |i| i + 1)
}

fn rewrite_imports(src: &str, sort: bool) -> Option<String> {
    let tree = new_parser().parse(src, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }
    let mut usages = Usages::default();
    usages.collect(root, src);
    let mut blocks = Vec::new();
    import_blocks(root, src, &mut blocks);

    // (start, end, replacement), applied back to front.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for block in &blocks {
        let (first, last) = (block[0], block[block.len() - 1]);
        let all: Vec<Clause> = block.iter().flat_map(|d| clauses_of(*d, src)).collect();
        let mut kept: Vec<Clause> = Vec::new();
        for c in all.iter().filter(|c| usages.uses(c)) {
            let dup = kept.iter().any(|k| {
                k.kind == c.kind && k.fqn.eq_ignore_ascii_case(&c.fqn) && k.alias == c.alias
            });
            if !dup {
                kept.push(c.clone());
            }
        }
        if sort {
            kept.sort_by(|a, b| {
                a.kind_rank()
                    .cmp(&b.kind_rank())
                    .then_with(|| a.fqn.to_lowercase().cmp(&b.fqn.to_lowercase()))
                    .then_with(|| a.alias.cmp(&b.alias))
            });
        }
        if kept == all && !sort {
            continue;
        }
        let start = first.start_byte();
        let end = last.end_byte();
        if kept.is_empty() {
            edits.push(remove_lines(src, start, end));
            continue;
        }
        let ls = line_start(src, start);
        let indent = &src[ls..start];
        let indent = if indent.trim().is_empty() { indent } else { "" };
        let text = kept
            .iter()
            .map(Clause::render)
            .collect::<Vec<_>>()
            .join(&format!("\n{indent}"));
        if text != src[start..end] {
            edits.push((start, end, text));
        }
    }
    if edits.is_empty() {
        return None;
    }
    Some(apply_edits(src, edits))
}

/// Apply non-overlapping `(start, end, text)` edits.
pub(crate) fn apply_edits(src: &str, mut edits: Vec<(usize, usize, String)>) -> String {
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut out = src.to_string();
    for (start, end, text) in edits {
        out.replace_range(start..end, &text);
    }
    out
}

/// An edit removing `start..end` together with its lines when nothing else is on them, and
/// one of the blank lines around it so no double blank line is left behind.
pub(crate) fn remove_lines(src: &str, start: usize, end: usize) -> (usize, usize, String) {
    let ls = line_start(src, start);
    let le = src[end..].find('\n').map_or(src.len(), |i| end + i);
    let before_blank = src[ls..start].trim().is_empty();
    let after_blank = src[end..le].trim().is_empty();
    if !(before_blank && after_blank) {
        return (start, end, String::new());
    }
    // The whole lines including the line break after them.
    let mut to = (le + 1).min(src.len());
    let prev_line_blank = ls == 0 || {
        let pls = line_start(src, ls - 1);
        src[pls..ls - 1].trim().is_empty()
    };
    let next_le = src[to..].find('\n').map(|i| to + i);
    let next_line_blank = next_le.is_some_and(|nle| src[to..nle].trim().is_empty());
    if prev_line_blank && next_line_blank {
        to = next_le.map_or(src.len(), |nle| nle + 1);
    }
    (ls, to, String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_unused_and_sorts() {
        let src = "<?php\n\nnamespace App;\n\nuse Zed\\Last;\nuse Foo\\Unused;\nuse Acme\\First;\nuse function Acme\\helper;\nuse const Acme\\LIMIT;\n\nclass A extends First\n{\n    public function x(Last $l): int\n    {\n        helper();\n        return LIMIT;\n    }\n}\n";
        let out = optimize_imports(src).unwrap();
        assert_eq!(
            out,
            "<?php\n\nnamespace App;\n\nuse Acme\\First;\nuse Zed\\Last;\nuse function Acme\\helper;\nuse const Acme\\LIMIT;\n\nclass A extends First\n{\n    public function x(Last $l): int\n    {\n        helper();\n        return LIMIT;\n    }\n}\n"
        );
    }

    #[test]
    fn unchanged_file_gives_none() {
        let src = "<?php\nuse A\\B;\nuse C\\D;\n\nnew B(); new D();\n";
        assert_eq!(optimize_imports(src), None);
        assert_eq!(remove_unused_imports(src), None);
    }

    #[test]
    fn cleanup_keeps_the_order() {
        let src = "<?php\nuse Z\\B;\nuse X\\Gone;\nuse A\\C;\n\nnew B(); new C();\n";
        assert_eq!(
            remove_unused_imports(src).unwrap(),
            "<?php\nuse Z\\B;\nuse A\\C;\n\nnew B(); new C();\n"
        );
    }

    #[test]
    fn docblocks_attributes_and_qualified_names_count_as_usages() {
        let src = "<?php\nuse Doctrine\\ORM\\Mapping as ORM;\nuse App\\Models\\User;\nuse App\\Attr\\Route;\nuse Vendor\\Sub;\nuse App\\Unused;\n\n#[Route('/')]\nclass C {\n    /** @ORM\\Column @var User */\n    private $u;\n    public function f() { return new Sub\\Thing(); }\n}\n";
        let out = optimize_imports(src).unwrap();
        assert!(out.contains("use Doctrine\\ORM\\Mapping as ORM;"));
        assert!(out.contains("use App\\Models\\User;"));
        assert!(out.contains("use App\\Attr\\Route;"));
        assert!(out.contains("use Vendor\\Sub;"));
        assert!(!out.contains("Unused"));
    }

    #[test]
    fn group_and_multi_clause_uses_are_expanded() {
        let src = "<?php\nuse Foo\\{B, A as Alias, Gone};\nuse X\\Y, X\\Z;\n\nnew B(); new Alias(); new Z();\n";
        assert_eq!(
            optimize_imports(src).unwrap(),
            "<?php\nuse Foo\\A as Alias;\nuse Foo\\B;\nuse X\\Z;\n\nnew B(); new Alias(); new Z();\n"
        );
    }

    #[test]
    fn removes_a_block_without_leaving_blank_lines() {
        let src = "<?php\n\nnamespace App;\n\nuse Gone\\One;\nuse Gone\\Two;\n\nclass A {}\n";
        assert_eq!(
            optimize_imports(src).unwrap(),
            "<?php\n\nnamespace App;\n\nclass A {}\n"
        );
    }

    #[test]
    fn class_names_are_case_insensitive_constants_are_not() {
        let src = "<?php\nuse App\\User;\nuse const App\\MAX;\n\nnew user(); echo max;\n";
        assert_eq!(
            optimize_imports(src).unwrap(),
            "<?php\nuse App\\User;\n\nnew user(); echo max;\n"
        );
    }

    #[test]
    fn duplicates_are_dropped() {
        let src = "<?php\nuse A\\B;\nuse A\\B;\n\nnew B();\n";
        assert_eq!(optimize_imports(src).unwrap(), "<?php\nuse A\\B;\n\nnew B();\n");
    }

    #[test]
    fn braced_namespaces_are_handled_separately() {
        let src = "<?php\nnamespace A {\n    use X\\Q;\n    use X\\P;\n    new P(); new Q();\n}\nnamespace B {\n    use X\\R;\n}\n";
        assert_eq!(
            optimize_imports(src).unwrap(),
            "<?php\nnamespace A {\n    use X\\P;\n    use X\\Q;\n    new P(); new Q();\n}\nnamespace B {\n}\n"
        );
    }

    #[test]
    fn files_with_syntax_errors_are_not_touched() {
        let src = "<?php\nuse A\\Gone;\nclass {\n";
        assert_eq!(optimize_imports(src), None);
    }

    #[test]
    fn variables_do_not_keep_imports() {
        let src = "<?php\nuse App\\Thing;\n\n$thing = 1;\n";
        assert_eq!(optimize_imports(src).unwrap(), "<?php\n\n$thing = 1;\n");
    }
}
