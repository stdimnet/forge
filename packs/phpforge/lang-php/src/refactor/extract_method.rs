//! Extract Method / Extract Function (Ctrl+Alt+M) like PHPStorm: the selected statements or
//! expression become a new method of the class (a function outside classes). Variables the
//! fragment reads before writing become parameters; variables it changes that are used
//! afterwards are returned (several as an array, `[$a, $b] = $this->foo()`). The method is
//! static when the fragment does not use `$this`. Other fragments of the class with the same
//! code (variables renamed consistently) can be replaced by calls too.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use super::names::suggest;
use super::vars::{self, VarUse};
use super::{
    cannot, check_name, collect_nodes, enclosing_class, enclosing_function,
    enclosing_named_function, eol, expression_at, finish, fit, is_function_like,
    is_nested_declaration, is_statement_list, is_static_method, leading_ws, line_of, parse,
    reindent, scope_body, text, trim_range, Plan, Span,
};
use crate::generate::{Fmt, TextEdit};
use crate::model::Visibility;
use crate::parser::index_source;
use crate::style::PhpCodeStyle;

const NOT_STATEMENTS: &str = "Selected block should represent a set of statements or an expression";
const EXIT_POINTS: &str = "There are multiple exit points in the selected code fragment";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractParameter {
    /// Without `$`.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_hint: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReturnKind {
    /// Nothing is returned.
    #[default]
    None,
    /// An expression was extracted: its value.
    Expression,
    /// The variables in `outputs` (an array when there are several).
    Variables,
    /// The fragment ends with `return`: the call is returned.
    Return,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractMethodInfo {
    /// "Extract Method" or "Extract Function".
    pub title: String,
    /// The fragment (statements widened to whole statements).
    pub span: Span,
    /// A suggested name (may be empty).
    pub name: String,
    pub parameters: Vec<ExtractParameter>,
    /// Variables the new method returns (without `$`).
    pub outputs: Vec<String>,
    pub returns: ReturnKind,
    pub in_class: bool,
    /// The fragment uses `$this`: the method cannot be static.
    pub uses_this: bool,
    /// The fragment is in a static method: the new one must be static.
    pub static_context: bool,
    /// Other fragments with the same code that can be replaced by a call.
    pub duplicates: usize,
    /// The class the method goes to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterChoice {
    /// The variable in the fragment (without `$`).
    pub original: String,
    /// The parameter's name (without `$`).
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExtractMethodOptions {
    pub name: String,
    pub visibility: Option<Visibility>,
    pub is_static: bool,
    /// The parameters in their new order with their new names; empty keeps the defaults.
    pub parameters: Vec<ParameterChoice>,
    pub replace_duplicates: bool,
    pub style: PhpCodeStyle,
}

enum Fragment<'t> {
    Expression(Node<'t>),
    Statements(Vec<Node<'t>>),
}

impl<'t> Fragment<'t> {
    fn first(&self) -> Node<'t> {
        match self {
            Fragment::Expression(n) => *n,
            Fragment::Statements(s) => s[0],
        }
    }
    fn span(&self) -> Span {
        match self {
            Fragment::Expression(n) => Span::of(*n),
            Fragment::Statements(s) => Span::new(s[0].start_byte(), s[s.len() - 1].end_byte()),
        }
    }
    fn nodes(&self) -> Vec<Node<'t>> {
        match self {
            Fragment::Expression(n) => vec![*n],
            Fragment::Statements(s) => s.clone(),
        }
    }
}

fn fragment<'t>(
    root: Node<'t>,
    src: &str,
    start: usize,
    end: usize,
) -> Result<Fragment<'t>, String> {
    let (s, e) = {
        let t = &src[start..end];
        let lead = t.len() - t.trim_start().len();
        (start + lead, start + lead + t.trim().len())
    };
    if s >= e {
        return Err(cannot(NOT_STATEMENTS));
    }
    if !src[s..e].ends_with(';') && !src[s..e].ends_with('}') {
        if let Some(expr) = expression_at(root, src, s, e) {
            if expr.kind() == "variable_name" {
                return Err(cannot(NOT_STATEMENTS));
            }
            return Ok(Fragment::Expression(expr));
        }
    }
    // The statements of one list that the selection covers completely.
    let mut list = root
        .descendant_for_byte_range(s, e)
        .ok_or_else(|| cannot(NOT_STATEMENTS))?;
    while !(is_statement_list(list) && list.start_byte() <= s && e <= list.end_byte()) {
        list = list.parent().ok_or_else(|| cannot(NOT_STATEMENTS))?;
    }
    let mut c = list.walk();
    let mut picked = Vec::new();
    for ch in list.named_children(&mut c) {
        if ch.kind() == "comment" {
            continue;
        }
        let inside = s <= ch.start_byte() && ch.end_byte() <= e;
        let overlaps = ch.start_byte() < e && s < ch.end_byte();
        if overlaps && !inside {
            return Err(cannot(NOT_STATEMENTS));
        }
        if inside {
            picked.push(ch);
        }
    }
    if picked.is_empty() {
        return Err(cannot(NOT_STATEMENTS));
    }
    // Only whitespace and comments may be selected around them.
    let (ts, te) = trim_range(src, s, e);
    let first = picked[0].start_byte();
    let last = picked[picked.len() - 1].end_byte();
    let around_ok = |a: usize, b: usize| {
        let t = src[a.min(b)..a.max(b)].trim();
        t.is_empty() || t.starts_with("//") || t.starts_with("/*") || t.starts_with('#')
    };
    if !around_ok(ts, first) || !(around_ok(last, te) || last == e) {
        return Err(cannot(NOT_STATEMENTS));
    }
    for p in &picked {
        if is_nested_declaration(*p)
            || matches!(
                p.kind(),
                "namespace_definition"
                    | "namespace_use_declaration"
                    | "const_declaration"
                    | "php_tag"
                    | "text"
                    | "text_interpolation"
            )
        {
            return Err(cannot("Declarations cannot be extracted"));
        }
    }
    Ok(Fragment::Statements(picked))
}

