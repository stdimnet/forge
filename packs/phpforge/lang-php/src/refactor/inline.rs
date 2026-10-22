//! Inline (Ctrl+Alt+N) like PHPStorm: a local variable is replaced by its initializer, a
//! method or function call by the body, a constant by its value. "All" replaces every
//! reference and removes the declaration, "this" only the one at the caret. Changes in
//! behaviour (an initializer with side effects evaluated several times, its variables
//! changed in between) are conflicts for "Problems Detected".
//!
//! Variables are inlined within the file ([`inline_variable`]); methods, functions and
//! constants at the usages Find Usages found ([`CallableBody`], [`ConstantValue`]).

use std::collections::{HashMap, HashSet};

use forge_codeintel::RenameConflict;
use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use super::introduce::{call_args, call_at, receiver, substitute};
use super::names::unique;
use super::vars::{self, Access, VarUse};
use super::{
    cannot, collect_nodes, conflict_at, enclosing_class, enclosing_function, eol, fit,
    has_side_effects, leading_ws, line_of, line_start, parse, reindent, scope_body, statement_of,
    statement_removal, text, FileEdits, Plan, Span, UsageSpan,
};
use crate::generate::TextEdit;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InlineKind {
    Variable,
    Method,
    Function,
    Constant,
}

impl InlineKind {
    pub fn title(self) -> &'static str {
        match self {
            Self::Variable => "Inline Variable",
            Self::Method => "Inline Method",
            Self::Function => "Inline Function",
            Self::Constant => "Inline Constant",
        }
    }
}

/// How much to inline.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InlineMode {
    /// Every reference, and the declaration is removed.
    #[default]
    All,
    /// Every reference, the declaration stays.
    AllKeep,
    /// Only the reference at the caret.
    This,
}

/// What is at the caret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineTarget {
    pub kind: InlineKind,
    /// `$total`, `total()`, `MAX`.
    pub name: String,
    /// The caret is on a reference / call (not the declaration).
    pub on_reference: bool,
    /// The name at the caret (for Find Usages).
    pub span: Span,
    /// Variables: the number of references.
    pub occurrences: usize,
}

const NOTHING: &str = "Caret should be positioned at a variable, method, function or constant";

/// What can be inlined at `pos`.
pub fn target_at(src: &str, pos: usize) -> Result<InlineTarget, String> {
    let tree = parse(src).ok_or_else(|| cannot(NOTHING))?;
    let root = tree.root_node();
    for at in [pos, pos.saturating_sub(1)] {
        let Some(mut n) = root.descendant_for_byte_range(at, at) else {
            continue;
        };
        // `$` / `name` inside a variable.
        if n.kind() == "$"
            || (n.kind() == "name" && n.parent().is_some_and(|p| p.kind() == "variable_name"))
        {
            n = n.parent().unwrap_or(n);
        }
        if n.kind() == "variable_name" {
            let var = variable_definition(root, src, n)?;
            return Ok(InlineTarget {
                kind: InlineKind::Variable,
                name: text(n, src).to_string(),
                on_reference: var.at_reference,
                span: Span::of(n),
                occurrences: var.references.len(),
            });
        }
        if n.kind() != "name" {
            continue;
        }
        let Some(p) = n.parent() else { continue };
        let name = text(n, src).to_string();
        let target = |kind, on_reference| {
            Ok(InlineTarget {
                kind,
                name: if matches!(kind, InlineKind::Method | InlineKind::Function) {
                    format!("{name}()")
                } else {
                    name.clone()
                },
                on_reference,
                span: Span::of(n),
                occurrences: 0,
            })
        };
        match p.kind() {
            "method_declaration" => {
                if p.child_by_field_name("body").is_none() {
                    return Err(cannot("Abstract methods cannot be inlined"));
                }
                return target(InlineKind::Method, false);
            }
            "function_definition" => return target(InlineKind::Function, false),
            "member_call_expression"
            | "nullsafe_member_call_expression"
            | "scoped_call_expression"
                if p.child_by_field_name("name") == Some(n) =>
            {
                return target(InlineKind::Method, true)
            }
            "function_call_expression" => return target(InlineKind::Function, true),
            "qualified_name"
                if p.parent()
                    .is_some_and(|g| g.kind() == "function_call_expression") =>
            {
                return target(InlineKind::Function, true)
            }
            "const_element" => return target(InlineKind::Constant, false),
            "class_constant_access_expression" if p.named_child(0) != Some(n) => {
                if name == "class" {
                    return Err(cannot(NOTHING));
                }
                return target(InlineKind::Constant, true);
            }
            "binary_expression"
            | "argument"
            | "return_statement"
            | "array_element_initializer"
            | "parenthesized_expression"
            | "conditional_expression"
            | "echo_statement"
            | "assignment_expression"
            | "qualified_name" => return target(InlineKind::Constant, true),
            "enum_case" => return Err(cannot("Enum cases cannot be inlined")),
            _ => {}
        }
    }
    Err(cannot(NOTHING))
}

// ---- Variables ----

struct VariableDef<'t> {
    name: String,
    /// The `$x = <value>` assignment and its statement.
    assignment: Node<'t>,
    statement: Node<'t>,
    value: Node<'t>,
    references: Vec<VarUse>,
    uses: Vec<VarUse>,
    at_reference: bool,
    /// The reference at the caret.
    at: Span,
}

