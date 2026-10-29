//! JetBrains PhpStorm attributes (the `jetbrains/phpstorm-attributes` package and the PHP
//! stubs): `#[Deprecated]`, `#[ExpectedValues]`, `#[ArrayShape]`, `#[ObjectShape]`,
//! `#[Immutable]`, `#[Pure]`, `#[NoReturn]` and `#[Language]`, read from the attributes the
//! indexer stores ([`Attribute`]). PHP 8.4's own `#[\Deprecated]` counts as well.
//!
//! Values are source text; class names in them (`Order::class`, `Order::PAID`) are resolved
//! by the caller in the declaring file.

use serde::{Deserialize, Serialize};

use crate::docblock::AnnotationArg;
use crate::model::{Attribute, FileIndex};

/// Namespace of the JetBrains attributes.
pub const JETBRAINS_NS: &str = "JetBrains\\PhpStorm";

/// Whether `attr` is the JetBrains attribute `short` (`Deprecated`, `Pure`, ...). Also
/// accepts the attribute imported under another namespace's alias only by FQN.
pub fn is(attr: &Attribute, short: &str) -> bool {
    let fqn = attr.fqn.trim_start_matches('\\');
    fqn.eq_ignore_ascii_case(&format!("{JETBRAINS_NS}\\{short}"))
        || fqn.eq_ignore_ascii_case(&format!("{JETBRAINS_NS}\\Internal\\{short}"))
}

fn find<'a>(attrs: &'a [Attribute], short: &str) -> Option<&'a Attribute> {
    attrs.iter().find(|a| is(a, short))
}

/// Whether one of `attrs` is the JetBrains attribute `short`.
pub fn has(attrs: &[Attribute], short: &str) -> bool {
    find(attrs, short).is_some()
}

/// Argument `name`, else the positional argument at `pos`.
fn arg<'a>(args: &'a [AnnotationArg], name: &str, pos: usize) -> Option<&'a AnnotationArg> {
    args.iter()
        .find(|a| {
            a.name
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .or_else(|| {
            // Positional arguments come before named ones.
            args.iter().take_while(|a| a.name.is_none()).nth(pos)
        })
}

/// A string literal's content (`'a'`, `"b"`); `None` for other expressions.
pub fn string_literal(s: &str) -> Option<String> {
    let s = s.trim();
    for q in ['\'', '"'] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            let inner = &s[1..s.len() - 1];
            return Some(inner.replace(&format!("\\{q}"), &q.to_string()));
        }
    }
    None
}

