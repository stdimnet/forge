//! Change Signature (Ctrl+F6) like PHPStorm: the name, visibility, return type and
//! parameters (add, remove, reorder, rename, type, default value and the value passed at
//! call sites for a new parameter) of a method or function. The declaration, the
//! overriding / implementing methods (the caller passes their declarations) and every call
//! site (positional and named arguments, `new` for constructors) are updated.
//!
//! Safe Delete of a parameter is a signature change without that parameter
//! ([`without_parameter`]).

use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use super::move_class::with_doc;
use super::{cannot, is_identifier, is_nested_declaration, parse, text, Span};
use crate::generate::TextEdit;

/// One parameter of the current signature.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SignatureParam {
    /// Without `$`.
    pub name: String,
    pub type_hint: Option<String>,
    pub default_value: Option<String>,
    pub by_ref: bool,
    pub variadic: bool,
    /// Constructor promotion as written (`private readonly`).
    pub promotion: Option<String>,
    /// Attributes as written (`#[SensitiveParameter]`).
    pub attributes: Option<String>,
    /// Used in the function's body.
    pub used: bool,
}

/// The signature at the caret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureInfo {
    /// `method` or `function`.
    pub kind: String,
    pub name: String,
    pub class_name: Option<String>,
    pub visibility: Option<String>,
    pub is_static: bool,
    pub is_abstract: bool,
    pub constructor: bool,
    pub return_type: Option<String>,
    pub parameters: Vec<SignatureParam>,
    pub name_span: Span,
}

/// A parameter of the new signature.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewParam {
    /// Its index in the current signature; `None` for an added parameter.
    pub original: Option<usize>,
    pub name: String,
    pub type_hint: Option<String>,
    pub default_value: Option<String>,
    /// What call sites pass for an added parameter ("Default value for call sites").
    pub call_value: Option<String>,
    pub by_ref: bool,
    pub variadic: bool,
}

/// The choices of the Change Signature dialog.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChangeSignatureOptions {
    pub name: String,
    /// `public`, `protected`, `private` (methods only).
    pub visibility: Option<String>,
    /// Empty for none.
    pub return_type: String,
    pub parameters: Vec<NewParam>,
}

fn non_empty(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

fn function_node<'t>(root: Node<'t>, pos: usize) -> Option<Node<'t>> {
    let mut n = root.descendant_for_byte_range(pos, pos)?;
    loop {
        if matches!(n.kind(), "method_declaration" | "function_definition") {
            return Some(n);
        }
        // Not through a class: the caret in a class header is no function.
        if matches!(
            n.kind(),
            "class_declaration"
                | "interface_declaration"
                | "trait_declaration"
                | "enum_declaration"
        ) {
            return None;
        }
        n = n.parent()?;
    }
}

/// The method or function around `pos` (its header or body).
pub fn signature_at(src: &str, pos: usize) -> Result<SignatureInfo, String> {
    let tree = parse(src).ok_or_else(|| cannot("The file cannot be parsed"))?;
    let f = function_node(tree.root_node(), pos).ok_or_else(|| {
        cannot("Caret should be positioned at the name of the method or function to be refactored")
    })?;
    Ok(info_of(f, src))
}

/// The method or function whose header (name, parameters, return type) the caret is in.
pub fn signature_in_header(src: &str, pos: usize) -> Result<SignatureInfo, String> {
    let tree = parse(src).ok_or_else(|| cannot("The file cannot be parsed"))?;
    let f = function_node(tree.root_node(), pos)
        .filter(|f| {
            f.child_by_field_name("body")
                .is_none_or(|b| pos <= b.start_byte())
        })
        .ok_or_else(|| {
            cannot(
                "Caret should be positioned at the name of the method or function to be refactored",
            )
        })?;
    Ok(info_of(f, src))
}