fn variable_definition<'t>(
    root: Node<'t>,
    src: &str,
    at: Node<'t>,
) -> Result<VariableDef<'t>, String> {
    let name = text(at, src).trim_start_matches('$').to_string();
    if name == "this" {
        return Err(cannot("$this cannot be inlined"));
    }
    let function = enclosing_function(at);
    let body = scope_body(function, root);
    if let Some(params) = function.and_then(|f| f.child_by_field_name("parameters")) {
        if vars::parameter_names(params, src).contains(&name) {
            return Err(cannot(format!("Parameter ${name} cannot be inlined")));
        }
    }
    let uses: Vec<VarUse> = vars::collect(body, src)
        .into_iter()
        .filter(|u| u.name == name)
        .collect();
    let writes: Vec<&VarUse> = uses.iter().filter(|u| u.access.writes()).collect();
    let def = match writes.as_slice() {
        [] => return Err(cannot(format!("Variable ${name} has no initializer"))),
        [w] if w.access == Access::Write => *w,
        _ => return Err(cannot(format!("Variable ${name} is accessed for writing"))),
    };
    let var_node = root
        .descendant_for_byte_range(def.span.start, def.span.end)
        .and_then(|n| {
            if n.kind() == "variable_name" {
                Some(n)
            } else {
                n.parent()
            }
        })
        .ok_or_else(|| cannot(NOTHING))?;
    let assignment = var_node
        .parent()
        .filter(|p| {
            p.kind() == "assignment_expression" && p.child_by_field_name("left") == Some(var_node)
        })
        .ok_or_else(|| cannot(format!("Variable ${name} has no initializer")))?;
    let statement = assignment
        .parent()
        .filter(|s| {
            s.kind() == "expression_statement" && super::is_statement_list(s.parent().unwrap_or(*s))
        })
        .ok_or_else(|| {
            cannot(format!(
                "The initializer of ${name} is not a statement of its own"
            ))
        })?;
    let value = assignment
        .child_by_field_name("right")
        .ok_or_else(|| cannot(format!("Variable ${name} has no initializer")))?;
    let references: Vec<VarUse> = uses
        .iter()
        .filter(|u| u.access == Access::Read && u.span.start >= assignment.end_byte())
        .cloned()
        .collect();
    if references.is_empty() {
        return Err(cannot(format!("Variable ${name} is never used")));
    }
    let at_span = Span::of(at);
    Ok(VariableDef {
        name,
        assignment,
        statement,
        value,
        at_reference: references.iter().any(|r| r.span == at_span),
        references,
        uses,
        at: at_span,
    })
}

/// Inline the variable at `pos` (its declaration or a reference).
pub fn inline_variable(
    src: &str,
    path: &str,
    pos: usize,
    mode: InlineMode,
) -> Result<Plan, String> {
    let tree = parse(src).ok_or_else(|| cannot(NOTHING))?;
    let root = tree.root_node();
    let mut at = root
        .descendant_for_byte_range(pos, pos)
        .ok_or_else(|| cannot(NOTHING))?;
    while at.kind() != "variable_name" {
        at = match at.parent() {
            Some(p) => p,
            None => return Err(cannot(NOTHING)),
        };
    }
    let def = variable_definition(root, src, at)?;
    let refs: Vec<&VarUse> = match mode {
        InlineMode::This if def.at_reference => {
            def.references.iter().filter(|r| r.span == def.at).collect()
        }
        _ => def.references.iter().collect(),
    };
    let value_text = text(def.value, src);
    let value_kind = def.value.kind();
    let mut conflicts = Vec::new();
    let mut edits = Vec::new();
    let body = scope_body(enclosing_function(at), root);
    let all_uses = vars::collect(body, src);
    // Variables of the initializer changed between the declaration and a reference.
    let operands: HashSet<String> = vars::collect(def.value, src)
        .into_iter()
        .map(|u| u.name)
        .collect();
    for r in &refs {
        let node = root
            .descendant_for_byte_range(r.span.start, r.span.end)
            .map(|n| {
                if n.kind() == "variable_name" {
                    n
                } else {
                    n.parent().unwrap_or(n)
                }
            })
            .ok_or_else(|| cannot(NOTHING))?;
        if let Some(changed) = all_uses.iter().find(|u| {
            operands.contains(&u.name)
                && u.access.writes()
                && u.order > def.assignment.end_byte()
                && u.span.start < r.span.start
        }) {
            conflicts.push(conflict_at(
                format!(
                    "Variable ${} used in the initializer of ${} is changed before the reference on line {}",
                    changed.name,
                    def.name,
                    line_of(src, r.span.start) + 1
                ),
                path,
                src,
                changed.span.start,
            ));
        }
        let replacement = match node.parent().map(|p| p.kind()) {
            Some("encapsed_string" | "heredoc_body" | "heredoc") => {
                let braced = node.prev_sibling().is_some_and(|s| s.kind() == "{");
                if !value_text.starts_with('$') || value_text.contains('\n') {
                    return Err(cannot(format!(
                        "${} is used in a string and its initializer cannot be put there",
                        def.name
                    )));
                }
                if braced {
                    value_text.to_string()
                } else {
                    format!("{{{value_text}}}")
                }
            }
            _ => fit(
                node,
                &reindent(
                    value_text,
                    &leading_ws(src, def.value.start_byte()),
                    &leading_ws(src, r.span.start),
                ),
                value_kind,
            ),
        };
        edits.push(TextEdit {
            start: r.span.start,
            end: r.span.end,
            new_text: replacement,
        });
    }
    if has_side_effects(def.value) && refs.len() > 1 {
        conflicts.push(conflict_at(
            format!(
                "The initializer of ${} has side effects and is evaluated {} times after inlining",
                def.name,
                refs.len()
            ),
            path,
            src,
            def.value.start_byte(),
        ));
    }
    let remove = mode == InlineMode::All || (mode == InlineMode::This && def.references.len() == 1);
    if remove {
        let s = statement_removal(src, Span::of(def.statement));
        edits.push(TextEdit {
            start: s.start,
            end: s.end,
            new_text: String::new(),
        });
    }
    let _ = &def.uses;
    Ok(Plan {
        edits,
        other_files: vec![],
        conflicts,
        name_spans: vec![],
    })
}

