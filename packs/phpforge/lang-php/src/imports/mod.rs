//! Optimize Imports (Ctrl+Alt+O) for PHP: removes `use` imports that the file does not
//! reference and arranges the rest by the code style's Imports options (sorted
//! alphabetically or by length, grouped by kind or top-level namespace, blank lines between
//! groups; see [`style`]), like PHPStorm. Code Cleanup and "Optimize imports on the fly"
//! remove unused imports without rearranging the others.
//!
//! The analysis is conservative: a name counts as used when it appears anywhere in the code
//! (types, `new`, static calls, attributes, function calls, constants) or in a comment
//! (docblock types and annotations such as `@ORM\Column`), so an import is only removed
//! when nothing can refer to it. Files with syntax errors are left alone.
//!
//! - [`style`]: the options and the scoped style of a call.
//! - [`insert`]: the one helper that adds `use` statements (Import class, completion,
//!   Create from usage, Move, Copy, Pull Members Up).
//! - [`auto`]: Auto Import: unresolved class names and their candidates, the "Import class"
//!   quick fix, completion, the import popup and the on-the-fly decisions.

pub mod auto;
pub mod insert;
pub mod style;

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use crate::model::UseKind;
use crate::parser::new_parser;
pub use insert::{insert_imports, NewImport};
pub use style::{with_style, ImportGrouping, ImportSort, ImportStyle};

/// Remove unused imports and arrange the remaining ones with the current style
/// ([`style::current`]). `None` when nothing changes.
pub fn optimize_imports(src: &str) -> Option<String> {
    optimize_imports_with(src, &style::current()).text
}

/// Remove unused imports, keeping the order of the others. `None` when nothing changes.
pub fn remove_unused_imports(src: &str) -> Option<String> {
    let edits = import_edits(src, Mode::RemoveUnused, &style::current(), &|_, _| false)?;
    (!edits.edits.is_empty()).then(|| apply_edits(src, edits.edits))
}

/// What Optimize Imports did to one file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeReport {
    /// The new text, `None` when nothing changes (or the file has syntax errors).
    pub text: Option<String>,
    /// Imports removed (unused or duplicate).
    pub removed: usize,
    /// The order or the grouping of the kept imports changed.
    pub rearranged: bool,
}

impl OptimizeReport {
    /// PHPStorm's notification: "Removed 2 imports, rearranged imports".
    pub fn message(&self) -> String {
        summary(self.removed, self.rearranged, 0)
    }
}

/// "Removed 2 imports, added 1 import, rearranged imports" / "Imports are already optimized".
pub fn summary(removed: usize, rearranged: bool, added: usize) -> String {
    let plural = |n: usize| if n == 1 { "import" } else { "imports" };
    let mut parts = Vec::new();
    if removed > 0 {
        parts.push(format!("removed {removed} {}", plural(removed)));
    }
    if added > 0 {
        parts.push(format!("added {added} {}", plural(added)));
    }
    if rearranged {
        parts.push("rearranged imports".to_string());
    }
    if parts.is_empty() {
        return "Imports are already optimized".into();
    }
    let text = parts.join(", ");
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => text,
    }
}

/// Optimize Imports with an explicit style, with what it changed.
pub fn optimize_imports_with(src: &str, style: &ImportStyle) -> OptimizeReport {
    let Some(outcome) = import_edits(src, Mode::Optimize, style, &|_, _| false) else {
        return OptimizeReport::default();
    };
    OptimizeReport {
        text: (!outcome.edits.is_empty()).then(|| apply_edits(src, outcome.edits)),
        removed: outcome.removed.len(),
        rearranged: outcome.rearranged,
    }
}

/// One imported name of a `use` statement (group uses are expanded).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Clause {
    pub kind: UseKind,
    pub fqn: String,
    pub alias: Option<String>,
}

impl Clause {
    pub(crate) fn local_name(&self) -> &str {
        self.alias
            .as_deref()
            .unwrap_or_else(|| self.fqn.rsplit('\\').next().unwrap_or(&self.fqn))
    }

