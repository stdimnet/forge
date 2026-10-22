//! PHP inspections from the index, PHPStorm metadata and JetBrains attributes (Forge's own,
//! next to the language server's diagnostics):
//!
//! - Deprecated symbols (`#[Deprecated]`, `#[\Deprecated]`, `@deprecated`): struck through,
//!   with the quick-fix "Replace with …" when the attribute names a `replacement`.
//! - Expected values (`#[ExpectedValues]`, `expectedArguments()`): a literal argument that
//!   is not one of them.
//! - `#[Immutable]` properties written outside their write scope.
//! - Unused results of `#[Pure]` functions.
//! - Unreachable statements after calls that never return (`#[NoReturn]`, `exitPoint()`).
//! - "Namespace name doesn't match PSR-0/PSR-4 project structure" with the quick-fix "Set
//!   namespace to …" (source roots from composer.json and Mark Directory as).
//!
//! Offsets are bytes; [`crate::service::inspect`] converts them to UTF-16.

use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use crate::attributes::{
    self, deprecated, immutable, qualify_value, render_replacement, string_literal,
    DeprecationInfo, WriteScope,
};
use crate::callsite::{callee_decl, Callee, Decl};
use crate::complete::expected_argument_values;
use crate::generate::TextEdit;
use crate::hover::find_member;
use crate::index::ProjectIndex;
use crate::model::{short_name, ClassLike, FileIndex, Member, SymbolKind};
use crate::navigate::{class_chain, find_class, resolve_word, Resolved, SymbolRef};
use crate::parser;

/// Severity, as the editor shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProblemSeverity {
    Error,
    Warning,
    /// A weak warning.
    Weak,
}

/// A quick-fix of a problem (Alt+Enter).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemFix {
    pub title: String,
    pub edits: Vec<TextEdit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpProblem {
    pub start: usize,
    pub end: usize,
    pub severity: ProblemSeverity,
    pub message: String,
    /// The inspection: `deprecated`, `expectedValues`, `immutable`, `pureFunction`,
    /// `unreachable`, `psr4Namespace`.
    pub code: String,
    /// Struck through.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deprecated: bool,
    /// Greyed out (unreachable code).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unnecessary: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<ProblemFix>,
}

/// Upper bound of the calls one run looks at.
const MAX_SITES: usize = 4000;

struct Ctx<'a> {
    index: &'a mut ProjectIndex,
    file: FileIndex,
    rel: &'a str,
    src: &'a str,
    out: Vec<PhpProblem>,
    sites: usize,
}

/// What a call or name resolved to.
struct Target {
    callee: Option<Callee>,
    decl: Option<Decl>,
    /// `Method 'settle'`, `Function 'old'`, `Class 'Order'`.
    label: String,
    deprecation: Option<DeprecationInfo>,
    no_return: bool,
    pure: bool,
}

fn deprecation(
    attrs: &[crate::model::Attribute],
    doc: Option<&crate::docblock::DocBlock>,
) -> Option<DeprecationInfo> {
    deprecated(attrs).or_else(|| {
        doc.and_then(|d| d.deprecated.clone())
            .map(|reason| DeprecationInfo {
                reason,
                ..Default::default()
            })
    })
}

fn member_label(m: &Member) -> String {
    match m.kind {
        SymbolKind::Method => format!("Method '{}'", m.name),
        SymbolKind::Property => format!("Property '${}'", m.name),
        SymbolKind::EnumCase => format!("Enum case '{}'", m.name),
        _ => format!("Constant '{}'", m.name),
    }
}

