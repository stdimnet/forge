//! PHP refactorings of the Refactor menu on the tree-sitter syntax tree, like PHPStorm:
//!
//! - [`introduce`]: Introduce Variable / Constant / Field / Parameter (Ctrl+Alt+V / C / F /
//!   P), with the "Expressions" chooser, name suggestions and occurrences.
//! - [`extract_method`]: Extract Method / Function (Ctrl+Alt+M) with parameters, return
//!   values and duplicates.
//! - [`inline`]: Inline (Ctrl+Alt+N) of variables, methods, functions and constants.
//!
//! Everything here is a pure function over source text (byte offsets). Edits of other files
//! (call sites of Introduce Parameter and Inline Method) are computed from Find Usages
//! locations the caller passes in, with the texts of those files; the app converts offsets
//! to LSP positions and applies the result as one workspace edit.

pub mod available;
pub mod classes;
pub mod extract_method;
pub mod hierarchy;
pub mod inline;
pub mod introduce;
pub mod make_static;
pub mod move_class;
pub mod move_members;
pub mod names;
pub mod project;
pub mod safe_delete;
pub mod signature;
pub mod vars;

use forge_codeintel::RenameConflict;
use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Tree};

use crate::generate::TextEdit;
use crate::parser::{new_parser, parse_tree};

/// The start of every message that stops a refactoring, like PHPStorm's.
pub const CANNOT: &str = "Cannot perform refactoring.";

/// `Cannot perform refactoring. <reason>`
pub(crate) fn cannot(reason: impl AsRef<str>) -> String {
    format!("{CANNOT} {}", reason.as_ref())
}

pub(crate) const NOT_AN_EXPRESSION: &str = "Selected block should represent an expression";

/// Markers around a new name inside generated text; [`finish`] turns them into spans.
pub(crate) const NAME_START: char = '\u{3}';
pub(crate) const NAME_END: char = '\u{4}';

/// `name` wrapped in markers (the in-place editing ranges after the refactoring).
pub(crate) fn marked(name: &str) -> String {
    format!("{NAME_START}{name}{NAME_END}")
}

/// A byte range.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn of(n: Node) -> Self {
        Self::new(n.start_byte(), n.end_byte())
    }

    pub fn contains(&self, other: Span) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    pub fn overlaps(&self, other: Span) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// The edits of another file (call sites, usages), byte offsets into its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEdits {
    pub path: String,
    pub edits: Vec<TextEdit>,
}

/// The result of a refactoring: edits of the current file (bytes), of other files, the
/// problems to confirm first ("Problems Detected") and where the new name ended up.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub edits: Vec<TextEdit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub other_files: Vec<FileEdits>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<RenameConflict>,
    /// The new name's occurrences in the current file after the edit (in-place editing).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub name_spans: Vec<Span>,
}

/// A usage found by Find Usages: the file and the byte range of the name there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageSpan {
    pub path: String,
    pub span: Span,
}

/// Strip the name markers from the edits and find the marked names in the edited text.
pub(crate) fn finish(src: &str, mut edits: Vec<TextEdit>) -> (Vec<TextEdit>, Vec<Span>) {
    edits.sort_by_key(|e| (e.start, e.end));
    let mut spans = Vec::new();
    // Offsets in the new text: the text before each edit shifts by the earlier edits.
    let mut delta: isize = 0;
    for e in &mut edits {
        let base = (e.start as isize + delta) as usize;
        let mut clean = String::with_capacity(e.new_text.len());
        let mut open = None;
        for ch in e.new_text.chars() {
            match ch {
                NAME_START => open = Some(base + clean.len()),
                NAME_END => {
                    if let Some(s) = open.take() {
                        spans.push(Span::new(s, base + clean.len()));
                    }
                }
                c => clean.push(c),
            }
        }
        delta += clean.len() as isize - (e.end - e.start) as isize;
        e.new_text = clean;
    }
    let _ = src;
    (edits, spans)
}

pub(crate) fn parse(src: &str) -> Option<Tree> {
    parse_tree(&mut new_parser(), src)
}

pub(crate) fn text<'a>(n: Node, src: &'a str) -> &'a str {
    &src[n.byte_range()]
}

