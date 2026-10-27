//! A small data-flow analysis of PHP functions (and the top-level code of a file), built on
//! the tree-sitter parse tree:
//!
//! - "Variable is always null" (`PhpVariableIsAlwaysNull`): a member access or method call
//!   on a variable that holds `null` on every path;
//! - "Undefined variable" (`PhpUndefinedVariable`): a variable read inside a function
//!   before any path assigned it;
//! - "Condition is always true / false" (`PhpConstantCondition`): an `if`, `elseif`,
//!   `while` or ternary condition whose value is known from literals and variables holding
//!   known constants;
//! - "Unused assignment" (`PhpUnusedAssignment`): a value assigned to a local variable that
//!   is overwritten before it is read, or never read afterwards.
//!
//! The analysis is deliberately modest: values are tracked through straight-line code and
//! branches (merged at joins), loops forget every variable they assign, and a function
//! that uses `$$name`, `extract()`, `compact()`, `get_defined_vars()`, `eval`, `include`
//! or `goto` is not analyzed. Variables shared by reference (`&$x`, `global`, `static`,
//! `use (&$x)`, by-reference parameters and `foreach` values) are never tracked. A variable
//! passed to a call may be written by reference, so it counts as read and then unknown.
//!
//! Offsets are bytes.

use std::collections::{HashMap, HashSet};

use tree_sitter::Node;

use crate::generate::TextEdit;
use crate::inspections::{PhpProblem, ProblemFix, ProblemSeverity};

pub const ALWAYS_NULL: &str = "PhpVariableIsAlwaysNull";
pub const UNDEFINED: &str = "PhpUndefinedVariable";
pub const CONSTANT_CONDITION: &str = "PhpConstantCondition";
pub const UNUSED_ASSIGNMENT: &str = "PhpUnusedAssignment";

/// A value as far as the analysis knows it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Val {
    Null,
    Bool(bool),
    Int(i64),
    /// A string literal (its hash) or another non-null value (`NotNull`).
    Str(u64),
    NotNull,
    Unknown,
}

impl Val {
    /// PHP truthiness of known values.
    fn truthy(self) -> Option<bool> {
        match self {
            Val::Null => Some(false),
            Val::Bool(b) => Some(b),
            Val::Int(i) => Some(i != 0),
            // `''` and `'0'` are falsy; string hashes do not tell, objects are truthy only
            // when they are objects.
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Var {
    /// Not assigned on any path.
    Undefined,
    Defined(Val),
    /// Assigned on some paths only, or unknown.
    Maybe,
}

#[derive(Debug, Clone)]
struct State {
    vars: HashMap<String, Var>,
    /// For variables not in `vars`: `Undefined` in functions, `Maybe` at the top level.
    default: Var,
    /// After `return`, `throw`, `break`, `continue`, `exit`.
    dead: bool,
}

impl State {
    fn get(&self, name: &str) -> Var {
        self.vars.get(name).copied().unwrap_or(self.default)
    }

    fn set(&mut self, name: &str, v: Var) {
        self.vars.insert(name.to_string(), v);
    }

    /// Forget what is known about `name` (it may have been written).
    fn havoc(&mut self, name: &str) {
        let v = match self.get(name) {
            Var::Defined(_) => Var::Defined(Val::Unknown),
            _ => Var::Maybe,
        };
        self.set(name, v);
    }

    fn merge(a: State, b: State) -> State {
        if a.dead {
            return b;
        }
        if b.dead {
            return a;
        }
        let mut out = State {
            vars: HashMap::new(),
            default: a.default,
            dead: false,
        };
        let names: HashSet<&String> = a.vars.keys().chain(b.vars.keys()).collect();
        for n in names {
            let v = match (a.get(n), b.get(n)) {
                (Var::Undefined, Var::Undefined) => Var::Undefined,
                (Var::Defined(x), Var::Defined(y)) => {
                    Var::Defined(if x == y { x } else { Val::Unknown })
                }
                _ => Var::Maybe,
            };
            out.vars.insert(n.clone(), v);
        }
        out
    }
}

fn hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn named_children(n: Node) -> Vec<Node> {
    let mut c = n.walk();
    n.named_children(&mut c).collect()
}

fn is_unit(kind: &str) -> bool {
    matches!(
        kind,
        "function_definition" | "method_declaration" | "anonymous_function"
    )
}

fn is_loop(kind: &str) -> bool {
    matches!(
        kind,
        "while_statement" | "do_statement" | "for_statement" | "foreach_statement"
    )
}

/// Constructs that make variables impossible to follow.
fn has_dynamic_scope(n: Node, src: &str) -> bool {
    match n.kind() {
        "dynamic_variable_name"
        | "include_expression"
        | "include_once_expression"
        | "require_expression"
        | "require_once_expression"
        | "goto_statement"
        | "named_label_statement" => return true,
        "function_call_expression" => {
            if let Some(f) = n.child_by_field_name("function") {
                let name = src[f.byte_range()]
                    .trim_start_matches('\\')
                    .to_ascii_lowercase();
                if matches!(
                    name.as_str(),
                    "extract"
                        | "compact"
                        | "get_defined_vars"
                        | "eval"
                        | "parse_str"
                        | "func_get_args"
                ) {
                    return true;
                }
            }
        }
        "variable_name" if &src[n.byte_range()] == "$GLOBALS" => return true,
        _ => {}
    }
    // Nested functions and closures have their own scope.
    named_children(n)
        .into_iter()
        .filter(|c| !is_unit(c.kind()))
        .any(|c| has_dynamic_scope(c, src))
}

fn var_name<'a>(n: Node, src: &'a str) -> Option<&'a str> {
    (n.kind() == "variable_name").then(|| src[n.byte_range()].trim_start_matches('$'))
}

/// Variables shared by reference somewhere in a unit (never tracked).
fn escaped_vars(unit_body: Node, params: Option<Node>, src: &str) -> HashSet<String> {
    let mut out: HashSet<String> = HashSet::new();
    out.insert("this".into());
    if let Some(ps) = params {
        for p in named_children(ps) {
            if p.child_by_field_name("reference_modifier").is_some() {
                if let Some(n) = p.child_by_field_name("name").and_then(|n| var_name(n, src)) {
                    out.insert(n.into());
                }
            }
        }
    }
    fn walk(n: Node, src: &str, out: &mut HashSet<String>) {
        match n.kind() {
            "by_ref" => {
                for c in named_children(n) {
                    if let Some(v) = var_name(c, src) {
                        out.insert(v.into());
                    }
                }
            }
            "reference_assignment_expression" => {
                for f in ["left", "right"] {
                    if let Some(v) = n.child_by_field_name(f).and_then(|c| var_name(c, src)) {
                        out.insert(v.into());
                    }
                }
            }
            "global_declaration" | "function_static_declaration" => {
                let mut stack = vec![n];
                while let Some(x) = stack.pop() {
                    if let Some(v) = var_name(x, src) {
                        out.insert(v.into());
                    } else {
                        stack.extend(named_children(x));
                    }
                }
            }
            _ => {}
        }
        for c in named_children(n) {
            if !matches!(c.kind(), "function_definition" | "method_declaration") {
                walk(c, src, out);
            }
        }
    }
    walk(unit_body, src, &mut out);
    out
}

/// Every variable a subtree may write (for loops, `switch`, `try`).
fn written_vars(n: Node, src: &str, out: &mut HashSet<String>) {
    let add_vars_in = |x: Node, out: &mut HashSet<String>| {
        let mut stack = vec![x];
        while let Some(y) = stack.pop() {
            if let Some(v) = var_name(y, src) {
                out.insert(v.into());
            } else if !is_unit(y.kind()) {
                stack.extend(named_children(y));
            }
        }
    };
    match n.kind() {
        "assignment_expression"
        | "augmented_assignment_expression"
        | "reference_assignment_expression" => {
            if let Some(l) = n.child_by_field_name("left") {
                add_vars_in(l, out);
            }
        }
        "update_expression" | "unset_statement" | "catch_clause" | "arguments" => {
            add_vars_in(n, out)
        }
        "foreach_statement" => {
            // Everything after the subject is a binding.
            for c in named_children(n).into_iter().skip(1) {
                if c.kind() != "compound_statement" && c.kind() != "colon_block" {
                    add_vars_in(c, out);
                }
            }
        }
        _ => {}
    }
    for c in named_children(n) {
        if !is_unit(c.kind()) {
            written_vars(c, src, out);
        }
    }
}

struct Analyzer<'a> {
    src: &'a str,
    out: Vec<PhpProblem>,
    escaped: HashSet<String>,
    /// Inside a function (undefined variables are reported).
    function: bool,
    reported_undefined: HashSet<String>,
}

