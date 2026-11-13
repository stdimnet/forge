//! PHPStorm's advanced metadata: `.phpstorm.meta.php` files (or the PHP files in a
//! `.phpstorm.meta.php/` folder) anywhere in the project or its vendor packages. Their
//! `namespace PHPSTORM_META { … }` holds calls that describe other code:
//!
//! - `override(\app(0), map(['' => '@', 'cache' => \Cache\Repository::class]))`: the return
//!   type of a call from its argument (`@` stands for the argument's value, so
//!   `'' => '@Factory'` maps `'User'` to `UserFactory`); `type(0)` returns the argument's
//!   type and `elementType(0)` the element type of an array argument.
//! - `expectedArguments(\setStatus(), 0, 'draft', 'paid')`, `expectedReturnValues(...)`:
//!   the values completion offers (flags when they are written `A | B`), also through
//!   `registerArgumentsSet('statuses', …)` and `argumentsSet('statuses')`.
//! - `exitPoint(\abort())`: calls that never return.
//!
//! Targets are functions (`\app(0)`, `\app()`), methods (`\Foo::get(0)`, which also apply
//! to subclasses and implementations) and constructors (`new \Foo`). [`parse`] reads one
//! file; [`PhpMeta`] merges the files of a project and answers lookups.

use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use crate::model::{Range, UseImport};
use crate::parser::{self, range_of};
use crate::resolve::resolve_class_name;

/// The namespace that holds metadata.
pub const META_NAMESPACE: &str = "PHPSTORM_META";
/// File (or folder) name of metadata files.
pub const META_NAME: &str = ".phpstorm.meta.php";

/// Whether a path (`/`-separated) is a metadata file: `.phpstorm.meta.php` itself or a PHP
/// file inside a `.phpstorm.meta.php` folder.
pub fn is_meta_path(path: &str) -> bool {
    let path = path.replace('\\', "/");
    let mut parts = path.split('/').filter(|p| !p.is_empty()).peekable();
    while let Some(p) = parts.next() {
        if p == META_NAME {
            return parts.peek().is_none() || path.ends_with(".php");
        }
    }
    false
}

/// What a metadata call refers to: a function, a method or a constructor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaCallable {
    /// Declaring class or interface (FQN without leading `\`) for methods and constructors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// Function FQN (without leading `\`), method name, or `__construct`.
    pub name: String,
}

impl MetaCallable {
    pub fn function(fqn: &str) -> Self {
        Self {
            class: None,
            name: fqn.trim_start_matches('\\').to_string(),
        }
    }

    pub fn method(class: &str, name: &str) -> Self {
        Self {
            class: Some(class.trim_start_matches('\\').to_string()),
            name: name.to_string(),
        }
    }

    /// Whether this is the function `fqn` (case-insensitive like PHP).
    pub fn is_function(&self, fqn: &str) -> bool {
        self.class.is_none() && self.name.eq_ignore_ascii_case(fqn.trim_start_matches('\\'))
    }

    /// Whether this is method `name` of one of `classes` (a class and its ancestors).
    pub fn is_method_of<S: AsRef<str>>(&self, classes: &[S], name: &str) -> bool {
        let Some(class) = &self.class else {
            return false;
        };
        self.name.eq_ignore_ascii_case(name)
            && classes.iter().any(|c| {
                c.as_ref()
                    .trim_start_matches('\\')
                    .eq_ignore_ascii_case(class)
            })
    }

    /// `\app()`, `\Foo::get()`, `new \Foo()`.
    pub fn display(&self) -> String {
        match &self.class {
            Some(c) if self.name.eq_ignore_ascii_case("__construct") => format!("new \\{c}()"),
            Some(c) => format!("\\{c}::{}()", self.name),
            None => format!("\\{}()", self.name),
        }
    }
}

/// Where a metadata entry is written (for Go to Declaration from a mapped key).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaLocation {
    pub path: String,
    pub range: Range,
}

/// One `key => type` entry of `map([...])`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapEntry {
    /// The argument value it applies to (class names without leading `\`); empty for the
    /// default entry `''`.
    pub key: String,
    /// The type: a class (`\App\Config`) or a pattern with `@` for the argument's value.
    pub value: String,
    pub location: MetaLocation,
}

