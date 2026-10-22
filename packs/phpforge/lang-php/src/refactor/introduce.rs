//! Introduce Variable (Ctrl+Alt+V), Constant (Ctrl+Alt+C), Field (Ctrl+Alt+F) and Parameter
//! (Ctrl+Alt+P) like PHPStorm: the expression is the selection, or the one chosen in the
//! "Expressions" popup when the caret is in several; it is replaced (optionally with all its
//! occurrences) by the new variable, `self::CONSTANT`, `$this->field` or `$parameter`.
//! Introduce Parameter also passes the expression at every call site
//! ([`parameter_call_edits`], with the call sites from Find Usages).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use super::names::{suggest, unique};
use super::vars::{self, parameter_names};
use super::{
    cannot, check_name, constant_case, enclosing_class, enclosing_function,
    enclosing_named_function, eol, expression_at, expressions_at_caret, finish, fit, indent_unit,
    is_statement, is_statement_list, is_static_method, leading_ws, marked, one_line, parse,
    reindent, same_code, scope_body, text, FileEdits, Plan, Span, UsageSpan, NOT_AN_EXPRESSION,
};
use crate::generate::{insert_after_fields, insert_at_end, Fmt, TextEdit};
use crate::model::{ClassLike, SymbolKind, Visibility};
use crate::parser::index_source;
use crate::style::PhpCodeStyle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IntroduceKind {
    Variable,
    Constant,
    Field,
    Parameter,
}

impl IntroduceKind {
    /// PHPStorm's refactoring (and dialog) title.
    pub fn title(self) -> &'static str {
        match self {
            Self::Variable => "Introduce Variable",
            Self::Constant => "Introduce Constant",
            Self::Field => "Introduce Field",
            Self::Parameter => "Introduce Parameter",
        }
    }
}

/// Where Introduce Field initializes the field.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldInit {
    #[default]
    CurrentMethod,
    FieldDeclaration,
    Constructor,
}

/// An entry of the "Expressions" popup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpressionChoice {
    pub span: Span,
    /// One line (`$this->getUser()->name`).
    pub text: String,
}

/// What the dialog / in-place editing needs to know about the expression.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntroduceInfo {
    pub span: Span,
    pub text: String,
    /// Occurrences of the expression where it would be replaced (itself included).
    pub occurrences: usize,
    /// Name suggestions, best first (constants in UPPER_CASE, others without `$`).
    pub names: Vec<String>,
    /// The class that gets the constant / field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
    /// Introduce Parameter: `foo()` / `User::foo()`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_name: Option<String>,
    /// Introduce Field: the places the field can be initialized in.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub init_places: Vec<FieldInit>,
    /// Introduce Field in a static method: a static field.
    pub is_static: bool,
    /// The expression is a constant expression (a field's or parameter's default value).
    pub constant_expression: bool,
    /// Introduce Constant outside a class: a global `const`.
    pub global: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    /// Several expressions at the caret: the "Expressions" popup (innermost first).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expressions: Vec<ExpressionChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<IntroduceInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct IntroduceOptions {
    /// The new name (constants as typed, others without `$`).
    pub name: String,
    pub replace_all: bool,
    pub visibility: Option<Visibility>,
    pub init: FieldInit,
    /// Introduce Parameter: the expression becomes the default value (call sites unchanged).
    pub default_value: bool,
    pub style: PhpCodeStyle,
}

/// Whether `n` can initialize a constant: literals, arrays of them, operators on them and
/// other constants.
pub(crate) fn is_constant_expression(n: Node, src: &str) -> bool {
    match n.kind() {
        "string" | "integer" | "float" | "boolean" | "null" | "nowdoc" => true,
        "encapsed_string" | "heredoc" => {
            let mut c = n.walk();
            let plain = n.named_children(&mut c).all(|ch| {
                matches!(
                    ch.kind(),
                    "string_content"
                        | "escape_sequence"
                        | "string_value"
                        | "heredoc_start"
                        | "heredoc_end"
                        | "heredoc_body"
                ) && (ch.kind() != "heredoc_body" || is_constant_expression(ch, src))
            });
            plain
        }
        "name" | "qualified_name" => true,
        "class_constant_access_expression" => true,
        "array_creation_expression"
        | "array_element_initializer"
        | "pair"
        | "parenthesized_expression"
        | "unary_op_expression"
        | "binary_expression"
        | "conditional_expression" => {
            if n.kind() == "binary_expression"
                && n.child_by_field_name("operator")
                    .is_some_and(|o| text(o, src) == "instanceof")
            {
                return false;
            }
            let mut c = n.walk();
            let all = n
                .named_children(&mut c)
                .filter(|ch| ch.kind() != "comment")
                .all(|ch| is_constant_expression(ch, src));
            all
        }
        _ => false,
    }
}

/// `n` is written to (`<n> = ...`, `<n>++`), so it is not an occurrence to replace.
fn is_write_target(n: Node) -> bool {
    let Some(p) = n.parent() else { return false };
    match p.kind() {
        "assignment_expression"
        | "augmented_assignment_expression"
        | "reference_assignment_expression" => p.child_by_field_name("left") == Some(n),
        "update_expression" | "list_literal" | "unset_statement" => true,
        _ => false,
    }
}

/// Context of the selected expression.
struct Site<'t> {
    expr: Node<'t>,
    root: Node<'t>,
    function: Option<Node<'t>>,
    /// The named function / method (Introduce Parameter / Field).
    named: Option<Node<'t>>,
    class: Option<Node<'t>>,
}

impl<'t> Site<'t> {
    fn new(expr: Node<'t>, root: Node<'t>) -> Self {
        Site {
            expr,
            root,
            function: enclosing_function(expr),
            named: enclosing_named_function(expr),
            class: enclosing_class(expr),
        }
    }

    /// Where occurrences are searched.
    fn scope(&self, kind: IntroduceKind) -> Node<'t> {
        match kind {
            IntroduceKind::Variable => scope_body(self.function, self.root),
            IntroduceKind::Parameter | IntroduceKind::Field => scope_body(self.named, self.root),
            IntroduceKind::Constant => self.class.unwrap_or(self.root),
        }
    }

    fn occurrences(&self, kind: IntroduceKind, src: &str) -> Vec<Node<'t>> {
        let scope = self.scope(kind);
        let into = kind == IntroduceKind::Constant;
        let expr = self.expr;
        let mut found = super::collect_nodes(scope, into, &mut |n| {
            n == expr || (same_code(n, expr, src) && !is_write_target(n))
        });
        // Nested occurrences (`f(f(x))` with `f(x)`... ) keep the outer one.
        found.dedup_by(|b, a| a.byte_range().contains(&b.start_byte()));
        if !found.contains(&expr) {
            found.push(expr);
            found.sort_by_key(|n| n.start_byte());
        }
        found
    }
}