impl<'a> Ctx<'a> {
    fn text(&self, n: Node) -> &'a str {
        &self.src[n.byte_range()]
    }

    fn resolve(&mut self, n: Node) -> Resolved {
        let (s, e) = (n.start_byte(), n.end_byte());
        let file = self.file.clone();
        resolve_word(self.index, &file, self.src, s, e)
    }

    fn member(&mut self, class: &str, kind: SymbolKind, name: &str) -> Option<(ClassLike, Member)> {
        let file = self.file.clone();
        let c = find_class(self.index, &file, class)?;
        let kinds: &[SymbolKind] = match kind {
            SymbolKind::Constant | SymbolKind::EnumCase => {
                &[SymbolKind::Constant, SymbolKind::EnumCase]
            }
            SymbolKind::Method => &[SymbolKind::Method],
            _ => &[SymbolKind::Property],
        };
        find_member(self.index, &c, name, kinds)
    }

    /// What the name node `n` (of a call, `new`, access or type) refers to.
    fn target(&mut self, n: Node, constructor: bool) -> Option<Target> {
        let file = self.file.clone();
        match self.resolve(n) {
            Resolved::Symbol(SymbolRef::Function { fqn }) => {
                let callee = Callee::Function(vec![fqn.clone()]);
                let decl = callee_decl(self.index, &file, &callee);
                let (attrs, doc) = match &decl {
                    Some(Decl::Function(f)) => (f.attributes.clone(), f.doc.clone()),
                    _ => (Vec::new(), None),
                };
                Some(Target {
                    label: format!("Function '{}'", short_name(&fqn)),
                    deprecation: deprecation(&attrs, doc.as_ref()),
                    no_return: attributes::has(&attrs, "NoReturn"),
                    pure: attributes::has(&attrs, "Pure"),
                    callee: Some(callee),
                    decl,
                })
            }
            Resolved::External(e) if !e.functions.is_empty() && e.member.is_none() => {
                Some(Target {
                    label: format!("Function '{}'", short_name(&e.functions[0])),
                    callee: Some(Callee::Function(e.functions)),
                    decl: None,
                    deprecation: None,
                    no_return: false,
                    pure: false,
                })
            }
            Resolved::Symbol(SymbolRef::Member {
                class,
                member,
                name,
            }) => {
                let (owner, m) = self.member(&class, member, &name)?;
                let callee = (member == SymbolKind::Method).then(|| Callee::Method {
                    classes: class_chain(self.index, &file, &owner.fqn),
                    name: m.name.clone(),
                });
                let decl = (member == SymbolKind::Method).then(|| Decl::Method {
                    owner: owner.clone(),
                    member: m.clone(),
                });
                Some(Target {
                    label: member_label(&m),
                    deprecation: deprecation(&m.attributes, m.doc.as_ref()),
                    no_return: attributes::has(&m.attributes, "NoReturn"),
                    pure: attributes::has(&m.attributes, "Pure"),
                    callee,
                    decl,
                })
            }
            Resolved::Symbol(SymbolRef::Class { fqn }) => {
                let class = find_class(self.index, &file, &fqn)?;
                let mut t = Target {
                    label: format!("Class '{}'", class.name),
                    deprecation: deprecation(&class.attributes, class.doc.as_ref()),
                    callee: None,
                    decl: None,
                    no_return: false,
                    pure: false,
                };
                if constructor {
                    t.callee = Some(Callee::Constructor {
                        classes: class_chain(self.index, &file, &class.fqn),
                    });
                    if let Some((owner, m)) =
                        find_member(self.index, &class, "__construct", &[SymbolKind::Method])
                    {
                        // A deprecated constructor counts like a deprecated class.
                        if t.deprecation.is_none() {
                            if let Some(d) = deprecation(&m.attributes, m.doc.as_ref()) {
                                t.deprecation = Some(d);
                                t.label = format!("Constructor of '{}'", class.name);
                            }
                        }
                        t.decl = Some(Decl::Method { owner, member: m });
                    }
                }
                Some(t)
            }
            Resolved::Symbol(SymbolRef::Constant { fqn }) => {
                let doc = self
                    .index
                    .files()
                    .flat_map(|f| f.constants.iter())
                    .chain(file.constants.iter())
                    .find(|k| k.fqn == fqn)
                    .and_then(|k| k.doc.clone());
                Some(Target {
                    label: format!("Constant '{}'", short_name(&fqn)),
                    deprecation: deprecation(&[], doc.as_ref()),
                    callee: None,
                    decl: None,
                    no_return: false,
                    pure: false,
                })
            }
            _ => None,
        }
    }

    fn deprecated_problem(&mut self, name: Node, t: &Target, call: Option<(Node, Option<Node>)>) {
        let Some(d) = &t.deprecation else {
            return;
        };
        let mut message = format!("{} is deprecated", t.label);
        if !d.reason.is_empty() {
            message.push_str(": ");
            message.push_str(&d.reason);
        }
        if let Some(since) = &d.since {
            message.push_str(&format!(" (since {since})"));
        }
        // "Replace with …" for calls.
        let fix = match (&d.replacement, call) {
            (Some(template), Some((call, receiver))) => {
                let args: Vec<&str> = call
                    .child_by_field_name("arguments")
                    .map(|a| {
                        let mut c = a.walk();
                        a.named_children(&mut c)
                            .filter(|x| x.kind() == "argument")
                            .map(|x| self.text(x))
                            .collect()
                    })
                    .unwrap_or_default();
                let class = receiver.map(|r| self.text(r));
                let new_text = render_replacement(template, class, self.text(name), &args);
                Some(ProblemFix {
                    title: format!("Replace with '{}'", new_text),
                    edits: vec![TextEdit {
                        start: call.start_byte(),
                        end: call.end_byte(),
                        new_text,
                    }],
                })
            }
            _ => None,
        };
        self.out.push(PhpProblem {
            start: name.start_byte(),
            end: name.end_byte(),
            severity: ProblemSeverity::Weak,
            message,
            code: "deprecated".into(),
            deprecated: true,
            unnecessary: false,
            fix,
        });
    }

    /// A literal argument that is not one of the expected values.
    fn check_arguments(&mut self, call: Node, t: &Target) {
        let Some(callee) = &t.callee else {
            return;
        };
        let Some(args) = call.child_by_field_name("arguments") else {
            return;
        };
        let mut c = args.walk();
        let list: Vec<Node> = args
            .named_children(&mut c)
            .filter(|x| x.kind() == "argument")
            .collect();
        let file = self.file.clone();
        for (i, arg) in list.iter().enumerate() {
            // Named arguments count by their parameter's position.
            let n = match arg.child_by_field_name("name") {
                Some(name) => {
                    let name = self.text(name).to_string();
                    match t
                        .decl
                        .as_ref()
                        .and_then(|d| d.params().iter().position(|p| p.name == name))
                    {
                        Some(p) => p,
                        None => continue,
                    }
                }
                None => i,
            };
            let Some(value) = arg.named_child(arg.named_child_count().saturating_sub(1)) else {
                continue;
            };
            let mut literals = Vec::new();
            if !self.literals(value, &mut literals) {
                continue;
            }
            let (expected, flags) = expected_argument_values(self.index, &file, callee, n);
            if expected.is_empty() {
                continue;
            }
            let wanted: Vec<String> = expected.iter().map(|v| normalize(v)).collect();
            let bad = literals.iter().any(|(text, lit)| {
                !(wanted.contains(&normalize(text)) || (flags && *lit && text.trim() == "0"))
            });
            if literals.len() > 1 && !flags {
                // `A | B` for a value that is not a flag: leave it to the type checks.
                continue;
            }
            if bad {
                let shown: Vec<String> = expected
                    .iter()
                    .map(|v| match v.split_once("::") {
                        Some((cls, rest)) if string_literal(v).is_none() => {
                            format!("{}::{rest}", short_name(cls))
                        }
                        _ => v.clone(),
                    })
                    .collect();
                self.out.push(PhpProblem {
                    start: value.start_byte(),
                    end: value.end_byte(),
                    severity: ProblemSeverity::Warning,
                    message: format!("Value should be one of: {}", shown.join(", ")),
                    code: "expectedValues".into(),
                    deprecated: false,
                    unnecessary: false,
                    fix: None,
                });
            }
        }
    }

    /// The literal parts of an argument (`'a'`, `42`, `Order::PAID`, `A | B`), qualified;
    /// `false` when it is not made of literals only (a variable, a call).
    fn literals(&self, n: Node, out: &mut Vec<(String, bool)>) -> bool {
        match n.kind() {
            "string" | "integer" | "float" => {
                out.push((self.text(n).to_string(), true));
                true
            }
            "encapsed_string" => {
                let mut c = n.walk();
                let plain = n
                    .named_children(&mut c)
                    .all(|x| matches!(x.kind(), "string_content" | "string_value"));
                if plain {
                    out.push((self.text(n).to_string(), true));
                }
                plain
            }
            "class_constant_access_expression" => {
                let text = self.text(n);
                if text.ends_with("::class") {
                    return false;
                }
                let owner = self.file.class_at(n.start_byte()).map(|c| c.fqn.clone());
                out.push((
                    qualify_value(text, &self.file, n.start_byte(), owner.as_deref()),
                    false,
                ));
                true
            }
            "name" | "qualified_name" => {
                let t = self.text(n);
                if matches!(t.to_ascii_lowercase().as_str(), "true" | "false" | "null") {
                    out.push((t.to_ascii_lowercase(), true));
                } else {
                    out.push((t.to_string(), false));
                }
                true
            }
            "boolean" | "null" => {
                out.push((self.text(n).to_ascii_lowercase(), true));
                true
            }
            "binary_expression"
                if n.child_by_field_name("operator")
                    .is_some_and(|o| self.text(o) == "|") =>
            {
                let (Some(l), Some(r)) = (
                    n.child_by_field_name("left"),
                    n.child_by_field_name("right"),
                ) else {
                    return false;
                };
                self.literals(l, out) && self.literals(r, out)
            }
            "parenthesized_expression" => match n.named_child(0) {
                Some(inner) => self.literals(inner, out),
                None => false,
            },
            _ => false,
        }
    }

    /// `$obj->prop = …` outside an `#[Immutable]` property's write scope.
    fn check_write(&mut self, left: Node) {
        let Some(name) = left.child_by_field_name("name") else {
            return;
        };
        let Resolved::Symbol(SymbolRef::Member {
            class,
            member: SymbolKind::Property,
            name: prop,
        }) = self.resolve(name)
        else {
            return;
        };
        let Some((owner, m)) = self.member(&class, SymbolKind::Property, &prop) else {
            return;
        };
        let Some(scope) = immutable(&m.attributes).or_else(|| immutable(&owner.attributes)) else {
            return;
        };
        let at = left.start_byte();
        let enclosing = self.file.class_at(at).cloned();
        let method = enclosing.as_ref().and_then(|c| {
            c.members
                .iter()
                .find(|x| x.kind == SymbolKind::Method && x.range.contains(at))
                .map(|x| x.name.clone())
        });
        let in_class = enclosing
            .as_ref()
            .is_some_and(|c| c.fqn.eq_ignore_ascii_case(&owner.fqn));
        let file = self.file.clone();
        let in_subclass = enclosing.as_ref().is_some_and(|c| {
            class_chain(self.index, &file, &c.fqn)
                .iter()
                .any(|x| x.eq_ignore_ascii_case(&owner.fqn))
        });
        let allowed = match scope {
            WriteScope::Constructor => {
                in_class
                    && method
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case("__construct"))
            }
            WriteScope::Private => in_class && method.is_some(),
            WriteScope::Protected => in_subclass && method.is_some(),
        };
        if allowed {
            return;
        }
        let where_ = match scope {
            WriteScope::Constructor => "outside of the constructor".to_string(),
            WriteScope::Private => format!("outside of class '{}'", owner.name),
            WriteScope::Protected => {
                format!("outside of class '{}' and its subclasses", owner.name)
            }
        };
        self.out.push(PhpProblem {
            start: name.start_byte(),
            end: name.end_byte(),
            severity: ProblemSeverity::Warning,
            message: format!("Immutable property '${prop}' cannot be written {where_}"),
            code: "immutable".into(),
            deprecated: false,
            unnecessary: false,
            fix: None,
        });
    }

    /// The call expression a statement consists of, and what it calls.
    fn statement_call<'t>(&mut self, stmt: Node<'t>) -> Option<(Node<'t>, Target)> {
        if stmt.kind() != "expression_statement" {
            return None;
        }
        let call = stmt.named_child(0)?;
        let name = call_name(call)?;
        let target = self.target(name, call.kind() == "object_creation_expression")?;
        Some((call, target))
    }

    /// Statements after a call that never returns.
    fn check_block(&mut self, block: Node) {
        let mut c = block.walk();
        let stmts: Vec<Node> = block
            .named_children(&mut c)
            .filter(|s| s.kind() != "comment")
            .collect();
        for (i, stmt) in stmts.iter().enumerate() {
            let Some((call, t)) = self.statement_call(*stmt) else {
                continue;
            };
            let exits = t.no_return
                || t.callee
                    .as_ref()
                    .is_some_and(|c| self.index.meta().is_exit_point(|m| c.matches(m)));
            if t.pure && !exits {
                self.out.push(PhpProblem {
                    start: call.start_byte(),
                    end: call.end_byte(),
                    severity: ProblemSeverity::Weak,
                    message: format!(
                        "The result of the pure {} call is not used",
                        if t.label.starts_with("Method") {
                            "method"
                        } else {
                            "function"
                        }
                    ),
                    code: "pureFunction".into(),
                    deprecated: false,
                    unnecessary: false,
                    fix: None,
                });
            }
            if !exits {
                continue;
            }
            let rest: Vec<&Node> = stmts[i + 1..]
                .iter()
                .filter(|s| {
                    !matches!(
                        s.kind(),
                        "function_definition"
                            | "class_declaration"
                            | "interface_declaration"
                            | "trait_declaration"
                            | "enum_declaration"
                    )
                })
                .collect();
            if let (Some(first), Some(last)) = (rest.first(), rest.last()) {
                self.out.push(PhpProblem {
                    start: first.start_byte(),
                    end: last.end_byte(),
                    severity: ProblemSeverity::Weak,
                    message: "Unreachable statement".into(),
                    code: "unreachable".into(),
                    deprecated: false,
                    unnecessary: true,
                    fix: None,
                });
            }
            break;
        }
    }

    fn visit(&mut self, node: Node) {
        if self.sites > MAX_SITES {
            return;
        }
        match node.kind() {
            "function_call_expression"
            | "member_call_expression"
            | "nullsafe_member_call_expression"
            | "scoped_call_expression"
            | "object_creation_expression" => {
                self.sites += 1;
                if let Some(name) = call_name(node) {
                    let ctor = node.kind() == "object_creation_expression";
                    if let Some(t) = self.target(name, ctor) {
                        let receiver = match node.kind() {
                            "member_call_expression" | "nullsafe_member_call_expression" => {
                                node.child_by_field_name("object")
                            }
                            "scoped_call_expression" => node.child_by_field_name("scope"),
                            _ => None,
                        };
                        self.deprecated_problem(name, &t, Some((node, receiver)));
                        self.check_arguments(node, &t);
                    }
                }
            }
            "member_access_expression" | "nullsafe_member_access_expression" => {
                self.sites += 1;
                if let Some(name) = node.child_by_field_name("name") {
                    if let Some(t) = self.target(name, false) {
                        self.deprecated_problem(name, &t, None);
                    }
                }
            }
            "class_constant_access_expression" => {
                self.sites += 1;
                let mut c = node.walk();
                let parts: Vec<Node> = node.named_children(&mut c).collect();
                if parts.len() == 2 && !self.text(parts[1]).eq_ignore_ascii_case("class") {
                    if let Some(t) = self.target(parts[1], false) {
                        self.deprecated_problem(parts[1], &t, None);
                    }
                }
            }
            "named_type" | "base_clause" | "class_interface_clause" => {
                let mut c = node.walk();
                let names: Vec<Node> = node
                    .named_children(&mut c)
                    .filter(|n| matches!(n.kind(), "name" | "qualified_name"))
                    .collect();
                for n in names {
                    if let Some(t) = self.target(n, false) {
                        self.deprecated_problem(n, &t, None);
                    }
                }
            }
            "assignment_expression"
            | "augmented_assignment_expression"
            | "reference_assignment_expression" => {
                if let Some(left) = node.child_by_field_name("left").filter(|l| {
                    matches!(
                        l.kind(),
                        "member_access_expression" | "nullsafe_member_access_expression"
                    )
                }) {
                    self.check_write(left);
                }
            }
            "update_expression" => {
                if let Some(left) = node
                    .named_child(0)
                    .filter(|l| l.kind() == "member_access_expression")
                {
                    self.check_write(left);
                }
            }
            "compound_statement" | "program" => self.check_block(node),
            _ => {}
        }
        let mut c = node.walk();
        let children: Vec<Node> = node.named_children(&mut c).collect();
        for child in children {
            self.visit(child);
        }
    }

    /// "Namespace name doesn't match PSR-0/PSR-4 project structure".
    fn check_namespace(&mut self) {
        if ProjectIndex::is_library(self.rel) {
            return;
        }
        let Some(ns) = self.file.namespaces.first().cloned() else {
            return;
        };
        let dir = self.rel.rsplit_once('/').map_or("", |(d, _)| d);
        let Some(expected) =
            forge_file_templates::psr4::namespace_for_dir(self.index.source_roots(), dir)
        else {
            return;
        };
        if expected.is_empty() || expected == ns.name {
            return;
        }
        // The name after the `namespace` keyword.
        let head = &self.src[ns.range.start..ns.range.end.min(self.src.len())];
        let Some(i) = head.find(ns.name.as_str()).filter(|_| !ns.name.is_empty()) else {
            return;
        };
        let start = ns.range.start + i;
        let end = start + ns.name.len();
        self.out.push(PhpProblem {
            start,
            end,
            severity: ProblemSeverity::Warning,
            message: "Namespace name doesn't match PSR-0/PSR-4 project structure".into(),
            code: "psr4Namespace".into(),
            deprecated: false,
            unnecessary: false,
            fix: Some(ProblemFix {
                title: format!("Set namespace to '{expected}'"),
                edits: vec![TextEdit {
                    start,
                    end,
                    new_text: expected,
                }],
            }),
        });
    }
}

