//! Completion from PHPStorm metadata and JetBrains attributes, next to the language
//! server's: expected arguments (`expectedArguments()`, `#[ExpectedValues]`, the keys of
//! `override()` maps such as service ids), expected return values in comparisons
//! (`$order->status() === '|'`) and the keys of an `#[ArrayShape]` (`$data['|']`).
//! Offsets are bytes here; [`crate::service::complete`] converts to UTF-16.

use serde::{Deserialize, Serialize};

use crate::attributes::{class_constants, expected_values, qualify_value, string_literal};
use crate::callsite::{
    call_at, callee_before, callee_decl, declaring_file, scan_to, shape_of, Callee, Decl,
};
use crate::index::ProjectIndex;
use crate::model::FileIndex;
use crate::navigate::{find_class, matching_open};
use crate::parser;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PhpCompletionKind {
    /// A literal (`'paid'`, `42`).
    Value,
    /// A constant (`Order::PAID`, `E_ALL`).
    Constant,
    /// An array key of `#[ArrayShape]`.
    Key,
    /// A class (`Mailer::class` keys of a metadata map).
    Class,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpCompletionItem {
    pub label: String,
    /// The text replacing `from..to`.
    pub insert: String,
    pub kind: PhpCompletionKind,
    /// Type of a key, or where the values come from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpCompletion {
    pub from: usize,
    pub to: usize,
    pub items: Vec<PhpCompletionItem>,
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\\' || c == ':'
}

/// Start of the word being typed before `offset`.
fn prefix_start(src: &str, offset: usize) -> usize {
    src[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_word(*c))
        .last()
        .map_or(offset, |(i, _)| i)
}

/// End of the string content after `offset` (up to the closing quote, if on this line).
fn string_rest_end(src: &str, offset: usize, quote: char) -> usize {
    src[offset..]
        .char_indices()
        .take_while(|(_, c)| *c != '\n')
        .find(|(_, c)| *c == quote)
        .map_or(offset, |(i, _)| offset + i)
}

/// `\App\Models\Order::PAID` -> `Order::PAID` for the label.
fn short_label(value: &str) -> String {
    match value.split_once("::") {
        Some((class, rest)) if string_literal(value).is_none() => {
            format!("{}::{rest}", crate::model::short_name(class))
        }
        _ => value.to_string(),
    }
}

fn kind_of(value: &str) -> PhpCompletionKind {
    if string_literal(value).is_some()
        || value
            .trim_start_matches('-')
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    {
        PhpCompletionKind::Value
    } else {
        PhpCompletionKind::Constant
    }
}

/// Values of `#[ExpectedValues]` on `attrs` (from `decl`'s file), class names qualified.
fn attribute_values(
    index: &mut ProjectIndex,
    file: &FileIndex,
    decl: &Decl,
    attrs: &[crate::model::Attribute],
) -> (Vec<String>, bool) {
    let Some(ev) = expected_values(attrs) else {
        return (Vec::new(), false);
    };
    let decl_file = declaring_file(index, file, decl);
    let offset = match decl {
        Decl::Method { member, .. } => member.range.start,
        Decl::Function(f) => f.range.start,
    };
    let owner = decl.owner().map(|o| o.fqn.clone());
    let mut out: Vec<String> = ev
        .values
        .iter()
        .map(|v| qualify_value(v, &decl_file, offset, owner.as_deref()))
        .collect();
    if let Some(from) = &ev.from_class {
        let class = from.trim().trim_end_matches("::class");
        let fqn = match class.to_ascii_lowercase().as_str() {
            "self" | "static" => owner.clone().unwrap_or_default(),
            _ => decl_file.resolve_class(class, offset),
        };
        if let Some(c) = find_class(index, &decl_file, &fqn) {
            out.extend(class_constants(&c));
        }
    }
    (out, ev.flags)
}

/// Expected values of argument `n` of a call: metadata, `#[ExpectedValues]` on the
/// parameter, and map keys (as `'key'` / `\Class::class`).
pub fn expected_argument_values(
    index: &mut ProjectIndex,
    file: &FileIndex,
    callee: &Callee,
    n: usize,
) -> (Vec<String>, bool) {
    let (mut values, mut flags) = index.meta().expected_arguments(|t| callee.matches(t), n);
    if let Some(decl) = callee_decl(index, file, callee) {
        if let Some(p) = decl.param(n).cloned() {
            let (v, f) = attribute_values(index, file, &decl, &p.attributes);
            flags |= f;
            for x in v {
                if !values.contains(&x) {
                    values.push(x);
                }
            }
        }
    }
    let keys: Vec<String> = index
        .meta()
        .map_keys(|t| callee.matches(t), n)
        .into_iter()
        .map(|e| e.key.clone())
        .collect();
    for key in keys {
        let key = if key.contains('\\')
            || (key.chars().next().is_some_and(char::is_uppercase)
                && find_class(index, file, &key).is_some())
        {
            format!("\\{key}::class")
        } else {
            format!("'{}'", key.replace('\'', "\\'"))
        };
        if !values.contains(&key) {
            values.push(key);
        }
    }
    (values, flags)
}

/// Expected return values of a callee: metadata and `#[ExpectedValues]` on the function.
pub fn expected_return_values(
    index: &mut ProjectIndex,
    file: &FileIndex,
    callee: &Callee,
) -> (Vec<String>, bool) {
    let (mut values, mut flags) = index.meta().expected_return_values(|t| callee.matches(t));
    if let Some(decl) = callee_decl(index, file, callee) {
        let attrs = decl.attributes().to_vec();
        let (v, f) = attribute_values(index, file, &decl, &attrs);
        flags |= f;
        for x in v {
            if !values.contains(&x) {
                values.push(x);
            }
        }
    }
    (values, flags)
}

/// Items for `values`; inside a string only string values, inserted without quotes.
fn value_items(values: &[String], in_string: bool, detail: &str) -> Vec<PhpCompletionItem> {
    values
        .iter()
        .filter_map(|v| {
            if in_string {
                let inner = string_literal(v)?;
                Some(PhpCompletionItem {
                    label: inner.clone(),
                    insert: inner,
                    kind: PhpCompletionKind::Value,
                    detail: Some(detail.to_string()),
                })
            } else {
                Some(PhpCompletionItem {
                    label: short_label(v),
                    insert: v.clone(),
                    kind: if v.ends_with("::class") {
                        PhpCompletionKind::Class
                    } else {
                        kind_of(v)
                    },
                    detail: Some(detail.to_string()),
                })
            }
        })
        .collect()
}

/// Completion at `offset` (bytes) of `src`, the content of `rel`.
pub fn complete(
    index: &mut ProjectIndex,
    rel: &str,
    src: &str,
    offset: usize,
) -> Option<PhpCompletion> {
    let offset = offset.min(src.len());
    if !src.is_char_boundary(offset) {
        return None;
    }
    let file = parser::index_source(rel, src);
    let (_, string) = scan_to(src, offset);

    // 1. `$data['|` / `$data[|`: the keys of an `#[ArrayShape]`.
    let (from, bracket) = match string {
        Some(s) => (
            s.quote + 1,
            src[..s.quote].trim_end().strip_suffix('[').map(str::len),
        ),
        None => {
            let p = prefix_start(src, offset);
            (p, src[..p].trim_end().strip_suffix('[').map(str::len))
        }
    };
    if let Some(open) = bracket {
        if let Some(shape) = shape_of(index, &file, src, open, 0) {
            let in_string = string.is_some();
            let items = shape
                .keys
                .iter()
                .map(|k| PhpCompletionItem {
                    label: k.key.clone(),
                    insert: if in_string {
                        k.key.clone()
                    } else {
                        format!("'{}'", k.key)
                    },
                    kind: PhpCompletionKind::Key,
                    detail: Some(shape.key_type(k)),
                })
                .collect::<Vec<_>>();
            let to = match string {
                Some(s) => string_rest_end(src, offset, s.quote_char),
                None => offset,
            };
            return (!items.is_empty()).then_some(PhpCompletion { from, to, items });
        }
    }

    // 2. A comparison with a call: expected return values.
    let value_start = string.map_or_else(|| prefix_start(src, offset), |s| s.quote);
    let before = src[..value_start].trim_end();
    for op in ["!==", "===", "!=", "=="] {
        let Some(left) = before.strip_suffix(op) else {
            continue;
        };
        let left = left.trim_end();
        if !left.ends_with(')') {
            break;
        }
        let Some(open) = matching_open(src, left.len() - 1) else {
            break;
        };
        let Some((callee, _)) = callee_before(index, &file, src, open) else {
            break;
        };
        let (values, _) = expected_return_values(index, &file, &callee);
        let items = value_items(&values, string.is_some(), "expected return value");
        if items.is_empty() {
            return None;
        }
        let to = match string {
            Some(s) => string_rest_end(src, offset, s.quote_char),
            None => offset,
        };
        let from = string.map_or(value_start, |s| s.quote + 1);
        return Some(PhpCompletion { from, to, items });
    }

    // 3. An argument: expected arguments, `#[ExpectedValues]`, map keys.
    let site = call_at(index, &file, src, offset)?;
    let (values, flags) = expected_argument_values(index, &file, &site.callee, site.argument);
    let detail = if flags {
        "expected flag"
    } else {
        "expected value"
    };
    let items = value_items(&values, string.is_some(), detail);
    if items.is_empty() {
        return None;
    }
    let (from, to) = match string {
        Some(s) => (s.quote + 1, string_rest_end(src, offset, s.quote_char)),
        None => (prefix_start(src, offset), offset),
    };
    Some(PhpCompletion { from, to, items })
}