/// The expressions at the caret or the selected one, checked for `kind`.
pub fn prepare(
    src: &str,
    start: usize,
    end: usize,
    kind: IntroduceKind,
) -> Result<Prepared, String> {
    let tree = parse(src).ok_or_else(|| cannot(NOT_AN_EXPRESSION))?;
    let root = tree.root_node();
    let expr = if start == end {
        let mut candidates = expressions_at_caret(root, src, start);
        if kind == IntroduceKind::Constant {
            candidates.retain(|c| is_constant_expression(*c, src));
        }
        match candidates.len() {
            0 => return Err(cannot(NOT_AN_EXPRESSION)),
            1 => candidates[0],
            _ => {
                return Ok(Prepared {
                    expressions: candidates
                        .iter()
                        .map(|n| ExpressionChoice {
                            span: Span::of(*n),
                            text: one_line(text(*n, src)),
                        })
                        .collect(),
                    info: None,
                })
            }
        }
    } else {
        expression_at(root, src, start, end).ok_or_else(|| cannot(NOT_AN_EXPRESSION))?
    };
    let site = Site::new(expr, root);
    check(&site, kind, src)?;
    let occurrences = site.occurrences(kind, src);
    let constant_expression = is_constant_expression(expr, src);
    let mut info = IntroduceInfo {
        span: Span::of(expr),
        text: one_line(text(expr, src)),
        occurrences: occurrences.len(),
        constant_expression,
        ..Default::default()
    };
    let suggestions = suggest(expr, src);
    match kind {
        IntroduceKind::Variable | IntroduceKind::Parameter => {
            let taken = vars::names_in(site.scope(kind), src);
            info.names = suggestions.iter().map(|n| unique(n, &taken)).collect();
            if kind == IntroduceKind::Parameter {
                info.function_name = site.named.map(|f| function_label(f, site.class, src));
            }
        }
        IntroduceKind::Field => {
            let class = site.class.expect("checked");
            let taken = class_members(class, src, "property_element");
            info.names = suggestions.iter().map(|n| unique(n, &taken)).collect();
            info.class_name = class_name(class, src);
            let method = site.named.expect("checked");
            info.is_static = is_static_method(method);
            info.init_places = vec![FieldInit::CurrentMethod];
            if constant_expression {
                info.init_places.push(FieldInit::FieldDeclaration);
            }
            if !info.is_static && !uses_locals(expr, src) {
                info.init_places.push(FieldInit::Constructor);
            }
        }
        IntroduceKind::Constant => {
            let taken = match site.class {
                Some(c) => class_members(c, src, "const_element"),
                None => HashSet::new(),
            };
            info.names = suggestions
                .iter()
                .map(|n| unique(&constant_case(n), &taken))
                .collect();
            info.class_name = site.class.and_then(|c| class_name(c, src));
            info.global = site.class.is_none();
        }
    }
    info.names.dedup();
    Ok(Prepared {
        expressions: vec![],
        info: Some(info),
    })
}

fn class_name(class: Node, src: &str) -> Option<String> {
    class
        .child_by_field_name("name")
        .map(|n| text(n, src).to_string())
}

/// Names declared in the class by `kind` nodes (`property_element`, `const_element`).
fn class_members(class: Node, src: &str, kind: &str) -> HashSet<String> {
    super::collect_nodes(class, false, &mut |n| n.kind() == kind)
        .into_iter()
        .filter_map(|n| {
            n.child_by_field_name("name")
                .or_else(|| n.named_child(0))
                .map(|x| text(x, src).trim_start_matches('$').to_string())
        })
        .collect()
}

/// `foo()` or `User::foo()`.
fn function_label(f: Node, class: Option<Node>, src: &str) -> String {
    let name = f
        .child_by_field_name("name")
        .map(|n| text(n, src))
        .unwrap_or_default();
    match (f.kind(), class.and_then(|c| class_name(c, src))) {
        ("method_declaration", Some(c)) => format!("{c}::{name}()"),
        _ => format!("{name}()"),
    }
}

/// The expression reads local variables (not usable in a constructor or at call sites).
fn uses_locals(expr: Node, src: &str) -> bool {
    !vars::collect(expr, src).is_empty()
}

fn check(site: &Site, kind: IntroduceKind, src: &str) -> Result<(), String> {
    let expr = site.expr;
    match kind {
        IntroduceKind::Variable => {
            if super::statement_of(expr).is_none() && anchor_candidates(expr).is_empty() {
                return Err(cannot("The expression is not inside a statement"));
            }
        }
        IntroduceKind::Constant => {
            if !is_constant_expression(expr, src) {
                return Err(cannot(
                    "Selected expression cannot be used as a constant initializer",
                ));
            }
        }
        IntroduceKind::Field => {
            let in_method = site.named.is_some_and(|m| m.kind() == "method_declaration");
            match site.class {
                Some(c) if in_method && c.kind() != "interface_declaration" => {
                    if c.kind() == "enum_declaration" {
                        return Err(cannot("Enums cannot have fields"));
                    }
                }
                _ => return Err(cannot("The expression should be inside a class method")),
            }
            if super::statement_of(expr).is_none() {
                return Err(cannot("The expression is not inside a statement"));
            }
        }
        IntroduceKind::Parameter => {
            let Some(f) = site.named else {
                return Err(cannot(
                    "The expression should be inside a function or method",
                ));
            };
            if f.child_by_field_name("body").is_none() {
                return Err(cannot("Abstract methods have no body"));
            }
            let params: HashSet<String> = f
                .child_by_field_name("parameters")
                .map(|p| parameter_names(p, src).into_iter().collect())
                .unwrap_or_default();
            if let Some(local) = vars::collect(expr, src)
                .into_iter()
                .find(|u| !params.contains(&u.name))
            {
                return Err(cannot(format!(
                    "The expression uses the local variable ${}",
                    local.name
                )));
            }
            if vars::uses_this(expr, src) && is_static_method(f) {
                return Err(cannot("The expression uses $this in a static method"));
            }
        }
    }
    Ok(())
}

fn anchor_candidates(n: Node) -> Vec<Node> {
    let mut out = Vec::new();
    let mut cur = n.parent();
    while let Some(p) = cur {
        if super::is_function_like(p) || super::is_nested_declaration(p) {
            break;
        }
        if is_statement(p) {
            out.push(p);
        }
        cur = p.parent();
    }
    out
}

