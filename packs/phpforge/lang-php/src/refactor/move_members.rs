//! Move Members (F6 on a static member) like PHPStorm: static methods, static properties
//! and constants move to another class. The members leave their class and are added to
//! the target class; `Source::member` references become `Target::member` (`self::` inside
//! the target), `self::` references inside the moved code to members that stay become
//! `Source::`, and class names in the moved code are fully qualified where the target
//! file would resolve them differently.

use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use super::classes::{all_doc_class_names, code_class_names, tidy};
use super::make_static::class_reference;
use super::move_class::with_doc;
use super::{parse, text, Span};
use crate::generate::TextEdit;
use crate::imports::remove_lines;
use crate::parser::index_source;

/// A member that can move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovableMember {
    /// `orders`, `count` (no `$`), `MAX`.
    pub name: String,
    /// `method`, `property` or `constant`.
    pub kind: String,
    /// `public static function make(): self`.
    pub signature: String,
    pub visibility: String,
    /// With its PHPDoc.
    pub span: Span,
    /// The name (without `$`), where Find Usages starts.
    pub name_span: Span,
}

/// The Move Members dialog's data: the class and its static members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MembersInfo {
    pub class_name: String,
    pub class_fqn: String,
    pub class_span: Span,
    pub members: Vec<MovableMember>,
    /// The member at the caret.
    pub selected: Option<String>,
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn visibility(n: Node, src: &str) -> String {
    let mut c = n.walk();
    let v = n
        .children(&mut c)
        .find(|ch| ch.kind() == "visibility_modifier")
        .map(|v| text(v, src).to_ascii_lowercase());
    v.unwrap_or_else(|| "public".into())
}

fn has_static(n: Node) -> bool {
    let mut c = n.walk();
    let found = n.children(&mut c).any(|ch| ch.kind() == "static_modifier");
    found
}

/// The static members of the class at `pos` (a static member at the caret is selected).
pub fn members_at(src: &str, pos: usize) -> Option<MembersInfo> {
    let tree = parse(src)?;
    let root = tree.root_node();
    let class = super::move_class::class_at(src, pos)?;
    let node = root
        .descendant_for_byte_range(class.name_span.start, class.name_span.end)?
        .parent()?;
    let body = node.child_by_field_name("body")?;
    let mut members = Vec::new();
    let mut selected = None;
    let mut c = body.walk();
    for m in body.named_children(&mut c) {
        let span = with_doc(m, src);
        let at_caret = m.start_byte() <= pos && pos <= m.end_byte();
        let found: Vec<MovableMember> = match m.kind() {
            "method_declaration" if has_static(m) => {
                let Some(name) = m.child_by_field_name("name") else {
                    continue;
                };
                let head_end = m
                    .child_by_field_name("body")
                    .map_or(m.end_byte(), |b| b.start_byte());
                vec![MovableMember {
                    name: text(name, src).to_string(),
                    kind: "method".into(),
                    signature: one_line(src[m.start_byte()..head_end].trim()),
                    visibility: visibility(m, src),
                    span,
                    name_span: Span::of(name),
                }]
            }
            "property_declaration" if has_static(m) => {
                let mut c2 = m.walk();
                let els: Vec<Node> = m
                    .named_children(&mut c2)
                    .filter(|e| e.kind() == "property_element")
                    .collect();
                if els.len() != 1 {
                    continue;
                }
                let Some(name) = els[0].child_by_field_name("name") else {
                    continue;
                };
                vec![MovableMember {
                    name: text(name, src).trim_start_matches('$').to_string(),
                    kind: "property".into(),
                    signature: one_line(text(m, src).trim_end_matches(';')),
                    visibility: visibility(m, src),
                    span,
                    name_span: Span::new(name.start_byte() + 1, name.end_byte()),
                }]
            }
            "const_declaration" => {
                let mut c2 = m.walk();
                let els: Vec<Node> = m
                    .named_children(&mut c2)
                    .filter(|e| e.kind() == "const_element")
                    .collect();
                if els.len() != 1 {
                    continue;
                }
                let mut c3 = els[0].walk();
                let Some(name) = els[0].named_children(&mut c3).find(|x| x.kind() == "name") else {
                    continue;
                };
                vec![MovableMember {
                    name: text(name, src).to_string(),
                    kind: "constant".into(),
                    signature: one_line(text(m, src).trim_end_matches(';')),
                    visibility: visibility(m, src),
                    span,
                    name_span: Span::of(name),
                }]
            }
            _ => vec![],
        };
        if at_caret {
            selected = found.first().map(|m| m.name.clone());
        }
        members.extend(found);
    }
    Some(MembersInfo {
        class_name: class.name,
        class_fqn: class.fqn,
        class_span: class.span,
        members,
        selected,
    })
}