/// Analysis of the fragment in its function.
struct Analysis<'t> {
    fragment: Fragment<'t>,
    root: Node<'t>,
    /// The innermost function-like scope (closures included).
    scope: Option<Node<'t>>,
    /// The method or function the new one is added next to.
    host: Option<Node<'t>>,
    class: Option<Node<'t>>,
    inputs: Vec<String>,
    outputs: Vec<String>,
    returns: ReturnKind,
    uses_this: bool,
    /// Uses in the fragment (for renaming parameters).
    inside: Vec<VarUse>,
}

fn analyze<'t>(
    root: Node<'t>,
    src: &str,
    start: usize,
    end: usize,
) -> Result<Analysis<'t>, String> {
    let fragment = fragment(root, src, start, end)?;
    let first = fragment.first();
    let scope = enclosing_function(first);
    let host = enclosing_named_function(first);
    let class = host.and_then(enclosing_class_of_method);
    let body = scope_body(scope, root);
    let span = fragment.span();
    let uses = vars::collect(body, src);
    let inside: Vec<VarUse> = uses
        .iter()
        .filter(|u| span.contains(u.span))
        .cloned()
        .collect();
    let mut inputs: Vec<String> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for u in &inside {
        if seen.insert(&u.name) && u.access.reads() {
            inputs.push(u.name.clone());
        }
    }
    // Inputs in the order they first appear in the code.
    inputs.sort_by_key(|n| {
        inside
            .iter()
            .filter(|u| &u.name == n)
            .map(|u| u.span.start)
            .min()
            .unwrap_or(0)
    });
    let written: Vec<&str> = {
        let mut w: Vec<&str> = Vec::new();
        for u in &inside {
            if u.access.writes() && !w.contains(&u.name.as_str()) {
                w.push(&u.name);
            }
        }
        w
    };
    let loop_range = enclosing_loop(first, body).map(Span::of);
    let mut outputs: Vec<String> = Vec::new();
    for name in &written {
        let after = uses
            .iter()
            .filter(|u| u.name == *name && u.span.start >= span.end)
            .min_by_key(|u| u.order);
        let used_after = after.is_some_and(|u| u.access.reads());
        // In a loop, the next iteration reads what this one wrote.
        let carried = loop_range.is_some_and(|l| {
            uses.iter()
                .any(|u| u.name == *name && l.contains(u.span) && u.access.reads())
        });
        if used_after || carried {
            outputs.push(name.to_string());
        }
    }
    let mut returns = match fragment {
        Fragment::Expression(_) => ReturnKind::Expression,
        Fragment::Statements(_) if outputs.is_empty() => ReturnKind::None,
        Fragment::Statements(_) => ReturnKind::Variables,
    };
    if let Fragment::Statements(stmts) = &fragment {
        let mut rets = Vec::new();
        for s in stmts {
            rets.extend(collect_nodes(*s, false, &mut |n| {
                n.kind() == "return_statement"
            }));
            if !collect_nodes(*s, false, &mut |n| n.kind() == "yield_expression").is_empty() {
                return Err(cannot("The selected code fragment contains yield"));
            }
            for jump in collect_nodes(*s, false, &mut |n| {
                matches!(n.kind(), "break_statement" | "continue_statement")
            }) {
                if escapes(jump, span, src) {
                    return Err(cannot(EXIT_POINTS));
                }
            }
        }
        if !rets.is_empty() {
            let last = stmts[stmts.len() - 1];
            if last.kind() != "return_statement" {
                return Err(cannot(EXIT_POINTS));
            }
            let valueless = rets.iter().all(|r| r.named_child_count() == 0);
            if valueless && rets.len() > 1 {
                return Err(cannot(EXIT_POINTS));
            }
            // Nothing after the fragment runs: no variables to hand back.
            outputs.clear();
            returns = ReturnKind::Return;
        }
    }
    let uses_this = fragment.nodes().iter().any(|n| {
        vars::uses_this(*n, src)
            || collect_nodes(*n, true, &mut |x| {
                x.kind() == "relative_scope" && text(x, src) == "parent"
            })
            .iter()
            .any(|_| host.is_some_and(|h| !is_static_method(h)))
    });
    Ok(Analysis {
        fragment,
        root,
        scope,
        host,
        class,
        inputs,
        outputs,
        returns,
        uses_this,
        inside,
    })
}

