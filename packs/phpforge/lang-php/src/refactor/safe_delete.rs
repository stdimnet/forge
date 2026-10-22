//! Safe Delete (Alt+Delete) like PHPStorm: what the caret is on (class, method, property,
//! constant, enum case, function or parameter), the edit that deletes it and which usages
//! Find Usages reported do not count (the ones inside the deleted declaration). A parameter
//! is deleted as a signature change, so its call sites lose the argument
//! ([`super::signature`]).

use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use super::move_class::{with_doc, ClassInfo};
use super::signature::{self, SignatureInfo};
use super::{cannot, parse, text, Span};
use crate::generate::TextEdit;
use crate::imports::remove_lines;

/// The element Safe Delete deletes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteTarget {
    /// `class`, `interface`, `trait`, `enum`, `method`, `property`, `constant`, `enum case`,
    /// `function` or `parameter`.
    pub kind: String,
    /// How PHPStorm names it: `User`, `orders()`, `$email`, `MAX`.
    pub name: String,
    /// Where Find Usages looks: the element's name (the function's name for a parameter).
    pub name_span: Span,
    /// The declaration: usages inside it do not keep it alive.
    pub span: Span,
    /// The class is the only declaration of its file, which is deleted with it.
    pub delete_file: bool,
    /// A parameter: its index.
    pub parameter: Option<usize>,
    /// A parameter: its uses in the function body (they stay behind).
    pub body_usages: Vec<Span>,
}

const NOT_ON_ELEMENT: &str = "Caret should be positioned at the name of the element to be deleted";

fn class_kind(n: Node) -> Option<&'static str> {
    Some(match n.kind() {
        "class_declaration" => "class",
        "interface_declaration" => "interface",
        "trait_declaration" => "trait",
        "enum_declaration" => "enum",
        _ => return None,
    })
}

/// The caret is in the header of `n` (before its body).
fn in_header(n: Node, pos: usize) -> bool {
    let body = n.child_by_field_name("body");
    body.is_none_or(|b| pos <= b.start_byte())
}

/// What the caret is on.
pub fn target_at(src: &str, pos: usize) -> Result<DeleteTarget, String> {
    let tree = parse(src).ok_or_else(|| cannot("The file cannot be parsed"))?;
    let root = tree.root_node();
    let mut n = root
        .descendant_for_byte_range(pos, pos)
        .ok_or_else(|| cannot(NOT_ON_ELEMENT))?;
    loop {
        match n.kind() {
            "simple_parameter" | "variadic_parameter" | "property_promotion_parameter" => {
                return parameter_target(n, src);
            }
            "property_element" => {
                let decl = n.parent().ok_or_else(|| cannot(NOT_ON_ELEMENT))?;
                let name = n
                    .child_by_field_name("name")
                    .ok_or_else(|| cannot(NOT_ON_ELEMENT))?;
                return Ok(DeleteTarget {
                    kind: "property".into(),
                    name: text(name, src).to_string(),
                    name_span: Span::new(name.start_byte() + 1, name.end_byte()),
                    span: element_span(decl, n, "property_element", src),
                    delete_file: false,
                    parameter: None,
                    body_usages: vec![],
                });
            }
            "const_element" => {
                let decl = n.parent().ok_or_else(|| cannot(NOT_ON_ELEMENT))?;
                let mut c = n.walk();
                let name = n
                    .named_children(&mut c)
                    .find(|x| x.kind() == "name")
                    .ok_or_else(|| cannot(NOT_ON_ELEMENT))?;
                return Ok(DeleteTarget {
                    kind: "constant".into(),
                    name: text(name, src).to_string(),
                    name_span: Span::of(name),
                    span: element_span(decl, n, "const_element", src),
                    delete_file: false,
                    parameter: None,
                    body_usages: vec![],
                });
            }
            "enum_case" => {
                let name = n
                    .child_by_field_name("name")
                    .ok_or_else(|| cannot(NOT_ON_ELEMENT))?;
                return Ok(DeleteTarget {
                    kind: "enum case".into(),
                    name: text(name, src).to_string(),
                    name_span: Span::of(name),
                    span: with_doc(n, src),
                    delete_file: false,
                    parameter: None,
                    body_usages: vec![],
                });
            }
            "method_declaration" | "function_definition" => {
                if !in_header(n, pos) {
                    return Err(cannot(NOT_ON_ELEMENT));
                }
                let name = n
                    .child_by_field_name("name")
                    .ok_or_else(|| cannot(NOT_ON_ELEMENT))?;
                return Ok(DeleteTarget {
                    kind: if n.kind() == "method_declaration" {
                        "method".into()
                    } else {
                        "function".into()
                    },
                    name: format!("{}()", text(name, src)),
                    name_span: Span::of(name),
                    span: with_doc(n, src),
                    delete_file: false,
                    parameter: None,
                    body_usages: vec![],
                });
            }
            "anonymous_function" | "arrow_function" | "anonymous_class" => {
                return Err(cannot(NOT_ON_ELEMENT))
            }
            _ => {}
        }
        if let Some(kind) = class_kind(n) {
            if !in_header(n, pos) {
                return Err(cannot(NOT_ON_ELEMENT));
            }
            let info: ClassInfo =
                super::move_class::class_at(src, pos).ok_or_else(|| cannot(NOT_ON_ELEMENT))?;
            return Ok(DeleteTarget {
                kind: kind.into(),
                name: info.name.clone(),
                name_span: info.name_span,
                span: info.span,
                delete_file: info.sole,
                parameter: None,
                body_usages: vec![],
            });
        }
        n = n.parent().ok_or_else(|| cannot(NOT_ON_ELEMENT))?;
    }
}