impl<'a> Analyzer<'a> {
    fn text(&self, n: Node) -> &'a str {
        &self.src[n.byte_range()]
    }

    fn report(&mut self, n: Node, code: &str, severity: ProblemSeverity, message: String) {
        self.out.push(PhpProblem {
            start: n.start_byte(),
            end: n.end_byte(),
            severity,
            message,
            code: code.into(),
            deprecated: false,
            unnecessary: false,
            fix: None,
        });
    }

    fn read_var(&mut self, n: Node, st: &mut State, silent: bool) -> Val {
        let Some(name) = var_name(n, self.src) else {
            return Val::Unknown;
        };
        if name == "this" {
            return Val::NotNull;
        }
        if self.escaped.contains(name) || is_superglobal(name) {
            return Val::Unknown;
        }
        match st.get(name) {
            Var::Defined(v) => v,
            Var::Undefined => {
                if !silent
                    && self.function
                    && !st.dead
                    && self.reported_undefined.insert(name.to_string())
                {
                    self.report(
                        n,
                        UNDEFINED,
                        ProblemSeverity::Warning,
                        format!("Variable '${name}' is undefined here"),
                    );
                }
                Val::Unknown
            }
            Var::Maybe => Val::Unknown,
        }
    }

    fn write_var(&mut self, n: Node, st: &mut State, v: Val) {
        if let Some(name) = var_name(n, self.src) {
            if !self.escaped.contains(name) {
                st.set(name, Var::Defined(v));
            }
        } else {
            // A destructuring target or something else: its variables become unknown.
            self.havoc_in(n, st);
        }
    }

    fn havoc_in(&mut self, n: Node, st: &mut State) {
        let mut stack = vec![n];
        while let Some(x) = stack.pop() {
            if let Some(name) = var_name(x, self.src) {
                if st.get(name) == Var::Undefined {
                    st.set(name, Var::Defined(Val::Unknown));
                } else {
                    st.havoc(name);
                }
            } else if !is_unit(x.kind()) {
                stack.extend(named_children(x));
            }
        }
    }

    fn havoc_written(&mut self, n: Node, st: &mut State) {
        let mut vars = HashSet::new();
        written_vars(n, self.src, &mut vars);
        for v in vars {
            st.havoc(&v);
        }
    }

    /// A condition whose value is known: report it.
    fn check_condition(&mut self, cond: Node, v: Val, st: &State) {
        if st.dead {
            return;
        }
        let Some(b) = v.truthy() else {
            return;
        };
        let inner = if cond.kind() == "parenthesized_expression" {
            named_children(cond).into_iter().next().unwrap_or(cond)
        } else {
            cond
        };
        self.report(
            inner,
            CONSTANT_CONDITION,
            ProblemSeverity::Warning,
            format!("Condition is always {}", if b { "true" } else { "false" }),
        );
    }

    fn eval(&mut self, n: Node, st: &mut State, silent: bool) -> Val {
        match n.kind() {
            "variable_name" => self.read_var(n, st, silent),
            "null" => Val::Null,
            "boolean" => Val::Bool(self.text(n).eq_ignore_ascii_case("true")),
            "integer" => {
                let t = self.text(n).replace('_', "");
                t.parse::<i64>().map_or(Val::NotNull, Val::Int)
            }
            "float" => Val::NotNull,
            "string" => {
                let simple = named_children(n)
                    .iter()
                    .all(|c| c.kind() == "string_content");
                if simple {
                    Val::Str(hash(self.text(n).trim_matches(['\'', '"'])))
                } else {
                    self.eval_children(n, st, silent);
                    Val::NotNull
                }
            }
            "encapsed_string" | "heredoc" | "nowdoc" | "shell_command_expression" => {
                self.eval_children(n, st, silent);
                Val::NotNull
            }
            "anonymous_class" => {
                // Constructor arguments are read here; its methods have their own scope.
                for c in named_children(n) {
                    match c.kind() {
                        "arguments" => {
                            self.eval(c, st, silent);
                            self.havoc_args(n, st);
                        }
                        "declaration_list" => {
                            for m in named_children(c)
                                .into_iter()
                                .filter(|m| m.kind() == "method_declaration")
                            {
                                analyze_unit(m, self.src, &mut self.out);
                            }
                        }
                        _ => {}
                    }
                }
                Val::NotNull
            }
            "array_creation_expression" | "object_creation_expression" => {
                self.eval_children(n, st, silent);
                if n.kind() == "object_creation_expression" {
                    self.havoc_args(n, st);
                }
                Val::NotNull
            }
            "parenthesized_expression" => named_children(n)
                .into_iter()
                .next()
                .map_or(Val::Unknown, |c| self.eval(c, st, silent)),
            "error_suppression_expression" => {
                self.eval_children(n, st, true);
                Val::Unknown
            }
            "assignment_expression" => {
                let (Some(l), Some(r)) = (
                    n.child_by_field_name("left"),
                    n.child_by_field_name("right"),
                ) else {
                    return Val::Unknown;
                };
                let v = self.eval(r, st, silent);
                match l.kind() {
                    "variable_name" => self.write_var(l, st, v),
                    "subscript_expression" => {
                        // `$a[] = 1` creates the array; the index is read.
                        let mut base = l;
                        while base.kind() == "subscript_expression" {
                            for c in named_children(base).into_iter().skip(1) {
                                self.eval(c, st, silent);
                            }
                            match named_children(base).into_iter().next() {
                                Some(b) => base = b,
                                None => break,
                            }
                        }
                        if let Some(name) = var_name(base, self.src) {
                            if !self.escaped.contains(name) {
                                let next = match st.get(name) {
                                    Var::Defined(Val::Null) | Var::Undefined => {
                                        Var::Defined(Val::NotNull)
                                    }
                                    Var::Defined(_) => Var::Defined(Val::Unknown),
                                    Var::Maybe => Var::Maybe,
                                };
                                st.set(name, next);
                            }
                        } else {
                            self.eval(base, st, silent);
                        }
                    }
                    "list_literal" | "array_creation_expression" => self.havoc_in(l, st),
                    _ => {
                        // `$o->p = ...`, `Foo::$p = ...`: the object is read.
                        self.eval(l, st, silent);
                    }
                }
                v
            }
            "reference_assignment_expression" => {
                for f in ["left", "right"] {
                    if let Some(c) = n.child_by_field_name(f) {
                        self.havoc_in(c, st);
                    }
                }
                Val::Unknown
            }
            "augmented_assignment_expression" => {
                let op = n
                    .child_by_field_name("operator")
                    .map(|o| self.text(o))
                    .unwrap_or("");
                if let Some(r) = n.child_by_field_name("right") {
                    self.eval(r, st, silent);
                }
                if let Some(l) = n.child_by_field_name("left") {
                    self.eval(l, st, silent || op == "??=");
                    self.havoc_in(l, st);
                    if let Some(name) = var_name(l, self.src) {
                        if !self.escaped.contains(name) {
                            st.set(name, Var::Defined(Val::Unknown));
                        }
                    }
                }
                Val::Unknown
            }
            "update_expression" => {
                for c in named_children(n) {
                    self.eval(c, st, silent);
                    self.havoc_in(c, st);
                }
                Val::Unknown
            }
            "member_call_expression" | "member_access_expression" => {
                if let Some(obj) = n.child_by_field_name("object") {
                    let v = self.eval(obj, st, silent);
                    if v == Val::Null && obj.kind() == "variable_name" && !st.dead {
                        let name = self.text(obj).to_string();
                        self.report(
                            obj,
                            ALWAYS_NULL,
                            ProblemSeverity::Warning,
                            format!("Variable '{name}' is always null here"),
                        );
                    }
                }
                for c in named_children(n).into_iter().skip(1) {
                    self.eval(c, st, silent);
                }
                if n.kind() == "member_call_expression" {
                    self.havoc_args(n, st);
                }
                Val::Unknown
            }
            "function_call_expression" => {
                let fname = n
                    .child_by_field_name("function")
                    .map(|f| self.text(f).trim_start_matches('\\').to_ascii_lowercase())
                    .unwrap_or_default();
                let args: Vec<Node> = n
                    .child_by_field_name("arguments")
                    .map(|a| {
                        named_children(a)
                            .into_iter()
                            .filter(|c| c.kind() == "argument")
                            .collect()
                    })
                    .unwrap_or_default();
                let quiet = silent || matches!(fname.as_str(), "isset" | "empty");
                let mut vals = Vec::new();
                for a in &args {
                    let inner = named_children(*a).into_iter().last();
                    // A plain variable argument may be an out parameter (`preg_match`).
                    let v = match inner {
                        Some(v) if v.kind() == "variable_name" && !quiet => {
                            let name = var_name(v, self.src).unwrap_or("");
                            if st.get(name) == Var::Undefined {
                                Val::Unknown
                            } else {
                                self.eval(v, st, quiet)
                            }
                        }
                        Some(v) => self.eval(v, st, quiet),
                        None => Val::Unknown,
                    };
                    vals.push(v);
                }
                if let Some(f) = n.child_by_field_name("function") {
                    if f.kind() != "name" && f.kind() != "qualified_name" {
                        self.eval(f, st, silent);
                    }
                }
                let result = match (fname.as_str(), vals.as_slice()) {
                    ("is_null", [v]) if *v != Val::Unknown => Val::Bool(*v == Val::Null),
                    ("isset", [v])
                        if args.len() == 1
                            && named_children(args[0])
                                .last()
                                .is_some_and(|x| x.kind() == "variable_name")
                            && *v != Val::Unknown =>
                    {
                        Val::Bool(*v != Val::Null)
                    }
                    _ => Val::Unknown,
                };
                if matches!(fname.as_str(), "isset" | "empty") {
                    // Guarded: whatever follows may read it.
                    for a in &args {
                        if let Some(name) = named_children(*a)
                            .last()
                            .and_then(|v| var_name(*v, self.src))
                        {
                            if st.get(name) == Var::Undefined {
                                st.set(name, Var::Maybe);
                            }
                        }
                    }
                } else if fname != "is_null" {
                    self.havoc_args(n, st);
                }
                result
            }
            "scoped_call_expression" | "nullsafe_member_call_expression" => {
                self.eval_children(n, st, silent);
                self.havoc_args(n, st);
                Val::Unknown
            }
            "binary_expression" => self.eval_binary(n, st, silent),
            "unary_op_expression" => {
                let op = n
                    .child_by_field_name("operator")
                    .map(|o| self.text(o).to_string())
                    .unwrap_or_default();
                let v = n
                    .child_by_field_name("argument")
                    .or_else(|| named_children(n).into_iter().last())
                    .map_or(Val::Unknown, |a| self.eval(a, st, silent));
                match op.as_str() {
                    "!" => v.truthy().map_or(Val::Unknown, |b| Val::Bool(!b)),
                    _ => Val::Unknown,
                }
            }
            "conditional_expression" => {
                let cond = n.child_by_field_name("condition");
                let c = cond.map_or(Val::Unknown, |c| self.eval(c, st, silent));
                if let Some(cn) = cond {
                    if n.child_by_field_name("body").is_some() {
                        self.check_condition(cn, c, st);
                    }
                }
                let mut a = st.clone();
                let va = n
                    .child_by_field_name("body")
                    .map_or(c, |b| self.eval(b, &mut a, silent));
                let mut b = st.clone();
                let vb = n
                    .child_by_field_name("alternative")
                    .map_or(Val::Unknown, |x| self.eval(x, &mut b, silent));
                *st = State::merge(a, b);
                if va == vb {
                    va
                } else {
                    Val::Unknown
                }
            }
            "match_expression" => {
                let mut children = named_children(n).into_iter();
                if let Some(c) = children.next() {
                    self.eval(c, st, silent);
                }
                let pre = st.clone();
                let mut merged: Option<State> = None;
                for arm in children.flat_map(named_children) {
                    let mut s = pre.clone();
                    self.eval_children(arm, &mut s, silent);
                    merged = Some(match merged {
                        Some(m) => State::merge(m, s),
                        None => s,
                    });
                }
                if let Some(m) = merged {
                    *st = State::merge(pre, m);
                }
                Val::Unknown
            }
            "throw_expression" | "exit_statement" => {
                self.eval_children(n, st, silent);
                st.dead = true;
                Val::Unknown
            }
            "anonymous_function" => {
                if let Some(uses) = named_children(n)
                    .into_iter()
                    .find(|c| c.kind() == "anonymous_function_use_clause")
                {
                    for u in named_children(uses) {
                        if u.kind() == "variable_name" {
                            self.eval(u, st, true);
                        } else {
                            self.havoc_in(u, st);
                        }
                    }
                }
                analyze_unit(n, self.src, &mut self.out);
                Val::NotNull
            }
            "arrow_function" => {
                // Reads the enclosing scope by value; its own parameters shadow it.
                let mut inner = st.clone();
                if let Some(ps) = n.child_by_field_name("parameters") {
                    self.havoc_in(ps, &mut inner);
                }
                if let Some(body) = n.child_by_field_name("body") {
                    self.eval(body, &mut inner, true);
                }
                Val::NotNull
            }
            "scoped_property_access_expression" => {
                // `self::$cache`: a static property, not a local variable.
                if let Some(scope) = n.child_by_field_name("scope") {
                    self.eval(scope, st, silent);
                }
                Val::Unknown
            }
            "cast_expression"
            | "clone_expression"
            | "print_intrinsic"
            | "yield_expression"
            | "sequence_expression"
            | "argument"
            | "arguments"
            | "pair"
            | "array_element_initializer"
            | "subscript_expression"
            | "class_constant_access_expression"
            | "nullsafe_member_access_expression"
            | "instanceof_expression" => {
                self.eval_children(n, st, silent);
                Val::Unknown
            }
            _ => {
                self.eval_children(n, st, silent);
                Val::Unknown
            }
        }
    }

    fn eval_children(&mut self, n: Node, st: &mut State, silent: bool) {
        for c in named_children(n) {
            if !matches!(
                c.kind(),
                "function_definition" | "method_declaration" | "class_declaration"
            ) {
                self.eval(c, st, silent);
            }
        }
    }

    /// Variables passed to a call may be written by reference.
    fn havoc_args(&mut self, call: Node, st: &mut State) {
        let Some(args) = call.child_by_field_name("arguments") else {
            return;
        };
        for a in named_children(args) {
            if let Some(v) = named_children(a).into_iter().last() {
                if let Some(name) = var_name(v, self.src) {
                    if !self.escaped.contains(name) {
                        let next = match st.get(name) {
                            Var::Defined(_) | Var::Undefined => Var::Defined(Val::Unknown),
                            Var::Maybe => Var::Maybe,
                        };
                        st.set(name, next);
                    }
                }
            }
        }
    }

    fn eval_binary(&mut self, n: Node, st: &mut State, silent: bool) -> Val {
        let op = n
            .child_by_field_name("operator")
            .map(|o| self.text(o).to_ascii_lowercase())
            .unwrap_or_default();
        let (Some(l), Some(r)) = (
            n.child_by_field_name("left"),
            n.child_by_field_name("right"),
        ) else {
            self.eval_children(n, st, silent);
            return Val::Unknown;
        };
        match op.as_str() {
            "&&" | "and" | "||" | "or" => {
                let a = self.eval(l, st, silent);
                let mut right = st.clone();
                let b = self.eval(r, &mut right, silent);
                *st = State::merge(st.clone(), right);
                let (ta, tb) = (a.truthy(), b.truthy());
                let and = op == "&&" || op == "and";
                match (and, ta, tb) {
                    (true, Some(false), _) | (true, _, Some(false)) => Val::Bool(false),
                    (true, Some(true), Some(true)) => Val::Bool(true),
                    (false, Some(true), _) | (false, _, Some(true)) => Val::Bool(true),
                    (false, Some(false), Some(false)) => Val::Bool(false),
                    _ => Val::Unknown,
                }
            }
            "??" => {
                let a = self.eval(l, st, true);
                let mut right = st.clone();
                let b = self.eval(r, &mut right, silent);
                *st = State::merge(st.clone(), right);
                match a {
                    Val::Null => b,
                    Val::Unknown => Val::Unknown,
                    v => v,
                }
            }
            _ => {
                let a = self.eval(l, st, silent);
                let b = self.eval(r, st, silent);
                compare(&op, a, b)
            }
        }
    }

    fn exec(&mut self, n: Node, st: &mut State) {
        if st.dead {
            return;
        }
        match n.kind() {
            "compound_statement" | "colon_block" | "program" | "declaration_list" => {
                for c in named_children(n) {
                    if st.dead {
                        break;
                    }
                    self.exec(c, st);
                }
            }
            "expression_statement" | "echo_statement" => self.eval_children(n, st, false),
            "return_statement" => {
                self.eval_children(n, st, false);
                st.dead = true;
            }
            "break_statement" | "continue_statement" => st.dead = true,
            "exit_statement" => {
                self.eval_children(n, st, false);
                st.dead = true;
            }
            "unset_statement" => {
                for c in named_children(n) {
                    if let Some(name) = var_name(c, self.src) {
                        st.set(name, Var::Maybe);
                    } else {
                        self.eval(c, st, true);
                    }
                }
            }
            "global_declaration" | "function_static_declaration" => {
                for c in named_children(n) {
                    if c.kind() == "static_variable_declaration" {
                        if let Some(v) = c.child_by_field_name("value") {
                            self.eval(v, st, false);
                        }
                    }
                }
            }
            "if_statement" => self.exec_if(n, st),
            "while_statement" => {
                self.havoc_written(n, st);
                if let Some(c) = n.child_by_field_name("condition") {
                    let v = self.eval(c, st, false);
                    let literal_true = matches!(v, Val::Bool(true) | Val::Int(1))
                        && named_children(c)
                            .first()
                            .is_some_and(|x| matches!(x.kind(), "boolean" | "integer"));
                    if !literal_true {
                        self.check_condition(c, v, st);
                    }
                }
                let mut body = st.clone();
                if let Some(b) = n.child_by_field_name("body") {
                    self.exec(b, &mut body);
                }
                body.dead = false;
                *st = State::merge(st.clone(), body);
            }
            "do_statement" => {
                self.havoc_written(n, st);
                let mut body = st.clone();
                if let Some(b) = n.child_by_field_name("body") {
                    self.exec(b, &mut body);
                }
                body.dead = false;
                if let Some(c) = n.child_by_field_name("condition") {
                    self.eval(c, &mut body, false);
                }
                *st = State::merge(st.clone(), body);
            }
            "for_statement" => {
                // The initializers run once, the rest repeats.
                let children = named_children(n);
                let body = n.child_by_field_name("body");
                self.havoc_written(n, st);
                let mut inner = st.clone();
                for c in children {
                    if Some(c) == body {
                        self.exec(c, &mut inner);
                    } else if c.kind() != "comment" {
                        self.eval(c, &mut inner, true);
                    }
                }
                inner.dead = false;
                *st = State::merge(st.clone(), inner);
            }
            "foreach_statement" => {
                let children = named_children(n);
                if let Some(subject) = children.first() {
                    self.eval(*subject, st, false);
                }
                self.havoc_written(n, st);
                let mut inner = st.clone();
                for c in children.iter().skip(1) {
                    if matches!(c.kind(), "compound_statement" | "colon_block")
                        || Some(*c) == n.child_by_field_name("body")
                    {
                        self.exec(*c, &mut inner);
                    } else {
                        self.havoc_in(*c, &mut inner);
                    }
                }
                inner.dead = false;
                *st = State::merge(st.clone(), inner);
            }
            "switch_statement" => {
                if let Some(c) = n.child_by_field_name("condition") {
                    self.eval(c, st, false);
                }
                self.havoc_written(n, st);
                let pre = st.clone();
                let mut merged = pre.clone();
                if let Some(body) = n.child_by_field_name("body") {
                    for case in named_children(body) {
                        let mut s = pre.clone();
                        for c in named_children(case) {
                            if s.dead {
                                break;
                            }
                            if matches!(
                                c.kind(),
                                "compound_statement"
                                    | "expression_statement"
                                    | "if_statement"
                                    | "return_statement"
                                    | "echo_statement"
                                    | "break_statement"
                                    | "continue_statement"
                                    | "foreach_statement"
                                    | "while_statement"
                                    | "for_statement"
                                    | "switch_statement"
                                    | "try_statement"
                                    | "unset_statement"
                                    | "do_statement"
                            ) {
                                self.exec(c, &mut s);
                            } else {
                                self.eval(c, &mut s, false);
                            }
                        }
                        s.dead = false;
                        merged = State::merge(merged, s);
                    }
                }
                *st = merged;
            }
            "try_statement" => {
                let pre = st.clone();
                let mut tried = st.clone();
                if let Some(b) = n.child_by_field_name("body") {
                    self.exec(b, &mut tried);
                }
                let mut after_catch = pre.clone();
                self.havoc_written(n, &mut after_catch);
                let mut merged = tried;
                let mut finally = None;
                for c in named_children(n) {
                    match c.kind() {
                        "catch_clause" => {
                            let mut s = after_catch.clone();
                            if let Some(v) = c.child_by_field_name("name") {
                                self.write_var(v, &mut s, Val::NotNull);
                            }
                            if let Some(b) = c.child_by_field_name("body") {
                                self.exec(b, &mut s);
                            }
                            merged = State::merge(merged, s);
                        }
                        "finally_clause" => finally = Some(c),
                        _ => {}
                    }
                }
                if let Some(f) = finally {
                    let dead = merged.dead;
                    let mut s = State::merge(merged, after_catch.clone());
                    s.dead = false;
                    for c in named_children(f) {
                        self.exec(c, &mut s);
                    }
                    s.dead = s.dead || dead;
                    merged = s;
                }
                *st = merged;
            }
            "function_definition" | "method_declaration" => {
                analyze_unit(n, self.src, &mut self.out)
            }
            "class_declaration"
            | "trait_declaration"
            | "enum_declaration"
            | "interface_declaration" => {
                let mut stack = vec![n];
                while let Some(x) = stack.pop() {
                    if x.kind() == "method_declaration" {
                        analyze_unit(x, self.src, &mut self.out);
                    } else {
                        stack.extend(named_children(x));
                    }
                }
            }
            "namespace_definition" => {
                if let Some(b) = n.child_by_field_name("body") {
                    self.exec(b, st);
                }
            }
            "comment"
            | "namespace_use_declaration"
            | "const_declaration"
            | "declare_statement"
            | "php_tag"
            | "text_interpolation"
            | "text"
            | "empty_statement" => {}
            _ => {
                // Statements the analysis does not model: forget what they may write.
                self.havoc_written(n, st);
                let mut copy = st.clone();
                self.eval_children(n, &mut copy, true);
            }
        }
    }

    fn exec_if(&mut self, n: Node, st: &mut State) {
        let Some(cond) = n.child_by_field_name("condition") else {
            self.havoc_written(n, st);
            return;
        };
        let v = self.eval(cond, st, false);
        self.check_condition(cond, v, st);
        let mut branches = Vec::new();
        let mut then = st.clone();
        if let Some(b) = n.child_by_field_name("body") {
            self.exec(b, &mut then);
        }
        branches.push(then);
        let mut rest = st.clone();
        let mut has_else = false;
        let mut c = n.walk();
        let alternatives: Vec<Node> = n.children_by_field_name("alternative", &mut c).collect();
        for alt in alternatives {
            match alt.kind() {
                "else_if_clause" => {
                    if let Some(ec) = alt.child_by_field_name("condition") {
                        let v = self.eval(ec, &mut rest, false);
                        self.check_condition(ec, v, &rest);
                    }
                    let mut b = rest.clone();
                    if let Some(body) = alt.child_by_field_name("body") {
                        self.exec(body, &mut b);
                    }
                    branches.push(b);
                }
                "else_clause" => {
                    has_else = true;
                    let mut b = rest.clone();
                    if let Some(body) = alt.child_by_field_name("body") {
                        self.exec(body, &mut b);
                    }
                    branches.push(b);
                }
                _ => {}
            }
        }
        if !has_else {
            branches.push(rest);
        }
        let mut merged = branches.remove(0);
        for b in branches {
            merged = State::merge(merged, b);
        }
        *st = merged;
    }
}