/// Split `s` at top-level `sep` characters (outside strings, brackets and parentheses).
pub fn split_top(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth -= 1,
            _ if c == sep && depth == 0 => {
                out.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

/// The elements of an array literal (`['a', 'b']`, `array(1, 2)`); `None` for other text.
pub fn array_elements(s: &str) -> Option<Vec<String>> {
    let s = s.trim();
    let inner = if let Some(rest) = s.strip_prefix('[') {
        rest.strip_suffix(']')?
    } else {
        let rest = s.get(..5).filter(|p| p.eq_ignore_ascii_case("array"))?;
        s[rest.len()..]
            .trim()
            .strip_prefix('(')?
            .strip_suffix(')')?
    };
    Some(
        split_top(inner, ',')
            .into_iter()
            .map(str::trim)
            .filter(|e| !e.is_empty())
            .map(String::from)
            .collect(),
    )
}

/// `key => value` of an array element; the key `None` for a list element.
pub fn key_value(element: &str) -> (Option<&str>, &str) {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let bytes = element.as_bytes();
    for (i, c) in element.char_indices() {
        if let Some(q) = quote {
            if c == q && (i == 0 || bytes[i - 1] != b'\\') {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            '=' if depth == 0 && bytes.get(i + 1) == Some(&b'>') => {
                return (Some(element[..i].trim()), element[i + 2..].trim());
            }
            _ => {}
        }
    }
    (None, element.trim())
}

/// `#[Deprecated(reason: …, replacement: …, since: …)]`, or `@deprecated`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeprecationInfo {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
    /// Template of the replacing call: `%parametersList%`, `%parameter0%`, `%class%`, `%name%`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
}

/// The deprecation of a declaration from its attributes (JetBrains' or PHP 8.4's
/// `#[\Deprecated]`).
pub fn deprecated(attrs: &[Attribute]) -> Option<DeprecationInfo> {
    let text = |a: Option<&AnnotationArg>| {
        a.and_then(|a| string_literal(&a.value))
            .filter(|s| !s.trim().is_empty())
    };
    if let Some(a) = find(attrs, "Deprecated") {
        return Some(DeprecationInfo {
            reason: text(arg(&a.args, "reason", 0)).unwrap_or_default(),
            replacement: text(arg(&a.args, "replacement", 1)),
            since: text(arg(&a.args, "since", 2)),
        });
    }
    let native = attrs.iter().find(|a| {
        a.fqn
            .trim_start_matches('\\')
            .eq_ignore_ascii_case("Deprecated")
    })?;
    Some(DeprecationInfo {
        reason: text(arg(&native.args, "message", 0)).unwrap_or_default(),
        replacement: None,
        since: text(arg(&native.args, "since", 1)),
    })
}

/// The replacing call of `#[Deprecated(replacement: …)]` for a call with `args`;
/// `class` is the receiver (`$order`, `Order`) of a method call.
pub fn render_replacement(
    template: &str,
    class: Option<&str>,
    name: &str,
    args: &[&str],
) -> String {
    let mut out = template.replace("%parametersList%", &args.join(", "));
    // Highest index first, so `%parameter1%` is not touched by `%parameter10%`.
    for i in (0..args.len().max(10)).rev() {
        let value = args.get(i).copied().unwrap_or("");
        out = out.replace(&format!("%parameter{i}%"), value);
    }
    out = out.replace("%class%", class.unwrap_or(""));
    out.replace("%name%", name)
}

/// `#[ExpectedValues]` of a parameter, function or property.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedValuesAttr {
    /// Values as written (`'asc'`, `Order::PAID`, `42`).
    pub values: Vec<String>,
    /// `flags:` — values may be combined with `|`.
    pub flags: bool,
    /// `valuesFromClass:` / `flagsFromClass:` as written (`Order::class`): every constant of
    /// that class.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_class: Option<String>,
}

pub fn expected_values(attrs: &[Attribute]) -> Option<ExpectedValuesAttr> {
    let a = find(attrs, "ExpectedValues")?;
    let mut out = ExpectedValuesAttr::default();
    for (i, x) in a.args.iter().enumerate() {
        let name = x.name.as_deref().unwrap_or(match i {
            0 => "values",
            1 => "flags",
            2 => "valuesFromClass",
            _ => "flagsFromClass",
        });
        match name {
            "values" | "flags" => {
                if let Some(els) = array_elements(&x.value) {
                    out.flags |= name == "flags" && !els.is_empty();
                    out.values.extend(els);
                }
            }
            "valuesFromClass" | "flagsFromClass" => {
                let v = x.value.trim();
                if !v.is_empty() && !v.eq_ignore_ascii_case("null") {
                    out.from_class = Some(v.to_string());
                    out.flags |= name == "flagsFromClass";
                }
            }
            _ => {}
        }
    }
    Some(out)
}

/// One key of `#[ArrayShape]` / `#[ObjectShape]`: `'id' => 'int'`, `'user' => User::class`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapeKey {
    pub key: String,
    /// The type as written: a type string's content (`int`, `string[]`, `User`) or
    /// `User::class`.
    pub type_name: String,
}

impl ShapeKey {
    /// The type with `X::class` turned into `X`.
    pub fn type_text(&self) -> &str {
        self.type_name
            .strip_suffix("::class")
            .unwrap_or(&self.type_name)
    }
}