fn parameter_target(p: Node, src: &str) -> Result<DeleteTarget, String> {
    let list = p.parent().ok_or_else(|| cannot(NOT_ON_ELEMENT))?;
    let f = list.parent().ok_or_else(|| cannot(NOT_ON_ELEMENT))?;
    if !matches!(f.kind(), "method_declaration" | "function_definition") {
        return Err(cannot("Parameters of closures cannot be deleted safely"));
    }
    let fname = f
        .child_by_field_name("name")
        .ok_or_else(|| cannot(NOT_ON_ELEMENT))?;
    let mut c = list.walk();
    let index = list
        .named_children(&mut c)
        .filter(|x| x.kind().ends_with("parameter"))
        .position(|x| x == p)
        .ok_or_else(|| cannot(NOT_ON_ELEMENT))?;
    let name = p
        .child_by_field_name("name")
        .map(|n| text(n, src).trim_start_matches('$').to_string())
        .unwrap_or_default();
    let body_usages = f
        .child_by_field_name("body")
        .map(|b| {
            super::collect_nodes(b, true, &mut |n| {
                n.kind() == "variable_name" && text(n, src).trim_start_matches('$') == name
            })
            .into_iter()
            .map(Span::of)
            .collect()
        })
        .unwrap_or_default();
    Ok(DeleteTarget {
        kind: "parameter".into(),
        name: format!("${name}"),
        name_span: Span::of(fname),
        span: Span::of(p),
        delete_file: false,
        parameter: Some(index),
        body_usages,
    })
}

/// One element of `public $a, $b;` / `const A = 1, B = 2;`: the element with its comma,
/// or the whole declaration (with its PHPDoc) when it is the only one.
fn element_span(decl: Node, el: Node, kind: &str, src: &str) -> Span {
    let mut c = decl.walk();
    let items: Vec<Node> = decl
        .named_children(&mut c)
        .filter(|x| x.kind() == kind)
        .collect();
    if items.len() <= 1 {
        return with_doc(decl, src);
    }
    let i = items.iter().position(|x| *x == el).unwrap_or(0);
    if i + 1 < items.len() {
        Span::new(el.start_byte(), items[i + 1].start_byte())
    } else {
        Span::new(items[i - 1].end_byte(), el.end_byte())
    }
}

/// The signature of the function whose parameter is deleted.
pub fn parameter_signature(src: &str, target: &DeleteTarget) -> Option<SignatureInfo> {
    signature::signature_of(src, target.name_span)
}

/// The edits deleting the element (not a parameter, see [`parameter_edits`]).
pub fn delete_edits(src: &str, target: &DeleteTarget) -> Vec<TextEdit> {
    let whole = target.span;
    let (s, e, t) = remove_lines(src, whole.start, whole.end);
    vec![TextEdit {
        start: s,
        end: e,
        new_text: t,
    }]
}

/// The edits of the declaration that loses parameter `target.parameter`, with the
/// problems (the body still uses it).
pub fn parameter_edits(
    src: &str,
    info: &SignatureInfo,
    index: usize,
) -> Result<(Vec<TextEdit>, Vec<String>), String> {
    let options = signature::without_parameter(info, index);
    signature::declaration_edits(src, info, &options, false)
}