/// Where the declaration of a new variable goes: the statement before which it is
/// inserted, and whether that statement is the body of a control structure without braces
/// (which then gets braces).
fn anchor<'t>(occ: &[Node<'t>]) -> Result<(Node<'t>, bool), String> {
    let first = occ[0];
    let inside = |outer: Node| {
        occ.iter()
            .all(|o| outer.start_byte() <= o.start_byte() && o.end_byte() <= outer.end_byte())
    };
    for s in anchor_candidates(first) {
        let Some(p) = s.parent() else { continue };
        if is_statement_list(p) {
            if inside(p) {
                return Ok((s, false));
            }
        } else if s.kind() != "compound_statement" && is_control_body(p, s) && inside(s) {
            return Ok((s, true));
        }
    }
    Err(cannot("The expression is not inside a statement"))
}

/// `s` is the body of `if`/`else`/a loop, written without braces.
fn is_control_body(p: Node, s: Node) -> bool {
    matches!(
        p.kind(),
        "if_statement"
            | "else_clause"
            | "else_if_clause"
            | "while_statement"
            | "for_statement"
            | "foreach_statement"
            | "do_statement"
    ) && p.child_by_field_name("body") == Some(s)
}

/// The edits replacing the occurrences and declaring `decl` (`$name = <expr>;`) before the
/// anchor statement. `replacement` replaces the occurrences.
fn declare_before(
    src: &str,
    occ: &[Node],
    decl: &dyn Fn(&str) -> String,
    replacement: &str,
) -> Result<Vec<TextEdit>, String> {
    let (stmt, wrap) = anchor(occ)?;
    let first = occ[0];
    let indent = leading_ws(src, stmt.start_byte());
    let nl = eol(src);
    let expr_text = text(first, src);
    let replaced = |n: Node| TextEdit {
        start: n.start_byte(),
        end: n.end_byte(),
        new_text: fit(n, replacement, "variable_name"),
    };
    if wrap {
        // `if ($x) return foo();` -> `if ($x) {\n    $v = foo();\n    return $v;\n}`.
        let control = stmt.parent().expect("control");
        let ind = leading_ws(src, control.start_byte());
        let unit = indent_unit(src);
        let inner: Vec<TextEdit> = occ.iter().map(|n| replaced(*n)).collect();
        let mut body = text(stmt, src).to_string();
        let mut sorted = inner;
        sorted.sort_by_key(|e| std::cmp::Reverse(e.start));
        for e in sorted {
            body.replace_range(
                e.start - stmt.start_byte()..e.end - stmt.start_byte(),
                &e.new_text,
            );
        }
        let from = src[..stmt.start_byte()].trim_end().len();
        let decl_text = decl(&reindent(
            expr_text,
            &leading_ws(src, first.start_byte()),
            &format!("{ind}{unit}"),
        ));
        let body = reindent(
            &body,
            &leading_ws(src, stmt.start_byte()),
            &format!("{ind}{unit}"),
        );
        return Ok(vec![TextEdit {
            start: from,
            end: stmt.end_byte(),
            new_text: format!(" {{{nl}{ind}{unit}{decl_text}{nl}{ind}{unit}{body}{nl}{ind}}}"),
        }]);
    }
    let decl_text = decl(&reindent(
        expr_text,
        &leading_ws(src, first.start_byte()),
        &indent,
    ));
    let mut edits = Vec::new();
    let whole_statement =
        stmt.kind() == "expression_statement" && stmt.named_child(0) == Some(first);
    if whole_statement {
        // `foo();` -> `$v = foo();` (no separate line).
        edits.push(TextEdit {
            start: stmt.start_byte(),
            end: stmt.end_byte(),
            new_text: decl_text,
        });
    } else {
        edits.push(TextEdit {
            start: stmt.start_byte(),
            end: stmt.start_byte(),
            new_text: format!("{decl_text}{nl}{indent}"),
        });
        edits.push(replaced(first));
    }
    edits.extend(occ[1..].iter().map(|n| replaced(*n)));
    Ok(edits)
}

/// A simple type for a declaration from the expression (`'a'` -> `string`, `new Foo` ->
/// `Foo`).
fn simple_type(n: Node, src: &str) -> Option<String> {
    match n.kind() {
        "string" | "encapsed_string" | "heredoc" | "nowdoc" => Some("string".into()),
        "integer" => Some("int".into()),
        "float" => Some("float".into()),
        "boolean" => Some("bool".into()),
        "array_creation_expression" => Some("array".into()),
        "object_creation_expression" => {
            let mut c = n.walk();
            let class = n
                .named_children(&mut c)
                .find(|ch| matches!(ch.kind(), "name" | "qualified_name"))
                .map(|ch| text(ch, src).to_string());
            class
        }
        "cast_expression" => n
            .child_by_field_name("type")
            .map(|t| match text(t, src).trim().to_lowercase().as_str() {
                "integer" => "int".to_string(),
                "boolean" => "bool".to_string(),
                "double" | "real" => "float".to_string(),
                other => other.to_string(),
            })
            .filter(|t| matches!(t.as_str(), "int" | "bool" | "float" | "string" | "array")),
        "parenthesized_expression" => n.named_child(0).and_then(|c| simple_type(c, src)),
        _ => None,
    }
}

/// Introduce the variable, constant, field or parameter for the expression at
/// `start..end` (as returned by [`prepare`]). Introduce Parameter's call sites are not part
/// of the plan; see [`parameter_call_edits`].
pub fn introduce(
    src: &str,
    path: &str,
    start: usize,
    end: usize,
    kind: IntroduceKind,
    opts: &IntroduceOptions,
) -> Result<Plan, String> {
    let name = opts.name.trim().trim_start_matches('$').to_string();
    check_name(&name)?;
    let tree = parse(src).ok_or_else(|| cannot(NOT_AN_EXPRESSION))?;
    let root = tree.root_node();
    let expr = expression_at(root, src, start, end).ok_or_else(|| cannot(NOT_AN_EXPRESSION))?;
    let site = Site::new(expr, root);
    check(&site, kind, src)?;
    let occ = if opts.replace_all {
        site.occurrences(kind, src)
    } else {
        vec![expr]
    };
    let mut conflicts = Vec::new();
    let edits = match kind {
        IntroduceKind::Variable => {
            let taken = vars::names_in(site.scope(kind), src);
            if taken.contains(&name) {
                conflicts.push(super::conflict_at(
                    format!("Variable ${name} is already defined in the scope"),
                    path,
                    src,
                    expr.start_byte(),
                ));
            }
            let var = format!("${}", marked(&name));
            declare_before(src, &occ, &|e| format!("{var} = {e};"), &var)?
        }
        IntroduceKind::Field => {
            introduce_field(src, path, &site, &occ, &name, opts, &mut conflicts)?
        }
        IntroduceKind::Constant => {
            introduce_constant(src, path, &site, &occ, &name, opts, &mut conflicts)?
        }
        IntroduceKind::Parameter => {
            introduce_parameter(src, path, &site, &occ, &name, opts, &mut conflicts)?
        }
    };
    let (edits, name_spans) = finish(src, edits);
    Ok(Plan {
        edits,
        other_files: vec![],
        conflicts,
        name_spans,
    })
}