// ---- Methods and functions ----

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BodyParam {
    pub name: String,
    pub default: Option<String>,
    pub by_ref: bool,
}

/// A method's or function's body, ready to be put at call sites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallableBody {
    pub kind: InlineKind,
    pub name: String,
    pub path: String,
    /// The declaration's name.
    pub declaration: Span,
    /// The whole declaration (with its doc comment), what "remove" deletes.
    pub removal: Span,
    pub params: Vec<BodyParam>,
    /// The statements before the final `return` (each line indented relative to the body).
    pub statements: String,
    /// The returned expression and its node kind.
    pub returns: Option<(String, String)>,
    /// Variables of the body other than the parameters (renamed on clashes).
    pub locals: Vec<String>,
    pub writes_params: Vec<String>,
    pub uses_this: bool,
    /// Uses `self::` / `static::` / `parent::`.
    pub uses_class_scope: bool,
    /// The class the method is declared in (short name).
    pub class_name: Option<String>,
}

/// Check that the method / function declared at `name_span` can be inlined and take its body.
pub fn callable_body(src: &str, path: &str, name_span: Span) -> Result<CallableBody, String> {
    let tree = parse(src).ok_or_else(|| cannot(NOTHING))?;
    let root = tree.root_node();
    let name_node = root
        .descendant_for_byte_range(name_span.start, name_span.end)
        .ok_or_else(|| cannot(NOTHING))?;
    let decl = name_node
        .parent()
        .filter(|p| matches!(p.kind(), "method_declaration" | "function_definition"))
        .ok_or_else(|| cannot(NOTHING))?;
    let kind = if decl.kind() == "method_declaration" {
        InlineKind::Method
    } else {
        InlineKind::Function
    };
    let name = text(name_node, src).to_string();
    let word = if kind == InlineKind::Method {
        "method"
    } else {
        "function"
    };
    let body = decl
        .child_by_field_name("body")
        .ok_or_else(|| cannot("Abstract methods cannot be inlined"))?;
    // Parameters.
    let mut params = Vec::new();
    if let Some(list) = decl.child_by_field_name("parameters") {
        let mut c = list.walk();
        for p in list.named_children(&mut c) {
            if p.kind() == "variadic_parameter" {
                return Err(cannot(format!("The {word} has a variadic parameter")));
            }
            if p.kind() == "property_promotion_parameter" {
                return Err(cannot("Constructors cannot be inlined"));
            }
            let Some(n) = p.child_by_field_name("name") else {
                continue;
            };
            params.push(BodyParam {
                name: text(n, src).trim_start_matches('$').to_string(),
                default: p
                    .child_by_field_name("default_value")
                    .map(|d| text(d, src).to_string()),
                by_ref: p.child_by_field_name("reference_modifier").is_some(),
            });
        }
    }
    let mut c = body.walk();
    let stmts: Vec<Node> = body
        .named_children(&mut c)
        .filter(|n| n.kind() != "comment")
        .collect();
    let returns = collect_nodes(body, false, &mut |n| n.kind() == "return_statement");
    let last_is_return = stmts.last().is_some_and(|s| s.kind() == "return_statement");
    if returns.len() > 1 || (returns.len() == 1 && !last_is_return) {
        return Err(cannot(format!("The {word} has multiple exit points")));
    }
    if !collect_nodes(body, false, &mut |n| n.kind() == "yield_expression").is_empty() {
        return Err(cannot(format!("The {word} is a generator")));
    }
    let calls_itself = !collect_nodes(body, true, &mut |n| match n.kind() {
        "function_call_expression" => n.child_by_field_name("function").is_some_and(|f| {
            text(f, src)
                .rsplit('\\')
                .next()
                .unwrap_or_default()
                .eq_ignore_ascii_case(&name)
        }),
        "member_call_expression" | "scoped_call_expression" | "nullsafe_member_call_expression" => {
            kind == InlineKind::Method
                && n.child_by_field_name("name")
                    .is_some_and(|m| text(m, src).eq_ignore_ascii_case(&name))
        }
        _ => false,
    })
    .is_empty();
    if calls_itself {
        return Err(cannot(format!("The {word} {name}() is recursive")));
    }
    if text(body, src).contains("func_get_args") {
        return Err(cannot(format!("The {word} uses func_get_args()")));
    }
    let returns_expr = if last_is_return {
        stmts
            .last()
            .and_then(|r| r.named_child(0))
            .filter(|e| e.kind() != "comment")
    } else {
        None
    };
    let before: Vec<Node> = if last_is_return {
        stmts[..stmts.len() - 1].to_vec()
    } else {
        stmts.clone()
    };
    let statements = match (before.first(), before.last()) {
        (Some(f), Some(l)) => reindent(
            &src[f.start_byte()..l.end_byte()],
            &leading_ws(src, f.start_byte()),
            "",
        ),
        _ => String::new(),
    };
    let returns = returns_expr.map(|e| {
        (
            reindent(text(e, src), &leading_ws(src, e.start_byte()), ""),
            e.kind().to_string(),
        )
    });
    let param_names: HashSet<&str> = params.iter().map(|p| p.name.as_str()).collect();
    let uses = vars::collect(body, src);
    let mut locals: Vec<String> = Vec::new();
    let mut writes_params: Vec<String> = Vec::new();
    for u in &uses {
        if param_names.contains(u.name.as_str()) {
            if u.access.writes() && !writes_params.contains(&u.name) {
                writes_params.push(u.name.clone());
            }
        } else if !locals.contains(&u.name) {
            locals.push(u.name.clone());
        }
    }
    let uses_class_scope =
        !collect_nodes(body, true, &mut |n| n.kind() == "relative_scope").is_empty();
    // The doc comment goes with the declaration.
    let mut start = decl.start_byte();
    if let Some(prev) = decl
        .prev_sibling()
        .filter(|p| p.kind() == "comment" && text(*p, src).starts_with("/**"))
    {
        if src[prev.end_byte()..start].trim().is_empty() {
            start = prev.start_byte();
        }
    }
    Ok(CallableBody {
        kind,
        name,
        path: path.to_string(),
        declaration: name_span,
        removal: member_removal(src, Span::new(start, decl.end_byte())),
        params,
        statements,
        returns,
        locals,
        writes_params,
        uses_this: vars::uses_this(body, src),
        uses_class_scope,
        class_name: enclosing_class(decl)
            .and_then(|c| c.child_by_field_name("name"))
            .map(|n| text(n, src).to_string()),
    })
}

