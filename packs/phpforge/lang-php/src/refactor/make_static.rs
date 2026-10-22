//! Make Static like PHPStorm: an instance method becomes `static`. A method that uses
//! `$this` gets the object as a parameter (`$this` -> `$user`), or, when it only reads
//! fields, one parameter per field (`$this->name` -> `$name`). Calls change from
//! `$obj->m(...)` to `Class::m(...)` (`self::m(...)` for `$this->m(...)`), passing the
//! object or its fields.

use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use super::introduce::call_at;
use super::{cannot, is_nested_declaration, parse, text, Span};
use crate::generate::TextEdit;
use crate::model::{short_name, FileIndex, UseKind};

/// What the Make Static dialog shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MakeStaticInfo {
    pub name: String,
    pub class_name: String,
    pub name_span: Span,
    /// The body uses `$this`.
    pub uses_this: bool,
    /// Fields the body reads through `$this->`, in order.
    pub fields: Vec<String>,
    /// `$this` is used for more than reading fields (calls, writes, passed on).
    pub needs_object: bool,
    /// The suggested name of the object parameter (`user` for `User`).
    pub parameter: String,
}

/// The dialog's choice.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MakeStaticOptions {
    /// Add the object as a parameter with this name.
    pub object_parameter: Option<String>,
    /// Add a parameter for every field read.
    pub field_parameters: bool,
}

fn method_at<'t>(root: Node<'t>, pos: usize) -> Option<Node<'t>> {
    let mut n = root.descendant_for_byte_range(pos, pos)?;
    loop {
        if n.kind() == "method_declaration" {
            return Some(n);
        }
        if matches!(
            n.kind(),
            "class_declaration" | "function_definition" | "anonymous_class"
        ) {
            return None;
        }
        n = n.parent()?;
    }
}

/// `$this` nodes of a body (closures included, nested classes not).
fn this_nodes<'t>(n: Node<'t>, src: &str, out: &mut Vec<Node<'t>>) {
    if is_nested_declaration(n) {
        return;
    }
    if n.kind() == "variable_name" && text(n, src) == "$this" {
        out.push(n);
        return;
    }
    let mut c = n.walk();
    for ch in n.children(&mut c) {
        this_nodes(ch, src, out);
    }
}