/// How `override()` computes a call's return type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MetaReturn {
    Map {
        entries: Vec<MapEntry>,
    },
    /// `type(n)`: the type of argument `n`.
    Type {
        argument: usize,
    },
    /// `elementType(n)`: the element type of the array (or iterable) argument `n`.
    ElementType {
        argument: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaOverride {
    pub target: MetaCallable,
    /// The argument the map looks at.
    pub argument: usize,
    pub returns: MetaReturn,
}

/// A value of `expectedArguments()` / `expectedReturnValues()` / `registerArgumentsSet()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum MetaValue {
    /// An expression as it can be inserted: `'draft'`, `42`, `\App\Order::PAID`, `E_ALL`.
    Expr(String),
    /// `argumentsSet('name')`.
    Set(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedArguments {
    pub target: MetaCallable,
    pub argument: usize,
    pub values: Vec<MetaValue>,
    /// Written as `A | B`: values may be combined with `|`.
    #[serde(default)]
    pub flags: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedReturnValues {
    pub target: MetaCallable,
    pub values: Vec<MetaValue>,
    #[serde(default)]
    pub flags: bool,
}

/// The metadata of one file, or of a whole project after [`PhpMeta::merge`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpMeta {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<MetaOverride>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_arguments: Vec<ExpectedArguments>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_return_values: Vec<ExpectedReturnValues>,
    /// `registerArgumentsSet('name', ...)`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub argument_sets: Vec<(String, Vec<MetaValue>)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exit_points: Vec<MetaCallable>,
}

/// The value of a call argument as far as metadata needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgValue {
    /// A string literal's content.
    Str(String),
    /// `Foo::class`, resolved (no leading `\`).
    Class(String),
    Unknown,
}

/// What [`PhpMeta::return_type`] found for a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetaType {
    /// A type to resolve as fully qualified (`\App\Config`, `\App\UserFactory`).
    Named(String),
    /// The type of argument `n`.
    ArgumentType(usize),
    /// The element type of argument `n`.
    ArgumentElementType(usize),
}

impl PhpMeta {
    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty()
            && self.expected_arguments.is_empty()
            && self.expected_return_values.is_empty()
            && self.argument_sets.is_empty()
            && self.exit_points.is_empty()
    }

    /// Add the entries of `other`.
    pub fn merge(&mut self, other: &PhpMeta) {
        self.overrides.extend(other.overrides.iter().cloned());
        self.expected_arguments
            .extend(other.expected_arguments.iter().cloned());
        self.expected_return_values
            .extend(other.expected_return_values.iter().cloned());
        self.argument_sets
            .extend(other.argument_sets.iter().cloned());
        self.exit_points.extend(other.exit_points.iter().cloned());
    }

    /// The overrides of a callable, `matches` deciding which targets apply.
    fn overrides_for(&self, matches: impl Fn(&MetaCallable) -> bool) -> Vec<&MetaOverride> {
        self.overrides
            .iter()
            .filter(|o| matches(&o.target))
            .collect()
    }

    /// The return type of a call to the function `fqn`; `arg(n)` is the value of argument `n`.
    pub fn function_return(&self, fqn: &str, arg: impl Fn(usize) -> ArgValue) -> Option<MetaType> {
        resolve_overrides(&self.overrides_for(|t| t.is_function(fqn)), arg)
    }

    /// The return type of a call to method `name` on a class whose class and ancestors are
    /// `classes`.
    pub fn method_return<S: AsRef<str>>(
        &self,
        classes: &[S],
        name: &str,
        arg: impl Fn(usize) -> ArgValue,
    ) -> Option<MetaType> {
        resolve_overrides(&self.overrides_for(|t| t.is_method_of(classes, name)), arg)
    }

    /// The map entry a call's argument hits (Go to Declaration on `app('cache')`).
    pub fn map_entry(
        &self,
        matches: impl Fn(&MetaCallable) -> bool,
        argument: usize,
        value: &ArgValue,
    ) -> Option<&MapEntry> {
        self.overrides
            .iter()
            .filter(|o| o.argument == argument && matches(&o.target))
            .find_map(|o| match &o.returns {
                MetaReturn::Map { entries } => entries
                    .iter()
                    .find(|e| !e.key.is_empty() && key_matches(&e.key, value)),
                _ => None,
            })
    }

    /// The keys of the maps of a callable's argument (completion of `app('|')`).
    pub fn map_keys(
        &self,
        matches: impl Fn(&MetaCallable) -> bool,
        argument: usize,
    ) -> Vec<&MapEntry> {
        self.overrides
            .iter()
            .filter(|o| o.argument == argument && matches(&o.target))
            .flat_map(|o| match &o.returns {
                MetaReturn::Map { entries } => {
                    entries.iter().filter(|e| !e.key.is_empty()).collect()
                }
                _ => Vec::new(),
            })
            .collect()
    }

    /// Values `argumentsSet(name)` stands for, expanded (sets may use other sets).
    fn expand(&self, values: &[MetaValue], depth: u8, out: &mut Vec<String>) {
        for v in values {
            match v {
                MetaValue::Expr(e) => {
                    if !out.contains(e) {
                        out.push(e.clone());
                    }
                }
                MetaValue::Set(name) if depth < 8 => {
                    for (n, vals) in &self.argument_sets {
                        if n == name {
                            self.expand(vals, depth + 1, out);
                        }
                    }
                }
                MetaValue::Set(_) => {}
            }
        }
    }

    /// Expected values of argument `argument` of a callable (`matches` picks the targets),
    /// and whether they are flags.
    pub fn expected_arguments(
        &self,
        matches: impl Fn(&MetaCallable) -> bool,
        argument: usize,
    ) -> (Vec<String>, bool) {
        let mut out = Vec::new();
        let mut flags = false;
        for e in &self.expected_arguments {
            if e.argument == argument && matches(&e.target) {
                self.expand(&e.values, 0, &mut out);
                flags |= e.flags;
            }
        }
        (out, flags)
    }

    /// Expected return values of a callable, and whether they are flags.
    pub fn expected_return_values(
        &self,
        matches: impl Fn(&MetaCallable) -> bool,
    ) -> (Vec<String>, bool) {
        let mut out = Vec::new();
        let mut flags = false;
        for e in &self.expected_return_values {
            if matches(&e.target) {
                self.expand(&e.values, 0, &mut out);
                flags |= e.flags;
            }
        }
        (out, flags)
    }

    /// Whether a callable is an `exitPoint()`.
    pub fn is_exit_point(&self, matches: impl Fn(&MetaCallable) -> bool) -> bool {
        self.exit_points.iter().any(matches)
    }
}

