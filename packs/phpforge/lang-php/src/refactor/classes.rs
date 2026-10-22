//! Class names in PHP code for the refactorings that move or copy classes: where a file
//! names a class (type hints, `new`, `::`, `extends`, attributes, PHPDoc types), how a
//! name resolves through the namespace and the `use` imports, and the edits that add
//! imports.

use std::collections::HashSet;

use tree_sitter::Node;

use super::{text, Span};
use crate::generate::TextEdit;
use crate::model::{FileIndex, UseKind};
use crate::resolve::is_builtin_type;

/// A class name written in code or in a PHPDoc type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassName {
    pub span: Span,
    /// As written (`User`, `Models\User`, `\App\Models\User`).
    pub text: String,
    /// In a PHPDoc comment.
    pub doc: bool,
}

impl ClassName {
    pub fn is_fully_qualified(&self) -> bool {
        self.text.starts_with('\\')
    }

    pub fn is_bare(&self) -> bool {
        !self.text.contains('\\')
    }
}

/// `App\Models\User` -> `App\Models`.
pub fn namespace_of(fqn: &str) -> &str {
    fqn.trim_start_matches('\\')
        .rsplit_once('\\')
        .map_or("", |(ns, _)| ns)
}

/// `ns` + `\` + `name` (just `name` in the global namespace).
pub fn qualify(ns: &str, name: &str) -> String {
    let ns = ns.trim_matches('\\');
    if ns.is_empty() {
        name.to_string()
    } else {
        format!("{ns}\\{name}")
    }
}

/// Names that are not classes: `self`, `static`, `parent` and the built-in types.
pub fn is_special(name: &str) -> bool {
    is_builtin_type(name)
        || matches!(
            name.to_ascii_lowercase().as_str(),
            "self" | "static" | "parent" | "namespace"
        )
}

fn is_name(n: Node) -> bool {
    matches!(n.kind(), "name" | "qualified_name")
}

/// Whether the `name` / `qualified_name` node `n` names a class.
fn is_class_context(n: Node) -> bool {
    let Some(p) = n.parent() else { return false };
    match p.kind() {
        "named_type" | "base_clause" | "class_interface_clause" | "use_declaration" => true,
        "object_creation_expression" => p.named_child(0) == Some(n),
        "scoped_call_expression" | "scoped_property_access_expression" => {
            p.child_by_field_name("scope") == Some(n)
        }
        "class_constant_access_expression" => p.named_child(0) == Some(n),
        "binary_expression" => {
            p.child_by_field_name("right") == Some(n)
                && p.child_by_field_name("operator")
                    .is_some_and(|o| o.kind() == "instanceof")
        }
        "attribute" => p.named_child(0) == Some(n),
        "type_list" => true,
        _ => false,
    }
}

/// Every class name in the code of `root` (not in `use` statements, not declarations).
pub fn code_class_names(root: Node, src: &str) -> Vec<ClassName> {
    let mut out = Vec::new();
    fn walk(n: Node, src: &str, out: &mut Vec<ClassName>) {
        if matches!(
            n.kind(),
            "namespace_use_declaration" | "namespace_definition"
        ) {
            // A braced namespace's body is still code.
            if let Some(body) = n.child_by_field_name("body") {
                walk(body, src, out);
            }
            return;
        }
        if is_name(n) && is_class_context(n) {
            let t = text(n, src);
            if !is_special(t) {
                out.push(ClassName {
                    span: Span::of(n),
                    text: t.to_string(),
                    doc: false,
                });
            }
            return;
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            walk(ch, src, out);
        }
    }
    walk(root, src, &mut out);
    out
}

/// PHPDoc tags whose first word is a type.
const TYPE_TAGS: &[&str] = &[
    "param",
    "var",
    "return",
    "throws",
    "property",
    "property-read",
    "property-write",
    "method",
    "mixin",
    "see",
    "extends",
    "implements",
    "use",
    "phpstan-param",
    "phpstan-return",
    "phpstan-var",
    "psalm-param",
    "psalm-return",
    "psalm-var",
];

/// Template names declared in PHPDoc (`@template T`): not classes.
pub fn template_names(src: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for (i, _) in src.match_indices("@template") {
        let rest = &src[i + "@template".len()..];
        let rest = rest.trim_start_matches(|c: char| c.is_alphanumeric() || c == '-');
        if let Some(name) = rest.split_whitespace().next() {
            out.insert(name.to_string());
        }
    }
    for (i, _) in src.match_indices("-template") {
        let rest = &src[i + "-template".len()..];
        if let Some(name) = rest.split_whitespace().next() {
            out.insert(name.to_string());
        }
    }
    out
}