fn is_superglobal(name: &str) -> bool {
    matches!(
        name,
        "GLOBALS"
            | "_SERVER"
            | "_GET"
            | "_POST"
            | "_FILES"
            | "_COOKIE"
            | "_SESSION"
            | "_REQUEST"
            | "_ENV"
            | "http_response_header"
            | "argc"
            | "argv"
            | "php_errormsg"
    )
}

fn compare(op: &str, a: Val, b: Val) -> Val {
    let known = |v: Val| !matches!(v, Val::Unknown | Val::NotNull);
    if !known(a) || !known(b) {
        return Val::Unknown;
    }
    let ints = match (a, b) {
        (Val::Int(x), Val::Int(y)) => Some((x, y)),
        _ => None,
    };
    let same_kind = std::mem::discriminant(&a) == std::mem::discriminant(&b);
    match op {
        "===" => Val::Bool(a == b),
        "!==" => Val::Bool(a != b),
        // Loose comparison only between values of the same kind.
        "==" if same_kind => Val::Bool(a == b),
        "!=" | "<>" if same_kind => Val::Bool(a != b),
        "<" => ints.map_or(Val::Unknown, |(x, y)| Val::Bool(x < y)),
        ">" => ints.map_or(Val::Unknown, |(x, y)| Val::Bool(x > y)),
        "<=" => ints.map_or(Val::Unknown, |(x, y)| Val::Bool(x <= y)),
        ">=" => ints.map_or(Val::Unknown, |(x, y)| Val::Bool(x >= y)),
        _ => Val::Unknown,
    }
}