/// 0-based line of a byte offset.
pub(crate) fn line_of(src: &str, offset: usize) -> u32 {
    src[..offset.min(src.len())].matches('\n').count() as u32
}

/// A conflict at a place in a file.
pub(crate) fn conflict_at(message: String, path: &str, src: &str, offset: usize) -> RenameConflict {
    RenameConflict::at(message, path, Some(line_of(src, offset)))
}

/// Node kinds that are expressions (what can be extracted).
pub(crate) fn is_expression(n: Node) -> bool {
    match n.kind() {
        "binary_expression"
        | "unary_op_expression"
        | "parenthesized_expression"
        | "member_access_expression"
        | "nullsafe_member_access_expression"
        | "member_call_expression"
        | "nullsafe_member_call_expression"
        | "scoped_call_expression"
        | "scoped_property_access_expression"
        | "class_constant_access_expression"
        | "function_call_expression"
        | "object_creation_expression"
        | "array_creation_expression"
        | "encapsed_string"
        | "string"
        | "integer"
        | "float"
        | "boolean"
        | "null"
        | "conditional_expression"
        | "match_expression"
        | "cast_expression"
        | "subscript_expression"
        | "heredoc"
        | "nowdoc"
        | "clone_expression"
        | "anonymous_function"
        | "arrow_function"
        | "print_intrinsic"
        | "error_suppression_expression"
        | "shell_command_expression"
        | "include_expression"
        | "include_once_expression"
        | "require_expression"
        | "require_once_expression" => true,
        "variable_name" => is_variable_expression(n),
        "name" | "qualified_name" => is_constant_reference(n),
        _ => false,
    }
}

/// A variable that is read as a value (not an assignment target, parameter, property name).
fn is_variable_expression(n: Node) -> bool {
    let Some(p) = n.parent() else { return false };
    match p.kind() {
        "assignment_expression"
        | "reference_assignment_expression"
        | "augmented_assignment_expression" => p.child_by_field_name("left") != Some(n),
        "update_expression"
        | "list_literal"
        | "simple_parameter"
        | "variadic_parameter"
        | "property_promotion_parameter"
        | "property_element"
        | "catch_clause"
        | "global_declaration"
        | "static_variable_declaration"
        | "unset_statement"
        | "anonymous_function_use_clause"
        | "by_ref"
        | "pair"
        | "dynamic_variable_name" => false,
        "scoped_property_access_expression" | "member_access_expression" => {
            p.child_by_field_name("name") != Some(n)
        }
        "foreach_statement" => !after_as(p, n),
        _ => true,
    }
}

/// `n` comes after `as` in a `foreach`.
pub(crate) fn after_as(foreach: Node, n: Node) -> bool {
    let mut c = foreach.walk();
    let as_kw = foreach.children(&mut c).find(|ch| ch.kind() == "as");
    as_kw.is_some_and(|a| n.start_byte() >= a.end_byte())
}

/// A bare name used as a constant (`FOO`, `\App\FOO`).
fn is_constant_reference(n: Node) -> bool {
    let Some(p) = n.parent() else { return false };
    match p.kind() {
        "binary_expression"
        | "argument"
        | "return_statement"
        | "array_element_initializer"
        | "parenthesized_expression"
        | "conditional_expression"
        | "echo_statement"
        | "unary_op_expression" => true,
        "assignment_expression" => p.child_by_field_name("right") == Some(n),
        _ => false,
    }
}

/// Function-like scopes: their bodies have their own variables.
pub(crate) fn is_function_like(n: Node) -> bool {
    matches!(
        n.kind(),
        "method_declaration" | "function_definition" | "anonymous_function" | "arrow_function"
    )
}

/// Declarations whose bodies are not part of the enclosing code at all.
pub(crate) fn is_nested_declaration(n: Node) -> bool {
    matches!(
        n.kind(),
        "method_declaration"
            | "function_definition"
            | "class_declaration"
            | "interface_declaration"
            | "trait_declaration"
            | "enum_declaration"
            | "anonymous_class"
    )
}

/// Nodes whose children are statements.
pub(crate) fn is_statement_list(n: Node) -> bool {
    matches!(
        n.kind(),
        "program" | "compound_statement" | "colon_block" | "case_statement" | "default_statement"
    )
}

