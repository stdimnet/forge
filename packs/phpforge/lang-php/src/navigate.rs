//! Code navigation on the PHP index, used when no language server answers (none running, or
//! one without implementation / type definition): the symbol under the
//! caret, Go to Declaration, Find Usages, Go to Implementation, Go to Super Method, Go to
//! Type Declaration and the implemented / overridden gutter markers.
//!
//! Symbols the index does not declare (PHP's own classes, functions and constants such as
//! `\Throwable` or `array_map`) come back as an [`ExternalSymbol`]: the caller looks them up
//! in the PHP stubs ([`crate::stubs`]).
//!
//! Offsets inside are UTF-8 bytes; positions handed out are LSP positions (0-based line,
//! UTF-16 character), like the language servers' answers they stand in for.

use std::collections::{HashMap, HashSet};

use forge_codeintel::{
    classify_usage, ElementKind, Position, Range, UsageKind, UsageLocation, UsageOptions,
};
use serde::{Deserialize, Serialize};

use crate::hover::{find_member, word_at};
use crate::index::ProjectIndex;
use crate::model::{short_name, ClassLike, FileIndex, Member, SymbolKind, UseKind};
use crate::parser;
use crate::resolve::{is_builtin_type, resolve_class_name};

/// A project symbol found under the caret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SymbolRef {
    Class {
        fqn: String,
    },
    /// A member, identified by the class that declares it.
    Member {
        class: String,
        member: SymbolKind,
        name: String,
    },
    Function {
        fqn: String,
    },
    /// A namespace constant (`const VERSION = 2;`).
    Constant {
        fqn: String,
    },
    /// A local variable or parameter, identified by its first occurrence in its scope.
    Variable {
        name: String,
        /// Byte offset of the first occurrence (the parameter or first assignment).
        declared_at: usize,
    },
}

impl SymbolRef {
    /// The text that has to appear where the symbol is used.
    pub fn search_word(&self) -> &str {
        match self {
            SymbolRef::Class { fqn }
            | SymbolRef::Function { fqn }
            | SymbolRef::Constant { fqn } => short_name(fqn),
            SymbolRef::Member { name, .. } | SymbolRef::Variable { name, .. } => name,
        }
    }

    /// How PHPStorm names it in titles: `User`, `User::orders()`, `User::$email`, `money()`.
    pub fn display(&self) -> String {
        match self {
            SymbolRef::Class { fqn } | SymbolRef::Constant { fqn } => short_name(fqn).to_string(),
            SymbolRef::Function { fqn } => format!("{}()", short_name(fqn)),
            SymbolRef::Variable { name, .. } => name.clone(),
            SymbolRef::Member {
                class,
                member,
                name,
            } => match member {
                SymbolKind::Method => format!("{}::{name}()", short_name(class)),
                SymbolKind::Property => format!("{}::${name}", short_name(class)),
                _ => format!("{}::{name}", short_name(class)),
            },
        }
    }

    /// Lowercase kind for the UI (`class`, `method`, `property`, `constant`, `function`).
    pub fn kind_name(&self) -> &'static str {
        match self {
            SymbolRef::Class { .. } => "class",
            SymbolRef::Function { .. } => "function",
            SymbolRef::Constant { .. } => "constant",
            SymbolRef::Variable { .. } => "variable",
            SymbolRef::Member { member, .. } => match member {
                SymbolKind::Method => "method",
                SymbolKind::Property => "property",
                SymbolKind::EnumCase => "enumCase",
                _ => "constant",
            },
        }
    }

    fn same(&self, other: &SymbolRef) -> bool {
        match (self, other) {
            (SymbolRef::Class { fqn: a }, SymbolRef::Class { fqn: b })
            | (SymbolRef::Function { fqn: a }, SymbolRef::Function { fqn: b }) => {
                a.eq_ignore_ascii_case(b)
            }
            (SymbolRef::Constant { fqn: a }, SymbolRef::Constant { fqn: b }) => a == b,
            (
                SymbolRef::Variable {
                    name: a,
                    declared_at: x,
                },
                SymbolRef::Variable {
                    name: b,
                    declared_at: y,
                },
            ) => a == b && x == y,
            (
                SymbolRef::Member {
                    class: c1,
                    member: k1,
                    name: n1,
                },
                SymbolRef::Member {
                    class: c2,
                    member: k2,
                    name: n2,
                },
            ) => {
                k1 == k2
                    && c1.eq_ignore_ascii_case(c2)
                    && if *k1 == SymbolKind::Method {
                        n1.eq_ignore_ascii_case(n2)
                    } else {
                        n1 == n2
                    }
            }
            _ => false,
        }
    }
}

/// A symbol the project index does not declare: PHP's own classes, functions and
/// constants (`\Throwable`, `array_map`, `PHP_EOL`) or library code that is not indexed.
/// Candidates are fully qualified names without a leading backslash, most likely first;
/// the caller looks them up in the PHP stubs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExternalSymbol {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub constants: Vec<String>,
    /// A member of one of `classes` (or of their ancestors).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<ExternalMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalMember {
    pub kind: SymbolKind,
    pub name: String,
}

impl ExternalSymbol {
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty() && self.functions.is_empty() && self.constants.is_empty()
    }

    /// The name to show while the lookup runs: `Throwable`, `Exception::getMessage()`.
    pub fn display(&self) -> String {
        match (&self.member, self.classes.first()) {
            (Some(m), Some(c)) => match m.kind {
                SymbolKind::Method => format!("{}::{}()", short_name(c), m.name),
                SymbolKind::Property => format!("{}::${}", short_name(c), m.name),
                _ => format!("{}::{}", short_name(c), m.name),
            },
            (None, Some(c)) => short_name(c).to_string(),
            _ => self
                .functions
                .last()
                .map(|f| format!("{}()", short_name(f)))
                .or_else(|| self.constants.last().map(|c| short_name(c).to_string()))
                .unwrap_or_default(),
        }
    }
}

/// What a word in the source refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Resolved {
    Symbol(SymbolRef),
    /// The declaration of the symbol itself.
    Declaration(SymbolRef),
    /// A member accessed on an expression the index cannot type (`$x->save()`).
    Dynamic {
        kind: SymbolKind,
        name: String,
    },
    External(ExternalSymbol),
    Nothing,
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\\' || c == '$'
}

pub(crate) fn ident_start(src: &str, end: usize) -> usize {
    src[..end]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_ident(*c))
        .last()
        .map(|(i, _)| i)
        .unwrap_or(end)
}

/// Words that are never navigable symbols.
const KEYWORDS: &[&str] = &[
    "abstract",
    "and",
    "array",
    "as",
    "break",
    "case",
    "catch",
    "class",
    "clone",
    "const",
    "continue",
    "declare",
    "default",
    "die",
    "do",
    "echo",
    "else",
    "elseif",
    "empty",
    "enddeclare",
    "endfor",
    "endforeach",
    "endif",
    "endswitch",
    "endwhile",
    "enum",
    "eval",
    "exit",
    "extends",
    "final",
    "finally",
    "fn",
    "for",
    "foreach",
    "function",
    "global",
    "goto",
    "if",
    "implements",
    "include",
    "include_once",
    "instanceof",
    "insteadof",
    "interface",
    "isset",
    "list",
    "match",
    "namespace",
    "new",
    "or",
    "print",
    "private",
    "protected",
    "public",
    "readonly",
    "require",
    "require_once",
    "return",
    "switch",
    "throw",
    "trait",
    "try",
    "unset",
    "use",
    "var",
    "while",
    "xor",
    "yield",
];

pub(crate) fn is_keyword(word: &str) -> bool {
    KEYWORDS.iter().any(|k| k.eq_ignore_ascii_case(word))
}

/// Class names inside a type as written (`?Order`, `Collection<int, Order>`, `A|B`,
/// `Order[]`), resolved in `file` at `offset`; builtin types are left out and `self` /
/// `static` / `$this` mean `owner`.
pub(crate) fn type_classes(
    ty: &str,
    file: &FileIndex,
    offset: usize,
    owner: Option<&str>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in ty.split(['|', '&', '<', '>', ',', '(', ')', ' ']) {
        let name = part.trim().trim_start_matches('?').trim_end_matches("[]");
        if name.is_empty() || (name.starts_with('$') && name != "$this") {
            continue;
        }
        let fqn = match name.to_ascii_lowercase().as_str() {
            "self" | "static" | "$this" => match owner {
                Some(o) => o.to_string(),
                None => continue,
            },
            n if is_builtin_type(n) || n == "mixed" || n == "void" || n == "never" => continue,
            _ => file.resolve_class(name, offset),
        };
        if !out.iter().any(|o| o.eq_ignore_ascii_case(&fqn)) {
            out.push(fqn);
        }
    }
    out
}

/// The file declaring `class` (for resolving its member types), the given one if it is
/// declared there.
fn declaring_file(index: &ProjectIndex, file: &FileIndex, class: &str) -> Option<FileIndex> {
    if file
        .classes
        .iter()
        .any(|c| c.fqn.eq_ignore_ascii_case(class))
    {
        return Some(file.clone());
    }
    index.class_path(class).and_then(|p| index.file(p)).cloned()
}

/// Classes of a member's type (property type or method return type).
fn member_type_classes(
    index: &ProjectIndex,
    file: &FileIndex,
    owner: &ClassLike,
    m: &Member,
) -> Vec<String> {
    let Some(ty) = m.effective_type() else {
        return vec![];
    };
    let ctx = declaring_file(index, file, &owner.fqn).unwrap_or_else(|| file.clone());
    type_classes(&ty, &ctx, m.range.start, Some(&owner.fqn))
}

/// A class of the current file (its unsaved state) or of the index.
pub(crate) fn find_class(
    index: &mut ProjectIndex,
    file: &FileIndex,
    fqn: &str,
) -> Option<ClassLike> {
    let fqn = fqn.trim_start_matches('\\');
    file.classes
        .iter()
        .find(|c| c.fqn.eq_ignore_ascii_case(fqn))
        .cloned()
        .or_else(|| index.resolve_class(fqn))
}

/// The method or function whose range contains `offset`: its params and range.
pub(crate) fn enclosing_callable(
    file: &FileIndex,
    offset: usize,
) -> Option<(Vec<crate::model::Param>, usize, usize)> {
    if let Some(m) = file.class_at(offset).and_then(|c| {
        c.members
            .iter()
            .find(|m| m.kind == SymbolKind::Method && m.range.contains(offset))
    }) {
        return Some((m.params.clone(), m.range.start, m.range.end));
    }
    file.functions
        .iter()
        .find(|f| f.range.contains(offset))
        .map(|f| (f.params.clone(), f.range.start, f.range.end))
}