/// The signature of the declaration whose name is at `name_span` (overriding methods).
pub fn signature_of(src: &str, name_span: Span) -> Option<SignatureInfo> {
    let tree = parse(src)?;
    let n = tree
        .root_node()
        .descendant_for_byte_range(name_span.start, name_span.end)?;
    let f = n.parent()?;
    matches!(f.kind(), "method_declaration" | "function_definition").then(|| info_of(f, src))
}

fn info_of(f: Node, src: &str) -> SignatureInfo {
    let mut info = SignatureInfo {
        kind: if f.kind() == "method_declaration" {
            "method".into()
        } else {
            "function".into()
        },
        name: f
            .child_by_field_name("name")
            .map(|n| text(n, src).to_string())
            .unwrap_or_default(),
        class_name: super::enclosing_class(f)
            .and_then(|c| c.child_by_field_name("name"))
            .map(|n| text(n, src).to_string()),
        visibility: None,
        is_static: false,
        is_abstract: false,
        constructor: false,
        return_type: f
            .child_by_field_name("return_type")
            .map(|t| text(t, src).to_string()),
        parameters: vec![],
        name_span: f
            .child_by_field_name("name")
            .map(Span::of)
            .unwrap_or_default(),
    };
    info.constructor = info.name.eq_ignore_ascii_case("__construct");
    let mut c = f.walk();
    for ch in f.children(&mut c) {
        match ch.kind() {
            "visibility_modifier" => info.visibility = Some(text(ch, src).to_ascii_lowercase()),
            "static_modifier" => info.is_static = true,
            "abstract_modifier" => info.is_abstract = true,
            _ => {}
        }
    }
    if info.kind == "method" && info.visibility.is_none() {
        info.visibility = Some("public".into());
    }
    let body = f.child_by_field_name("body");
    if let Some(params) = f.child_by_field_name("parameters") {
        let mut c = params.walk();
        for p in params.named_children(&mut c) {
            if !p.kind().ends_with("parameter") {
                continue;
            }
            let name = p
                .child_by_field_name("name")
                .map(|n| text(n, src).trim_start_matches('$').to_string())
                .unwrap_or_default();
            let mut promotion = Vec::new();
            let mut attributes = None;
            let mut by_ref = false;
            let mut c2 = p.walk();
            for ch in p.children(&mut c2) {
                match ch.kind() {
                    "visibility_modifier" | "readonly_modifier" => {
                        promotion.push(text(ch, src).to_string())
                    }
                    "attribute_list" => attributes = Some(text(ch, src).to_string()),
                    "reference_modifier" => by_ref = true,
                    _ => {}
                }
            }
            let used = body.is_some_and(|b| !variable_nodes(b, src, &name).is_empty());
            info.parameters.push(SignatureParam {
                type_hint: p
                    .child_by_field_name("type")
                    .map(|t| text(t, src).to_string()),
                default_value: p
                    .child_by_field_name("default_value")
                    .map(|d| text(d, src).to_string()),
                by_ref,
                variadic: p.kind() == "variadic_parameter",
                promotion: (!promotion.is_empty()).then(|| promotion.join(" ")),
                attributes,
                used: used || p.kind() == "property_promotion_parameter",
                name,
            });
        }
    }
    info
}

/// The `$name` nodes in a body (closures included, nested declarations not).
fn variable_nodes<'t>(body: Node<'t>, src: &str, name: &str) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    fn walk<'t>(n: Node<'t>, src: &str, name: &str, out: &mut Vec<Node<'t>>) {
        if is_nested_declaration(n) {
            return;
        }
        if n.kind() == "variable_name" && text(n, src).trim_start_matches('$') == name {
            out.push(n);
            return;
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            walk(ch, src, name, out);
        }
    }
    let mut c = body.walk();
    for ch in body.children(&mut c) {
        walk(ch, src, name, &mut out);
    }
    out
}