fn shape(a: &Attribute) -> Vec<ShapeKey> {
    let Some(first) = a.args.first() else {
        return Vec::new();
    };
    let Some(els) = array_elements(&first.value) else {
        return Vec::new();
    };
    els.iter()
        .filter_map(|e| {
            let (k, v) = key_value(e);
            let key = match k {
                Some(k) => string_literal(k).unwrap_or_else(|| k.to_string()),
                None => return None,
            };
            let type_name = string_literal(v).unwrap_or_else(|| v.to_string());
            Some(ShapeKey { key, type_name })
        })
        .collect()
}

/// `#[ArrayShape(['id' => 'int', ...])]`.
pub fn array_shape(attrs: &[Attribute]) -> Option<Vec<ShapeKey>> {
    find(attrs, "ArrayShape").map(shape)
}

/// `#[ObjectShape(['id' => 'int', ...])]`.
pub fn object_shape(attrs: &[Attribute]) -> Option<Vec<ShapeKey>> {
    find(attrs, "ObjectShape").map(shape)
}

/// Who may write an `#[Immutable]` property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WriteScope {
    /// The constructor only (the default, `Immutable::CONSTRUCTOR_WRITE_SCOPE`).
    Constructor,
    /// Methods of the class (`Immutable::PRIVATE_WRITE_SCOPE`).
    Private,
    /// Methods of the class and its subclasses (`Immutable::PROTECTED_WRITE_SCOPE`).
    Protected,
}

pub fn immutable(attrs: &[Attribute]) -> Option<WriteScope> {
    let a = find(attrs, "Immutable")?;
    let scope = a
        .args
        .first()
        .map(|x| x.value.to_ascii_lowercase())
        .unwrap_or_default();
    Some(if scope.contains("private") {
        WriteScope::Private
    } else if scope.contains("protected") {
        WriteScope::Protected
    } else {
        WriteScope::Constructor
    })
}

/// `#[Language('SQL')]`: the injected language of a string parameter or property.
pub fn language(attrs: &[Attribute]) -> Option<String> {
    let a = find(attrs, "Language")?;
    a.args
        .first()
        .and_then(|x| string_literal(&x.value))
        .filter(|s| !s.trim().is_empty())
}

/// The constants of `class` as `\Fqn::NAME` (for `valuesFromClass`).
pub fn class_constants(class: &crate::model::ClassLike) -> Vec<String> {
    class
        .members
        .iter()
        .filter(|m| {
            matches!(
                m.kind,
                crate::model::SymbolKind::Constant | crate::model::SymbolKind::EnumCase
            )
        })
        .map(|m| format!("\\{}::{}", class.fqn, m.name))
        .collect()
}