/// The class type of a local variable at `offset`: a typed parameter, `$x = new X(...)`,
/// `catch (X $x)` or an inline `/** @var X $x */` earlier in the same method or function.
pub(crate) fn variable_classes(
    file: &FileIndex,
    src: &str,
    offset: usize,
    var: &str,
) -> Vec<String> {
    let name = var.trim_start_matches('$');
    let owner = file.class_at(offset).map(|c| c.fqn.clone());
    if name == "this" {
        return owner.into_iter().collect();
    }
    let Some((params, start, _)) = enclosing_callable(file, offset) else {
        return vec![];
    };
    if let Some(ty) = params
        .iter()
        .find(|p| p.name == name)
        .and_then(|p| p.type_hint.clone())
    {
        return type_classes(&ty, file, offset, owner.as_deref());
    }
    let body = &src[start.min(offset)..offset];
    let assign = format!("${name} = new ");
    if let Some(i) = body.rfind(&assign) {
        let rest = &body[i + assign.len()..];
        let class: String = rest
            .chars()
            .take_while(|c| is_ident(*c) && *c != '$')
            .collect();
        if !class.is_empty() {
            return type_classes(&class, file, start + i, owner.as_deref());
        }
    }
    // `catch (FirstException | \Throwable $e)`.
    let var_name = format!("${name}");
    let mut search = body;
    while let Some(i) = search.rfind("catch") {
        let rest = body[i + 5..].trim_start();
        if let Some(inner) = rest.strip_prefix('(').and_then(|r| r.split(')').next()) {
            if let Some(ty) = inner.trim().strip_suffix(&var_name) {
                if !ty.trim().is_empty() {
                    return type_classes(ty.trim(), file, start + i, owner.as_deref());
                }
            }
        }
        search = &body[..i];
    }
    let var_tag = format!(" ${name}");
    for line in body.lines().rev() {
        if let Some(rest) = line
            .trim()
            .strip_prefix("/** @var ")
            .or_else(|| line.trim().strip_prefix("* @var "))
        {
            if let Some(ty) = rest.trim_end_matches("*/").trim().strip_suffix(&var_tag) {
                return type_classes(ty, file, start, owner.as_deref());
            }
        }
    }
    vec![]
}

/// Byte index of the `(` matching the `)` at `close`.
pub(crate) fn matching_open(src: &str, close: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut depth = 0i32;
    let mut i = close + 1;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b')' => depth += 1,
            b'(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'\n' if close - i > 4000 => return None,
            _ => {}
        }
    }
    None
}

/// The `(byte end of receiver, operator)` when `before` ends with `->`, `?->` or `::`.
pub(crate) fn access_before(before: &str) -> Option<(usize, &'static str)> {
    if let Some(b) = before.strip_suffix("?->") {
        Some((b.len(), "->"))
    } else if let Some(b) = before.strip_suffix("->") {
        Some((b.len(), "->"))
    } else {
        before.strip_suffix("::").map(|b| (b.len(), "::"))
    }
}

/// Byte ranges of the top-level arguments between the parentheses at `open` and `close`.
pub(crate) fn split_call_args(src: &str, open: usize, close: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if close <= open + 1 {
        return out;
    }
    let inner = &src[open + 1..close];
    let mut pos = open + 1;
    for part in crate::attributes::split_top(inner, ',') {
        let lead = part.len() - part.trim_start().len();
        let start = pos + lead;
        let end = pos + part.trim_end().len();
        if end > start {
            out.push((start, end));
        }
        pos += part.len() + 1;
    }
    out
}

/// The value of an argument expression as metadata needs it: a string literal or
/// `Foo::class`. Named arguments (`id: 'x'`) count by position.
pub(crate) fn arg_value(
    file: &FileIndex,
    src: &str,
    range: (usize, usize),
) -> crate::meta::ArgValue {
    use crate::meta::ArgValue;
    let text = src[range.0..range.1].trim();
    let text = match text.split_once(':') {
        Some((name, rest))
            if !name.contains("::")
                && !rest.starts_with(':')
                && name.trim().chars().all(|c| c.is_alphanumeric() || c == '_') =>
        {
            rest.trim()
        }
        _ => text,
    };
    if let Some(s) = crate::attributes::string_literal(text) {
        return ArgValue::Str(s);
    }
    if let Some(class) = text.strip_suffix("::class") {
        let class = class.trim();
        if !class.is_empty() && !class.starts_with('$') {
            let fqn = match class.to_ascii_lowercase().as_str() {
                "self" | "static" => match file.class_at(range.0) {
                    Some(c) => c.fqn.clone(),
                    None => return ArgValue::Unknown,
                },
                _ => file.resolve_class(class, range.0),
            };
            return ArgValue::Class(fqn);
        }
    }
    ArgValue::Unknown
}

/// The classes a metadata return type stands for at a call with `args`.
fn meta_type_classes(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    args: &[(usize, usize)],
    t: crate::meta::MetaType,
    depth: u8,
) -> Vec<String> {
    use crate::meta::MetaType;
    match t {
        MetaType::Named(ty) => type_classes(&ty, file, 0, None),
        MetaType::ArgumentType(n) | MetaType::ArgumentElementType(n) => match args.get(n) {
            Some(&(_, end)) => receiver_types(index, file, src, end, depth + 1),
            None => vec![],
        },
    }
}

/// The class and its ancestors' names (metadata targets of methods apply to subclasses and
/// implementations).
pub(crate) fn class_chain(index: &mut ProjectIndex, file: &FileIndex, fqn: &str) -> Vec<String> {
    let mut out = vec![fqn.trim_start_matches('\\').to_string()];
    if let Some(class) = find_class(index, file, fqn) {
        for a in index.ancestors(&class) {
            out.push(a.fqn);
        }
        for n in class.extends.iter().chain(&class.implements) {
            if !out.iter().any(|o| o.eq_ignore_ascii_case(n)) {
                out.push(n.clone());
            }
        }
    }
    out
}

/// The function a call resolves to (`fqn`), from the index or as the first candidate.
pub(crate) fn called_function(
    index: &ProjectIndex,
    file: &FileIndex,
    name: &str,
    at: usize,
) -> Vec<String> {
    let cands = function_candidates(file, name, at);
    match find_function(index, file, &cands) {
        Some(f) => vec![f],
        None => cands,
    }
}

/// The end (the `;`) of the expression last assigned to `var` before `end` in its scope:
/// `$order = $container->get(Order::class);`.
pub(crate) fn last_assignment_end(
    file: &FileIndex,
    src: &str,
    end: usize,
    var: &str,
) -> Option<usize> {
    let var_start = end.saturating_sub(var.len());
    let (scope_start, _) = variable_scope(file, src, var_start);
    let scope_start = scope_start.min(var_start);
    let body = &src[scope_start..var_start];
    let bytes = body.as_bytes();
    let mut search_end = body.len();
    while let Some(i) = body[..search_end].rfind(var) {
        search_end = i;
        let after_name = i + var.len();
        // Whole variable name only (`$order` but not `$orders`).
        if body[after_name..]
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            continue;
        }
        let rest = &body[after_name..];
        let trimmed = rest.trim_start();
        let eq = after_name + (rest.len() - trimmed.len());
        if bytes.get(eq) != Some(&b'=') || matches!(bytes.get(eq + 1), Some(b'=') | Some(b'>')) {
            continue;
        }
        // The expression up to the end of the statement.
        let expr_start = scope_start + eq + 1;
        let mut depth = 0i32;
        let mut quote: Option<u8> = None;
        let all = src.as_bytes();
        let mut j = expr_start;
        while j < var_start {
            let c = all[j];
            if let Some(q) = quote {
                if c == b'\\' {
                    j += 1;
                } else if c == q {
                    quote = None;
                }
            } else {
                match c {
                    b'\'' | b'"' => quote = Some(c),
                    b'(' | b'[' | b'{' => depth += 1,
                    b')' | b']' | b'}' => depth -= 1,
                    b';' if depth == 0 => break,
                    _ => {}
                }
            }
            j += 1;
        }
        if j >= var_start || src[expr_start..j].trim().is_empty() {
            return None;
        }
        return Some(j);
    }
    None
}

/// Classes of a variable from its last assignment before `end` in its scope:
/// `$order = $container->get(Order::class);`, `$cache = app('cache');`.
fn assigned_classes(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    end: usize,
    var: &str,
    depth: u8,
) -> Vec<String> {
    match last_assignment_end(file, src, end, var) {
        Some(j) => receiver_types(index, file, src, j, depth + 1),
        None => vec![],
    }
}

/// Fully qualified names of the possible classes of the expression ending at `end` (just
/// before `->` or `::`): variables, `$this`, `self` / `static` / `parent`, class names,
/// property chains and method call chains through declared (or PHPDoc) types, and calls
/// that `.phpstorm.meta.php` overrides.
pub(crate) fn receiver_types(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    end: usize,
    depth: u8,
) -> Vec<String> {
    if depth > 4 {
        return vec![];
    }
    let enclosing = file.class_at(end).cloned();
    // `$user->orders()->first()`, `User::query()->where()`: the call's return type.
    let head = src[..end].trim_end();
    // `$data['customer']->`: the key's type from `#[ArrayShape]`.
    if head.ends_with(']') {
        return crate::callsite::subscript_classes(index, file, src, head.len(), depth);
    }
    if head.ends_with(')') {
        let Some(open) = matching_open(src, head.len() - 1) else {
            return vec![];
        };
        let name_end = src[..open].trim_end().len();
        let name_start = ident_start(src, name_end);
        let name = &src[name_start..name_end];
        if name.is_empty() {
            // `(new Mailer())->send()`: the parenthesized expression.
            return receiver_types(index, file, src, head.len() - 1, depth + 1);
        }
        if name.starts_with('$') {
            return vec![];
        }
        let before = src[..name_start].trim_end();
        let args = split_call_args(src, open, head.len() - 1);
        let arg = |n: usize| match args.get(n) {
            Some(r) => arg_value(file, src, *r),
            None => crate::meta::ArgValue::Unknown,
        };
        // `new Foo(...)`.
        if before
            .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
            .next()
            .is_some_and(|w| w.eq_ignore_ascii_case("new"))
        {
            return vec![file.resolve_class(name, name_start)];
        }
        if let Some((recv_end, _)) = access_before(before) {
            let mut out = Vec::new();
            for owner in receiver_types(index, file, src, recv_end, depth + 1) {
                // `.phpstorm.meta.php` overrides (also on parents and interfaces).
                let chain = class_chain(index, file, &owner);
                if let Some(t) = index.meta().method_return(&chain, name, arg) {
                    for c in meta_type_classes(index, file, src, &args, t, depth) {
                        if !out.iter().any(|o: &String| o.eq_ignore_ascii_case(&c)) {
                            out.push(c);
                        }
                    }
                    continue;
                }
                let Some(class) = find_class(index, file, &owner) else {
                    continue;
                };
                if let Some((decl, m)) = find_member(index, &class, name, &[SymbolKind::Method]) {
                    for t in member_type_classes(index, file, &decl, &m) {
                        // `static` / `$this` mean the receiver's class, not the declaring one.
                        let t = if t.eq_ignore_ascii_case(&decl.fqn)
                            && m.effective_type().is_some_and(|ty| {
                                let ty = ty.to_ascii_lowercase();
                                ty.contains("static") || ty.contains("$this")
                            }) {
                            class.fqn.clone()
                        } else {
                            t
                        };
                        if !out.iter().any(|o: &String| o.eq_ignore_ascii_case(&t)) {
                            out.push(t);
                        }
                    }
                }
            }
            return out;
        }
        // A function call: its return type.
        let candidates = function_candidates(file, name, name_start);
        for cand in &candidates {
            if let Some(t) = index.meta().function_return(cand, arg) {
                return meta_type_classes(index, file, src, &args, t, depth);
            }
        }
        for cand in candidates {
            if let Some((f, decl)) = index.files().find_map(|f| {
                f.functions
                    .iter()
                    .find(|x| x.fqn.eq_ignore_ascii_case(&cand))
                    .map(|x| (f.clone(), x.clone()))
            }) {
                return decl
                    .return_type
                    .as_deref()
                    .map(|t| type_classes(t, &f, decl.range.start, None))
                    .unwrap_or_default();
            }
        }
        return vec![];
    }

    let start = ident_start(src, end);
    let recv = &src[start..end];
    match recv.to_ascii_lowercase().as_str() {
        "" => vec![],
        "$this" | "self" | "static" => enclosing.map(|c| c.fqn).into_iter().collect(),
        "parent" => enclosing
            .and_then(|c| c.extends.first().cloned())
            .into_iter()
            .collect(),
        r if r.starts_with('$') => {
            // `$this->invoices->settle()` is handled below; a plain variable here.
            let before = src[..start].trim_end();
            if before.ends_with("::") {
                // `Foo::$instance->bar()`: the static property's type.
                let Some((recv_end, _)) = access_before(before) else {
                    return vec![];
                };
                return property_types(index, file, src, recv_end, &recv[1..], depth);
            }
            let declared = variable_classes(file, src, end, recv);
            if !declared.is_empty() {
                return declared;
            }
            assigned_classes(index, file, src, end, recv, depth)
        }
        _ => {
            let before = src[..start].trim_end();
            if let Some((chain_end, "->")) = access_before(before) {
                return property_types(index, file, src, chain_end, recv, depth);
            }
            vec![file.resolve_class(recv, start)]
        }
    }
}

