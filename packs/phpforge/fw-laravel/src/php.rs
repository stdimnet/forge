//! Small helpers over the tree-sitter PHP syntax tree: literal values, call arguments and
//! returned arrays. Laravel's configuration lives in plain PHP arrays and fluent calls, so
//! static evaluation of these few shapes covers routes, config, translations and models.

use phpforge_lang_php::parser;
use tree_sitter::{Node, Tree};

pub fn parse(source: &str) -> Option<Tree> {
    let mut p = parser::new_parser();
    parser::parse_tree(&mut p, source)
}

pub fn text<'a>(node: Node, src: &'a str) -> &'a str {
    src.get(node.byte_range()).unwrap_or("")
}

/// 0-based line of a byte offset.
pub fn line_of(src: &str, byte: usize) -> u32 {
    src.as_bytes()[..byte.min(src.len())]
        .iter()
        .filter(|b| **b == b'\n')
        .count() as u32
}

/// Value of a string literal without interpolation (`'a'`, `"b"`), else `None`.
pub fn string_value(node: Node, src: &str) -> Option<String> {
    match node.kind() {
        "string" | "encapsed_string" => {
            let mut out = String::new();
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                match child.kind() {
                    "string_content" => out.push_str(text(child, src)),
                    "escape_sequence" => out.push_str(&unescape(text(child, src))),
                    _ => return None,
                }
            }
            Some(out)
        }
        "argument" | "parenthesized_expression" => {
            let child = node.named_child(node.named_child_count().checked_sub(1)?)?;
            string_value(child, src)
        }
        _ => None,
    }
}

fn unescape(seq: &str) -> String {
    match seq {
        "\\n" => "\n".into(),
        "\\t" => "\t".into(),
        "\\\\" => "\\".into(),
        "\\'" => "'".into(),
        "\\\"" => "\"".into(),
        "\\$" => "$".into(),
        other => other.to_string(),
    }
}

/// A statically known PHP value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    /// Numbers, booleans, `null` and constants, as written.
    Scalar(String),
    Array(Vec<Entry>),
    /// `Foo::class`, the class name as written.
    ClassRef(String),
    /// Anything else (calls, closures, ...), as written.
    Expr(String),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn entries(&self) -> &[Entry] {
        match self {
            Value::Array(e) => e,
            _ => &[],
        }
    }

    /// Strings of a list (`['a', 'b']`) or a single string.
    pub fn strings(&self) -> Vec<String> {
        match self {
            Value::Str(s) => vec![s.clone()],
            Value::Array(e) => e
                .iter()
                .filter_map(|e| e.value.as_str().map(String::from))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Short preview for completion details.
    pub fn preview(&self) -> String {
        match self {
            Value::Str(s) => format!("'{s}'"),
            Value::Scalar(s) | Value::Expr(s) => {
                let one: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
                if one.chars().count() > 60 {
                    format!("{}…", one.chars().take(59).collect::<String>())
                } else {
                    one
                }
            }
            Value::Array(e) => format!("array({})", e.len()),
            Value::ClassRef(c) => format!("{c}::class"),
        }
    }
}

/// One array element: `key => value` or a list value.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub key: Option<String>,
    pub value: Value,
    /// Byte offset of the key (or of the value for list entries).
    pub offset: usize,
}

impl Entry {
    pub fn get<'a>(entries: &'a [Entry], key: &str) -> Option<&'a Value> {
        entries
            .iter()
            .find(|e| e.key.as_deref() == Some(key))
            .map(|e| &e.value)
    }
}

pub fn value(node: Node, src: &str) -> Value {
    match node.kind() {
        "string" | "encapsed_string" => match string_value(node, src) {
            Some(s) => Value::Str(s),
            None => Value::Expr(text(node, src).to_string()),
        },
        "integer" | "float" | "boolean" | "null" => Value::Scalar(text(node, src).to_string()),
        "parenthesized_expression" => match node.named_child(0) {
            Some(c) => value(c, src),
            None => Value::Expr(text(node, src).into()),
        },
        "array_creation_expression" => {
            let mut entries = Vec::new();
            let mut cursor = node.walk();
            for el in node.named_children(&mut cursor) {
                if el.kind() != "array_element_initializer" {
                    continue;
                }
                let mut c = el.walk();
                let parts: Vec<Node> = el.named_children(&mut c).collect();
                match parts.as_slice() {
                    [k, v] => entries.push(Entry {
                        key: Some(match value(*k, src) {
                            Value::Str(s) | Value::Scalar(s) => s,
                            other => other.preview(),
                        }),
                        value: value(*v, src),
                        offset: k.start_byte(),
                    }),
                    [v] => entries.push(Entry {
                        key: None,
                        value: value(*v, src),
                        offset: v.start_byte(),
                    }),
                    _ => {}
                }
            }
            Value::Array(entries)
        }
        "class_constant_access_expression" => {
            let t = text(node, src);
            match t.strip_suffix("::class") {
                Some(class) => Value::ClassRef(class.trim().to_string()),
                None => Value::Expr(t.to_string()),
            }
        }
        _ => Value::Expr(text(node, src).to_string()),
    }
}