/// Expected values with class names fully qualified for `file` at `offset`: `Order::PAID` ->
/// `\App\Models\Order::PAID` (`self` / `static` -> `owner`).
pub fn qualify_value(value: &str, file: &FileIndex, offset: usize, owner: Option<&str>) -> String {
    let v = value.trim();
    if let Some((class, rest)) = v.split_once("::") {
        if string_literal(v).is_none() && !class.starts_with('$') {
            let fqn = match class.to_ascii_lowercase().as_str() {
                "self" | "static" => match owner {
                    Some(o) => o.to_string(),
                    None => return v.to_string(),
                },
                _ => file.resolve_class(class, offset),
            };
            return format!("\\{}::{rest}", fqn.trim_start_matches('\\'));
        }
    }
    v.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::index_source;

    const SRC: &str = r#"<?php
namespace App;

use JetBrains\PhpStorm\{ArrayShape, Deprecated, ExpectedValues, Immutable, Language, NoReturn, ObjectShape, Pure};

#[Immutable(Immutable::PRIVATE_WRITE_SCOPE)]
class Order {
    public const PAID = 'paid';
    public const OPEN = 'open';

    #[Deprecated(reason: 'Use pay()', replacement: '%class%->pay(%parameter0%)', since: '2.0')]
    public function settle(int $amount, bool $now = true) {}

    #[Pure]
    public function total(): int { return 1; }

    #[NoReturn]
    public function fail(): void { exit; }

    #[ArrayShape(['id' => 'int', 'customer' => Customer::class, "tags" => 'string[]'])]
    public function toArray(): array { return []; }

    #[ObjectShape(['name' => 'string'])]
    public function info(): object {}

    public function sort(
        #[ExpectedValues(['asc', 'desc'])] string $dir,
        #[ExpectedValues(flags: [self::PAID, Order::OPEN])] int $mask,
        #[ExpectedValues(valuesFromClass: Order::class)] string $status,
        #[Language('SQL')] string $where,
    ) {}
}

#[\Deprecated(message: 'gone', since: '8.4')]
function old() {}
"#;

    #[test]
    fn reads_jetbrains_attributes() {
        let file = index_source("app/Order.php", SRC);
        let order = &file.classes[0];
        assert_eq!(immutable(&order.attributes), Some(WriteScope::Private));
        let settle = order.method("settle").unwrap();
        let d = deprecated(&settle.attributes).unwrap();
        assert_eq!(d.reason, "Use pay()");
        assert_eq!(d.replacement.as_deref(), Some("%class%->pay(%parameter0%)"));
        assert_eq!(d.since.as_deref(), Some("2.0"));
        assert!(has(&order.method("total").unwrap().attributes, "Pure"));
        assert!(has(&order.method("fail").unwrap().attributes, "NoReturn"));
        assert!(!has(&order.method("fail").unwrap().attributes, "Pure"));
        let shape = array_shape(&order.method("toArray").unwrap().attributes).unwrap();
        let keys: Vec<(&str, &str)> = shape
            .iter()
            .map(|k| (k.key.as_str(), k.type_text()))
            .collect();
        assert_eq!(
            keys,
            vec![
                ("id", "int"),
                ("customer", "Customer"),
                ("tags", "string[]")
            ]
        );
        let obj = object_shape(&order.method("info").unwrap().attributes).unwrap();
        assert_eq!(obj[0].key, "name");

        let old = deprecated(&file.functions[0].attributes).unwrap();
        assert_eq!(old.reason, "gone");
        assert_eq!(old.replacement, None);
    }

    #[test]
    fn reads_parameter_attributes() {
        let file = index_source("app/Order.php", SRC);
        let sort = file.classes[0].method("sort").unwrap();
        let dir = expected_values(&sort.params[0].attributes).unwrap();
        assert_eq!(dir.values, vec!["'asc'", "'desc'"]);
        assert!(!dir.flags);
        let mask = expected_values(&sort.params[1].attributes).unwrap();
        assert!(mask.flags);
        assert_eq!(
            mask.values
                .iter()
                .map(|v| qualify_value(v, &file, sort.range.start, Some("App\\Order")))
                .collect::<Vec<_>>(),
            vec!["\\App\\Order::PAID", "\\App\\Order::OPEN"]
        );
        let status = expected_values(&sort.params[2].attributes).unwrap();
        assert_eq!(status.from_class.as_deref(), Some("Order::class"));
        assert_eq!(
            class_constants(&file.classes[0]),
            vec!["\\App\\Order::PAID", "\\App\\Order::OPEN"]
        );
        assert_eq!(language(&sort.params[3].attributes).as_deref(), Some("SQL"));
        assert_eq!(language(&sort.params[0].attributes), None);
    }

    #[test]
    fn renders_replacements_and_splits_literals() {
        assert_eq!(
            render_replacement(
                "%class%->pay(%parameter0%)",
                Some("$order"),
                "settle",
                &["10", "true"]
            ),
            "$order->pay(10)"
        );
        assert_eq!(
            render_replacement("new_fn(%parametersList%)", None, "old", &["$a", "[1, 2]"]),
            "new_fn($a, [1, 2])"
        );
        assert_eq!(
            array_elements("['a, b' => \"x]\", 'c' => [1, 2]]").unwrap(),
            vec!["'a, b' => \"x]\"", "'c' => [1, 2]"]
        );
        assert_eq!(array_elements("array(1, 2)").unwrap(), vec!["1", "2"]);
        assert_eq!(array_elements("$x"), None);
        assert_eq!(key_value("'a=>b' => 'int'"), (Some("'a=>b'"), "'int'"));
        assert_eq!(string_literal("'it\\'s'").as_deref(), Some("it's"));
    }
}