/// Types of property `name` on the expression ending at `recv_end`.
fn property_types(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    recv_end: usize,
    name: &str,
    depth: u8,
) -> Vec<String> {
    let mut out = Vec::new();
    for owner in receiver_types(index, file, src, recv_end, depth + 1) {
        let Some(class) = find_class(index, file, &owner) else {
            continue;
        };
        if let Some((decl, m)) = find_member(index, &class, name, &[SymbolKind::Property]) {
            out.extend(member_type_classes(index, file, &decl, &m));
        }
    }
    out
}

/// Ancestors of `class` the index cannot resolve (PHP's own classes and interfaces such as
/// `\Exception` or `\ArrayAccess`), nearest first.
pub(crate) fn unresolved_ancestors(index: &mut ProjectIndex, class: &ClassLike) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut chain = vec![class.clone()];
    chain.extend(index.ancestors(class));
    for c in &chain {
        for name in c.extends.iter().chain(&c.implements).chain(&c.traits) {
            if index.find_class(name).is_none()
                && index.resolve_class(name).is_none()
                && !out.iter().any(|o| o.eq_ignore_ascii_case(name))
            {
                out.push(name.clone());
            }
        }
    }
    out
}

/// Fully qualified candidates for a function name as called at `offset` of `file`:
/// imported (`use function`), namespaced, then global (PHP's fallback).
fn function_candidates(file: &FileIndex, word: &str, offset: usize) -> Vec<String> {
    named_candidates(file, word, offset, UseKind::Function)
}

fn constant_candidates(file: &FileIndex, word: &str, offset: usize) -> Vec<String> {
    named_candidates(file, word, offset, UseKind::Const)
}

fn named_candidates(file: &FileIndex, word: &str, offset: usize, kind: UseKind) -> Vec<String> {
    if let Some(bare) = word.strip_prefix('\\') {
        return vec![bare.to_string()];
    }
    let ns = file.namespace_at(offset);
    if word.contains('\\') {
        // `Support\money()`: the first segment is a namespace (class) import.
        return vec![resolve_class_name(word, ns, &file.uses)];
    }
    let matches = |a: &str, b: &str| {
        if kind == UseKind::Const {
            a == b
        } else {
            a.eq_ignore_ascii_case(b)
        }
    };
    if let Some(u) = file
        .uses
        .iter()
        .find(|u| u.kind == kind && matches(u.local_name(), word))
    {
        return vec![u.fqn.clone()];
    }
    if ns.is_empty() {
        vec![word.to_string()]
    } else {
        vec![format!("{ns}\\{word}"), word.to_string()]
    }
}

fn find_function(index: &ProjectIndex, file: &FileIndex, cands: &[String]) -> Option<String> {
    cands.iter().find_map(|cand| {
        file.functions
            .iter()
            .chain(index.files().flat_map(|f| f.functions.iter()))
            .find(|f| f.fqn.eq_ignore_ascii_case(cand))
            .map(|f| f.fqn.clone())
    })
}

fn find_constant(index: &ProjectIndex, file: &FileIndex, cands: &[String]) -> Option<String> {
    cands.iter().find_map(|cand| {
        file.constants
            .iter()
            .chain(index.files().flat_map(|f| f.constants.iter()))
            .find(|k| k.fqn == *cand)
            .map(|k| k.fqn.clone())
    })
}

/// The scope of a variable at `offset`: the enclosing method or function, else the file.
pub(crate) fn variable_scope(file: &FileIndex, src: &str, offset: usize) -> (usize, usize) {
    enclosing_callable(file, offset)
        .map(|(_, s, e)| (s, e.min(src.len())))
        .unwrap_or((0, src.len()))
}

/// Resolve the word at `start..end` of `src` (the content of `file`).
pub(crate) fn resolve_word(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    start: usize,
    end: usize,
) -> Resolved {
    let word = &src[start..end];
    // 1. Declarations.
    for c in &file.classes {
        if c.name_range.start <= start && end <= c.name_range.end {
            return Resolved::Declaration(SymbolRef::Class { fqn: c.fqn.clone() });
        }
        for m in &c.members {
            if m.name_range.start <= start && end <= m.name_range.end {
                return Resolved::Declaration(SymbolRef::Member {
                    class: c.fqn.clone(),
                    member: m.kind,
                    name: m.name.clone(),
                });
            }
        }
    }
    for f in &file.functions {
        if f.name_range.start <= start && end <= f.name_range.end {
            return Resolved::Declaration(SymbolRef::Function { fqn: f.fqn.clone() });
        }
    }
    for k in &file.constants {
        if k.range.start == start && word == k.name {
            return Resolved::Declaration(SymbolRef::Constant { fqn: k.fqn.clone() });
        }
    }

    let before = src[..start].trim_end();
    let after = src[end..].trim_start();
    let is_call = after.starts_with('(');

    // 2. Member access.
    if let Some((recv_end, op)) = access_before(before) {
        let name = word.trim_start_matches('$');
        let kinds: &[SymbolKind] = match (op, is_call) {
            (_, true) => &[SymbolKind::Method],
            ("->", false) => &[SymbolKind::Property],
            _ if word.starts_with('$') => &[SymbolKind::Property],
            _ if word.eq_ignore_ascii_case("class") => &[],
            _ => &[SymbolKind::Constant, SymbolKind::EnumCase],
        };
        let types = receiver_types(index, file, src, recv_end, 0);
        if types.is_empty() {
            return if op == "->" {
                Resolved::Dynamic {
                    kind: kinds.first().copied().unwrap_or(SymbolKind::Property),
                    name: name.to_string(),
                }
            } else {
                Resolved::Nothing
            };
        }
        if kinds.is_empty() {
            // `Foo::class`: the class.
            return match find_class(index, file, &types[0]) {
                Some(c) => Resolved::Symbol(SymbolRef::Class { fqn: c.fqn }),
                None => Resolved::External(ExternalSymbol {
                    classes: types,
                    ..Default::default()
                }),
            };
        }
        let mut external: Vec<String> = Vec::new();
        for fqn in &types {
            match find_class(index, file, fqn) {
                Some(class) => {
                    if let Some((owner, m)) = find_member(index, &class, name, kinds) {
                        return Resolved::Symbol(SymbolRef::Member {
                            class: owner.fqn,
                            member: m.kind,
                            name: m.name,
                        });
                    }
                    external.extend(unresolved_ancestors(index, &class));
                }
                None => external.push(fqn.clone()),
            }
        }
        if external.is_empty() {
            return Resolved::Nothing;
        }
        return Resolved::External(ExternalSymbol {
            classes: external,
            member: Some(ExternalMember {
                kind: kinds[0],
                name: name.to_string(),
            }),
            ..Default::default()
        });
    }

    // 3. Variables: to the parameter or first assignment in their scope.
    if let Some(var) = word.strip_prefix('$') {
        if var == "this" {
            return match file.class_at(start) {
                Some(c) => Resolved::Symbol(SymbolRef::Class { fqn: c.fqn.clone() }),
                None => Resolved::Nothing,
            };
        }
        let superglobal =
            var.starts_with('_') && var.chars().all(|c| c.is_ascii_uppercase() || c == '_');
        if var.is_empty() || superglobal {
            // `$_GET`, `$_SERVER`: superglobals have no declaration.
            return Resolved::Nothing;
        }
        let (s, e) = variable_scope(file, src, start);
        let declared_at = occurrences(&src[s..e], word)
            .first()
            .map_or(start, |i| s + i);
        return Resolved::Symbol(SymbolRef::Variable {
            name: word.to_string(),
            declared_at,
        });
    }

    // 4. Names in `use` imports (also group uses `use App\{Models\User, Support\Money}`).
    if let Some(u) = file
        .uses
        .iter()
        .find(|u| u.range.start <= start && end <= u.range.end)
        .cloned()
    {
        let cands = vec![u.fqn.clone()];
        return match u.kind {
            UseKind::Class => match find_class(index, file, &u.fqn) {
                Some(c) => Resolved::Symbol(SymbolRef::Class { fqn: c.fqn }),
                None => Resolved::External(ExternalSymbol {
                    classes: cands,
                    ..Default::default()
                }),
            },
            UseKind::Function => match find_function(index, file, &cands) {
                Some(fqn) => Resolved::Symbol(SymbolRef::Function { fqn }),
                None => Resolved::External(ExternalSymbol {
                    functions: cands,
                    ..Default::default()
                }),
            },
            UseKind::Const => match find_constant(index, file, &cands) {
                Some(fqn) => Resolved::Symbol(SymbolRef::Constant { fqn }),
                None => Resolved::External(ExternalSymbol {
                    constants: cands,
                    ..Default::default()
                }),
            },
        };
    }

    let bare = word.trim_start_matches('\\');
    if bare.is_empty() || (is_keyword(bare) && !word.starts_with('\\')) {
        return Resolved::Nothing;
    }
    let prev_word = before
        .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .unwrap_or("");
    let after_new = prev_word.eq_ignore_ascii_case("new");

    // 5. Function call.
    if is_call && !after_new && !prev_word.eq_ignore_ascii_case("function") {
        let cands = function_candidates(file, word, start);
        return match find_function(index, file, &cands) {
            Some(fqn) => Resolved::Symbol(SymbolRef::Function { fqn }),
            None => Resolved::External(ExternalSymbol {
                functions: cands,
                ..Default::default()
            }),
        };
    }

    // 6. Class reference (type, `new`, `extends`, `X::`, attribute, PHPDoc) or constant.
    match bare.to_ascii_lowercase().as_str() {
        "self" | "static" => {
            return match file.class_at(start) {
                Some(c) => Resolved::Symbol(SymbolRef::Class { fqn: c.fqn.clone() }),
                None => Resolved::Nothing,
            }
        }
        "parent" => {
            let Some(parent) = file
                .class_at(start)
                .and_then(|c| c.extends.first().cloned())
            else {
                return Resolved::Nothing;
            };
            return match find_class(index, file, &parent) {
                Some(c) => Resolved::Symbol(SymbolRef::Class { fqn: c.fqn }),
                None => Resolved::External(ExternalSymbol {
                    classes: vec![parent],
                    ..Default::default()
                }),
            };
        }
        n if is_builtin_type(n) => return Resolved::Nothing,
        _ => {}
    }
    let class_context = after.starts_with("::")
        || after.starts_with('$')
        || after_new
        || ["extends", "implements", "instanceof", "insteadof"]
            .iter()
            .any(|k| prev_word.eq_ignore_ascii_case(k))
        || before.ends_with('?')
        || before.ends_with('|')
        || before.ends_with("#[")
        || (before.ends_with(':') && !before.ends_with("::"));
    let fqn = file.resolve_class(word, start);
    if let Some(c) = find_class(index, file, &fqn) {
        return Resolved::Symbol(SymbolRef::Class { fqn: c.fqn });
    }
    let constants = if class_context {
        vec![]
    } else {
        constant_candidates(file, word, start)
    };
    if let Some(k) = find_constant(index, file, &constants) {
        return Resolved::Symbol(SymbolRef::Constant { fqn: k });
    }
    Resolved::External(ExternalSymbol {
        classes: vec![fqn],
        constants,
        ..Default::default()
    })
}