fn enclosing_class_of_method(host: Node) -> Option<Node> {
    if host.kind() == "method_declaration" {
        enclosing_class(host)
    } else {
        None
    }
}

/// The innermost loop around `n` inside `body`.
fn enclosing_loop<'t>(n: Node<'t>, body: Node<'t>) -> Option<Node<'t>> {
    let mut cur = n.parent();
    while let Some(p) = cur {
        if p == body || is_function_like(p) {
            return None;
        }
        if matches!(
            p.kind(),
            "for_statement" | "foreach_statement" | "while_statement" | "do_statement"
        ) {
            return Some(p);
        }
        cur = p.parent();
    }
    None
}

/// A `break` / `continue` that leaves the fragment.
fn escapes(jump: Node, fragment: Span, src: &str) -> bool {
    let levels: usize = jump
        .named_child(0)
        .and_then(|l| text(l, src).trim().parse().ok())
        .unwrap_or(1);
    let mut inner = 0;
    let mut cur = jump.parent();
    while let Some(p) = cur {
        if !fragment.contains(Span::of(p)) {
            break;
        }
        if matches!(
            p.kind(),
            "for_statement"
                | "foreach_statement"
                | "while_statement"
                | "do_statement"
                | "switch_statement"
        ) {
            inner += 1;
        }
        cur = p.parent();
    }
    inner < levels
}

/// The declared type of a parameter of the scope's function.
fn parameter_type(scope: Option<Node>, name: &str, src: &str) -> Option<String> {
    let params = scope?.child_by_field_name("parameters")?;
    let mut c = params.walk();
    let found = params.named_children(&mut c).find(|p| {
        p.child_by_field_name("name")
            .is_some_and(|n| text(n, src).trim_start_matches('$') == name)
    })?;
    found
        .child_by_field_name("type")
        .map(|t| text(t, src).to_string())
}

