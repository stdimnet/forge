//! Create symbols from usage, PHPStorm's quick fixes on an unresolved name (Alt+Enter):
//!
//! - "Create method 'foo'" for `$this->foo()`, `$order->foo()` (the receiver's class is
//!   known) and `Foo::foo()` (static), with parameters inferred from the call's arguments
//!   (types and names from variables, literals, `new`, properties and getters) and the return
//!   type from the usage where it is obvious (a statement: `void`, `return`: the enclosing
//!   return type, conditions: `bool`, `echo` / concatenation: `string`);
//! - "Create field 'x'" for `$this->x` / `$order->x` / `Foo::$x`, typed from an assigned
//!   value when it is obvious;
//! - "Create constant 'X'" for `Foo::X` / `self::X` (a class constant; "Create enum case
//!   'X'" on an enum) and for a bare `X` (a namespace constant);
//! - "Create function 'foo'" for an unknown function call;
//! - "Create class 'Foo'" ("Create interface", "Create trait" by context) for `new Foo()`,
//!   `Foo::x()`, type hints, `extends` / `implements`, `catch` and imports. This one is not
//!   an edit: the UI opens the Create New PHP Class dialog with the name, namespace and
//!   template (the file comes from the file templates and the PSR-4 directory of the
//!   namespace).
//!
//! The member goes into the right section of the target class (constants first, fields
//! after the fields, methods after the method being edited or at the end) and may live in
//! another file: then the edit is a [`WorkspaceEdit`] in LSP positions. Method bodies come
//! from the code template "PHP Implemented Method Body". Every action carries the caret
//! position after the edit and the symbol as the PHP stubs would know it, so the app can
//! drop actions for PHP's own functions and classes.

use std::collections::HashSet;

use forge_codeintel::{Caret, TextEdit as LspEdit, WorkspaceEdit};
use forge_file_templates::{is_class_reference, names as templates, render_code, Vars};
use serde_json::json;

use crate::generate::{
    apply_edits, insert_after_fields, insert_at_end, Fmt, GenerateOptions, TextEdit,
};
use crate::hover::{find_member, word_at};
use crate::index::ProjectIndex;
use crate::intentions::{Intention, IntentionKind};
use crate::model::{short_name, ClassLike, FileIndex, SymbolKind};
use crate::navigate::{
    access_before, enclosing_callable, find_class, ident_start, is_keyword, matching_open,
    receiver_types, resolve_word, unresolved_ancestors, variable_classes, ExternalMember,
    ExternalSymbol, Lines, Resolved,
};
use crate::parser;
use crate::resolve::{is_builtin_type, resolve_type, shorten_type};
use crate::style::apply_braces;

/// The UI action of "Create class": opens the Create New PHP Class dialog with `data`.
pub const CREATE_CLASS_ACTION: &str = "PhpCreateClass";

/// Caret (and selection end) markers inside generated code; removed before the edit leaves.
const CARET: char = '\u{1}';
const SELECTION_END: char = '\u{2}';

/// The file a symbol is created in, with its current text.
struct Target {
    path: String,
    text: String,
    file: FileIndex,
    /// The file being edited (its unsaved buffer): plain `edits` instead of a workspace edit.
    current: bool,
}

impl Target {
    fn load(index: &ProjectIndex, path: &str) -> Option<Self> {
        let text = std::fs::read_to_string(index.root().join(path)).ok()?;
        Some(Self {
            path: path.to_string(),
            file: parser::index_source(path, &text),
            text,
            current: false,
        })
    }

    fn class(&self, fqn: &str) -> Option<&ClassLike> {
        self.file
            .classes
            .iter()
            .find(|c| c.fqn.eq_ignore_ascii_case(fqn))
    }

    /// A resolved type (`\App\Models\Order`, `int`) as written in this file.
    fn type_name(&self, ty: &str, offset: usize) -> String {
        let ns = self.file.namespace_at(offset);
        shorten_type(ty, ns, &self.file.uses)
    }
}

/// Remove the caret markers from `text`: the text and the marked byte range in it.
fn take_markers(text: &str) -> (String, Option<(usize, usize)>) {
    let caret = text.find(CARET);
    let clean: String = text
        .chars()
        .filter(|c| *c != CARET && *c != SELECTION_END)
        .collect();
    let range = caret.map(|start| {
        let end = text
            .find(SELECTION_END)
            .map_or(start, |e| e - CARET.len_utf8());
        (start, end)
    });
    (clean, range)
}

/// The intention for one edit of `target`; the edit's text may carry caret markers.
fn intention(
    title: String,
    target: &Target,
    mut edit: TextEdit,
    unresolved: Option<ExternalSymbol>,
) -> Intention {
    let (text, marked) = take_markers(&edit.new_text);
    edit.new_text = text;
    let after = apply_edits(&target.text, std::slice::from_ref(&edit));
    let (from, to) = marked.unwrap_or((edit.new_text.len(), edit.new_text.len()));
    let lines = Lines::new(&after);
    let caret = Caret {
        path: target.path.clone(),
        range: lines.range(edit.start + from, edit.start + to),
    };
    let mut out = Intention {
        title,
        kind: IntentionKind::QuickFix,
        caret: Some(caret),
        unresolved,
        ..Default::default()
    };
    if target.current {
        out.edits = vec![edit];
    } else {
        let lines = Lines::new(&target.text);
        let mut ws = WorkspaceEdit::default();
        ws.add_edits(
            target.path.clone(),
            vec![LspEdit {
                range: lines.range(edit.start, edit.end),
                new_text: edit.new_text,
            }],
        );
        out.workspace_edit = Some(ws);
    }
    out
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn ends_with_word(text: &str, word: &str) -> bool {
    text.len() >= word.len()
        && text[text.len() - word.len()..].eq_ignore_ascii_case(word)
        && !text[..text.len() - word.len()]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

fn lcfirst(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().chain(c).collect(),
        None => String::new(),
    }
}

fn namespace_of(fqn: &str) -> &str {
    fqn.rsplit_once('\\').map_or("", |(ns, _)| ns)
}

/// Inside a comment or a string literal on the caret's line (a cheap check: names there
/// are not code).
fn in_comment_or_string(src: &str, offset: usize) -> bool {
    let line_start = src[..offset].rfind('\n').map_or(0, |i| i + 1);
    let head = &src[line_start..offset];
    let t = head.trim_start();
    if t.starts_with('*') || t.starts_with("/*") {
        return true;
    }
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut prev = ' ';
    for c in head.chars() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None => {
                if c == '\'' || c == '"' {
                    quote = Some(c);
                } else if c == '#' || (c == '/' && prev == '/') {
                    return true;
                }
            }
        }
        prev = c;
    }
    quote.is_some()
}