/// The current signature unchanged, as dialog options.
pub fn unchanged(info: &SignatureInfo) -> ChangeSignatureOptions {
    ChangeSignatureOptions {
        name: info.name.clone(),
        visibility: info.visibility.clone(),
        return_type: info.return_type.clone().unwrap_or_default(),
        parameters: info
            .parameters
            .iter()
            .enumerate()
            .map(|(i, p)| NewParam {
                original: Some(i),
                name: p.name.clone(),
                type_hint: p.type_hint.clone(),
                default_value: p.default_value.clone(),
                call_value: None,
                by_ref: p.by_ref,
                variadic: p.variadic,
            })
            .collect(),
    }
}

/// Safe Delete of parameter `index`: the signature without it.
pub fn without_parameter(info: &SignatureInfo, index: usize) -> ChangeSignatureOptions {
    let mut o = unchanged(info);
    o.parameters.retain(|p| p.original != Some(index));
    o
}

/// Checks the dialog's choices; the message is shown in the dialog.
pub fn check(info: &SignatureInfo, options: &ChangeSignatureOptions) -> Result<(), String> {
    if !is_identifier(options.name.trim()) {
        return Err(format!(
            "'{}' is not a valid identifier",
            options.name.trim()
        ));
    }
    let mut seen: Vec<String> = Vec::new();
    for (i, p) in options.parameters.iter().enumerate() {
        let name = p.name.trim().trim_start_matches('$');
        if !is_identifier(name) {
            return Err(format!("'{name}' is not a valid parameter name"));
        }
        if seen.contains(&name.to_string()) {
            return Err(format!("Duplicate parameter name: ${name}"));
        }
        seen.push(name.to_string());
        if p.variadic && i + 1 != options.parameters.len() {
            return Err("Only the last parameter can be variadic".into());
        }
        if p.original.is_some_and(|o| o >= info.parameters.len()) {
            return Err(cannot("Unknown parameter"));
        }
    }
    for p in options.parameters.iter().filter(|p| p.original.is_none()) {
        if non_empty(&p.call_value).is_none()
            && non_empty(&p.default_value).is_none()
            && !p.variadic
        {
            return Err(format!(
                "Specify the default value for call sites of the new parameter ${}",
                p.name.trim().trim_start_matches('$')
            ));
        }
    }
    Ok(())
}

/// `int &...$ids = []` with promotion and attributes kept from the old parameter.
fn render_parameter(p: &NewParam, old: Option<&SignatureParam>) -> String {
    let mut s = String::new();
    if let Some(a) = old.and_then(|o| o.attributes.as_ref()) {
        s.push_str(a);
        s.push(' ');
    }
    if let Some(pr) = old.and_then(|o| o.promotion.as_ref()) {
        s.push_str(pr);
        s.push(' ');
    }
    if let Some(t) = non_empty(&p.type_hint) {
        s.push_str(t);
        s.push(' ');
    }
    if p.by_ref {
        s.push('&');
    }
    if p.variadic {
        s.push_str("...");
    }
    s.push('$');
    s.push_str(p.name.trim().trim_start_matches('$'));
    if let Some(d) = non_empty(&p.default_value).filter(|_| !p.variadic) {
        s.push_str(" = ");
        s.push_str(d);
    }
    s
}

/// The signature preview of the dialog: `public function name(int $a): void`.
pub fn render_signature(info: &SignatureInfo, options: &ChangeSignatureOptions) -> String {
    let params: Vec<String> = options
        .parameters
        .iter()
        .map(|p| render_parameter(p, p.original.and_then(|i| info.parameters.get(i))))
        .collect();
    let mut head = String::new();
    if info.is_abstract {
        head.push_str("abstract ");
    }
    if let Some(v) = options
        .visibility
        .as_deref()
        .filter(|_| info.kind == "method")
    {
        head.push_str(v);
        head.push(' ');
    }
    if info.is_static {
        head.push_str("static ");
    }
    let ret = options.return_type.trim();
    let ret = if ret.is_empty() {
        String::new()
    } else {
        format!(": {ret}")
    };
    format!(
        "{head}function {}({}){ret}",
        options.name.trim(),
        params.join(", ")
    )
}