/// Check the selection and describe the dialog.
pub fn prepare(src: &str, start: usize, end: usize) -> Result<ExtractMethodInfo, String> {
    let tree = parse(src).ok_or_else(|| cannot(NOT_STATEMENTS))?;
    let a = analyze(tree.root_node(), src, start, end)?;
    let in_class = a.class.is_some();
    let duplicates = find_duplicates(&a, src).len();
    let name = match a.fragment {
        Fragment::Expression(n) => {
            let s = suggest(n, src);
            let first = s.first().cloned().unwrap_or_default();
            if first == "value" || first.is_empty() {
                String::new()
            } else {
                let mut cs = first.chars();
                let head = cs
                    .next()
                    .map(|c| c.to_uppercase().collect::<String>())
                    .unwrap_or_default();
                format!("get{head}{}", cs.as_str())
            }
        }
        Fragment::Statements(_) => String::new(),
    };
    Ok(ExtractMethodInfo {
        title: if in_class {
            "Extract Method"
        } else {
            "Extract Function"
        }
        .into(),
        span: a.fragment.span(),
        name,
        parameters: a
            .inputs
            .iter()
            .map(|n| ExtractParameter {
                name: n.clone(),
                type_hint: parameter_type(a.scope, n, src),
            })
            .collect(),
        outputs: a.outputs.clone(),
        returns: a.returns,
        in_class,
        uses_this: a.uses_this,
        static_context: a.host.is_some_and(is_static_method),
        duplicates,
        class_name: a
            .class
            .and_then(|c| c.child_by_field_name("name"))
            .map(|n| text(n, src).to_string()),
    })
}

/// A duplicate: the nodes and how its variables map to the fragment's.
struct Duplicate<'t> {
    nodes: Vec<Node<'t>>,
    /// Fragment variable -> duplicate variable.
    vars: HashMap<String, String>,
}

/// Leaf tokens with variables apart, for matching code up to variable names.
fn shape<'a>(n: Node, src: &'a str, out: &mut Vec<(bool, &'a str)>) {
    if n.kind() == "comment" {
        return;
    }
    if n.kind() == "variable_name" {
        out.push((true, text(n, src)));
        return;
    }
    if n.child_count() == 0 {
        out.push((false, text(n, src)));
        return;
    }
    let mut c = n.walk();
    for ch in n.children(&mut c) {
        shape(ch, src, out);
    }
}

fn match_code(a: &[Node], b: &[Node], src: &str) -> Option<HashMap<String, String>> {
    if a.len() != b.len() {
        return None;
    }
    let mut sa = Vec::new();
    let mut sb = Vec::new();
    for (x, y) in a.iter().zip(b) {
        if x.kind() != y.kind() {
            return None;
        }
        shape(*x, src, &mut sa);
        shape(*y, src, &mut sb);
    }
    if sa.len() != sb.len() {
        return None;
    }
    let mut map: HashMap<String, String> = HashMap::new();
    let mut back: HashMap<String, String> = HashMap::new();
    for ((va, ta), (vb, tb)) in sa.iter().zip(&sb) {
        if va != vb {
            return None;
        }
        if !va {
            if ta != tb {
                return None;
            }
            continue;
        }
        let (na, nb) = (ta.trim_start_matches('$'), tb.trim_start_matches('$'));
        if (na == "this") != (nb == "this") {
            return None;
        }
        match (map.get(na), back.get(nb)) {
            (Some(m), _) if m != nb => return None,
            (_, Some(m)) if m != na => return None,
            _ => {
                map.insert(na.to_string(), nb.to_string());
                back.insert(nb.to_string(), na.to_string());
            }
        }
    }
    Some(map)
}

fn find_duplicates<'t>(a: &Analysis<'t>, src: &str) -> Vec<Duplicate<'t>> {
    let area = a.class.unwrap_or(a.root);
    let own = a.fragment.span();
    let mut out: Vec<Duplicate<'t>> = Vec::new();
    match &a.fragment {
        Fragment::Expression(expr) => {
            let kind = expr.kind();
            for cand in collect_nodes(area, true, &mut |n| n.kind() == kind) {
                if Span::of(cand).overlaps(own)
                    || out
                        .iter()
                        .any(|d| Span::of(d.nodes[0]).contains(Span::of(cand)))
                {
                    continue;
                }
                if let Some(vars) = match_code(&[*expr], &[cand], src) {
                    out.push(Duplicate {
                        nodes: vec![cand],
                        vars,
                    });
                }
            }
        }
        Fragment::Statements(stmts) => {
            let k = stmts.len();
            for list in collect_nodes(area, true, &mut |n| is_statement_list(n)) {
                let mut c = list.walk();
                let children: Vec<Node> = list
                    .named_children(&mut c)
                    .filter(|n| n.kind() != "comment")
                    .collect();
                let mut i = 0;
                while i + k <= children.len() {
                    let window = &children[i..i + k];
                    let span = Span::new(window[0].start_byte(), window[k - 1].end_byte());
                    if !span.overlaps(own) && !span.contains(own) && !own.contains(span) {
                        if let Some(vars) = match_code(stmts, window, src) {
                            out.push(Duplicate {
                                nodes: window.to_vec(),
                                vars,
                            });
                            i += k;
                            continue;
                        }
                    }
                    i += 1;
                }
            }
        }
    }
    out.sort_by_key(|d| d.nodes[0].start_byte());
    out
}