/// Byte index of the `)` matching the `(` at `open`, skipping strings.
fn matching_close(src: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let bytes = src.as_bytes();
    let mut i = open;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == b'\\' {
                    i += 1;
                } else if b == q {
                    quote = None;
                }
            }
            None => match b {
                b'\'' | b'"' => quote = Some(b),
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return (b == b')').then_some(i);
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    None
}

/// The top-level, comma separated parts of `src[from..to]`, as trimmed byte ranges.
fn split_top_level(src: &str, from: usize, to: usize) -> Vec<(usize, usize)> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut part = from;
    let push = |s: usize, e: usize, out: &mut Vec<(usize, usize)>| {
        let text = &src[s..e];
        let lead = text.len() - text.trim_start().len();
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            out.push((s + lead, s + lead + trimmed.len()));
        }
    };
    let mut i = from;
    while i < to {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == b'\\' {
                    i += 1;
                } else if b == q {
                    quote = None;
                }
            }
            None => match b {
                b'\'' | b'"' => quote = Some(b),
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                b',' if depth == 0 => {
                    push(part, i, &mut out);
                    part = i + 1;
                }
                _ => {}
            },
        }
        i += 1;
    }
    push(part, to, &mut out);
    out
}

/// Where the expression whose last access starts at `name_start` begins (`$a->b()->c()`).
fn chain_start(src: &str, name_start: usize) -> usize {
    let mut pos = name_start;
    loop {
        let before = src[..pos].trim_end();
        let Some((recv_end, _)) = access_before(before) else {
            return pos;
        };
        let end = src[..recv_end].trim_end().len();
        if src[..end].ends_with(')') {
            let Some(open) = matching_open(src, end - 1) else {
                return pos;
            };
            let name_end = src[..open].trim_end().len();
            let start = ident_start(src, name_end);
            pos = if start == name_end { open } else { start };
            if pos == open {
                return pos;
            }
        } else {
            let start = ident_start(src, end);
            if start == end {
                return pos;
            }
            pos = start;
        }
    }
}

/// Return type of the method or function around `offset` as written.
fn enclosing_return_type(file: &FileIndex, offset: usize) -> Option<String> {
    if let Some(m) = file.class_at(offset).and_then(|c| {
        c.members
            .iter()
            .find(|m| m.kind == SymbolKind::Method && m.range.contains(offset))
    }) {
        return m.type_hint.clone();
    }
    file.functions
        .iter()
        .find(|f| f.range.contains(offset))
        .and_then(|f| f.return_type.clone())
}

/// A resolved type (`\Fqn` or a builtin) for a type as written at `offset`.
fn resolved(file: &FileIndex, ty: &str, offset: usize, owner: Option<&str>) -> String {
    match ty.trim_start_matches('?').to_ascii_lowercase().as_str() {
        "self" | "static" | "$this" => match owner {
            Some(o) => format!("{}\\{o}", if ty.starts_with('?') { "?" } else { "" }),
            None => ty.to_string(),
        },
        _ => resolve_type(ty, file.namespace_at(offset), &file.uses),
    }
}

/// The type of the expression `src[start..end]` when it is obvious: literals, `new`, typed
/// parameters and variables, property and call chains. Classes come back as `\Fqn`.
fn expr_type(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    start: usize,
    end: usize,
    depth: u8,
) -> Option<String> {
    let start = start + (src[start..end].len() - src[start..end].trim_start().len());
    let e = src[start..end].trim();
    let lower = e.to_ascii_lowercase();
    let owner = file.class_at(start).map(|c| c.fqn.clone());
    if e.starts_with('\'') || e.starts_with('"') || lower.ends_with("::class") {
        return Some("string".into());
    }
    let num = e.trim_start_matches('-').replace('_', "");
    if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
        return Some("int".into());
    }
    if num.parse::<f64>().is_ok() && num.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return Some("float".into());
    }
    match lower.as_str() {
        "true" | "false" => return Some("bool".into()),
        "null" => return None,
        "$this" => return owner.map(|o| format!("\\{o}")),
        _ => {}
    }
    if e.starts_with('[') || lower.starts_with("array(") {
        return Some("array".into());
    }
    if e.starts_with('!') {
        return Some("bool".into());
    }
    let callable = lower.trim_start_matches("static").trim_start();
    if callable.starts_with("function")
        || callable.starts_with("fn(")
        || callable.starts_with("fn ")
    {
        return Some("callable".into());
    }
    if let Some(rest) = lower.strip_prefix("new ") {
        let at = start + e.len() - rest.len();
        let name: String = src[at..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '\\')
            .collect();
        return match name.to_ascii_lowercase().as_str() {
            "" => None,
            "self" | "static" => owner.map(|o| format!("\\{o}")),
            _ => Some(format!("\\{}", file.resolve_class(&name, at))),
        };
    }
    if let Some(var) = e.strip_prefix('$').filter(|v| is_identifier(v)) {
        // A parameter of the enclosing method or function.
        if let Some((params, _, _)) = enclosing_callable(file, start) {
            if let Some(p) = params.iter().find(|p| p.name == var) {
                return p
                    .type_hint
                    .as_deref()
                    .map(|t| resolved(file, t, start, owner.as_deref()));
            }
        }
        if let Some(class) = variable_classes(file, src, start, e).into_iter().next() {
            return Some(format!("\\{class}"));
        }
        // The last plain assignment before the usage.
        if depth == 0 {
            let scope = enclosing_callable(file, start).map_or(0, |(_, s, _)| s);
            let assign = format!("{e} = ");
            if let Some(i) = src[scope..start].rfind(&assign) {
                let value = scope + i + assign.len();
                let value_end = src[value..].find(';').map_or(value, |j| value + j);
                return expr_type(index, file, src, value, value_end, depth + 1);
            }
        }
        return None;
    }
    if e.contains("->") || e.contains("::") {
        let end = start + e.len();
        if let Some(class) = receiver_types(index, file, src, end, 0).into_iter().next() {
            return Some(format!("\\{class}"));
        }
        return member_scalar_type(index, file, src, end);
    }
    None
}