    pub(crate) fn render(&self) -> String {
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
        let first = name
            .trim_start_matches('\\')
            .split('\\')
            .next()
            .unwrap_or("");
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Remove unused imports and arrange the rest.
    Optimize,
    /// Remove unused imports only.
    RemoveUnused,
}

/// The edits of [`import_edits`] and what they do.
#[derive(Debug, Default)]
pub(crate) struct ImportEdits {
    /// `(start, end, text)` in bytes.
    pub edits: Vec<(usize, usize, String)>,
    /// Removed imports (fully qualified names).
    pub removed: Vec<String>,
    pub rearranged: bool,
}

/// One `use` block as Optimize Imports sees it (for the on-the-fly decisions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BlockSpan {
    pub start: usize,
    pub end: usize,
}

/// The `use` blocks of a file (byte ranges of the statements), empty with syntax errors.
pub(crate) fn block_spans(src: &str) -> Vec<BlockSpan> {
    let Some(tree) = new_parser().parse(src, None) else {
        return vec![];
    };
    let root = tree.root_node();
    if root.has_error() {
        return vec![];
    }
    let mut blocks = Vec::new();
    import_blocks(root, src, &mut blocks);
    blocks
        .iter()
        .map(|b| BlockSpan {
            start: b[0].start_byte(),
            end: b[b.len() - 1].end_byte(),
        })
        .collect()
}

/// The edits removing unused imports (and arranging the rest in [`Mode::Optimize`]).
/// `keep` protects imports from removal (on the fly: the one the user is typing a usage
/// of). `None` for files with syntax errors.
pub(crate) fn import_edits(
    src: &str,
    mode: Mode,
    style: &ImportStyle,
    keep: &dyn Fn(&str, usize) -> bool,
) -> Option<ImportEdits> {
    let tree = new_parser().parse(src, None)?;
    let root = tree.root_node();
    if root.has_error() {
        return None;
    }
    let mut usages = Usages::default();
    usages.collect(root, src);
    let mut blocks = Vec::new();
    import_blocks(root, src, &mut blocks);

    let mut out = ImportEdits::default();
    for block in &blocks {
        let (first, last) = (block[0], block[block.len() - 1]);
        let (start, end) = (first.start_byte(), last.end_byte());
        // Per statement: its clauses and whether each one stays.
        let mut kept: Vec<Clause> = Vec::new();
        let mut decls: Vec<(Node, Vec<Clause>, Vec<bool>)> = Vec::new();
        for decl in block {
            let clauses = clauses_of(*decl, src);
            let mut stays = Vec::with_capacity(clauses.len());
            for c in &clauses {
                let used = usages.uses(c) || keep(c.local_name(), start);
                let dup = kept.iter().any(|k| {
                    k.kind == c.kind && k.fqn.eq_ignore_ascii_case(&c.fqn) && k.alias == c.alias
                });
                if used && !dup {
                    kept.push(c.clone());
                } else {
                    out.removed.push(c.fqn.clone());
                }
                stays.push(used && !dup);
            }
            decls.push((*decl, clauses, stays));
        }
        let removed_here = decls.iter().any(|(_, _, stays)| stays.iter().any(|s| !s));
        if kept.is_empty() {
            out.edits.push(remove_lines(src, start, end));
            continue;
        }
        let ls = line_start(src, start);
        let indent = &src[ls..start];
        let indent = if indent.trim().is_empty() { indent } else { "" };
        match mode {
            Mode::RemoveUnused => {
                if !removed_here {
                    continue;
                }
                // Statement by statement, so the others keep their layout.
                for (decl, clauses, stays) in &decls {
                    if stays.iter().all(|s| *s) {
                        continue;
                    }
                    let (ds, de) = (decl.start_byte(), decl.end_byte());
                    if stays.iter().all(|s| !s) {
                        out.edits.push(remove_lines(src, ds, de));
                        continue;
                    }
                    let text = clauses
                        .iter()
                        .zip(stays)
                        .filter(|(_, s)| **s)
                        .map(|(c, _)| c.render())
                        .collect::<Vec<_>>()
                        .join(&format!("\n{indent}"));
                    out.edits.push((ds, de, text));
                }
            }
            Mode::Optimize => {
                let arranged = arrange(&kept, style);
                let order_changed = arranged.iter().flatten().ne(kept.iter());
                let text = arranged
                    .iter()
                    .map(|group| {
                        group
                            .iter()
                            .map(Clause::render)
                            .collect::<Vec<_>>()
                            .join(&format!("\n{indent}"))
                    })
                    .collect::<Vec<_>>()
                    .join(&format!("{}{indent}", style.group_separator()));
                if text != src[start..end] {
                    out.rearranged |= order_changed || !removed_here;
                    out.edits.push((start, end, text));
                }
            }
        }
    }
    Some(out)
}