fn key_matches(key: &str, value: &ArgValue) -> bool {
    let v = match value {
        ArgValue::Str(s) => s.trim_start_matches('\\'),
        ArgValue::Class(c) => c.trim_start_matches('\\'),
        ArgValue::Unknown => return false,
    };
    // Class names are case-insensitive; service ids are compared the same way (PHPStorm).
    key.trim_start_matches('\\').eq_ignore_ascii_case(v)
}

fn resolve_overrides(
    overrides: &[&MetaOverride],
    arg: impl Fn(usize) -> ArgValue,
) -> Option<MetaType> {
    for o in overrides {
        match &o.returns {
            MetaReturn::Type { argument } => return Some(MetaType::ArgumentType(*argument)),
            MetaReturn::ElementType { argument } => {
                return Some(MetaType::ArgumentElementType(*argument))
            }
            MetaReturn::Map { entries } => {
                let value = arg(o.argument);
                if let Some(e) = entries
                    .iter()
                    .find(|e| !e.key.is_empty() && key_matches(&e.key, &value))
                {
                    if let Some(t) = substitute(&e.value, &value) {
                        return Some(MetaType::Named(t));
                    }
                }
                if let Some(e) = entries.iter().find(|e| e.key.is_empty()) {
                    if let Some(t) = substitute(&e.value, &value) {
                        return Some(MetaType::Named(t));
                    }
                }
            }
        }
    }
    None
}

/// `@` in a map value replaced with the argument's value, as a fully qualified type.
fn substitute(pattern: &str, value: &ArgValue) -> Option<String> {
    let t = if pattern.contains('@') {
        let v = match value {
            ArgValue::Str(s) => s.as_str(),
            ArgValue::Class(c) => c.as_str(),
            ArgValue::Unknown => return None,
        };
        if v.is_empty() {
            return None;
        }
        pattern.replace('@', v.trim_start_matches('\\'))
    } else {
        pattern.to_string()
    };
    let t = t.trim();
    if t.is_empty() {
        return None;
    }
    Some(if t.starts_with('\\') {
        t.to_string()
    } else {
        format!("\\{t}")
    })
}