/// Analyze a function, method or closure.
fn analyze_unit(unit: Node, src: &str, out: &mut Vec<PhpProblem>) {
    let Some(body) = unit.child_by_field_name("body") else {
        return;
    };
    if body.kind() != "compound_statement" || has_dynamic_scope(body, src) {
        return;
    }
    let params = unit.child_by_field_name("parameters");
    let mut escaped = escaped_vars(body, params, src);
    let mut st = State {
        vars: HashMap::new(),
        default: Var::Undefined,
        dead: false,
    };
    if let Some(ps) = params {
        for p in named_children(ps) {
            if let Some(n) = p.child_by_field_name("name").and_then(|n| var_name(n, src)) {
                st.set(n, Var::Defined(Val::Unknown));
            }
        }
    }
    // Closure `use` variables are defined inside.
    for u in named_children(unit)
        .into_iter()
        .filter(|c| c.kind() == "anonymous_function_use_clause")
    {
        let mut stack = vec![u];
        while let Some(x) = stack.pop() {
            if let Some(n) = var_name(x, src) {
                st.set(n, Var::Defined(Val::Unknown));
                if x.parent().is_some_and(|p| p.kind() == "by_ref") {
                    escaped.insert(n.to_string());
                }
            } else {
                stack.extend(named_children(x));
            }
        }
    }
    let mut a = Analyzer {
        src,
        out: Vec::new(),
        escaped: escaped.clone(),
        function: true,
        reported_undefined: HashSet::new(),
    };
    a.exec(body, &mut st);
    out.extend(a.out);
    unused_assignments(body, src, &escaped, out);
}