/// Statements: children of statement lists, and the bodies of control structures.
pub(crate) fn is_statement(n: Node) -> bool {
    if !n.is_named()
        || matches!(
            n.kind(),
            "php_tag" | "text_interpolation" | "comment" | "text"
        )
    {
        return false;
    }
    n.parent().is_some_and(is_statement_list)
        || n.kind().ends_with("_statement")
        || matches!(
            n.kind(),
            "global_declaration" | "function_static_declaration"
        )
}

/// The innermost function-like node around `n` (not `n` itself).
pub(crate) fn enclosing_function(n: Node) -> Option<Node> {
    let mut cur = n.parent();
    while let Some(p) = cur {
        if is_function_like(p) {
            return Some(p);
        }
        cur = p.parent();
    }
    None
}

/// The innermost method or named function around `n` (closures skipped).
pub(crate) fn enclosing_named_function(n: Node) -> Option<Node> {
    let mut cur = n.parent();
    while let Some(p) = cur {
        if matches!(p.kind(), "method_declaration" | "function_definition") {
            return Some(p);
        }
        cur = p.parent();
    }
    None
}

/// The class-like declaration around `n`.
pub(crate) fn enclosing_class(n: Node) -> Option<Node> {
    let mut cur = n.parent();
    while let Some(p) = cur {
        match p.kind() {
            "class_declaration"
            | "trait_declaration"
            | "enum_declaration"
            | "interface_declaration" => return Some(p),
            "anonymous_class" => return None,
            _ => {}
        }
        cur = p.parent();
    }
    None
}

/// The body that holds the code of a scope: a function's body, or the whole file.
pub(crate) fn scope_body<'t>(function: Option<Node<'t>>, root: Node<'t>) -> Node<'t> {
    function
        .and_then(|f| f.child_by_field_name("body"))
        .unwrap_or(root)
}

/// The method has `static`.
pub(crate) fn is_static_method(f: Node) -> bool {
    let mut c = f.walk();
    let found = f.children(&mut c).any(|ch| ch.kind() == "static_modifier");
    found
}

/// Leaf tokens (comments skipped), for comparing code structurally.
pub(crate) fn tokens<'a>(n: Node, src: &'a str) -> Vec<&'a str> {
    let mut out = Vec::new();
    fn walk<'a>(n: Node, src: &'a str, out: &mut Vec<&'a str>) {
        if n.kind() == "comment" {
            return;
        }
        if n.child_count() == 0 {
            out.push(&src[n.byte_range()]);
            return;
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            walk(ch, src, out);
        }
    }
    walk(n, src, &mut out);
    out
}

/// Two expressions are the same code (ignoring whitespace and comments).
pub(crate) fn same_code(a: Node, b: Node, src: &str) -> bool {
    a.kind() == b.kind() && tokens(a, src) == tokens(b, src)
}

/// Every node in `root` (pre-order) for which `keep` holds, not descending into nested
/// functions and classes unless `into_functions`.
pub(crate) fn collect_nodes<'t>(
    root: Node<'t>,
    into_functions: bool,
    keep: &mut dyn FnMut(Node<'t>) -> bool,
) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    fn walk<'t>(
        n: Node<'t>,
        root: Node<'t>,
        into: bool,
        keep: &mut dyn FnMut(Node<'t>) -> bool,
        out: &mut Vec<Node<'t>>,
    ) {
        if n != root && !into && (is_function_like(n) || is_nested_declaration(n)) {
            return;
        }
        if keep(n) {
            out.push(n);
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            walk(ch, root, into, keep, out);
        }
    }
    walk(root, root, into_functions, keep, &mut out);
    out
}

/// One indentation level of the file (tabs or the smallest space indent).
pub(crate) fn indent_unit(src: &str) -> String {
    let mut tabs = 0;
    let mut min_spaces = usize::MAX;
    for line in src.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with('\t') {
            tabs += 1;
        } else {
            let n = line.chars().take_while(|c| *c == ' ').count();
            if n >= 2 && !line.trim_start().starts_with('*') {
                min_spaces = min_spaces.min(n);
            }
        }
    }
    if tabs > 0 && min_spaces == usize::MAX {
        "\t".into()
    } else if min_spaces != usize::MAX && min_spaces <= 8 {
        " ".repeat(min_spaces)
    } else {
        "    ".into()
    }
}