/// The name node of a call (`foo`, `->bar`, `::baz`, `new Qux`).
fn call_name(call: Node) -> Option<Node> {
    match call.kind() {
        "function_call_expression" => call
            .child_by_field_name("function")
            .filter(|f| matches!(f.kind(), "name" | "qualified_name")),
        "member_call_expression" | "nullsafe_member_call_expression" | "scoped_call_expression" => {
            call.child_by_field_name("name")
                .filter(|n| n.kind() == "name")
        }
        "object_creation_expression" => {
            let mut c = call.walk();
            let n = call
                .named_children(&mut c)
                .find(|x| matches!(x.kind(), "name" | "qualified_name"));
            n
        }
        _ => None,
    }
}

/// Values compared regardless of quotes and leading backslashes.
fn normalize(v: &str) -> String {
    let v = v.trim();
    match string_literal(v) {
        Some(s) => format!("'{s}'"),
        None => v.trim_start_matches('\\').to_ascii_lowercase(),
    }
}

/// Run the inspections on `src`, the content of `rel`.
pub fn inspect(index: &mut ProjectIndex, rel: &str, src: &str) -> Vec<PhpProblem> {
    let mut p = parser::new_parser();
    let Some(tree) = p.parse(src, None) else {
        return Vec::new();
    };
    let mut ctx = Ctx {
        index,
        file: parser::index_source(rel, src),
        rel,
        src,
        out: Vec::new(),
        sites: 0,
    };
    ctx.check_namespace();
    ctx.visit(tree.root_node());
    let mut out = ctx.out;
    out.sort_by_key(|p| (p.start, p.end));
    out.dedup_by(|a, b| a.start == b.start && a.end == b.end && a.code == b.code);
    out
}

/// Quick-fixes of the problems at `offset`.
pub fn fixes_at(index: &mut ProjectIndex, rel: &str, src: &str, offset: usize) -> Vec<ProblemFix> {
    inspect(index, rel, src)
        .into_iter()
        .filter(|p| p.start <= offset && offset <= p.end)
        .filter_map(|p| p.fix)
        .collect()
}