/// One occurrence of a variable, in evaluation order.
struct Event<'t> {
    name: String,
    /// Where it happens (a write after its right-hand side).
    pos: usize,
    write: Option<Node<'t>>,
}

fn collect_events<'t>(n: Node<'t>, src: &str, out: &mut Vec<Event<'t>>) {
    match n.kind() {
        "function_definition" | "method_declaration" | "class_declaration" => return,
        "anonymous_class" => {
            for a in named_children(n)
                .into_iter()
                .filter(|c| c.kind() == "arguments")
            {
                collect_events(a, src, out);
            }
            return;
        }
        "anonymous_function" => {
            // Only its `use` clause reads the enclosing scope.
            for u in named_children(n)
                .into_iter()
                .filter(|c| c.kind() == "anonymous_function_use_clause")
            {
                collect_events(u, src, out);
            }
            return;
        }
        "scoped_property_access_expression" => {
            if let Some(scope) = n.child_by_field_name("scope") {
                collect_events(scope, src, out);
            }
            return;
        }
        "variable_name" => {
            if let Some(name) = var_name(n, src) {
                out.push(Event {
                    name: name.into(),
                    pos: n.start_byte(),
                    write: None,
                });
            }
            return;
        }
        "assignment_expression" => {
            let plain_statement = n
                .parent()
                .is_some_and(|p| p.kind() == "expression_statement");
            if let (true, Some(l), Some(r)) = (
                plain_statement,
                n.child_by_field_name("left"),
                n.child_by_field_name("right"),
            ) {
                if let Some(name) = var_name(l, src) {
                    collect_events(r, src, out);
                    out.push(Event {
                        name: name.into(),
                        pos: n.end_byte(),
                        write: Some(n),
                    });
                    return;
                }
            }
        }
        _ => {}
    }
    for c in named_children(n) {
        collect_events(c, src, out);
    }
}