/// Parse the metadata of one file (`path` is stored in locations).
pub fn parse(path: &str, src: &str) -> PhpMeta {
    let mut meta = PhpMeta::default();
    if !src.contains(META_NAMESPACE) {
        return meta;
    }
    let mut p = parser::new_parser();
    let Some(tree) = p.parse(src, None) else {
        return meta;
    };
    let ctx = MetaCtx {
        src,
        path,
        uses: parser::index_source(path, src).uses,
    };
    let root = tree.root_node();
    let mut cursor = root.walk();
    for ns in root.named_children(&mut cursor) {
        if ns.kind() != "namespace_definition" {
            continue;
        }
        let name = ns
            .child_by_field_name("name")
            .map(|n| ctx.text(n))
            .unwrap_or_default();
        if !name.eq_ignore_ascii_case(META_NAMESPACE) {
            continue;
        }
        let Some(body) = ns.child_by_field_name("body") else {
            continue;
        };
        ctx.collect(body, &mut meta);
    }
    meta
}

struct MetaCtx<'a> {
    src: &'a str,
    path: &'a str,
    uses: Vec<UseImport>,
}

impl<'a> MetaCtx<'a> {
    fn text(&self, n: Node) -> &'a str {
        &self.src[n.byte_range()]
    }

    /// Calls at statement level (also inside blocks and `if`s).
    fn collect(&self, node: Node, meta: &mut PhpMeta) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "expression_statement" => {
                    if let Some(call) = child
                        .named_child(0)
                        .filter(|c| c.kind() == "function_call_expression")
                    {
                        self.call(call, meta);
                    }
                }
                "compound_statement" | "if_statement" | "else_clause" => self.collect(child, meta),
                _ => {}
            }
        }
    }

    fn name_of(&self, call: Node) -> String {
        call.child_by_field_name("function")
            .map(|n| self.text(n).trim_start_matches('\\').to_string())
            .unwrap_or_default()
    }

    fn args<'t>(call: Node<'t>) -> Vec<Node<'t>> {
        let Some(args) = call.child_by_field_name("arguments") else {
            return Vec::new();
        };
        let mut cursor = args.walk();
        args.named_children(&mut cursor)
            .filter(|a| a.kind() == "argument")
            .filter_map(|a| a.named_child(a.named_child_count().saturating_sub(1)))
            .collect()
    }

    fn class_name(&self, n: Node) -> String {
        let written = self.text(n).trim();
        resolve_class_name(written, "", &self.uses)
    }

    /// `\app(0)` -> (`app`, 0), `\Foo::get()` -> (`Foo::get`, None), `new \Foo` -> constructor.
    fn target(&self, n: Node) -> Option<(MetaCallable, Option<usize>)> {
        let first_int = |call: Node| {
            Self::args(call)
                .first()
                .filter(|a| a.kind() == "integer")
                .and_then(|a| self.text(*a).parse::<usize>().ok())
        };
        match n.kind() {
            "function_call_expression" => {
                let f = n.child_by_field_name("function")?;
                let written = self.text(f).trim();
                // Functions in metadata are global unless imported.
                let fqn = match self.uses.iter().find(|u| {
                    u.kind == crate::model::UseKind::Function
                        && u.local_name().eq_ignore_ascii_case(written)
                }) {
                    Some(u) => u.fqn.clone(),
                    None => written.trim_start_matches('\\').to_string(),
                };
                Some((MetaCallable::function(&fqn), first_int(n)))
            }
            "scoped_call_expression" => {
                let scope = n.child_by_field_name("scope")?;
                let name = n.child_by_field_name("name")?;
                Some((
                    MetaCallable::method(&self.class_name(scope), self.text(name)),
                    first_int(n),
                ))
            }
            "object_creation_expression" => {
                let mut c = n.walk();
                let class = n
                    .named_children(&mut c)
                    .find(|x| matches!(x.kind(), "name" | "qualified_name" | "relative_name"))?;
                Some((
                    MetaCallable::method(&self.class_name(class), "__construct"),
                    first_int(n),
                ))
            }
            _ => None,
        }
    }

    fn int(&self, n: Option<&Node>) -> Option<usize> {
        n.filter(|a| a.kind() == "integer")
            .and_then(|a| self.text(*a).parse().ok())
    }

    /// A string literal's content.
    fn string(&self, n: Node) -> Option<String> {
        if !matches!(n.kind(), "string" | "encapsed_string") {
            return None;
        }
        let t = self.text(n);
        Some(
            t.get(1..t.len().saturating_sub(1))
                .unwrap_or("")
                .to_string(),
        )
    }

    /// A map key or value: a string or `Foo::class` (class names without leading `\`).
    fn type_ref(&self, n: Node) -> Option<String> {
        if let Some(s) = self.string(n) {
            return Some(s);
        }
        if n.kind() == "class_constant_access_expression" {
            let mut c = n.walk();
            let parts: Vec<Node> = n.named_children(&mut c).collect();
            if parts.len() == 2 && self.text(parts[1]).eq_ignore_ascii_case("class") {
                return Some(self.class_name(parts[0]));
            }
        }
        None
    }

    /// An expected value as source text; class constants fully qualified.
    fn value_text(&self, n: Node) -> String {
        if n.kind() == "class_constant_access_expression" {
            let mut c = n.walk();
            let parts: Vec<Node> = n.named_children(&mut c).collect();
            if parts.len() == 2 {
                let class = self.text(parts[0]);
                let class = match class.to_ascii_lowercase().as_str() {
                    "self" | "static" | "parent" => class.to_string(),
                    _ => format!("\\{}", self.class_name(parts[0])),
                };
                return format!("{class}::{}", self.text(parts[1]));
            }
        }
        self.text(n)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Values of `expectedArguments` and friends; `A | B` makes them flags.
    fn values(&self, nodes: &[Node], flags: &mut bool, out: &mut Vec<MetaValue>) {
        for n in nodes {
            match n.kind() {
                "binary_expression"
                    if n.child_by_field_name("operator")
                        .is_some_and(|o| self.text(o) == "|") =>
                {
                    *flags = true;
                    let parts: Vec<Node> = [
                        n.child_by_field_name("left"),
                        n.child_by_field_name("right"),
                    ]
                    .into_iter()
                    .flatten()
                    .collect();
                    self.values(&parts, flags, out);
                }
                "function_call_expression"
                    if self.name_of(*n).eq_ignore_ascii_case("argumentsSet") =>
                {
                    if let Some(name) = Self::args(*n).first().and_then(|a| self.string(*a)) {
                        out.push(MetaValue::Set(name));
                    }
                }
                _ => out.push(MetaValue::Expr(self.value_text(*n))),
            }
        }
    }

    fn call(&self, call: Node, meta: &mut PhpMeta) {
        let name = self.name_of(call);
        let args = Self::args(call);
        match name.to_ascii_lowercase().as_str() {
            "override" => {
                let (Some(target), Some(how)) = (args.first(), args.get(1)) else {
                    return;
                };
                let Some((target, argument)) = self.target(*target) else {
                    return;
                };
                let argument = argument.unwrap_or(0);
                if how.kind() != "function_call_expression" {
                    return;
                }
                let how_args = Self::args(*how);
                let returns = match self.name_of(*how).to_ascii_lowercase().as_str() {
                    "map" => {
                        let Some(array) = how_args
                            .first()
                            .filter(|a| a.kind() == "array_creation_expression")
                        else {
                            return;
                        };
                        let mut entries = Vec::new();
                        let mut c = array.walk();
                        for el in array.named_children(&mut c) {
                            if el.kind() != "array_element_initializer" {
                                continue;
                            }
                            let mut c2 = el.walk();
                            let kv: Vec<Node> = el.named_children(&mut c2).collect();
                            if kv.len() != 2 {
                                continue;
                            }
                            let (Some(key), Some(value)) =
                                (self.type_ref(kv[0]), self.type_ref(kv[1]))
                            else {
                                continue;
                            };
                            entries.push(MapEntry {
                                key: key.trim_start_matches('\\').to_string(),
                                value: if value.contains('@') || value.starts_with('\\') {
                                    value
                                } else {
                                    format!("\\{value}")
                                },
                                location: MetaLocation {
                                    path: self.path.to_string(),
                                    range: range_of(kv[0]),
                                },
                            });
                        }
                        MetaReturn::Map { entries }
                    }
                    "type" => MetaReturn::Type {
                        argument: self.int(how_args.first()).unwrap_or(argument),
                    },
                    "elementtype" => MetaReturn::ElementType {
                        argument: self.int(how_args.first()).unwrap_or(argument),
                    },
                    _ => return,
                };
                meta.overrides.push(MetaOverride {
                    target,
                    argument,
                    returns,
                });
            }
            "expectedarguments" => {
                let Some((target, _)) = args.first().and_then(|t| self.target(*t)) else {
                    return;
                };
                let Some(argument) = self.int(args.get(1)) else {
                    return;
                };
                let mut flags = false;
                let mut values = Vec::new();
                self.values(&args[2..], &mut flags, &mut values);
                meta.expected_arguments.push(ExpectedArguments {
                    target,
                    argument,
                    values,
                    flags,
                });
            }
            "expectedreturnvalues" => {
                let Some((target, _)) = args.first().and_then(|t| self.target(*t)) else {
                    return;
                };
                let mut flags = false;
                let mut values = Vec::new();
                self.values(&args[1..], &mut flags, &mut values);
                meta.expected_return_values.push(ExpectedReturnValues {
                    target,
                    values,
                    flags,
                });
            }
            "registerargumentsset" => {
                let Some(name) = args.first().and_then(|a| self.string(*a)) else {
                    return;
                };
                let mut flags = false;
                let mut values = Vec::new();
                self.values(&args[1..], &mut flags, &mut values);
                meta.argument_sets.push((name, values));
            }
            "exitpoint" => {
                if let Some((target, _)) = args.first().and_then(|t| self.target(*t)) {
                    meta.exit_points.push(target);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const META: &str = r#"<?php
namespace PHPSTORM_META {
    use App\Services\Clock;

    override(\Psr\Container\ContainerInterface::get(0), map([
        '' => '@',
        'config' => \Illuminate\Config\Repository::class,
        'logger' => '\Monolog\Logger',
        Clock::class => \App\Services\SystemClock::class,
    ]));
    override(\app(0), map(['' => '@', 'cache' => \Illuminate\Cache\Repository::class]));
    override(\factory(0), map(['' => '@Factory']));
    override(\tap(0), type(0));
    override(\head(0), elementType(0));
    registerArgumentsSet('statuses', 'draft', 'paid');
    registerArgumentsSet('more', argumentsSet('statuses'), 'void');
    expectedArguments(\App\Models\Order::setStatus(), 0, argumentsSet('more'));
    expectedArguments(\json_encode(), 1, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES);
    expectedArguments(new \App\Mail\Mailer(), 0, Clock::UTC, 'smtp');
    expectedReturnValues(\App\Models\Order::status(), 'draft', 'paid');
    exitPoint(\abort());
    exitPoint(\App\Support\Http::fail());
    if (function_exists('x')) {
        exitPoint(\dd());
    }
}
"#;

    #[test]
    fn recognizes_meta_paths() {
        assert!(is_meta_path(".phpstorm.meta.php"));
        assert!(is_meta_path("vendor/acme/pkg/.phpstorm.meta.php"));
        assert!(is_meta_path(".phpstorm.meta.php/laravel.meta.php"));
        assert!(is_meta_path("/opt/lib/.phpstorm.meta.php/a/b.php"));
        assert!(!is_meta_path("app/Models/User.php"));
        assert!(!is_meta_path("phpstorm.meta.php"));
    }

    #[test]
    fn parses_overrides_with_maps_types_and_element_types() {
        let meta = parse(".phpstorm.meta.php", META);
        assert_eq!(meta.overrides.len(), 5);
        let get = &meta.overrides[0];
        assert_eq!(
            get.target,
            MetaCallable::method("Psr\\Container\\ContainerInterface", "get")
        );
        assert_eq!(get.argument, 0);
        let MetaReturn::Map { entries } = &get.returns else {
            panic!("map expected");
        };
        let pairs: Vec<(&str, &str)> = entries
            .iter()
            .map(|e| (e.key.as_str(), e.value.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("", "@"),
                ("config", "\\Illuminate\\Config\\Repository"),
                ("logger", "\\Monolog\\Logger"),
                ("App\\Services\\Clock", "\\App\\Services\\SystemClock"),
            ]
        );
        assert_eq!(entries[1].location.range.line, 6);
        assert_eq!(meta.overrides[3].returns, MetaReturn::Type { argument: 0 });
        assert_eq!(
            meta.overrides[4].returns,
            MetaReturn::ElementType { argument: 0 }
        );
    }

    #[test]
    fn resolves_return_types_from_arguments() {
        let meta = parse(".phpstorm.meta.php", META);
        let container = ["App\\Container", "Psr\\Container\\ContainerInterface"];
        let call = |v: ArgValue| meta.method_return(&container, "get", move |_| v.clone());
        assert_eq!(
            call(ArgValue::Class("App\\Mailer".into())),
            Some(MetaType::Named("\\App\\Mailer".into()))
        );
        assert_eq!(
            call(ArgValue::Str("config".into())),
            Some(MetaType::Named("\\Illuminate\\Config\\Repository".into()))
        );
        assert_eq!(
            call(ArgValue::Class("App\\Services\\Clock".into())),
            Some(MetaType::Named("\\App\\Services\\SystemClock".into()))
        );
        assert_eq!(call(ArgValue::Unknown), None);
        assert_eq!(
            meta.method_return(&["Other"], "get", |_| ArgValue::Str("x".into())),
            None
        );
        assert_eq!(
            meta.function_return("app", |_| ArgValue::Str("cache".into())),
            Some(MetaType::Named("\\Illuminate\\Cache\\Repository".into()))
        );
        assert_eq!(
            meta.function_return("\\factory", |_| ArgValue::Str("User".into())),
            Some(MetaType::Named("\\UserFactory".into()))
        );
        assert_eq!(
            meta.function_return("tap", |_| ArgValue::Unknown),
            Some(MetaType::ArgumentType(0))
        );
        assert_eq!(
            meta.function_return("head", |_| ArgValue::Unknown),
            Some(MetaType::ArgumentElementType(0))
        );
        let entry = meta
            .map_entry(|t| t.is_function("app"), 0, &ArgValue::Str("cache".into()))
            .unwrap();
        assert_eq!(entry.location.range.line, 10);
        assert_eq!(meta.map_keys(|t| t.is_function("app"), 0).len(), 1);
    }

    #[test]
    fn expected_arguments_return_values_sets_and_exit_points() {
        let meta = parse(".phpstorm.meta.php", META);
        let (values, flags) =
            meta.expected_arguments(|t| t.is_method_of(&["App\\Models\\Order"], "setStatus"), 0);
        assert_eq!(values, vec!["'draft'", "'paid'", "'void'"]);
        assert!(!flags);
        let (values, flags) = meta.expected_arguments(|t| t.is_function("json_encode"), 1);
        assert_eq!(values, vec!["JSON_PRETTY_PRINT", "JSON_UNESCAPED_SLASHES"]);
        assert!(flags);
        let (values, _) =
            meta.expected_arguments(|t| t.is_method_of(&["App\\Mail\\Mailer"], "__construct"), 0);
        assert_eq!(values, vec!["\\App\\Services\\Clock::UTC", "'smtp'"]);
        let (values, _) =
            meta.expected_return_values(|t| t.is_method_of(&["App\\Models\\Order"], "status"));
        assert_eq!(values, vec!["'draft'", "'paid'"]);
        assert!(meta.is_exit_point(|t| t.is_function("abort")));
        assert!(meta.is_exit_point(|t| t.is_function("dd")));
        assert!(meta.is_exit_point(|t| t.is_method_of(&["App\\Support\\Http"], "FAIL")));
        assert!(!meta.is_exit_point(|t| t.is_function("app")));
        assert_eq!(
            meta.exit_points[1].display(),
            "\\App\\Support\\Http::fail()"
        );
    }

    #[test]
    fn ignores_other_namespaces_and_broken_code() {
        assert!(parse(
            "a.php",
            "<?php\nnamespace App;\noverride(\\app(0), type(0));\n"
        )
        .is_empty());
        let broken = "<?php\nnamespace PHPSTORM_META {\n override(\\app(0), map([\n expectedArguments(\\x(), 0, 'a');\n";
        let meta = parse(".phpstorm.meta.php", broken);
        assert!(meta.overrides.len() <= 1);
        let mut all = PhpMeta::default();
        all.merge(&parse(".phpstorm.meta.php", META));
        all.merge(&parse(
            "b/.phpstorm.meta.php",
            "<?php namespace PHPSTORM_META { exitPoint(\\stop()); }",
        ));
        assert!(all.is_exit_point(|t| t.is_function("stop")));
        assert!(all.is_exit_point(|t| t.is_function("abort")));
    }
}