/// The kept imports of a block in groups, each sorted by the style.
fn arrange(kept: &[Clause], style: &ImportStyle) -> Vec<Vec<Clause>> {
    let mut groups: Vec<((u8, String), Vec<Clause>)> = Vec::new();
    for c in kept {
        let key = style.group_of(c.kind, &c.fqn);
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, g)) => g.push(c.clone()),
            None => groups.push((key, vec![c.clone()])),
        }
    }
    groups.sort_by(|a, b| a.0.cmp(&b.0));
    groups
        .into_iter()
        .map(|(_, mut g)| {
            // Stable: "Don't sort" keeps the order.
            g.sort_by(|a, b| {
                style
                    .compare(&a.fqn, &a.render(), &b.fqn, &b.render())
                    .then_with(|| match style.sort {
                        ImportSort::None => std::cmp::Ordering::Equal,
                        _ => a.alias.cmp(&b.alias),
                    })
            });
            g
        })
        .collect()
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
        assert_eq!(
            optimize_imports(src).unwrap(),
            "<?php\nuse A\\B;\n\nnew B();\n"
        );
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
    fn optimize_follows_the_code_style() {
        let src = "<?php\nnamespace App;\n\nuse Symfony\\Yaml;\nuse function App\\helper;\nuse App\\Models\\User;\nuse Exception;\nuse App\\Gone;\nuse Illuminate\\Support\\Str;\n\nhelper(new User(), new Exception(), new Yaml(), Str::of(''));\n";
        let grouped = ImportStyle {
            group_by: ImportGrouping::Namespace,
            blank_lines_between_groups: 1,
            ..Default::default()
        };
        let report = optimize_imports_with(src, &grouped);
        assert_eq!(
            report.text.as_deref().unwrap(),
            "<?php\nnamespace App;\n\nuse Exception;\n\nuse App\\Models\\User;\n\nuse Illuminate\\Support\\Str;\n\nuse Symfony\\Yaml;\n\nuse function App\\helper;\n\nhelper(new User(), new Exception(), new Yaml(), Str::of(''));\n"
        );
        assert_eq!(report.removed, 1);
        assert!(report.rearranged);
        assert_eq!(report.message(), "Removed 1 import, rearranged imports");
        // Optimizing again changes nothing.
        let again = optimize_imports_with(report.text.as_deref().unwrap(), &grouped);
        assert_eq!(again.text, None);
        assert_eq!(again.message(), "Imports are already optimized");

        let by_length = ImportStyle {
            sort: ImportSort::ByLength,
            group_by: ImportGrouping::None,
            ..Default::default()
        };
        assert_eq!(
            optimize_imports_with(src, &by_length).text.unwrap(),
            "<?php\nnamespace App;\n\nuse Exception;\nuse Symfony\\Yaml;\nuse App\\Models\\User;\nuse function App\\helper;\nuse Illuminate\\Support\\Str;\n\nhelper(new User(), new Exception(), new Yaml(), Str::of(''));\n"
        );
        // The scoped style is what `optimize_imports` (Actions on Save) uses.
        assert_eq!(
            with_style(by_length, || optimize_imports(src)),
            optimize_imports_with(src, &by_length).text
        );
    }

    #[test]
    fn removing_only_keeps_the_layout_of_the_others() {
        let src = "<?php\nuse B\\Two;\n\nuse A\\{One, Gone};\nuse function f\\gone;\n\nnew Two(); new One();\n";
        let report = optimize_imports_with(src, &ImportStyle::default());
        assert_eq!(report.removed, 2);
        assert_eq!(
            remove_unused_imports(src).unwrap(),
            "<?php\nuse B\\Two;\n\nuse A\\One;\n\nnew Two(); new One();\n"
        );
    }

    #[test]
    fn summaries() {
        assert_eq!(summary(2, false, 0), "Removed 2 imports");
        assert_eq!(summary(0, true, 1), "Added 1 import, rearranged imports");
        assert_eq!(summary(0, true, 0), "Rearranged imports");
    }

    #[test]
    fn variables_do_not_keep_imports() {
        let src = "<?php\nuse App\\Thing;\n\n$thing = 1;\n";
        assert_eq!(optimize_imports(src).unwrap(), "<?php\n\n$thing = 1;\n");
    }
}