/// Inside a loop or a `try` block of the unit.
fn has_ancestor(n: Node, body: Node, pred: impl Fn(Node) -> bool) -> bool {
    let mut cur = n.parent();
    while let Some(p) = cur {
        if p.id() == body.id() {
            return false;
        }
        if pred(p) {
            return true;
        }
        cur = p.parent();
    }
    false
}

/// Statements between `a` and `b` (siblings) that may jump away.
fn jumps_between(a: Node, b: Node) -> bool {
    fn jumps(n: Node) -> bool {
        matches!(
            n.kind(),
            "break_statement" | "continue_statement" | "goto_statement" | "yield_expression"
        ) || named_children(n)
            .into_iter()
            .filter(|c| !is_unit(c.kind()) && c.kind() != "arrow_function")
            .any(jumps)
    }
    let mut cur = a.next_named_sibling();
    while let Some(s) = cur {
        if s.id() == b.id() {
            return false;
        }
        if jumps(s) {
            return true;
        }
        cur = s.next_named_sibling();
    }
    true
}

fn side_effect_free(n: Node) -> bool {
    match n.kind() {
        "null"
        | "boolean"
        | "integer"
        | "float"
        | "string"
        | "variable_name"
        | "class_constant_access_expression" => true,
        "array_creation_expression"
        | "array_element_initializer"
        | "pair"
        | "parenthesized_expression"
        | "binary_expression"
        | "unary_op_expression"
        | "encapsed_string"
        | "string_content" => named_children(n).into_iter().all(side_effect_free),
        _ => false,
    }
}