/// The type expression at the start of `s`: up to the first whitespace outside brackets.
fn type_expression(s: &str) -> &str {
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '<' | '(' | '{' | '[' => depth += 1,
            '>' | ')' | '}' | ']' => depth -= 1,
            c if c.is_whitespace() && depth <= 0 => return &s[..i],
            _ => {}
        }
    }
    s
}

/// Class names in the types of a PHPDoc comment (`comment` starts at `base` in the file).
pub fn doc_class_names(comment: &str, base: usize, templates: &HashSet<String>) -> Vec<ClassName> {
    let mut out = Vec::new();
    if !comment.starts_with("/**") {
        return out;
    }
    for (at, _) in comment.match_indices('@') {
        let rest = &comment[at + 1..];
        let tag_len = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '-'))
            .unwrap_or(rest.len());
        let tag = &rest[..tag_len];
        if !TYPE_TAGS.contains(&tag) {
            continue;
        }
        let mut pos = at + 1 + tag_len;
        // Only blanks on the same line before the type.
        let after = &comment[pos..];
        let skip = after.len() - after.trim_start_matches([' ', '\t']).len();
        pos += skip;
        let mut ty = type_expression(&comment[pos..]);
        if tag == "method" && ty == "static" {
            let after = &comment[pos + ty.len()..];
            let skip = after.len() - after.trim_start_matches([' ', '\t']).len();
            pos += ty.len() + skip;
            ty = type_expression(&comment[pos..]);
            // `@method foo()` without a return type.
            if ty.contains('(') && !ty.starts_with('(') {
                continue;
            }
        }
        if tag == "see" || tag == "mixin" {
            ty = ty.split("::").next().unwrap_or(ty);
        }
        let ty = ty.trim_end_matches(['.', ',', '*']);
        let is_char = |c: char| c.is_alphanumeric() || c == '_' || c == '\\';
        let mut i = 0;
        let bytes: Vec<(usize, char)> = ty.char_indices().collect();
        while i < bytes.len() {
            let (start, c) = bytes[i];
            if !is_char(c) {
                i += 1;
                continue;
            }
            let mut j = i;
            while j < bytes.len() && is_char(bytes[j].1) {
                j += 1;
            }
            let end = bytes.get(j).map_or(ty.len(), |b| b.0);
            let word = &ty[start..end];
            let before = ty[..start].chars().next_back();
            let after = ty[end..].chars().next();
            i = j;
            let first = word.trim_start_matches('\\').chars().next();
            if before.is_some_and(|c| c == '$' || c == '-' || c == ':')
                || after.is_some_and(|c| c == '-' || c == ':' || c == '(')
                || is_special(word)
                || templates.contains(word)
                || word.chars().all(|c| c.is_ascii_digit())
                || !(word.contains('\\') || first.is_some_and(char::is_uppercase))
            {
                continue;
            }
            out.push(ClassName {
                span: Span::new(base + pos + start, base + pos + end),
                text: word.to_string(),
                doc: true,
            });
        }
    }
    out
}

/// Class names in the PHPDoc comments of `root`.
pub fn all_doc_class_names(root: Node, src: &str) -> Vec<ClassName> {
    let templates = template_names(src);
    let comments = super::collect_nodes(root, true, &mut |n| n.kind() == "comment");
    comments
        .into_iter()
        .flat_map(|c| doc_class_names(text(c, src), c.start_byte(), &templates))
        .collect()
}

/// Whether a bare class name is imported (`use ... as Name`) in `file`.
pub fn imported(file: &FileIndex, name: &str) -> bool {
    let first = name.split('\\').next().unwrap_or(name);
    file.uses
        .iter()
        .any(|u| u.kind == UseKind::Class && u.local_name().eq_ignore_ascii_case(first))
}

/// The fully qualified class `fqn` is imported under its own short name.
pub fn imports_fqn(file: &FileIndex, fqn: &str) -> bool {
    file.uses.iter().any(|u| {
        u.kind == UseKind::Class
            && u.fqn.eq_ignore_ascii_case(fqn.trim_start_matches('\\'))
            && u.alias.is_none()
    })
}