/// The scope node of a static access to a member whose name is at `span` (`Foo` of
/// `Foo::bar()`, `Foo::$x`, `Foo::X`).
fn scope_of(root: Node, span: Span) -> Option<Node> {
    let mut n = root.descendant_for_byte_range(span.start, span.end)?;
    for _ in 0..3 {
        let p = n.parent()?;
        match p.kind() {
            "scoped_call_expression" | "scoped_property_access_expression" => {
                return p.child_by_field_name("scope")
            }
            "class_constant_access_expression" => return p.named_child(0),
            _ => n = p,
        }
    }
    None
}

/// The edits of a usage file: each `Scope::member` gets `new_scope(at)` as its scope.
/// `skip` are spans whose usages move with the code. Dynamic scopes are reported.
pub fn usage_edits(
    src: &str,
    spans: &[Span],
    skip: &[Span],
    new_scope: &dyn Fn(usize, &str) -> Option<String>,
) -> (Vec<TextEdit>, Vec<usize>) {
    let Some(tree) = parse(src) else {
        return (vec![], vec![]);
    };
    let root = tree.root_node();
    let mut edits = Vec::new();
    let mut dynamic = Vec::new();
    for s in spans {
        if skip.iter().any(|k| k.contains(*s)) {
            continue;
        }
        let Some(scope) = scope_of(root, *s) else {
            continue;
        };
        if !matches!(scope.kind(), "name" | "qualified_name" | "relative_scope") {
            dynamic.push(s.start);
            continue;
        }
        if let Some(t) = new_scope(scope.start_byte(), text(scope, src)) {
            if t != text(scope, src) {
                edits.push(TextEdit {
                    start: scope.start_byte(),
                    end: scope.end_byte(),
                    new_text: t,
                });
            }
        }
    }
    (edits, dynamic)
}

/// The code of the members as it goes into the target class: re-indented, `self::` /
/// `static::` references to members that stay become `source_ref::`, class names that the
/// target file resolves differently become fully qualified.
#[allow(clippy::too_many_arguments)]
pub fn moved_code(
    src: &str,
    path: &str,
    members: &[MovableMember],
    staying: &[String],
    source_ref: &str,
    target_src: &str,
    target_at: usize,
    indent: &str,
) -> String {
    let source_file = index_source(path, src);
    let target_file = index_source("target.php", target_src);
    let mut parts = Vec::new();
    for m in members {
        let code = &src[m.span.start..m.span.end];
        let wrapped = format!("<?php class __Forge_Move {{\n{code}\n}}");
        let base = "<?php class __Forge_Move {\n".len();
        let Some(tree) = parse(&wrapped) else {
            parts.push(code.to_string());
            continue;
        };
        let root = tree.root_node();
        let mut edits = Vec::new();
        // `self::stays()` -> `Source::stays()`.
        let scopes = super::collect_nodes(root, true, &mut |n| n.kind() == "relative_scope");
        for s in scopes {
            if text(s, &wrapped).eq_ignore_ascii_case("parent") {
                continue;
            }
            let member = s.parent().and_then(|p| match p.kind() {
                "scoped_call_expression" | "scoped_property_access_expression" => {
                    p.child_by_field_name("name")
                }
                "class_constant_access_expression" => p.named_child(1),
                _ => None,
            });
            let Some(member) = member else { continue };
            let name = text(member, &wrapped).trim_start_matches('$');
            if staying.iter().any(|x| x.eq_ignore_ascii_case(name)) {
                edits.push(TextEdit {
                    start: s.start_byte() - base,
                    end: s.end_byte() - base,
                    new_text: source_ref.to_string(),
                });
            }
        }
        let mut names = code_class_names(root, &wrapped);
        names.extend(all_doc_class_names(root, &wrapped));
        for n in names {
            if n.is_fully_qualified() || n.text == "__Forge_Move" {
                continue;
            }
            let fqn = source_file.resolve_class(&n.text, m.span.start);
            let there = target_file.resolve_class(&n.text, target_at);
            if !fqn.eq_ignore_ascii_case(&there) {
                edits.push(TextEdit {
                    start: n.span.start - base,
                    end: n.span.end - base,
                    new_text: format!("\\{fqn}"),
                });
            }
        }
        let rewritten = crate::generate::apply_edits(code, &tidy(edits));
        let from = super::leading_ws(src, m.span.start);
        parts.push(format!(
            "{indent}{}",
            super::reindent(&rewritten, &from, indent)
        ));
    }
    parts.join("\n\n")
}

/// The edit inserting `code` (indented members) at the end of the class body whose `}`
/// is at `close`.
pub fn insert_members(target_src: &str, close: usize, code: &str) -> TextEdit {
    let ls = super::line_start(target_src, close);
    let before = target_src[..close].trim_end();
    let empty = before.ends_with('{');
    if target_src[ls..close].trim().is_empty() {
        let text = if empty {
            format!("{code}\n")
        } else {
            format!("\n{code}\n")
        };
        TextEdit {
            start: ls,
            end: ls,
            new_text: text,
        }
    } else {
        TextEdit {
            start: close,
            end: close,
            new_text: format!("\n{code}\n"),
        }
    }
}