/// The fix of an unused assignment: drop the statement, or keep the right-hand side when it
/// has side effects.
fn removal_fix(assign: Node, src: &str) -> Option<ProblemFix> {
    let stmt = assign.parent()?;
    let right = assign.child_by_field_name("right")?;
    if side_effect_free(right) {
        let line_start = src[..stmt.start_byte()].rfind('\n').map_or(0, |p| p + 1);
        let line_end = src[stmt.end_byte()..]
            .find('\n')
            .map_or(src.len(), |p| stmt.end_byte() + p);
        let alone = src[line_start..stmt.start_byte()].trim().is_empty()
            && src[stmt.end_byte()..line_end].trim().is_empty();
        let (start, end) = if alone {
            (line_start, (line_end + 1).min(src.len()))
        } else {
            (stmt.start_byte(), stmt.end_byte())
        };
        Some(ProblemFix {
            title: "Remove unused assignment".into(),
            edits: vec![TextEdit {
                start,
                end,
                new_text: String::new(),
            }],
        })
    } else {
        Some(ProblemFix {
            title: "Remove assignment, keep the expression".into(),
            edits: vec![TextEdit {
                start: assign.start_byte(),
                end: right.start_byte(),
                new_text: String::new(),
            }],
        })
    }
}

fn unused_assignments(body: Node, src: &str, escaped: &HashSet<String>, out: &mut Vec<PhpProblem>) {
    let mut events = Vec::new();
    collect_events(body, src, &mut events);
    events.sort_by_key(|e| e.pos);
    for (i, e) in events.iter().enumerate() {
        let Some(assign) = e.write else {
            continue;
        };
        if escaped.contains(&e.name) || is_superglobal(&e.name) {
            continue;
        }
        let next = events[i + 1..].iter().find(|x| x.name == e.name);
        let unused = match next {
            // Overwritten before any read: by a sibling statement, with nothing in between
            // that could jump to a read (and no `catch` that could read it).
            Some(Event {
                write: Some(next_assign),
                ..
            }) => {
                let (s1, s2) = (assign.parent(), next_assign.parent());
                match (s1, s2) {
                    (Some(s1), Some(s2)) => {
                        s1.parent().map(|p| p.id()) == s2.parent().map(|p| p.id())
                            && !jumps_between(s1, s2)
                            && !has_ancestor(assign, body, |p| p.kind() == "try_statement")
                    }
                    _ => false,
                }
            }
            Some(_) => false,
            // Never read again: only outside loops (a loop may read it in the next round).
            None => !has_ancestor(assign, body, |p| is_loop(p.kind())),
        };
        if !unused {
            continue;
        }
        let name = e.name.clone();
        out.push(PhpProblem {
            start: assign.start_byte(),
            end: assign.end_byte(),
            severity: ProblemSeverity::Warning,
            message: format!("The value assigned to '${name}' is never used"),
            code: UNUSED_ASSIGNMENT.into(),
            deprecated: false,
            unnecessary: true,
            fix: removal_fix(assign, src),
        });
    }
}