/// The builtin type (`int`, `string`) of the property or method call ending at `end`.
fn member_scalar_type(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    end: usize,
) -> Option<String> {
    let (name_end, kinds): (usize, &[SymbolKind]) = if src[..end].ends_with(')') {
        let open = matching_open(src, end - 1)?;
        (src[..open].trim_end().len(), &[SymbolKind::Method])
    } else {
        (end, &[SymbolKind::Property])
    };
    let name_start = ident_start(src, name_end);
    let name = src[name_start..name_end].trim_start_matches('$');
    let (recv_end, _) = access_before(src[..name_start].trim_end())?;
    for owner in receiver_types(index, file, src, recv_end, 0) {
        let Some(class) = find_class(index, file, &owner) else {
            continue;
        };
        if let Some((_, m)) = find_member(index, &class, name, kinds) {
            return m.effective_type().filter(|t| {
                let t = t.trim_start_matches('?').to_ascii_lowercase();
                is_builtin_type(&t) && !matches!(t.as_str(), "self" | "static" | "$this" | "void")
            });
        }
    }
    None
}

/// The parameter name an argument suggests: the variable, property or getter name, the class
/// of `new`, else the type.
fn arg_name(expr: &str, ty: Option<&str>) -> String {
    let e = expr.trim();
    if let Some(var) = e
        .strip_prefix('$')
        .filter(|v| is_identifier(v) && *v != "this")
    {
        return var.to_string();
    }
    let lower = e.to_ascii_lowercase();
    if !lower.starts_with("new ") {
        // `$order->total`, `$order->getTotal()`, `Money::zero()`.
        let tail = e.trim_end_matches(|c: char| c == ')' || c == '(' || c.is_whitespace());
        let call = e.ends_with(')') && !tail.contains('(');
        let last = tail
            .rsplit(['>', ':'])
            .next()
            .map(|s| s.trim_start_matches('$'))
            .filter(|s| is_identifier(s));
        if let (Some(name), true) = (last, e.contains("->") || e.contains("::")) {
            if !(e.ends_with(')') && !call) {
                let stripped = name
                    .strip_prefix("get")
                    .filter(|r| r.starts_with(char::is_uppercase))
                    .unwrap_or(name);
                if !stripped.eq_ignore_ascii_case("class") {
                    return lcfirst(stripped);
                }
            }
        }
    }
    match ty {
        Some(t) if t.starts_with('\\') => lcfirst(short_name(t.trim_start_matches('?'))),
        Some(t) if is_identifier(t) => t.to_string(),
        _ => "param".to_string(),
    }
}

/// Parameters for a call whose argument list is `src[open + 1..close]`, rendered for the
/// target file (`int $count`, `Order $order`, `...$rest`).
fn params_for(
    index: &mut ProjectIndex,
    cur: &Target,
    open: usize,
    close: usize,
    target: &Target,
    target_at: usize,
) -> (Vec<String>, Vec<String>) {
    let src = &cur.text;
    let mut used: HashSet<String> = HashSet::new();
    let mut params = Vec::new();
    let mut args = Vec::new();
    for (s, e) in split_top_level(src, open + 1, close) {
        let mut s = s;
        let variadic = src[s..e].starts_with("...");
        if variadic {
            s += 3;
        }
        // A named argument (`label: 'x'`).
        let mut named: Option<String> = None;
        let text = &src[s..e];
        if let Some(colon) = text.find(':') {
            let label = text[..colon].trim();
            if is_identifier(label) && !text[colon..].starts_with("::") {
                named = Some(label.to_string());
                s += colon + 1;
                s += src[s..e].len() - src[s..e].trim_start().len();
            }
        }
        let ty = expr_type(index, &cur.file, src, s, e, 0);
        let mut name = named.unwrap_or_else(|| arg_name(&src[s..e], ty.as_deref()));
        if !is_identifier(&name) || name == "this" {
            name = "param".into();
        }
        if !used.insert(name.clone()) {
            let mut n = 1;
            while !used.insert(format!("{name}{n}")) {
                n += 1;
            }
            name = format!("{name}{n}");
        }
        let ty = ty.map(|t| target.type_name(&t, target_at));
        let mut p = String::new();
        if let Some(t) = &ty {
            p.push_str(t);
            p.push(' ');
        }
        if variadic {
            p.push_str("...");
        }
        p.push('$');
        p.push_str(&name);
        params.push(p);
        args.push(format!("{}${name}", if variadic { "..." } else { "" }));
    }
    (params, args)
}

/// The return type a call at `expr_start..call_end` obviously needs, resolved.
fn usage_return_type(
    file: &FileIndex,
    src: &str,
    expr_start: usize,
    call_end: usize,
) -> Option<String> {
    let before = src[..expr_start].trim_end();
    let after = src[call_end..].trim_start();
    if ends_with_word(before, "return") {
        let owner = file.class_at(expr_start).map(|c| c.fqn.clone());
        return enclosing_return_type(file, expr_start)
            .filter(|t| !matches!(t.to_ascii_lowercase().as_str(), "void" | "never"))
            .map(|t| resolved(file, &t, expr_start, owner.as_deref()));
    }
    if after.starts_with(';')
        && (before.is_empty()
            || before.ends_with('{')
            || before.ends_with('}')
            || before.ends_with(';')
            || before.ends_with("?>")
            || (before.ends_with(':') && !before.ends_with("::")))
    {
        return Some("void".into());
    }
    let concat = |s: &str| s.starts_with('.') && !s.starts_with("..");
    if ends_with_word(before, "echo")
        || ends_with_word(before, "print")
        || (before.ends_with('.') && !before.ends_with(".."))
        || before.ends_with(".=")
        || concat(after)
    {
        return Some("string".into());
    }
    if (before.ends_with('!') && !before.ends_with("!="))
        || before.ends_with("&&")
        || before.ends_with("||")
        || after.starts_with("&&")
        || after.starts_with("||")
        || after.starts_with('?') && !after.starts_with("??") && !after.starts_with("?->")
    {
        return Some("bool".into());
    }
    if let Some(head) = before.strip_suffix('(') {
        let head = head.trim_end();
        if after.starts_with(')')
            && ["if", "elseif", "while"]
                .iter()
                .any(|k| ends_with_word(head, k))
        {
            return Some("bool".into());
        }
        if ends_with_word(head, "foreach") && after.to_ascii_lowercase().starts_with("as") {
            return Some("array".into());
        }
    }
    None
}