/// The edits of the declaration (`info` is its current signature): name, visibility,
/// parameters, return type, renamed parameters in the body and PHPDoc. Overriding methods
/// keep their visibility (`own_visibility` false). Returns the problems too (removed
/// parameters that the body still uses).
pub fn declaration_edits(
    src: &str,
    info: &SignatureInfo,
    options: &ChangeSignatureOptions,
    own_visibility: bool,
) -> Result<(Vec<TextEdit>, Vec<String>), String> {
    let tree = parse(src).ok_or_else(|| cannot("The file cannot be parsed"))?;
    let root = tree.root_node();
    let name = root
        .descendant_for_byte_range(info.name_span.start, info.name_span.end)
        .ok_or_else(|| cannot("The declaration cannot be found"))?;
    let f = name
        .parent()
        .filter(|f| matches!(f.kind(), "method_declaration" | "function_definition"))
        .ok_or_else(|| cannot("The declaration cannot be found"))?;
    let mut edits = Vec::new();
    let mut problems = Vec::new();
    let new_name = options.name.trim();
    if new_name != info.name {
        edits.push(TextEdit {
            start: name.start_byte(),
            end: name.end_byte(),
            new_text: new_name.to_string(),
        });
    }
    if own_visibility && info.kind == "method" {
        let wanted = options.visibility.as_deref().unwrap_or("public");
        let mut c = f.walk();
        let current = f
            .children(&mut c)
            .find(|ch| ch.kind() == "visibility_modifier");
        match current {
            Some(v) if !text(v, src).eq_ignore_ascii_case(wanted) => edits.push(TextEdit {
                start: v.start_byte(),
                end: v.end_byte(),
                new_text: wanted.to_string(),
            }),
            Some(_) => {}
            None if wanted != "public" => {
                let mut c = f.walk();
                let before = f
                    .children(&mut c)
                    .find(|ch| matches!(ch.kind(), "static_modifier" | "function"))
                    .map_or(name.start_byte(), |n| n.start_byte());
                edits.push(TextEdit {
                    start: before,
                    end: before,
                    new_text: format!("{wanted} "),
                });
            }
            None => {}
        }
    }
    let params = f
        .child_by_field_name("parameters")
        .ok_or_else(|| cannot("The declaration has no parameter list"))?;
    let rendered: Vec<String> = options
        .parameters
        .iter()
        .map(|p| render_parameter(p, p.original.and_then(|i| info.parameters.get(i))))
        .collect();
    let old_text = text(params, src);
    let new_list = if old_text.contains('\n') && !rendered.is_empty() {
        let mut c = params.walk();
        let first = params
            .named_children(&mut c)
            .find(|p| p.kind().ends_with("parameter"));
        let indent = first.map_or_else(
            || "    ".to_string(),
            |p| super::leading_ws(src, p.start_byte()),
        );
        let close = super::leading_ws(src, params.end_byte() - 1);
        format!(
            "(\n{indent}{}\n{close})",
            rendered.join(&format!(",\n{indent}"))
        )
    } else {
        format!("({})", rendered.join(", "))
    };
    if new_list != old_text {
        edits.push(TextEdit {
            start: params.start_byte(),
            end: params.end_byte(),
            new_text: new_list,
        });
    }
    let ret = options.return_type.trim();
    match f.child_by_field_name("return_type") {
        Some(rt) if ret.is_empty() => edits.push(TextEdit {
            start: params.end_byte(),
            end: rt.end_byte(),
            new_text: String::new(),
        }),
        Some(rt) if text(rt, src) != ret => edits.push(TextEdit {
            start: rt.start_byte(),
            end: rt.end_byte(),
            new_text: ret.to_string(),
        }),
        Some(_) => {}
        None if !ret.is_empty() => edits.push(TextEdit {
            start: params.end_byte(),
            end: params.end_byte(),
            new_text: format!(": {ret}"),
        }),
        None => {}
    }
    // The body: renamed parameters, and removed ones it still uses.
    let body = f.child_by_field_name("body");
    let doc = {
        let s = with_doc(f, src);
        (s.start < f.start_byte()).then(|| Span::new(s.start, f.start_byte()))
    };
    for (i, old) in info.parameters.iter().enumerate() {
        let new = options.parameters.iter().find(|p| p.original == Some(i));
        let Some(body) = body else {
            if let (Some(d), None) = (doc, new) {
                edits.extend(remove_doc_param(src, d, &old.name));
            }
            continue;
        };
        match new {
            None => {
                if !variable_nodes(body, src, &old.name).is_empty() {
                    problems.push(format!(
                        "Parameter ${} is used in the body of {}()",
                        old.name, info.name
                    ));
                }
                if let Some(d) = doc {
                    edits.extend(remove_doc_param(src, d, &old.name));
                }
            }
            Some(p) => {
                let renamed = p.name.trim().trim_start_matches('$');
                if renamed != old.name {
                    for v in variable_nodes(body, src, &old.name) {
                        edits.push(TextEdit {
                            start: v.start_byte(),
                            end: v.end_byte(),
                            new_text: format!("${renamed}"),
                        });
                    }
                    if let Some(d) = doc {
                        edits.extend(rename_doc_param(src, d, &old.name, renamed));
                    }
                }
            }
        }
    }
    edits.sort_by_key(|e| (e.start, e.end));
    Ok((edits, problems))
}