pub(crate) use crate::generate::{leading_ws, line_start};

/// The line break of the file.
pub(crate) fn eol(src: &str) -> &'static str {
    if src.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// `text`'s lines re-indented: the indentation `from` is removed from every line after the
/// first and `to` put in its place (the first line goes where the caller puts it).
pub(crate) fn reindent(text: &str, from: &str, to: &str) -> String {
    let mut out = String::new();
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
            if line.trim().is_empty() {
                continue;
            }
            out.push_str(to);
            out.push_str(line.strip_prefix(from).unwrap_or(line.trim_start()));
        } else {
            out.push_str(line);
        }
    }
    out
}

/// The node at `start..end`: the outermost node with exactly that range.
pub(crate) fn node_with_range(root: Node<'_>, start: usize, end: usize) -> Option<Node<'_>> {
    let mut n = root.descendant_for_byte_range(start, end)?;
    if n.start_byte() != start || n.end_byte() != end {
        return None;
    }
    while let Some(p) = n.parent() {
        if p.start_byte() == start && p.end_byte() == end {
            n = p;
        } else {
            break;
        }
    }
    Some(n)
}

/// The expression with exactly this range (whitespace and a trailing `;` trimmed), going
/// down through same-range wrappers to the first expression.
pub(crate) fn expression_at<'t>(
    root: Node<'t>,
    src: &str,
    start: usize,
    end: usize,
) -> Option<Node<'t>> {
    let (start, end) = trim_range(src, start, end);
    let mut n = node_with_range(root, start, end)?;
    loop {
        if is_expression(n) {
            return Some(n);
        }
        // `foo();` selected without the `;` is the call inside the expression statement.
        let mut c = n.walk();
        let inner = n
            .named_children(&mut c)
            .find(|ch| ch.start_byte() == start && ch.end_byte() == end)?;
        n = inner;
    }
}

/// `start..end` without surrounding whitespace and a trailing `;`.
pub(crate) fn trim_range(src: &str, start: usize, end: usize) -> (usize, usize) {
    let s = &src[start..end];
    let lead = s.len() - s.trim_start().len();
    let mut t = s.trim();
    if let Some(stripped) = t.strip_suffix(';') {
        t = stripped.trim_end();
    }
    (start + lead, start + lead + t.len())
}

/// The candidates for an expression at the caret, innermost first (PHPStorm's
/// "Expressions" popup): every enclosing expression up to the statement.
pub(crate) fn expressions_at_caret<'t>(root: Node<'t>, src: &str, pos: usize) -> Vec<Node<'t>> {
    let mut out: Vec<Node<'t>> = Vec::new();
    let mut seeds = vec![pos];
    if pos > 0 {
        seeds.push(pos - 1);
    }
    for at in seeds {
        let Some(mut n) = root.descendant_for_byte_range(at, at) else {
            continue;
        };
        loop {
            if is_statement(n) || is_function_like(n) && !out.is_empty() {
                break;
            }
            if is_expression(n)
                && !(n.kind() == "variable_name" && text(n, src) == "$this")
                && !out.iter().any(|o| o.byte_range() == n.byte_range())
                && !is_condition_parens(n)
            {
                out.push(n);
            }
            if matches!(
                n.kind(),
                "assignment_expression" | "augmented_assignment_expression"
            ) {
                break;
            }
            match n.parent() {
                Some(p) => n = p,
                None => break,
            }
        }
        if !out.is_empty() {
            break;
        }
    }
    out
}

/// The parentheses of `if (...)`, `while (...)`: syntax, not an expression of its own.
fn is_condition_parens(n: Node) -> bool {
    n.kind() == "parenthesized_expression"
        && n.parent().is_some_and(|p| {
            matches!(
                p.kind(),
                "if_statement"
                    | "else_if_clause"
                    | "while_statement"
                    | "do_statement"
                    | "switch_statement"
            ) && p.child_by_field_name("condition") == Some(n)
        })
}