/// Extract the method with the dialog's choices.
pub fn extract_method(
    src: &str,
    path: &str,
    start: usize,
    end: usize,
    opts: &ExtractMethodOptions,
) -> Result<Plan, String> {
    let name = opts.name.trim().to_string();
    check_name(&name)?;
    let tree = parse(src).ok_or_else(|| cannot(NOT_STATEMENTS))?;
    let a = analyze(tree.root_node(), src, start, end)?;
    let nl = eol(src);
    let is_static = (opts.is_static || a.host.is_some_and(is_static_method)) && a.class.is_some();
    if is_static && a.uses_this {
        return Err(cannot(
            "The fragment uses $this, so the method cannot be static",
        ));
    }
    // Parameters in the chosen order and names.
    let params: Vec<ParameterChoice> = if opts.parameters.is_empty() {
        a.inputs
            .iter()
            .map(|n| ParameterChoice {
                original: n.clone(),
                name: n.clone(),
            })
            .collect()
    } else {
        let mut chosen: Vec<ParameterChoice> = opts
            .parameters
            .iter()
            .filter(|p| a.inputs.contains(&p.original))
            .cloned()
            .collect();
        for i in &a.inputs {
            if !chosen.iter().any(|p| &p.original == i) {
                chosen.push(ParameterChoice {
                    original: i.clone(),
                    name: i.clone(),
                });
            }
        }
        chosen
    };
    let mut seen = HashSet::new();
    for p in &params {
        check_name(&p.name)?;
        if !seen.insert(p.name.clone()) {
            return Err(format!("Duplicate parameter name ${}", p.name));
        }
    }
    let renames: HashMap<&str, &str> = params
        .iter()
        .filter(|p| p.original != p.name)
        .map(|p| (p.original.as_str(), p.name.as_str()))
        .collect();
    let mut conflicts = Vec::new();
    // Name clashes.
    let siblings: Vec<Node> = match a.class {
        Some(c) => collect_nodes(c, false, &mut |n| n.kind() == "method_declaration"),
        None => collect_nodes(a.root, false, &mut |n| n.kind() == "function_definition"),
    };
    if let Some(clash) = siblings.iter().find(|m| {
        m.child_by_field_name("name")
            .is_some_and(|n| text(n, src).eq_ignore_ascii_case(&name))
    }) {
        let what = match a.class.and_then(|c| c.child_by_field_name("name")) {
            Some(c) => format!(
                "Method {name}() is already defined in class {}",
                text(c, src)
            ),
            None => format!("Function {name}() already exists"),
        };
        conflicts.push(super::conflict_at(what, path, src, clash.start_byte()));
    }

    // The new method's body: the fragment with renamed parameters.
    let span = a.fragment.span();
    let body_code = {
        let mut code = src[span.start..span.end].to_string();
        let mut edits: Vec<(usize, usize, String)> = a
            .inside
            .iter()
            .filter_map(|u| {
                renames
                    .get(u.name.as_str())
                    .map(|to| (u.span.start, u.span.end, format!("${to}")))
            })
            .collect();
        edits.sort_by_key(|e| std::cmp::Reverse(e.0));
        for (s, e, t) in edits {
            code.replace_range(s - span.start..e - span.start, &t);
        }
        code
    };
    let first_indent = leading_ws(src, span.start);
    let body = match a.fragment {
        Fragment::Expression(_) => {
            format!("return {};", reindent(&body_code, &first_indent, "    "))
        }
        Fragment::Statements(_) => {
            let mut b = reindent(&body_code, &first_indent, "    ");
            match a.outputs.len() {
                0 => {}
                1 => b.push_str(&format!("\n\n    return ${};", a.outputs[0])),
                _ => b.push_str(&format!(
                    "\n\n    return [{}];",
                    a.outputs
                        .iter()
                        .map(|o| format!("${o}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            }
            b
        }
    };
    let param_list = params
        .iter()
        .map(|p| {
            let ty = parameter_type(a.scope, &p.original, src)
                .map(|t| format!("{t} "))
                .unwrap_or_default();
            format!("{ty}${}", p.name)
        })
        .collect::<Vec<_>>()
        .join(", ");
    let header = match a.class {
        Some(_) => format!(
            "{} {}function {name}({param_list})",
            opts.visibility.unwrap_or(Visibility::Private).as_str(),
            if is_static { "static " } else { "" },
        ),
        None => format!("function {name}({param_list})"),
    };
    let block = format!("{header}\n{{\n    {body}\n}}");

    let mut edits = Vec::new();
    // The call replacing the fragment.
    let call_for = |vars: &HashMap<String, String>| {
        let args = params
            .iter()
            .map(|p| format!("${}", vars.get(&p.original).unwrap_or(&p.original)))
            .collect::<Vec<_>>()
            .join(", ");
        let callee = match (a.class.is_some(), is_static) {
            (true, true) => format!("self::{name}"),
            (true, false) => format!("$this->{name}"),
            _ => name.clone(),
        };
        let call = format!("{callee}({args})");
        let outs: Vec<String> = a
            .outputs
            .iter()
            .map(|o| format!("${}", vars.get(o).unwrap_or(o)))
            .collect();
        match a.returns {
            ReturnKind::Expression => call,
            ReturnKind::Return => {
                let valueless = match &a.fragment {
                    Fragment::Statements(s) => s[s.len() - 1].named_child_count() == 0,
                    _ => false,
                };
                if valueless {
                    format!("{call};{nl}{{indent}}return;")
                } else {
                    format!("return {call};")
                }
            }
            ReturnKind::None => format!("{call};"),
            ReturnKind::Variables if outs.len() == 1 => format!("{} = {call};", outs[0]),
            ReturnKind::Variables => format!("[{}] = {call};", outs.join(", ")),
        }
    };
    let identity: HashMap<String, String> = HashMap::new();
    let replace = |nodes: &[Node], vars: &HashMap<String, String>| -> TextEdit {
        let s = nodes[0].start_byte();
        let e = nodes[nodes.len() - 1].end_byte();
        let indent = leading_ws(src, s);
        let mut call = call_for(vars).replace("{indent}", &indent);
        if let Fragment::Expression(_) = a.fragment {
            call = fit(nodes[0], &call, "member_call_expression");
        }
        TextEdit {
            start: s,
            end: e,
            new_text: call,
        }
    };
    edits.push(replace(&a.fragment.nodes(), &identity));
    if opts.replace_duplicates {
        for d in find_duplicates(&a, src) {
            edits.push(replace(&d.nodes, &d.vars));
        }
    }

    // Insert the method after the one the fragment is in.
    let insert = match (a.class, a.host) {
        (Some(class), Some(host)) => {
            let file = index_source(path, src);
            let info = file
                .class_at(class.start_byte() + 1)
                .or_else(|| file.class_at(class.end_byte() - 1))
                .ok_or_else(|| cannot("The class cannot be parsed"))?;
            let fmt = Fmt::for_class(src, info, &opts.style);
            TextEdit {
                start: host.end_byte(),
                end: host.end_byte(),
                new_text: fmt.eol(format!(
                    "{}{}",
                    fmt.member_separator(),
                    fmt.indent_block(&block)
                )),
            }
        }
        (None, Some(host)) => {
            let fmt = plain_fmt(src, &leading_ws(src, host.start_byte()), &opts.style);
            TextEdit {
                start: host.end_byte(),
                end: host.end_byte(),
                new_text: fmt.eol(format!("\n\n{}", fmt.indent_block(&block))),
            }
        }
        _ => {
            let fmt = plain_fmt(src, "", &opts.style);
            let at = src.trim_end().len();
            TextEdit {
                start: at,
                end: at,
                new_text: fmt.eol(format!("\n\n{}", fmt.indent_block(&block))),
            }
        }
    };
    edits.push(insert);
    // The new name is edited in place afterwards: mark the declaration and the calls.
    for e in &mut edits {
        e.new_text = mark_name(&e.new_text, &name);
    }
    let (edits, name_spans) = finish(src, edits);
    let _ = line_of;
    Ok(Plan {
        edits,
        other_files: vec![],
        conflicts,
        name_spans,
    })
}

fn plain_fmt(src: &str, indent: &str, style: &PhpCodeStyle) -> Fmt {
    Fmt {
        indent: indent.to_string(),
        unit: super::indent_unit(src),
        class_indent: String::new(),
        crlf: src.contains("\r\n"),
        style: *style,
    }
}

/// Marks `name` in `function name(`, `->name(`, `::name(` and a bare call `name(`.
fn mark_name(code: &str, name: &str) -> String {
    let mut out = String::with_capacity(code.len() + 8);
    let mut rest = code;
    let pat = format!("{name}(");
    while let Some(i) = rest.find(&pat) {
        let before = &rest[..i];
        let boundary = before
            .chars()
            .last()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '$'));
        out.push_str(before);
        if boundary {
            out.push_str(&super::marked(name));
        } else {
            out.push_str(name);
        }
        out.push('(');
        rest = &rest[i + pat.len()..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refactor::test_util::{apply, sel};

    fn info(marked: &str) -> ExtractMethodInfo {
        let (src, s, e) = sel(marked);
        prepare(&src, s, e).unwrap()
    }

    fn run(marked: &str, opts: ExtractMethodOptions) -> (String, Plan) {
        let (src, s, e) = sel(marked);
        let plan = extract_method(&src, "a.php", s, e, &opts).unwrap();
        (apply(&src, &plan.edits), plan)
    }

    fn named(name: &str) -> ExtractMethodOptions {
        ExtractMethodOptions {
            name: name.into(),
            ..Default::default()
        }
    }

    const CART: &str = "<?php
class Cart
{
    private array $items = [];

    public function total(float $discount): float
    {
        ‹$sum = 0;
        foreach ($this->items as $item) {
            $sum += $item->price * $item->quantity;
        }›
        return $sum * (1 - $discount);
    }
}
";

    #[test]
    fn finds_parameters_and_return_values() {
        let i = info(CART);
        assert_eq!(i.title, "Extract Method");
        assert!(i.parameters.is_empty());
        assert_eq!(i.outputs, vec!["sum"]);
        assert_eq!(i.returns, ReturnKind::Variables);
        assert!(i.uses_this);
        assert!(i.in_class);
    }

    #[test]
    fn extracts_statements_into_a_private_method_after_the_current_one() {
        let (out, plan) = run(CART, named("sumItems"));
        assert_eq!(
            out,
            "<?php
class Cart
{
    private array $items = [];

    public function total(float $discount): float
    {
        $sum = $this->sumItems();
        return $sum * (1 - $discount);
    }

    private function sumItems()
    {
        $sum = 0;
        foreach ($this->items as $item) {
            $sum += $item->price * $item->quantity;
        }

        return $sum;
    }
}
"
        );
        assert_eq!(plan.name_spans.len(), 2);
        for s in &plan.name_spans {
            assert_eq!(&out[s.start..s.end], "sumItems");
        }
    }

    #[test]
    fn passes_parameters_with_their_types_renamed_and_reordered() {
        let src = "<?php
class Price
{
    public function apply(float $amount, int $percent)
    {
        $result = ‹$amount * $percent / 100›;
        return $result;
    }
}
";
        let i = info(src);
        assert_eq!(
            i.parameters
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["amount", "percent"]
        );
        assert_eq!(i.returns, ReturnKind::Expression);
        assert!(!i.uses_this);
        assert_eq!(i.name, "getResult");
        let opts = ExtractMethodOptions {
            name: "percentOf".into(),
            is_static: true,
            parameters: vec![
                ParameterChoice {
                    original: "percent".into(),
                    name: "rate".into(),
                },
                ParameterChoice {
                    original: "amount".into(),
                    name: "amount".into(),
                },
            ],
            ..Default::default()
        };
        let (out, _) = run(src, opts);
        assert!(
            out.contains("$result = self::percentOf($percent, $amount);"),
            "{out}"
        );
        assert!(out.contains("    private static function percentOf(int $rate, float $amount)\n    {\n        return $amount * $rate / 100;\n    }"), "{out}");
    }

    #[test]
    fn several_outputs_are_destructured() {
        let src = "<?php
function stats(array $xs)
{
    ‹$min = min($xs);
    $max = max($xs);›
    return $max - $min;
}
";
        let (out, _) = run(src, named("bounds"));
        assert!(
            out.contains("    [$min, $max] = bounds($xs);\n    return $max - $min;"),
            "{out}"
        );
        assert!(out.contains("}\n\nfunction bounds(array $xs)\n{\n    $min = min($xs);\n    $max = max($xs);\n\n    return [$min, $max];\n}\n"), "{out}");
    }

    #[test]
    fn fragments_ending_with_return_are_returned() {
        let src = "<?php
function f($x)
{
    $y = 1;
    ‹if ($x) {
        return $y;
    }
    return $x + $y;›
}
";
        let i = info(src);
        assert_eq!(i.returns, ReturnKind::Return);
        let (out, _) = run(src, named("pick"));
        assert!(
            out.contains("    $y = 1;\n    return pick($x, $y);\n}"),
            "{out}"
        );
    }

    #[test]
    fn rejects_multiple_exit_points_and_partial_statements() {
        let (src, s, e) = sel("<?php\nfunction f($x)\n{\n    ‹if ($x) {\n        return 1;\n    }\n    $x++;›\n    return $x;\n}\n");
        assert_eq!(prepare(&src, s, e).unwrap_err(), "Cannot perform refactoring. There are multiple exit points in the selected code fragment");
        let (src, s, e) = sel("<?php\nforeach ($a as $b) {\n    ‹if ($b) break;›\n}\n");
        assert!(prepare(&src, s, e).is_err());
        let (src, s, e) = sel("<?php\n$a = ‹1;\n$b› = 2;\n");
        assert!(prepare(&src, s, e).is_err());
    }

    #[test]
    fn loop_carried_variables_are_returned() {
        let src = "<?php
function f($items)
{
    $total = 0;
    foreach ($items as $i) {
        ‹$total = $total + $i;›
    }
}
";
        let i = info(src);
        assert_eq!(
            i.parameters
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["total", "i"]
        );
        assert_eq!(i.outputs, vec!["total"]);
    }

    #[test]
    fn replaces_duplicates_with_mapped_variables() {
        let src = "<?php
class Report
{
    public function a($rows)
    {
        ‹$count = count($rows);
        echo $count;›
    }

    public function b($lines)
    {
        $n = count($lines);
        echo $n;
    }
}
";
        let i = info(src);
        assert_eq!(i.duplicates, 1);
        assert_eq!(i.outputs, Vec::<String>::new());
        let opts = ExtractMethodOptions {
            replace_duplicates: true,
            is_static: true,
            ..named("printCount")
        };
        let (out, _) = run(src, opts);
        assert!(
            out.contains(
                "    public function a($rows)\n    {\n        self::printCount($rows);\n    }"
            ),
            "{out}"
        );
        assert!(
            out.contains(
                "    public function b($lines)\n    {\n        self::printCount($lines);\n    }"
            ),
            "{out}"
        );
        assert!(
            out.contains("    private static function printCount($rows)\n"),
            "{out}"
        );
    }

    #[test]
    fn extracts_from_closures_and_script_code() {
        let src = "<?php
class A
{
    public function f($xs)
    {
        return array_map(function ($x) {
            return ‹$x * $this->rate›;
        }, $xs);
    }
}
";
        let i = info(src);
        assert_eq!(
            i.parameters
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["x"]
        );
        assert!(i.uses_this);
        let (out, _) = run(src, named("scaled"));
        assert!(out.contains("return $this->scaled($x);"), "{out}");
        assert!(out.contains("    private function scaled($x)\n    {\n        return $x * $this->rate;\n    }\n}\n"), "{out}");
        let (out, _) = run(
            "<?php
$a = 1;
‹echo $a;
echo $a + 1;›
",
            named("show"),
        );
        assert_eq!(
            out,
            "<?php
$a = 1;
show($a);

function show($a)
{
    echo $a;
    echo $a + 1;
}
"
        );
    }
}