pub(crate) fn resolve_at(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    offset: usize,
) -> Resolved {
    match word_at(src, offset) {
        Some((start, end)) => resolve_word(index, file, src, start, end),
        None => Resolved::Nothing,
    }
}

/// The project symbol at `offset` (bytes) of `src`, the content of `file`.
pub fn symbol_at(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    offset: usize,
) -> Option<SymbolRef> {
    match resolve_at(index, file, src, offset) {
        Resolved::Symbol(s) | Resolved::Declaration(s) => Some(s),
        _ => None,
    }
}

/// Line starts of a text, to turn byte offsets into LSP positions.
pub(crate) struct Lines<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Lines { text, starts }
    }

    pub(crate) fn position(&self, byte: usize) -> Position {
        let byte = byte.min(self.text.len());
        let line = self.starts.partition_point(|&s| s <= byte) - 1;
        let start = self.starts[line];
        let character = self.text[start..byte].encode_utf16().count() as u32;
        Position::new(line as u32, character)
    }

    pub(crate) fn range(&self, start: usize, end: usize) -> Range {
        Range::new(self.position(start), self.position(end))
    }
}

/// Find Usages from the index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpUsages {
    pub symbol: SymbolRef,
    /// What the symbol is (class, interface, method, ...).
    #[serde(default)]
    pub element: ElementKind,
    /// `User`, `User::orders()`, ...
    pub title: String,
    pub declaration: Option<UsageLocation>,
    pub usages: Vec<UsageLocation>,
}

/// Contents of the project's PHP files: the editor buffer of `rel`, the rest from disk.
pub(crate) struct Sources<'a> {
    root: std::path::PathBuf,
    rel: &'a str,
    src: &'a str,
    cache: HashMap<String, Option<String>>,
}

impl<'a> Sources<'a> {
    pub(crate) fn get(&mut self, path: &str) -> Option<String> {
        if path == self.rel {
            return Some(self.src.to_string());
        }
        let root = &self.root;
        self.cache
            .entry(path.to_string())
            .or_insert_with(|| std::fs::read_to_string(root.join(path)).ok())
            .clone()
    }
}

pub(crate) fn is_vendor(path: &str) -> bool {
    path.starts_with("vendor/") || path.contains("/vendor/")
}

/// Byte offsets of `word` in `text` at identifier boundaries (a namespace separator may
/// come before it: `App\Models\User`).
fn occurrences(text: &str, word: &str) -> Vec<usize> {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(word)
        .filter(|(i, _)| {
            let before = text[..*i].chars().next_back();
            let after = text[i + word.len()..].chars().next();
            !before.is_some_and(|c| ident(c) || (c == '$' && !word.starts_with('$')))
                && !after.is_some_and(ident)
        })
        .map(|(i, _)| i)
        .collect()
}

/// Path and byte range of the name of `symbol`'s declaration. Classes and members of the
/// current file come from its unsaved content.
pub(crate) fn declaration_range(
    index: &mut ProjectIndex,
    current: &FileIndex,
    symbol: &SymbolRef,
) -> Option<(String, crate::model::Range)> {
    let class_of = |index: &mut ProjectIndex, fqn: &str| -> Option<(String, ClassLike)> {
        if let Some(c) = current
            .classes
            .iter()
            .find(|c| c.fqn.eq_ignore_ascii_case(fqn))
        {
            return Some((current.path.clone(), c.clone()));
        }
        let c = index.resolve_class(fqn)?;
        Some((index.class_path(&c.fqn)?.to_string(), c))
    };
    match symbol {
        SymbolRef::Class { fqn } => class_of(index, fqn).map(|(p, c)| (p, c.name_range)),
        SymbolRef::Member {
            class,
            member,
            name,
        } => {
            let (path, c) = class_of(index, class)?;
            Some((path, c.member(*member, name)?.name_range))
        }
        SymbolRef::Function { fqn } => current
            .functions
            .iter()
            .find(|x| x.fqn.eq_ignore_ascii_case(fqn))
            .map(|x| (current.path.clone(), x.name_range))
            .or_else(|| {
                index.files().find_map(|f| {
                    f.functions
                        .iter()
                        .find(|x| x.fqn.eq_ignore_ascii_case(fqn))
                        .map(|x| (f.path.clone(), x.name_range))
                })
            }),
        SymbolRef::Constant { fqn } => current
            .constants
            .iter()
            .find(|k| k.fqn == *fqn)
            .map(|k| (current.path.clone(), k.range))
            .or_else(|| {
                index.files().find_map(|f| {
                    f.constants
                        .iter()
                        .find(|k| k.fqn == *fqn)
                        .map(|k| (f.path.clone(), k.range))
                })
            }),
        SymbolRef::Variable { name, declared_at } => Some((
            current.path.clone(),
            crate::model::Range {
                start: *declared_at,
                end: declared_at + name.len(),
                line: 0,
                column: 0,
            },
        )),
    }
}

/// Where `symbol` is declared.
fn declaration_of(
    index: &mut ProjectIndex,
    current: &FileIndex,
    sources: &mut Sources,
    symbol: &SymbolRef,
) -> Option<UsageLocation> {
    let (path, range) = declaration_range(index, current, symbol)?;
    let text = sources.get(&path)?;
    let lines = Lines::new(&text);
    // Constants' ranges cover `NAME = value`; the name is enough.
    let end = match symbol {
        SymbolRef::Constant { fqn } => range.start + short_name(fqn).len(),
        _ => range.end,
    };
    Some(UsageLocation {
        range: lines.range(range.start, end),
        path,
        kind: None,
    })
}

/// The symbol Find Usages searches for at `offset`: a project symbol, or one of PHP's own
/// classes / functions / constants (their declaration is in the stubs).
pub(crate) fn usage_symbol(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    offset: usize,
) -> Option<SymbolRef> {
    match resolve_at(index, file, src, offset) {
        Resolved::Symbol(s) | Resolved::Declaration(s) => Some(s),
        Resolved::External(e) if e.member.is_none() => {
            if let Some(c) = e.classes.first().filter(|_| e.constants.is_empty()) {
                Some(SymbolRef::Class { fqn: c.clone() })
            } else if let Some(f) = e.functions.last() {
                Some(SymbolRef::Function { fqn: f.clone() })
            } else {
                e.constants
                    .last()
                    .map(|c| SymbolRef::Constant { fqn: c.clone() })
                    .or_else(|| {
                        e.classes
                            .first()
                            .map(|c| SymbolRef::Class { fqn: c.clone() })
                    })
            }
        }
        _ => None,
    }
}

/// Whether an unresolved reference is a usage of `symbol` (a PHP built-in).
fn external_matches(symbol: &SymbolRef, e: &ExternalSymbol) -> bool {
    if e.member.is_some() {
        return false;
    }
    match symbol {
        SymbolRef::Class { fqn } => e.classes.iter().any(|c| c.eq_ignore_ascii_case(fqn)),
        SymbolRef::Function { fqn } => e.functions.iter().any(|c| c.eq_ignore_ascii_case(fqn)),
        SymbolRef::Constant { fqn } => e.constants.iter().any(|c| c == fqn),
        _ => false,
    }
}

/// Usages of a variable inside its scope of the current file.
pub(crate) fn variable_usages(
    file: &FileIndex,
    rel: &str,
    src: &str,
    name: &str,
    declared_at: usize,
) -> Vec<UsageLocation> {
    let (s, e) = variable_scope(file, src, declared_at);
    let lines = Lines::new(src);
    occurrences(&src[s..e], name)
        .into_iter()
        .map(|i| UsageLocation {
            path: rel.to_string(),
            range: lines.range(s + i, s + i + name.len()),
            kind: None,
        })
        .collect()
}

/// What kind of element a symbol is, for the Find Usages options and dialog.
pub(crate) fn element_kind(
    index: &mut ProjectIndex,
    current: &FileIndex,
    symbol: &SymbolRef,
) -> ElementKind {
    match symbol {
        SymbolRef::Class { fqn } => match find_class(index, current, fqn).map(|c| c.kind) {
            Some(SymbolKind::Interface) => ElementKind::Interface,
            Some(SymbolKind::Trait) => ElementKind::Trait,
            Some(SymbolKind::Enum) => ElementKind::Enum,
            _ => ElementKind::Class,
        },
        SymbolRef::Member { member, .. } => match member {
            SymbolKind::Method => ElementKind::Method,
            SymbolKind::Property => ElementKind::Property,
            SymbolKind::EnumCase => ElementKind::EnumCase,
            _ => ElementKind::Constant,
        },
        SymbolRef::Function { .. } => ElementKind::Function,
        SymbolRef::Constant { .. } => ElementKind::Constant,
        SymbolRef::Variable { .. } => ElementKind::Variable,
    }
}