/// Edits adding `use <fqn>;` for every class in `fqns` (sorted, skipping ones the file
/// already imports), inserted where Create class puts imports.
pub fn add_imports(file: &FileIndex, src: &str, fqns: &[String]) -> Vec<TextEdit> {
    let mut list: Vec<&String> = Vec::new();
    for f in fqns {
        let f_trim = f.trim_start_matches('\\');
        if imports_fqn(file, f_trim)
            || list
                .iter()
                .any(|l| l.trim_start_matches('\\').eq_ignore_ascii_case(f_trim))
        {
            continue;
        }
        list.push(f);
    }
    list.sort_by_key(|f| f.to_ascii_lowercase());
    let mut edits: Vec<TextEdit> = Vec::new();
    for f in list {
        let e = crate::intentions::add_import(file, src, f.trim_start_matches('\\'));
        match edits
            .iter_mut()
            .find(|x| x.start == e.start && x.end == e.end)
        {
            // Several imports at one place: one edit, in order.
            Some(x) => {
                if x.new_text.ends_with('\n') && !x.new_text.starts_with('\n') {
                    x.new_text.push_str(&e.new_text);
                } else {
                    let line = e.new_text.trim_start_matches('\n');
                    x.new_text.push('\n');
                    x.new_text.push_str(line.trim_end_matches('\n'));
                }
            }
            None => edits.push(e),
        }
    }
    edits
}

/// Sort edits, drop ones that overlap an earlier one (two passes reached the same name) and
/// merge inserts at one place into one edit (in their order), so every way of applying
/// them gives the same text.
pub fn tidy(mut edits: Vec<TextEdit>) -> Vec<TextEdit> {
    edits.sort_by_key(|e| (e.start, e.end));
    let mut out: Vec<TextEdit> = Vec::with_capacity(edits.len());
    for e in edits {
        if let Some(last) = out.last_mut() {
            if last.start == last.end && e.start == last.start {
                if e.start == e.end && e.new_text == last.new_text {
                    continue;
                }
                last.end = e.end;
                last.new_text.push_str(&e.new_text);
                continue;
            }
            if e.start < last.end || (e.start == last.start && e.end == last.end) {
                continue;
            }
        }
        out.push(e);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::index_source;
    use crate::refactor::parse;

    #[test]
    fn finds_class_names_in_code() {
        let src = "<?php\nnamespace App;\nuse X\\Y;\n#[Attr]\nclass A extends B implements C, \\D\\E {\n  use T;\n  public function f(F $f, ?G $g): H|I {\n    new J(); K::m(); L::C; M::$p;\n    $a instanceof N;\n    try {} catch (O | P $e) {}\n    return self::x();\n  }\n}\n";
        let tree = parse(src).unwrap();
        let names: Vec<String> = code_class_names(tree.root_node(), src)
            .into_iter()
            .map(|n| n.text)
            .collect();
        assert_eq!(
            names,
            [
                "Attr", "B", "C", "\\D\\E", "T", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O",
                "P"
            ]
        );
    }

    #[test]
    fn finds_class_names_in_docs() {
        let src = "/**\n * @param Order $o\n * @return Collection<int, Order|\\App\\Item>\n * @template T\n * @var T[] list<string> \n * @method static User find(int $id)\n * @see Checkout::run()\n * @property-read non-empty-string $x\n */";
        let names: Vec<String> = doc_class_names(src, 0, &template_names(src))
            .into_iter()
            .map(|n| n.text)
            .collect();
        assert_eq!(
            names,
            [
                "Order",
                "Collection",
                "Order",
                "\\App\\Item",
                "User",
                "Checkout"
            ]
        );
        // Spans point into the text.
        let first = &doc_class_names(src, 10, &HashSet::new())[0];
        assert_eq!(&src[first.span.start - 10..first.span.end - 10], "Order");
    }

    #[test]
    fn adds_imports_once_and_sorted() {
        let src = "<?php\n\nnamespace App;\n\nuse App\\B;\n\nclass A {}\n";
        let file = index_source("a.php", src);
        let edits = add_imports(
            &file,
            src,
            &[
                "App\\Z".into(),
                "App\\B".into(),
                "App\\C".into(),
                "App\\z".into(),
            ],
        );
        let out = crate::generate::apply_edits(src, &edits);
        assert_eq!(
            out,
            "<?php\n\nnamespace App;\n\nuse App\\B;\nuse App\\C;\nuse App\\Z;\n\nclass A {}\n"
        );
        let src = "<?php\n\nnamespace App;\n\nclass A {}\n";
        let file = index_source("a.php", src);
        let out = crate::generate::apply_edits(
            src,
            &add_imports(&file, src, &["X\\B".into(), "X\\A".into()]),
        );
        assert_eq!(
            out,
            "<?php\n\nnamespace App;\n\nuse X\\A;\nuse X\\B;\n\nclass A {}\n"
        );
    }
}