/// The `}` of the class body whose name is at `name`.
pub fn class_body_end(src: &str, name: Span) -> Option<usize> {
    let tree = parse(src)?;
    let n = tree
        .root_node()
        .descendant_for_byte_range(name.start, name.end)?
        .parent()?;
    let body = n.child_by_field_name("body")?;
    Some(body.end_byte() - 1)
}

/// The edit removing a member (with its blank line).
pub fn remove_member(src: &str, m: &MovableMember) -> TextEdit {
    let (s, e, t) = remove_lines(src, m.span.start, m.span.end);
    TextEdit {
        start: s,
        end: e,
        new_text: t,
    }
}

/// How a usage file names the target class at `at`: `self` inside the target class.
pub fn scope_text(
    file_src: &str,
    path: &str,
    target_fqn: &str,
    inside_target: Option<Span>,
    at: usize,
) -> String {
    if inside_target.is_some_and(|s| s.start <= at && at <= s.end) {
        return "self".into();
    }
    class_reference(&index_source(path, file_src), target_fqn, at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::apply_edits;

    const SRC: &str = "<?php\nnamespace App\\Models;\n\nuse Illuminate\\Support\\Str;\n\nclass User\n{\n    public const ROLE = 'user';\n    public static int $count = 0;\n    public $name;\n\n    /** Makes one. */\n    public static function make(): Order\n    {\n        return Str::of(self::ROLE . self::helper());\n    }\n\n    private static function helper(): string { return ''; }\n\n    public function x() {}\n}\n";

    #[test]
    fn lists_static_members() {
        let info = members_at(SRC, SRC.find("make").unwrap()).unwrap();
        let names: Vec<&str> = info.members.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["ROLE", "count", "make", "helper"]);
        assert_eq!(info.selected.as_deref(), Some("make"));
        assert_eq!(
            info.members[2].signature,
            "public static function make(): Order"
        );
        assert_eq!(info.members[1].kind, "property");
        assert_eq!(
            &SRC[info.members[1].name_span.start..info.members[1].name_span.end],
            "count"
        );
        assert_eq!(
            members_at(SRC, SRC.find("$name").unwrap())
                .unwrap()
                .selected,
            None
        );
    }

    #[test]
    fn rewrites_moved_code_for_the_target() {
        let info = members_at(SRC, SRC.find("make").unwrap()).unwrap();
        let target = "<?php\nnamespace App\\Support;\n\nclass Factory\n{\n}\n";
        let code = moved_code(
            SRC,
            "u.php",
            &info.members[2..3],
            &["ROLE".into(), "count".into(), "helper".into()],
            "\\App\\Models\\User",
            target,
            40,
            "    ",
        );
        assert_eq!(code, "    /** Makes one. */\n    public static function make(): \\App\\Models\\Order\n    {\n        return \\Illuminate\\Support\\Str::of(\\App\\Models\\User::ROLE . \\App\\Models\\User::helper());\n    }");
        let close = class_body_end(
            target,
            Span::new(
                target.find("Factory").unwrap(),
                target.find("Factory").unwrap() + 7,
            ),
        )
        .unwrap();
        let out = apply_edits(target, &[insert_members(target, close, &code)]);
        assert!(out.ends_with("class Factory\n{\n    /** Makes one. */\n    public static function make(): \\App\\Models\\Order\n    {\n        return \\Illuminate\\Support\\Str::of(\\App\\Models\\User::ROLE . \\App\\Models\\User::helper());\n    }\n}\n"));
        let out = apply_edits(SRC, &[remove_member(SRC, &info.members[2])]);
        assert!(!out.contains("make"));
        assert!(out.contains("public $name;\n\n    private static function helper()"));
    }

    #[test]
    fn retargets_usages() {
        let src = "<?php\nuse App\\Models\\User;\nUser::make();\n$u::make();\necho User::ROLE . User::$count;\n";
        let spans: Vec<Span> = ["make", "ROLE", "count"]
            .iter()
            .flat_map(|w| {
                src.match_indices(w)
                    .map(|(i, _)| Span::new(i, i + w.len()))
                    .collect::<Vec<_>>()
            })
            .collect();
        let (edits, dynamic) = usage_edits(src, &spans, &[], &|_, _| Some("Factory".into()));
        assert_eq!(dynamic.len(), 1);
        assert_eq!(apply_edits(src, &edits), "<?php\nuse App\\Models\\User;\nFactory::make();\n$u::make();\necho Factory::ROLE . Factory::$count;\n");
        assert_eq!(
            scope_text(src, "x.php", "App\\Support\\Factory", None, 30),
            "\\App\\Support\\Factory"
        );
        assert_eq!(
            scope_text(
                src,
                "x.php",
                "App\\Support\\Factory",
                Some(Span::new(0, 40)),
                30
            ),
            "self"
        );
    }
}