/// `$name` as a whole word in `s`.
fn find_var(s: &str, name: &str) -> Option<usize> {
    let needle = format!("${name}");
    s.match_indices(&needle)
        .find(|(i, _)| {
            !s[i + needle.len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
        .map(|(i, _)| i)
}

fn param_lines(src: &str, doc: Span) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = doc.start;
    for line in src[doc.start..doc.end].split_inclusive('\n') {
        if line.contains("@param") {
            out.push((at, at + line.len()));
        }
        at += line.len();
    }
    out
}

fn rename_doc_param(src: &str, doc: Span, old: &str, new: &str) -> Vec<TextEdit> {
    param_lines(src, doc)
        .into_iter()
        .filter_map(|(s, e)| {
            let i = find_var(&src[s..e], old)?;
            Some(TextEdit {
                start: s + i,
                end: s + i + old.len() + 1,
                new_text: format!("${new}"),
            })
        })
        .collect()
}

fn remove_doc_param(src: &str, doc: Span, name: &str) -> Vec<TextEdit> {
    param_lines(src, doc)
        .into_iter()
        .filter(|(s, e)| find_var(&src[*s..*e], name).is_some())
        .map(|(s, e)| TextEdit {
            start: s,
            end: e,
            new_text: String::new(),
        })
        .collect()
}

/// An argument of a call: its name (named arguments) and value.
struct Arg<'t> {
    name: Option<String>,
    value: Node<'t>,
    spread: bool,
}

fn arguments_of(call: Node) -> Option<Node> {
    if let Some(a) = call.child_by_field_name("arguments") {
        return Some(a);
    }
    let mut c = call.walk();
    let found = call
        .named_children(&mut c)
        .find(|n| n.kind() == "arguments");
    found
}

fn args_of<'t>(args: Node<'t>, src: &str) -> (Vec<Arg<'t>>, bool) {
    let mut out = Vec::new();
    let mut callable = false;
    let mut c = args.walk();
    for a in args.named_children(&mut c) {
        match a.kind() {
            "argument" => {
                let name = a.child_by_field_name("name");
                let mut c2 = a.walk();
                let Some(value) = a
                    .named_children(&mut c2)
                    .filter(|v| Some(*v) != name)
                    .last()
                else {
                    continue;
                };
                out.push(Arg {
                    name: name.map(|n| text(n, src).to_string()),
                    spread: value.kind() == "variadic_unpacking"
                        || text(a, src).trim_start().starts_with("..."),
                    value,
                });
            }
            "variadic_placeholder" => callable = true,
            _ => {}
        }
    }
    (out, callable)
}