/// A member's lines and one blank line next to them.
fn member_removal(src: &str, s: Span) -> Span {
    let mut r = statement_removal(src, s);
    let rest = &src[r.end..];
    let blank_after = rest.find('\n').is_some_and(|i| rest[..i].trim().is_empty());
    if blank_after {
        r.end += rest.find('\n').unwrap() + 1;
    } else {
        // Before the class' closing brace: drop the blank line above instead.
        let before = &src[..r.start];
        if let Some(stripped) = before.strip_suffix('\n') {
            let ls = line_start(stripped, stripped.len());
            if stripped[ls..].trim().is_empty() {
                r.start = ls;
            }
        }
    }
    r
}

/// Is the expression simple enough to repeat (a variable, literal or constant)?
fn is_simple(kind: &str) -> bool {
    matches!(
        kind,
        "variable_name"
            | "string"
            | "integer"
            | "float"
            | "boolean"
            | "null"
            | "name"
            | "class_constant_access_expression"
    )
}

/// Inline the call whose name is at `span` in `src` (another or the declaring file).
pub fn inline_call(
    body: &CallableBody,
    path: &str,
    src: &str,
    span: Span,
) -> Result<Vec<TextEdit>, RenameConflict> {
    let problem = |what: String| conflict_at(what, path, src, span.start);
    let tree = parse(src).ok_or_else(|| problem("The file cannot be parsed".into()))?;
    let root = tree.root_node();
    let call = call_at(root, span).ok_or_else(|| {
        problem(format!(
            "The reference to {}() is not a call and is not inlined",
            body.name
        ))
    })?;
    let args =
        call_args(call, src).ok_or_else(|| problem("The call has no arguments list".into()))?;
    if args.spread || args.callable {
        return Err(problem(format!(
            "The call to {}() passes its arguments dynamically and is not inlined",
            body.name
        )));
    }
    let site_class = enclosing_class(call)
        .and_then(|c| c.child_by_field_name("name"))
        .map(|n| text(n, src).to_string());
    if body.uses_class_scope && (site_class != body.class_name || path != body.path) {
        return Err(problem(format!(
            "{}() uses self::, static:: or parent:: and is called from another class",
            body.name
        )));
    }
    let recv = receiver(call, src);
    if body.uses_this && recv.is_none() {
        return Err(problem(format!(
            "{}() uses $this and is called statically",
            body.name
        )));
    }
    let recv = recv.unwrap_or_else(|| "$this".into());
    let recv =
        if recv.starts_with("parent") || recv.starts_with("self") || recv.starts_with("static") {
            "$this".to_string()
        } else {
            recv
        };
    // Argument per parameter.
    let mut values: HashMap<String, (String, String)> = HashMap::new();
    for (i, p) in body.params.iter().enumerate() {
        let value = args
            .named
            .get(&p.name)
            .or_else(|| args.positional.get(i))
            .map(|a| (text(*a, src).to_string(), a.kind().to_string()))
            .or_else(|| {
                p.default
                    .clone()
                    .map(|d| (d, "parenthesized_expression".to_string()))
            });
        match value {
            Some(v) => {
                values.insert(p.name.clone(), v);
            }
            None => {
                return Err(problem(format!(
                    "The call to {}() misses the argument ${}",
                    body.name, p.name
                )))
            }
        }
    }
    let single = body.statements.trim().is_empty();
    if single {
        // `return <expr>;` only: the call becomes the expression.
        let Some((expr, kind)) = &body.returns else {
            // An empty body: the call disappears (as a statement) or becomes `null`.
            return Ok(vec![replace_call_statement(call, src, None)]);
        };
        let result = substitute(expr, &|v| {
            if v == "this" {
                return Some((recv.clone(), "variable_name".into()));
            }
            values.get(v).cloned()
        })
        .ok_or_else(|| problem("The body cannot be parsed".into()))?;
        if call
            .parent()
            .is_some_and(|p| p.kind() == "expression_statement")
        {
            return Ok(vec![TextEdit {
                start: call.start_byte(),
                end: call.end_byte(),
                new_text: result,
            }]);
        }
        return Ok(vec![TextEdit {
            start: call.start_byte(),
            end: call.end_byte(),
            new_text: fit(call, &result, kind),
        }]);
    }
    // Statements: they go before the statement with the call.
    let stmt = statement_of(call).ok_or_else(|| {
        problem(format!(
            "The call to {}() is not inside a statement and is not inlined",
            body.name
        ))
    })?;
    let site_scope = scope_body(enclosing_function(call), root);
    let mut taken: HashSet<String> = vars::names_in(site_scope, src);
    let mut renames: HashMap<String, (String, String)> = HashMap::new();
    let mut prologue = Vec::new();
    for p in &body.params {
        let (value, kind) = values[&p.name].clone();
        let reuse = !body.writes_params.contains(&p.name) && !p.by_ref && is_simple(&kind)
            || p.by_ref && kind == "variable_name";
        if reuse {
            renames.insert(p.name.clone(), (value, kind));
        } else {
            let local = unique(&p.name, &taken);
            taken.insert(local.clone());
            prologue.push(format!("${local} = {value};"));
            renames.insert(
                p.name.clone(),
                (format!("${local}"), "variable_name".into()),
            );
        }
    }
    for l in &body.locals {
        if taken.contains(l) {
            let local = unique(l, &taken);
            taken.insert(local.clone());
            renames.insert(l.clone(), (format!("${local}"), "variable_name".into()));
        }
    }
    renames.insert("this".into(), (recv.clone(), "variable_name".into()));
    let rename = |code: &str| {
        substitute_statements(code, &|v| renames.get(v).cloned())
            .unwrap_or_else(|| code.to_string())
    };
    let indent = leading_ws(src, stmt.start_byte());
    let nl = eol(src);
    let mut lines: Vec<String> = prologue;
    lines.push(rename(&body.statements));
    let ret = body.returns.as_ref().map(|(e, k)| (rename(e), k.clone()));
    let whole = call.parent() == Some(stmt) && stmt.kind() == "expression_statement";
    let mut edits = Vec::new();
    if whole {
        // `$this->foo();`: the statements, and the returned value if it does something.
        if let Some((r, _)) = &ret {
            let r_tree = parse(&format!("<?php {r};"));
            let effects = r_tree
                .as_ref()
                .is_some_and(|t| has_side_effects(t.root_node()));
            if effects {
                lines.push(format!("{r};"));
            }
        }
        let code = indent_lines(&lines.join("\n"), &indent, nl);
        edits.push(TextEdit {
            start: stmt.start_byte(),
            end: stmt.end_byte(),
            new_text: code,
        });
        return Ok(edits);
    }
    let (value, kind) = ret.unwrap_or_else(|| ("null".into(), "null".into()));
    let simple_place = matches!(call.parent().map(|p| p.kind()), Some("return_statement"))
        || call.parent().is_some_and(|p| {
            p.kind() == "assignment_expression"
                && p.child_by_field_name("right") == Some(call)
                && p.parent() == Some(stmt)
        });
    if simple_place {
        lines.push(
            format!("{}{}", &src[stmt.start_byte()..call.start_byte()], value)
                + &src[call.end_byte()..stmt.end_byte()],
        );
        edits.push(TextEdit {
            start: stmt.start_byte(),
            end: stmt.end_byte(),
            new_text: indent_lines(&lines.join("\n"), &indent, nl),
        });
    } else {
        // Inside an expression: the result goes through a variable.
        let var = unique("result", &taken);
        lines.push(format!("${var} = {value};"));
        edits.push(TextEdit {
            start: stmt.start_byte(),
            end: stmt.start_byte(),
            new_text: format!(
                "{}{nl}{indent}",
                indent_lines(&lines.join("\n"), &indent, nl)
            ),
        });
        let _ = kind;
        edits.push(TextEdit {
            start: call.start_byte(),
            end: call.end_byte(),
            new_text: format!("${var}"),
        });
    }
    Ok(edits)
}