/// The data-flow problems of a PHP file.
pub fn analyze(root: Node, src: &str) -> Vec<PhpProblem> {
    let mut out = Vec::new();
    let mut top = Analyzer {
        src,
        out: Vec::new(),
        escaped: HashSet::new(),
        function: false,
        reported_undefined: HashSet::new(),
    };
    if !has_dynamic_scope(root, src) {
        let mut st = State {
            vars: HashMap::new(),
            default: Var::Maybe,
            dead: false,
        };
        top.exec(root, &mut st);
    } else {
        // Still analyze the functions; the top level shares its scope with includes.
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            if matches!(n.kind(), "function_definition" | "method_declaration") {
                analyze_unit(n, src, &mut top.out);
            } else {
                stack.extend(named_children(n));
            }
        }
    }
    out.extend(top.out);
    out.sort_by_key(|p| (p.start, p.end));
    out.dedup_by(|a, b| a.start == b.start && a.end == b.end && a.code == b.code);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn problems(src: &str) -> Vec<(String, String)> {
        let mut p = parser::new_parser();
        let tree = p.parse(src, None).unwrap();
        analyze(tree.root_node(), src)
            .into_iter()
            .map(|p| (p.code, src[p.start..p.end].to_string()))
            .collect()
    }

    fn codes(src: &str, code: &str) -> Vec<String> {
        problems(src)
            .into_iter()
            .filter(|(c, _)| c == code)
            .map(|(_, t)| t)
            .collect()
    }

    #[test]
    fn null_variables() {
        let src = "<?php\nfunction f() {\n    $a = null;\n    $a->run();\n    $b = null;\n    if (rand()) { $b = new B(); }\n    $b->run();\n    $c = null;\n    $c?->run();\n    $d = null;\n    init($d);\n    $d->run();\n}\n";
        assert_eq!(codes(src, ALWAYS_NULL), ["$a"]);
    }

    #[test]
    fn undefined_variables() {
        let src = "<?php\nfunction f($p) {\n    echo $p, $x;\n    if ($p) { $y = 1; }\n    echo $y;\n    preg_match('/a/', 'a', $m);\n    echo $m[0];\n    foreach ([1] as $k => $v) { echo $k, $v; }\n    echo isset($z) ? $z : 0;\n    $w ??= 1;\n    echo $this, $_GET['a'];\n    try { $t = 1; } catch (\\Exception $e) { echo $e; }\n    $fn = function () use ($p) { return $p . $q; };\n    $af = fn($n) => $n + $p;\n    list($l1, $l2) = [1, 2];\n    echo $l1, $l2;\n    while ($i < 3) { $i = 1; }\n    echo $x;\n}\n$top = $undefinedAtTopLevel;\n";
        // `$i` in the loop is undefined in the first round only: loops are not reported.
        assert_eq!(codes(src, UNDEFINED), ["$x", "$q"]);
    }

    #[test]
    fn constant_conditions() {
        let src = "<?php\nfunction f($p) {\n    if (true) {}\n    if (1 === 2) {}\n    $debug = false;\n    if ($debug) {}\n    if (!$debug && $p) {}\n    while (true) { break; }\n    while (false) {}\n    $n = null;\n    echo is_null($n) ? 1 : 2;\n    $flag = false;\n    foreach ($p as $x) { if ($x) { $flag = true; } }\n    if ($flag) {}\n    if ($p) {}\n    $s = 'a';\n    if ($s == 'a') {}\n    if ($p === null) {}\n    if (DEBUG) {}\n}\n";
        assert_eq!(
            codes(src, CONSTANT_CONDITION),
            [
                "true",
                "1 === 2",
                "$debug",
                "false",
                "is_null($n)",
                "$s == 'a'"
            ]
        );
    }

    #[test]
    fn unused_assignments() {
        let src = "<?php\nfunction f($p) {\n    $a = 1;\n    $a = 2;\n    echo $a;\n    $b = compute();\n    $c = 3;\n    if ($p) { echo $c; }\n    $d = 1;\n    $d = $d + 1;\n    echo $d;\n    for ($i = 0; $i < 3; $i++) { $last = $i; }\n    echo $last;\n    $e = 1;\n    $fn = function () use ($e) { return $e; };\n    $g = 1;\n    $h = fn() => $g;\n    $s = 'x';\n    echo \"{$s}\";\n    $loop = 0;\n    while ($p) { $loop = $loop + 1; }\n    return [$fn, $h, $loop];\n}\nfunction g(&$r) { $r = 1; }\nfunction h() { global $gl; $gl = 2; static $st = 0; $st = 3; }\nfunction i() { $x = 1; try { $x = risky(); } catch (\\Exception $e) { return $x; } return $x; }\n";
        assert_eq!(codes(src, UNUSED_ASSIGNMENT), ["$a = 1", "$b = compute()"]);
    }

    #[test]
    fn unused_assignment_fixes() {
        let src = "<?php\nfunction f() {\n    $a = 1;\n    $b = compute();\n    return 2;\n}\n";
        let mut p = parser::new_parser();
        let tree = p.parse(src, None).unwrap();
        let found = analyze(tree.root_node(), src);
        let fixed = crate::generate::apply_edits(src, &found[0].fix.as_ref().unwrap().edits);
        assert_eq!(
            fixed,
            "<?php\nfunction f() {\n    $b = compute();\n    return 2;\n}\n"
        );
        let fixed = crate::generate::apply_edits(src, &found[1].fix.as_ref().unwrap().edits);
        assert!(fixed.contains("    compute();\n"), "{fixed}");
    }

    #[test]
    fn static_properties_are_not_variables() {
        let src = "<?php\nclass A {\n    public static function f($d) {\n        self::$cache = $d;\n        static::$x[] = 1;\n        return A::$cache;\n    }\n}\n";
        assert!(problems(src).is_empty(), "{:?}", problems(src));
        let anon = "<?php\nfunction f($a) {\n    $v = new class($a) {\n        public array $seen = [];\n        public function g() { return $missing; }\n    };\n    return $v;\n}\n";
        assert_eq!(codes(anon, UNDEFINED), ["$missing"]);
    }

    #[test]
    fn dynamic_scopes_are_not_analyzed() {
        let src = "<?php\nfunction f() {\n    extract($_GET);\n    echo $name;\n    $a = 1;\n}\nfunction g() {\n    $v = 'x';\n    $$v = 1;\n    echo $x;\n}\nfunction h() {\n    $a = 1;\n    return compact('a');\n}\n";
        assert!(problems(src).is_empty(), "{:?}", problems(src));
    }
}