fn class_index(src: &str, path: &str, class: Node) -> Option<ClassLike> {
    let file = index_source(path, src);
    file.class_at(class.start_byte() + 1)
        .or_else(|| file.class_at(class.end_byte().saturating_sub(1)))
        .cloned()
}

#[allow(clippy::too_many_arguments)]
fn introduce_field(
    src: &str,
    path: &str,
    site: &Site,
    occ: &[Node],
    name: &str,
    opts: &IntroduceOptions,
    conflicts: &mut Vec<super::RenameConflict>,
) -> Result<Vec<TextEdit>, String> {
    let class = site.class.expect("checked");
    let method = site.named.expect("checked");
    let is_static = is_static_method(method);
    if class_members(class, src, "property_element").contains(name) {
        conflicts.push(super::conflict_at(
            format!(
                "Field ${name} is already defined in class {}",
                class_name(class, src).unwrap_or_default()
            ),
            path,
            src,
            class.start_byte(),
        ));
    }
    let info = class_index(src, path, class).ok_or_else(|| cannot("The class cannot be parsed"))?;
    let fmt = Fmt::for_class(src, &info, &opts.style);
    let vis = opts.visibility.unwrap_or(Visibility::Private).as_str();
    let access = if is_static {
        format!("self::${}", marked(name))
    } else {
        format!("$this->{}", marked(name))
    };
    let expr = site.expr;
    let init = match opts.init {
        FieldInit::FieldDeclaration if !is_constant_expression(expr, src) => {
            return Err(cannot(
                "The expression cannot initialize a field declaration",
            ))
        }
        FieldInit::Constructor if is_static || uses_locals(expr, src) => {
            return Err(cannot(
                "The expression cannot be evaluated in the constructor",
            ))
        }
        i => i,
    };
    let ty = match init {
        FieldInit::CurrentMethod => None,
        _ => simple_type(expr, src),
    };
    let value = match init {
        FieldInit::FieldDeclaration => format!(" = {}", text(expr, src)),
        _ => String::new(),
    };
    let decl = format!(
        "{vis}{} {}${}{value};",
        if is_static { " static" } else { "" },
        ty.map(|t| format!("{t} ")).unwrap_or_default(),
        marked(name),
    );
    let mut edits = Vec::new();
    match init {
        FieldInit::CurrentMethod => {
            edits.extend(declare_before(
                src,
                occ,
                &|e| format!("{access} = {e};"),
                &access,
            )?);
            edits.push(field_declaration(src, &info, &fmt, &decl));
        }
        FieldInit::FieldDeclaration => {
            edits.extend(occ.iter().map(|n| replace(*n, &access)));
            edits.push(field_declaration(src, &info, &fmt, &decl));
        }
        FieldInit::Constructor => {
            edits.extend(occ.iter().map(|n| replace(*n, &access)));
            let assign = format!("$this->{} = {};", marked(name), text(expr, src));
            match constructor_body(class, src) {
                Some(body) => {
                    edits.push(field_declaration(src, &info, &fmt, &decl));
                    edits.push(append_to_body(src, body, &assign, &fmt.unit));
                }
                None => {
                    let ctor = format!("public function __construct()\n{{\n    {assign}\n}}");
                    let block = format!("{decl}{}{ctor}", fmt.member_separator());
                    edits.push(field_declaration(src, &info, &fmt, &block));
                }
            }
        }
    }
    Ok(edits)
}

/// A new field: right after the other fields, else where Generate puts fields.
fn field_declaration(src: &str, class: &ClassLike, fmt: &Fmt, block: &str) -> TextEdit {
    let last_field = class
        .members
        .iter()
        .filter(|m| m.kind == SymbolKind::Property && !m.promoted)
        .map(|m| m.range.end)
        .max();
    match last_field {
        Some(end) => TextEdit {
            start: end,
            end,
            new_text: fmt.eol(format!("\n{}", fmt.indent_block(block))),
        },
        None => insert_after_fields(src, class, fmt, block),
    }
}

fn replace(n: Node, with: &str) -> TextEdit {
    TextEdit {
        start: n.start_byte(),
        end: n.end_byte(),
        new_text: fit(n, with, "member_access_expression"),
    }
}

fn constructor_body<'t>(class: Node<'t>, src: &str) -> Option<Node<'t>> {
    let body = class.child_by_field_name("body")?;
    let mut c = body.walk();
    let ctor = body.named_children(&mut c).find(|m| {
        m.kind() == "method_declaration"
            && m.child_by_field_name("name")
                .is_some_and(|n| text(n, src).eq_ignore_ascii_case("__construct"))
    });
    ctor?.child_by_field_name("body")
}