/// Expression nodes of a call's arguments (named arguments keep their value only).
pub fn call_args(call: Node) -> Vec<Node> {
    let Some(args) = call.child_by_field_name("arguments") else {
        return Vec::new();
    };
    let mut cursor = args.walk();
    args.named_children(&mut cursor)
        .filter(|a| a.kind() == "argument")
        .filter_map(|a| a.named_child(a.named_child_count().checked_sub(1)?))
        .collect()
}

/// `(name, value)` of named arguments (`name: 'x'`).
pub fn named_arg<'t>(call: Node<'t>, src: &str, name: &str) -> Option<Node<'t>> {
    let args = call.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let found = args
        .named_children(&mut cursor)
        .filter(|a| a.kind() == "argument")
        .find(|a| a.child_by_field_name("name").map(|n| text(n, src)) == Some(name));
    found.and_then(|a| a.named_child(a.named_child_count().checked_sub(1)?))
}

/// Name of the called function / method (`route`, `get`, `hasMany`).
pub fn call_name<'a>(call: Node, src: &'a str) -> Option<&'a str> {
    let n = match call.kind() {
        "function_call_expression" => call.child_by_field_name("function")?,
        "member_call_expression" | "nullsafe_member_call_expression" | "scoped_call_expression" => {
            call.child_by_field_name("name")?
        }
        _ => return None,
    };
    Some(text(n, src).trim_start_matches('\\'))
}

pub fn is_call(node: Node) -> bool {
    matches!(
        node.kind(),
        "function_call_expression"
            | "member_call_expression"
            | "nullsafe_member_call_expression"
            | "scoped_call_expression"
    )
}

/// Visit every node in pre-order; return `false` from `f` to skip the children.
pub fn walk<'t>(node: Node<'t>, f: &mut impl FnMut(Node<'t>) -> bool) {
    if !f(node) {
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        walk(child, f);
    }
}

/// The array (or other value) returned at the top level of a file (`return [...]`).
pub fn returned_value(tree: &Tree, src: &str) -> Option<Value> {
    let root = tree.root_node();
    let mut cursor = root.walk();
    let ret = root
        .named_children(&mut cursor)
        .find(|n| n.kind() == "return_statement")?;
    let expr = ret.named_child(0)?;
    Some(value(expr, src))
}

/// Flatten nested arrays into dotted keys: `['a' => ['b' => 1]]` → `a.b`. Lists are
/// treated as leaves. `f` receives every key, including intermediate arrays.
pub fn flatten<'v>(entries: &'v [Entry], prefix: &str, f: &mut impl FnMut(String, &'v Entry)) {
    for e in entries {
        let Some(key) = &e.key else { continue };
        let full = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        f(full.clone(), e);
        if let Value::Array(children) = &e.value {
            if children.iter().any(|c| c.key.is_some()) {
                flatten(children, &full, f);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_returned_arrays() {
        let src = "<?php\nreturn [\n  'name' => env('APP_NAME', 'Laravel'),\n  'debug' => false,\n  'nested' => ['a' => \"x\\ty\", 'list' => ['p', 'q']],\n  'model' => App\\Models\\User::class,\n];\n";
        let tree = parse(src).unwrap();
        let v = returned_value(&tree, src).unwrap();
        let mut keys = Vec::new();
        flatten(v.entries(), "", &mut |k, e| {
            keys.push((k, e.value.preview()))
        });
        assert_eq!(
            keys,
            vec![
                ("name".to_string(), "env('APP_NAME', 'Laravel')".to_string()),
                ("debug".into(), "false".into()),
                ("nested".into(), "array(2)".into()),
                ("nested.a".into(), "'x\ty'".into()),
                ("nested.list".into(), "array(2)".into()),
                ("model".into(), "App\\Models\\User::class".into()),
            ]
        );
        assert_eq!(line_of(src, src.find("'debug'").unwrap()), 3);
    }
}