/// The rendered body of a new method or function (code template "PHP Implemented Method
/// Body"), 4-space indented, with the caret marker at its start.
fn body(
    opts: &GenerateOptions,
    name: &str,
    class_name: &str,
    args: &[String],
    ret: Option<&str>,
) -> String {
    let mut vars = Vars::new();
    vars.insert("NAME".into(), name.to_string());
    vars.insert("CLASS_NAME".into(), class_name.to_string());
    vars.insert("PARAMETERS".into(), args.join(", "));
    vars.insert("RETURN_TYPE".into(), ret.unwrap_or_default().to_string());
    let rendered = render_code(
        &opts.code_templates,
        templates::IMPLEMENTED_METHOD_BODY,
        &vars,
    )
    .unwrap_or_default();
    let lines: Vec<String> = rendered
        .trim_end()
        .lines()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                format!("    {l}")
            }
        })
        .collect();
    match lines.iter().position(|l| !l.is_empty()) {
        Some(first) => {
            let mut lines = lines;
            lines[first].insert(4, CARET);
            format!("{{\n{}\n}}", lines.join("\n"))
        }
        None => format!("{{\n{CARET}}}"),
    }
}

/// Insert a member block after `end` (the end of another member).
fn insert_after(fmt: &Fmt, end: usize, block: &str, separator: &str) -> TextEdit {
    TextEdit {
        start: end,
        end,
        new_text: fmt.eol(format!("{separator}{}", fmt.indent_block(block))),
    }
}

// ---- Members ----

struct MemberSite<'a> {
    target: Target,
    class: ClassLike,
    name: &'a str,
    visibility: &'static str,
    is_static: bool,
    unresolved: Option<ExternalSymbol>,
}

fn create_member(
    index: &mut ProjectIndex,
    cur: &Target,
    start: usize,
    end: usize,
    recv_end: usize,
    op: &str,
    opts: &GenerateOptions,
) -> Option<Intention> {
    let src = &cur.text;
    let file = &cur.file;
    let word = &src[start..end];
    let is_call = src[end..].trim_start().starts_with('(');
    let kind = if is_call {
        SymbolKind::Method
    } else if op == "->" {
        if word.starts_with('$') {
            return None;
        }
        SymbolKind::Property
    } else if word.starts_with('$') {
        SymbolKind::Property
    } else {
        SymbolKind::Constant
    };
    let name = word.trim_start_matches('$');
    if !is_identifier(name) || name.eq_ignore_ascii_case("class") {
        return None;
    }
    let lookup: &[SymbolKind] = match kind {
        SymbolKind::Method => &[SymbolKind::Method],
        SymbolKind::Property => &[SymbolKind::Property],
        _ => &[SymbolKind::Constant, SymbolKind::EnumCase],
    };
    let recv_start = ident_start(src, src[..recv_end].trim_end().len());
    let recv = src[recv_start..recv_end].trim().to_ascii_lowercase();
    let mut target_class: Option<ClassLike> = None;
    for fqn in receiver_types(index, file, src, recv_end, 0) {
        let Some(class) = find_class(index, file, &fqn) else {
            continue;
        };
        if find_member(index, &class, name, lookup).is_some() {
            return None;
        }
        if target_class.is_none() {
            target_class = Some(class);
        }
    }
    let class = target_class?;
    let target = if file
        .classes
        .iter()
        .any(|c| c.fqn.eq_ignore_ascii_case(&class.fqn))
    {
        Target {
            path: cur.path.clone(),
            text: cur.text.clone(),
            file: cur.file.clone(),
            current: true,
        }
    } else {
        let path = index.class_path(&class.fqn)?.to_string();
        if path.starts_with("vendor/") {
            return None;
        }
        Target::load(index, &path)?
    };
    let class = target.class(&class.fqn)?.clone();
    let external = unresolved_ancestors(index, &class);
    let unresolved = (!external.is_empty()).then(|| ExternalSymbol {
        classes: external,
        member: Some(ExternalMember {
            kind,
            name: name.to_string(),
        }),
        ..Default::default()
    });
    let own = file
        .class_at(start)
        .is_some_and(|c| c.fqn.eq_ignore_ascii_case(&class.fqn));
    let visibility = if class.kind == SymbolKind::Interface {
        "public"
    } else if recv == "parent" {
        "protected"
    } else if own && matches!(recv.as_str(), "$this" | "self" | "static") {
        "private"
    } else {
        "public"
    };
    let site = MemberSite {
        target,
        class,
        name,
        visibility,
        is_static: op == "::" && recv != "parent",
        unresolved,
    };
    match kind {
        SymbolKind::Method => create_method(index, cur, site, start, end, opts),
        SymbolKind::Property => create_field(index, cur, site, end, opts),
        _ => create_constant(site, opts),
    }
}

fn create_method(
    index: &mut ProjectIndex,
    cur: &Target,
    site: MemberSite,
    start: usize,
    end: usize,
    opts: &GenerateOptions,
) -> Option<Intention> {
    let src = &cur.text;
    let open = end + src[end..].find('(')?;
    let close = matching_close(src, open)?;
    let at = site.class.body_range.start;
    let (params, args) = params_for(index, cur, open, close, &site.target, at);
    let ret = usage_return_type(&cur.file, src, chain_start(src, start), close + 1)
        .map(|t| site.target.type_name(&t, at));
    let interface = site.class.kind == SymbolKind::Interface;
    let head = format!(
        "{}{} function {}({}){}",
        site.visibility,
        if site.is_static { " static" } else { "" },
        site.name,
        params.join(", "),
        ret.as_ref().map(|r| format!(": {r}")).unwrap_or_default()
    );
    let block = if interface {
        format!("{head};{CARET}")
    } else {
        format!(
            "{head}\n{}",
            body(opts, site.name, &site.class.name, &args, ret.as_deref())
        )
    };
    let text = &site.target.text;
    let fmt = Fmt::for_class(text, &site.class, &opts.style);
    // After the method being edited when it is in the same class, else at the end.
    let after_method = site
        .class
        .methods()
        .find(|m| site.target.current && m.range.contains(start))
        .map(|m| m.range.end);
    let edit = match after_method {
        Some(end) => insert_after(&fmt, end, &block, &fmt.member_separator()),
        None => insert_at_end(text, &site.class, &fmt, &[block]),
    };
    Some(intention(
        format!("Create method '{}'", site.name),
        &site.target,
        edit,
        site.unresolved,
    ))
}