/// One line of an expression for choosers and titles (`foo(...)` style shortening).
pub fn one_line(code: &str) -> String {
    let joined = code.split_whitespace().collect::<Vec<_>>().join(" ");
    let joined = joined.replace("( ", "(").replace(" )", ")");
    if joined.chars().count() > 80 {
        let cut: String = joined.chars().take(77).collect();
        format!("{cut}...")
    } else {
        joined
    }
}

/// Whether evaluating the expression can change state (calls, assignments, `new`, ...).
pub(crate) fn has_side_effects(n: Node) -> bool {
    if matches!(
        n.kind(),
        "function_call_expression"
            | "member_call_expression"
            | "nullsafe_member_call_expression"
            | "scoped_call_expression"
            | "object_creation_expression"
            | "assignment_expression"
            | "reference_assignment_expression"
            | "augmented_assignment_expression"
            | "update_expression"
            | "include_expression"
            | "include_once_expression"
            | "require_expression"
            | "require_once_expression"
            | "print_intrinsic"
            | "yield_expression"
            | "shell_command_expression"
            | "throw_expression"
    ) {
        return true;
    }
    if matches!(n.kind(), "anonymous_function" | "arrow_function") {
        return false;
    }
    let mut c = n.walk();
    let any = n.named_children(&mut c).any(has_side_effects);
    any
}

/// The context of `at` needs an operand, so a non-primary expression put there needs
/// parentheses (`$a * <x>`, `<x>->foo()`, `!<x>`).
pub(crate) fn needs_parens(at: Node, replacement_kind: &str) -> bool {
    let primary = matches!(
        replacement_kind,
        "variable_name"
            | "member_access_expression"
            | "nullsafe_member_access_expression"
            | "member_call_expression"
            | "nullsafe_member_call_expression"
            | "scoped_call_expression"
            | "scoped_property_access_expression"
            | "class_constant_access_expression"
            | "function_call_expression"
            | "array_creation_expression"
            | "encapsed_string"
            | "string"
            | "integer"
            | "float"
            | "boolean"
            | "null"
            | "parenthesized_expression"
            | "subscript_expression"
            | "name"
            | "qualified_name"
            | "match_expression"
    );
    if primary {
        return false;
    }
    let Some(p) = at.parent() else { return false };
    match p.kind() {
        "binary_expression" | "unary_op_expression" | "cast_expression" | "clone_expression" => {
            true
        }
        "conditional_expression" => true,
        "member_access_expression"
        | "nullsafe_member_access_expression"
        | "member_call_expression"
        | "nullsafe_member_call_expression"
        | "subscript_expression" => {
            p.child_by_field_name("object") == Some(at) || {
                let first = p.named_child(0);
                first == Some(at)
            }
        }
        "scoped_call_expression"
        | "scoped_property_access_expression"
        | "class_constant_access_expression" => p.named_child(0) == Some(at),
        _ => false,
    }
}

/// `code` for the place of `at`, in parentheses when the context needs them.
pub(crate) fn fit(at: Node, code: &str, kind: &str) -> String {
    if !needs_parens(at, kind) {
        return code.to_string();
    }
    // A binary operator that binds tighter than the one around it needs none.
    if kind == "binary_expression" {
        if let Some(parent) = at.parent().filter(|p| p.kind() == "binary_expression") {
            let wrapped = format!("<?php {code};");
            let inner_op = parse(&wrapped).and_then(|t| {
                let stmt = t.root_node().named_child(1)?;
                let e = stmt.named_child(0)?;
                (e.kind() == "binary_expression" && e.byte_range() == (6..6 + code.len()))
                    .then(|| e.child_by_field_name("operator"))
                    .flatten()
                    .map(|o| wrapped[o.byte_range()].to_string())
            });
            let outer_op = parent.child_by_field_name("operator");
            if let (Some(inner), Some(outer)) = (inner_op, outer_op) {
                let (pi, po) = (precedence(&inner), precedence(outer.kind()));
                let right = parent.child_by_field_name("right") == Some(at);
                let right_assoc = matches!(outer.kind(), "**" | "??");
                let fine = pi > po || (pi == po && (right_assoc == right) && inner == outer.kind());
                if fine {
                    return code.to_string();
                }
            }
        }
    }
    format!("({code})")
}