/// Occurrences of `symbol` in `paths`: references the index resolves to it (members also
/// through parent and child classes), dynamic member accesses, PHPDoc mentions of classes
/// and, with `comments`, the name in comments and strings.
pub(crate) fn search_symbol(
    index: &mut ProjectIndex,
    rel: &str,
    current: &FileIndex,
    sources: &mut Sources,
    symbol: &SymbolRef,
    paths: &[String],
    comments: bool,
) -> Vec<UsageLocation> {
    let word = symbol.search_word().to_string();
    // Members also count where they are reached through a parent or child class (an
    // interface method called on an implementation and vice versa), like PHPStorm's
    // search for a base method.
    let related: HashSet<String> = match symbol {
        SymbolRef::Member { class, .. } => {
            let mut set: HashSet<String> = subclasses(index, class)
                .into_iter()
                .map(|c| c.fqn.to_ascii_lowercase())
                .collect();
            set.insert(class.to_ascii_lowercase());
            if let Some(c) = find_class(index, current, class) {
                set.extend(
                    index
                        .ancestors(&c)
                        .into_iter()
                        .map(|a| a.fqn.to_ascii_lowercase()),
                );
            }
            set
        }
        _ => HashSet::new(),
    };
    let matches = |s: &SymbolRef| {
        s.same(symbol)
            || matches!((s, symbol), (
                SymbolRef::Member { class, member, name },
                SymbolRef::Member { member: m2, name: n2, .. },
            ) if member == m2
                && name.eq_ignore_ascii_case(n2)
                && related.contains(&class.to_ascii_lowercase()))
    };

    let mut usages = Vec::new();
    for path in paths {
        let Some(text) = sources.get(path) else {
            continue;
        };
        let hits = occurrences(&text, &word);
        if hits.is_empty() {
            continue;
        }
        let file = if path == rel {
            current.clone()
        } else {
            match index.file(path) {
                Some(f) => f.clone(),
                None => parser::index_source(path, &text),
            }
        };
        let lines = Lines::new(&text);
        for at in hits {
            let end = at + word.len();
            let range = lines.range(at, end);
            let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
            let line_end = text[at..].find('\n').map_or(text.len(), |i| at + i);
            let line = &text[line_start..line_end];
            let start_col = text[line_start..at].encode_utf16().count() as u32;
            let classified = classify_usage(
                line,
                start_col,
                start_col + word.encode_utf16().count() as u32,
            );
            if matches!(classified, UsageKind::Comment | UsageKind::StringLiteral) {
                if comments && !matches!(symbol, SymbolRef::Variable { .. }) {
                    usages.push(UsageLocation {
                        path: path.clone(),
                        range,
                        kind: Some(classified),
                    });
                }
                continue;
            }
            let kind = match resolve_at(index, &file, &text, at) {
                Resolved::Symbol(s) if matches(&s) => None,
                Resolved::External(e) if external_matches(symbol, &e) => None,
                Resolved::Dynamic { kind, name }
                    if matches!(symbol, SymbolRef::Member { member, name: n, .. }
                        if *member == kind && n.eq_ignore_ascii_case(&name)) =>
                {
                    Some(UsageKind::Dynamic)
                }
                // PHPDoc mentions of a class (`@return User`, `@var User[]`).
                Resolved::Nothing
                    if classified == UsageKind::Doc
                        && matches!(symbol, SymbolRef::Class { fqn }
                            if file.resolve_class(&word, at).eq_ignore_ascii_case(fqn)) =>
                {
                    None
                }
                _ => continue,
            };
            usages.push(UsageLocation {
                path: path.clone(),
                range,
                kind,
            });
        }
    }
    usages
}

/// The location of a class-like's or member's name in its file.
fn name_location(
    index: &ProjectIndex,
    sources: &mut Sources,
    class: &ClassLike,
    range: crate::model::Range,
    kind: UsageKind,
) -> Option<UsageLocation> {
    let path = index.class_path(&class.fqn)?.to_string();
    let text = sources.get(&path)?;
    Some(UsageLocation {
        range: Lines::new(&text).range(range.start, range.end),
        path,
        kind: Some(kind),
    })
}

/// Usages of the symbol at `offset` in the project's own files (vendor excluded). Members
/// used on expressions the index cannot type are reported as dynamic usages.
pub fn find_usages(
    index: &mut ProjectIndex,
    rel: &str,
    src: &str,
    offset: usize,
) -> Option<PhpUsages> {
    find_usages_with(index, rel, src, offset, &UsageOptions::default())
}

/// Find Usages with the options of the Find Usages dialog: for classes also the usages of
/// their methods and fields and their inheritors, for methods the overriding methods, and
/// comments and strings on request. Scopes and text occurrences are the caller's.
pub fn find_usages_with(
    index: &mut ProjectIndex,
    rel: &str,
    src: &str,
    offset: usize,
    options: &UsageOptions,
) -> Option<PhpUsages> {
    let current = parser::index_source(rel, src);
    let symbol = usage_symbol(index, &current, src, offset)?;
    let element = element_kind(index, &current, &symbol);
    let opts = options.effective(element);
    let mut sources = Sources {
        root: index.root().to_path_buf(),
        rel,
        src,
        cache: HashMap::new(),
    };
    let declaration = declaration_of(index, &current, &mut sources, &symbol);
    if let SymbolRef::Variable { name, declared_at } = &symbol {
        return Some(PhpUsages {
            title: symbol.display(),
            usages: variable_usages(&current, rel, src, name, *declared_at),
            declaration,
            symbol,
            element,
        });
    }
    let mut paths: Vec<String> = index
        .files()
        .map(|f| f.path.clone())
        .filter(|p| !is_vendor(p) && p != rel)
        .collect();
    paths.sort();
    paths.insert(0, rel.to_string());

    let mut usages = if opts.usages || opts.comments_and_strings {
        let found = search_symbol(
            index,
            rel,
            &current,
            &mut sources,
            &symbol,
            &paths,
            opts.comments_and_strings,
        );
        if opts.usages {
            found
        } else {
            found
                .into_iter()
                .filter(|u| u.kind.is_some_and(|k| k != UsageKind::Dynamic))
                .collect()
        }
    } else {
        Vec::new()
    };

    match &symbol {
        SymbolRef::Class { fqn } => {
            let class = find_class(index, &current, fqn);
            if let Some(class) = class
                .as_ref()
                .filter(|_| opts.method_usages || opts.field_usages)
            {
                for m in &class.members {
                    let wanted = match m.kind {
                        SymbolKind::Method => opts.method_usages,
                        _ => opts.field_usages,
                    };
                    if !wanted {
                        continue;
                    }
                    let member = SymbolRef::Member {
                        class: class.fqn.clone(),
                        member: m.kind,
                        name: m.name.clone(),
                    };
                    usages.extend(search_symbol(
                        index,
                        rel,
                        &current,
                        &mut sources,
                        &member,
                        &paths,
                        false,
                    ));
                }
            }
            if opts.derived_classes || opts.implementing_classes || opts.derived_interfaces {
                let base_is_interface = class
                    .as_ref()
                    .is_some_and(|c| c.kind == SymbolKind::Interface);
                for sub in subclasses(index, fqn) {
                    let kind = match (sub.kind, base_is_interface) {
                        (SymbolKind::Interface, _) if opts.derived_interfaces => {
                            UsageKind::DerivedInterface
                        }
                        (SymbolKind::Interface, _) => continue,
                        (_, true) if opts.implementing_classes => UsageKind::ImplementingClass,
                        (_, false) if opts.derived_classes => UsageKind::DerivedClass,
                        _ => continue,
                    };
                    if let Some(l) = name_location(index, &mut sources, &sub, sub.name_range, kind)
                    {
                        usages.push(l);
                    }
                }
            }
        }
        SymbolRef::Member {
            class,
            member: SymbolKind::Method,
            name,
        } if opts.overriding_methods => {
            let base = find_class(index, &current, class);
            let abstract_base = base.as_ref().is_some_and(|c| {
                c.kind == SymbolKind::Interface
                    || c.method(name).is_some_and(|m| m.modifiers.is_abstract)
            });
            let kind = if abstract_base {
                UsageKind::ImplementingMethod
            } else {
                UsageKind::OverridingMethod
            };
            for sub in subclasses(index, class) {
                if let Some(m) = sub.method(name) {
                    let range = m.name_range;
                    if let Some(l) = name_location(index, &mut sources, &sub, range, kind) {
                        usages.push(l);
                    }
                }
            }
        }
        _ => {}
    }
    Some(PhpUsages {
        title: symbol.display(),
        symbol,
        declaration,
        usages,
        element,
    })
}

/// The index-based Navigate actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NavKind {
    Implementation,
    SuperMethod,
    TypeDeclaration,
}

/// Targets of `kind` for the caret at `offset` (bytes).
pub fn navigate(
    index: &mut ProjectIndex,
    rel: &str,
    src: &str,
    offset: usize,
    kind: NavKind,
) -> Vec<NavTarget> {
    match kind {
        NavKind::Implementation => implementations(index, rel, src, offset),
        NavKind::SuperMethod => super_targets(index, rel, src, offset),
        NavKind::TypeDeclaration => type_targets(index, rel, src, offset),
    }
}

/// A navigation target: a declaration for Go to Declaration, Implementation, Super Method,
/// Type Declaration and the gutter markers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavTarget {
    pub path: String,
    /// 0-based line and UTF-16 column of the name.
    pub line: u32,
    pub column: u32,
    /// `OrderController`, `Model::save()`.
    pub name: String,
    /// Namespace or declaring class, shown greyed out in the chooser.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
}

fn target(
    sources: &mut Sources,
    path: &str,
    at: crate::model::Range,
    name: String,
    container: Option<String>,
) -> Option<NavTarget> {
    let text = sources.get(path)?;
    let p = Lines::new(&text).position(at.start);
    Some(NavTarget {
        path: path.to_string(),
        line: p.line,
        column: p.character,
        name,
        container,
    })
}

fn class_target(index: &mut ProjectIndex, sources: &mut Sources, fqn: &str) -> Option<NavTarget> {
    let c = index.resolve_class(fqn)?;
    let path = index.class_path(&c.fqn)?.to_string();
    class_nav(sources, &path, &c)
}

fn class_nav(sources: &mut Sources, path: &str, c: &ClassLike) -> Option<NavTarget> {
    let ns = (!c.namespace.is_empty()).then(|| c.namespace.clone());
    target(sources, path, c.name_range, c.name.clone(), ns)
}

fn member_name(owner: &ClassLike, m: &Member) -> String {
    match m.kind {
        SymbolKind::Method => format!("{}::{}()", owner.name, m.name),
        SymbolKind::Property => format!("{}::${}", owner.name, m.name),
        _ => format!("{}::{}", owner.name, m.name),
    }
}

fn member_target(
    index: &ProjectIndex,
    sources: &mut Sources,
    owner: &ClassLike,
    m: &Member,
) -> Option<NavTarget> {
    let path = index.class_path(&owner.fqn)?.to_string();
    member_nav(sources, &path, owner, m)
}

fn member_nav(
    sources: &mut Sources,
    path: &str,
    owner: &ClassLike,
    m: &Member,
) -> Option<NavTarget> {
    let ns = (!owner.namespace.is_empty()).then(|| owner.namespace.clone());
    target(sources, path, m.name_range, member_name(owner, m), ns)
}

/// Every indexed class that extends or implements `fqn` (directly or not).
pub(crate) fn subclasses(index: &mut ProjectIndex, fqn: &str) -> Vec<ClassLike> {
    let all: Vec<ClassLike> = index
        .files()
        .filter(|f| !is_vendor(&f.path))
        .flat_map(|f| f.classes.iter().cloned())
        .collect();
    let mut out: Vec<ClassLike> = all
        .into_iter()
        .filter(|c| !c.fqn.eq_ignore_ascii_case(fqn))
        .filter(|c| {
            index
                .ancestors(c)
                .iter()
                .any(|a| a.fqn.eq_ignore_ascii_case(fqn))
        })
        .collect();
    out.sort_by(|a, b| a.fqn.cmp(&b.fqn));
    out
}

pub(crate) fn sources<'a>(index: &ProjectIndex, rel: &'a str, src: &'a str) -> Sources<'a> {
    Sources {
        root: index.root().to_path_buf(),
        rel,
        src,
        cache: HashMap::new(),
    }
}