fn create_field(
    index: &mut ProjectIndex,
    cur: &Target,
    site: MemberSite,
    end: usize,
    opts: &GenerateOptions,
) -> Option<Intention> {
    if matches!(site.class.kind, SymbolKind::Interface | SymbolKind::Enum) {
        return None;
    }
    let src = &cur.text;
    let rest = src[end..].trim_start();
    let value_at = end + (src[end..].len() - rest.len());
    let ty = if rest.starts_with("[]") {
        Some("array".to_string())
    } else if rest.starts_with('=') && !rest.starts_with("==") && !rest.starts_with("=>") {
        let from = value_at + 1;
        let to = split_top_level(
            src,
            from,
            src[from..].find(';').map_or(src.len(), |i| from + i),
        )
        .first()
        .map_or(from, |(_, e)| *e);
        expr_type(index, &cur.file, src, from, to, 0)
    } else {
        None
    };
    let at = site.class.body_range.start;
    let ty = ty.map(|t| site.target.type_name(&t, at));
    let decl = format!(
        "{}{} {}${};{CARET}",
        site.visibility,
        if site.is_static { " static" } else { "" },
        ty.map(|t| format!("{t} ")).unwrap_or_default(),
        site.name
    );
    let text = &site.target.text;
    let fmt = Fmt::for_class(text, &site.class, &opts.style);
    // Next to the other fields, else where Generate puts them.
    let last_field = site
        .class
        .members
        .iter()
        .filter(|m| m.kind == SymbolKind::Property && !m.promoted)
        .map(|m| m.range.end)
        .max();
    let edit = match last_field {
        Some(end) => insert_after(&fmt, end, &decl, "\n"),
        None => insert_after_fields(text, &site.class, &fmt, &decl),
    };
    Some(intention(
        format!("Create field '{}'", site.name),
        &site.target,
        edit,
        site.unresolved,
    ))
}

fn create_constant(site: MemberSite, opts: &GenerateOptions) -> Option<Intention> {
    let class = &site.class;
    let (title, decl) = if class.kind == SymbolKind::Enum {
        let decl = match class.backing_type.as_deref() {
            Some("string") => format!("case {0} = '{CARET}{0}{SELECTION_END}';", site.name),
            Some(_) => format!("case {} = {CARET}0{SELECTION_END};", site.name),
            None => format!("case {};{CARET}", site.name),
        };
        (format!("Create enum case '{}'", site.name), decl)
    } else {
        (
            format!("Create constant '{}'", site.name),
            format!(
                "{} const {} = {CARET}null{SELECTION_END};",
                site.visibility, site.name
            ),
        )
    };
    let text = &site.target.text;
    let fmt = Fmt::for_class(text, class, &opts.style);
    let last = class
        .members
        .iter()
        .filter(|m| matches!(m.kind, SymbolKind::Constant | SymbolKind::EnumCase))
        .map(|m| m.range.end)
        .max();
    let open = class.body_range.start;
    let close = class.body_range.end - 1;
    let edit = match last {
        Some(end) => insert_after(&fmt, end, &decl, "\n"),
        None if text[open + 1..close].trim().is_empty() => {
            insert_at_end(text, class, &fmt, &[decl])
        }
        None => TextEdit {
            start: open + 1,
            end: open + 1,
            new_text: fmt.eol(format!("\n{}\n", fmt.indent_block(&decl))),
        },
    };
    Some(intention(title, &site.target, edit, site.unresolved))
}

// ---- Functions, namespace constants and classes ----

/// The namespace a new function or constant goes to: the current one, unless the name is
/// qualified with another.
fn same_namespace(file: &FileIndex, fqn: &str, offset: usize) -> bool {
    namespace_of(fqn).eq_ignore_ascii_case(file.namespace_at(offset))
}

fn create_function(
    index: &mut ProjectIndex,
    cur: &Target,
    e: &ExternalSymbol,
    start: usize,
    end: usize,
    opts: &GenerateOptions,
) -> Option<Intention> {
    let src = &cur.text;
    let file = &cur.file;
    let fqn = e.functions.first()?;
    if !same_namespace(file, fqn, start) {
        return None;
    }
    let name = short_name(fqn);
    let open = end + src[end..].find('(')?;
    let close = matching_close(src, open)?;
    let (params, args) = params_for(index, cur, open, close, cur, start);
    let ret = usage_return_type(file, src, start, close + 1).map(|t| cur.type_name(&t, start));
    let head = format!(
        "function {name}({}){}",
        params.join(", "),
        ret.as_ref().map(|r| format!(": {r}")).unwrap_or_default()
    );
    let block = apply_braces(
        &format!("{head}\n{}", body(opts, name, "", &args, ret.as_deref())),
        opts.style.braces,
    );
    let crlf = src.contains("\r\n");
    let eol = |s: String| if crlf { s.replace('\n', "\r\n") } else { s };
    // After the function or class the usage is in, else at the end of the file.
    let after = file
        .functions
        .iter()
        .find(|f| f.range.contains(start))
        .map(|f| f.range.end)
        .or_else(|| {
            file.classes
                .iter()
                .find(|c| c.range.contains(start))
                .map(|c| c.range.end)
        });
    let edit = match after {
        Some(at) => TextEdit {
            start: at,
            end: at,
            new_text: eol(format!("\n\n{block}")),
        },
        None => {
            let at = src.trim_end().len();
            let tail = if src.len() > at { "" } else { "\n" };
            TextEdit {
                start: at,
                end: at,
                new_text: eol(format!("\n\n{block}{tail}")),
            }
        }
    };
    let unresolved = ExternalSymbol {
        functions: e.functions.clone(),
        ..Default::default()
    };
    Some(intention(
        format!("Create function '{name}'"),
        cur,
        edit,
        Some(unresolved),
    ))
}

fn create_global_constant(cur: &Target, e: &ExternalSymbol, start: usize) -> Option<Intention> {
    let src = &cur.text;
    let file = &cur.file;
    let fqn = e.constants.first()?;
    let name = short_name(fqn);
    let looks_constant =
        name.chars().any(|c| c.is_uppercase()) && !name.chars().any(|c| c.is_lowercase());
    if !looks_constant || !same_namespace(file, fqn, start) {
        return None;
    }
    let statement_end = |from: usize| {
        src[from..]
            .find(';')
            .map(|i| from + i + 1)
            .unwrap_or(src.len())
    };
    let at = if let Some(last) = file
        .uses
        .iter()
        .filter(|u| u.range.end <= start)
        .map(|u| u.range.end)
        .max()
    {
        statement_end(last)
    } else if let Some(ns) = file.namespaces.iter().rfind(|n| n.range.start <= start) {
        statement_end(ns.range.start)
    } else {
        src.find('\n').unwrap_or(src.len())
    };
    let crlf = src.contains("\r\n");
    let text = format!("\n\nconst {name} = {CARET}null{SELECTION_END};");
    let edit = TextEdit {
        start: at,
        end: at,
        new_text: if crlf {
            text.replace('\n', "\r\n")
        } else {
            text
        },
    };
    let unresolved = ExternalSymbol {
        constants: e.constants.clone(),
        ..Default::default()
    };
    Some(intention(
        format!("Create constant '{name}'"),
        cur,
        edit,
        Some(unresolved),
    ))
}