/// A field read `$this->name` (not a call, not written): the field name.
fn field_read(this: Node, src: &str) -> Option<String> {
    let access = this
        .parent()
        .filter(|p| p.kind() == "member_access_expression")?;
    if access.child_by_field_name("object") != Some(this) {
        return None;
    }
    let name = access
        .child_by_field_name("name")
        .filter(|n| n.kind() == "name")?;
    let parent = access.parent()?;
    let written = match parent.kind() {
        "assignment_expression"
        | "reference_assignment_expression"
        | "augmented_assignment_expression" => parent.child_by_field_name("left") == Some(access),
        "update_expression" | "unset_statement" => true,
        "member_call_expression" | "nullsafe_member_call_expression" | "subscript_expression" => {
            // `$this->items[] = ...` writes; `$this->repo->find()` only reads.
            parent.kind() == "subscript_expression"
                && parent.parent().is_some_and(|g| {
                    g.kind() == "assignment_expression"
                        && g.child_by_field_name("left") == Some(parent)
                })
        }
        _ => false,
    };
    (!written).then(|| text(name, src).to_string())
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// The method at the caret, if it can be made static.
pub fn make_static_at(src: &str, pos: usize) -> Result<MakeStaticInfo, String> {
    let tree = parse(src).ok_or_else(|| cannot("The file cannot be parsed"))?;
    let m = method_at(tree.root_node(), pos)
        .ok_or_else(|| cannot("Caret should be positioned at the method to be made static"))?;
    if super::is_static_method(m) {
        return Err(cannot("The method is already static"));
    }
    let name_node = m
        .child_by_field_name("name")
        .ok_or_else(|| cannot("The method has no name"))?;
    let name = text(name_node, src).to_string();
    if name.starts_with("__") {
        return Err(cannot("Magic methods and constructors cannot be static"));
    }
    let body = m
        .child_by_field_name("body")
        .ok_or_else(|| cannot("Abstract methods cannot be made static"))?;
    let class_name = super::enclosing_class(m)
        .and_then(|c| c.child_by_field_name("name"))
        .map(|n| text(n, src).to_string())
        .ok_or_else(|| cannot("The method is not in a class"))?;
    let mut thises = Vec::new();
    this_nodes(body, src, &mut thises);
    let mut fields: Vec<String> = Vec::new();
    let mut needs_object = false;
    for t in &thises {
        match field_read(*t, src) {
            Some(f) => {
                if !fields.contains(&f) {
                    fields.push(f);
                }
            }
            None => needs_object = true,
        }
    }
    Ok(MakeStaticInfo {
        parameter: lower_first(&class_name),
        uses_this: !thises.is_empty(),
        fields,
        needs_object,
        class_name,
        name,
        name_span: Span::of(name_node),
    })
}

fn check(info: &MakeStaticInfo, options: &MakeStaticOptions) -> Result<(), String> {
    if let Some(p) = &options.object_parameter {
        super::check_name(p.trim_start_matches('$'))?;
    } else if info.uses_this && !options.field_parameters {
        return Err(cannot(
            "The method uses $this: add the object or its fields as parameters",
        ));
    } else if options.field_parameters && info.needs_object {
        return Err(cannot(
            "The method uses $this for more than reading fields: add the object as a parameter",
        ));
    }
    Ok(())
}

/// The declaration's edits: `static`, the new parameters and `$this` replaced.
pub fn declaration_edits(
    src: &str,
    info: &MakeStaticInfo,
    options: &MakeStaticOptions,
) -> Result<Vec<TextEdit>, String> {
    check(info, options)?;
    let tree = parse(src).ok_or_else(|| cannot("The file cannot be parsed"))?;
    let m = method_at(tree.root_node(), info.name_span.start)
        .ok_or_else(|| cannot("The method cannot be found"))?;
    let mut edits = Vec::new();
    let mut c = m.walk();
    let kw = m
        .children(&mut c)
        .find(|ch| ch.kind() == "function")
        .ok_or_else(|| cannot("The method cannot be found"))?;
    edits.push(TextEdit {
        start: kw.start_byte(),
        end: kw.start_byte(),
        new_text: "static ".into(),
    });
    let body = m
        .child_by_field_name("body")
        .ok_or_else(|| cannot("The method has no body"))?;
    let mut thises = Vec::new();
    this_nodes(body, src, &mut thises);
    let mut added: Vec<String> = Vec::new();
    if let Some(p) = &options.object_parameter {
        let p = p.trim_start_matches('$');
        added.push(format!("{} ${p}", info.class_name));
        for t in thises {
            edits.push(TextEdit {
                start: t.start_byte(),
                end: t.end_byte(),
                new_text: format!("${p}"),
            });
        }
    } else if options.field_parameters {
        for f in &info.fields {
            added.push(format!("${f}"));
        }
        for t in thises {
            if let Some(access) = t
                .parent()
                .filter(|p| p.kind() == "member_access_expression")
            {
                if let Some(f) = field_read(t, src) {
                    edits.push(TextEdit {
                        start: access.start_byte(),
                        end: access.end_byte(),
                        new_text: format!("${f}"),
                    });
                }
            }
        }
    }
    if !added.is_empty() {
        let params = m
            .child_by_field_name("parameters")
            .ok_or_else(|| cannot("The method has no parameters"))?;
        let mut c = params.walk();
        let first = params
            .named_children(&mut c)
            .find(|p| p.kind().ends_with("parameter"));
        let (at, text) = match first {
            Some(f) => (f.start_byte(), format!("{}, ", added.join(", "))),
            None => (params.start_byte() + 1, added.join(", ")),
        };
        edits.push(TextEdit {
            start: at,
            end: at,
            new_text: text,
        });
    }
    Ok(super::classes::tidy(edits))
}

/// How a file names the class `fqn`: its import, its short name in the same namespace,
/// else the fully qualified name.
pub fn class_reference(file: &FileIndex, fqn: &str, at: usize) -> String {
    let fqn = fqn.trim_start_matches('\\');
    if let Some(u) = file
        .uses
        .iter()
        .find(|u| u.kind == UseKind::Class && u.fqn.eq_ignore_ascii_case(fqn))
    {
        return u.local_name().to_string();
    }
    let ns = fqn.rsplit_once('\\').map_or("", |(n, _)| n);
    if file.namespace_at(at).eq_ignore_ascii_case(ns)
        && !file.uses.iter().any(|u| {
            u.kind == UseKind::Class && u.local_name().eq_ignore_ascii_case(short_name(fqn))
        })
    {
        return short_name(fqn).to_string();
    }
    format!("\\{fqn}")
}

/// The edits of one call: `$obj->m(a)` -> `Class::m($obj, a)`. `class_ref` is how the file
/// names the class.
pub fn call_edits(
    src: &str,
    span: Span,
    info: &MakeStaticInfo,
    options: &MakeStaticOptions,
    class_ref: &str,
) -> Result<Vec<TextEdit>, String> {
    let tree = parse(src).ok_or_else(|| cannot("The file cannot be parsed"))?;
    let Some(call) = call_at(tree.root_node(), span) else {
        return Ok(vec![]);
    };
    let (scope, object) = match call.kind() {
        "member_call_expression" | "nullsafe_member_call_expression" => {
            let o = call
                .child_by_field_name("object")
                .ok_or_else(|| cannot("No object"))?;
            let o_text = text(o, src).to_string();
            let scope = if o_text == "$this" {
                "self".to_string()
            } else {
                class_ref.to_string()
            };
            (
                Some((call.start_byte(), span.start, format!("{scope}::"))),
                o_text,
            )
        }
        "scoped_call_expression" => {
            let s = call
                .child_by_field_name("scope")
                .ok_or_else(|| cannot("No scope"))?;
            if s.kind() != "relative_scope" {
                return Ok(vec![]);
            }
            (None, "$this".to_string())
        }
        _ => return Ok(vec![]),
    };
    let mut edits = Vec::new();
    if let Some((s, e, t)) = scope {
        edits.push(TextEdit {
            start: s,
            end: e,
            new_text: t,
        });
    }
    let extra: Vec<String> = if options.object_parameter.is_some() {
        vec![object]
    } else if options.field_parameters {
        info.fields
            .iter()
            .map(|f| format!("{object}->{f}"))
            .collect()
    } else {
        vec![]
    };
    if !extra.is_empty() {
        let args = call
            .child_by_field_name("arguments")
            .ok_or_else(|| cannot("No arguments"))?;
        let mut c = args.walk();
        let first = args.named_children(&mut c).find(|a| a.kind() != "comment");
        if first.is_some_and(|f| f.kind() == "variadic_placeholder") {
            return Err(format!(
                "The call to {}() is a first-class callable and is not updated",
                info.name
            ));
        }
        let (at, t) = match first {
            Some(f) => (f.start_byte(), format!("{}, ", extra.join(", "))),
            None => (args.start_byte() + 1, extra.join(", ")),
        };
        edits.push(TextEdit {
            start: at,
            end: at,
            new_text: t,
        });
    }
    Ok(edits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::apply_edits;
    use crate::parser::index_source;

    const SRC: &str = "<?php\nnamespace App;\nclass Price\n{\n    private float $rate = 0.2;\n    private float $net = 0;\n\n    public function gross(float $amount): float\n    {\n        return $amount * (1 + $this->rate) + $this->net;\n    }\n\n    public function round(float $x): float\n    {\n        return round($x, 2);\n    }\n\n    public function log(): void\n    {\n        $this->rate = 1;\n        $this->save();\n    }\n\n    public function total(): float\n    {\n        return $this->round($this->gross(1));\n    }\n}\n";

    fn info(word: &str) -> MakeStaticInfo {
        make_static_at(SRC, SRC.find(word).unwrap()).unwrap()
    }

    #[test]
    fn analyses_the_method() {
        let i = info("round(float");
        assert!(!i.uses_this);
        let i = info("gross(float");
        assert_eq!(i.fields, ["rate", "net"]);
        assert!(!i.needs_object);
        assert_eq!(i.parameter, "price");
        assert!(info("log()").needs_object);
        assert!(make_static_at(SRC, SRC.find("class Price").unwrap()).is_err());
    }

    #[test]
    fn makes_methods_static() {
        let i = info("round(float");
        let out = apply_edits(
            SRC,
            &declaration_edits(SRC, &i, &MakeStaticOptions::default()).unwrap(),
        );
        assert!(out.contains("public static function round(float $x): float"));
        let i = info("gross(float");
        assert!(declaration_edits(SRC, &i, &MakeStaticOptions::default()).is_err());
        let fields = MakeStaticOptions {
            field_parameters: true,
            ..Default::default()
        };
        let out = apply_edits(SRC, &declaration_edits(SRC, &i, &fields).unwrap());
        assert!(out.contains("public static function gross($rate, $net, float $amount): float\n    {\n        return $amount * (1 + $rate) + $net;"));
        let object = MakeStaticOptions {
            object_parameter: Some("price".into()),
            ..Default::default()
        };
        let out = apply_edits(SRC, &declaration_edits(SRC, &i, &object).unwrap());
        assert!(out.contains("static function gross(Price $price, float $amount): float\n    {\n        return $amount * (1 + $price->rate) + $price->net;"));
        let i = info("log()");
        assert!(declaration_edits(SRC, &i, &fields).is_err());
    }

    #[test]
    fn updates_calls() {
        let i = info("gross(float");
        let fields = MakeStaticOptions {
            field_parameters: true,
            ..Default::default()
        };
        let at = SRC.rfind("gross(").unwrap();
        let out = apply_edits(
            SRC,
            &call_edits(SRC, Span::new(at, at + 5), &i, &fields, "Price").unwrap(),
        );
        assert!(out.contains("return $this->round(self::gross($this->rate, $this->net, 1));"));
        let ext = "<?php\n$p = new Price();\necho $p->gross(2);\n";
        let at = ext.find("gross").unwrap();
        let object = MakeStaticOptions {
            object_parameter: Some("price".into()),
            ..Default::default()
        };
        let out = apply_edits(
            ext,
            &call_edits(ext, Span::new(at, at + 5), &i, &object, "\\App\\Price").unwrap(),
        );
        assert_eq!(
            out,
            "<?php\n$p = new Price();\necho \\App\\Price::gross($p, 2);\n"
        );
        let file = index_source(
            "x.php",
            "<?php\nnamespace App\\Http;\nuse App\\Price as P;\n",
        );
        assert_eq!(class_reference(&file, "App\\Price", 30), "P");
        let file = index_source("x.php", "<?php\nnamespace App;\n");
        assert_eq!(class_reference(&file, "App\\Price", 20), "Price");
        let file = index_source("x.php", "<?php\nnamespace Other;\n");
        assert_eq!(class_reference(&file, "App\\Price", 20), "\\App\\Price");
    }
}