/// Go to Implementation: classes extending / implementing the class or interface at the
/// caret, or the methods overriding the method at the caret.
pub fn implementations(
    index: &mut ProjectIndex,
    rel: &str,
    src: &str,
    offset: usize,
) -> Vec<NavTarget> {
    let file = parser::index_source(rel, src);
    let mut sources = sources(index, rel, src);
    match symbol_at(index, &file, src, offset) {
        Some(SymbolRef::Class { fqn }) => subclasses(index, &fqn)
            .iter()
            .filter_map(|c| class_target(index, &mut sources, &c.fqn))
            .collect(),
        Some(SymbolRef::Member {
            class,
            member: SymbolKind::Method,
            name,
        }) => subclasses(index, &class)
            .iter()
            .filter_map(|c| {
                let m = c.method(&name)?;
                member_target(index, &mut sources, c, m)
            })
            .collect(),
        _ => vec![],
    }
}

/// Go to Super Method: the methods the method at (or around) the caret overrides or
/// implements; on a class, its parent class and interfaces.
pub fn super_targets(
    index: &mut ProjectIndex,
    rel: &str,
    src: &str,
    offset: usize,
) -> Vec<NavTarget> {
    let file = parser::index_source(rel, src);
    let mut sources = sources(index, rel, src);
    let Some(class) = file.class_at(offset).cloned() else {
        return vec![];
    };
    let method = match symbol_at(index, &file, src, offset) {
        Some(SymbolRef::Member {
            member: SymbolKind::Method,
            name,
            class: owner,
        }) if owner.eq_ignore_ascii_case(&class.fqn) => Some(name),
        Some(SymbolRef::Class { fqn }) if fqn.eq_ignore_ascii_case(&class.fqn) => None,
        _ => class
            .members
            .iter()
            .find(|m| m.kind == SymbolKind::Method && m.range.contains(offset))
            .map(|m| m.name.clone()),
    };
    match method {
        Some(name) => {
            let mut out: Vec<NavTarget> = Vec::new();
            for anc in index.ancestors(&class) {
                if let Some(m) = anc.method(&name) {
                    if let Some(t) = member_target(index, &mut sources, &anc, m) {
                        if !out.contains(&t) {
                            out.push(t);
                        }
                    }
                }
            }
            out
        }
        None => class
            .extends
            .iter()
            .chain(&class.implements)
            .filter_map(|fqn| class_target(index, &mut sources, fqn))
            .collect(),
    }
}

/// Go to Type Declaration: the classes of the variable, property, method or function at
/// the caret.
pub fn type_targets(
    index: &mut ProjectIndex,
    rel: &str,
    src: &str,
    offset: usize,
) -> Vec<NavTarget> {
    let file = parser::index_source(rel, src);
    let mut sources = sources(index, rel, src);
    let Some((start, end)) = word_at(src, offset) else {
        return vec![];
    };
    let word = &src[start..end];
    let before = src[..start].trim_end();
    let classes: Vec<String> = if word.starts_with('$') && !before.ends_with("::") {
        variable_classes(&file, src, start, word)
    } else {
        match symbol_at(index, &file, src, offset) {
            Some(SymbolRef::Class { fqn }) => vec![fqn],
            Some(SymbolRef::Member {
                class,
                member,
                name,
            }) => match find_class(index, &file, &class) {
                Some(owner) => match owner.member(member, &name).cloned() {
                    Some(m) if matches!(member, SymbolKind::Method | SymbolKind::Property) => {
                        member_type_classes(index, &file, &owner, &m)
                    }
                    Some(_) => vec![owner.fqn],
                    None => vec![],
                },
                None => vec![],
            },
            Some(SymbolRef::Function { fqn }) => index
                .files()
                .flat_map(|f| f.functions.iter().map(move |x| (f, x)))
                .find(|(_, x)| x.fqn.eq_ignore_ascii_case(&fqn))
                .and_then(|(f, x)| {
                    let ty = x.return_type.clone()?;
                    Some(type_classes(&ty, f, x.range.start, None))
                })
                .unwrap_or_default(),
            _ => vec![],
        }
    };
    classes
        .iter()
        .filter_map(|fqn| class_target(index, &mut sources, fqn))
        .collect()
}

// ---- Go to Declaration ----

/// What the Ctrl+hover tooltip shows: the container (namespace or class) and the
/// declaration's signature.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolInfo {
    /// `App\Models`, `App\Models\User`; empty for globals and variables.
    pub container: String,
    /// `class User extends Model`, `public function orders(): HasMany`.
    pub signature: String,
}

/// Go to Declaration (Ctrl+B / Ctrl+Click) from the index.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpDefinition {
    /// The caret is on a declaration: PHPStorm shows its usages instead.
    pub on_declaration: bool,
    /// The reference under the caret (`\App\Models\User`, `save`), for the Ctrl+hover link.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
    /// `User`, `User::orders()`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub targets: Vec<NavTarget>,
    /// Not declared in the project: look it up in the PHP stubs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<ExternalSymbol>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<SymbolInfo>,
}

fn namespace_of(fqn: &str) -> String {
    fqn.rsplit_once('\\')
        .map(|(ns, _)| ns.to_string())
        .unwrap_or_default()
}

fn symbol_info(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    symbol: &SymbolRef,
) -> Option<SymbolInfo> {
    match symbol {
        SymbolRef::Class { fqn } => {
            let c = find_class(index, file, fqn)?;
            Some(SymbolInfo {
                container: c.namespace.clone(),
                signature: c.signature(),
            })
        }
        SymbolRef::Member {
            class,
            member,
            name,
        } => {
            let c = find_class(index, file, class)?;
            let m = c.member(*member, name)?;
            Some(SymbolInfo {
                container: c.fqn.clone(),
                signature: m.signature(),
            })
        }
        SymbolRef::Function { fqn } => file
            .functions
            .iter()
            .chain(index.files().flat_map(|f| f.functions.iter()))
            .find(|f| f.fqn.eq_ignore_ascii_case(fqn))
            .map(|f| SymbolInfo {
                container: f.namespace.clone(),
                signature: f.signature(),
            }),
        SymbolRef::Constant { fqn } => file
            .constants
            .iter()
            .chain(index.files().flat_map(|f| f.constants.iter()))
            .find(|k| k.fqn == *fqn)
            .map(|k| SymbolInfo {
                container: namespace_of(fqn),
                signature: match &k.value {
                    Some(v) => format!("const {} = {v}", k.name),
                    None => format!("const {}", k.name),
                },
            }),
        SymbolRef::Variable { name, declared_at } => {
            let types = variable_classes(file, src, *declared_at + 1, name);
            let ty: Vec<&str> = types.iter().map(|t| short_name(t)).collect();
            Some(SymbolInfo {
                container: String::new(),
                signature: if ty.is_empty() {
                    name.clone()
                } else {
                    format!("{} {name}", ty.join("|"))
                },
            })
        }
    }
}

/// The declaration of `symbol` as a navigation target.
fn symbol_target(
    index: &mut ProjectIndex,
    current: &FileIndex,
    sources: &mut Sources,
    symbol: &SymbolRef,
) -> Option<NavTarget> {
    let (path, range) = declaration_range(index, current, symbol)?;
    let (name, container) = match symbol {
        SymbolRef::Member { class, .. } => (symbol.display(), Some(namespace_of(class))),
        SymbolRef::Variable { name, .. } => (name.clone(), None),
        SymbolRef::Class { fqn } | SymbolRef::Function { fqn } | SymbolRef::Constant { fqn } => {
            (symbol.display(), Some(namespace_of(fqn)))
        }
    };
    target(
        sources,
        &path,
        range,
        name,
        container.filter(|c| !c.is_empty()),
    )
}

/// Go to Declaration at `offset` (bytes): the declaration of the symbol under the caret,
/// or `on_declaration` when the caret is on a declaration (Show Usages instead), or the
/// unresolved name to look up in the PHP stubs.
pub fn definition(index: &mut ProjectIndex, rel: &str, src: &str, offset: usize) -> PhpDefinition {
    let file = parser::index_source(rel, src);
    if let Some(d) = meta_key_definition(index, &file, rel, src, offset) {
        return d;
    }
    let Some((start, end)) = word_at(src, offset) else {
        return PhpDefinition::default();
    };
    let lines = Lines::new(src);
    let mut out = PhpDefinition {
        range: Some(lines.range(start, end)),
        ..Default::default()
    };
    match resolve_word(index, &file, src, start, end) {
        Resolved::Declaration(s) => {
            out.on_declaration = true;
            out.title = Some(s.display());
            out.info = symbol_info(index, &file, src, &s);
        }
        Resolved::Symbol(s) => {
            out.on_declaration =
                matches!(&s, SymbolRef::Variable { declared_at, .. } if *declared_at == start);
            out.title = Some(s.display());
            out.info = symbol_info(index, &file, src, &s);
            let mut sources = sources(index, rel, src);
            if !out.on_declaration {
                out.targets = symbol_target(index, &file, &mut sources, &s)
                    .into_iter()
                    .collect();
            }
        }
        Resolved::External(e) => {
            out.title = Some(e.display());
            out.external = Some(e);
        }
        Resolved::Dynamic { .. } | Resolved::Nothing => out.range = None,
    }
    out
}

/// Go to Declaration on a string argument that a `.phpstorm.meta.php` map knows
/// (`app('cache')`, `$container->get('mailer')`): the class it maps to, else the map entry.
fn meta_key_definition(
    index: &mut ProjectIndex,
    file: &FileIndex,
    rel: &str,
    src: &str,
    offset: usize,
) -> Option<PhpDefinition> {
    if index.meta().overrides.is_empty() {
        return None;
    }
    // The caret inside a string literal: the call it is an argument of.
    let site = crate::callsite::call_at(index, file, src, offset)?;
    let string = site.string?;
    let content_start = string.quote + 1;
    let content_end = src[content_start..]
        .find(string.quote_char)
        .map(|i| content_start + i)?;
    if src[content_start..content_end].contains('\n') {
        return None;
    }
    let value = crate::meta::ArgValue::Str(src[content_start..content_end].to_string());
    let entry = index
        .meta()
        .map_entry(|t| site.callee.matches(t), site.argument, &value)?
        .clone();
    let lines = Lines::new(src);
    let mut out = PhpDefinition {
        range: Some(lines.range(content_start, content_end)),
        title: Some(src[content_start..content_end].to_string()),
        ..Default::default()
    };
    let mut sources = sources(index, rel, src);
    if !entry.value.contains('@') {
        if let Some(t) = class_target(index, &mut sources, entry.value.trim_start_matches('\\')) {
            out.targets.push(t);
            return Some(out);
        }
    }
    if let Some(t) = target(
        &mut sources,
        &entry.location.path,
        entry.location.range,
        format!("'{}'", entry.key),
        Some(crate::meta::META_NAME.to_string()),
    ) {
        out.targets.push(t);
    }
    Some(out)
}

// ---- Gutter markers ----