/// The class-like a reference at `start` needs: `PHP Interface` after `implements` (and in
/// an interface's `extends`), `PHP Trait` for a `use` in a class body, else `PHP Class`.
fn template_at(src: &str, file: &FileIndex, start: usize) -> &'static str {
    let head = statement_words(src, start);
    let has = |w: &str| head.iter().any(|h| h.trim_end_matches(',') == w);
    let in_body = file
        .class_at(start)
        .is_some_and(|c| start > c.body_range.start);
    if has("implements") || (head.first().is_some_and(|w| w == "interface") && has("extends")) {
        templates::PHP_INTERFACE
    } else if (head.first().is_some_and(|w| w == "use") && in_body) || has("insteadof") {
        templates::PHP_TRAIT
    } else {
        templates::PHP_CLASS
    }
}

/// The lowercase words of the statement before `start`.
fn statement_words(src: &str, start: usize) -> Vec<String> {
    let stmt = src[..start].rfind([';', '{', '}']).map_or(0, |i| i + 1);
    src[stmt..start]
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect()
}

/// `throw new X`, `catch (X $e)`: the new class is an exception.
fn is_exception_context(src: &str, start: usize) -> bool {
    let line_start = src[..start].rfind([';', '{', '}']).map_or(0, |i| i + 1);
    let head = src[line_start..start].to_ascii_lowercase();
    let words: Vec<&str> = head.split_whitespace().collect();
    if words.ends_with(&["throw", "new"]) {
        return true;
    }
    match head.rfind("catch") {
        Some(i) => head[i..].contains('(') && !head[i..].contains(')'),
        None => false,
    }
}

fn create_class(cur: &Target, e: &ExternalSymbol, start: usize, end: usize) -> Option<Intention> {
    let src = &cur.text;
    let file = &cur.file;
    let fqn = e.classes.first()?.trim_start_matches('\\');
    let name = short_name(fqn);
    if !is_class_reference(fqn) || is_builtin_type(name) || is_keyword(name) {
        return None;
    }
    let template = template_at(src, file, start);
    let word = &src[start..end];
    let in_import = file
        .uses
        .iter()
        .any(|u| u.range.start <= start && end <= u.range.end);
    let label = match template {
        templates::PHP_INTERFACE => "interface",
        templates::PHP_TRAIT => "trait",
        _ => "class",
    };
    let extends = (template == templates::PHP_CLASS && is_exception_context(src, start))
        .then_some("\\Exception");
    Some(Intention {
        title: format!("Create {label} '{name}'"),
        kind: IntentionKind::QuickFix,
        action: Some(CREATE_CLASS_ACTION.into()),
        data: Some(json!({
            "name": name,
            "namespace": namespace_of(fqn),
            "template": template,
            "extends": extends,
            // The usage needs an import when the class lands in another namespace.
            "import": !word.contains('\\') && !in_import,
            "path": cur.path,
        })),
        unresolved: Some(ExternalSymbol {
            classes: vec![fqn.to_string()],
            ..Default::default()
        }),
        ..Default::default()
    })
}