/// Append a statement at the end of a `{ ... }` body.
fn append_to_body(src: &str, body: Node, stmt: &str, unit: &str) -> TextEdit {
    let nl = eol(src);
    let close = body.end_byte() - 1;
    let open = body.start_byte();
    let base = leading_ws(src, open);
    let inner = &src[open + 1..close];
    let last_stmt_end = open + 1 + inner.trim_end().len();
    if inner.trim().is_empty() {
        TextEdit {
            start: open + 1,
            end: close,
            new_text: format!("{nl}{base}{unit}{stmt}{nl}{base}"),
        }
    } else {
        let indent = {
            let mut c = body.walk();
            let first = body.named_children(&mut c).next();
            first
                .map(|f| leading_ws(src, f.start_byte()))
                .unwrap_or_else(|| format!("{base}{unit}"))
        };
        TextEdit {
            start: last_stmt_end,
            end: last_stmt_end,
            new_text: format!("{nl}{indent}{stmt}"),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn introduce_constant(
    src: &str,
    path: &str,
    site: &Site,
    occ: &[Node],
    name: &str,
    opts: &IntroduceOptions,
    conflicts: &mut Vec<super::RenameConflict>,
) -> Result<Vec<TextEdit>, String> {
    let value = text(site.expr, src);
    let mut edits = Vec::new();
    match site.class {
        Some(class) => {
            if class_members(class, src, "const_element").contains(name) {
                conflicts.push(super::conflict_at(
                    format!(
                        "Constant {name} is already defined in class {}",
                        class_name(class, src).unwrap_or_default()
                    ),
                    path,
                    src,
                    class.start_byte(),
                ));
            }
            let info = class_index(src, path, class)
                .ok_or_else(|| cannot("The class cannot be parsed"))?;
            let fmt = Fmt::for_class(src, &info, &opts.style);
            let vis = opts
                .visibility
                .map(|v| format!("{} ", v.as_str()))
                .unwrap_or_default();
            let decl = format!("{vis}const {} = {value};", marked(name));
            edits.push(constant_declaration(src, &info, &fmt, &decl));
            let reference = format!("self::{}", marked(name));
            edits.extend(occ.iter().map(|n| replace(*n, &reference)));
        }
        None => {
            let nl = eol(src);
            let root = site.root;
            let (at, sep) = global_constant_place(root);
            edits.push(TextEdit {
                start: at,
                end: at,
                new_text: format!("{sep}const {} = {value};", marked(name)).replace('\n', nl),
            });
            let reference = marked(name);
            edits.extend(occ.iter().map(|n| replace(*n, &reference)));
        }
    }
    Ok(edits)
}

/// After the other constants, else at the top of the class body.
fn constant_declaration(src: &str, class: &ClassLike, fmt: &Fmt, decl: &str) -> TextEdit {
    let last = class
        .members
        .iter()
        .filter(|m| matches!(m.kind, SymbolKind::Constant | SymbolKind::EnumCase))
        .map(|m| m.range.end)
        .max();
    let open = class.body_range.start;
    let close = class.body_range.end - 1;
    match last {
        Some(end) => {
            let end = end.min(close);
            TextEdit {
                start: end,
                end,
                new_text: fmt.eol(format!("\n{}", fmt.indent_block(decl))),
            }
        }
        None if src[open + 1..close].trim().is_empty() => {
            insert_at_end(src, class, fmt, &[decl.to_string()])
        }
        None => {
            // Keep the class' `use Trait;` lines first.
            let after_uses = class_trait_uses_end(src, open, close);
            TextEdit {
                start: after_uses,
                end: after_uses,
                new_text: fmt.eol(format!("\n{}\n", fmt.indent_block(decl))),
            }
        }
    }
}

fn class_trait_uses_end(src: &str, open: usize, close: usize) -> usize {
    let tree = match parse(src) {
        Some(t) => t,
        None => return open + 1,
    };
    let Some(body) = tree
        .root_node()
        .descendant_for_byte_range(open, open + 1)
        .and_then(|n| {
            if n.kind() == "declaration_list" {
                Some(n)
            } else {
                n.parent()
            }
        })
    else {
        return open + 1;
    };
    let mut c = body.walk();
    let end = body
        .named_children(&mut c)
        .filter(|ch| ch.kind() == "use_declaration" && ch.end_byte() <= close)
        .map(|ch| ch.end_byte())
        .max();
    end.unwrap_or(open + 1)
}

/// Where a global `const` goes: after the namespace, imports and other constants.
fn global_constant_place(root: Node) -> (usize, &'static str) {
    let mut c = root.walk();
    let mut at = None;
    let mut sep = "\n\n";
    for ch in root.named_children(&mut c) {
        match ch.kind() {
            "php_tag" => {
                at = Some(ch.end_byte());
                sep = "\n\n";
            }
            "namespace_definition" if ch.child_by_field_name("body").is_none() => {
                at = Some(ch.end_byte());
                sep = "\n\n";
            }
            "namespace_use_declaration" | "declare_statement" => {
                at = Some(ch.end_byte());
                sep = "\n\n";
            }
            "const_declaration" => {
                at = Some(ch.end_byte());
                sep = "\n";
            }
            "comment" => {}
            _ => break,
        }
    }
    (at.unwrap_or(0), sep)
}

#[allow(clippy::too_many_arguments)]
fn introduce_parameter(
    src: &str,
    path: &str,
    site: &Site,
    occ: &[Node],
    name: &str,
    opts: &IntroduceOptions,
    conflicts: &mut Vec<super::RenameConflict>,
) -> Result<Vec<TextEdit>, String> {
    let f = site.named.expect("checked");
    let params = f
        .child_by_field_name("parameters")
        .ok_or_else(|| cannot("The function has no parameter list"))?;
    let existing = parameter_names(params, src);
    if existing.iter().any(|p| p == name)
        || vars::names_in(scope_body(Some(f), site.root), src).contains(name)
    {
        conflicts.push(super::conflict_at(
            format!("Variable ${name} is already defined in the function"),
            path,
            src,
            params.start_byte(),
        ));
    }
    if opts.default_value && !is_constant_expression(site.expr, src) {
        return Err(cannot("The expression cannot be a default value"));
    }
    let ty = simple_type(site.expr, src);
    let decl = format!(
        "{}${}{}",
        ty.map(|t| format!("{t} ")).unwrap_or_default(),
        marked(name),
        if opts.default_value {
            format!(" = {}", text(site.expr, src))
        } else {
            String::new()
        }
    );
    let mut edits = vec![add_parameter(src, params, &decl)];
    let var = format!("${}", marked(name));
    edits.extend(occ.iter().map(|n| TextEdit {
        start: n.start_byte(),
        end: n.end_byte(),
        new_text: var.clone(),
    }));
    Ok(edits)
}

/// Add a parameter declaration at the end of the list (before a variadic one).
fn add_parameter(src: &str, params: Node, decl: &str) -> TextEdit {
    let mut c = params.walk();
    let list: Vec<Node> = params
        .named_children(&mut c)
        .filter(|p| p.kind() != "comment")
        .collect();
    let multiline = text(params, src).contains('\n');
    let nl = eol(src);
    match list.iter().position(|p| p.kind() == "variadic_parameter") {
        Some(i) => {
            let v = list[i];
            let sep = if multiline {
                format!(",{nl}{}", leading_ws(src, v.start_byte()))
            } else {
                ", ".into()
            };
            TextEdit {
                start: v.start_byte(),
                end: v.start_byte(),
                new_text: format!("{decl}{sep}"),
            }
        }
        None => match list.last() {
            Some(last) => {
                let sep = if multiline {
                    format!(",{nl}{}", leading_ws(src, last.start_byte()))
                } else {
                    ", ".into()
                };
                TextEdit {
                    start: last.end_byte(),
                    end: last.end_byte(),
                    new_text: format!("{sep}{decl}"),
                }
            }
            None => TextEdit {
                start: params.start_byte() + 1,
                end: params.start_byte() + 1,
                new_text: decl.to_string(),
            },
        },
    }
}

/// What a call site of the function needs to pass the new parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterCall {
    pub path: String,
    /// The function's name (call sites are recognized by it).
    pub function: String,
    /// The declaration's name range (not a call site).
    pub declaration: Span,
    /// The new parameter (`limit`).
    pub name: String,
    /// The expression and its variables: the function's parameters by position, `$this`.
    pub expression: String,
    pub expression_kind: String,
    /// The function's parameters in order, with their default values.
    pub parameters: Vec<(String, Option<String>)>,
    /// Number of parameters before the new one (its position).
    pub position: usize,
}

/// The data [`parameter_call_edits`] needs, from the same input as [`introduce`].
pub fn parameter_call(
    src: &str,
    path: &str,
    start: usize,
    end: usize,
    name: &str,
) -> Option<ParameterCall> {
    let tree = parse(src)?;
    let root = tree.root_node();
    let expr = expression_at(root, src, start, end)?;
    let f = enclosing_named_function(expr)?;
    let fname = f.child_by_field_name("name")?;
    let params = f.child_by_field_name("parameters")?;
    let mut c = params.walk();
    let mut parameters = Vec::new();
    let mut position = 0;
    for p in params.named_children(&mut c) {
        let Some(n) = p.child_by_field_name("name") else {
            continue;
        };
        if p.kind() != "variadic_parameter" {
            position += 1;
        }
        parameters.push((
            text(n, src).trim_start_matches('$').to_string(),
            p.child_by_field_name("default_value")
                .map(|d| text(d, src).to_string()),
        ));
    }
    Some(ParameterCall {
        path: path.to_string(),
        function: text(fname, src).to_string(),
        declaration: Span::of(fname),
        name: name.trim_start_matches('$').to_string(),
        expression: text(expr, src).to_string(),
        expression_kind: expr.kind().to_string(),
        parameters,
        position,
    })
}

/// The call node around a function / method name at `span`.
pub(crate) fn call_at<'t>(root: Node<'t>, span: Span) -> Option<Node<'t>> {
    let mut n = root.descendant_for_byte_range(span.start, span.end)?;
    for _ in 0..4 {
        let p = n.parent()?;
        match p.kind() {
            "function_call_expression" => {
                if p.child_by_field_name("function")
                    .is_some_and(|f| f.start_byte() <= span.start && span.end <= f.end_byte())
                {
                    return Some(p);
                }
            }
            "member_call_expression"
            | "nullsafe_member_call_expression"
            | "scoped_call_expression"
                if p.child_by_field_name("name") == Some(n) =>
            {
                return Some(p);
            }
            _ => {}
        }
        n = p;
    }
    None
}