/// PHPStorm's implemented / overridden line markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkerKind {
    /// A class with subclasses (↓).
    Subclassed,
    /// An interface, or an abstract / interface method, with implementations (↓).
    Implemented,
    /// A method overridden in subclasses (↓).
    Overridden,
    /// A method implementing an interface or abstract method (↑).
    Implements,
    /// A method overriding a parent's method (↑).
    Overrides,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpLineMarker {
    /// 0-based line of the class or method name.
    pub line: u32,
    pub kind: MarkerKind,
    /// The tooltip: "Is implemented in User", "Overrides method in Model".
    pub title: String,
    pub targets: Vec<NavTarget>,
}

fn names_title(prefix: &str, targets: &[NavTarget], what: &str) -> String {
    match targets {
        [one] => format!("{prefix} {}", one.name),
        many => format!("{prefix} {} {what}", many.len()),
    }
}

/// The gutter markers of `src` (the content of `rel`): subclasses and implementations of
/// its classes and interfaces, methods overridden below and overriding above.
pub fn line_markers(index: &mut ProjectIndex, rel: &str, src: &str) -> Vec<PhpLineMarker> {
    let file = parser::index_source(rel, src);
    if file.classes.is_empty() {
        return vec![];
    }
    let mut sources = sources(index, rel, src);
    let lines = Lines::new(src);
    // Descendants of this file's classes, in one pass over the project's classes.
    let wanted: HashSet<String> = file
        .classes
        .iter()
        .map(|c| c.fqn.to_ascii_lowercase())
        .collect();
    let mut all: Vec<(String, ClassLike)> = index
        .files()
        .filter(|f| !is_vendor(&f.path) && f.path != rel)
        .flat_map(|f| f.classes.iter().map(|c| (f.path.clone(), c.clone())))
        .collect();
    all.extend(file.classes.iter().map(|c| (rel.to_string(), c.clone())));
    all.sort_by(|a, b| a.1.fqn.cmp(&b.1.fqn));
    let mut subs: HashMap<String, Vec<(String, ClassLike)>> = HashMap::new();
    for (path, c) in &all {
        for anc in index.ancestors(c) {
            let key = anc.fqn.to_ascii_lowercase();
            if wanted.contains(&key) {
                subs.entry(key).or_default().push((path.clone(), c.clone()));
            }
        }
    }

    let mut out = Vec::new();
    for c in &file.classes {
        let below = subs
            .get(&c.fqn.to_ascii_lowercase())
            .cloned()
            .unwrap_or_default();
        if !below.is_empty() {
            let targets: Vec<NavTarget> = below
                .iter()
                .filter_map(|(p, s)| class_nav(&mut sources, p, s))
                .collect();
            let interface = c.kind == SymbolKind::Interface;
            out.push(PhpLineMarker {
                line: lines.position(c.name_range.start).line,
                kind: if interface {
                    MarkerKind::Implemented
                } else {
                    MarkerKind::Subclassed
                },
                title: if interface {
                    names_title("Is implemented in", &targets, "classes")
                } else {
                    names_title("Is subclassed by", &targets, "classes")
                },
                targets,
            });
        }
        let ancestors: Vec<ClassLike> = index
            .ancestors(c)
            .into_iter()
            .filter(|a| a.kind != SymbolKind::Trait)
            .collect();
        for m in c.methods() {
            let line = lines.position(m.name_range.start).line;
            // Up: the declarations this method overrides or implements.
            let mut up: Vec<NavTarget> = Vec::new();
            let mut implements = false;
            for anc in &ancestors {
                if let Some(am) = anc.method(&m.name) {
                    if up.is_empty() {
                        implements = anc.kind == SymbolKind::Interface || am.modifiers.is_abstract;
                    }
                    if let Some(t) = member_target(index, &mut sources, anc, am) {
                        if !up.contains(&t) {
                            up.push(t);
                        }
                    }
                }
            }
            if let Some(first) = up.first() {
                let owner = first
                    .name
                    .split("::")
                    .next()
                    .unwrap_or_default()
                    .to_string();
                out.push(PhpLineMarker {
                    line,
                    kind: if implements {
                        MarkerKind::Implements
                    } else {
                        MarkerKind::Overrides
                    },
                    title: format!(
                        "{} method in {owner}",
                        if implements {
                            "Implements"
                        } else {
                            "Overrides"
                        }
                    ),
                    targets: up,
                });
            }
            // Down: the subclasses' methods overriding or implementing it.
            let down: Vec<NavTarget> = below
                .iter()
                .filter_map(|(p, s)| {
                    let sm = s.members.iter().find(|x| {
                        x.kind == SymbolKind::Method && x.name.eq_ignore_ascii_case(&m.name)
                    })?;
                    member_nav(&mut sources, p, s, sm)
                })
                .collect();
            if !down.is_empty() {
                let implemented = c.kind == SymbolKind::Interface || m.modifiers.is_abstract;
                let names: Vec<NavTarget> = down
                    .iter()
                    .map(|t| NavTarget {
                        name: t.name.split("::").next().unwrap_or_default().to_string(),
                        ..t.clone()
                    })
                    .collect();
                out.push(PhpLineMarker {
                    line,
                    kind: if implemented {
                        MarkerKind::Implemented
                    } else {
                        MarkerKind::Overridden
                    },
                    title: if implemented {
                        names_title("Is implemented in", &names, "classes")
                    } else {
                        names_title("Is overridden in", &names, "subclasses")
                    },
                    targets: down,
                });
            }
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::IndexConfig;
    use std::fs;

    const MODEL: &str = "<?php\nnamespace App\\Models;\n\nabstract class Model\n{\n    public function save(): bool {}\n}\n";
    const USER: &str = "<?php\nnamespace App\\Models;\n\n/**\n * @property int $id\n */\nclass User extends Model implements Contact\n{\n    public ?Address $address;\n\n    public function save(): bool { return true; }\n\n    public function orders(): Collection\n    {\n        return $this->hasMany(Order::class);\n    }\n}\n";
    const CONTACT: &str = "<?php\nnamespace App\\Models;\n\ninterface Contact\n{\n    public function save(): bool;\n}\n";
    const ADDRESS: &str = "<?php\nnamespace App\\Models;\n\nfinal class Address\n{\n    public string $city = '';\n}\n";
    const SERVICE: &str = "<?php\nnamespace App\\Services;\n\nuse App\\Models\\User;\n\n// User is mentioned in a comment\nfinal class Billing\n{\n    public function __construct(private readonly User $owner) {}\n\n    /** @return User */\n    public function bill(User $user, $any): User\n    {\n        $copy = new User();\n        $user->save();\n        $this->owner->save();\n        $any->save();\n        echo $user->address->city;\n        return $copy;\n    }\n}\n";

    fn setup() -> (tempfile::TempDir, ProjectIndex) {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        fs::create_dir_all(r.join("app/Models")).unwrap();
        fs::create_dir_all(r.join("app/Services")).unwrap();
        for (p, t) in [
            ("app/Models/Model.php", MODEL),
            ("app/Models/User.php", USER),
            ("app/Models/Contact.php", CONTACT),
            ("app/Models/Address.php", ADDRESS),
            ("app/Services/Billing.php", SERVICE),
        ] {
            fs::write(r.join(p), t).unwrap();
        }
        let mut idx = ProjectIndex::new(
            r,
            IndexConfig {
                cache_file: Some(r.join("cache.json")),
                ..Default::default()
            },
        );
        idx.index_workspace(|_, _| {});
        (dir, idx)
    }

    fn at(src: &str, needle: &str, nth: usize) -> usize {
        src.match_indices(needle).nth(nth).expect("needle").0 + 1
    }

    #[test]
    fn resolves_symbols_like_hover() {
        let (_d, mut idx) = setup();
        let file = parser::index_source("app/Services/Billing.php", SERVICE);
        let sym = |idx: &mut ProjectIndex, o| symbol_at(idx, &file, SERVICE, o);
        assert_eq!(
            sym(&mut idx, at(SERVICE, "User;", 0)),
            Some(SymbolRef::Class {
                fqn: "App\\Models\\User".into()
            })
        );
        // `$user->save()` through the typed parameter; the declaration is User's own.
        let save = sym(&mut idx, at(SERVICE, "save();", 0)).unwrap();
        assert_eq!(save.display(), "User::save()");
        // `$this->owner->save()` through the promoted property's type.
        assert_eq!(sym(&mut idx, at(SERVICE, "save();", 1)), Some(save));
        // Untyped receivers are dynamic.
        assert_eq!(sym(&mut idx, at(SERVICE, "save();", 2)), None);
        // Property chain: `$user->address->city`.
        assert_eq!(
            sym(&mut idx, at(SERVICE, "city;", 0)).unwrap().display(),
            "Address::$city"
        );
    }

    #[test]
    fn finds_usages_with_kinds_and_dynamic_ones() {
        let (_d, mut idx) = setup();
        let o = at(USER, "User extends", 0);
        let u = find_usages(&mut idx, "app/Models/User.php", USER, o).unwrap();
        assert_eq!(u.title, "User");
        let decl = u.declaration.unwrap();
        assert_eq!(
            (decl.path.as_str(), decl.range.start.line),
            ("app/Models/User.php", 6)
        );
        let lines: Vec<(String, u32)> = u
            .usages
            .iter()
            .map(|l| (l.path.clone(), l.range.start.line))
            .collect();
        // Import, constructor type, @return, param type, return type, `new User()`; not the
        // comment and not the declaration.
        assert_eq!(
            lines,
            vec![
                ("app/Services/Billing.php".to_string(), 3),
                ("app/Services/Billing.php".to_string(), 8),
                ("app/Services/Billing.php".to_string(), 10),
                ("app/Services/Billing.php".to_string(), 11),
                ("app/Services/Billing.php".to_string(), 11),
                ("app/Services/Billing.php".to_string(), 13),
            ]
        );
        assert_eq!(u.usages[0].range.start.character, 15);

        let o = at(USER, "save()", 0);
        let u = find_usages(&mut idx, "app/Models/User.php", USER, o).unwrap();
        assert_eq!(u.title, "User::save()");
        let kinds: Vec<Option<UsageKind>> = u.usages.iter().map(|l| l.kind).collect();
        assert_eq!(kinds, vec![None, None, Some(UsageKind::Dynamic)]);

        // The interface method: calls through the implementing class count too.
        let o = at(CONTACT, "save", 0);
        let u = find_usages(&mut idx, "app/Models/Contact.php", CONTACT, o).unwrap();
        assert_eq!(u.title, "Contact::save()");
        let lines: Vec<(&str, u32)> = u
            .usages
            .iter()
            .map(|l| (l.path.as_str(), l.range.start.line))
            .collect();
        assert_eq!(
            lines,
            vec![
                ("app/Services/Billing.php", 14),
                ("app/Services/Billing.php", 15),
                ("app/Services/Billing.php", 16),
            ]
        );
    }

    #[test]
    fn navigates_to_implementations_super_methods_and_types() {
        let (_d, mut idx) = setup();
        let names = |t: Vec<NavTarget>| t.into_iter().map(|t| t.name).collect::<Vec<_>>();

        let o = at(CONTACT, "Contact", 0);
        assert_eq!(
            names(implementations(
                &mut idx,
                "app/Models/Contact.php",
                CONTACT,
                o
            )),
            vec!["User"]
        );
        let o = at(MODEL, "save", 0);
        let impls = implementations(&mut idx, "app/Models/Model.php", MODEL, o);
        assert_eq!(names(impls.clone()), vec!["User::save()"]);
        assert_eq!(
            (impls[0].path.as_str(), impls[0].line),
            ("app/Models/User.php", 10)
        );

        // Ctrl+U inside User::save(): the parent's and the interface's declarations.
        let o = at(USER, "return true", 0);
        assert_eq!(
            names(super_targets(&mut idx, "app/Models/User.php", USER, o)),
            vec!["Model::save()", "Contact::save()"]
        );
        let o = at(USER, "User extends", 0);
        assert_eq!(
            names(super_targets(&mut idx, "app/Models/User.php", USER, o)),
            vec!["Model", "Contact"]
        );

        let o = at(SERVICE, "$user->save", 0);
        assert_eq!(
            names(type_targets(
                &mut idx,
                "app/Services/Billing.php",
                SERVICE,
                o
            )),
            vec!["User"]
        );
        let o = at(SERVICE, "address->", 0);
        let t = type_targets(&mut idx, "app/Services/Billing.php", SERVICE, o);
        assert_eq!(names(t.clone()), vec!["Address"]);
        assert_eq!(t[0].container.as_deref(), Some("App\\Models"));
        let o = at(SERVICE, "$copy;", 0);
        assert_eq!(
            names(type_targets(
                &mut idx,
                "app/Services/Billing.php",
                SERVICE,
                o
            )),
            vec!["User"]
        );
    }

    const HELPERS: &str = "<?php\nnamespace App\\Support;\n\nconst CURRENCY = 'EUR';\n\nfunction money(int $cents): string { return ''; }\n";
    const RESOLVE: &str = r#"<?php
namespace App\Http;

use App\Models\User as Customer;
use App\Models\{Address, Contact};
use function App\Support\money;
use const App\Support\CURRENCY;

final class Checkout extends \RuntimeException implements \JsonSerializable
{
    public function run(Customer $customer, \App\Models\User $user): void
    {
        try {
            money(1) . CURRENCY . PHP_EOL;
            $total = array_map(fn ($a) => $a, []);
            $customer->save();
            $customer->address->city;
            $this->getMessage();
            echo $total;
        } catch (\Throwable $e) {
            $e->getMessage();
            throw new \LogicException(Address::class);
        }
    }

    public function jsonSerialize(): mixed { return self::class; }
}
"#;

    fn setup_more() -> (tempfile::TempDir, ProjectIndex) {
        let (dir, _) = setup();
        let r = dir.path();
        fs::create_dir_all(r.join("app/Support")).unwrap();
        fs::create_dir_all(r.join("app/Http")).unwrap();
        fs::write(r.join("app/Support/helpers.php"), HELPERS).unwrap();
        fs::write(r.join("app/Http/Checkout.php"), RESOLVE).unwrap();
        let mut idx = ProjectIndex::new(
            r,
            IndexConfig {
                cache_file: Some(r.join("cache2.json")),
                ..Default::default()
            },
        );
        idx.index_workspace(|_, _| {});
        (dir, idx)
    }

    fn def(idx: &mut ProjectIndex, needle: &str, nth: usize) -> PhpDefinition {
        definition(
            idx,
            "app/Http/Checkout.php",
            RESOLVE,
            at(RESOLVE, needle, nth),
        )
    }

    fn first_target(d: &PhpDefinition) -> (String, String) {
        let t = d.targets.first().expect("a target");
        (t.path.clone(), t.name.clone())
    }

    #[test]
    fn resolves_imports_aliases_group_uses_and_qualified_names() {
        let (_d, mut idx) = setup_more();
        // `use X as Y`: the alias and the import line both lead to User.
        let d = def(&mut idx, "Customer $customer", 0);
        assert_eq!(
            first_target(&d),
            ("app/Models/User.php".into(), "User".into())
        );
        let d = def(&mut idx, "Models\\User as", 0);
        assert_eq!(first_target(&d).1, "User");
        // Fully qualified with a leading backslash.
        let d = def(&mut idx, "App\\Models\\User $user", 0);
        assert_eq!(first_target(&d).1, "User");
        assert_eq!(d.info.unwrap().container, "App\\Models");
        // Group use: each name of `{Address, Contact}`.
        let d = def(&mut idx, "Contact}", 0);
        assert_eq!(first_target(&d).1, "Contact");
        let d = def(&mut idx, "Address::class", 0);
        assert_eq!(first_target(&d).1, "Address");
        // `use function` / `use const` imports and their usages.
        let d = def(&mut idx, "money(1)", 0);
        assert_eq!(
            first_target(&d),
            ("app/Support/helpers.php".into(), "money()".into())
        );
        let d = def(&mut idx, "CURRENCY . PHP", 0);
        assert_eq!(first_target(&d).1, "CURRENCY");
        let t = &d.targets[0];
        assert_eq!((t.line, t.column), (3, 6));
    }

    #[test]
    fn php_builtins_are_external_symbols() {
        let (_d, mut idx) = setup_more();
        let d = def(&mut idx, "Throwable $e", 0);
        assert!(d.targets.is_empty());
        let e = d.external.unwrap();
        assert_eq!(e.classes, vec!["Throwable"]);
        assert_eq!(d.title.as_deref(), Some("Throwable"));
        // The range covers the whole qualified name for the Ctrl+hover link.
        let r = d.range.unwrap();
        assert_eq!(
            r.end.character - r.start.character,
            "\\Throwable".len() as u32
        );

        // A global function called from a namespace: namespaced first, then global.
        let e = def(&mut idx, "array_map", 0).external.unwrap();
        assert_eq!(e.functions, vec!["App\\Http\\array_map", "array_map"]);
        let e = def(&mut idx, "PHP_EOL", 0).external.unwrap();
        assert_eq!(e.constants, vec!["App\\Http\\PHP_EOL", "PHP_EOL"]);
        // `extends \RuntimeException` and `new \LogicException`.
        let e = def(&mut idx, "RuntimeException", 0).external.unwrap();
        assert_eq!(e.classes, vec!["RuntimeException"]);
        assert!(e.constants.is_empty());
        // Members of built-in classes: through a catch variable and through `$this` of a
        // class extending one.
        let e = def(&mut idx, "getMessage", 1).external.unwrap();
        assert_eq!(e.classes, vec!["Throwable"]);
        assert_eq!(
            e.member,
            Some(ExternalMember {
                kind: SymbolKind::Method,
                name: "getMessage".into()
            })
        );
        assert_eq!(e.display(), "Throwable::getMessage()");
        let e = def(&mut idx, "getMessage", 0).external.unwrap();
        assert_eq!(e.classes, vec!["RuntimeException", "JsonSerializable"]);
        // Keywords and builtin types are not navigable.
        assert!(def(&mut idx, "function run", 0).range.is_none());
        assert!(def(&mut idx, "void", 0).range.is_none());
    }

    #[test]
    fn members_variables_and_declarations() {
        let (_d, mut idx) = setup_more();
        // Through an aliased parameter type, and a property chain.
        let d = def(&mut idx, "save()", 0);
        assert_eq!(first_target(&d).1, "User::save()");
        assert_eq!(d.info.unwrap().container, "App\\Models\\User");
        let d = def(&mut idx, "city;", 0);
        assert_eq!(first_target(&d).1, "Address::$city");
        // `self::class` leads to the enclosing class.
        let d = def(&mut idx, "self::class", 0);
        assert_eq!(first_target(&d).1, "Checkout");
        // Variables go to their first assignment or parameter...
        let d = def(&mut idx, "$total;", 0);
        let t = &d.targets[0];
        assert_eq!((t.line, t.name.as_str()), (14, "$total"));
        let d = def(&mut idx, "$customer->save", 0);
        assert_eq!(d.targets[0].line, 10);
        assert_eq!(d.info.unwrap().signature, "User $customer");
        // ...and show their usages on the declaration itself.
        let d = def(&mut idx, "$total = ", 0);
        assert!(d.on_declaration);
        let u = find_usages(
            &mut idx,
            "app/Http/Checkout.php",
            RESOLVE,
            at(RESOLVE, "$total = ", 0),
        )
        .unwrap();
        assert_eq!(u.usages.len(), 2);
        assert_eq!(u.declaration.unwrap().range.start.line, 14);
        // Declarations: class and method names.
        assert!(def(&mut idx, "Checkout extends", 0).on_declaration);
        let d = def(&mut idx, "run(", 0);
        assert!(d.on_declaration);
        assert_eq!(d.title.as_deref(), Some("Checkout::run()"));
    }

    #[test]
    fn method_call_chains_use_return_types() {
        let (_d, mut idx) = setup();
        let src = "<?php\nnamespace App\\Models;\n\nfunction f(User $u) { $u->orders()->first(); $u->self()->save(); }\n";
        let user = "<?php\nnamespace App\\Models;\n\nclass User extends Model\n{\n    public function self(): static { return $this; }\n    public function orders(): Collection {}\n}\n";
        fs::write(idx.root().join("app/Models/User.php"), user).unwrap();
        idx.update_file("app/Models/User.php", user);
        let d = definition(&mut idx, "app/Models/f.php", src, at(src, "save()", 0));
        assert_eq!(first_target(&d).1, "Model::save()");
        // Unknown return type (Collection is not indexed): an external member lookup.
        let d = definition(&mut idx, "app/Models/f.php", src, at(src, "first()", 0));
        assert_eq!(
            d.external.unwrap().classes,
            vec!["App\\Models\\Collection".to_string()]
        );
    }

    #[test]
    fn finds_usages_of_php_builtins() {
        let (_d, mut idx) = setup_more();
        let u = find_usages(
            &mut idx,
            "app/Http/Checkout.php",
            RESOLVE,
            at(RESOLVE, "Throwable $e", 0),
        )
        .unwrap();
        assert_eq!(u.title, "Throwable");
        assert_eq!(u.declaration, None);
        assert_eq!(u.usages.len(), 1);
        assert_eq!(u.usages[0].range.start.line, 19);
    }

    #[test]
    fn gutter_markers_for_implementations_and_overrides() {
        let (_d, mut idx) = setup();
        let m = line_markers(&mut idx, "app/Models/Model.php", MODEL);
        let summary: Vec<(u32, MarkerKind, String)> = m
            .iter()
            .map(|m| (m.line, m.kind, m.title.clone()))
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    3,
                    MarkerKind::Subclassed,
                    "Is subclassed by User".to_string()
                ),
                (
                    5,
                    MarkerKind::Overridden,
                    "Is overridden in User".to_string()
                ),
            ]
        );
        assert_eq!(m[1].targets[0].name, "User::save()");
        assert_eq!(m[1].targets[0].line, 10);

        let m = line_markers(&mut idx, "app/Models/Contact.php", CONTACT);
        assert_eq!(m[0].kind, MarkerKind::Implemented);
        assert_eq!(m[0].title, "Is implemented in User");
        assert_eq!(m[1].kind, MarkerKind::Implemented);

        let m = line_markers(&mut idx, "app/Models/User.php", USER);
        let save: Vec<&PhpLineMarker> = m.iter().filter(|m| m.line == 10).collect();
        assert_eq!(save.len(), 1);
        assert_eq!(save[0].kind, MarkerKind::Overrides);
        assert_eq!(save[0].title, "Overrides method in Model");
        let names: Vec<&str> = save[0].targets.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["Model::save()", "Contact::save()"]);
    }
}