/// The call statement removed (an empty body), or the call replaced by `value`.
fn replace_call_statement(call: Node, src: &str, value: Option<&str>) -> TextEdit {
    match call.parent() {
        Some(stmt) if stmt.kind() == "expression_statement" && value.is_none() => {
            let s = statement_removal(src, Span::of(stmt));
            TextEdit {
                start: s.start,
                end: s.end,
                new_text: String::new(),
            }
        }
        _ => TextEdit {
            start: call.start_byte(),
            end: call.end_byte(),
            new_text: value.unwrap_or("null").to_string(),
        },
    }
}

/// Lines indented for the call site (the first one goes where the statement starts).
fn indent_lines(code: &str, indent: &str, nl: &str) -> String {
    code.split('\n')
        .enumerate()
        .map(|(i, l)| {
            if i == 0 || l.trim().is_empty() {
                l.trim_end().to_string()
            } else {
                format!("{indent}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join(nl)
}

/// Rename variables in statements (`substitute` for code that is not one expression).
fn substitute_statements(
    code: &str,
    values: &dyn Fn(&str) -> Option<(String, String)>,
) -> Option<String> {
    let src = format!("<?php {code}");
    let tree = parse(&src)?;
    let root = tree.root_node();
    let found = collect_nodes(root, false, &mut |n| n.kind() == "variable_name");
    let mut out = code.to_string();
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for v in found {
        let name = text(v, &src).trim_start_matches('$');
        if let Some((value, kind)) = values(name) {
            let s = v.start_byte() - 6;
            edits.push((s, s + v.end_byte() - v.start_byte(), fit(v, &value, &kind)));
        }
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    for (s, e, t) in edits {
        out.replace_range(s..e, &t);
    }
    Some(out)
}

/// Inline every call in `usages` (or only `only`), removing the declaration when asked and
/// every call could be inlined.
pub fn inline_callable(
    body: &CallableBody,
    usages: &[UsageSpan],
    only: Option<&UsageSpan>,
    remove: bool,
    text_of: &mut dyn FnMut(&str) -> Option<String>,
) -> (Vec<FileEdits>, Vec<RenameConflict>) {
    let mut files: Vec<FileEdits> = Vec::new();
    let mut conflicts = Vec::new();
    let sites: Vec<&UsageSpan> = match only {
        Some(u) => vec![u],
        None => usages
            .iter()
            .filter(|u| !(u.path == body.path && u.span == body.declaration))
            .collect(),
    };
    let mut failed = false;
    for u in sites {
        let Some(src) = text_of(&u.path) else {
            continue;
        };
        match inline_call(body, &u.path, &src, u.span) {
            Ok(edits) => push_edits(&mut files, &u.path, edits),
            Err(c) => {
                failed = true;
                conflicts.push(c);
            }
        }
    }
    if remove && only.is_none() {
        if failed {
            conflicts.push(RenameConflict::new(format!(
                "{}() is kept because not every call could be inlined",
                body.name
            )));
        } else {
            push_edits(
                &mut files,
                &body.path,
                vec![TextEdit {
                    start: body.removal.start,
                    end: body.removal.end,
                    new_text: String::new(),
                }],
            );
        }
    }
    (files, conflicts)
}

fn push_edits(files: &mut Vec<FileEdits>, path: &str, edits: Vec<TextEdit>) {
    match files.iter_mut().find(|f| f.path == path) {
        Some(f) => f.edits.extend(edits),
        None => files.push(FileEdits {
            path: path.to_string(),
            edits,
        }),
    }
}

// ---- Constants ----

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstantValue {
    pub name: String,
    pub path: String,
    pub declaration: Span,
    pub value: String,
    pub kind: String,
    /// What "remove" deletes: the declaration, or only this constant of a list.
    pub removal: Span,
    pub class_name: Option<String>,
    pub uses_class_scope: bool,
}

/// The value of the constant declared at `name_span` (`const X = ...` in a class or file).
pub fn constant_value(src: &str, path: &str, name_span: Span) -> Result<ConstantValue, String> {
    let tree = parse(src).ok_or_else(|| cannot(NOTHING))?;
    let root = tree.root_node();
    let name = root
        .descendant_for_byte_range(name_span.start, name_span.end)
        .ok_or_else(|| cannot(NOTHING))?;
    let element = name
        .parent()
        .filter(|p| p.kind() == "const_element")
        .ok_or_else(|| cannot(NOTHING))?;
    let decl = element.parent().ok_or_else(|| cannot(NOTHING))?;
    let value = element
        .named_child(element.named_child_count().saturating_sub(1))
        .filter(|v| *v != name)
        .ok_or_else(|| cannot("The constant has no value"))?;
    let mut c = decl.walk();
    let elements: Vec<Node> = decl
        .named_children(&mut c)
        .filter(|n| n.kind() == "const_element")
        .collect();
    let removal = if elements.len() == 1 {
        let s = statement_removal(src, Span::of(decl));
        if enclosing_class(decl).is_some() {
            member_removal(src, Span::of(decl))
        } else {
            s
        }
    } else {
        let i = elements.iter().position(|e| *e == element).unwrap_or(0);
        if i + 1 < elements.len() {
            Span::new(element.start_byte(), elements[i + 1].start_byte())
        } else {
            Span::new(elements[i - 1].end_byte(), element.end_byte())
        }
    };
    Ok(ConstantValue {
        name: text(name, src).to_string(),
        path: path.to_string(),
        declaration: name_span,
        value: text(value, src).to_string(),
        kind: value.kind().to_string(),
        removal,
        class_name: enclosing_class(decl)
            .and_then(|c| c.child_by_field_name("name"))
            .map(|n| text(n, src).to_string()),
        uses_class_scope: !collect_nodes(value, true, &mut |n| n.kind() == "relative_scope")
            .is_empty(),
    })
}

/// Replace the constant reference whose name is at `span`.
pub fn inline_constant_at(
    c: &ConstantValue,
    path: &str,
    src: &str,
    span: Span,
) -> Result<TextEdit, RenameConflict> {
    let problem = |what: String| conflict_at(what, path, src, span.start);
    let tree = parse(src).ok_or_else(|| problem("The file cannot be parsed".into()))?;
    let root = tree.root_node();
    let mut n = root
        .descendant_for_byte_range(span.start, span.end)
        .ok_or_else(|| problem("The reference cannot be found".into()))?;
    if let Some(p) = n.parent() {
        if matches!(
            p.kind(),
            "class_constant_access_expression" | "qualified_name"
        ) {
            n = p;
        }
    }
    let site_class = enclosing_class(n)
        .and_then(|x| x.child_by_field_name("name"))
        .map(|x| text(x, src).to_string());
    let mut value = c.value.clone();
    if c.uses_class_scope && site_class != c.class_name {
        match &c.class_name {
            Some(class) if path == c.path => {
                value = value
                    .replace("self::", &format!("{class}::"))
                    .replace("static::", &format!("{class}::"));
            }
            _ => {
                return Err(problem(format!(
                    "The value of {} uses self:: and cannot be inlined into another class",
                    c.name
                )))
            }
        }
    }
    Ok(TextEdit {
        start: n.start_byte(),
        end: n.end_byte(),
        new_text: fit(n, &value, &c.kind),
    })
}

/// Inline every reference in `usages` (or only `only`).
pub fn inline_constant(
    c: &ConstantValue,
    usages: &[UsageSpan],
    only: Option<&UsageSpan>,
    remove: bool,
    text_of: &mut dyn FnMut(&str) -> Option<String>,
) -> (Vec<FileEdits>, Vec<RenameConflict>) {
    let mut files: Vec<FileEdits> = Vec::new();
    let mut conflicts = Vec::new();
    let sites: Vec<&UsageSpan> = match only {
        Some(u) => vec![u],
        None => usages
            .iter()
            .filter(|u| !(u.path == c.path && u.span == c.declaration))
            .collect(),
    };
    let mut failed = false;
    for u in sites {
        let Some(src) = text_of(&u.path) else {
            continue;
        };
        match inline_constant_at(c, &u.path, &src, u.span) {
            Ok(e) => push_edits(&mut files, &u.path, vec![e]),
            Err(x) => {
                failed = true;
                conflicts.push(x);
            }
        }
    }
    if remove && only.is_none() {
        if failed {
            conflicts.push(RenameConflict::new(format!(
                "{} is kept because not every reference could be inlined",
                c.name
            )));
        } else {
            push_edits(
                &mut files,
                &c.path,
                vec![TextEdit {
                    start: c.removal.start,
                    end: c.removal.end,
                    new_text: String::new(),
                }],
            );
        }
    }
    (files, conflicts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refactor::test_util::{apply, sel};

    fn inline_var(marked: &str, mode: InlineMode) -> (String, Plan) {
        let (src, s, _) = sel(marked);
        let plan = inline_variable(&src, "a.php", s, mode).unwrap();
        (apply(&src, &plan.edits), plan)
    }

    #[test]
    fn finds_what_is_at_the_caret() {
        let (src, s, _) = sel("<?php\nfunction f() { $a = 1; return $‸a + g(); }");
        let t = target_at(&src, s).unwrap();
        assert_eq!(
            (t.kind, t.name.as_str(), t.on_reference, t.occurrences),
            (InlineKind::Variable, "$a", true, 1)
        );
        let (src, s, _) =
            sel("<?php\nclass A { const ‸X = 1; function f() { return $this->g‸o(); } }");
        assert_eq!(target_at(&src, s).unwrap().kind, InlineKind::Constant);
        let (src, s, _) = sel("<?php\nclass A { function f() { return $this->g‸o() + self::X; } }");
        let t = target_at(&src, s).unwrap();
        assert_eq!(
            (t.kind, t.name.as_str(), t.on_reference),
            (InlineKind::Method, "go()", true)
        );
        let (src, s, _) = sel("<?php\nclass A { function f() { return self::‸X; } }");
        assert_eq!(target_at(&src, s).unwrap().kind, InlineKind::Constant);
    }

    #[test]
    fn inlines_a_variable_everywhere_and_removes_it() {
        let src = "<?php
function price($net, $rate)
{
    $tax = $net * $rate;
    echo $‸tax;
    return $net + $tax;
}
";
        let (out, plan) = inline_var(src, InlineMode::All);
        assert_eq!(out, "<?php\nfunction price($net, $rate)\n{\n    echo $net * $rate;\n    return $net + $net * $rate;\n}\n");
        assert!(plan.conflicts.is_empty());
        let (out, _) = inline_var(&src.replace("$net + $tax", "$net / $tax"), InlineMode::All);
        assert!(out.contains("return $net / ($net * $rate);"), "{out}");
    }

    #[test]
    fn inlines_this_reference_only_and_keeps_the_variable() {
        let src = "<?php\nfunction f($u)\n{\n    $name = $u->name;\n    echo $‸name;\n    echo \"Hi $name\";\n}\n";
        let (out, _) = inline_var(src, InlineMode::This);
        assert_eq!(out, "<?php\nfunction f($u)\n{\n    $name = $u->name;\n    echo $u->name;\n    echo \"Hi $name\";\n}\n");
        let (out, _) = inline_var(src, InlineMode::All);
        assert!(out.contains("echo \"Hi {$u->name}\";"), "{out}");
    }

    #[test]
    fn reports_side_effects_and_changed_operands() {
        let src = "<?php\nfunction f($a)\n{\n    $x = load($a);\n    $a++;\n    echo $‸x;\n    echo $x;\n}\n";
        let (_, plan) = inline_var(src, InlineMode::All);
        let messages: Vec<_> = plan.conflicts.iter().map(|c| c.message.as_str()).collect();
        assert_eq!(
            messages,
            vec![
                "Variable $a used in the initializer of $x is changed before the reference on line 6",
                "Variable $a used in the initializer of $x is changed before the reference on line 7",
                "The initializer of $x has side effects and is evaluated 2 times after inlining",
            ]
        );
    }

    #[test]
    fn rejects_reassigned_variables_and_parameters() {
        let (src, s, _) =
            sel("<?php\nfunction f()\n{\n    $x = 1;\n    $x = 2;\n    echo $‸x;\n}\n");
        assert_eq!(
            inline_variable(&src, "a.php", s, InlineMode::All).unwrap_err(),
            "Cannot perform refactoring. Variable $x is accessed for writing"
        );
        let (src, s, _) = sel("<?php\nfunction f($p)\n{\n    echo $‸p;\n}\n");
        assert!(target_at(&src, s).is_err());
    }

    const SHOP: &str = "<?php
class Shop
{
    public function total($items)
    {
        $sum = $this->net($items, 2);
        return $sum;
    }

    /** The net sum. */
    private function net($items, $digits = 0)
    {
        return round(array_sum($items) * $this->rate, $digits);
    }
}
";

    fn body_of(src: &str, name: &str) -> CallableBody {
        let at = src.find(&format!("function {name}(")).unwrap() + 9;
        callable_body(src, "Shop.php", Span::new(at, at + name.len())).unwrap()
    }

    fn usage(src: &str, needle: &str, name: &str) -> UsageSpan {
        let at = src.find(needle).unwrap() + needle.find(name).unwrap();
        UsageSpan {
            path: "Shop.php".into(),
            span: Span::new(at, at + name.len()),
        }
    }

    #[test]
    fn inlines_a_single_return_method_and_removes_it() {
        let body = body_of(SHOP, "net");
        assert_eq!(
            body.returns.as_ref().unwrap().0,
            "round(array_sum($items) * $this->rate, $digits)"
        );
        let usages = vec![
            usage(SHOP, "->net(", "net"),
            UsageSpan {
                path: "Shop.php".into(),
                span: body.declaration,
            },
        ];
        let (files, conflicts) =
            inline_callable(&body, &usages, None, true, &mut |_| Some(SHOP.to_string()));
        assert!(conflicts.is_empty(), "{conflicts:?}");
        let out = apply(SHOP, &files[0].edits);
        assert_eq!(
            out,
            "<?php
class Shop
{
    public function total($items)
    {
        $sum = round(array_sum($items) * $this->rate, 2);
        return $sum;
    }
}
"
        );
    }

    #[test]
    fn inlines_statement_bodies_with_parameters_and_locals() {
        let src = "<?php
class A
{
    public function run($list)
    {
        $total = 5;
        echo $this->sum($list, 'x') + 1;
    }

    public function sum(array $values, $label)
    {
        $total = 0;
        foreach ($values as $v) {
            $total += $v;
        }
        log($label);
        return $total;
    }
}
";
        let at = src.find("function sum(").unwrap() + 9;
        let body = callable_body(src, "A.php", Span::new(at, at + 3)).unwrap();
        let call = src.find("->sum(").unwrap() + 2;
        let edits = inline_call(&body, "A.php", src, Span::new(call, call + 3)).unwrap();
        let out = apply(src, &edits);
        assert!(out.contains("        $total = 5;\n        $total1 = 0;\n        foreach ($list as $v) {\n            $total1 += $v;\n        }\n        log('x');\n        $result = $total1;\n        echo $result + 1;\n"), "{out}");
    }

    #[test]
    fn rejects_recursion_and_multiple_returns() {
        let src = "<?php\nfunction f($n) { if ($n) { return 1; } return f($n - 1); }\n";
        let at = src.find("f(").unwrap();
        let err = callable_body(src, "a.php", Span::new(at, at + 1)).unwrap_err();
        assert_eq!(
            err,
            "Cannot perform refactoring. The function has multiple exit points"
        );
        let src = "<?php\nfunction f($n) { return f($n - 1); }\n";
        let at = src.find("f(").unwrap();
        assert_eq!(
            callable_body(src, "a.php", Span::new(at, at + 1)).unwrap_err(),
            "Cannot perform refactoring. The function f() is recursive"
        );
    }

    #[test]
    fn inlines_constants_and_removes_one_of_a_list() {
        let src = "<?php\nclass A\n{\n    const RATE = 0.2, MAX = self::RATE * 10;\n\n    function f() { return self::MAX + B::MAX; }\n}\n";
        let at = src.find("MAX =").unwrap();
        let c = constant_value(src, "A.php", Span::new(at, at + 3)).unwrap();
        assert_eq!(c.value, "self::RATE * 10");
        let u1 = src.find("self::MAX").unwrap() + 6;
        let u2 = src.find("B::MAX").unwrap() + 3;
        let usages = vec![
            UsageSpan {
                path: "A.php".into(),
                span: Span::new(u1, u1 + 3),
            },
            UsageSpan {
                path: "A.php".into(),
                span: Span::new(u2, u2 + 3),
            },
        ];
        let (files, conflicts) =
            inline_constant(&c, &usages, None, true, &mut |_| Some(src.to_string()));
        assert!(conflicts.is_empty(), "{conflicts:?}");
        let out = apply(src, &files[0].edits);
        assert_eq!(out, "<?php\nclass A\n{\n    const RATE = 0.2;\n\n    function f() { return self::RATE * 10 + self::RATE * 10; }\n}\n");
    }

    #[test]
    fn inlines_a_call_statement_and_function_calls() {
        let src = "<?php
function greet($name)
{
    $text = \"Hi \" . $name;
    echo $text;
}

greet('Ann');
";
        let at = src.find("greet(").unwrap() + 9;
        let at = src[..at].rfind("greet").unwrap();
        let body = callable_body(src, "a.php", Span::new(at, at + 5)).unwrap();
        assert!(body.returns.is_none());
        let call = src.rfind("greet(").unwrap();
        let edits = inline_call(&body, "a.php", src, Span::new(call, call + 5)).unwrap();
        let out = apply(src, &edits);
        assert!(
            out.ends_with("\n$text = \"Hi \" . 'Ann';\necho $text;\n"),
            "{out}"
        );
    }

    #[test]
    fn static_calls_cannot_inline_this() {
        let src = "<?php
class A
{
    public function name()
    {
        return $this->n;
    }
}

echo A::name();
";
        let at = src.find("name()").unwrap();
        let body = callable_body(src, "a.php", Span::new(at, at + 4)).unwrap();
        let call = src.rfind("name()").unwrap();
        let err = inline_call(&body, "a.php", src, Span::new(call, call + 4)).unwrap_err();
        assert_eq!(err.message, "name() uses $this and is called statically");
    }
}