/// The arguments of a call: positional ones in order and named ones by name.
pub(crate) struct CallArgs<'t> {
    pub node: Node<'t>,
    pub positional: Vec<Node<'t>>,
    pub named: HashMap<String, Node<'t>>,
    pub spread: bool,
    /// First-class callable syntax `foo(...)`.
    pub callable: bool,
}

pub(crate) fn call_args<'t>(call: Node<'t>, src: &str) -> Option<CallArgs<'t>> {
    let args = call.child_by_field_name("arguments")?;
    let mut out = CallArgs {
        node: args,
        positional: vec![],
        named: HashMap::new(),
        spread: false,
        callable: false,
    };
    let mut c = args.walk();
    for a in args.named_children(&mut c) {
        match a.kind() {
            "argument" => {
                let name = a.child_by_field_name("name");
                let mut c2 = a.walk();
                let value = a
                    .named_children(&mut c2)
                    .filter(|v| Some(*v) != name)
                    .last();
                let Some(value) = value else { continue };
                if text(a, src).trim_start().starts_with("...") {
                    out.spread = true;
                } else if let Some(name) = a.child_by_field_name("name") {
                    out.named.insert(text(name, src).to_string(), value);
                } else {
                    out.positional.push(value);
                }
            }
            "variadic_placeholder" => out.callable = true,
            "comment" => {}
            _ => {}
        }
    }
    if text(args, src).replace(char::is_whitespace, "") == "(...)" {
        out.callable = true;
    }
    Some(out)
}

/// The receiver of a method call: `$obj` of `$obj->foo()`, `$this` for `self::`/`parent::`.
pub(crate) fn receiver(call: Node, src: &str) -> Option<String> {
    match call.kind() {
        "member_call_expression" | "nullsafe_member_call_expression" => call
            .child_by_field_name("object")
            .map(|o| text(o, src).to_string()),
        "scoped_call_expression" => {
            // `self::foo()` / `static::` / `parent::` run on `$this`; `Foo::bar()` has none.
            let scope = call.child_by_field_name("scope")?;
            (scope.kind() == "relative_scope").then(|| "$this".into())
        }
        _ => None,
    }
}