/// Whether a usage at `usage` in the target's own file is inside the deleted declaration
/// (recursion, a class naming itself) and so does not keep it alive.
pub fn is_inside(target: &DeleteTarget, usage: Span) -> bool {
    target.span.contains(usage) || usage == target.name_span
}

/// PHPStorm's message of the Usages Detected dialog.
pub fn usages_message(target: &DeleteTarget, usages: usize, in_comments: usize) -> String {
    let what = if usages == 1 { "usage" } else { "usages" };
    format!(
        "{} {} has {usages} {what} that {} not safe to delete.\nOf those {in_comments} {} in strings, comments, or non-code files.",
        target.kind,
        target.name,
        if usages == 1 { "is" } else { "are" },
        if in_comments == 1 { "usage is" } else { "usages are" },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::apply_edits;

    const SRC: &str = "<?php\nnamespace App;\n\nclass Order\n{\n    public const OPEN = 'open', CLOSED = 'closed';\n\n    /** @var int */\n    private int $total = 0;\n\n    public $a, $b;\n\n    /**\n     * The lines.\n     */\n    public function lines(int $limit, bool $all = false): array\n    {\n        return $this->lines($limit);\n    }\n\n    public function other(): void {}\n}\n";

    fn at(word: &str) -> DeleteTarget {
        target_at(SRC, SRC.find(word).unwrap() + 1).unwrap()
    }

    fn deleted(word: &str) -> String {
        apply_edits(SRC, &delete_edits(SRC, &at(word)))
    }

    #[test]
    fn finds_what_to_delete() {
        let t = at("Order");
        assert_eq!((t.kind.as_str(), t.name.as_str()), ("class", "Order"));
        assert!(t.delete_file);
        let t = at("lines(");
        assert_eq!((t.kind.as_str(), t.name.as_str()), ("method", "lines()"));
        assert!(&SRC[t.span.start..].starts_with("/**\n     * The lines."));
        let t = at("$limit");
        assert_eq!((t.kind.as_str(), t.name.as_str()), ("parameter", "$limit"));
        assert_eq!(t.parameter, Some(0));
        assert_eq!(t.body_usages.len(), 1);
        assert_eq!(&SRC[t.name_span.start..t.name_span.end], "lines");
        assert_eq!(at("$total").kind, "property");
        assert_eq!(at("CLOSED").name, "CLOSED");
        // Inside a body there is nothing to delete.
        assert!(target_at(SRC, SRC.find("return").unwrap()).is_err());
        let e = "<?php\nenum S { case A; case B; }\nfunction f() {}\n";
        assert_eq!(
            target_at(e, e.find("B;").unwrap()).unwrap().kind,
            "enum case"
        );
        assert!(!target_at(e, e.find("S {").unwrap()).unwrap().delete_file);
        assert_eq!(target_at(e, e.find("f()").unwrap()).unwrap().name, "f()");
    }

    #[test]
    fn deletes_declarations() {
        assert!(!deleted("other").contains("other"));
        let out = deleted("lines(");
        assert!(!out.contains("The lines"));
        assert!(out.contains("public $a, $b;\n\n    public function other()"));
        let out = deleted("$total");
        assert!(!out.contains("@var int"));
        assert!(out.contains("'closed';\n\n    public $a"));
        assert!(deleted("CLOSED").contains("public const OPEN = 'open';"));
        assert!(deleted("OPEN").contains("public const CLOSED = 'closed';"));
        assert!(deleted("$b").contains("public $a;"));
    }

    #[test]
    fn deletes_parameters() {
        let t = at("$all");
        let info = parameter_signature(SRC, &t).unwrap();
        let (edits, problems) = parameter_edits(SRC, &info, 1).unwrap();
        assert!(problems.is_empty());
        assert!(apply_edits(SRC, &edits).contains("lines(int $limit): array"));
        let (_, problems) = parameter_edits(SRC, &info, 0).unwrap();
        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn usages_inside_do_not_count() {
        let t = at("lines(");
        let call = SRC.rfind("lines(").unwrap();
        assert!(is_inside(&t, Span::new(call, call + 5)));
        assert!(!is_inside(&t, Span::new(5, 6)));
        assert_eq!(
            usages_message(&t, 2, 0),
            "method lines() has 2 usages that are not safe to delete.\nOf those 0 usages are in strings, comments, or non-code files."
        );
    }
}