/// The "Create ..." quick fixes for the unresolved name at `offset` (bytes) of `src`.
pub fn create_from_usage(
    index: &mut ProjectIndex,
    file: &FileIndex,
    rel: &str,
    src: &str,
    offset: usize,
    opts: &GenerateOptions,
) -> Vec<Intention> {
    let Some((start, end)) = word_at(src, offset) else {
        return vec![];
    };
    if in_comment_or_string(src, start) {
        return vec![];
    }
    let cur = Target {
        path: rel.to_string(),
        text: src.to_string(),
        file: file.clone(),
        current: true,
    };
    let before = src[..start].trim_end();
    if let Some((recv_end, op)) = access_before(before) {
        return create_member(index, &cur, start, end, recv_end, op, opts)
            .into_iter()
            .collect();
    }
    let Resolved::External(e) = resolve_word(index, file, src, start, end) else {
        return vec![];
    };
    if e.member.is_some() {
        return vec![];
    }
    // A class-like in a declaration head (`implements A, B`) or a trait `use`, where the
    // index cannot tell a class from a constant.
    let words = statement_words(src, start);
    let header = template_at(src, file, start) != templates::PHP_CLASS
        || words.iter().any(|w| w == "extends");
    let created = if !e.functions.is_empty() {
        create_function(index, &cur, &e, start, end, opts)
    } else if !e.constants.is_empty() && !header {
        create_global_constant(&cur, &e, start)
    } else if !e.classes.is_empty() {
        create_class(&cur, &e, start, end)
    } else {
        None
    };
    created.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IndexConfig;
    use std::fs;

    fn index_with(files: &[(&str, &str)]) -> (tempfile::TempDir, ProjectIndex) {
        let dir = tempfile::tempdir().unwrap();
        for (p, t) in files {
            let path = dir.path().join(p);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, t).unwrap();
        }
        let mut idx = ProjectIndex::new(
            dir.path(),
            IndexConfig {
                cache_file: Some(dir.path().join("cache.json")),
                ..Default::default()
            },
        );
        idx.index_workspace(|_, _| {});
        (dir, idx)
    }

    fn run(idx: &mut ProjectIndex, rel: &str, src: &str, at: &str) -> Vec<Intention> {
        let offset = src.find(at).unwrap_or_else(|| panic!("{at} not in source"));
        let file = parser::index_source(rel, src);
        create_from_usage(idx, &file, rel, src, offset, &GenerateOptions::default())
    }

    fn one(idx: &mut ProjectIndex, rel: &str, src: &str, at: &str) -> Intention {
        let mut found = run(idx, rel, src, at);
        assert_eq!(
            found.len(),
            1,
            "{:?}",
            found.iter().map(|i| &i.title).collect::<Vec<_>>()
        );
        found.remove(0)
    }

    /// The text after the intention's current-file edits and the caret's line text.
    fn applied(src: &str, i: &Intention) -> String {
        apply_edits(src, &i.edits)
    }

    const ORDER: &str = "<?php\nnamespace App\\Models;\n\nclass Order\n{\n    private int $total = 0;\n\n    public function total(): int\n    {\n        return $this->total;\n    }\n}\n";

    #[test]
    fn creates_a_private_method_after_the_current_one() {
        let (_d, mut idx) = index_with(&[("app/Models/Order.php", ORDER)]);
        let src = "<?php\nnamespace App\\Services;\n\nuse App\\Models\\Order;\n\nclass Checkout\n{\n    private float $rate = 0.19;\n\n    public function run(Order $order, int $count): void\n    {\n        $label = 'x';\n        $this->settle($order, $count, $label, 3, new Order(), $order->total(), [1]);\n    }\n\n    public function other() {}\n}\n";
        let i = one(&mut idx, "app/Services/Checkout.php", src, "settle(");
        assert_eq!(i.title, "Create method 'settle'");
        assert_eq!(i.kind, IntentionKind::QuickFix);
        let out = applied(src, &i);
        assert!(
            out.contains("    }\n\n    private function settle(Order $order, int $count, string $label, int $int, Order $order1, int $total, array $array): void\n    {\n        // TODO: Implement settle() method.\n    }\n\n    public function other() {}"),
            "{out}"
        );
        // The caret is at the start of the body.
        let caret = i.caret.unwrap();
        assert_eq!(caret.path, "app/Services/Checkout.php");
        let line = out.lines().nth(caret.range.start.line as usize).unwrap();
        assert_eq!(
            &line[caret.range.start.character as usize..],
            "// TODO: Implement settle() method."
        );
    }

    #[test]
    fn creates_methods_in_other_files_as_workspace_edits() {
        let (_d, mut idx) = index_with(&[("app/Models/Order.php", ORDER)]);
        let src = "<?php\nnamespace App\\Http;\n\nuse App\\Models\\Order;\n\nfunction show(Order $order): bool\n{\n    return $order->isPaid();\n}\n";
        let i = one(&mut idx, "app/Http/show.php", src, "isPaid");
        assert!(i.edits.is_empty());
        let ws = i.workspace_edit.expect("edit of Order.php");
        let edits = ws.edits_of("app/Models/Order.php");
        assert_eq!(edits.len(), 1);
        let out = forge_codeintel::apply_text_edits(ORDER, &edits).unwrap();
        assert!(
            out.contains("        return $this->total;\n    }\n\n    public function isPaid(): bool\n    {\n        // TODO: Implement isPaid() method.\n    }\n}\n"),
            "{out}"
        );
        assert_eq!(i.caret.unwrap().path, "app/Models/Order.php");
        // Existing members get nothing.
        assert!(run(
            &mut idx,
            "app/Http/show.php",
            src.replace("isPaid", "total").as_str(),
            "total()"
        )
        .is_empty());
    }

    #[test]
    fn static_calls_return_types_and_interfaces() {
        let (_d, mut idx) = index_with(&[
            ("app/Models/Order.php", ORDER),
            (
                "app/Contracts/Pays.php",
                "<?php\nnamespace App\\Contracts;\n\ninterface Pays\n{\n}\n",
            ),
        ]);
        let src = "<?php\nnamespace App;\n\nuse App\\Models\\Order;\nuse App\\Contracts\\Pays;\n\nfunction f(Pays $p): string\n{\n    if (Order::exists(5)) {\n        echo $p->label();\n    }\n    return Order::make();\n}\n";
        let i = one(&mut idx, "app/f.php", src, "exists");
        let out = forge_codeintel::apply_text_edits(
            ORDER,
            &i.workspace_edit.unwrap().edits_of("app/Models/Order.php"),
        )
        .unwrap();
        assert!(
            out.contains("public static function exists(int $int): bool\n"),
            "{out}"
        );

        let i = one(&mut idx, "app/f.php", src, "make");
        let out = forge_codeintel::apply_text_edits(
            ORDER,
            &i.workspace_edit.unwrap().edits_of("app/Models/Order.php"),
        )
        .unwrap();
        assert!(
            out.contains("public static function make(): string\n"),
            "{out}"
        );

        let pays = "<?php\nnamespace App\\Contracts;\n\ninterface Pays\n{\n}\n";
        let i = one(&mut idx, "app/f.php", src, "label");
        let out = forge_codeintel::apply_text_edits(
            pays,
            &i.workspace_edit.unwrap().edits_of("app/Contracts/Pays.php"),
        )
        .unwrap();
        assert_eq!(
            out,
            "<?php\nnamespace App\\Contracts;\n\ninterface Pays\n{\n    public function label(): string;\n}\n"
        );
    }

    #[test]
    fn creates_typed_fields_after_the_fields() {
        let (_d, mut idx) = index_with(&[]);
        let src = "<?php\nclass Cart\n{\n    const MAX = 3;\n    private array $items = [];\n\n    public function add(): void\n    {\n        $this->count = 0;\n        $this->owner = new Cart();\n        $this->note;\n        self::$instances[] = $this;\n    }\n}\n";
        let i = one(&mut idx, "Cart.php", src, "count =");
        assert_eq!(i.title, "Create field 'count'");
        assert!(applied(src, &i).contains(
            "    private array $items = [];\n    private int $count;\n\n    public function add()"
        ));
        let i = one(&mut idx, "Cart.php", src, "owner");
        assert!(
            applied(src, &i).contains("    private Cart $owner;\n"),
            "{}",
            applied(src, &i)
        );
        let i = one(&mut idx, "Cart.php", src, "note");
        assert!(applied(src, &i).contains("    private $note;\n"));
        let i = one(&mut idx, "Cart.php", src, "instances");
        assert!(applied(src, &i).contains("    private static array $instances;\n"));
        assert!(run(&mut idx, "Cart.php", src, "items").is_empty());
    }

    #[test]
    fn creates_class_constants_and_enum_cases() {
        let (_d, mut idx) = index_with(&[(
            "app/Status.php",
            "<?php\nnamespace App;\n\nenum Status: string\n{\n    case Open = 'open';\n}\n",
        )]);
        let src = "<?php\nnamespace App;\n\nclass Shop\n{\n    private $a;\n\n    public function f()\n    {\n        return [self::LIMIT, Status::Closed, Status::Open];\n    }\n}\n";
        let i = one(&mut idx, "app/Shop.php", src, "LIMIT");
        assert_eq!(i.title, "Create constant 'LIMIT'");
        let out = applied(src, &i);
        assert!(
            out.contains("class Shop\n{\n    private const LIMIT = null;\n\n    private $a;"),
            "{out}"
        );
        // The value is selected.
        let caret = i.caret.unwrap();
        assert_eq!(caret.range.end.character - caret.range.start.character, 4);

        let i = one(&mut idx, "app/Shop.php", src, "Closed");
        assert_eq!(i.title, "Create enum case 'Closed'");
        let status =
            "<?php\nnamespace App;\n\nenum Status: string\n{\n    case Open = 'open';\n}\n";
        let out = forge_codeintel::apply_text_edits(
            status,
            &i.workspace_edit.unwrap().edits_of("app/Status.php"),
        )
        .unwrap();
        assert!(
            out.contains("    case Open = 'open';\n    case Closed = 'Closed';\n}"),
            "{out}"
        );
        assert!(run(&mut idx, "app/Shop.php", src, "Open]").is_empty());
    }

    #[test]
    fn creates_functions_and_namespace_constants() {
        let (_d, mut idx) = index_with(&[]);
        let src = "<?php\nnamespace App\\Support;\n\nuse App\\Models\\Order;\n\nfunction total(Order $order): string\n{\n    $sum = format_money($order, 2.5);\n    return VERSION;\n}\n";
        let i = one(&mut idx, "app/Support/helpers.php", src, "format_money");
        assert_eq!(i.title, "Create function 'format_money'");
        let out = applied(src, &i);
        assert!(
            out.ends_with("}\n\nfunction format_money(Order $order, float $float)\n{\n    // TODO: Implement format_money() method.\n}\n"),
            "{out}"
        );
        assert_eq!(
            i.unresolved.as_ref().unwrap().functions,
            ["App\\Support\\format_money", "format_money"]
        );

        let i = one(&mut idx, "app/Support/helpers.php", src, "VERSION");
        assert_eq!(i.title, "Create constant 'VERSION'");
        assert!(applied(src, &i)
            .contains("use App\\Models\\Order;\n\nconst VERSION = null;\n\nfunction total"));

        // Calls in other namespaces, keywords and lowercase bare words are left alone.
        let src = "<?php\nnamespace App;\n\nOther\\run();\nif (isset($a)) { echo foo; }\n";
        assert!(run(&mut idx, "a.php", src, "run").is_empty());
        assert!(run(&mut idx, "a.php", src, "isset").is_empty());
        assert!(run(&mut idx, "a.php", src, "foo;").is_empty());
    }

    #[test]
    fn offers_the_class_dialog_by_context() {
        let (_d, mut idx) = index_with(&[]);
        let src = "<?php\nnamespace App\\Http;\n\nuse App\\Models\\Invoice;\n\nclass Pay extends Base implements Payable, \\App\\Contracts\\Refunds\n{\n    use Logs;\n\n    public function f(Money $m)\n    {\n        try {\n            throw new PaymentFailed();\n        } catch (Declined $e) {\n        }\n    }\n}\n";
        let class = |at: &str, idx: &mut ProjectIndex| one(idx, "app/Http/Pay.php", src, at);
        let i = class("Base", &mut idx);
        assert_eq!(i.title, "Create class 'Base'");
        assert_eq!(i.action.as_deref(), Some(CREATE_CLASS_ACTION));
        let data = i.data.unwrap();
        assert_eq!(data["name"], "Base");
        assert_eq!(data["namespace"], "App\\Http");
        assert_eq!(data["template"], "PHP Class");
        assert_eq!(data["import"], true);
        assert_eq!(data["extends"], serde_json::Value::Null);

        assert_eq!(
            class("Payable", &mut idx).title,
            "Create interface 'Payable'"
        );
        let refunds = class("Refunds", &mut idx);
        assert_eq!(refunds.title, "Create interface 'Refunds'");
        let data = refunds.data.unwrap();
        assert_eq!(data["namespace"], "App\\Contracts");
        assert_eq!(data["import"], false);
        assert_eq!(class("Logs", &mut idx).title, "Create trait 'Logs'");
        assert_eq!(class("Money", &mut idx).title, "Create class 'Money'");
        let exc = class("PaymentFailed", &mut idx);
        assert_eq!(exc.data.unwrap()["extends"], "\\Exception");
        assert_eq!(
            class("Declined", &mut idx).data.unwrap()["extends"],
            "\\Exception"
        );
        let import = class("Invoice", &mut idx);
        assert_eq!(import.data.as_ref().unwrap()["namespace"], "App\\Models");
        assert_eq!(import.data.unwrap()["import"], false);
    }

    #[test]
    fn nothing_in_comments_strings_or_unknown_receivers() {
        let (_d, mut idx) = index_with(&[]);
        let src =
            "<?php\n// $this->foo();\n$x = 'bar()';\n$y->baz();\nfunction g() { return 1; }\n";
        assert!(run(&mut idx, "a.php", src, "foo").is_empty());
        assert!(run(&mut idx, "a.php", src, "bar").is_empty());
        assert!(run(&mut idx, "a.php", src, "baz").is_empty());
        assert!(run(&mut idx, "a.php", src, "g()").is_empty());
    }

    #[test]
    fn named_and_spread_arguments_and_parent_calls() {
        let (_d, mut idx) = index_with(&[(
            "app/Base.php",
            "<?php\nnamespace App;\n\nabstract class Base extends \\Exception\n{\n}\n",
        )]);
        let src = "<?php\nnamespace App;\n\nclass Child extends Base\n{\n    public function f(string ...$tags)\n    {\n        parent::boot(label: 'x', ...$tags);\n    }\n}\n";
        let i = one(&mut idx, "app/Child.php", src, "boot");
        let out = forge_codeintel::apply_text_edits(
            "<?php\nnamespace App;\n\nabstract class Base extends \\Exception\n{\n}\n",
            &i.workspace_edit.unwrap().edits_of("app/Base.php"),
        )
        .unwrap();
        assert!(
            out.contains("protected function boot(string $label, string ...$tags): void\n"),
            "{out}"
        );
        // `\Exception` may declare it: the stubs decide.
        let u = i.unresolved.unwrap();
        assert_eq!(u.classes, ["Exception"]);
        assert_eq!(u.member.unwrap().name, "boot");
    }
}