/// The text of an expression with its variables replaced (`$this` and parameters).
pub(crate) fn substitute(
    expr: &str,
    values: &dyn Fn(&str) -> Option<(String, String)>,
) -> Option<String> {
    let src = format!("<?php {expr};");
    let tree = parse(&src)?;
    let root = tree.root_node();
    let vars = super::collect_nodes(root, true, &mut |n| n.kind() == "variable_name");
    let mut out = expr.to_string();
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for v in vars {
        let name = text(v, &src).trim_start_matches('$');
        if let Some((value, kind)) = values(name) {
            let s = v.start_byte() - 6;
            edits.push((
                s,
                s + (v.end_byte() - v.start_byte()),
                fit(v, &value, &kind),
            ));
        }
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    for (s, e, t) in edits {
        out.replace_range(s..e, &t);
    }
    Some(out)
}

/// Introduce Parameter: pass the expression at every call site. `usages` are the
/// function's references from Find Usages; `text_of` reads a file. Call sites that cannot
/// be updated become conflicts.
pub fn parameter_call_edits(
    call: &ParameterCall,
    usages: &[UsageSpan],
    text_of: &mut dyn FnMut(&str) -> Option<String>,
) -> (Vec<FileEdits>, Vec<super::RenameConflict>) {
    let mut by_file: Vec<(String, Vec<Span>)> = Vec::new();
    for u in usages {
        if u.path == call.path && u.span == call.declaration {
            continue;
        }
        match by_file.iter_mut().find(|(p, _)| *p == u.path) {
            Some((_, spans)) => spans.push(u.span),
            None => by_file.push((u.path.clone(), vec![u.span])),
        }
    }
    let mut out = Vec::new();
    let mut conflicts = Vec::new();
    for (path, spans) in by_file {
        let Some(src) = text_of(&path) else { continue };
        let Some(tree) = parse(&src) else { continue };
        let root = tree.root_node();
        let mut edits = Vec::new();
        for span in spans {
            let problem = |what: &str| {
                super::conflict_at(
                    format!("The call to {}() {what}", call.function),
                    &path,
                    &src,
                    span.start,
                )
            };
            let Some(site) = call_at(root, span) else {
                conflicts.push(problem("is not a call and is not updated"));
                continue;
            };
            let Some(args) = call_args(site, &src) else {
                continue;
            };
            if args.spread || args.callable {
                conflicts.push(problem(
                    "passes its arguments dynamically and is not updated",
                ));
                continue;
            }
            let recv = receiver(site, &src);
            let missing = std::cell::Cell::new(false);
            let value = substitute(&call.expression, &|var| {
                if var == "this" {
                    return Some((
                        recv.clone().unwrap_or_else(|| "$this".into()),
                        "variable_name".into(),
                    ));
                }
                let i = call.parameters.iter().position(|(p, _)| p == var)?;
                if let Some(a) = args.named.get(var).or_else(|| args.positional.get(i)) {
                    return Some((text(*a, &src).to_string(), a.kind().to_string()));
                }
                match &call.parameters[i].1 {
                    Some(d) => Some((d.clone(), "parenthesized_expression".into())),
                    None => {
                        missing.set(true);
                        None
                    }
                }
            });
            let Some(value) = value.filter(|_| !missing.get()) else {
                conflicts.push(problem("misses arguments the expression needs"));
                continue;
            };
            let named = !args.named.is_empty() || args.positional.len() < call.position;
            let arg = if named {
                format!("{}: {value}", call.name)
            } else {
                value
            };
            let mut c = args.node.walk();
            let last = args
                .node
                .named_children(&mut c)
                .filter(|a| a.kind() != "comment")
                .last();
            edits.push(match last {
                Some(l) => TextEdit {
                    start: l.end_byte(),
                    end: l.end_byte(),
                    new_text: format!(", {arg}"),
                },
                None => TextEdit {
                    start: args.node.start_byte() + 1,
                    end: args.node.start_byte() + 1,
                    new_text: arg,
                },
            });
        }
        if !edits.is_empty() {
            out.push(FileEdits { path, edits });
        }
    }
    (out, conflicts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refactor::test_util::{apply, sel};

    fn info(marked: &str, kind: IntroduceKind) -> IntroduceInfo {
        let (src, s, e) = sel(marked);
        prepare(&src, s, e, kind).unwrap().info.unwrap()
    }

    fn run(marked: &str, kind: IntroduceKind, opts: IntroduceOptions) -> (String, Plan) {
        let (src, s, e) = sel(marked);
        let plan = introduce(&src, "a.php", s, e, kind, &opts).unwrap();
        (apply(&src, &plan.edits), plan)
    }

    fn opts(name: &str, all: bool) -> IntroduceOptions {
        IntroduceOptions {
            name: name.into(),
            replace_all: all,
            ..Default::default()
        }
    }

    const ORDER: &str = "<?php
class Order
{
    private array $items = [];

    public function total(): float
    {
        $sum = 0;
        foreach ($this->items as $item) {
            $sum += ‹$item->price * $item->quantity›;
        }
        if ($sum > 100) return $sum * 0.9 + $item->price * $item->quantity;
        return $sum;
    }
}
";

    #[test]
    fn extracts_a_variable_before_the_statement() {
        let (out, plan) = run(ORDER, IntroduceKind::Variable, opts("subtotal", false));
        assert!(out.contains(
            "        foreach ($this->items as $item) {\n            $subtotal = $item->price * $item->quantity;\n            $sum += $subtotal;\n        }"
        ), "{out}");
        assert_eq!(plan.name_spans.len(), 2);
        for s in &plan.name_spans {
            assert_eq!(&out[s.start..s.end], "subtotal");
        }
    }

    #[test]
    fn counts_occurrences_and_suggests_names() {
        let i = info(ORDER, IntroduceKind::Variable);
        assert_eq!(i.occurrences, 2);
        assert_eq!(i.names[0], "result");
        assert_eq!(i.text, "$item->price * $item->quantity");
    }

    #[test]
    fn replacing_all_occurrences_declares_in_the_common_block_and_wraps_parens() {
        let (out, _) = run(ORDER, IntroduceKind::Variable, opts("subtotal", true));
        assert!(out.contains("        $sum = 0;\n        $subtotal = $item->price * $item->quantity;\n        foreach"), "{out}");
        assert!(out.contains("$sum += $subtotal;"));
        assert!(out.contains("return $sum * 0.9 + $subtotal;"), "{out}");
    }

    #[test]
    fn braceless_bodies_get_braces() {
        let src = "<?php\nfunction f($x)\n{\n    if ($x) return ‹strlen($x)›;\n    return 0;\n}\n";
        let (out, _) = run(src, IntroduceKind::Variable, opts("length", false));
        assert_eq!(
            out,
            "<?php\nfunction f($x)\n{\n    if ($x) {\n        $length = strlen($x);\n        return $length;\n    }\n    return 0;\n}\n"
        );
    }

    #[test]
    fn a_whole_expression_statement_becomes_the_assignment() {
        let src = "<?php\nfunction f()\n{\n    ‹save($a)›;\n}\n";
        let (out, _) = run(src, IntroduceKind::Variable, opts("saved", false));
        assert!(out.contains("    $saved = save($a);\n}"), "{out}");
    }

    #[test]
    fn offers_the_expressions_at_the_caret() {
        let (src, s, e) = sel("<?php\n$n = strlen($this->us‸er->name) + 1;");
        let p = prepare(&src, s, e, IntroduceKind::Variable).unwrap();
        let texts: Vec<_> = p.expressions.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "$this->user",
                "$this->user->name",
                "strlen($this->user->name)",
                "strlen($this->user->name) + 1"
            ]
        );
        assert!(p.info.is_none());
    }

    #[test]
    fn rejects_non_expressions() {
        let (src, s, e) = sel("<?php\n‹function f() {}›");
        let err = prepare(&src, s, e, IntroduceKind::Variable).unwrap_err();
        assert_eq!(
            err,
            "Cannot perform refactoring. Selected block should represent an expression"
        );
    }

    #[test]
    fn introduces_class_constants_after_the_others() {
        let src = "<?php\nclass A\n{\n    const X = 1;\n\n    public function f()\n    {\n        return ‹'active'› . 'active';\n    }\n}\n";
        let i = info(src, IntroduceKind::Constant);
        assert_eq!(i.names[0], "ACTIVE");
        assert_eq!(i.occurrences, 2);
        let o = IntroduceOptions {
            visibility: Some(Visibility::Private),
            ..opts("ACTIVE", true)
        };
        let (out, plan) = run(src, IntroduceKind::Constant, o);
        assert!(
            out.contains("    const X = 1;\n    private const ACTIVE = 'active';\n"),
            "{out}"
        );
        assert!(out.contains("return self::ACTIVE . self::ACTIVE;"));
        assert_eq!(plan.name_spans.len(), 3);
        let (src2, s, e) = sel("<?php\nclass A { function f() { return ‹$x›; } }");
        assert!(prepare(&src2, s, e, IntroduceKind::Constant).is_err());
    }

    #[test]
    fn introduces_global_constants_after_the_imports() {
        let src = "<?php\nnamespace App;\n\nuse Foo\\Bar;\n\necho ‹42›;\n";
        let (out, _) = run(src, IntroduceKind::Constant, opts("ANSWER", false));
        assert_eq!(
            out,
            "<?php\nnamespace App;\n\nuse Foo\\Bar;\n\nconst ANSWER = 42;\n\necho ANSWER;\n"
        );
    }

    const USER: &str = "<?php
class User
{
    private string $name;

    public function greet()
    {
        return ‹'Hello'› . $this->name;
    }
}
";

    #[test]
    fn introduces_fields_initialized_in_the_method() {
        let i = info(USER, IntroduceKind::Field);
        assert_eq!(
            i.init_places,
            vec![
                FieldInit::CurrentMethod,
                FieldInit::FieldDeclaration,
                FieldInit::Constructor
            ]
        );
        let (out, _) = run(USER, IntroduceKind::Field, opts("greeting", false));
        assert!(
            out.contains("    private string $name;\n    private $greeting;\n"),
            "{out}"
        );
        assert!(
            out.contains(
                "        $this->greeting = 'Hello';\n        return $this->greeting . $this->name;"
            ),
            "{out}"
        );
    }

    #[test]
    fn introduces_fields_in_the_declaration_or_constructor() {
        let o = IntroduceOptions {
            init: FieldInit::FieldDeclaration,
            visibility: Some(Visibility::Protected),
            ..opts("greeting", false)
        };
        let (out, _) = run(USER, IntroduceKind::Field, o);
        assert!(
            out.contains("    protected string $greeting = 'Hello';\n"),
            "{out}"
        );
        assert!(out.contains("return $this->greeting . $this->name;"));
        let o = IntroduceOptions {
            init: FieldInit::Constructor,
            ..opts("greeting", false)
        };
        let (out, _) = run(USER, IntroduceKind::Field, o);
        assert!(
            out.contains("    private string $greeting;\n\n    public function __construct()\n    {\n        $this->greeting = 'Hello';\n    }\n"),
            "{out}"
        );
    }

    #[test]
    fn appends_to_an_existing_constructor() {
        let src = "<?php\nclass A\n{\n    public function __construct()\n    {\n        $this->a = 1;\n    }\n\n    public function f()\n    {\n        return ‹[1, 2]›;\n    }\n}\n";
        let o = IntroduceOptions {
            init: FieldInit::Constructor,
            ..opts("list", false)
        };
        let (out, _) = run(src, IntroduceKind::Field, o);
        assert!(
            out.contains("        $this->a = 1;\n        $this->list = [1, 2];\n    }"),
            "{out}"
        );
        assert!(
            out.contains("{\n    private array $list;\n\n    public function __construct()"),
            "{out}"
        );
    }

    #[test]
    fn introduces_parameters_and_updates_calls() {
        let src = "<?php\nclass Mailer\n{\n    public function send($to)\n    {\n        return mail($to, ‹strtoupper($to)›);\n    }\n\n    public function run()\n    {\n        $this->send('a@b.c');\n    }\n}\n";
        let (clean, s, e) = sel(src);
        let i = info(src, IntroduceKind::Parameter);
        assert_eq!(i.function_name.as_deref(), Some("Mailer::send()"));
        let plan = introduce(
            &clean,
            "Mailer.php",
            s,
            e,
            IntroduceKind::Parameter,
            &opts("subject", false),
        )
        .unwrap();
        let out = apply(&clean, &plan.edits);
        assert!(out.contains("public function send($to, $subject)"), "{out}");
        assert!(out.contains("return mail($to, $subject);"));
        let call = parameter_call(&clean, "Mailer.php", s, e, "subject").unwrap();
        let send_call = clean.find("send('a").unwrap();
        let usages = vec![
            UsageSpan {
                path: "Mailer.php".into(),
                span: call.declaration,
            },
            UsageSpan {
                path: "Mailer.php".into(),
                span: Span::new(send_call, send_call + 4),
            },
            UsageSpan {
                path: "b.php".into(),
                span: Span::new(15, 19),
            },
        ];
        let other = "<?php $m->x()->send(to: 'x');";
        assert_eq!(&other[15..19], "send");
        let (files, conflicts) = parameter_call_edits(&call, &usages, &mut |p| {
            Some(if p == "b.php" {
                other.to_string()
            } else {
                clean.clone()
            })
        });
        assert!(conflicts.is_empty(), "{conflicts:?}");
        assert_eq!(
            apply(&clean, &files[0].edits)
                .matches("$this->send('a@b.c', strtoupper('a@b.c'))")
                .count(),
            1
        );
        assert_eq!(
            apply(other, &files[1].edits),
            "<?php $m->x()->send(to: 'x', subject: strtoupper('x'));"
        );
    }

    #[test]
    fn parameters_with_default_values_and_local_variables() {
        let src = "<?php\nfunction f($a, ...$rest)\n{\n    return $a + ‹10›;\n}\n";
        let o = IntroduceOptions {
            default_value: true,
            ..opts("offset", false)
        };
        let (out, _) = run(src, IntroduceKind::Parameter, o);
        assert!(
            out.contains("function f($a, int $offset = 10, ...$rest)"),
            "{out}"
        );
        let (src, s, e) = sel("<?php\nfunction f()\n{\n    $x = 1;\n    return ‹$x + 1›;\n}\n");
        assert_eq!(
            prepare(&src, s, e, IntroduceKind::Parameter).unwrap_err(),
            "Cannot perform refactoring. The expression uses the local variable $x"
        );
    }

    #[test]
    fn works_in_script_code_and_closures() {
        let (out, _) = run(
            "<?php
$total = ‹count($items)› * 2;
",
            IntroduceKind::Variable,
            opts("count", false),
        );
        assert_eq!(
            out,
            "<?php
$count = count($items);
$total = $count * 2;
"
        );
        let src = "<?php
class A
{
    public function f($xs)
    {
        return array_map(function ($x) {
            return ‹$x * 2› + 1;
        }, $xs);
    }
}
";
        let (out, _) = run(src, IntroduceKind::Variable, opts("double", false));
        assert!(out.contains("        return array_map(function ($x) {\n            $double = $x * 2;\n            return $double + 1;\n        }, $xs);"), "{out}");
        // Arrow functions have no statements to put the variable before.
        let (src, s, e) = sel("<?php\n$f = fn($x) => ‹$x + 1›;\n");
        assert_eq!(
            prepare(&src, s, e, IntroduceKind::Variable).unwrap_err(),
            "Cannot perform refactoring. The expression is not inside a statement"
        );
    }
}