/// Binary operator precedence (PHP 8: `+`/`-` bind tighter than `.`).
fn precedence(op: &str) -> u8 {
    match op.to_ascii_lowercase().as_str() {
        "**" => 17,
        "instanceof" => 16,
        "*" | "/" | "%" => 14,
        "+" | "-" => 13,
        "<<" | ">>" => 12,
        "." => 11,
        "<" | "<=" | ">" | ">=" => 10,
        "==" | "!=" | "===" | "!==" | "<>" | "<=>" => 9,
        "&" => 8,
        "^" => 7,
        "|" => 6,
        "&&" => 5,
        "||" => 4,
        "??" => 3,
        "and" => 2,
        "xor" => 1,
        _ => 0,
    }
}

/// The statement that holds `n` directly inside a statement list.
pub(crate) fn statement_of(n: Node) -> Option<Node> {
    let mut cur = n;
    loop {
        let p = cur.parent()?;
        if is_statement_list(p) {
            return Some(cur);
        }
        if is_function_like(p) || is_nested_declaration(p) {
            return None;
        }
        cur = p;
    }
}

/// The range to delete for a whole statement: its line(s) when it is alone on them.
pub(crate) fn statement_removal(src: &str, s: Span) -> Span {
    let ls = line_start(src, s.start);
    let line_end = src[s.end..].find('\n').map_or(src.len(), |i| s.end + i + 1);
    let before_alone = src[ls..s.start].trim().is_empty();
    let after_alone = src[s.end..line_end].trim().is_empty();
    if before_alone && after_alone {
        Span::new(ls, line_end)
    } else {
        Span::new(s.start, s.end)
    }
}

/// `camelCase` / `snake_case` / `kebab` words of a name.
pub(crate) fn words(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' || c == '-' || c == ' ' || !c.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }
        let boundary = c.is_uppercase()
            && !cur.is_empty()
            && (chars[i - 1].is_lowercase()
                || chars[i - 1].is_ascii_digit()
                || chars.get(i + 1).is_some_and(|n| n.is_lowercase()));
        if boundary {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.into_iter().map(|w| w.to_lowercase()).collect()
}

/// `["user", "name"]` -> `userName`.
pub(crate) fn camel(words: &[String]) -> String {
    let mut out = String::new();
    for (i, w) in words.iter().enumerate() {
        if i == 0 {
            out.push_str(w);
        } else {
            let mut cs = w.chars();
            if let Some(f) = cs.next() {
                out.extend(f.to_uppercase());
                out.push_str(cs.as_str());
            }
        }
    }
    out
}

/// `userName` -> `USER_NAME`.
pub fn constant_case(name: &str) -> String {
    words(name)
        .iter()
        .map(|w| w.to_uppercase())
        .collect::<Vec<_>>()
        .join("_")
}

/// A valid PHP identifier.
pub(crate) fn is_identifier(name: &str) -> bool {
    let mut cs = name.chars();
    matches!(cs.next(), Some(c) if c == '_' || c.is_alphabetic())
        && cs.all(|c| c == '_' || c.is_alphanumeric())
}

/// Checks a new name; the message is PHPStorm's.
pub(crate) fn check_name(name: &str) -> Result<(), String> {
    if is_identifier(name.trim_start_matches('$')) {
        Ok(())
    } else {
        Err(format!("'{name}' is not a valid identifier"))
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use crate::generate::{apply_edits, TextEdit};

    /// Source with `‹` / `›` marking a selection (or `‸` a caret); returns the clean source
    /// and the byte range.
    pub fn sel(marked: &str) -> (String, usize, usize) {
        if let Some(caret) = marked.find('‸') {
            let src = marked.replacen('‸', "", 1);
            return (src, caret, caret);
        }
        let s = marked.find('‹').expect("selection start");
        let without = marked.replacen('‹', "", 1);
        let e = without.find('›').expect("selection end");
        (without.replacen('›', "", 1), s, e)
    }

    pub fn apply(src: &str, edits: &[TextEdit]) -> String {
        apply_edits(src, edits)
    }
}