/// The call (or `new`) whose name is at `span`.
fn call_node<'t>(root: Node<'t>, span: Span) -> Option<Node<'t>> {
    if let Some(c) = super::introduce::call_at(root, span) {
        return Some(c);
    }
    let mut n = root.descendant_for_byte_range(span.start, span.end)?;
    for _ in 0..3 {
        let p = n.parent()?;
        if p.kind() == "object_creation_expression" && p.named_child(0) == Some(n) {
            return Some(p);
        }
        n = p;
    }
    None
}

/// The edits of one call site of the function whose current signature is `info`: the new
/// name, and the arguments in the new order (removed ones dropped, added ones with their
/// call value; named arguments stay named). `Err` describes a call that cannot be updated.
pub fn call_edits(
    src: &str,
    span: Span,
    info: &SignatureInfo,
    options: &ChangeSignatureOptions,
) -> Result<Vec<TextEdit>, String> {
    let tree = parse(src).ok_or_else(|| cannot("The file cannot be parsed"))?;
    let root = tree.root_node();
    let mut edits = Vec::new();
    let new_name = options.name.trim();
    let call = call_node(root, span);
    let is_new = call.is_some_and(|c| c.kind() == "object_creation_expression");
    let word = &src[span.start..span.end];
    if new_name != info.name && !is_new && word.eq_ignore_ascii_case(&info.name) {
        let n = root.descendant_for_byte_range(span.start, span.end);
        if n.is_some_and(|n| n.kind() == "name") {
            edits.push(TextEdit {
                start: span.start,
                end: span.end,
                new_text: new_name.to_string(),
            });
        }
    }
    let Some(call) = call else {
        return Ok(edits);
    };
    let Some(args_node) = arguments_of(call) else {
        // `new User` without parentheses.
        if is_new && !options.parameters.is_empty() {
            let list = new_arguments(&[], info, options)?;
            if !list.is_empty() {
                edits.push(TextEdit {
                    start: call.end_byte(),
                    end: call.end_byte(),
                    new_text: format!("({})", list.join(", ")),
                });
            }
        }
        return Ok(edits);
    };
    let (args, callable) = args_of(args_node, src);
    if callable {
        return Ok(edits);
    }
    if args.iter().any(|a| a.spread) {
        return Err(format!(
            "The call to {}() passes its arguments dynamically and is not updated",
            info.name
        ));
    }
    let list = new_arguments(
        &args
            .iter()
            .map(|a| (a.name.clone(), text(a.value, src).to_string()))
            .collect::<Vec<_>>(),
        info,
        options,
    )?;
    let old_inner = &src[args_node.start_byte() + 1..args_node.end_byte() - 1];
    let multiline = old_inner.contains('\n') && !args.is_empty();
    let new_inner = if multiline {
        let indent = super::leading_ws(src, args[0].value.start_byte());
        let close = super::leading_ws(src, args_node.end_byte() - 1);
        if list.is_empty() {
            String::new()
        } else {
            format!("\n{indent}{}\n{close}", list.join(&format!(",\n{indent}")))
        }
    } else {
        list.join(", ")
    };
    let same = {
        let squash = |s: &str| s.split_whitespace().collect::<String>().replace(",)", ")");
        squash(old_inner).trim_end_matches(',') == squash(&new_inner)
    };
    if !same {
        edits.push(TextEdit {
            start: args_node.start_byte() + 1,
            end: args_node.end_byte() - 1,
            new_text: new_inner,
        });
    }
    Ok(edits)
}

/// The new argument list from the old one (`(name, value)`, `name` for named arguments).
fn new_arguments(
    old: &[(Option<String>, String)],
    info: &SignatureInfo,
    options: &ChangeSignatureOptions,
) -> Result<Vec<String>, String> {
    let positional: Vec<&String> = old.iter().filter(|a| a.0.is_none()).map(|a| &a.1).collect();
    let named = |n: &str| old.iter().find(|a| a.0.as_deref() == Some(n)).map(|a| &a.1);
    let mut out = Vec::new();
    // Once an argument is left out (or was named), the rest must be named.
    let mut named_mode = false;
    for p in &options.parameters {
        let new_name = p.name.trim().trim_start_matches('$');
        let (values, was_named): (Vec<String>, bool) = match p.original {
            Some(i) => {
                let old_param = &info.parameters[i];
                if old_param.variadic {
                    (
                        positional.iter().skip(i).map(|s| s.to_string()).collect(),
                        false,
                    )
                } else if let Some(v) = positional.get(i) {
                    (vec![v.to_string()], false)
                } else if let Some(v) = named(&old_param.name) {
                    (vec![v.clone()], true)
                } else {
                    (vec![], false)
                }
            }
            None => match non_empty(&p.call_value) {
                Some(v) => (vec![v.to_string()], false),
                None => (vec![], false),
            },
        };
        if values.is_empty() {
            let optional = p.variadic || non_empty(&p.default_value).is_some();
            if !optional {
                return Err(format!("No value for the parameter ${new_name}"));
            }
            named_mode = true;
            continue;
        }
        if was_named || named_mode {
            named_mode = true;
            if p.variadic {
                // Named arguments cannot fill a variadic parameter by name.
                out.extend(values);
            } else {
                out.push(format!("{new_name}: {}", values[0]));
            }
        } else {
            out.extend(values);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::apply_edits;

    const CLASS: &str = "<?php\nclass Mailer\n{\n    /**\n     * @param string $to The address\n     * @param int $retries How often\n     */\n    public function send(string $to, int $retries = 3, ...$extra): bool\n    {\n        return mail($to, '', '') && $retries > 0;\n    }\n}\n";

    fn info() -> SignatureInfo {
        signature_at(CLASS, CLASS.find("send").unwrap()).unwrap()
    }

    #[test]
    fn reads_the_signature() {
        let i = info();
        assert_eq!(i.name, "send");
        assert_eq!(i.kind, "method");
        assert_eq!(i.class_name.as_deref(), Some("Mailer"));
        assert_eq!(i.visibility.as_deref(), Some("public"));
        assert_eq!(i.return_type.as_deref(), Some("bool"));
        assert_eq!(i.parameters.len(), 3);
        assert_eq!(i.parameters[1].default_value.as_deref(), Some("3"));
        assert!(i.parameters[2].variadic);
        assert!(i.parameters[0].used);
        assert!(!i.parameters[2].used);
        // Inside the body too.
        assert_eq!(
            signature_at(CLASS, CLASS.find("mail(").unwrap())
                .unwrap()
                .name,
            "send"
        );
        assert!(signature_at(CLASS, CLASS.find("Mailer").unwrap()).is_err());
        assert_eq!(
            render_signature(&i, &unchanged(&i)),
            "public function send(string $to, int $retries = 3, ...$extra): bool"
        );
    }

    #[test]
    fn changes_the_declaration() {
        let i = info();
        let mut o = unchanged(&i);
        o.name = "deliver".into();
        o.visibility = Some("protected".into());
        o.return_type = "void".into();
        o.parameters.swap(0, 1);
        o.parameters[1].name = "address".into();
        o.parameters.insert(
            2,
            NewParam {
                name: "html".into(),
                type_hint: Some("bool".into()),
                default_value: Some("false".into()),
                call_value: Some("true".into()),
                ..Default::default()
            },
        );
        check(&i, &o).unwrap();
        let (edits, problems) = declaration_edits(CLASS, &i, &o, true).unwrap();
        assert!(problems.is_empty());
        let out = apply_edits(CLASS, &edits);
        assert_eq!(
            out,
            "<?php\nclass Mailer\n{\n    /**\n     * @param string $address The address\n     * @param int $retries How often\n     */\n    protected function deliver(int $retries = 3, string $address, bool $html = false, ...$extra): void\n    {\n        return mail($address, '', '') && $retries > 0;\n    }\n}\n"
        );
        // Removing a used parameter is a problem; its PHPDoc goes.
        let o = without_parameter(&i, 1);
        let (edits, problems) = declaration_edits(CLASS, &i, &o, true).unwrap();
        assert_eq!(
            problems,
            ["Parameter $retries is used in the body of send()"]
        );
        let out = apply_edits(CLASS, &edits);
        assert!(out.contains("send(string $to, ...$extra): bool"));
        assert!(!out.contains("@param int $retries"));
    }

    #[test]
    fn rewrites_calls() {
        let i = info();
        let mut o = unchanged(&i);
        o.name = "deliver".into();
        o.parameters.swap(0, 1);
        o.parameters.insert(
            1,
            NewParam {
                name: "html".into(),
                call_value: Some("true".into()),
                ..Default::default()
            },
        );
        let call = |src: &str| {
            let at = src.find("send").unwrap();
            let edits = call_edits(src, Span::new(at, at + 4), &i, &o)?;
            Ok::<String, String>(apply_edits(src, &edits))
        };
        assert_eq!(
            call("<?php $m->send('a@b', 5, 'x', 'y');").unwrap(),
            "<?php $m->deliver(5, true, 'a@b', 'x', 'y');"
        );
        // The left-out default becomes named arguments after it.
        assert_eq!(
            call("<?php $m->send('a@b');").unwrap(),
            "<?php $m->deliver(html: true, to: 'a@b');"
        );
        assert_eq!(
            call("<?php $m->send(retries: 1, to: 'a@b');").unwrap(),
            "<?php $m->deliver(retries: 1, html: true, to: 'a@b');"
        );
        assert!(call("<?php $m->send(...$args);").is_err());
        assert_eq!(
            call("<?php $f = $m->send(...);").unwrap(),
            "<?php $f = $m->deliver(...);"
        );
        // Safe delete of a parameter.
        let o = without_parameter(&i, 0);
        let edits = call_edits(
            "<?php Mailer::send(\n    'a@b',\n    2,\n);",
            Span::new(14, 18),
            &i,
            &o,
        )
        .unwrap();
        assert_eq!(
            apply_edits("<?php Mailer::send(\n    'a@b',\n    2,\n);", &edits),
            "<?php Mailer::send(\n    2\n);"
        );
    }

    #[test]
    fn constructors_update_new_expressions() {
        let src = "<?php\nclass P { public function __construct(private int $x, int $y = 0) {} }\nnew P(1, 2);\nnew P;\n";
        let i = signature_at(src, src.find("__construct").unwrap()).unwrap();
        assert!(i.constructor);
        assert_eq!(i.parameters[0].promotion.as_deref(), Some("private"));
        let mut o = unchanged(&i);
        o.parameters.push(NewParam {
            name: "z".into(),
            call_value: Some("3".into()),
            ..Default::default()
        });
        let (edits, _) = declaration_edits(src, &i, &o, true).unwrap();
        assert!(apply_edits(src, &edits).contains("__construct(private int $x, int $y = 0, $z)"));
        let at = src.find("new P(").unwrap() + 4;
        let edits = call_edits(src, Span::new(at, at + 1), &i, &o).unwrap();
        assert!(apply_edits(src, &edits).contains("new P(1, 2, 3);"));
        // `new P` without arguments was broken before and cannot be fixed.
        let at = src.find("new P;").unwrap() + 4;
        assert!(call_edits(src, Span::new(at, at + 1), &i, &o).is_err());
    }

    #[test]
    fn checks_the_options() {
        let i = info();
        let mut o = unchanged(&i);
        o.name = "2x".into();
        assert!(check(&i, &o).is_err());
        let mut o = unchanged(&i);
        o.parameters[1].name = "to".into();
        assert_eq!(check(&i, &o).unwrap_err(), "Duplicate parameter name: $to");
        let mut o = unchanged(&i);
        o.parameters.push(NewParam {
            name: "x".into(),
            ..Default::default()
        });
        assert!(check(&i, &o).is_err());
    }
}
